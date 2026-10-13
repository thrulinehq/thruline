//! The one timestamp shape a licence carries: `YYYY-MM-DDTHH:MM:SSZ`, UTC.
//!
//! Strict on purpose. A licence is signed, so the issuer controls the shape;
//! anything else is not a licence, and guessing at a time zone would be a
//! second answer to "has this expired?".

/// Seconds since the Unix epoch, or `None` for anything not exactly the shape.
pub(crate) fn unix(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let s = text.get(from..to)?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + s)
}

/// "23 Sep 2027", for a sentence.
pub(crate) fn day(text: &str) -> Option<String> {
    unix(text)?;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let y = text.get(0..4)?;
    let m: usize = text.get(5..7)?.parse().ok()?;
    let d: u32 = text.get(8..10)?.parse().ok()?;
    Some(format!("{d} {} {y}", MONTHS.get(m.checked_sub(1)?)?))
}

/// A day number as `YYYY-MM-DD`.
pub(crate) fn iso_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Seconds since the epoch as `YYYY-MM-DDTHH:MM:SSZ`.
pub(crate) fn timestamp(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        iso_date(days),
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

/// Howard Hinnant's civil-from-days, the inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Howard Hinnant's days-from-civil.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(unix("2026-09-23T12:34:56Z"), Some(1_790_166_896));
        assert_eq!(day("2027-09-23T00:00:00Z").as_deref(), Some("23 Sep 2027"));
    }

    #[test]
    fn dates_are_written_as_calendar_days() {
        // The two day-number readers were removed under C20; what remains
        // writes days, and timestamps are written through it.
        for (days, text) in [
            (-1, "1969-12-31"),
            (0, "1970-01-01"),
            (59, "1970-03-01"),
            (11_016, "2000-02-29"),
            (20_720, "2026-09-24"),
            (20_734, "2026-10-08"),
        ] {
            assert_eq!(iso_date(days), text, "{days}");
        }
        assert_eq!(timestamp(1_790_166_896), "2026-09-23T12:34:56Z");
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            "",
            "2026-09-23",
            "2026-09-23T12:34:56",
            "2026-09-23T12:34:56+00:00",
            "2026-13-01T00:00:00Z",
            "2026-02-30T00:00:00Z",
            "2026-09-23T24:00:00Z",
            "2026-09-23t12:34:56Z",
            "20x6-09-23T12:34:56Z",
        ] {
            assert_eq!(unix(bad), None, "{bad}");
        }
    }
}
