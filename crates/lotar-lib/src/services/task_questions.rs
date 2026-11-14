//! Question and answer comments (DEV-488).
//!
//! A question comment carries options; an answer comment points to the
//! question by its `date` and records the chosen keys. Answering applies the
//! chosen options' `set:` like an automation `set:`, as an update by the
//! answerer. Options never run commands or start agents directly: their text
//! comes from whoever asked.

use crate::api_types::TaskDTO;
use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::automation_service::option_set_patch;
use crate::services::task_service::{CommentContext, TaskService, TaskUpdateContext};
use crate::services::task_validation as validation;
use crate::storage::manager::Storage;
use crate::types::{CommentAnswer, CommentQuestion, CommentReport, OptionSet, TaskComment};
use serde::Deserialize;

/// The fields an option's `set:` accepts.
pub const OPTION_SET_FIELDS: &[&str] = &["status", "assignee", "reporter", "priority", "type", "due_date", "effort", "tags"];

/// An answer as a caller gives it: `to` defaults to the last open question.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnswerInput {
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default, deserialize_with = "crate::api_types::deserialize_string_or_vec")]
    pub choice: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// A comment as REST and MCP take it: `text`, plus `question` to ask,
/// `answer` to answer (`text` is then the note) or `review` to report a
/// review (`text` is then the summary, DEV-489).
#[derive(Debug, Clone)]
pub enum CommentRequest {
    Plain { text: String, report: Option<CommentReport> },
    Question { prompt: String, question: CommentQuestion },
    Answer(AnswerInput),
    Review { summary: Option<String>, review: crate::types::CommentReview },
}

impl CommentRequest {
    /// Read `text`, `question`, `answer` and `review` from a request body.
    pub fn from_json(body: &serde_json::Value) -> Result<Self, String> {
        let text = body.get("text").and_then(serde_json::Value::as_str).map(str::trim).filter(|text| !text.is_empty()).map(str::to_string);
        let present = |key: &str| body.get(key).is_some_and(|value| !value.is_null());
        if present("report") {
            if present("question") || present("answer") || present("review") {
                return Err("a report goes with a plain comment, not with a question, answer or review".to_string());
            }
            let report: CommentReport = serde_json::from_value(body["report"].clone()).map_err(|err| format!("Invalid report: {err}"))?;
            let text = text.ok_or("Missing text: the report's prose summary")?;
            return Ok(Self::Plain { text, report: Some(report) });
        }
        if present("review") {
            if present("question") || present("answer") || present("report") {
                return Err("a review comment cannot ask or answer".to_string());
            }
            let review = serde_json::from_value(body["review"].clone()).map_err(|err| format!("Invalid review: {err}"))?;
            return Ok(Self::Review { summary: text, review });
        }
        match (present("question"), present("answer")) {
            (true, true) => Err("a comment cannot ask and answer at once".to_string()),
            (true, false) => {
                let question: CommentQuestion = serde_json::from_value(body["question"].clone()).map_err(|err| format!("Invalid question: {err}"))?;
                let prompt = text.ok_or("Missing text: the question itself")?;
                Ok(Self::Question { prompt, question })
            }
            (false, true) => {
                let mut answer: AnswerInput = serde_json::from_value(body["answer"].clone()).map_err(|err| format!("Invalid answer: {err}"))?;
                if answer.note.is_none() {
                    answer.note = text;
                }
                Ok(Self::Answer(answer))
            }
            (false, false) => text.map(|text| Self::Plain { text, report: None }).ok_or_else(|| "Missing text".to_string()),
        }
    }
}

impl TaskService {
    /// Add a plain, question, answer or review comment.
    pub fn add_comment_request(storage: &mut Storage, id: &str, request: CommentRequest) -> LoTaRResult<TaskDTO> {
        match request {
            CommentRequest::Plain { text, report } => {
                Self::add_comment_with_context(storage, id, &text, CommentContext { report, ..CommentContext::default() })
            }
            CommentRequest::Question { prompt, question } => Self::ask_question(storage, id, &prompt, question),
            CommentRequest::Answer(answer) => Self::answer_question(storage, id, &answer),
            CommentRequest::Review { summary, review } => Self::add_review(storage, id, summary.as_deref(), review),
        }
    }

    /// Add a question comment: `prompt` is the question; the stored text
    /// adds the options in prose.
    pub fn ask_question(storage: &mut Storage, id: &str, prompt: &str, question: CommentQuestion) -> LoTaRResult<TaskDTO> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err(LoTaRError::ValidationError("the question needs its text".to_string()));
        }
        let project = crate::storage::TaskId::parse(id).map(|parsed| parsed.project).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let config =
            crate::config::resolution::config_for_project(&storage.root_path, Some(&project)).map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
        let question = validate_question(question, &config)?;
        let pushed = Self::push_comment(storage, id, CommentContext::default(), |_, now, actor| {
            let text = crate::services::task_service::strip_author_prefix(prompt, actor.as_deref())?;
            let mut comment = TaskComment::new(now, actor, question_text(text, &question));
            comment.question = Some(question);
            Ok(comment)
        })?;
        Ok(Self::finish_comment(storage, pushed, true))
    }

    /// Publish the answer and option fields together, then fire comment rules.
    pub fn answer_question(storage: &mut Storage, id: &str, input: &AnswerInput) -> LoTaRResult<TaskDTO> {
        let context = TaskUpdateContext { question_answer: Some(input.clone()), ..Default::default() };
        let receipt = Self::update_with_receipt(storage, id, Default::default(), context)?;
        let dto = receipt.dto;
        let comment = dto
            .comments
            .iter()
            .rev()
            .find(|comment| comment.answer.is_some())
            .cloned()
            .ok_or_else(|| LoTaRError::ValidationError("the answer was not stored".into()))?;
        let project = crate::storage::TaskId::parse(&dto.id).map_err(|err| LoTaRError::InvalidTaskId(err.to_string()))?.project;
        let config = Self::resolve_config_for_project(&storage.root_path, &project);
        Ok(Self::finish_comment(storage, super::task_service::PushedComment { dto, comment, config, project }, true))
    }
}

/// Called with the update transaction's locks held, against its current task.
pub(crate) fn prepare_answer(
    storage: &Storage,
    id: &str,
    task: &crate::Task,
    config: &ResolvedConfig,
    input: &AnswerInput,
) -> LoTaRResult<(TaskComment, crate::api_types::TaskUpdate)> {
    let question = find_question(&task.comments, input.to.as_deref())?;
    let spec = question.question.as_ref().ok_or_else(|| LoTaRError::ValidationError("that comment is not a question".into()))?;
    let answer = resolve_answer(question, spec, input)?;
    let patch = if let Some(set) = merged_set(spec, &answer.choice) {
        validate_option_set("answer", &set, config)?;
        let current = TaskService::to_dto(id, task.clone(), None, config, chrono::Utc::now());
        option_set_patch(storage, &current, config, &set)?
    } else {
        Default::default()
    };
    let project = crate::storage::TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(err.to_string()))?.project;
    let actor = crate::utils::identity::resolve_current_user_for_project(Some(&storage.root_path), Some(&project));
    let mut comment = TaskComment::new(chrono::Utc::now().to_rfc3339(), actor, answer_text(spec, &answer));
    comment.answer = Some(answer);
    Ok((comment, patch))
}

/// Check a question before it is asked and return it normalized: trimmed
/// keys and labels, no empty `set:`, and the recommended key spelled as its
/// option.
pub fn validate_question(mut question: CommentQuestion, config: &ResolvedConfig) -> LoTaRResult<CommentQuestion> {
    let invalid = |message: String| Err(LoTaRError::ValidationError(message));
    if let Some(key) = question.extra.keys().next() {
        return invalid(format!("unknown question field '{key}' (expected options, recommended, multiple, other)"));
    }
    if question.options.is_empty() {
        return invalid("a question needs at least one option".to_string());
    }
    let mut seen: Vec<String> = Vec::new();
    for option in &mut question.options {
        option.key = option.key.trim().to_string();
        option.label = option.label.trim().to_string();
        let key = option.key.clone();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return invalid(format!("option key '{key}' must be letters, digits, '-' or '_'"));
        }
        if seen.iter().any(|other| other.eq_ignore_ascii_case(&key)) {
            return invalid(format!("option key '{key}' is used twice"));
        }
        seen.push(key.clone());
        if option.label.is_empty() {
            return invalid(format!("option '{key}' needs a label"));
        }
        if let Some(field) = option.extra.keys().next() {
            return invalid(format!("unknown field '{field}' in option '{key}' (expected key, label, set)"));
        }
        if let Some(set) = option.set.as_ref() {
            validate_option_set(&key, set, config)?;
            if set.fields().is_empty() {
                option.set = None;
            }
        }
    }
    if let Some(recommended) = question.recommended.as_deref().map(str::trim) {
        match question.options.iter().find(|option| option.key.eq_ignore_ascii_case(recommended)) {
            Some(option) => question.recommended = Some(option.key.clone()),
            None => return invalid(format!("recommended '{recommended}' is not one of the options ({})", seen.join(", "))),
        }
    }
    Ok(question)
}

fn validate_option_set(key: &str, set: &OptionSet, config: &ResolvedConfig) -> LoTaRResult<()> {
    let invalid = |message: String| LoTaRError::ValidationError(format!("option '{key}': {message}"));
    let reason = |err: LoTaRError| match err {
        LoTaRError::ValidationError(message) => invalid(message),
        other => invalid(other.to_string()),
    };
    if let Some(field) = set.extra.keys().next() {
        return Err(invalid(format!("set: cannot change '{field}' (an option may set {})", OPTION_SET_FIELDS.join(", "))));
    }
    if let Some(status) = set.status.as_deref() {
        validation::parse_status(status, config).map_err(reason)?;
    }
    if let Some(priority) = set.priority.as_deref() {
        validation::parse_priority(priority, config).map_err(reason)?;
    }
    if let Some(task_type) = set.task_type.as_deref() {
        validation::parse_task_type(task_type, config).map_err(reason)?;
    }
    if let Some(effort) = set.effort.as_deref().filter(|effort| !effort.trim().is_empty()) {
        crate::utils::effort::parse_effort(effort).map_err(|err| invalid(format!("effort: {err}")))?;
    }
    Ok(())
}

/// The question comment `to` names (its `date`, or a unique start of it),
/// else the last open question.
fn find_question<'a>(comments: &'a [TaskComment], to: Option<&str>) -> LoTaRResult<&'a TaskComment> {
    let Some(to) = to.map(str::trim).filter(|to| !to.is_empty()) else {
        return crate::types::open_questions(comments)
            .last()
            .map(|&index| &comments[index])
            .ok_or_else(|| LoTaRError::ValidationError("the ticket has no open question".to_string()));
    };
    let found = match comments.iter().find(|comment| comment.question.is_some() && comment.date == to) {
        Some(exact) => exact,
        None => {
            let mut matches = comments.iter().filter(|comment| comment.question.is_some() && comment.date.starts_with(to));
            match (matches.next(), matches.next()) {
                (Some(question), None) => question,
                (Some(_), Some(_)) => return Err(LoTaRError::ValidationError(format!("several questions were asked at '{to}'; give the full date"))),
                (None, _) => return Err(LoTaRError::ValidationError(format!("no question was asked at '{to}'"))),
            }
        }
    };
    if comments.iter().any(|comment| comment.answer.as_ref().is_some_and(|answer| answer.to == found.date)) {
        return Err(LoTaRError::ValidationError(format!("the question asked at {} is answered already", found.date)));
    }
    Ok(found)
}

fn resolve_answer(question: &TaskComment, spec: &CommentQuestion, input: &AnswerInput) -> LoTaRResult<CommentAnswer> {
    let invalid = |message: String| Err(LoTaRError::ValidationError(message));
    let keys: Vec<&str> = spec.options.iter().map(|option| option.key.as_str()).collect();
    let mut choice: Vec<String> = Vec::new();
    for raw in input.choice.iter().flat_map(|value| value.split(',')).map(str::trim).filter(|value| !value.is_empty()) {
        let Some(key) = keys.iter().find(|key| key.eq_ignore_ascii_case(raw)) else {
            return invalid(format!("'{raw}' is not an option of this question ({})", keys.join(", ")));
        };
        if !choice.iter().any(|chosen| chosen == key) {
            choice.push((*key).to_string());
        }
    }
    let note = input.note.as_deref().map(str::trim).filter(|note| !note.is_empty()).map(str::to_string);
    if choice.len() > 1 && !spec.multiple {
        return invalid("this question takes one option".to_string());
    }
    if choice.is_empty() {
        if note.is_none() {
            return invalid(format!("choose one of {} or answer in words", keys.join(", ")));
        }
        if !spec.other {
            return invalid(format!("this question takes one of its options ({})", keys.join(", ")));
        }
    }
    Ok(CommentAnswer { to: question.date.clone(), choice, note, extra: Default::default() })
}

/// The chosen options' `set:` blocks merged in the order of the question's
/// options; a later option's field wins.
fn merged_set(spec: &CommentQuestion, choice: &[String]) -> Option<OptionSet> {
    let mut merged: Option<OptionSet> = None;
    for option in spec.options.iter().filter(|option| choice.contains(&option.key)) {
        let Some(set) = option.set.as_ref() else {
            continue;
        };
        let target = merged.get_or_insert_with(OptionSet::default);
        let overlay = set.clone();
        target.status = overlay.status.or(target.status.take());
        target.assignee = overlay.assignee.or(target.assignee.take());
        target.reporter = overlay.reporter.or(target.reporter.take());
        target.priority = overlay.priority.or(target.priority.take());
        target.task_type = overlay.task_type.or(target.task_type.take());
        target.due_date = overlay.due_date.or(target.due_date.take());
        target.effort = overlay.effort.or(target.effort.take());
        target.tags = overlay.tags.or(target.tags.take());
    }
    merged
}

/// The question in prose: the prompt, then one line per option, so readers
/// that do not know question comments see everything.
pub fn question_text(prompt: &str, question: &CommentQuestion) -> String {
    let mut text = format!("{}\n\nOptions:", prompt.trim_end());
    for option in &question.options {
        let recommended = if question.recommended.as_deref() == Some(option.key.as_str()) { " (recommended)" } else { "" };
        text.push_str(&format!("\n- {}: {}{recommended}", option.key, option.label));
    }
    if question.multiple {
        text.push_str("\n\nSeveral options may be chosen.");
    }
    text
}

/// The answer in prose: the chosen options with their labels, then the
/// answerer's note.
pub fn answer_text(question: &CommentQuestion, answer: &CommentAnswer) -> String {
    let chosen: Vec<String> = answer
        .choice
        .iter()
        .map(|key| match question.options.iter().find(|option| &option.key == key) {
            Some(option) => format!("{key} ({})", option.label),
            None => key.clone(),
        })
        .collect();
    match (chosen.is_empty(), answer.note.as_deref()) {
        (false, Some(note)) => format!("Answer: {}\n\n{note}", chosen.join(", ")),
        (false, None) => format!("Answer: {}", chosen.join(", ")),
        (true, note) => note.unwrap_or_default().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::QuestionOption;

    fn option(key: &str, label: &str) -> QuestionOption {
        QuestionOption { key: key.into(), label: label.into(), set: None, extra: Default::default() }
    }

    fn question(options: Vec<QuestionOption>) -> CommentQuestion {
        CommentQuestion { options, recommended: None, multiple: false, other: true, extra: Default::default() }
    }

    fn config() -> ResolvedConfig {
        ResolvedConfig::from_global(crate::config::types::GlobalConfig::default())
    }

    #[test]
    fn question_text_lists_the_options() {
        let mut spec = question(vec![option("a", "Keep it"), option("b", "Drop it")]);
        spec.recommended = Some("b".into());
        assert_eq!(question_text("Which one?", &spec), "Which one?\n\nOptions:\n- a: Keep it\n- b: Drop it (recommended)");
    }

    #[test]
    fn validation_refuses_unknown_set_fields_and_bad_values() {
        let mut bad = option("a", "Run it");
        let mut set = OptionSet::default();
        set.extra.insert("run".into(), serde_yaml_ng::Value::String("rm -rf /".into()));
        bad.set = Some(set);
        let err = validate_question(question(vec![bad]), &config()).unwrap_err().to_string();
        assert!(err.contains("cannot change 'run'"), "{err}");

        let mut bad_status = option("a", "Close");
        bad_status.set = Some(OptionSet { status: Some("Nope".into()), ..OptionSet::default() });
        assert!(validate_question(question(vec![bad_status]), &config()).is_err());

        let mut spec = question(vec![option("a", "A"), option("A", "B")]);
        assert!(validate_question(spec.clone(), &config()).unwrap_err().to_string().contains("twice"));
        spec.options.pop();
        spec.recommended = Some("x".into());
        assert!(validate_question(spec, &config()).is_err());
        assert!(validate_question(question(Vec::new()), &config()).is_err());
    }

    #[test]
    fn answers_match_keys_and_respect_multiple_and_other() {
        let mut spec = question(vec![option("yes", "Yes"), option("no", "No")]);
        let comment = TaskComment::new("2026-11-13T10:00:00Z".into(), None, "q".into());
        let answer = |choice: &[&str], note: Option<&str>| AnswerInput {
            to: None,
            choice: choice.iter().map(|c| c.to_string()).collect(),
            note: note.map(str::to_string),
        };

        let yes = resolve_answer(&comment, &spec, &answer(&["YES"], None)).unwrap();
        assert_eq!(yes.choice, vec!["yes"]);
        assert_eq!(yes.to, "2026-11-13T10:00:00Z");
        assert_eq!(answer_text(&spec, &yes), "Answer: yes (Yes)");
        assert!(resolve_answer(&comment, &spec, &answer(&["yes", "no"], None)).is_err());
        assert!(resolve_answer(&comment, &spec, &answer(&["maybe"], None)).is_err());
        assert!(resolve_answer(&comment, &spec, &answer(&[], None)).is_err());
        let other = resolve_answer(&comment, &spec, &answer(&[], Some("Ask me next week"))).unwrap();
        assert_eq!(answer_text(&spec, &other), "Ask me next week");

        spec.multiple = true;
        assert_eq!(resolve_answer(&comment, &spec, &answer(&["no,yes"], None)).unwrap().choice, vec!["no", "yes"]);
        spec.other = false;
        assert!(resolve_answer(&comment, &spec, &answer(&[], Some("words"))).is_err());
    }

    #[test]
    fn the_last_open_question_is_the_default_target() {
        let mut first = TaskComment::new("2026-11-13T10:00:00Z".into(), None, "first".into());
        first.question = Some(question(vec![option("a", "A")]));
        let mut second = first.clone();
        second.date = "2026-11-13T11:00:00Z".into();
        let mut answer = TaskComment::new("2026-11-13T12:00:00Z".into(), None, "Answer: a".into());
        answer.answer = Some(CommentAnswer { to: second.date.clone(), choice: vec!["a".into()], note: None, extra: Default::default() });
        let comments = vec![first, second, answer];
        assert_eq!(crate::types::open_questions(&comments), vec![0]);
        assert_eq!(find_question(&comments, None).unwrap().date, "2026-11-13T10:00:00Z");
        assert_eq!(find_question(&comments, Some("2026-11-13T10")).unwrap().date, "2026-11-13T10:00:00Z");
        assert!(find_question(&comments, Some("2026-11-13T1")).is_err());
        assert!(find_question(&comments, Some("2026-11-13T11:00:00Z")).unwrap_err().to_string().contains("answered already"));
    }
}
