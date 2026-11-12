//! PEM private keys and `mysql -p` style flags.

use crate::names::quoted_len;
use crate::{MASK, is_blank, run};

const BEGIN: &[u8] = b"-----BEGIN ";
const END: &[u8] = b"-----END ";
const DASHES: &[u8] = b"-----";

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A PEM private key block at the beginning of `s` (`BEGIN ... PRIVATE KEY` through the matching
/// `END`, on many lines or with a literal `\n` between them, or to the end of the input when the
/// `END` is missing): how many bytes it covers. Public keys and certificates are not matched.
pub(crate) fn pem_at(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if !b.starts_with(BEGIN) {
        return None;
    }
    // The label, up to the closing dashes, is short; look no further than that.
    let window = b.get(BEGIN.len()..(BEGIN.len() + 80).min(b.len()))?;
    let label_len = find(window, DASHES)?;
    find(window.get(..label_len)?, b"PRIVATE KEY")?;
    let after = BEGIN.len() + label_len + DASHES.len();
    let Some(end) = find(b.get(after..)?, END) else {
        return Some(b.len()); // cut off: everything after the header goes
    };
    let tail = after + end + END.len();
    let close = b
        .get(tail..(tail + 80).min(b.len()))
        .and_then(|w| find(w, DASHES));
    Some(close.map_or(tail, |c| tail + c + DASHES.len()))
}

/// Commands whose `-p` takes a password.
const PASSWORD_COMMANDS: [&str; 8] = [
    "mysql",
    "mysqldump",
    "mysqladmin",
    "psql",
    "mongo",
    "mongosh",
    "redis-cli",
    "sshpass",
];

/// Remembers whether the current line has a command whose `-p` is a password, so that
/// `mysql -u root -phunter2` is masked but `-pfoo` elsewhere is not.
#[derive(Default)]
pub(crate) struct ShellCtx {
    command_on_line: bool,
}

impl ShellCtx {
    /// Called at every position the scan stops at.
    pub(crate) fn note(&mut self, b: &[u8], i: usize) {
        match b.get(i) {
            Some(b'\n') => self.command_on_line = false,
            Some(_) if !self.command_on_line => {
                let word_start = i
                    .checked_sub(1)
                    .and_then(|p| b.get(p))
                    .is_none_or(|p| matches!(p, b' ' | b'\t' | b'/' | b'\n' | b';' | b'|' | b'&'));
                if word_start {
                    self.command_on_line = PASSWORD_COMMANDS.iter().any(|c| {
                        b.get(i..)
                            .is_some_and(|rest| rest.starts_with(c.as_bytes()))
                            && b.get(i + c.len())
                                .is_none_or(|n| matches!(n, b' ' | b'\t' | b'\n' | b';'))
                    });
                }
            }
            _ => {}
        }
    }

    /// `-p value` and `-pvalue` at the beginning of `s` (the caller checked that a blank or the
    /// start of the text comes before). With a password command on the line the value is
    /// masked; otherwise only `-p value` is, and not when the value looks like a port
    /// (`-p 8080`, `-p 8080:80`), which is what `docker -p` takes.
    pub(crate) fn short_flag(&self, s: &str) -> Option<(usize, String)> {
        let b = s.as_bytes();
        if !b.starts_with(b"-p") {
            return None;
        }
        let blanks = run(b, 2, is_blank);
        let j = 2 + blanks;
        let first = *b.get(j)?;
        if first == b'-' || first == b'\n' || first == b'\r' {
            return None;
        }
        let spaced = blanks > 0;
        if !spaced && !self.command_on_line {
            return None;
        }
        let (start, n) = if matches!(first, b'"' | b'\'') {
            (j + 1, quoted_len(b, j + 1, first))
        } else {
            (j, run(b, j, |c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r')))
        };
        let value = b.get(start..start + n)?;
        let port_like = value.iter().all(|c| c.is_ascii_digit() || *c == b':');
        if n == 0 || (!self.command_on_line && port_like) {
            return None;
        }
        Some((start + n, format!("{}{MASK}", s.get(..start)?)))
    }
}
