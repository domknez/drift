//! Times as the service sends them (ISO 8601) and as the UI shows them, without a
//! date library.

use std::time::{SystemTime, UNIX_EPOCH};

/// Now, in Unix milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

/// Unix seconds as calendar fields (UTC, or local after adding the offset).
pub fn civil(t: i64) -> Civil {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    Civil { year, month, day, hour: (secs / 3600) as u32, minute: (secs % 3600 / 60) as u32 }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// `2026-10-30T08:14:05.123Z` (or with an offset) as Unix milliseconds.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |r: std::ops::Range<usize>| s.get(r).and_then(|d| d.parse::<i64>().ok());
    let (year, month, day) = (num(0..4)?, num(5..7)? as u32, num(8..10)? as u32);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let rest = s.get(19..).unwrap_or("");
    let (fraction, zone) = match rest.strip_prefix('.') {
        Some(r) => r.split_at(r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len())),
        None => ("", rest),
    };
    let millis = format!("{fraction:0<3}").get(0..3).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0);
    let offset = match zone.as_bytes().first() {
        Some(b'+' | b'-') => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let digits = zone[1..].replace(':', "");
            sign * (digits.get(0..2)?.parse::<i64>().ok()? * 3600 + digits.get(2..4).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0) * 60)
        }
        _ => 0,
    };
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset;
    Some(secs * 1000 + millis)
}

/// The computer's time zone at `t` (Unix seconds), in seconds east of UTC.
pub fn local_offset(t: i64) -> i64 {
    #[cfg(unix)]
    {
        // SAFETY: localtime_r only writes into the tm we pass.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&(t as libc::time_t), &mut tm).is_null() {
                return 0;
            }
            tm.tm_gmtoff as i64
        }
    }
    #[cfg(not(unix))]
    {
        let _ = t;
        0
    }
}

fn local(ms: i64) -> Civil {
    let t = ms.div_euclid(1000);
    civil(t + local_offset(t))
}

/// `14:05`, local time.
pub fn clock(ms: i64) -> String {
    let c = local(ms);
    format!("{:02}:{:02}", c.hour, c.minute)
}

/// For the day dividers in a conversation: `Today`, `Yesterday`, `Monday 27 October`.
pub fn day(ms: i64, now: i64) -> String {
    let (c, today) = (local(ms), local(now));
    let d = days_from_civil(c.year, c.month, c.day);
    match days_from_civil(today.year, today.month, today.day) - d {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ if c.year == today.year => format!("{} {} {}", weekday(d), c.day, month_name(c.month)),
        _ => format!("{} {} {} {}", weekday(d), c.day, month_name(c.month), c.year),
    }
}

/// The local calendar day of `ms`, for telling whether two times fall on the same day.
pub fn day_number(ms: i64) -> i64 {
    let c = local(ms);
    days_from_civil(c.year, c.month, c.day)
}

fn weekday(days: i64) -> &'static str {
    // 1970-01-01 was a Thursday.
    ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"][days.rem_euclid(7) as usize]
}

fn month_name(m: u32) -> &'static str {
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(m.clamp(1, 12) - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_times() {
        assert_eq!(parse_iso("2026-10-29T20:00:00Z"), Some(1_793_304_000_000));
        assert_eq!(parse_iso("2026-10-29T20:00:00.5Z"), Some(1_793_304_000_500));
        assert_eq!(parse_iso("2026-10-29T20:00:00.1234567Z"), Some(1_793_304_000_123));
        assert_eq!(parse_iso("2026-10-29T21:00:00+01:00"), Some(1_793_304_000_000));
        assert_eq!(parse_iso("garbage"), None);
    }

    #[test]
    fn knows_the_weekday() {
        assert_eq!(weekday(days_from_civil(2026, 10, 30)), "Friday");
        assert_eq!(civil(1_793_304_000), Civil { year: 2026, month: 10, day: 29, hour: 20, minute: 0 });
    }
}
