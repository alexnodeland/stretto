//! UTC dates from Unix milliseconds, for the activity chart's days and job
//! ids, without a calendar crate.

/// One day, in milliseconds.
pub const DAY_MS: u64 = 86_400_000;

/// The UTC date of day `days` since 1970-01-01, as (year, month, day)
/// (Howard Hinnant's `civil_from_days`).
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `YYYY-MM-DD` of the UTC day holding `ms`.
pub fn day(ms: u64) -> String {
    let (y, m, d) = civil((ms / DAY_MS) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYYMMDDTHHMMSS.mmmZ`, as stretto's session ids start.
pub fn stamp(ms: u64) -> String {
    let (y, m, d) = civil((ms / DAY_MS) as i64);
    let in_day = ms % DAY_MS;
    let (h, min, s, milli) = (
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1000 % 60,
        in_day % 1000,
    );
    format!("{y:04}{m:02}{d:02}T{h:02}{min:02}{s:02}.{milli:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc() {
        assert_eq!(day(0), "1970-01-01");
        // The fixtures' sessions: 2026-09-28 02:04:01.312 UTC.
        assert_eq!(day(1_790_561_041_312), "2026-09-28");
        assert_eq!(stamp(1_790_561_041_312), "20260928T020401.312Z");
        assert_eq!(day(951_782_400_000), "2000-02-29");
        assert_eq!(day(951_868_800_000), "2000-03-01");
        assert_eq!(civil(-1), (1969, 12, 31));
    }
}
