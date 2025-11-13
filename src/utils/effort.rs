//! Effort parsing and normalization utilities.
//! Supports time units (m, h, d, w) with combined expressions (e.g., "1d 2h", "90m")
//! and basic points units (p, pt, pts). For now, stats aggregate time-only; points are parsed
//! but ignored by current aggregation.

#[derive(Debug, Clone, PartialEq)]
pub enum EffortKind {
    TimeHours(f64), // normalized to hours
    Points(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffortParsed {
    pub kind: EffortKind,
    pub canonical: String, // compact, lowercase; time is expressed in hours with 'h'
}

impl EffortParsed {
    /// Return true if self >= other, comparing only when the kinds match (time vs points).
    /// If kinds differ, returns false.
    pub fn total_cmp_ge(&self, other: &EffortParsed) -> bool {
        match (&self.kind, &other.kind) {
            (EffortKind::TimeHours(a), EffortKind::TimeHours(b)) => a >= b,
            (EffortKind::Points(a), EffortKind::Points(b)) => a >= b,
            _ => false,
        }
    }

    /// Return true if self <= other, comparing only when the kinds match (time vs points).
    /// If kinds differ, returns false.
    pub fn total_cmp_le(&self, other: &EffortParsed) -> bool {
        match (&self.kind, &other.kind) {
            (EffortKind::TimeHours(a), EffortKind::TimeHours(b)) => a <= b,
            (EffortKind::Points(a), EffortKind::Points(b)) => a <= b,
            _ => false,
        }
    }
}

fn parse_token(token: &str) -> Result<EffortKind, String> {
    let t = token.trim().to_lowercase();
    if t.is_empty() {
        return Err("empty token".into());
    }
    // Points suffixes (longer first)
    for suf in ["points", "point", "pts", "pt", "p"] {
        if let Some(num) = t.strip_suffix(suf) {
            let n: f64 = num.trim().parse().map_err(|_| "invalid number")?;
            if n < 0.0 {
                return Err("effort cannot be negative".into());
            }
            return Ok(EffortKind::Points(n));
        }
    }
    // Time word suffixes (longer first)
    for (suf, factor) in [
        ("minutes", 1.0 / 60.0),
        ("minute", 1.0 / 60.0),
        ("mins", 1.0 / 60.0),
        ("min", 1.0 / 60.0),
        ("hours", 1.0),
        ("hour", 1.0),
        ("hrs", 1.0),
        ("hr", 1.0),
        ("days", 8.0),
        ("day", 8.0),
        ("weeks", 40.0),
        ("week", 40.0),
        ("wks", 40.0),
        ("wk", 40.0),
    ] {
        if let Some(num) = t.strip_suffix(suf) {
            let n: f64 = num.trim().parse().map_err(|_| "invalid number")?;
            if n < 0.0 {
                return Err("effort cannot be negative".into());
            }
            return Ok(EffortKind::TimeHours(n * factor));
        }
    }
    // Time single-letter suffix
    if let Some(ch) = t.chars().last() {
        let num = &t[..t.len() - 1];
        match ch {
            'm' => {
                let n: f64 = num.trim().parse().map_err(|_| "invalid number")?;
                if n < 0.0 {
                    return Err("effort cannot be negative".into());
                }
                return Ok(EffortKind::TimeHours(n / 60.0));
            }
            'h' => {
                let n: f64 = num.trim().parse().map_err(|_| "invalid number")?;
                if n < 0.0 {
                    return Err("effort cannot be negative".into());
                }
                return Ok(EffortKind::TimeHours(n));
            }
            'd' => {
                let n: f64 = num.trim().parse().map_err(|_| "invalid number")?;
                if n < 0.0 {
                    return Err("effort cannot be negative".into());
                }
                return Ok(EffortKind::TimeHours(n * 8.0));
            }
            'w' => {
                let n: f64 = num.trim().parse().map_err(|_| "invalid number")?;
                if n < 0.0 {
                    return Err("effort cannot be negative".into());
                }
                return Ok(EffortKind::TimeHours(n * 40.0));
            }
            _ => {}
        }
    }

    // Plain number: default to points
    if let Ok(n) = t.parse::<f64>() {
        if n < 0.0 {
            return Err("effort cannot be negative".into());
        }
        return Ok(EffortKind::Points(n));
    }

    Err("unrecognized effort token".into())
}

/// Parse an effort string. Supports combined time expressions by summing tokens.
/// Examples: "2h", "1.5d", "90m", "1d 2h" => TimeHours; "3pt", "5" => Points
pub fn parse_effort(input: &str) -> Result<EffortParsed, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty effort".into());
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 1 {
        match parse_token(parts[0])? {
            EffortKind::TimeHours(h) => Ok(EffortParsed {
                kind: EffortKind::TimeHours(h),
                canonical: format!("{:.2}h", h),
            }),
            EffortKind::Points(p) => Ok(EffortParsed {
                kind: EffortKind::Points(p),
                canonical: format!("{}pt", trim_float(p)),
            }),
        }
    } else {
        // Combined tokens: allow number+unit pairs (e.g., "1 hr", "30 min") and single tokens
        let mut total_hours = 0.0;
        let mut total_points = 0.0;
        let mut i = 0usize;
        while i < parts.len() {
            let tok = parts[i].trim();
            if tok.is_empty() {
                i += 1;
                continue;
            }
            match parse_token(tok) {
                Ok(EffortKind::TimeHours(h)) => {
                    total_hours += h;
                    i += 1;
                }
                Ok(EffortKind::Points(p)) => {
                    // If this was a bare number, try to pair with next unit word
                    if tok.parse::<f64>().is_ok() && i + 1 < parts.len() {
                        let unit = parts[i + 1].trim().to_lowercase();
                        if let Some(f) = time_word_unit_factor(&unit) {
                            if p < 0.0 {
                                return Err("effort cannot be negative".into());
                            }
                            total_hours += p * f;
                            i += 2;
                            continue;
                        }
                        if is_points_unit_word(&unit) {
                            if p < 0.0 {
                                return Err("effort cannot be negative".into());
                            }
                            total_points += p;
                            i += 2;
                            continue;
                        }
                    }
                    total_points += p;
                    i += 1;
                }
                Err(_) => {
                    if let Ok(n) = tok.parse::<f64>()
                        && i + 1 < parts.len()
                    {
                        let unit = parts[i + 1].trim().to_lowercase();
                        if let Some(f) = time_word_unit_factor(&unit) {
                            if n < 0.0 {
                                return Err("effort cannot be negative".into());
                            }
                            total_hours += n * f;
                            i += 2;
                            continue;
                        }
                        if is_points_unit_word(&unit) {
                            if n < 0.0 {
                                return Err("effort cannot be negative".into());
                            }
                            total_points += n;
                            i += 2;
                            continue;
                        }
                    }
                    return Err("unrecognized effort token".into());
                }
            }
        }
        if total_hours > 0.0 && total_points > 0.0 {
            return Err("cannot mix points with time tokens".into());
        }
        if total_points > 0.0 {
            return Ok(EffortParsed {
                kind: EffortKind::Points(total_points),
                canonical: format!("{}pt", trim_float(total_points)),
            });
        }
        Ok(EffortParsed {
            kind: EffortKind::TimeHours(total_hours),
            canonical: format!("{:.2}h", total_hours),
        })
    }
}

/// Return hours if the input represents a time effort; otherwise None.
pub fn effort_hours(input: &str) -> Option<f64> {
    match parse_effort(input) {
        Ok(EffortParsed {
            kind: EffortKind::TimeHours(h),
            ..
        }) => Some(h),
        _ => None,
    }
}

fn trim_float(n: f64) -> String {
    // Render without trailing .0 if integer; otherwise keep up to 2 decimals
    if (n.fract() - 0.0).abs() < f64::EPSILON {
        format!("{}", n as i64)
    } else {
        format!("{:.2}", n)
    }
}

fn time_word_unit_factor(unit: &str) -> Option<f64> {
    match unit {
        // minutes
        "m" | "min" | "mins" | "minute" | "minutes" => Some(1.0 / 60.0),
        // hours
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(1.0),
        // days
        "d" | "day" | "days" => Some(8.0),
        // weeks
        "w" | "wk" | "wks" | "week" | "weeks" => Some(40.0),
        _ => None,
    }
}

fn is_points_unit_word(unit: &str) -> bool {
    matches!(unit, "p" | "pt" | "pts" | "point" | "points")
}
