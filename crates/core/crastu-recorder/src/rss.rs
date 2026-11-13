//! Resident memory of this process, for `perf.jsonl`.
//!
//! Linux and Android read `VmRSS` from `/proc/self/status` (in kB, so the page size does not
//! matter). macOS and Windows have no plain-file source: the only ways are system calls that
//! need `unsafe`, which this crate does not allow, so the value is left out of the sample there.

/// The `VmRSS` line of a `/proc/<pid>/status` text, in bytes.
pub fn parse_vm_rss(status: &str) -> Option<u64> {
    let rest = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))?
        .trim();
    let kb: u64 = rest.strip_suffix("kB")?.trim().parse().ok()?;
    kb.checked_mul(1024)
}

/// Resident memory in bytes, or `None` where this system gives no way to read it without
/// `unsafe` (macOS, Windows) or the read failed.
pub fn resident_bytes() -> Option<u64> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        parse_vm_rss(&std::fs::read_to_string("/proc/self/status").ok()?)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        None
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
}
