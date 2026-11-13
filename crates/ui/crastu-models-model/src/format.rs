//! Sizes and percentages as the page prints them.

/// `639.4 MB`, `2.3 KB`, `1.2 GB` (decimal units, like the catalogue's own figures).
pub fn bytes(n: u64) -> String {
    const KB: f64 = 1_000.0;
    let f = n as f64;
    if f >= KB * KB * KB {
        format!("{:.1} GB", f / (KB * KB * KB))
    } else if f >= KB * KB {
        format!("{:.1} MB", f / (KB * KB))
    } else if f >= KB {
        format!("{:.1} KB", f / KB)
    } else {
        format!("{n} B")
    }
}

/// `42%` of `done` out of `total`, never above 100%.
pub fn percent(done: u64, total: u64) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    format!("{}%", (done.min(total) as u128 * 100 / total as u128))
}
