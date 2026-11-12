//! Known token formats, JWTs and URL userinfo.

use crate::{MASK, is_token_byte, run, run_cap};

/// Token prefixes and the fewest token characters that must follow for a match.
const PREFIXES: [(&[u8], usize); 26] = [
    (b"sk-", 10),
    (b"hf_", 16),
    (b"ghp_", 20),
    (b"gho_", 20),
    (b"ghu_", 20),
    (b"ghs_", 20),
    (b"ghr_", 20),
    (b"github_pat_", 20),
    (b"xoxb-", 10),
    (b"xoxa-", 10),
    (b"xoxp-", 10),
    (b"xoxr-", 10),
    (b"xoxs-", 10),
    (b"xapp-", 10),
    (b"AKIA", 16),
    (b"ASIA", 16),
    (b"sk_live_", 10),
    (b"sk_test_", 10),
    (b"rk_live_", 10),
    (b"rk_test_", 10),
    (b"whsec_", 20),
    (b"GOCSPX-", 20),
    (b"AIza", 30),
    (b"glpat-", 16),
    (b"ya29.", 20),
    (b"npm_", 30),
];

/// The part of a Slack incoming-webhook URL that carries the secret.
const SLACK_HOOK: &[u8] = b"hooks.slack.com/services/";

/// Schemes that precede a credential (`Bearer abc`, `Basic abc`).
pub(crate) const SCHEMES: [&str; 4] = ["bearer", "basic", "digest", "token"];

/// A secret at the beginning of `s`: how many bytes it covers and what replaces them.
pub(crate) fn secret_at(s: &str) -> Option<(usize, String)> {
    let b = s.as_bytes();
    for (prefix, min) in PREFIXES {
        if b.starts_with(prefix) && run_cap(b, prefix.len(), is_token_byte) >= min {
            let n = run(b, prefix.len(), is_token_byte);
            return Some((prefix.len() + n, MASK.into()));
        }
    }
    // Crastu agent tokens: `crastu_` plus hex.
    if b.starts_with(b"crastu_") && run_cap(b, 7, |c| c.is_ascii_hexdigit()) >= 16 {
        return Some((7 + run(b, 7, |c| c.is_ascii_hexdigit()), MASK.into()));
    }
    // SendGrid: `SG.` 22 characters `.` 43 characters.
    if b.starts_with(b"SG.")
        && run_cap(b, 3, is_token_byte) == 22
        && b.get(25) == Some(&b'.')
        && run_cap(b, 26, is_token_byte) >= 43
    {
        return Some((26 + run(b, 26, is_token_byte), MASK.into()));
    }
    // Slack webhook URLs: the path is the secret.
    if b.starts_with(SLACK_HOOK)
        && run_cap(b, SLACK_HOOK.len(), |c| is_token_byte(c) || c == b'/') >= 20
    {
        let n = run(b, SLACK_HOOK.len(), |c| is_token_byte(c) || c == b'/');
        return Some((SLACK_HOOK.len() + n, MASK.into()));
    }
    // Telegram bot tokens: 8 to 10 digits, a colon, 35 characters.
    let digits = run_cap(b, 0, |c| c.is_ascii_digit());
    if (8..=10).contains(&digits)
        && b.get(digits) == Some(&b':')
        && run_cap(b, digits + 1, is_token_byte) >= 35
    {
        return Some((digits + 1 + run(b, digits + 1, is_token_byte), MASK.into()));
    }
    // `Bearer <token>`, `Basic <base64>` and the like, any case.
    for scheme in SCHEMES {
        let k = scheme.len() + 1;
        if b.len() > k
            && b[..scheme.len()].eq_ignore_ascii_case(scheme.as_bytes())
            && b[k - 1] == b' '
        {
            let value = |c: u8| !c.is_ascii_whitespace() && c != b'"' && c != b'\'';
            if run_cap(b, k, value) >= 8 {
                return Some((k + run(b, k, value), MASK.into()));
            }
        }
    }
    None
}

/// The run of token and dot bytes that a JWT candidate sits in, scanned once. Every `eyJ` inside
/// the same run asks the same question (does a JWT starting here, ending at the run's end, have
/// two dots?), which the last two dots answer without scanning again.
#[derive(Default)]
pub(crate) struct JwtRun {
    end: usize,
    last_dot: Option<usize>,
    second_last_dot: Option<usize>,
}

impl JwtRun {
    /// The length of the JWT starting at `i` (where `b` has `eyJ`), if there is one: the whole
    /// run of token and dot bytes from `i`, at least 20 bytes with two dots in it.
    pub(crate) fn len_at(&mut self, b: &[u8], i: usize) -> Option<usize> {
        if i >= self.end {
            let part = |c: u8| is_token_byte(c) || c == b'.';
            self.end = i + run(b, i, part);
            self.last_dot = None;
            self.second_last_dot = None;
            for (k, c) in b.iter().enumerate().take(self.end).skip(i) {
                if *c == b'.' {
                    self.second_last_dot = self.last_dot;
                    self.last_dot = Some(k);
                }
            }
        }
        let n = self.end - i;
        (n >= 20 && self.second_last_dot.is_some_and(|d| d >= i)).then_some(n)
    }
}

/// The length of `user:pass@` at the start of the authority that follows `://`, if there is one.
pub(crate) fn userinfo_len(after_scheme: &str) -> Option<usize> {
    let end = after_scheme
        .find(|c: char| matches!(c, '/' | '?' | '#' | '"' | '\'') || c.is_whitespace())
        .unwrap_or(after_scheme.len());
    after_scheme.get(..end)?.rfind('@').map(|at| at + 1)
}
