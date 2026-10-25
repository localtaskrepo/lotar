//! Matching task references (Jira keys, GitHub `repo#number`) to a remote.

use super::*;

#[derive(Debug)]
pub(super) enum ReferenceState {
    Matching(String),
    ProviderOnly(String),
    None,
}

pub(super) fn determine_reference_state(
    remote: &SyncRemoteConfig,
    task: &TaskDTO,
) -> ReferenceState {
    let mut provider_reference = None;
    let mut other_reference = None;

    for reference in &task.references {
        let (candidate, other_candidate) = match remote.provider {
            SyncProvider::Jira => (reference.jira.as_deref(), reference.github.as_deref()),
            SyncProvider::Github => (reference.github.as_deref(), reference.jira.as_deref()),
        };
        if let Some(raw) = candidate {
            if provider_reference.is_none() {
                provider_reference = Some(raw);
            }
            if let Some(normalized) = normalize_reference_for_remote(remote, raw) {
                return ReferenceState::Matching(normalized);
            }
        }
        if other_reference.is_none()
            && let Some(raw) = other_candidate
        {
            other_reference = Some(raw);
        }
    }

    if let Some(reference) = provider_reference {
        ReferenceState::ProviderOnly(reference.to_string())
    } else if let Some(reference) = other_reference {
        ReferenceState::ProviderOnly(reference.to_string())
    } else {
        ReferenceState::None
    }
}

pub(super) fn normalize_reference_for_remote(
    remote: &SyncRemoteConfig,
    value: &str,
) -> Option<String> {
    match remote.provider {
        SyncProvider::Jira => {
            let normalized = normalize_jira_reference(value)?;
            if let Some(project) = remote.project.as_ref() {
                let prefix = format!("{}-", project.trim().to_ascii_uppercase());
                if !normalized.to_ascii_uppercase().starts_with(&prefix) {
                    return None;
                }
            }
            Some(normalized)
        }
        SyncProvider::Github => {
            let (repo, number) = parse_github_reference(value, remote.repo.as_deref())?;
            if let Some(expected) = remote.repo.as_deref() {
                let expected_norm = normalize_github_repo(expected);
                if normalize_github_repo(&repo) != expected_norm {
                    return None;
                }
            }
            Some(format!("{}#{}", normalize_github_repo(&repo), number))
        }
    }
}

pub(super) fn normalize_jira_reference(value: &str) -> Option<String> {
    let trimmed = trim_platform_prefix(value, "jira");
    let (prefix, rest) = trimmed.split_once('-')?;
    if rest.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{}-{}",
        prefix.trim().to_ascii_uppercase(),
        rest.trim()
    ))
}

pub(super) fn parse_github_reference(
    value: &str,
    default_repo: Option<&str>,
) -> Option<(String, u64)> {
    let trimmed = trim_platform_prefix(value, "github");
    if let Some((repo, number)) = trimmed.split_once('#') {
        let repo_trimmed = repo.trim();
        let num = number.trim().parse::<u64>().ok()?;
        if repo_trimmed.is_empty() {
            let default_repo = default_repo?;
            return Some((default_repo.to_string(), num));
        }
        return Some((repo_trimmed.to_string(), num));
    }

    if let Ok(num) = trimmed.trim().parse::<u64>() {
        let default_repo = default_repo?;
        return Some((default_repo.to_string(), num));
    }

    None
}

pub(super) fn trim_platform_prefix(value: &str, prefix: &str) -> String {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let needle = format!("{}:", prefix.to_ascii_lowercase());
    if lower.starts_with(&needle) {
        trimmed[needle.len()..].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn build_reference_index(
    remote: &SyncRemoteConfig,
    tasks: &[(String, TaskDTO)],
) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for (_id, task) in tasks {
        for reference in &task.references {
            let raw = match remote.provider {
                SyncProvider::Jira => reference.jira.as_deref(),
                SyncProvider::Github => reference.github.as_deref(),
            };
            if let Some(raw) = raw
                && let Some(normalized) = normalize_reference_for_remote(remote, raw)
            {
                index.insert(normalized, task.id.clone());
            }
        }
    }
    index
}

pub(super) fn issue_reference_for_remote(
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
) -> Option<String> {
    match remote.provider {
        SyncProvider::Jira => issue
            .get("key")
            .and_then(|v| v.as_str())
            .and_then(|key| normalize_reference_for_remote(remote, key)),
        SyncProvider::Github => {
            let repo = remote.repo.as_deref()?;
            let number = issue.get("number").and_then(|v| v.as_u64())?;
            Some(format!("{}#{}", normalize_github_repo(repo), number))
        }
    }
}
