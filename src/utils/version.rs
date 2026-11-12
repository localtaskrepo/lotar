//! Version values and version ranges for `version` custom fields (DEV-373).
//!
//! A version is `MAJOR[.MINOR[.PATCH]][-PRERELEASE][+BUILD]`, with an
//! optional leading `v`. Versions order by semver precedence; build metadata
//! is ignored. A partial version (`0.13`, `1`, `0.13.x`) stands for every
//! version it starts: `0.13` is every 0.13.x, prereleases included. A field
//! may hold one version or a list.
//!
//! A requirement is one or more comparators, all of which must hold:
//! `>=0.13 <0.14`, `=0.13`, `0.13.2`, `^0.13`, `~0.13.2`, `*`. A task's value
//! matches when one of its versions overlaps the requirement, so a bug that
//! affects `0.13` matches `affects>=0.13.2`.

use std::cmp::Ordering;
use std::fmt;

/// One prerelease identifier: numeric ones sort before alphanumeric ones.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Ident {
    Num(u64),
    Alpha(String),
}

/// The prerelease part of a comparison key. `Min` sorts below every real
/// version with the same numbers, so a range can start at `0.14.0-Min` and
/// take in `0.14.0-rc.1`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Pre {
    Min,
    Ids(Vec<Ident>),
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    nums: [u64; 3],
    pre: Pre,
}

impl Key {
    fn floor(nums: [u64; 3]) -> Self {
        Self { nums, pre: Pre::Min }
    }
}

/// A parsed version value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    nums: [u64; 3],
    /// How many numbers were written: fewer than 3 (with no prerelease)
    /// makes a partial version.
    given: usize,
    pre: Option<Vec<Ident>>,
    /// The value as stored: trimmed, without a leading `v`.
    text: String,
}

/// One end of an interval: the key and whether it is included; `None` is
/// unbounded.
type Bound = Option<(Key, bool)>;

/// A set of versions: everything between two bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Interval {
    lo: Bound,
    hi: Bound,
}

/// The tighter of two bounds; `keep_greater` picks the later one (for lower
/// bounds). An excluded key is tighter than the same key included.
fn tighter(a: &Bound, b: &Bound, keep_greater: bool) -> Bound {
    match (a, b) {
        (None, other) | (other, None) => other.clone(),
        (Some((ka, ia)), Some((kb, ib))) => match ka.cmp(kb) {
            Ordering::Equal => Some((ka.clone(), *ia && *ib)),
            Ordering::Greater if keep_greater => a.clone(),
            Ordering::Less if !keep_greater => a.clone(),
            _ => b.clone(),
        },
    }
}

impl Interval {
    const ALL: Interval = Interval { lo: None, hi: None };

    fn intersect(&self, other: &Interval) -> Interval {
        Interval { lo: tighter(&self.lo, &other.lo, true), hi: tighter(&self.hi, &other.hi, false) }
    }

    fn is_empty(&self) -> bool {
        match (&self.lo, &self.hi) {
            (Some((lo, lo_incl)), Some((hi, hi_incl))) => match lo.cmp(hi) {
                Ordering::Less => false,
                Ordering::Equal => !(*lo_incl && *hi_incl),
                Ordering::Greater => true,
            },
            _ => false,
        }
    }
}

fn parse_ident(raw: &str) -> Result<Ident, ()> {
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(());
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        return raw.parse().map(Ident::Num).map_err(|_| ());
    }
    Ok(Ident::Alpha(raw.to_string()))
}

impl Version {
    /// Parse a version value. Errors name the value and the expected shape.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let invalid = || format!("'{}' is not a version: use numbers like 1.4.2, 0.13 or 1.0.0-rc.1", raw.trim());
        let trimmed = raw.trim();
        let text = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);
        if text.is_empty() {
            return Err(invalid());
        }
        let core = text.split_once('+').map_or(text, |(core, _build)| core);
        let (numbers, pre) = match core.split_once('-') {
            Some((numbers, pre)) => (numbers, Some(pre)),
            None => (core, None),
        };
        let mut parts: Vec<&str> = numbers.split('.').collect();
        // `0.13.x` and `0.13.*` are the partial `0.13`.
        while parts.len() > 1 && pre.is_none() && matches!(parts.last(), Some(&("x" | "X" | "*"))) {
            parts.pop();
        }
        if parts.len() > 3 {
            return Err(invalid());
        }
        let mut nums = [0u64; 3];
        for (slot, part) in nums.iter_mut().zip(&parts) {
            if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
                return Err(invalid());
            }
            *slot = part.parse().map_err(|_| invalid())?;
        }
        let pre = match pre {
            Some(pre) => Some(pre.split('.').map(parse_ident).collect::<Result<Vec<_>, _>>().map_err(|_| invalid())?),
            None => None,
        };
        Ok(Self { nums, given: parts.len(), pre, text: text.to_string() })
    }

    /// The value to store: as entered, without a leading `v`.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether this stands for a range of versions (`0.13`, `1`).
    pub fn is_partial(&self) -> bool {
        self.given < 3 && self.pre.is_none()
    }

    fn key(&self) -> Key {
        Key { nums: self.nums, pre: self.pre.clone().map_or(Pre::Release, Pre::Ids) }
    }

    /// The first version after every version this one stands for.
    fn next_partial(&self) -> Key {
        let [major, minor, _] = self.nums;
        match self.given {
            1 => Key::floor([major + 1, 0, 0]),
            _ => Key::floor([major, minor + 1, 0]),
        }
    }

    /// The first version this value stands for.
    fn start(&self) -> Key {
        if self.is_partial() { Key::floor(self.nums) } else { self.key() }
    }

    /// The versions this value stands for.
    fn interval(&self) -> Interval {
        if self.is_partial() {
            Interval { lo: Some((self.start(), true)), hi: Some((self.next_partial(), false)) }
        } else {
            Interval { lo: Some((self.key(), true)), hi: Some((self.key(), true)) }
        }
    }

    /// Order for sorting: by the first version a value stands for, so `0.13`
    /// sorts just before `0.13.0`.
    pub fn sort_cmp(&self, other: &Version) -> Ordering {
        self.start().cmp(&other.start()).then_with(|| self.text.cmp(&other.text))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// A version requirement: comparators that must all hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq {
    interval: Interval,
    text: String,
}

impl VersionReq {
    /// Parse a requirement such as `>=0.13 <0.14`, `^1.2` or `0.13`.
    /// Comparators are separated by spaces; an operator may stand apart
    /// from its version (`>= 0.13`).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.trim();
        if text.is_empty() {
            return Err("A version range needs at least one version".to_string());
        }
        let mut interval = Interval::ALL;
        let mut pending_op: Option<&str> = None;
        for word in text.split_whitespace() {
            let (op, rest) = split_operator(word);
            let (op, rest) = match (pending_op.take(), op) {
                (Some(op), "") => (op, rest),
                (Some(op), _) => return Err(format!("'{op}' needs a version after it")),
                (None, op) => (op, rest),
            };
            if rest.is_empty() {
                if op.is_empty() {
                    continue;
                }
                pending_op = Some(op);
                continue;
            }
            interval = interval.intersect(&comparator(op, rest)?);
        }
        if let Some(op) = pending_op {
            return Err(format!("'{op}' needs a version after it"));
        }
        Ok(Self { interval, text: text.to_string() })
    }

    /// Whether `version` (or, for a partial one, any version it stands for)
    /// meets the requirement.
    pub fn matches(&self, version: &Version) -> bool {
        !self.interval.intersect(&version.interval()).is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

fn split_operator(word: &str) -> (&str, &str) {
    for op in [">=", "<=", "==", ">", "<", "=", "^", "~"] {
        if let Some(rest) = word.strip_prefix(op) {
            return (op, rest);
        }
    }
    ("", word)
}

fn comparator(op: &str, raw: &str) -> Result<Interval, String> {
    if raw == "*" || raw.eq_ignore_ascii_case("x") {
        return match op {
            "" | "=" | "==" | ">=" | "<=" | "^" | "~" => Ok(Interval::ALL),
            _ => Err(format!("'{op}*' is not a version range")),
        };
    }
    let version = Version::parse(raw)?;
    let own = version.interval();
    let start = version.start();
    let [major, minor, patch] = version.nums;
    let interval = match op {
        "" | "=" | "==" => own,
        // After every version the operand stands for.
        ">" => Interval { lo: own.hi.map(|(key, incl)| (key, !incl)), hi: None },
        ">=" => Interval { lo: Some((start, true)), hi: None },
        "<" => Interval { lo: None, hi: Some((start, false)) },
        "<=" => Interval { lo: None, hi: own.hi },
        // Cargo's caret: changes that keep the leftmost non-zero number.
        "^" => {
            let end = match (version.given, major, minor) {
                (_, m, _) if m > 0 => [m + 1, 0, 0],
                (1, 0, _) => [1, 0, 0],
                (2, 0, n) if n > 0 => [0, n + 1, 0],
                (2, 0, 0) => [0, 1, 0],
                (_, 0, n) if n > 0 => [0, n + 1, 0],
                _ => [0, 0, patch + 1],
            };
            Interval { lo: Some((start, true)), hi: Some((Key::floor(end), false)) }
        }
        // Tilde: patch changes, or minor ones when only a major is given.
        "~" => {
            let end = if version.given == 1 { [major + 1, 0, 0] } else { [major, minor + 1, 0] };
            Interval { lo: Some((start, true)), hi: Some((Key::floor(end), false)) }
        }
        _ => return Err(format!("Unknown version operator '{op}'")),
    };
    Ok(interval)
}

/// Whether a filter value is written as a comparison (`<`, `>`), which only
/// version fields understand.
pub fn is_comparison(value: &str) -> bool {
    value.trim_start().starts_with(['<', '>'])
}

/// The version strings a custom field value holds: one for a scalar, one per
/// item for a list. Numbers (a hand-written `affects: 0.13` loads as one)
/// read as written by YAML's number formatting.
#[cfg(not(feature = "schema"))]
pub fn value_strings(value: &crate::types::CustomFieldValue) -> Vec<String> {
    use serde_yaml_ng::Value;
    match value {
        Value::Sequence(items) => items.iter().flat_map(value_strings).collect(),
        Value::Null => Vec::new(),
        Value::String(s) if s.trim().is_empty() => Vec::new(),
        other => vec![crate::types::custom_value_to_string(other)],
    }
}

#[cfg(feature = "schema")]
pub fn value_strings(value: &crate::types::CustomFieldValue) -> Vec<String> {
    use serde_json::Value;
    match value {
        Value::Array(items) => items.iter().flat_map(value_strings).collect(),
        Value::Null => Vec::new(),
        Value::String(s) if s.trim().is_empty() => Vec::new(),
        other => vec![crate::types::custom_value_to_string(other)],
    }
}

/// The versions a custom field value holds; unreadable items are skipped.
pub fn value_versions(value: &crate::types::CustomFieldValue) -> Vec<Version> {
    value_strings(value).iter().filter_map(|raw| Version::parse(raw).ok()).collect()
}

/// Check and normalise a value written to a version field: every item must
/// read as a version; each is stored as a string without a leading `v`. A
/// string with commas (`--field affects=0.13.2,0.13.3`) becomes a list.
pub fn normalize_value(field: &str, value: &crate::types::CustomFieldValue) -> Result<crate::types::CustomFieldValue, String> {
    let mut items: Vec<String> = Vec::new();
    for raw in value_strings(value) {
        items.extend(raw.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_string));
    }
    let mut versions = Vec::new();
    for raw in &items {
        let version = Version::parse(raw).map_err(|message| format!("Field '{field}' holds versions: {message}"))?;
        versions.push(version.as_str().to_string());
    }
    let is_list = items.len() > 1 || is_sequence(value);
    Ok(match versions.len() {
        0 if !is_list => crate::types::custom_value_string(""),
        1 if !is_list => crate::types::custom_value_string(versions.remove(0)),
        _ => string_list(versions),
    })
}

#[cfg(not(feature = "schema"))]
fn is_sequence(value: &crate::types::CustomFieldValue) -> bool {
    value.is_sequence()
}

#[cfg(feature = "schema")]
fn is_sequence(value: &crate::types::CustomFieldValue) -> bool {
    value.is_array()
}

#[cfg(not(feature = "schema"))]
fn string_list(items: Vec<String>) -> crate::types::CustomFieldValue {
    serde_yaml_ng::Value::Sequence(items.into_iter().map(serde_yaml_ng::Value::String).collect())
}

#[cfg(feature = "schema")]
fn string_list(items: Vec<String>) -> crate::types::CustomFieldValue {
    serde_json::Value::Array(items.into_iter().map(serde_json::Value::String).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(raw: &str) -> Version {
        Version::parse(raw).unwrap()
    }

    fn matches(req: &str, version: &str) -> bool {
        VersionReq::parse(req).unwrap().matches(&v(version))
    }

    #[test]
    fn parses_full_partial_prefixed_and_prerelease_versions() {
        assert_eq!(v("v0.13.3").as_str(), "0.13.3");
        assert_eq!(v(" V1.0 ").as_str(), "1.0");
        assert!(v("0.13").is_partial());
        assert!(v("0.13.x").is_partial());
        assert!(v("2").is_partial());
        assert!(!v("0.13.0").is_partial());
        assert!(!v("1.0-rc1").is_partial());
        assert_eq!(v("1.0.0-rc.1+build.5").as_str(), "1.0.0-rc.1+build.5");
        for bad in ["", "v", "abc", "1.2.3.4", "1..2", "1.2.3-", "1.2.3-rc..1", "1.a", "-1", "1.2.3-r$"] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn orders_by_semver_precedence() {
        let mut list = vec![
            v("1.0.0"),
            v("1.0.0-rc.1"),
            v("1.0.0-alpha"),
            v("1.0.0-alpha.1"),
            v("1.0.0-alpha.beta"),
            v("1.0.0-beta.11"),
            v("1.0.0-beta.2"),
            v("0.13.10"),
            v("0.13.9"),
            v("0.13"),
        ];
        list.sort_by(Version::sort_cmp);
        let order: Vec<&str> = list.iter().map(Version::as_str).collect();
        assert_eq!(
            order,
            ["0.13", "0.13.9", "0.13.10", "1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"]
        );
    }

    #[test]
    fn comparisons_against_full_versions() {
        assert!(matches(">=0.13", "0.13.0"));
        assert!(matches(">=0.13", "0.13.0-rc.1"));
        assert!(!matches(">=0.13", "0.12.9"));
        assert!(matches("<0.14", "0.13.99"));
        assert!(!matches("<0.14", "0.14.0-rc.1"));
        assert!(matches("<0.14.0", "0.14.0-rc.1"));
        assert!(matches(">0.13.2", "0.13.3"));
        assert!(!matches(">0.13.2", "0.13.2"));
        assert!(!matches(">0.13", "0.13.5"));
        assert!(matches(">0.13", "0.14.0"));
        assert!(matches("<=1.0", "1.0.7"));
        assert!(!matches("<=1.0", "1.1.0"));
        assert!(matches("=0.13", "0.13.4"));
        assert!(matches("0.13", "0.13.4"));
        assert!(!matches("0.13", "0.14.0"));
        assert!(matches("v1.2.3", "1.2.3"));
        assert!(!matches("1.2.3", "1.2.3-rc.1"));
        assert!(matches("1.2.3+build", "1.2.3"));
        assert!(matches("*", "9.9.9"));
    }

    #[test]
    fn ranges_combine_with_and() {
        assert!(matches(">=0.13 <0.14", "0.13.7"));
        assert!(!matches(">=0.13 <0.14", "0.14.0"));
        assert!(matches(">= 0.13 < 0.14", "0.13.0"));
        assert!(!matches(">=0.14 <0.13", "0.13.5"));
        assert!(VersionReq::parse(">=").is_err());
        assert!(VersionReq::parse(">= >=1").is_err());
        assert!(VersionReq::parse(">=abc").is_err());
        assert!(VersionReq::parse("").is_err());
    }

    #[test]
    fn caret_and_tilde_follow_cargo() {
        assert!(matches("^0.13", "0.13.9"));
        assert!(!matches("^0.13", "0.14.0"));
        assert!(matches("^1.2", "1.9.0"));
        assert!(!matches("^1.2", "2.0.0"));
        assert!(!matches("^1.2", "1.1.0"));
        assert!(matches("^0.0.3", "0.0.3"));
        assert!(!matches("^0.0.3", "0.0.4"));
        assert!(matches("^0", "0.9.0"));
        assert!(!matches("^0", "1.0.0"));
        assert!(matches("~0.13.2", "0.13.9"));
        assert!(!matches("~0.13.2", "0.13.1"));
        assert!(!matches("~0.13.2", "0.14.0"));
        assert!(matches("~1", "1.9.0"));
        assert!(!matches("~1", "2.0.0"));
    }

    #[test]
    fn partial_values_match_overlapping_ranges() {
        // A bug that affects every 0.13.x affects 0.13.2 and later.
        assert!(matches(">=0.13.2", "0.13"));
        assert!(matches("0.13.2", "0.13"));
        assert!(!matches("<0.13", "0.13"));
        assert!(!matches(">0.13", "0.13"));
        assert!(matches(">0.12.5", "0.13"));
        assert!(matches("^0.13", "0.13"));
        assert!(!matches(">=0.14", "0.13"));
    }

    #[cfg(not(feature = "schema"))]
    #[test]
    fn reads_and_normalizes_field_values() {
        let yaml = |raw: &str| serde_yaml_ng::from_str::<crate::types::CustomFieldValue>(raw).unwrap();
        // A hand-written `affects: 0.13` is a float in YAML.
        assert_eq!(value_strings(&yaml("0.13")), ["0.13"]);
        assert_eq!(value_strings(&yaml("[0.13.2, v0.13.3]")), ["0.13.2", "v0.13.3"]);
        assert_eq!(value_versions(&yaml("[0.13.2, nope]")).len(), 1);
        assert_eq!(normalize_value("affects", &yaml("v0.13.3")).unwrap(), yaml("'0.13.3'"));
        assert_eq!(normalize_value("affects", &yaml("0.13")).unwrap(), yaml("'0.13'"));
        assert_eq!(normalize_value("affects", &yaml("'0.13.2, v0.13.3'")).unwrap(), yaml("['0.13.2', '0.13.3']"));
        assert_eq!(normalize_value("affects", &yaml("[1.0]")).unwrap(), yaml("['1.0']"));
        assert_eq!(normalize_value("affects", &yaml("''")).unwrap(), yaml("''"));
        let error = normalize_value("affects", &yaml("soon")).unwrap_err();
        assert!(error.contains("'soon' is not a version"), "{error}");
    }
}
