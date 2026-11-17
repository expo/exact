//! Gregorian civil dates. Schedules keep wall-clock endpoints, not UTC durations.
//! The person's own calendars and activity are instants, placed on days in the
//! reader's zone at the offset in force at each one.

#[cfg(not(target_arch = "wasm32"))]
use chrono::{DateTime, NaiveDate, Offset, TimeDelta, TimeZone};
#[cfg(not(target_arch = "wasm32"))]
use chrono_tz::Tz;

pub(crate) const FIRST_MONTH: i32 = 1900 * 12;
pub(crate) const LAST_MONTH: i32 = 2100 * 12 + 11;
pub(crate) const DAY_MS: f64 = 86_400_000.0;
pub(crate) const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub(crate) fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

// Hinnant's integer Gregorian algorithms, also used by runner/src/format/mod.rs.
pub(crate) fn ordinal(year: i32, month: u32, day: u32) -> i32 {
    let y = year - i32::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = month as i32 + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day as i32 - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

pub(crate) fn civil(day: i32) -> (i32, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (yoe + era * 400 + i32::from(m <= 2), m as u32, d as u32)
}

pub(crate) fn month_of(day: i32) -> i32 {
    let (y, m, _) = civil(day);
    y * 12 + m as i32 - 1
}

pub(crate) fn month_first(month: i32) -> i32 {
    ordinal(month.div_euclid(12), month.rem_euclid(12) as u32 + 1, 1)
}

pub(crate) fn iso(day: i32) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub(crate) fn month_id(month: i32) -> String {
    format!(
        "{:04}-{:02}",
        month.div_euclid(12),
        month.rem_euclid(12) + 1
    )
}

pub(crate) fn month_label(month: i32) -> String {
    format!(
        "{} {}",
        MONTH_NAMES[month.rem_euclid(12) as usize],
        month.div_euclid(12)
    )
}

pub(crate) fn weekday(day: i32) -> usize {
    (day + 4).rem_euclid(7) as usize
}

pub(crate) fn day_label(day: i32, today: i32) -> String {
    let (y, m, d) = civil(day);
    let weekday = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday(day)];
    let label = format!("{weekday}, {} {d}", &MONTH_NAMES[m as usize - 1][..3]);
    if y == civil(today).0 {
        label
    } else {
        format!("{label}, {y}")
    }
}

pub(crate) fn short_date(day: i32) -> String {
    let (_, m, d) = civil(day);
    format!("{} {d}", &MONTH_NAMES[m as usize - 1][..3])
}

pub(crate) fn parse_date(text: &str) -> Result<i32, String> {
    let bad = || "Choose a valid date between 1900 and 2100.".to_string();
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return Err(bad());
    }
    let y: i32 = text[..4].parse().map_err(|_| bad())?;
    let m: u32 = text[5..7].parse().map_err(|_| bad())?;
    let d: u32 = text[8..].parse().map_err(|_| bad())?;
    if !(1900..=2100).contains(&y) || d == 0 || d > days_in_month(y, m) {
        return Err(bad());
    }
    Ok(ordinal(y, m, d))
}

pub(crate) fn parse_time(text: &str) -> Result<u16, String> {
    let bad = || "Choose a valid time in HH:MM format.".to_string();
    let b = text.as_bytes();
    if b.len() != 5 || b[2] != b':' || ![b[0], b[1], b[3], b[4]].iter().all(u8::is_ascii_digit) {
        return Err(bad());
    }
    let h: u16 = text[..2].parse().map_err(|_| bad())?;
    let m: u16 = text[3..].parse().map_err(|_| bad())?;
    if h >= 24 || m >= 60 {
        return Err(bad());
    }
    Ok(h * 60 + m)
}

pub(crate) fn time(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

pub(crate) fn display_time(minute: u16) -> String {
    let h = minute / 60;
    format!(
        "{}:{:02} {}",
        (h + 11) % 12 + 1,
        minute % 60,
        if h < 12 { "AM" } else { "PM" }
    )
}

pub(crate) fn in_range(day: i32) -> bool {
    day >= month_first(FIRST_MONTH) && day < month_first(LAST_MONTH + 1)
}

/// The reader's zone, by `time.timeZone`'s IANA name; UTC for a name the
/// zone database does not know.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn zone(name: &str) -> Tz {
    name.parse().unwrap_or(Tz::UTC)
}

/// `zone`'s offset from UTC at an instant, both in ms.
#[cfg(not(target_arch = "wasm32"))]
fn offset(zone: Tz, ms: f64) -> f64 {
    DateTime::from_timestamp_millis(ms as i64).map_or(0.0, |at| {
        let offset = zone.offset_from_utc_datetime(&at.naive_utc()).fix();
        f64::from(offset.local_minus_utc()) * 1000.0
    })
}

/// The local day an instant falls on in `zone`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn local_day(ms: f64, zone: Tz) -> i32 {
    ((ms + offset(zone, ms)) / DAY_MS).floor() as i32
}

/// The minute of its local day an instant falls on in `zone`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn local_minute(ms: f64, zone: Tz) -> u16 {
    let local = ms + offset(zone, ms);
    ((local - (local / DAY_MS).floor() * DAY_MS) / 60_000.0) as u16
}

/// The instant `day` begins in `zone`: its earlier local midnight where the
/// clocks repeat it, or the first minute after a change that skips it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn midnight(day: i32, zone: Tz) -> f64 {
    let wall = day as f64 * DAY_MS;
    let Some(midnight) = NaiveDate::from_num_days_from_ce_opt(day + 719_163)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
    else {
        return wall;
    };
    (0..=180)
        .find_map(|m| (zone.from_local_datetime(&(midnight + TimeDelta::minutes(m)))).earliest())
        .map_or(wall - offset(zone, wall), |at| at.timestamp_millis() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_date_roundtrips_and_weekdays_continue() {
        for d in month_first(FIRST_MONTH)..month_first(LAST_MONTH + 1) {
            let (y, m, n) = civil(d);
            assert_eq!(ordinal(y, m, n), d);
            assert_eq!(parse_date(&iso(d)).unwrap(), d);
            assert_eq!(weekday(d + 1), (weekday(d) + 1) % 7);
        }
        assert!(parse_date("1900-02-29").is_err());
        assert!(parse_date("2000-02-29").is_ok());
        assert!(parse_date("2100-02-29").is_err());
        assert!(parse_date("2026-00-01").is_err());
        assert_eq!(ordinal(1970, 1, 1), 0);
    }

    #[test]
    fn times_are_wall_clock_values() {
        assert_eq!(parse_time("00:00"), Ok(0));
        assert_eq!(parse_time("23:59"), Ok(1439));
        assert!(parse_time("24:00").is_err());
        assert!(parse_time("1:00").is_err());
        assert_eq!(display_time(0), "12:00 AM");
        assert_eq!(display_time(12 * 60), "12:00 PM");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_day_begins_at_its_own_offset() {
        let la = zone("America/Los_Angeles");
        let hour = 3_600_000.0;
        // 4 July 2026 begins at 07:00 UTC, the -7 hours of summer.
        let july_4 = ordinal(2026, 7, 4);
        assert_eq!(midnight(july_4, la), 1_783_148_400_000.0);
        assert_eq!(local_day(midnight(july_4, la), la), july_4);
        assert_eq!(local_day(midnight(july_4, la) - 1.0, la), july_4 - 1);
        assert_eq!(local_minute(midnight(july_4, la) + 9.0 * hour, la), 9 * 60);
        // The day the clocks go forward is 23 hours long, the day they go back 25.
        let spring = ordinal(2026, 3, 8);
        let fall = ordinal(2026, 11, 1);
        assert_eq!(midnight(spring + 1, la) - midnight(spring, la), 23.0 * hour);
        assert_eq!(midnight(fall + 1, la) - midnight(fall, la), 25.0 * hour);
        // Santiago skips midnight on 6 September 2026: the day begins at 01:00, UTC-3.
        let santiago = zone("America/Santiago");
        let skipped = ordinal(2026, 9, 6);
        assert_eq!(
            midnight(skipped, santiago),
            skipped as f64 * DAY_MS + 4.0 * hour
        );
        assert_eq!(zone("Not/A_Zone"), Tz::UTC);
    }

    #[test]
    fn sheet_dates_show_the_year_only_outside_the_current_year() {
        let today = ordinal(2026, 9, 30);
        assert_eq!(day_label(ordinal(2026, 8, 31), today), "Mon, Aug 31");
        assert_eq!(day_label(ordinal(2025, 8, 31), today), "Sun, Aug 31, 2025");
        assert_eq!(day_label(ordinal(2027, 1, 1), today), "Fri, Jan 1, 2027");
    }
}
