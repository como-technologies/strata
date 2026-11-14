//! The times that the stand-in gives the documents that a chapter makes one after the other.

use chrono::{DateTime, SecondsFormat, TimeDelta};

/// The time `n` minutes after `first`, in the form of a time of Palette, such as
/// `2026-09-15T08:01:00.000Z`. `first` is a time in RFC 3339.
///
/// So each document of a chapter gets a valid time, also after the sixtieth, and the times are in
/// the order of `n`.
pub(super) fn minutes_after(first: &str, n: usize) -> String {
    let first = DateTime::parse_from_rfc3339(first).expect("a time of the stand-in is RFC 3339");
    let minutes = i64::try_from(n).expect("a count of documents fits in an i64");
    (first + TimeDelta::minutes(minutes))
        .to_utc()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The minute after 59 is the next hour, and the day after 23:59 is the next day (#814).
    #[test]
    fn each_minute_after_the_first_is_a_valid_time_also_after_an_hour_and_a_day() {
        let first = "2026-09-15T08:00:00.000Z";
        assert_eq!(minutes_after(first, 0), first);
        assert_eq!(minutes_after(first, 1), "2026-09-15T08:01:00.000Z");
        assert_eq!(minutes_after(first, 59), "2026-09-15T08:59:00.000Z");
        assert_eq!(minutes_after(first, 60), "2026-09-15T09:00:00.000Z");
        assert_eq!(minutes_after(first, 61), "2026-09-15T09:01:00.000Z");
        assert_eq!(minutes_after(first, 16 * 60), "2026-09-16T00:00:00.000Z");
        // the seconds and the milliseconds of the first time stay
        assert_eq!(
            minutes_after("2026-09-18T14:02:12.107Z", 58),
            "2026-09-18T15:00:12.107Z"
        );
    }
}
