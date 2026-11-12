//! PEM private keys and `mysql -p` style flags.

use crate::values::quoted_len;
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
const PASSWORD_COMMANDS: [&str; 15] = [
    "mysql",
    "mysqldump",
    "mysqladmin",
    "mariadb",
    "mariadb-dump",
    "mariadb-admin",
    "psql",
    "pg_dump",
    "pg_restore",
    "mongo",
    "mongosh",
    "mongodump",
    "mongorestore",
    "redis-cli",
    "sshpass",
];

/// Flags whose value is `user:password` when it holds a colon (`curl -u admin:pw`).
const USER_FLAGS: [&str; 4] = ["--proxy-user", "--user", "-u", "-U"];

/// Whether a flag can start after `p`: a blank, a quote, a bracket, a backtick, `$`, a line
/// break, or a command separator.
pub(crate) fn word_start_prev(p: u8) -> bool {
    matches!(
        p,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'\''
            | b'"'
            | b'('
            | b'`'
            | b'{'
            | b'$'
            | b';'
            | b'|'
            | b'&'
    )
}

/// What the scan remembers about the current line: whether it has a command whose `-p` is a
/// password (`mysql -u root -phunter2` is masked, `-pfoo` elsewhere is not), and which quote is
/// open (so a header inside `-H '...'` ends at the closing quote).
#[derive(Default)]
pub(crate) struct ShellCtx {
    command_on_line: bool,
    quote: Option<u8>,
    scanned: usize,
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
                    .is_none_or(|p| *p == b'/' || word_start_prev(*p));
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

    /// The quote that is open on the current line just before `i`, if any. The text is read
    /// once, in order, whatever was consumed in between.
    pub(crate) fn quote_before(&mut self, b: &[u8], i: usize) -> Option<u8> {
        let mut escaped = false;
        let mut prev = self.scanned.checked_sub(1).and_then(|p| b.get(p)).copied();
        let seen = b.get(self.scanned..i.min(b.len())).unwrap_or_default();
        for &c in seen {
            if c == b'\n' {
                self.quote = None;
                escaped = false;
            } else if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if self.quote == Some(c) {
                self.quote = None;
            } else if self.quote.is_none()
                && matches!(c, b'"' | b'\'')
                && prev.is_none_or(|p| !p.is_ascii_alphanumeric())
            {
                // Opens a shell argument or a string, not an apostrophe inside a word (`it's`).
                self.quote = Some(c);
            }
            prev = Some(c);
        }
        self.scanned = self.scanned.max(i);
        self.quote
    }

    /// A password flag at the beginning of `s` (the caller checked that a word could start
    /// here): `-p value` and `-pvalue`, and `-u user:pass`, `--user user:pass`,
    /// `--proxy-user user:pass`. With a password command on the line `-p`'s value is masked;
    /// otherwise only `-p value` is, and not when the value looks like a port (`-p 8080`,
    /// `-p 8080:80`), which is what `docker -p` takes. The user flags need a colon in the value.
    pub(crate) fn short_flag(&self, s: &str) -> Option<(usize, String)> {
        let b = s.as_bytes();
        if b.starts_with(b"-p") {
            return self.password_flag(s);
        }
        let flag = USER_FLAGS.iter().find(|f| b.starts_with(f.as_bytes()))?;
        let mut j = flag.len();
        if flag.starts_with("--") && !matches!(b.get(j), Some(b' ' | b'\t' | b'=')) {
            return None; // `--username` is another flag
        }
        if b.get(j) == Some(&b'=') {
            j += 1;
        } else {
            j += run(b, j, is_blank);
        }
        let (start, n) = flag_value(b, j)?;
        let value = b.get(start..start + n)?;
        value.contains(&b':').then(|| {
            (
                start + n,
                format!("{}{MASK}", s.get(..start).unwrap_or_default()),
            )
        })
    }

    fn password_flag(&self, s: &str) -> Option<(usize, String)> {
        let b = s.as_bytes();
        let blanks = run(b, 2, is_blank);
        let j = 2 + blanks;
        let spaced = blanks > 0;
        if spaced && let Some(n) = pem_at(s.get(j..)?) {
            return Some((j + n, format!("{}[redacted private key]", s.get(..j)?)));
        }
        let first = *b.get(j)?;
        if first == b'-' || (!spaced && !self.command_on_line) {
            return None;
        }
        let (start, n) = flag_value(b, j)?;
        let value = b.get(start..start + n)?;
        let port_like = value.iter().all(|c| c.is_ascii_digit() || *c == b':');
        if !self.command_on_line && port_like {
            return None;
        }
        Some((start + n, format!("{}{MASK}", s.get(..start)?)))
    }
}

/// The value of a flag that starts at `j`: where it starts (after an opening quote) and its
/// length. `None` for no value, or a line break.
fn flag_value(b: &[u8], j: usize) -> Option<(usize, usize)> {
    let first = *b.get(j)?;
    if matches!(first, b'\n' | b'\r') {
        return None;
    }
    let (start, n) = if matches!(first, b'"' | b'\'') {
        (j + 1, quoted_len(b, j + 1, first))
    } else {
        (j, run(b, j, |c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r')))
    };
    (n > 0).then_some((start, n))
}
