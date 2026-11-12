//! The one gate every outbound network request passes through.
//!
//! With Offline on, [`NetGate::check`] refuses the request and says what wanted to go out and
//! where; the refusal is also kept (bounded) so that a later slice can show it in the activity
//! log. Model downloads, update checks, provider calls and sync all call `check` before they open
//! a socket. No I/O happens here.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How many refusals the gate remembers; the oldest is dropped first.
pub const MAX_REFUSALS: usize = 100;

/// A request that Offline mode refused. The message names the request and the host, never a
/// query string, path or credentials.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("Offline: {what} from {host} refused")]
pub struct Offline {
    /// What wanted to go out, for example `model download whisper-small`.
    pub what: String,
    /// The host it wanted to reach, for example `huggingface.co`.
    pub host: String,
}

/// One refusal kept by the gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Counts refusals from 1 in the order they happened; the gate has no wall clock.
    pub seq: u64,
    pub what: String,
    pub host: String,
}

#[derive(Debug)]
struct Inner {
    offline: AtomicBool,
    seq: AtomicU64,
    refusals: Mutex<VecDeque<Refusal>>,
}

/// Cheap to clone; every clone shares the same switch and the same refusal list.
#[derive(Clone, Debug)]
pub struct NetGate(Arc<Inner>);

impl NetGate {
    pub fn new(offline: bool) -> NetGate {
        NetGate(Arc::new(Inner {
            offline: AtomicBool::new(offline),
            seq: AtomicU64::new(0),
            refusals: Mutex::new(VecDeque::new()),
        }))
    }

    pub fn is_offline(&self) -> bool {
        self.0.offline.load(Ordering::SeqCst)
    }

    pub fn set_offline(&self, offline: bool) {
        self.0.offline.store(offline, Ordering::SeqCst);
    }

    /// Call before opening a request. `what` says what is going out (`model download x`), `url`
    /// where; only its host is kept.
    pub fn check(&self, what: &str, url: &str) -> Result<(), Offline> {
        if !self.is_offline() {
            return Ok(());
        }
        let refusal = Offline {
            what: what.to_string(),
            host: host_of(url),
        };
        let seq = self.0.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let mut list = self.lock();
        if list.len() >= MAX_REFUSALS {
            list.pop_front();
        }
        list.push_back(Refusal {
            seq,
            what: refusal.what.clone(),
            host: refusal.host.clone(),
        });
        Err(refusal)
    }

    /// The refusals kept so far, oldest first.
    pub fn refusals(&self) -> Vec<Refusal> {
        self.lock().iter().cloned().collect()
    }

    /// Takes the refusals kept so far and forgets them.
    pub fn drain_refusals(&self) -> Vec<Refusal> {
        self.lock().drain(..).collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<Refusal>> {
        self.0.refusals.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Default for NetGate {
    /// Online.
    fn default() -> NetGate {
        NetGate::new(false)
    }
}

const UNKNOWN_HOST: &str = "an unknown host";

/// The host of a URL, for messages: no scheme, credentials, port, path or query. A missing
/// scheme is fine; something that does not look like a host gives `an unknown host`.
pub fn host_of(url: &str) -> String {
    let url = url.trim();
    let rest = match url.find("://") {
        Some(i) if !url[..i].contains(['/', '?', '#', '@']) => &url[i + 3..],
        _ => url,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        match v6.find(']') {
            Some(end) => format!("[{}]", &v6[..end]),
            None => return UNKNOWN_HOST.to_string(),
        }
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    let plausible = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']'));
    if plausible {
        host.to_ascii_lowercase()
    } else {
        UNKNOWN_HOST.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_come_out_clean() {
        for (url, host) in [
            ("https://huggingface.co/a/b.bin", "huggingface.co"),
            ("https://user:pw@host.example:8443/p?q=1", "host.example"),
            ("https://u:p@w@host.example/x", "host.example"),
            ("https://[::1]/x", "[::1]"),
            ("https://[2001:db8::1]:8080/x", "[2001:db8::1]"),
            ("HTTPS://Example.COM", "example.com"),
            ("example.com/path?x=http://evil.test/", "example.com"),
            ("example.com:9000", "example.com"),
            ("host.example?token=abc", "host.example"),
            ("host.example#frag", "host.example"),
            ("  https://padded.example/  ", "padded.example"),
        ] {
            assert_eq!(host_of(url), host, "{url}");
        }
        for url in [
            "",
            "https://",
            "https:///path",
            "https://[::1/x",
            "https://bad host/x",
        ] {
            assert_eq!(host_of(url), UNKNOWN_HOST, "{url:?}");
        }
    }

    #[test]
    fn the_message_has_no_credentials_or_query() {
        let gate = NetGate::new(true);
        let e = gate
            .check(
                "model download whisper-small",
                "https://user:secret@huggingface.co/x?token=abc",
            )
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            "Offline: model download whisper-small from huggingface.co refused"
        );
        assert!(!e.to_string().contains("secret") && !e.to_string().contains("token"));
    }

    #[test]
    fn online_lets_everything_through_and_records_nothing() {
        let gate = NetGate::default();
        gate.check("x", "https://a.example/").unwrap();
        assert!(gate.refusals().is_empty());
    }

    #[test]
    fn clones_share_the_switch_and_the_list() {
        let gate = NetGate::new(false);
        let other = gate.clone();
        other.set_offline(true);
        assert!(gate.is_offline());
        gate.check("update check", "https://a.example/")
            .unwrap_err();
        assert_eq!(other.refusals().len(), 1);
        gate.set_offline(false);
        other.check("update check", "https://a.example/").unwrap();
    }

    #[test]
    fn refusals_are_numbered_bounded_and_drainable() {
        let gate = NetGate::new(true);
        for i in 0..MAX_REFUSALS + 5 {
            gate.check(&format!("req {i}"), "https://a.example/")
                .unwrap_err();
        }
        let kept = gate.refusals();
        assert_eq!(kept.len(), MAX_REFUSALS);
        assert_eq!(kept[0].what, "req 5");
        assert_eq!(kept[0].seq, 6);
        assert_eq!(kept.last().unwrap().seq, (MAX_REFUSALS + 5) as u64);
        let drained = gate.drain_refusals();
        assert_eq!(drained, kept);
        assert!(gate.refusals().is_empty());
        gate.check("again", "https://b.example/").unwrap_err();
        let next = gate.drain_refusals();
        assert_eq!(next[0].seq, (MAX_REFUSALS + 6) as u64);
        assert_eq!(next[0].host, "b.example");
    }
}
