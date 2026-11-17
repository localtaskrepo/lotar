//! Review comments (DEV-489).
//!
//! A review comment records a reviewer's verdict (`pass` or `block`), the
//! commit reviewed and the findings. `text` keeps the summary with the
//! verdict and findings in prose, so readers that do not know review
//! comments see everything.

use crate::api_types::TaskDTO;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::task_service::{CommentContext, TaskService};
use crate::storage::manager::Storage;
use crate::types::{CommentReview, REVIEW_VERDICTS, ReviewFinding, TaskComment};

impl TaskService {
    /// Add a review comment; `summary` is the reviewer's prose, if any.
    pub fn add_review(storage: &mut Storage, id: &str, summary: Option<&str>, review: CommentReview) -> LoTaRResult<TaskDTO> {
        let review = validate_review(review)?;
        let summary = summary.map(str::trim).filter(|summary| !summary.is_empty());
        let pushed = Self::push_comment(storage, id, CommentContext::default(), |_, now, actor| {
            let summary = summary.map(|summary| crate::services::task_service::strip_author_prefix(summary, actor.as_deref())).transpose()?;
            let mut comment = TaskComment::new(now, actor, review_text(summary, &review));
            comment.review = Some(review);
            Ok(comment)
        })?;
        Ok(Self::finish_comment(storage, pushed, true))
    }
}

/// Normalize the verdict and refuse what a review cannot say.
pub fn validate_review(mut review: CommentReview) -> LoTaRResult<CommentReview> {
    let invalid = |message: String| Err(LoTaRError::ValidationError(message));
    review.verdict = review.verdict.trim().to_ascii_lowercase();
    if !REVIEW_VERDICTS.contains(&review.verdict.as_str()) {
        return invalid(format!("unknown review verdict '{}' (verdicts: {})", review.verdict, REVIEW_VERDICTS.join(", ")));
    }
    review.commit = review.commit.map(|commit| commit.trim().to_string()).filter(|commit| !commit.is_empty());
    if let Some(commit) = review.commit.as_deref()
        && commit.chars().any(char::is_whitespace)
    {
        return invalid(format!("review commit '{commit}' is not a commit"));
    }
    for finding in &mut review.findings {
        finding.summary = finding.summary.trim().to_string();
        finding.file = finding.file.take().map(|file| file.trim().to_string()).filter(|file| !file.is_empty());
        if finding.summary.is_empty() {
            return invalid("a review finding needs its summary".to_string());
        }
        if finding.line == Some(0) {
            return invalid(format!("finding '{}': lines start at 1", finding.summary));
        }
        if finding.line.is_some() && finding.file.is_none() {
            return invalid(format!("finding '{}': a line needs its file", finding.summary));
        }
    }
    if review.verdict == "pass"
        && let Some(blocking) = review.findings.iter().find(|finding| finding.blocking)
    {
        return invalid(format!("a passing review cannot have the blocking finding '{}'", blocking.summary));
    }
    Ok(review)
}

/// A finding as the CLI takes it: `[<file>[:<line>]: ]<summary>`. The part
/// before the first ": " is a location when it has no spaces and names a
/// path (it has a '/' or '.').
pub fn parse_finding(raw: &str, blocking: bool) -> ReviewFinding {
    let raw = raw.trim();
    let (location, summary) = match raw.split_once(": ") {
        Some((head, rest)) if !head.contains(char::is_whitespace) && (head.contains('/') || head.contains('.')) => (Some(head), rest),
        _ => (None, raw),
    };
    let (file, line) = match location {
        Some(location) => match location.rsplit_once(':') {
            Some((file, line)) if line.parse::<u32>().is_ok() => (Some(file.to_string()), line.parse().ok()),
            _ => (Some(location.to_string()), None),
        },
        None => (None, None),
    };
    ReviewFinding { summary: summary.trim().to_string(), file, line, blocking, extra: Default::default() }
}

/// The review in prose: the verdict and commit, the summary, then one line
/// per finding, blocking ones first.
pub fn review_text(summary: Option<&str>, review: &CommentReview) -> String {
    let verdict = if review.verdict == "pass" { "passed" } else { "blocked" };
    let mut text = match review.commit.as_deref() {
        Some(commit) => format!("Review {verdict} at {commit}."),
        None => format!("Review {verdict}."),
    };
    if let Some(summary) = summary {
        text.push_str(&format!("\n\n{}", summary.trim_end()));
    }
    if !review.findings.is_empty() {
        text.push_str("\n\nFindings:");
        let (blocking, other): (Vec<_>, Vec<_>) = review.findings.iter().partition(|finding| finding.blocking);
        for finding in blocking.into_iter().chain(other) {
            let mark = if finding.blocking { "[blocking] " } else { "" };
            let location = match (finding.file.as_deref(), finding.line) {
                (Some(file), Some(line)) => format!("{file}:{line}: "),
                (Some(file), None) => format!("{file}: "),
                _ => String::new(),
            };
            text.push_str(&format!("\n- {mark}{location}{}", finding.summary));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(verdict: &str, findings: Vec<ReviewFinding>) -> CommentReview {
        CommentReview { verdict: verdict.into(), commit: Some("abc1234".into()), findings, extra: Default::default() }
    }

    #[test]
    fn parse_finding_reads_file_and_line() {
        let finding = parse_finding("src/main.rs:42: unwrap on user input", true);
        assert_eq!(
            (finding.file.as_deref(), finding.line, finding.summary.as_str(), finding.blocking),
            (Some("src/main.rs"), Some(42), "unwrap on user input", true)
        );
        let finding = parse_finding("README.md: a typo", false);
        assert_eq!((finding.file.as_deref(), finding.line), (Some("README.md"), None));
        let finding = parse_finding("Note: the tests are slow", false);
        assert_eq!((finding.file, finding.summary.as_str()), (None, "Note: the tests are slow"));
    }

    #[test]
    fn validation_normalizes_and_refuses() {
        let ok = validate_review(review(" Block ", vec![parse_finding("x.rs:3: bad", true)])).expect("valid");
        assert_eq!(ok.verdict, "block");
        assert!(validate_review(review("maybe", Vec::new())).is_err());
        assert!(validate_review(review("pass", vec![parse_finding("bad", true)])).unwrap_err().to_string().contains("passing review"));
        assert!(validate_review(review("pass", vec![parse_finding(" ", false)])).is_err());
        let mut no_file = parse_finding("bad", false);
        no_file.line = Some(3);
        assert!(validate_review(review("pass", vec![no_file])).is_err());
    }

    #[test]
    fn review_text_lists_blocking_findings_first() {
        let spec = review("block", vec![parse_finding("a nit", false), parse_finding("src/a.rs:7: data loss", true)]);
        assert_eq!(
            review_text(Some("Two problems."), &spec),
            "Review blocked at abc1234.\n\nTwo problems.\n\nFindings:\n- [blocking] src/a.rs:7: data loss\n- a nit"
        );
        assert_eq!(review_text(None, &review("pass", Vec::new())), "Review passed at abc1234.");
    }
}
