//! UTC time text without a date library: the folder stamp and RFC 3339 for the files.

use std::time::{SystemTime, UNIX_EPOCH};

/// A broken-down UTC time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Utc {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

impl Utc {
    /// Splits milliseconds since the Unix epoch (negative values clamp to the epoch).
    pub(crate) fn from_unix_millis(ms: i64) -> Utc {
        let ms = ms.max(0);
        let days = ms / 86_400_000;
        let rem = ms % 86_400_000;
        // Howard Hinnant's civil-from-days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        Utc {
            year,
            month,
            day,
            hour: (rem / 3_600_000) as u32,
            minute: (rem / 60_000 % 60) as u32,
            second: (rem / 1000 % 60) as u32,
            millis: (rem % 1000) as u32,
        }
    }

    pub(crate) fn now() -> Utc {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Utc::from_unix_millis(ms)
    }

    /// `20261112T103045Z`: sorts in time order as text.
    pub(crate) fn stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `2026-11-12T10:30:45.123Z`.
    pub(crate) fn rfc3339(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.millis
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        let u = Utc::from_unix_millis(0);
        assert_eq!(u.rfc3339(), "1970-01-01T00:00:00.000Z");
        // 2024-02-29T00:00:00Z, a leap day.
        let u = Utc::from_unix_millis(1_709_164_800_000);
        assert_eq!(u.rfc3339(), "2024-02-29T00:00:00.000Z");
        // 2000-03-01T12:34:56.789Z
        let u = Utc::from_unix_millis(951_914_096_789);
        assert_eq!(u.rfc3339(), "2000-03-01T12:34:56.789Z");
        assert_eq!(u.stamp(), "20000301T123456Z");
    }

    #[test]
    fn stamps_sort_as_text() {
        let a = Utc::from_unix_millis(1_709_164_800_000).stamp();
        let b = Utc::from_unix_millis(1_709_164_801_000).stamp();
        assert!(a < b);
    }
}
