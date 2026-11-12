//! The HTTP adapter of the model downloader: a ranged GET over `ureq` with rustls.
//!
//! `crastu-models` owns the download logic (resume, size and sha256 checks, atomic rename) and
//! only knows the `Fetch` trait; this crate is the real implementation, [`HttpFetch`].
//!
//! The client is compiled for desktop targets only. Its TLS stack (`ring`) builds C and assembly,
//! which needs a C cross toolchain that the portable-crates check (`cargo xtask mobile-check`)
//! does not have; on iOS and Android this crate is empty and the mobile apps download through
//! the platform, handing the bytes to the same `Fetch` trait.

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod http;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use http::HttpFetch;
