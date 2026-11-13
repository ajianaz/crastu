//! Resident memory of this process, for `perf.jsonl`.
//!
//! Linux and Android read `VmRSS` from `/proc/self/status` (in kB, so the page size does not
//! matter). macOS and Windows have no plain-file source: the only ways are system calls that
//! need `unsafe`, which this crate does not allow, so an app registers a reader (the
//! `crastu-procmem` crate's) with [`set_resident_reader`]; without one the value is left out.

/// The `VmRSS` line of a `/proc/<pid>/status` text, in bytes.
pub fn parse_vm_rss(status: &str) -> Option<u64> {
    let rest = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))?
        .trim();
    let kb: u64 = rest.strip_suffix("kB")?.trim().parse().ok()?;
    kb.checked_mul(1024)
}

/// The reader an app registers for systems without a plain-file source.
static READER: std::sync::OnceLock<fn() -> Option<u64>> = std::sync::OnceLock::new();

/// Registers the function that reads resident memory where this crate cannot (macOS and Windows:
/// the `crastu-procmem` crate has the system calls, and this crate may not depend on it or use
/// `unsafe`). The first call wins; it returns whether this one was used. It has no effect on Linux
/// and Android, which read `/proc` directly.
pub fn set_resident_reader(reader: fn() -> Option<u64>) -> bool {
    READER.set(reader).is_ok()
}

/// Resident memory in bytes, or `None` where this system has no plain-file source and no reader
/// was registered with [`set_resident_reader`], or the read failed.
pub fn resident_bytes() -> Option<u64> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        parse_vm_rss(&std::fs::read_to_string("/proc/self/status").ok()?)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        READER.get().and_then(|read| read())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_rss_is_read_in_bytes() {
        let status = "Name:\tcrastu\nVmPeak:\t  9999 kB\nVmRSS:\t   12345 kB\nThreads:\t4\n";
        assert_eq!(parse_vm_rss(status), Some(12345 * 1024));
    }

    #[test]
    fn a_missing_or_odd_line_gives_none() {
        assert_eq!(parse_vm_rss(""), None);
        assert_eq!(parse_vm_rss("VmRSS:\tlots\n"), None);
        assert_eq!(parse_vm_rss("VmRSS:\t5 MB\n"), None);
        assert_eq!(parse_vm_rss(&format!("VmRSS: {} kB", u64::MAX)), None);
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn this_process_has_some_memory() {
        assert!(resident_bytes().unwrap() > 0);
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    #[test]
    fn a_registered_reader_is_used_where_there_is_no_plain_file() {
        // The only test that registers one: the first registration is the process's.
        assert_eq!(resident_bytes(), None);
        assert!(set_resident_reader(|| Some(4242)));
        assert!(!set_resident_reader(|| Some(1)));
        assert_eq!(resident_bytes(), Some(4242));
    }
}
