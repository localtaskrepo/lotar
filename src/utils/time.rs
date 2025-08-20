use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc, Weekday};

/// Parse a human-friendly date/time expression into a UTC instant.
///
/// Supported examples:
/// - RFC3339: 2025-12-31T15:04:05Z, with or without zone (local naive -> local tz)
/// - Date only: 2025-12-31 (interpreted as local midnight)
/// - Keywords: now, today, tomorrow, yesterday, next week
/// - Weekday phrases: "next monday", "this friday", "by fri", "fri"
/// - Offsets: "+3d", "+2w", "-1d", "in 3 days", "3 days ago", "+1bd", "next business day"
pub fn parse_human_datetime_to_utc(s: &str) -> Result<DateTime<Utc>, String> {
    let raw = s.trim();
    let lower = raw.to_lowercase();

    // RFC3339 with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Naive local datetime
    if let Some(dt) = parse_local_naive_datetime_to_utc(raw) {
        return Ok(dt);
    }

    // Date only YYYY-MM-DD (local midnight)
    if let Ok(d) = NaiveDate::parse_from_str(&lower, "%Y-%m-%d") {
        let dt_local = Local
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .single();
        if let Some(dt_local) = dt_local {
            return Ok(dt_local.with_timezone(&Utc));
        }
    }

    // Keywords
    match lower.as_str() {
        "now" => return Ok(Utc::now()),
        "today" => {
            let d = Local::now().date_naive();
            let dt = Local
                .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
                .single();
            if let Some(dt) = dt {
                return Ok(dt.with_timezone(&Utc));
            }
        }
        "tomorrow" => {
            let d = Local::now().date_naive() + Duration::days(1);
            let dt = Local
                .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
                .single();
            if let Some(dt) = dt {
                return Ok(dt.with_timezone(&Utc));
            }
        }
        "yesterday" => {
            let d = Local::now().date_naive() - Duration::days(1);
            let dt = Local
                .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
                .single();
            if let Some(dt) = dt {
                return Ok(dt.with_timezone(&Utc));
            }
        }
        "next week" | "nextweek" => {
            let d = Local::now().date_naive() + Duration::weeks(1);
            let dt = Local
                .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
                .single();
            if let Some(dt) = dt {
                return Ok(dt.with_timezone(&Utc));
            }
        }
        _ => {}
    }

    // Weekday phrases
    if let Some(d) = parse_weekday_phrases(&lower) {
        let dt = Local
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .single();
        if let Some(dt) = dt {
            return Ok(dt.with_timezone(&Utc));
        }
    }
    if let Some(d) = parse_next_week_named(&lower) {
        let dt = Local
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .single();
        if let Some(dt) = dt {
            return Ok(dt.with_timezone(&Utc));
        }
    }

    // Relative offsets from now
    if let Some(off) = parse_signed_simple_offset(&lower) {
        return Ok(Utc::now() + off);
    }
    if let Some(off) = parse_in_offset(&lower) {
        return Ok(Utc::now() + off);
    }
    if let Some(off) = parse_ago_offset(&lower) {
        return Ok(Utc::now() - off);
    }
    if let Some(days) = parse_business_days_offset(&lower) {
        let base = Local::now().date_naive();
        let d = add_business_days(base, days);
        let dt = Local
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .single();
        if let Some(dt) = dt {
            return Ok(dt.with_timezone(&Utc));
        }
    }

    Err(format!(
        "Invalid date/time: '{}'. Try YYYY-MM-DD, RFC3339, 'now', 'today', 'in 3 days', '3 days ago', '+2w', '-1d', 'next monday'.",
        s
    ))
}

/// Parse since/until window. Defaults: since=now-30d, until=now.
/// For since, a bare like "14d" is interpreted as "14 days ago".
pub fn parse_since_until(
    since: Option<&str>,
    until: Option<&str>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let now = Utc::now();

    let start = match since {
        None => now - Duration::days(30),
        Some(s) => {
            let s = s.trim();
            let lower = s.to_lowercase();
            if is_bare_offset(&lower) {
                // Interpret bare offsets for convenience: "14d" => now - 14 days
                if let Some(off) = parse_unsigned_days_or_weeks(&lower) {
                    now - off
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

fn is_bare_offset(s: &str) -> bool {
    // e.g., "14d", "2w", "7 days", "3 weeks"
    if let Some(num) = s.strip_suffix('d').or_else(|| s.strip_suffix('w')) {
        return num.parse::<i64>().is_ok();
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 && parts[0].parse::<i64>().is_ok() {
        let unit = parts[1];
        return unit.starts_with('d')
            || unit.starts_with("day")
            || unit.starts_with('w')
            || unit.starts_with("week");
    }
    false
}

fn parse_unsigned_days_or_weeks(s: &str) -> Option<Duration> {
    if let Some(num) = s.strip_suffix('d') {
        let n = num.parse::<i64>().ok()?;
        return Some(Duration::days(n));
    }
    if let Some(num) = s.strip_suffix('w') {
        let n = num.parse::<i64>().ok()?;
        return Some(Duration::weeks(n));
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 {
        if let Ok(n) = parts[0].parse::<i64>() {
            let unit = parts[1].to_lowercase();
            if unit.starts_with('d') || unit.starts_with("day") {
                return Some(Duration::days(n));
            }
            if unit.starts_with('w') || unit.starts_with("week") {
                return Some(Duration::weeks(n));
            }
        }
    }
    None
}

fn parse_weekday_name(name: &str) -> Option<Weekday> {
    match name.to_lowercase().as_str() {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "weds" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn next_occurrence(target: Weekday) -> chrono::NaiveDate {
    let today = Local::now().date_naive();
    let today_num = today.weekday().num_days_from_monday() as i64;
    let target_num = target.num_days_from_monday() as i64;
    let diff = (target_num - today_num).rem_euclid(7);
    let days_ahead = if diff == 0 { 7 } else { diff };
    today + Duration::days(days_ahead)
}

fn parse_weekday_phrases(s: &str) -> Option<chrono::NaiveDate> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("next ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            return Some(next_occurrence(wd));
        }
    }
    if let Some(rest) = t.strip_prefix("this ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            return Some(next_occurrence(wd));
        }
    }
    if let Some(rest) = t.strip_prefix("by ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            return Some(next_occurrence(wd));
        }
    }
    if let Some(wd) = parse_weekday_name(t) {
        return Some(next_occurrence(wd));
    }
    None
}

fn parse_next_week_named(s: &str) -> Option<chrono::NaiveDate> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("next week ") {
        if let Some(wd) = parse_weekday_name(rest.trim()) {
            let today = Local::now().date_naive();
            let mon_this = today - Duration::days(today.weekday().num_days_from_monday() as i64);
            let mon_next = mon_this + Duration::weeks(1);
            let offset_days = wd.num_days_from_monday() as i64;
            return Some(mon_next + Duration::days(offset_days));
        }
    }
    None
}

fn parse_local_naive_datetime_to_utc(s: &str) -> Option<DateTime<Utc>> {
    use chrono::NaiveDateTime;
    let fmts = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ];
    for fmt in &fmts {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            if let Some(dt) = Local.from_local_datetime(&ndt).single() {
                return Some(dt.with_timezone(&Utc));
            }
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
    if let Some(unit) = rest.chars().last() {
        if unit == 'd' || unit == 'w' {
            let num_part = &rest[..rest.len() - 1];
            if let Ok(n) = num_part.parse::<i64>() {
                return Some(if unit == 'd' {
                    Duration::days(sign * n)
                } else {
                    Duration::weeks(sign * n)
                });
            }
        }
    }
    // spaced: +/-10 day(s), +/-2 week(s)
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() == 2 {
        if let Ok(n) = parts[0].parse::<i64>() {
            let unit = parts[1].to_lowercase();
            if unit.starts_with("day") {
                return Some(Duration::days(sign * n));
            }
            if unit.starts_with("week") {
                return Some(Duration::weeks(sign * n));
            }
        }
    }
    None
}

fn parse_in_offset(s: &str) -> Option<Duration> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("in ") {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() == 2 {
            if let Ok(n) = parts[0].parse::<i64>() {
                let unit = parts[1].to_lowercase();
                if unit.starts_with('d') || unit.starts_with("day") {
                    return Some(Duration::days(n));
                }
                if unit.starts_with('w') || unit.starts_with("week") {
                    return Some(Duration::weeks(n));
                }
            }
        }
    }
    None
}

fn parse_ago_offset(s: &str) -> Option<Duration> {
    // "3 days ago", "2w ago"
    let t = s.trim();
    if let Some(rest) = t.strip_suffix(" ago") {
        if let Some(d) = parse_unsigned_days_or_weeks(rest.trim()) {
            return Some(d);
        }
    }
    None
}

fn parse_business_days_offset(s: &str) -> Option<i64> {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix('+') {
        if let Some(rest2) = rest.strip_suffix("bd") {
            if let Ok(n) = rest2.parse::<i64>() {
                return Some(n);
            }
        }
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(n) = parts[0].parse::<i64>() {
                let unit = parts[1].to_lowercase();
                if unit.starts_with("business") {
                    return Some(n);
                }
            }
        }
    }
    if s.eq_ignore_ascii_case("next business day") {
        return Some(1);
    }
    None
}

fn add_business_days(mut date: chrono::NaiveDate, mut days: i64) -> chrono::NaiveDate {
    while days > 0 {
        date += Duration::days(1);
        let wd = date.weekday();
        if wd != Weekday::Sat && wd != Weekday::Sun {
            days -= 1;
        }
    }
    date
}
