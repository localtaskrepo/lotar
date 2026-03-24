use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

const CONTEXT_LIMIT_DEFAULT: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextMessage {
    pub role: String,
    pub content: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContextFile {
    pub ticket_id: String,
    pub updated_at: String,
    pub messages: Vec<AgentContextMessage>,
}

pub struct AgentContextService;

impl AgentContextService {
    pub fn load(
        tasks_dir: &Path,
        config: &ResolvedConfig,
        ticket_id: &str,
    ) -> LoTaRResult<Option<AgentContextFile>> {
        if !config.agent_context_enabled {
            return Ok(None);
        }

        let path = resolve_context_path(tasks_dir, ticket_id, &config.agent_context_extension)?;
        if !path.exists() {
            return Ok(None);
        }
        let payload = fs::read_to_string(&path)?;
        let parsed = serde_json::from_str::<AgentContextFile>(&payload).map_err(|err| {
            LoTaRError::SerializationError(format!("Invalid context payload: {}", err))
        })?;
        Ok(Some(parsed))
    }

    pub fn append_messages(
        tasks_dir: &Path,
        config: &ResolvedConfig,
        ticket_id: &str,
        mut messages: Vec<AgentContextMessage>,
        limit: Option<usize>,
    ) -> LoTaRResult<()> {
        if !config.agent_context_enabled {
            return Ok(());
        }

        let limit = limit.unwrap_or(CONTEXT_LIMIT_DEFAULT);
        let mut context = Self::load(tasks_dir, config, ticket_id)?.unwrap_or(AgentContextFile {
            ticket_id: ticket_id.to_string(),
            updated_at: Utc::now().to_rfc3339(),
            messages: Vec::new(),
        });

        context.messages.append(&mut messages);
        if context.messages.len() > limit {
            let start = context.messages.len().saturating_sub(limit);
            context.messages = context.messages[start..].to_vec();
        }
        context.updated_at = Utc::now().to_rfc3339();

        let path = resolve_context_path(tasks_dir, ticket_id, &config.agent_context_extension)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(&context).map_err(|err| {
            LoTaRError::SerializationError(format!("Failed to serialize context: {}", err))
        })?;
        fs::write(&path, payload)?;
        Ok(())
    }
}

pub fn build_user_message(content: &str) -> AgentContextMessage {
    AgentContextMessage {
        role: "user".to_string(),
        content: content.to_string(),
        at: Utc::now().to_rfc3339(),
    }
}

pub fn build_assistant_message(content: &str) -> AgentContextMessage {
    AgentContextMessage {
        role: "assistant".to_string(),
        content: content.to_string(),
        at: Utc::now().to_rfc3339(),
    }
}

/// Resolve the context file path.
/// Context files are stored at `<tasks_dir>/<PROJECT>/<TICKET_ID><extension>`,
/// right next to the task file itself. This makes the relationship clear
/// and allows easy filtering via gitignore.
fn resolve_context_path(
    tasks_dir: &Path,
    ticket_id: &str,
    extension: &str,
) -> LoTaRResult<PathBuf> {
    let trimmed = ticket_id.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError("Missing ticket id".to_string()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(LoTaRError::ValidationError("Invalid ticket id".to_string()));
    }
    if trimmed
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        return Err(LoTaRError::ValidationError("Invalid ticket id".to_string()));
    }

    // Extract project prefix from ticket_id (e.g., "DICE-1" -> "DICE")
    let project_prefix = trimmed.split('-').next().unwrap_or(trimmed).to_uppercase();

    // Build path: <tasks_dir>/<PROJECT>/<TICKET_ID><extension>
    let project_dir = tasks_dir.join(&project_prefix);
    let filename = format!("{}{}", trimmed, extension);
    let path = project_dir.join(&filename);

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(LoTaRError::ValidationError(
            "Invalid context path".to_string(),
        ));
    }

    Ok(path)
}
