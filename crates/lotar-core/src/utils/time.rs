use jiff::civil::{Date, Weekday};
use jiff::tz::{Offset, TimeZone};
use jiff::{SignedDuration as Duration, Timestamp};

/// RFC 3339 text for a [`Timestamp`] in the shapes chrono wrote, so stored
/// and printed timestamps keep their exact bytes.
pub trait Rfc3339 {
    /// UTC as chrono's `to_rfc3339()` wrote it, the shape task files, history
    /// and comments store: offset `+00:00`, and the fraction left out or
    /// written with 3, 6 or 9 digits.
    fn to_rfc3339(&self) -> String;

    /// [`Rfc3339::to_rfc3339`] with `Z` for UTC, the shape chrono serialized
    /// `DateTime<Utc>` with in JSON output.
    fn to_rfc3339_z(&self) -> String;
}

impl Rfc3339 for Timestamp {
    fn to_rfc3339(&self) -> String {
        format_utc(*self, "+00:00")
    }

    fn to_rfc3339_z(&self) -> String {
        format_utc(*self, "Z")
    }
}

/// The current time as [`Rfc3339::to_rfc3339`] writes it.
pub fn now_rfc3339() -> String {
    Timestamp::now().to_rfc3339()
}

/// Serde `serialize_with` for a [`Timestamp`] field, as [`Rfc3339::to_rfc3339_z`].
pub fn serialize_rfc3339_z<S: serde::Serializer>(ts: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&ts.to_rfc3339_z())
}

fn format_utc(ts: Timestamp, utc: &str) -> String {
    let dt = Offset::UTC.to_datetime(ts);
    let year = dt.year();
    let year = if (0..=9999).contains(&year) { format!("{year:04}") } else { format!("{year:+05}") };
    let nanos = dt.subsec_nanosecond();
    let fraction = if nanos == 0 {
        String::new()
    } else if nanos % 1_000_000 == 0 {
        format!(".{:03}", nanos / 1_000_000)
    } else if nanos % 1_000 == 0 {
        format!(".{:06}", nanos / 1_000)
    } else {
        format!(".{nanos:09}")
    };
    format!("{year}-{:02}-{:02}T{:02}:{:02}:{:02}{fraction}{utc}", dt.month(), dt.day(), dt.hour(), dt.minute(), dt.second())
}

/// Parse an RFC 3339 timestamp with offset (`Z`, `+00:00`, `-05:00`).
/// Accepts everything chrono's `parse_from_rfc3339` did, except fractions
/// longer than nine digits, plus the ISO 8601 forms jiff also reads.
pub fn parse_rfc3339(s: &str) -> Result<Timestamp, jiff::Error> {
    s.parse()
}

/// `n` days of 24 hours. Panics on overflow, like `n * 86400` seconds would.
pub fn days(n: i64) -> Duration {
    Duration::from_hours(n * 24)
}

/// `n` weeks of 7 × 24 hours. Panics on overflow.
pub fn weeks(n: i64) -> Duration {
    days(n * 7)
}

/// `n` days, or `None` when that overflows.
pub fn try_days(n: i64) -> Option<Duration> {
    n.checked_mul(24).and_then(Duration::try_from_hours)
}

/// `n` weeks, or `None` when that overflows.
pub fn try_weeks(n: i64) -> Option<Duration> {
    n.checked_mul(7).and_then(try_days)
}

/// Whole days in `d`, truncated toward zero.
pub fn whole_days(d: Duration) -> i64 {
    d.as_secs() / 86_400
}

/// The system time zone: `TZ` when set, else `/etc/localtime`.
pub fn local_tz() -> TimeZone {
    TimeZone::system()
}

/// Today's date in the system time zone.
pub fn local_today() -> Date {
    local_date(Timestamp::now())
}

/// The calendar date of `ts` in the system time zone.
pub fn local_date(ts: Timestamp) -> Date {
    local_tz().to_datetime(ts).date()
}

/// The calendar date of `ts` in UTC.
pub fn utc_date(ts: Timestamp) -> Date {
    Offset::UTC.to_datetime(ts).date()
}

/// The UTC instant at `date` and the given wall-clock time, for fixtures and fixed boundaries.
pub fn utc(date: Date, hour: i8, minute: i8, second: i8) -> Timestamp {
    Offset::UTC.to_timestamp(date.at(hour, minute, second, 0)).unwrap_or(Timestamp::MIN)
}

/// Local midnight of `date`, or `None` when that local time is skipped or
/// repeated by a DST change.
pub fn local_midnight(date: Date) -> Option<Timestamp> {
    local_tz().to_ambiguous_timestamp(date.to_datetime(jiff::civil::Time::midnight())).unambiguous().ok()
}

/// Parse a human-friendly date/time expression into a UTC instant.
///
/// Supported examples:
/// - RFC3339: 2025-12-31T15:04:05Z, with or without zone (local naive -> local tz)
/// - Date only: 2025-12-31 (interpreted as local midnight)
/// - Keywords: now, today, tomorrow, yesterday, next week
/// - Weekday phrases: "next monday", "this friday", "by fri", "fri"
/// - Offsets: "+3d", "+2w", "-1d", "in 3 days", "3 days ago", "+1bd", "next business day"
pub fn parse_human_datetime_to_utc(s: &str) -> Result<Timestamp, String> {
    let raw = s.trim();
    let lower = raw.to_lowercase();

    // RFC3339 with timezone
    if let Ok(dt) = parse_rfc3339(raw) {
        return Ok(dt);
    }

    // Naive local datetime
    if let Some(dt) = parse_local_naive_datetime_to_utc(raw) {
        return Ok(dt);
    }

    // Date only YYYY-MM-DD (local midnight)
    if let Ok(d) = Date::strptime("%Y-%m-%d", &lower)
        && let Some(dt) = local_midnight(d)
    {
        return Ok(dt);
    }

    // Keywords
    let keyword_date = match lower.as_str() {
        "now" => return Ok(Timestamp::now()),
        "today" => Some(local_today()),
        "tomorrow" => Some(local_today() + days(1)),
        "yesterday" => Some(local_today() - days(1)),
        "next week" | "nextweek" => Some(local_today() + weeks(1)),
        _ => None,
    };
    if let Some(dt) = keyword_date.and_then(local_midnight) {
        return Ok(dt);
    }

    // Weekday phrases
    if let Some(dt) = parse_weekday_phrases(&lower).and_then(local_midnight) {
        return Ok(dt);
    }
    if let Some(dt) = parse_next_week_named(&lower).and_then(local_midnight) {
        return Ok(dt);
    }

    // Relative offsets from now. Whole-day/week offsets use local date
    // arithmetic (snapped to local midnight) so results stay on the same
    // calendar date across DST transitions instead of drifting ±1 day.
    if let Some(off) = parse_signed_simple_offset(&lower) {
        return Ok(offset_from_local_today(whole_days(off)));
    }
    if let Some(off) = parse_in_offset(&lower) {
        return Ok(offset_from_local_today(whole_days(off)));
    }
    if let Some(off) = parse_ago_offset(&lower) {
        return Ok(offset_from_local_today(-whole_days(off)));
    }
    if let Some(n) = parse_business_days_offset(&lower)
        && let Some(dt) = local_midnight(add_business_days(local_today(), n))
    {
        return Ok(dt);
    }

    Err(format!("Invalid date/time: '{}'. Try YYYY-MM-DD, RFC3339, 'now', 'today', 'in 3 days', '3 days ago', '+2w', '-1d', 'next monday'.", s))
}

/// Whole-day offset from today, computed in local date arithmetic and snapped
/// to local midnight (DST-safe). Falls back to instant arithmetic if the
/// resulting local midnight is ambiguous or nonexistent.
fn offset_from_local_today(n: i64) -> Timestamp {
    local_midnight(local_today() + days(n)).unwrap_or_else(|| Timestamp::now() + days(n))
}

/// Parse since/until window. Defaults: since=now-30d, until=now.
/// For since, a bare like "14d" is interpreted as "14 days ago".
pub fn parse_since_until(since: Option<&str>, until: Option<&str>) -> Result<(Timestamp, Timestamp), String> {
    let now = Timestamp::now();

    let start = match since {
        None => now - days(30),
        Some(s) => {
            let s = s.trim();
            let lower = s.to_lowercase();
            if is_bare_offset(&lower) {
                // Interpret bare offsets for convenience: "14d" => now - 14 days
                if let Some(off) = parse_unsigned_days_or_weeks(&lower) {
                    let base = now - off;
                    // Snap bare offsets to the start of the resulting UTC day so whole-day ranges include early activity
                    Offset::UTC.to_timestamp(Offset::UTC.to_datetime(base).date().to_datetime(jiff::civil::Time::midnight())).unwrap_or(base)
                } else {
                    return Err(format!("Invalid --since offset: '{}'", s));
                }
            } else if let Ok(dt) = parse_human_datetime_to_utc(s) {
                dt
            } else {
                return Err(format!("Invalid --since value: '{}'", s));
            }
        }
    };

    let end = match until {
        None => now,
        Some(u) => parse_human_datetime_to_utc(u)?,
    };

    if start > end {
        return Err("since must be <= until".to_string());
    }
    Ok((start, end))
}

/// Parse a compact duration expression like `14d` or `2w` into a `Duration`.
/// Returns `None` when the input is empty or cannot be interpreted.
pub fn parse_duration_like(input: &str) -> Option<Duration> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    parse_unsigned_days_or_weeks(&raw.to_lowercase())
}

fn is_bare_offset(s: &str) -> bool {
    // e.g., "14d", "2w", "7 days", "3 weeks"
    if let Some(num) = s.strip_suffix('d').or_else(|| s.strip_suffix('w')) {
        return num.parse::<i64>().is_ok();
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 && parts[0].parse::<i64>().is_ok() {
        let unit = parts[1];
        return unit.starts_with('d') || unit.starts_with("day") || unit.starts_with('w') || unit.starts_with("week");
    }
    false
}

fn parse_unsigned_days_or_weeks(s: &str) -> Option<Duration> {
    // Checked constructors: an out-of-range count is "unparseable", never a panic.
    if let Some(num) = s.strip_suffix('d') {
        let n = num.parse::<i64>().ok()?;
        return try_days(n);
    }
    if let Some(num) = s.strip_suffix('w') {
        let n = num.parse::<i64>().ok()?;
        return try_weeks(n);
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2
        && let Ok(n) = parts[0].parse::<i64>()
    {
        let unit = parts[1].to_lowercase();
        if unit.starts_with('d') || unit.starts_with("day") {
            return try_days(n);
        }
        if unit.starts_with('w') || unit.starts_with("week") {
            return try_weeks(n);
        }
    }
    None
}

/// Upper bound for sprint durations (ten years), so a configured value can
/// never overflow date arithmetic.
pub const MAX_SPRINT_DURATION_DAYS: i64 = 3650;

#[derive(Clone, Copy)]
enum SprintDurationUnit {
    Hours,
    Days,
    Weeks,
}

/// Strict `<count><unit>` / `<count> <unit>` grammar shared by sprint
/// durations. Units: h/hour(s) (only when `allow_hours`), d/day(s), w/week(s).
fn parse_sprint_duration(input: &str, allow_hours: bool, allow_zero: bool) -> Result<Duration, String> {
    let raw = input.trim().to_lowercase();
    let units = if allow_hours { "h (hours), d (days) or w (weeks), e.g. 12h, 3d or 2w" } else { "d (days) or w (weeks), e.g. 14d or 2w" };
    let invalid = || format!("'{}' is not a duration; use a whole number with {}", input.trim(), units);
    let split = raw.find(|c: char| !c.is_ascii_digit()).ok_or_else(invalid)?;
    let (digits, unit) = raw.split_at(split);
    if digits.is_empty() {
        return Err(invalid());
    }
    let unit = match unit.trim() {
        "h" | "hour" | "hours" if allow_hours => SprintDurationUnit::Hours,
        "d" | "day" | "days" => SprintDurationUnit::Days,
        "w" | "week" | "weeks" => SprintDurationUnit::Weeks,
        _ => return Err(invalid()),
    };
    let max_hours = MAX_SPRINT_DURATION_DAYS * 24;
    let count: i64 = digits.parse().map_err(|_| format!("'{}' is longer than the {} day maximum", input.trim(), MAX_SPRINT_DURATION_DAYS))?;
    let hours = match unit {
        SprintDurationUnit::Hours => Some(count),
        SprintDurationUnit::Days => count.checked_mul(24),
        SprintDurationUnit::Weeks => count.checked_mul(24 * 7),
    };
    match hours {
        Some(0) if !allow_zero => Err(format!("'{}' must be longer than zero", input.trim())),
        Some(hours) if hours <= max_hours => Ok(Duration::from_hours(hours)),
        _ => Err(format!("'{}' is longer than the {} day maximum", input.trim(), MAX_SPRINT_DURATION_DAYS)),
    }
}

/// Validate a sprint plan length (`sprints.defaults.length`, `--length`):
/// a positive number of days or weeks, at most ten years.
pub fn parse_sprint_length(input: &str) -> Result<Duration, String> {
    parse_sprint_duration(input, false, false)
}

/// Validate a sprint overdue grace period (`sprints.defaults.overdue_after`,
/// `--overdue-after`): zero or more hours, days or weeks, at most ten years.
pub fn parse_sprint_overdue_after(input: &str) -> Result<Duration, String> {
    parse_sprint_duration(input, true, true)
}

fn parse_weekday_name(name: &str) -> Option<Weekday> {
    match name.to_lowercase().as_str() {
        "mon" | "monday" => Some(Weekday::Monday),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tuesday),
        "wed" | "weds" | "wednesday" => Some(Weekday::Wednesday),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thursday),
        "fri" | "friday" => Some(Weekday::Friday),
        "sat" | "saturday" => Some(Weekday::Saturday),
        "sun" | "sunday" => Some(Weekday::Sunday),
        _ => None,
    }
}

fn next_occurrence(target: Weekday) -> Date {
    let today = local_today();
    let today_num = i64::from(today.weekday().to_monday_zero_offset());
    let target_num = i64::from(target.to_monday_zero_offset());
    let diff = (target_num - today_num).rem_euclid(7);
    let days_ahead = if diff == 0 { 7 } else { diff };
    today + days(days_ahead)
}

fn parse_weekday_phrases(s: &str) -> Option<Date> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("next ")
        && let Some(wd) = parse_weekday_name(rest.trim())
    {
        return Some(next_occurrence(wd));
    }
    if let Some(rest) = t.strip_prefix("this ")
        && let Some(wd) = parse_weekday_name(rest.trim())
    {
        return Some(next_occurrence(wd));
    }
    if let Some(rest) = t.strip_prefix("by ")
        && let Some(wd) = parse_weekday_name(rest.trim())
    {
        return Some(next_occurrence(wd));
    }
    if let Some(wd) = parse_weekday_name(t) {
        return Some(next_occurrence(wd));
    }
    None
}

fn parse_next_week_named(s: &str) -> Option<Date> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("next week ")
        && let Some(wd) = parse_weekday_name(rest.trim())
    {
        let today = local_today();
        let mon_this = today - days(i64::from(today.weekday().to_monday_zero_offset()));
        let mon_next = mon_this + weeks(1);
        return Some(mon_next + days(i64::from(wd.to_monday_zero_offset())));
    }
    None
}

/// Public probe: does this string look like a naive local datetime?
pub fn parse_naive_local_datetime_to_utc(s: &str) -> Option<Timestamp> {
    parse_local_naive_datetime_to_utc(s)
}

fn parse_local_naive_datetime_to_utc(s: &str) -> Option<Timestamp> {
    let fmts = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"];
    for fmt in fmts {
        if let Ok(ndt) = jiff::civil::DateTime::strptime(fmt, s)
            && let Ok(dt) = local_tz().to_ambiguous_timestamp(ndt).unambiguous()
        {
            return Some(dt);
        }
    }
    None
}

fn parse_signed_simple_offset(s: &str) -> Option<Duration> {
    let t = s.trim_start();
    if !(t.starts_with('+') || t.starts_with('-')) {
        return None;
    }
    let sign = if t.starts_with('-') { -1 } else { 1 };
    let rest = &t[1..];
    // compact: +/-10d, +/-2w
    if let Some(unit) = rest.chars().last()
        && (unit == 'd' || unit == 'w')
    {
        let num_part = &rest[..rest.len() - 1];
        if let Ok(n) = num_part.parse::<i64>() {
            return Some(if unit == 'd' { days(sign * n) } else { weeks(sign * n) });
        }
    }
    // spaced: +/-10 day(s), +/-2 week(s)
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 2
        && let Ok(n) = parts[0].parse::<i64>()
    {
        let unit = parts[1].to_lowercase();
        if unit.starts_with("day") {
            return Some(days(sign * n));
        }
        if unit.starts_with("week") {
            return Some(weeks(sign * n));
        }
    }
    None
}

fn parse_in_offset(s: &str) -> Option<Duration> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("in ") {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() == 2
            && let Ok(n) = parts[0].parse::<i64>()
        {
            let unit = parts[1].to_lowercase();
            if unit.starts_with('d') || unit.starts_with("day") {
                return Some(days(n));
            }
            if unit.starts_with('w') || unit.starts_with("week") {
                return Some(weeks(n));
            }
        }
    }
    None
}

fn parse_ago_offset(s: &str) -> Option<Duration> {
    // "3 days ago", "2w ago"
    let t = s.trim();
    if let Some(rest) = t.strip_suffix(" ago")
        && let Some(d) = parse_unsigned_days_or_weeks(rest.trim())
    {
        return Some(d);
    }
    None
}

fn parse_business_days_offset(s: &str) -> Option<i64> {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix('+') {
        if let Some(rest2) = rest.strip_suffix("bd")
            && let Ok(n) = rest2.parse::<i64>()
        {
            return Some(n);
        }
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() >= 2
            && let Ok(n) = parts[0].parse::<i64>()
        {
            let unit = parts[1].to_lowercase();
            if unit.starts_with("business") {
                return Some(n);
            }
        }
    }
    if s.eq_ignore_ascii_case("next business day") {
        return Some(1);
    }
    None
}

fn add_business_days(mut date: Date, mut remaining: i64) -> Date {
    while remaining > 0 {
        date += days(1);
        if !matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday) {
            remaining -= 1;
        }
    }
    date
}

/// The milliseconds in environment variable `var`, or `default` when it is
/// unset, not a number or zero. Lets tests shorten waits and poll intervals.
pub fn env_millis(var: &str, default: std::time::Duration) -> std::time::Duration {
    std::env::var(var).ok().and_then(|raw| raw.trim().parse::<u64>().ok()).filter(|ms| *ms > 0).map(std::time::Duration::from_millis).unwrap_or(default)
}

#[cfg(test)]
mod sprint_duration_tests {
    use super::*;

    #[test]
    fn sprint_length_accepts_days_and_weeks_only() {
        assert_eq!(parse_sprint_length("2w"), Ok(weeks(2)));
        assert_eq!(parse_sprint_length(" 14D "), Ok(days(14)));
        assert_eq!(parse_sprint_length("3 weeks"), Ok(weeks(3)));
        assert_eq!(parse_sprint_length("1 day"), Ok(days(1)));
        assert_eq!(parse_sprint_length("520w"), Ok(weeks(520)));
        for invalid in ["", "12h", "0d", "-2w", "+2w", "2", "w", "2 dogs", "2.5w", "1y", "3651d"] {
            assert!(parse_sprint_length(invalid).is_err(), "{invalid:?} should be rejected");
        }
    }

    #[test]
    fn sprint_overdue_after_accepts_hours_and_zero() {
        assert_eq!(parse_sprint_overdue_after("12h"), Ok(Duration::from_hours(12)));
        assert_eq!(parse_sprint_overdue_after("0d"), Ok(Duration::ZERO));
        assert_eq!(parse_sprint_overdue_after("2 hours"), Ok(Duration::from_hours(2)));
        assert_eq!(parse_sprint_overdue_after("87600h"), Ok(days(3650)));
        for invalid in ["87601h", "soon", "-1d", "99999999999999999999h"] {
            assert!(parse_sprint_overdue_after(invalid).is_err(), "{invalid:?} should be rejected");
        }
    }

    #[test]
    fn duration_like_treats_out_of_range_counts_as_unparseable() {
        assert_eq!(parse_duration_like("9223372036854775807w"), None);
        assert_eq!(parse_duration_like("9223372036854775807 days"), None);
        assert_eq!(parse_duration_like("2w"), Some(weeks(2)));
    }
}

#[cfg(test)]
mod rfc3339_tests {
    use super::*;

    fn at(nanos: i32) -> Timestamp {
        utc(jiff::civil::date(2026, 11, 17), 18, 13, 11) + Duration::new(0, nanos)
    }

    /// Task files written before the move to jiff (DEV-669) and after it carry the same bytes.
    #[test]
    fn writes_the_shapes_chrono_wrote() {
        assert_eq!(at(0).to_rfc3339(), "2026-11-17T18:13:11+00:00");
        assert_eq!(at(120_000_000).to_rfc3339(), "2026-11-17T18:13:11.120+00:00");
        assert_eq!(at(123_456_000).to_rfc3339(), "2026-11-17T18:13:11.123456+00:00");
        assert_eq!(at(123_456_789).to_rfc3339(), "2026-11-17T18:13:11.123456789+00:00");
        assert_eq!(at(1).to_rfc3339(), "2026-11-17T18:13:11.000000001+00:00");
        assert_eq!(at(0).to_rfc3339_z(), "2026-11-17T18:13:11Z");
        assert_eq!(at(5_000_000).to_rfc3339_z(), "2026-11-17T18:13:11.005Z");
        assert_eq!(utc(jiff::civil::date(33, 1, 2), 3, 4, 5).to_rfc3339(), "0033-01-02T03:04:05+00:00");
    }

    #[test]
    fn reads_what_it_writes_and_offset_forms() {
        for nanos in [0, 120_000_000, 123_456_000, 123_456_789] {
            assert_eq!(parse_rfc3339(&at(nanos).to_rfc3339()).ok(), Some(at(nanos)));
            assert_eq!(parse_rfc3339(&at(nanos).to_rfc3339_z()).ok(), Some(at(nanos)));
        }
        assert_eq!(parse_rfc3339("2026-11-17T13:13:11-05:00").ok(), Some(at(0)));
        assert_eq!(parse_rfc3339("2026-11-17t18:13:11z").ok(), Some(at(0)));
        assert!(parse_rfc3339("2026-11-17").is_err());
        assert!(parse_rfc3339("2026-11-17T18:13:11").is_err());
    }
}
