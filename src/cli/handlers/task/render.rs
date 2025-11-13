use crate::output::{OutputFormat, OutputRenderer};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplainPlacement {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLevel {
    Success,
    Info,
    Warning,
}

impl TextLevel {
    pub fn emit(self, renderer: &OutputRenderer, message: &str) {
        match self {
            TextLevel::Success => renderer.emit_success(message),
            TextLevel::Info => renderer.emit_info(message),
            TextLevel::Warning => renderer.emit_warning(message),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PropertyExplain {
    pub message: String,
    pub placement: ExplainPlacement,
    pub level: TextLevel,
}

impl PropertyExplain {
    pub fn new(message: impl Into<String>, placement: ExplainPlacement, level: TextLevel) -> Self {
        Self {
            message: message.into(),
            placement,
            level,
        }
    }

    pub fn info(message: impl Into<String>, placement: ExplainPlacement) -> Self {
        Self::new(message, placement, TextLevel::Info)
    }
}

#[derive(Clone, Debug)]
pub struct PropertyCurrent {
    pub task_id: String,
    pub field: String,
    pub value: Option<String>,
    pub text_message: String,
    pub text_level: TextLevel,
    pub explain: Option<PropertyExplain>,
}

impl PropertyCurrent {
    pub fn new(
        task_id: impl Into<String>,
        field: impl Into<String>,
        value: Option<String>,
        text_message: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            field: field.into(),
            value,
            text_message: text_message.into(),
            text_level: TextLevel::Success,
            explain: None,
        }
    }

    pub fn with_explain(mut self, explain: PropertyExplain) -> Self {
        self.explain = Some(explain);
        self
    }
}

#[derive(Clone, Debug)]
pub struct PropertyNoop {
    pub task_id: String,
    pub field: String,
    pub value: Option<String>,
    pub json_message: String,
    pub text_message: String,
    pub text_level: TextLevel,
    pub extra_json: Vec<(String, Value)>,
    pub notice: Option<String>,
}

impl PropertyNoop {
    pub fn new(
        task_id: impl Into<String>,
        field: impl Into<String>,
        value: Option<String>,
        json_message: impl Into<String>,
        text_message: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            field: field.into(),
            value,
            json_message: json_message.into(),
            text_message: text_message.into(),
            text_level: TextLevel::Warning,
            extra_json: Vec::new(),
            notice: None,
        }
    }

    pub fn with_extra_json(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extra_json.push((key.into(), value));
        self
    }

    pub fn with_notice(mut self, notice: impl Into<String>) -> Self {
        self.notice = Some(notice.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct PropertyPreview {
    pub task_id: String,
    pub action: String,
    pub old_field: String,
    pub new_field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub text_message: String,
    pub text_level: TextLevel,
    pub json_message: Option<String>,
    pub explain: Option<PropertyExplain>,
    pub extra_json: Vec<(String, Value)>,
}

impl PropertyPreview {
    pub fn new(
        task_id: impl Into<String>,
        action: impl Into<String>,
        old_field: impl Into<String>,
        new_field: impl Into<String>,
        old_value: Option<String>,
        new_value: Option<String>,
        text_message: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            action: action.into(),
            old_field: old_field.into(),
            new_field: new_field.into(),
            old_value,
            new_value,
            text_message: text_message.into(),
            text_level: TextLevel::Info,
            json_message: None,
            explain: None,
            extra_json: Vec::new(),
        }
    }

    pub fn with_json_message(mut self, message: impl Into<String>) -> Self {
        self.json_message = Some(message.into());
        self
    }

    pub fn with_explain(mut self, explain: PropertyExplain) -> Self {
        self.explain = Some(explain);
        self
    }

    pub fn with_extra_json(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extra_json.push((key.into(), value));
        self
    }
}

#[derive(Clone, Debug)]
pub struct PropertySuccess {
    pub task_id: String,
    pub json_message: String,
    pub text_message: String,
    pub text_level: TextLevel,
    pub old_field: String,
    pub new_field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub extra_json: Vec<(String, Value)>,
}

impl PropertySuccess {
    pub fn new(
        task_id: impl Into<String>,
        old_field: impl Into<String>,
        new_field: impl Into<String>,
        old_value: Option<String>,
        new_value: Option<String>,
        json_message: impl Into<String>,
        text_message: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            json_message: json_message.into(),
            text_message: text_message.into(),
            text_level: TextLevel::Success,
            old_field: old_field.into(),
            new_field: new_field.into(),
            old_value,
            new_value,
            extra_json: Vec::new(),
        }
    }

    pub fn with_extra_json(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extra_json.push((key.into(), value));
        self
    }
}

pub fn render_property_current(renderer: &OutputRenderer, payload: PropertyCurrent) {
    let PropertyCurrent {
        task_id,
        field,
        value,
        text_message,
        text_level,
        explain,
    } = payload;

    match renderer.format {
        OutputFormat::Json => {
            let mut root = Map::new();
            root.insert("status".to_string(), Value::String("success".to_string()));
            root.insert("task_id".to_string(), Value::String(task_id));
            root.insert(field, option_to_value(value));
            if let Some(explain_ref) = explain.as_ref() {
                root.insert(
                    "explain".to_string(),
                    Value::String(explain_ref.message.clone()),
                );
            }
            let payload = Value::Object(root);
            renderer.emit_json(&payload);
        }
        _ => {
            if let Some(explain_ref) = explain.as_ref() {
                if explain_ref.placement == ExplainPlacement::Before {
                    explain_ref.level.emit(renderer, &explain_ref.message);
                }
            }
            text_level.emit(renderer, &text_message);
            if let Some(explain_ref) = explain.as_ref() {
                if explain_ref.placement == ExplainPlacement::After {
                    explain_ref.level.emit(renderer, &explain_ref.message);
                }
            }
        }
    }
}

pub fn render_property_noop(renderer: &OutputRenderer, payload: PropertyNoop) {
    let PropertyNoop {
        task_id,
        field,
        value,
        json_message,
        text_message,
        text_level,
        extra_json,
        notice,
    } = payload;

    match renderer.format {
        OutputFormat::Json => {
            let mut root = Map::new();
            root.insert("status".to_string(), Value::String("success".to_string()));
            root.insert("message".to_string(), Value::String(json_message));
            root.insert("task_id".to_string(), Value::String(task_id));
            root.insert(field, option_to_value(value));
            for (key, value) in extra_json {
                root.insert(key, value);
            }
            if let Some(notice_value) = notice.as_ref() {
                root.insert("notice".to_string(), Value::String(notice_value.clone()));
            }
            let payload = Value::Object(root);
            renderer.emit_json(&payload);
        }
        _ => {
            text_level.emit(renderer, &text_message);
            if let Some(notice_value) = notice.as_ref() {
                renderer.emit_notice(notice_value);
            }
        }
    }
}

pub fn render_property_preview(renderer: &OutputRenderer, payload: PropertyPreview) {
    let PropertyPreview {
        task_id,
        action,
        old_field,
        new_field,
        old_value,
        new_value,
        text_message,
        text_level,
        json_message,
        explain,
        extra_json,
    } = payload;

    match renderer.format {
        OutputFormat::Json => {
            let mut root = Map::new();
            root.insert("status".to_string(), Value::String("preview".to_string()));
            root.insert("action".to_string(), Value::String(action));
            root.insert("task_id".to_string(), Value::String(task_id));
            root.insert(old_field, option_to_value(old_value));
            root.insert(new_field, option_to_value(new_value));
            if let Some(message) = json_message {
                root.insert("message".to_string(), Value::String(message));
            }
            if let Some(explain_ref) = explain.as_ref() {
                root.insert(
                    "explain".to_string(),
                    Value::String(explain_ref.message.clone()),
                );
            }
            for (key, value) in extra_json {
                root.insert(key, value);
            }
            let payload = Value::Object(root);
            renderer.emit_json(&payload);
        }
        _ => {
            if let Some(explain_ref) = explain.as_ref() {
                if explain_ref.placement == ExplainPlacement::Before {
                    explain_ref.level.emit(renderer, &explain_ref.message);
                }
            }
            text_level.emit(renderer, &text_message);
            if let Some(explain_ref) = explain.as_ref() {
                if explain_ref.placement == ExplainPlacement::After {
                    explain_ref.level.emit(renderer, &explain_ref.message);
                }
            }
        }
    }
}

pub fn render_property_success(renderer: &OutputRenderer, payload: PropertySuccess) {
    let PropertySuccess {
        task_id,
        json_message,
        text_message,
        text_level,
        old_field,
        new_field,
        old_value,
        new_value,
        extra_json,
    } = payload;

    match renderer.format {
        OutputFormat::Json => {
            let mut root = Map::new();
            root.insert("status".to_string(), Value::String("success".to_string()));
            root.insert("message".to_string(), Value::String(json_message));
            root.insert("task_id".to_string(), Value::String(task_id));
            root.insert(old_field, option_to_value(old_value));
            root.insert(new_field, option_to_value(new_value));
            for (key, value) in extra_json {
                root.insert(key, value);
            }
            let payload = Value::Object(root);
            renderer.emit_json(&payload);
        }
        _ => {
            text_level.emit(renderer, &text_message);
        }
    }
}

fn option_to_value(value: Option<String>) -> Value {
    match value {
        Some(v) => Value::String(v),
        None => Value::Null,
    }
}
