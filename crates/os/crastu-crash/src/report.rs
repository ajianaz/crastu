//! The text of `crash.txt`, built without allocating.
//!
//! This code runs inside a signal handler, so it must be async-signal-safe: no allocation, no
//! locks, no `std::fmt` (it may allocate and takes no care about reentrancy), no `std::io`, and
//! nothing that can panic. Everything here is plain arithmetic on a fixed buffer, with `get_mut`
//! instead of indexing and saturating or checked arithmetic, so it is safe code that can be unit
//! tested like any other.

/// A fixed-capacity byte buffer. Writes past the end are dropped, never a panic.
#[derive(Debug)]
pub(crate) struct Buf<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Buf<N> {
    pub(crate) const fn new() -> Self {
        Buf {
            bytes: [0; N],
            len: 0,
        }
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or(&[])
    }

    pub(crate) fn push(&mut self, data: &[u8]) {
        for &b in data {
            let Some(slot) = self.bytes.get_mut(self.len) else {
                return;
            };
            *slot = b;
            self.len += 1;
        }
    }

    /// Appends `n` in decimal.
    pub(crate) fn push_dec(&mut self, n: u64) {
        // u64::MAX has 20 digits.
        let mut digits = [0u8; 20];
        let mut at = digits.len();
        let mut v = n;
        loop {
            at = at.saturating_sub(1);
            if let Some(d) = digits.get_mut(at) {
                *d = b'0' + (v % 10) as u8;
            }
            v /= 10;
            if v == 0 || at == 0 {
                break;
            }
        }
        self.push(digits.get(at..).unwrap_or(&[]));
    }

    /// Appends `n` in decimal with at least `width` digits (zero padded).
    pub(crate) fn push_dec_padded(&mut self, n: u64, width: usize) {
        let mut digits = 1;
        let mut v = n / 10;
        while v > 0 {
            digits += 1;
            v /= 10;
        }
        for _ in digits..width {
            self.push(b"0");
        }
        self.push_dec(n);
    }

    /// Appends a signed number in decimal.
    pub(crate) fn push_signed(&mut self, n: i64) {
        if n < 0 {
            self.push(b"-");
        }
        self.push_dec(n.unsigned_abs());
    }

    /// Appends `n` as `0x` and lower-case hex digits, without leading zeros.
    pub(crate) fn push_hex(&mut self, n: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.push(b"0x");
        let mut started = false;
        for shift in (0..16).rev() {
            let nibble = ((n >> (shift * 4)) & 0xf) as usize;
            if nibble != 0 || started || shift == 0 {
                started = true;
                self.push(HEX.get(nibble..nibble + 1).unwrap_or(b"?"));
            }
        }
    }
}

/// The latest second written as a date (9999-12-31T23:59:59Z); later clocks are clamped to it.
const MAX_SECS: i64 = 253_402_300_799;

/// Appends the Unix time `secs` as an RFC 3339 UTC timestamp (`2026-11-12T10:15:03Z`). Times
/// before 1970 or after 9999 are clamped. Civil-from-days by Howard Hinnant, integers only.
pub(crate) fn push_utc<const N: usize>(buf: &mut Buf<N>, secs: i64) {
    let secs = secs.clamp(0, MAX_SECS);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let parts = [
        (year, 4, b"-"),
        (month, 2, b"-"),
        (day, 2, b"T"),
        (rem / 3_600, 2, b":"),
        (rem % 3_600 / 60, 2, b":"),
        (rem % 60, 2, b"Z"),
    ];
    for (value, width, sep) in parts {
        buf.push_dec_padded(value.unsigned_abs(), width);
        buf.push(sep);
    }
}

/// What the handler knows about one fatal signal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fault<'a> {
    pub(crate) name: &'a [u8],
    pub(crate) number: i32,
    /// `si_code`: why the kernel sent it (`SEGV_MAPERR`, `BUS_ADRALN`...), or who did.
    pub(crate) code: Option<i32>,
    /// The faulting address, only when the kernel raised the signal for a memory or instruction
    /// fault (not for `kill` or `raise`).
    pub(crate) address: Option<u64>,
    pub(crate) pid: u32,
    /// Seconds since the Unix epoch, or `None` when the clock could not be read.
    pub(crate) unix_secs: Option<i64>,
}

/// The report buffer: fixed fields only, so a few hundred bytes is plenty.
pub(crate) type Report = Buf<1024>;

/// Writes the report for `fault` in the session `session`. Fixed fields only: it never holds a
/// message, an argument, an environment variable or memory contents, so it cannot carry user
/// content or a key.
pub(crate) fn write_report(out: &mut Report, fault: &Fault<'_>, session: &[u8]) {
    out.push(b"Crastu crash report\n");
    out.push(b"signal: ");
    out.push(fault.name);
    out.push(b" (");
    out.push_signed(i64::from(fault.number));
    out.push(b")\ncode: ");
    match fault.code {
        Some(c) => out.push_signed(i64::from(c)),
        None => out.push(b"none"),
    }
    out.push(b"\naddress: ");
    match fault.address {
        Some(a) => out.push_hex(a),
        None => out.push(b"none"),
    }
    out.push(b"\npid: ");
    out.push_dec(u64::from(fault.pid));
    out.push(b"\nsession: ");
    out.push(session);
    out.push(b"\ntime: ");
    match fault.unix_secs {
        Some(s) => {
            push_utc(out, s);
            out.push(b" (unix ");
            out.push_signed(s);
            out.push(b")");
        }
        None => out.push(b"unknown"),
    }
    out.push(b"\n\nThe process then died by this signal (default action), so the system's own crash report has the stack.\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text<const N: usize>(b: &Buf<N>) -> &str {
        std::str::from_utf8(b.as_bytes()).unwrap()
    }

    #[test]
    fn decimal_numbers() {
        for (n, want) in [
            (0u64, "0"),
            (7, "7"),
            (10, "10"),
            (4242, "4242"),
            (u64::MAX, "18446744073709551615"),
        ] {
            let mut b = Buf::<32>::new();
            b.push_dec(n);
            assert_eq!(text(&b), want);
        }
        let mut b = Buf::<32>::new();
        b.push_signed(-11);
        b.push(b" ");
        b.push_signed(i64::MIN);
        assert_eq!(text(&b), "-11 -9223372036854775808");
    }

    #[test]
    fn padded_numbers() {
        let mut b = Buf::<32>::new();
        b.push_dec_padded(5, 2);
        b.push_dec_padded(123, 2);
        b.push_dec_padded(0, 4);
        assert_eq!(text(&b), "051230000");
    }

    #[test]
    fn hex_numbers() {
        for (n, want) in [
            (0u64, "0x0"),
            (0x10, "0x10"),
            (0xdead_beef, "0xdeadbeef"),
            (0x1000_0001, "0x10000001"),
            (u64::MAX, "0xffffffffffffffff"),
        ] {
            let mut b = Buf::<32>::new();
            b.push_hex(n);
            assert_eq!(text(&b), want);
        }
    }

    #[test]
    fn a_full_buffer_drops_the_rest_without_panicking() {
        let mut b = Buf::<4>::new();
        b.push(b"abcdef");
        b.push_dec(123);
        b.push_hex(u64::MAX);
        assert_eq!(text(&b), "abcd");
        let mut z = Buf::<0>::new();
        z.push_dec(9);
        assert_eq!(text(&z), "");
    }

    #[test]
    fn utc_timestamps() {
        for (secs, want) in [
            (0, "1970-01-01T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (951_868_799, "2000-02-29T23:59:59Z"),
            (1_762_909_323, "2025-11-12T01:02:03Z"),
            (4_102_444_800, "2100-01-01T00:00:00Z"),
            (MAX_SECS, "9999-12-31T23:59:59Z"),
            (-5, "1970-01-01T00:00:00Z"),
            (i64::MAX, "9999-12-31T23:59:59Z"),
        ] {
            let mut b = Buf::<32>::new();
            push_utc(&mut b, secs);
            assert_eq!(text(&b), want, "{secs}");
        }
    }

    /// Every day from 1970 to 2370 follows the one before (month lengths, leap years, the
    /// 100 and 400 year rules), checked against a plain calendar walk.
    #[test]
    fn every_day_follows_the_one_before() {
        fn month_len(y: u32, m: u32) -> u32 {
            let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
            match m {
                2 if leap => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            }
        }
        let (mut y, mut m, mut d) = (1970u32, 1u32, 1u32);
        for day in 0..146_097i64 {
            let mut b = Buf::<32>::new();
            push_utc(&mut b, day * 86_400 + 86_399);
            let want = format!("{y:04}-{m:02}-{d:02}T23:59:59Z");
            assert_eq!(text(&b), want, "day {day}");
            d += 1;
            if d > month_len(y, m) {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
        }
    }

    #[test]
    fn the_report_holds_the_fixed_fields() {
        let mut r = Report::new();
        let fault = Fault {
            name: b"SIGSEGV",
            number: 11,
            code: Some(1),
            address: Some(0x10),
            pid: 4242,
            unix_secs: Some(1_762_909_323),
        };
        write_report(&mut r, &fault, b"20251112T010203Z-4242");
        let t = text(&r);
        assert!(t.starts_with("Crastu crash report\n"), "{t}");
        assert!(t.contains("\nsignal: SIGSEGV (11)\n"), "{t}");
        assert!(t.contains("\ncode: 1\n"), "{t}");
        assert!(t.contains("\naddress: 0x10\n"), "{t}");
        assert!(t.contains("\npid: 4242\n"), "{t}");
        assert!(t.contains("\nsession: 20251112T010203Z-4242\n"), "{t}");
        assert!(
            t.contains("\ntime: 2025-11-12T01:02:03Z (unix 1762909323)\n"),
            "{t}"
        );
        assert!(t.ends_with("crash report has the stack.\n"), "{t}");

        let mut r = Report::new();
        let fault = Fault {
            code: None,
            address: None,
            unix_secs: None,
            ..fault
        };
        write_report(&mut r, &fault, b"s");
        let t = text(&r);
        assert!(t.contains("\ncode: none\n"), "{t}");
        assert!(t.contains("\naddress: none\n"), "{t}");
        assert!(t.contains("\ntime: unknown\n"), "{t}");
    }
}
