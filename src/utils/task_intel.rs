use crate::config::types::ResolvedConfig;
use crate::types::{Priority, TaskStatus, TaskType};
use std::collections::HashMap;

struct BranchContext {
    branch_lower: String,
    first_segment: String,
    primary_token: String,
}

impl BranchContext {
    fn detect() -> Option<Self> {
        let cwd = std::env::current_dir().ok()?;
        let repo_root = crate::utils::git::find_repo_root(&cwd)?;
        let branch = crate::utils::git::read_current_branch(&repo_root)?;
        let branch_lower = branch.to_lowercase();
        let first_segment = branch_lower
            .split('/')
            .next()
            .unwrap_or(&branch_lower)
            .to_string();
        let primary_token = first_segment
            .split(['-', '_'])
            .next()
            .unwrap_or(&first_segment)
            .to_string();
        Some(Self {
            branch_lower,
            first_segment,
            primary_token,
        })
    }
}

fn alias_lookup<T: Clone>(map: &HashMap<String, T>, candidate: &str) -> Option<T> {
    if map.is_empty() {
        return None;
    }
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(v) = map.get(trimmed) {
        return Some(v.clone());
    }
    let needle = trimmed.to_ascii_lowercase();
    map.iter()
        .find(|(key, _)| key.trim().eq_ignore_ascii_case(trimmed))
        .map(|(_, value)| value.clone())
        .or_else(|| {
            map.iter()
                .find(|(key, _)| key.trim().to_ascii_lowercase() == needle)
                .map(|(_, value)| value.clone())
        })
}

fn alias_prefix_match<T: Clone>(map: &HashMap<String, T>, branch_lower: &str) -> Option<T> {
    if map.is_empty() {
        return None;
    }
    for (key, value) in map.iter() {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        for sep in ['-', '_', '/'] {
            let mut pattern = lower.clone();
            pattern.push(sep);
            if branch_lower.starts_with(&pattern) {
                return Some(value.clone());
            }
        }
        if branch_lower == lower {
            return Some(value.clone());
        }
    }
    None
}

pub fn infer_task_type_from_branch(config: &ResolvedConfig) -> Option<TaskType> {
    if !config.auto_branch_infer_type {
        return None;
    }
    let ctx = BranchContext::detect()?;

    if let Some(tt) = alias_lookup(&config.branch_type_aliases, &ctx.first_segment) {
        return Some(tt);
    }
    if let Some(tt) = alias_lookup(&config.branch_type_aliases, &ctx.primary_token) {
        return Some(tt);
    }
    if let Some(tt) = alias_prefix_match(&config.branch_type_aliases, &ctx.branch_lower) {
        return Some(tt);
    }

    for tt in &config.issue_types.values {
        let name_lower = tt.as_str().to_ascii_lowercase();
        if name_lower == ctx.first_segment
            || name_lower == ctx.primary_token
            || ctx.branch_lower.starts_with(&(name_lower.clone() + "-"))
            || ctx.branch_lower.starts_with(&(name_lower.clone() + "_"))
            || ctx.branch_lower.starts_with(&(name_lower + "/"))
        {
            return Some(tt.clone());
        }
    }

    match ctx.primary_token.as_str() {
        "feat" | "feature" => TaskType::parse_with_config("Feature", config).ok(),
        "fix" | "bugfix" | "hotfix" => TaskType::parse_with_config("Bug", config).ok(),
        "chore" | "docs" | "refactor" | "test" | "perf" => {
            TaskType::parse_with_config("Chore", config).ok()
        }
        _ => None,
    }
}

pub fn infer_priority_from_branch(config: &ResolvedConfig) -> Option<Priority> {
    if !config.auto_branch_infer_priority {
        return None;
    }
    let ctx = BranchContext::detect()?;

    if let Some(p) = alias_lookup(&config.branch_priority_aliases, &ctx.first_segment) {
        return Some(p);
    }
    if let Some(p) = alias_lookup(&config.branch_priority_aliases, &ctx.primary_token) {
        return Some(p);
    }
    alias_prefix_match(&config.branch_priority_aliases, &ctx.branch_lower)
}

pub fn infer_status_from_branch(config: &ResolvedConfig) -> Option<TaskStatus> {
    if !config.auto_branch_infer_status {
        return None;
    }
    let ctx = BranchContext::detect()?;

    if let Some(status) = alias_lookup(&config.branch_status_aliases, &ctx.first_segment) {
        return Some(status);
    }
    if let Some(status) = alias_lookup(&config.branch_status_aliases, &ctx.primary_token) {
        return Some(status);
    }
    alias_prefix_match(&config.branch_status_aliases, &ctx.branch_lower)
}

pub fn auto_tag_from_path(config: &ResolvedConfig) -> Option<String> {
    if !config.auto_tags_from_path {
        return None;
    }
    crate::utils::workspace_labels::derive_label_from_cwd()
}
