//! Local socket protocol between the host, the GUI and the CLI shim.
//!
//! One JSON object per line in each direction: a [`Request`] carries an id, a method and params;
//! the [`Response`] with the same id carries a result or an error. Connections stay open, so a
//! client can send many requests. Unix domain sockets on macOS, Linux, iOS and Android; Windows
//! named pipes come with the Windows port.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Method names understood by the host.
pub mod method {
    /// No params. Returns `{"version": "..."}`.
    pub const PING: &str = "host.ping";
    /// No params. Returns `[ToolInfo]`.
    pub const TOOLS_LIST: &str = "tools.list";
    /// Params: [`super::CallParams`]. Returns the tool's JSON result.
    pub const TOOLS_CALL: &str = "tools.call";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A tool as listed to agents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// `"read"`, `"change"` or `"delete"`.
    pub risk: String,
    pub input_schema: Value,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct CallParams {
    /// The name the client declares (for example `"claude-code"`). A label for the activity log
    /// only: the host trusts it for nothing. Identity is the `token`.
    pub agent: String,
    /// The agent's pairing token (`CRASTU_AGENT_TOKEN` in the shim). With a valid one the host
    /// resolves the agent and ignores `agent`; an invalid one is an error. Without one the caller
    /// is unpaired and gets the default policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub name: String,
    #[serde(default)]
    pub args: Value,
}

// Written by hand so that the token never reaches a log through `{:?}`.
impl std::fmt::Debug for CallParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallParams")
            .field("agent", &self.agent)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("name", &self.name)
            .field("args", &self.args)
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("bad message: {0}")]
    Protocol(String),
    #[error("{0}")]
    Remote(String),
}

/// Where the host listens. `CRASTU_SOCKET` overrides the platform default.
///
/// The default always sits in a per-user directory (`~/Library/Application Support/Crastu` on
/// macOS, `$XDG_RUNTIME_DIR` or `~/.local/share/crastu` elsewhere). There is no fallback to a
/// shared temp directory: without `HOME` (and without `XDG_RUNTIME_DIR` off macOS) this fails and
/// asks for `CRASTU_SOCKET` instead.
pub fn default_socket_path() -> Result<PathBuf, IpcError> {
    if let Some(p) = std::env::var_os("CRASTU_SOCKET") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Crastu/crastu.sock"))
    } else if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
        Some(PathBuf::from(rt).join("crastu.sock"))
    } else {
        home.map(|h| h.join(".local/share/crastu/crastu.sock"))
    };
    path.ok_or_else(|| {
        IpcError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME is not set, so there is no per-user place for the host socket; set CRASTU_SOCKET",
        ))
    })
}

#[cfg(unix)]
mod unix {
    use std::fs::{DirBuilder, File, OpenOptions};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};

    use rustix::fs::Mode;
    use rustix::fs::{FlockOperation, flock};
    use rustix::io::Errno;
    use rustix::process::{getuid, umask};
    use std::path::Path;
    use std::sync::Arc;

    use serde_json::Value;

    use super::{IpcError, Request, Response};

    /// Handles one request: `(method, params) -> result or error message`.
    pub type Handler = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

    /// A blocking client. One request at a time per client.
    #[derive(Debug)]
    pub struct Client {
        reader: BufReader<UnixStream>,
        writer: UnixStream,
        next_id: u64,
    }

    impl Client {
        pub fn connect(path: &Path) -> Result<Client, IpcError> {
            let stream = UnixStream::connect(path)?;
            Ok(Client {
                reader: BufReader::new(stream.try_clone()?),
                writer: stream,
                next_id: 1,
            })
        }

        pub fn call(&mut self, method: &str, params: Value) -> Result<Value, IpcError> {
            let id = self.next_id;
            self.next_id += 1;
            let mut line = serde_json::to_string(&Request {
                id,
                method: method.into(),
                params,
            })
            .map_err(|e| IpcError::Protocol(e.to_string()))?;
            line.push('\n');
            self.writer.write_all(line.as_bytes())?;
            let mut reply = String::new();
            if self.reader.read_line(&mut reply)? == 0 {
                return Err(IpcError::Protocol("host closed the connection".into()));
            }
            let resp: Response =
                serde_json::from_str(&reply).map_err(|e| IpcError::Protocol(e.to_string()))?;
            if resp.id != id {
                return Err(IpcError::Protocol(format!(
                    "expected response {id}, got {}",
                    resp.id
                )));
            }
            match (resp.result, resp.error) {
                (_, Some(e)) => Err(IpcError::Remote(e)),
                (Some(v), None) => Ok(v),
                (None, None) => Ok(Value::Null),
            }
        }
    }

    /// A bound host socket. Holds an exclusive lock on `<socket>.lock` for as long as it lives,
    /// so only one host can own the socket path at a time.
    #[derive(Debug)]
    pub struct Listener {
        inner: UnixListener,
        _lock: File,
    }

    /// Binds `path`, replacing a stale socket file left by a crashed host.
    /// Fails if another host is already running there.
    ///
    /// The liveness check, the removal of a stale socket and the bind all happen while holding an
    /// exclusive `flock` on `<socket>.lock`. Two hosts starting at the same moment cannot both see
    /// a dead socket and unlink each other's: the loser fails here. The kernel drops the lock when
    /// the host exits, crash included, so a stale lock file never blocks a new host.
    ///
    /// Only the user who runs the host may connect. The parent directory must belong to that user
    /// (see [`ensure_owned_dir`]), and the socket is created with mode 0600 from the start (umask
    /// 077 around the bind), so there is no moment in which another user can connect. Existing
    /// directories keep their permissions.
    pub fn bind(path: &Path) -> Result<Listener, IpcError> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            ensure_owned_dir(dir)?;
        }
        let in_use = || {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!("a Crastu host is already running at {}", path.display()),
            ))
        };
        let mut lock_path = path.as_os_str().to_owned();
        lock_path.push(".lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&lock_path)?;
        match flock(&lock, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(e) if e == Errno::WOULDBLOCK => return Err(in_use()),
            Err(e) => return Err(IpcError::Io(e.into())),
        }
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(in_use());
            }
            std::fs::remove_file(path)?;
        }
        let previous = umask(Mode::from_raw_mode(0o077));
        let bound = UnixListener::bind(path);
        umask(previous);
        let inner = bound?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Listener { inner, _lock: lock })
    }

    /// Makes sure `dir` exists and belongs to the current user. A missing directory is created
    /// with mode 0700; an existing one keeps its permissions (it may be `$HOME` or a project
    /// folder). Fails for a symlink or a directory owned by someone else, such as `/tmp`, where
    /// another user could replace the socket. Create first, then check: an existence check
    /// followed by a create would leave a gap in which someone else could make the directory.
    pub fn ensure_owned_dir(dir: &Path) -> Result<(), IpcError> {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let meta = std::fs::symlink_metadata(dir)?;
        if !meta.is_dir() {
            return Err(IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} is not a plain directory (symlinks are refused)",
                    dir.display()
                ),
            )));
        }
        if meta.uid() != getuid().as_raw() {
            return Err(IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} belongs to another user; choose a directory of your own (CRASTU_SOCKET, CRASTU_DATA)",
                    dir.display()
                ),
            )));
        }
        Ok(())
    }

    /// Makes every file this process creates from now on private to the user (umask 077). The
    /// headless host calls it at start, so the database and its journal files are 0600 even in
    /// a directory that others can list.
    pub fn private_file_mode() {
        umask(Mode::from_raw_mode(0o077));
    }

    /// Accepts connections until the listener fails; each connection gets its own thread.
    pub fn serve(listener: Listener, handler: Handler) {
        for stream in listener.inner.incoming() {
            let Ok(stream) = stream else { continue };
            let handler = handler.clone();
            std::thread::spawn(move || {
                let _ = serve_connection(stream, handler);
            });
        }
    }

    fn serve_connection(stream: UnixStream, handler: Handler) -> std::io::Result<()> {
        let mut writer = stream.try_clone()?;
        for line in BufReader::new(stream).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let resp = match serde_json::from_str::<Request>(&line) {
                Ok(req) => match handler(&req.method, req.params) {
                    Ok(v) => Response {
                        id: req.id,
                        result: Some(v),
                        error: None,
                    },
                    Err(e) => Response {
                        id: req.id,
                        result: None,
                        error: Some(e),
                    },
                },
                Err(e) => Response {
                    id: 0,
                    result: None,
                    error: Some(format!("bad request: {e}")),
                },
            };
            let mut out = serde_json::to_string(&resp).unwrap_or_default();
            out.push('\n');
            writer.write_all(out.as_bytes())?;
        }
        Ok(())
    }
}

#[cfg(unix)]
pub use unix::{Client, Handler, Listener, bind, ensure_owned_dir, private_file_mode, serve};

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn start(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("t.sock");
        let listener = bind(&path).unwrap();
        let handler: Handler = Arc::new(|m, p| match m {
            method::PING => Ok(json!({"version": "test"})),
            "echo" => Ok(p),
            _ => Err(format!("unknown method {m}")),
        });
        std::thread::spawn(move || serve(listener, handler));
        path
    }

    #[test]
    fn request_response_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = start(dir.path());
        let mut c = Client::connect(&path).unwrap();
        assert_eq!(
            c.call(method::PING, Value::Null).unwrap(),
            json!({"version": "test"})
        );
        assert_eq!(
            c.call("echo", json!({"a": [1, 2]})).unwrap(),
            json!({"a": [1, 2]})
        );
        assert!(
            matches!(c.call("nope", Value::Null), Err(IpcError::Remote(e)) if e.contains("unknown method"))
        );
    }

    #[test]
    fn refuses_a_second_host_and_replaces_stale_sockets() {
        let dir = tempfile::tempdir().unwrap();
        let path = start(dir.path());
        assert!(bind(&path).is_err(), "a live host keeps its socket");
        let stale = dir.path().join("stale.sock");
        drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
        assert!(bind(&stale).is_ok(), "a dead socket file is replaced");
    }

    #[test]
    fn only_the_owner_can_reach_the_socket() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private").join("p.sock");
        let _listener = bind(&path).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(&path), 0o600);
        let mut lock = path.as_os_str().to_owned();
        lock.push(".lock");
        assert_eq!(mode(std::path::Path::new(&lock)), 0o600);
    }

    #[test]
    fn an_existing_directory_keeps_its_mode_and_the_socket_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("project");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sock = shared.join("crastu.sock");
        let _listener = bind(&sock).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&shared), 0o755, "an existing directory is not changed");
        assert_eq!(mode(&sock), 0o600);
    }

    #[test]
    fn a_symlinked_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = bind(&link.join("s.sock")).unwrap_err();
        assert!(err.to_string().contains("symlinks are refused"), "{err}");
    }

    #[test]
    fn a_directory_of_another_user_is_refused() {
        if getuid_is_root() {
            return; // root owns /, so the check cannot be shown as root
        }
        let err = bind(std::path::Path::new("/crastu-test-shared.sock")).unwrap_err();
        assert!(err.to_string().contains("belongs to another user"), "{err}");
    }

    fn getuid_is_root() -> bool {
        rustix::process::getuid().is_root()
    }

    #[test]
    fn concurrent_hosts_cannot_steal_each_others_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("race.sock");
        // A stale socket, as after a crash, so every contender takes the replace path.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    bind(&path).ok()
                })
            })
            .collect();
        let winners: Vec<Listener> = handles
            .into_iter()
            .filter_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(winners.len(), 1, "exactly one host owns the socket");
        let handler: Handler = Arc::new(|_, _| Ok(json!("pong")));
        let winner = winners.into_iter().next().unwrap();
        std::thread::spawn(move || serve(winner, handler));
        let mut c = Client::connect(&path).expect("the winner's socket is still on disk");
        assert_eq!(c.call(method::PING, Value::Null).unwrap(), json!("pong"));
    }
}
