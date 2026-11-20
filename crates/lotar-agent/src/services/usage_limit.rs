//! A provider's usage limit in a failed job's output (DEV-584): the line that
//! names it and, when the provider says, when the limit resets.
//!
//! harnessmux types a failed turn (DEV-711): `rate_limited` and `quota` are
//! limits, with the reset time the agent's error data gives. A failure it
//! gives no kind (`other`) is read from its text.
//!
//! A job that stops on such a limit pauses until the reset instead of failing.
//! Codex prints "You've hit your usage limit. Upgrade to Pro or try again at
//! Nov 20th, 2026 7:43 PM."; Claude prints "Claude API rate limit reached. Try
//! again later." or "5-hour limit reached ∙ resets 3pm (Europe/Berlin)".

use std::sync::LazyLock;

use harnessmux::{Failure, FailureKind};
use jiff::{SignedDuration, Timestamp, civil, tz::TimeZone};
use regex::Regex;

/// The wait when the provider names no reset time.
const DEFAULT_WAIT_SECONDS: i64 = 3600;
/// The shortest wait for a named reset time, so one already past (a clock
/// off, a time zone misread) doesn't spin the queue.
const MIN_WAIT_SECONDS: i64 = 60;

#[expect(clippy::expect_used, reason = "a literal pattern, covered by the tests")]
static LIMIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)usage limit|rate limit|hit your limit|-hour limit|weekly limit|quota exceeded|exceeded your current quota|insufficient_quota|too many requests|try again at",
    )
    .expect("limit pattern")
});

/// A usage limit found in a job's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageLimit {
    /// The line that names the limit, trimmed.
    pub line: String,
    /// When the provider says the limit resets, if it says.
    pub resets_at: Option<Timestamp>,
}

impl UsageLimit {
    /// When the job may run again: the reset time, at least a minute from
    /// `now`, or the default wait when no time was named.
    pub fn resume_at(&self, now: Timestamp) -> Timestamp {
        match self.resets_at {
            Some(at) => at.max(now.saturating_add(SignedDuration::from_secs(MIN_WAIT_SECONDS)).unwrap_or(Timestamp::MAX)),
            None => now.saturating_add(default_wait()).unwrap_or(Timestamp::MAX),
        }
    }
}

/// `LOTAR_AGENT_USAGE_LIMIT_WAIT_SECONDS`, else an hour.
fn default_wait() -> SignedDuration {
    let seconds = std::env::var("LOTAR_AGENT_USAGE_LIMIT_WAIT_SECONDS").ok().and_then(|value| value.trim().parse::<i64>().ok()).unwrap_or(DEFAULT_WAIT_SECONDS);
    SignedDuration::from_secs(seconds.max(0))
}

/// The first usage limit among the runner's failed turns, a time in their
/// text read in the system time zone.
pub fn of_failures(failures: &[Failure], now: Timestamp) -> Option<UsageLimit> {
    of_failures_in(failures, now, &TimeZone::system())
}

/// A failure typed as a limit is one, with its `resets_at`, else the time its
/// text names; one of no kind is a limit when its text names one; any other
/// kind (sign-in, overload, context, transport) is none.
fn of_failures_in(failures: &[Failure], now: Timestamp, zone: &TimeZone) -> Option<UsageLimit> {
    failures.iter().find_map(|failure| {
        let from_text = find_in([failure.error.as_str()], now, zone);
        match failure.kind {
            FailureKind::RateLimited | FailureKind::Quota => {
                let line = from_text.as_ref().map_or_else(|| first_line(&failure.error), |limit| limit.line.clone());
                let resets_at = failure.resets_at.as_deref().and_then(|at| at.parse::<Timestamp>().ok()).or(from_text.and_then(|limit| limit.resets_at));
                Some(UsageLimit { line, resets_at })
            }
            FailureKind::Other => from_text,
            _ => None,
        }
    })
}

/// The first non-empty line of `text`, shortened.
fn first_line(text: &str) -> String {
    shorten(text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default())
}

fn find_in<'a>(texts: impl IntoIterator<Item = &'a str>, now: Timestamp, zone: &TimeZone) -> Option<UsageLimit> {
    texts
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .find(|line| LIMIT.is_match(line))
        .map(|line| UsageLimit { line: shorten(line), resets_at: reset_time(line, now, zone) })
}

fn shorten(line: &str) -> String {
    const MAX: usize = 300;
    if line.chars().count() <= MAX {
        return line.to_string();
    }
    let cut: String = line.chars().take(MAX).collect();
    format!("{cut}…")
}

#[expect(clippy::expect_used, reason = "literal patterns, covered by the tests")]
static RFC3339: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})").expect("rfc3339 pattern"));
#[expect(clippy::expect_used, reason = "literal patterns, covered by the tests")]
static EPOCH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|(\d{10})\b").expect("epoch pattern"));
#[expect(clippy::expect_used, reason = "literal patterns, covered by the tests")]
static TRY_AT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)try again at ([A-Za-z]{3})[a-z]* (\d{1,2})(?:st|nd|rd|th)?,? (\d{4}),? (\d{1,2}):(\d{2}) ?([AP]M)").expect("try-again-at pattern")
});
#[expect(clippy::expect_used, reason = "literal patterns, covered by the tests")]
static RESETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)resets (?:at )?(\d{1,2})(?::(\d{2}))? ?([ap]m)(?: \(([A-Za-z_]+/[A-Za-z_/]+)\))?").expect("resets pattern"));
#[expect(clippy::expect_used, reason = "literal patterns, covered by the tests")]
static TRY_IN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:try again|retry|resets) in (\d+) ?(s|sec|second|m|min|minute|h|hr|hour)s?\b").expect("try-again-in pattern"));

/// The reset time a limit line names, if any.
fn reset_time(line: &str, now: Timestamp, zone: &TimeZone) -> Option<Timestamp> {
    if let Some(found) = RFC3339.find(line) {
        return found.as_str().parse::<Timestamp>().ok();
    }
    if let Some(caps) = EPOCH.captures(line) {
        return caps[1].parse::<i64>().ok().and_then(|seconds| Timestamp::from_second(seconds).ok());
    }
    if let Some(caps) = TRY_AT.captures(line) {
        let text = format!("{} {} {} {}:{} {}", &caps[1], &caps[2], &caps[3], &caps[4], &caps[5], caps[6].to_uppercase());
        let at = civil::DateTime::strptime("%b %d %Y %I:%M %p", &text).ok()?;
        return zone.to_timestamp(at).ok();
    }
    if let Some(caps) = RESETS.captures(line) {
        let zone = match caps.get(4) {
            Some(name) => TimeZone::get(name.as_str()).ok()?,
            None => zone.clone(),
        };
        let hour: i8 = caps[1].parse().ok()?;
        let minute: i8 = caps.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
        let hour = match (hour % 12, caps[3].eq_ignore_ascii_case("pm")) {
            (h, true) => h + 12,
            (h, false) => h,
        };
        let at = zone.to_datetime(now).date().at(hour, minute, 0, 0);
        let reset = zone.to_timestamp(at).ok()?;
        // A time already past today means tomorrow.
        return if reset <= now { zone.to_timestamp(at.tomorrow().ok()?).ok() } else { Some(reset) };
    }
    if let Some(caps) = TRY_IN.captures(line) {
        let count: i64 = caps[1].parse().ok()?;
        let unit = caps[2].to_ascii_lowercase();
        let seconds = match unit.chars().next()? {
            'h' => count.saturating_mul(3600),
            'm' => count.saturating_mul(60),
            _ => count,
        };
        return now.saturating_add(SignedDuration::from_secs(seconds)).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-11-18T10:00:00Z".parse().expect("timestamp")
    }

    fn berlin() -> TimeZone {
        TimeZone::get("Europe/Berlin").expect("zone")
    }

    #[test]
    fn codex_message_names_its_reset_in_local_time() {
        let text = "t3 exited with exit status: 1: You've hit your usage limit. Upgrade to Pro or try again at Nov 20th, 2026 7:43 PM.";
        let limit = find_in([text], now(), &berlin()).expect("limit");
        assert_eq!(limit.line, text);
        assert_eq!(limit.resets_at, Some("2026-11-20T18:43:00Z".parse().expect("timestamp")));
        assert_eq!(limit.resume_at(now()), "2026-11-20T18:43:00Z".parse().expect("timestamp"));
    }

    #[test]
    fn claude_rate_limit_without_a_time_waits_the_default() {
        let limit = find_in(["working on it", "Claude API rate limit reached. Try again later."], now(), &berlin()).expect("limit");
        assert_eq!(limit.line, "Claude API rate limit reached. Try again later.");
        assert_eq!(limit.resets_at, None);
        assert!(limit.resume_at(now()) > now());
    }

    #[test]
    fn claude_resets_hour_in_its_zone_rolls_to_tomorrow_when_past() {
        let limit = find_in(["5-hour limit reached ∙ resets 3pm (Europe/Berlin)"], now(), &TimeZone::UTC).expect("limit");
        assert_eq!(limit.resets_at, Some("2026-11-18T14:00:00Z".parse().expect("timestamp")));
        let limit = find_in(["5-hour limit reached ∙ resets 9am (Europe/Berlin)"], now(), &TimeZone::UTC).expect("limit");
        assert_eq!(limit.resets_at, Some("2026-11-19T08:00:00Z".parse().expect("timestamp")));
    }

    #[test]
    fn rfc3339_epoch_and_relative_times() {
        let at = |text: &str| find_in([text], now(), &berlin()).and_then(|limit| limit.resets_at);
        assert_eq!(at("usage limit reached, resets 2026-11-18T12:30:00Z"), Some("2026-11-18T12:30:00Z".parse().expect("timestamp")));
        assert_eq!(at("Claude AI usage limit reached|1795089600"), Timestamp::from_second(1_795_089_600).ok());
        assert_eq!(at("429 Too Many Requests: try again in 20 minutes"), Some("2026-11-18T10:20:00Z".parse().expect("timestamp")));
    }

    #[test]
    fn a_past_reset_waits_at_least_a_minute() {
        let limit = find_in(["usage limit, resets 2026-11-18T09:00:00Z"], now(), &berlin()).expect("limit");
        assert_eq!(limit.resume_at(now()), "2026-11-18T10:01:00Z".parse().expect("timestamp"));
    }

    fn failure(error: &str, kind: FailureKind, resets_at: Option<&str>) -> Failure {
        Failure { resets_at: resets_at.map(str::to_string), ..Failure::new(error, kind) }
    }

    #[test]
    fn a_typed_limit_waits_for_its_resets_at_over_the_text() {
        let failures = [
            failure("You're out of extra usage", FailureKind::RateLimited, Some("2026-11-18T15:00:00Z")),
            failure("usage limit, resets 2026-11-18T12:00:00Z", FailureKind::Other, None),
        ];
        let limit = of_failures_in(&failures, now(), &berlin()).expect("limit");
        assert_eq!(limit, UsageLimit { line: "You're out of extra usage".into(), resets_at: Some("2026-11-18T15:00:00Z".parse().expect("timestamp")) });
    }

    #[test]
    fn a_typed_limit_without_resets_at_reads_its_texts_time() {
        let text = "You've hit your usage limit. Upgrade to Pro or try again at Nov 20th, 2026 7:43 PM.";
        let limit = of_failures_in(&[failure(text, FailureKind::RateLimited, None)], now(), &berlin()).expect("limit");
        assert_eq!(limit.resets_at, Some("2026-11-20T18:43:00Z".parse().expect("timestamp")));
        let limit = of_failures_in(&[failure("Out of credits\nbuy more", FailureKind::Quota, None)], now(), &berlin()).expect("limit");
        assert_eq!(limit, UsageLimit { line: "Out of credits".into(), resets_at: None });
    }

    #[test]
    fn untyped_failures_fall_back_to_the_text_and_other_kinds_are_no_limit() {
        let untyped = of_failures_in(&[failure("Claude API rate limit reached. Try again later.", FailureKind::Other, None)], now(), &berlin());
        assert_eq!(untyped.map(|limit| limit.line).as_deref(), Some("Claude API rate limit reached. Try again later."));
        let overloaded = failure("Codex exceeded retry limit: try again at Nov 20th, 2026 7:43 PM", FailureKind::Overloaded, None);
        assert_eq!(of_failures_in(&[overloaded, failure("boom", FailureKind::Other, None)], now(), &berlin()), None);
    }

    #[test]
    fn ordinary_failures_are_no_limit() {
        assert_eq!(find_in(["error: tests failed", "exit status 1"], now(), &berlin()), None);
    }
}
