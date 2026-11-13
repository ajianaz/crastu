//! Resident memory of this process where reading it needs a system call: macOS (`task_info`) and
//! Windows (`GetProcessMemoryInfo`). Linux and Android read a plain file, which
//! `crastu-recorder` does itself.
//!
//! `unsafe` is allowed in this crate only, in the one [`sys`] module, for those two calls: each
//! call fills a plain C struct that this module owns and sizes, and nothing else is touched. The
//! crate is `os` kind (it may only use `core` crates), so `crastu-recorder` cannot depend on it;
//! the apps hand [`resident_bytes`] to the recorder with `crastu_recorder::set_resident_reader`.
//! Everywhere else the answer is `None`.

/// Resident memory of this process in bytes, or `None` where this crate has no way to read it
/// (other systems) or the call failed.
pub fn resident_bytes() -> Option<u64> {
    sys::resident_bytes()
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "one system call that reports this process's memory"
)]
mod sys {
    use std::mem;

    pub(crate) fn resident_bytes() -> Option<u64> {
        // SAFETY: `mach_task_basic_info` is a plain C struct; all zeroes is a valid value, and
        // `task_info` overwrites it.
        let mut info: libc::mach_task_basic_info = unsafe { mem::zeroed() };
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
        // `libc` marks `mach_task_self` deprecated in favour of the `mach2` crate; one more
        // dependency for one call that has not changed in decades is not worth it.
        #[allow(
            deprecated,
            reason = "libc points to mach2; one call does not justify the crate"
        )]
        // SAFETY: `info` is a valid out buffer of `count` 32-bit words (the count says so);
        // `mach_task_self` returns the port of the calling task. `task_info` writes at most `count`
        // words and updates `count`.
        let status = unsafe {
            libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                (&raw mut info).cast::<libc::integer_t>(),
                &mut count,
            )
        };
        // `resident_size` is the pages in RAM now (what "resident" means on Linux too). It is not
        // `phys_footprint`, the figure Activity Monitor and the jetsam limit use, which also counts
        // compressed and swapped pages: for a trend in `perf.jsonl` the plain resident size is the
        // comparable number across systems.
        (status == libc::KERN_SUCCESS).then_some(info.resident_size)
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "one system call that reports this process's memory"
)]
mod sys {
    use std::ffi::c_void;
    use std::mem;

    /// `PROCESS_MEMORY_COUNTERS` from `psapi.h`.
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    // The struct must be exactly what psapi.h has: two 32-bit fields and eight pointer-sized ones.
    const _: () =
        assert!(mem::size_of::<ProcessMemoryCounters>() == 2 * 4 + 8 * mem::size_of::<usize>());

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        /// `GetProcessMemoryInfo` as kernel32 exports it (Windows 7 and later).
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    pub(crate) fn resident_bytes() -> Option<u64> {
        let cb = u32::try_from(mem::size_of::<ProcessMemoryCounters>()).ok()?;
        let mut counters = ProcessMemoryCounters {
            cb,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: `GetCurrentProcess` returns a pseudo handle that needs no closing, and
        // `counters` is a valid out struct whose size is passed as `cb`.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, cb) };
        (ok != 0).then_some(counters.working_set_size as u64)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod sys {
    pub(crate) fn resident_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn this_process_has_some_resident_memory() {
        let before = resident_bytes().expect("the system call works");
        assert!(before > 1024 * 1024, "{before}");
        // Zeroed memory that is not touched is address space, not resident memory: the figure
        // must not jump by the size of the block (this tells resident size from virtual size).
        let mut block = vec![0u8; 192 * 1024 * 1024];
        let mapped = resident_bytes().unwrap();
        assert!(mapped < before + 96 * 1024 * 1024, "{before} -> {mapped}");
        // Touched, it shows up: 128 MiB written page by page. The bound is below the 128 MiB, so
        // a runner that compresses or swaps part of it still passes.
        for page in block.chunks_mut(4096).take(32 * 1024) {
            page[0] = 1;
        }
        let touched = resident_bytes().unwrap();
        assert!(
            touched >= mapped + 96 * 1024 * 1024,
            "{mapped} -> {touched}"
        );
        assert!(block.iter().step_by(4096).take(32 * 1024).all(|b| *b == 1));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn other_systems_say_nothing() {
        assert_eq!(resident_bytes(), None);
    }
}
