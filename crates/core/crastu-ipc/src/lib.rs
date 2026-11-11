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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallParams {
    /// Who is asking, for the approval policy and the activity log (for example `"claude-code"`).
    pub agent: String,
    pub name: String,
    #[serde(default)]
    pub args: Value,
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
pub fn default_socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("CRASTU_SOCKET") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Crastu/crastu.sock")
    } else if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(rt).join("crastu.sock")
    } else {
        home.join(".local/share/crastu/crastu.sock")
    }
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
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

    /// Binds `path`, replacing a stale socket file left by a crashed host.
    /// Fails if another host is already listening there.
    pub fn bind(path: &Path) -> Result<UnixListener, IpcError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(IpcError::Io(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("a Crastu host is already running at {}", path.display()),
                )));
            }
            std::fs::remove_file(path)?;
        }
        Ok(UnixListener::bind(path)?)
    }

    /// Accepts connections until the listener fails; each connection gets its own thread.
    pub fn serve(listener: UnixListener, handler: Handler) {
        for stream in listener.incoming() {
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
pub use unix::{Client, Handler, bind, serve};

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
}
