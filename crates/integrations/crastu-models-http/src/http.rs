//! The HTTP adapter of the model downloader: a ranged GET over `ureq` with rustls.
//!
//! `crastu-models` owns the download logic (resume, size and sha256 checks, atomic rename) and
//! only knows the [`Fetch`] trait; this crate is the real implementation. It makes a request
//! only when [`Fetch::fetch`] is called, speaks https only, and sends nothing but the request
//! itself (no cookies, no identifiers).

use std::io;
use std::time::Duration;

use crastu_models::{Fetch, FetchResponse};

/// Ranged GET over https.
#[derive(Debug)]
pub struct HttpFetch {
    agent: ureq::Agent,
}

impl HttpFetch {
    /// An https-only client (redirects included).
    pub fn new() -> HttpFetch {
        HttpFetch::build(true)
    }

    fn build(https_only: bool) -> HttpFetch {
        let agent = ureq::Agent::config_builder()
            .https_only(https_only)
            .user_agent("crastu-models")
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_per_read(Some(Duration::from_secs(30)))
            .build()
            .new_agent();
        HttpFetch { agent }
    }

    /// Plain http, for the tests that talk to a server on localhost.
    #[cfg(test)]
    fn plain_http() -> HttpFetch {
        HttpFetch::build(false)
    }
}

impl Default for HttpFetch {
    fn default() -> HttpFetch {
        HttpFetch::new()
    }
}

fn other(message: String) -> io::Error {
    io::Error::other(message)
}

/// `bytes 100-199/200` into (start, total); the total is `*` when the server does not know it.
fn parse_content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (start, _end) = range.split_once('-')?;
    let total = match total.trim() {
        "*" => None,
        n => Some(n.parse().ok()?),
    };
    Some((start.trim().parse().ok()?, total))
}

impl Fetch for HttpFetch {
    fn fetch(&self, url: &str, offset: u64) -> io::Result<FetchResponse> {
        let mut request = self.agent.get(url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request
            .call()
            .map_err(|e| other(format!("GET {url}: {e}")))?;
        let (start, total) = match response.status().as_u16() {
            206 => {
                let value = response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| other("206 without a Content-Range header".into()))?;
                parse_content_range(value)
                    .ok_or_else(|| other(format!("unreadable Content-Range {value:?}")))?
            }
            // 200: the server ignored the range and sends the whole file.
            _ => (0, response.body().content_length()),
        };
        Ok(FetchResponse {
            reader: Box::new(response.into_body().into_reader()),
            start,
            total,
        })
    }
}

#[cfg(test)]
mod tests;
