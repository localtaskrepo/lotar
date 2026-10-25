//! GitHub REST calls: issue listing, search, create/update and fetch.

use super::*;

pub(super) fn github_list_issues(
    client: &SyncClient,
    remote: &SyncRemoteConfig,
    warnings: &mut Vec<String>,
) -> LoTaRResult<Vec<JsonValue>> {
    let repo = remote
        .repo
        .as_deref()
        .ok_or_else(|| LoTaRError::ValidationError("GitHub remote must define repo".to_string()))?;
    let normalized_repo = normalize_github_repo(repo);
    if let Some(filter) = remote.filter.as_deref() {
        let trimmed = filter.trim();
        if !trimmed.is_empty() {
            warnings.push(
                "GitHub filter uses search API (results may omit fields and are capped at 1000)"
                    .to_string(),
            );
            return github_search_issues(client, &normalized_repo, trimmed, warnings);
        }
    }

    let mut issues = Vec::new();
    let mut page = 1u32;
    loop {
        let url = format!("{}/repos/{}/issues", client.auth.api_base, normalized_repo);
        let req = client
            .agent
            .get(&url)
            .query("state", "all")
            .query("per_page", "100")
            .query("page", page.to_string())
            .force_send_body();
        let payload = send_json_request(client, req, None)?;
        let batch = payload.as_array().cloned().unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        for issue in &batch {
            if issue.get("pull_request").is_some() {
                continue;
            }
            issues.push(issue.clone());
        }
        if batch.len() < 100 {
            break;
        }
        page += 1;
    }
    Ok(issues)
}

pub(super) fn github_fetch_repo(client: &SyncClient, repo: &str) -> LoTaRResult<JsonValue> {
    let url = format!(
        "{}/repos/{}",
        client.auth.api_base,
        normalize_github_repo(repo)
    );
    let req = client.agent.get(&url).force_send_body();
    send_json_request(client, req, None)
}

pub(super) fn github_build_search_query(repo: &str, filter: &str) -> String {
    let normalized = normalize_github_repo(repo);
    let mut query = format!("repo:{} type:issue", normalized);
    if !filter_has_state(filter) {
        query.push_str(" state:all");
    }
    if !filter.trim().is_empty() {
        query.push(' ');
        query.push_str(filter.trim());
    }
    query
}

pub(super) fn github_validate_search(
    client: &SyncClient,
    repo: &str,
    filter: &str,
    warnings: &mut Vec<String>,
) -> LoTaRResult<()> {
    warnings.push(
        "GitHub filter uses search API (results may omit fields and are capped at 1000)"
            .to_string(),
    );
    let query = github_build_search_query(repo, filter);
    let url = format!("{}/search/issues", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("q", &query)
        .query("per_page", "1")
        .query("page", "1")
        .force_send_body();
    let _ = send_json_request(client, req, None)?;
    Ok(())
}

pub(super) fn github_search_issues(
    client: &SyncClient,
    repo: &str,
    filter: &str,
    warnings: &mut Vec<String>,
) -> LoTaRResult<Vec<JsonValue>> {
    let mut issues = Vec::new();
    let mut page = 1u32;
    let mut total_processed = 0usize;
    let query = github_build_search_query(repo, filter);

    loop {
        let url = format!("{}/search/issues", client.auth.api_base);
        let req = client
            .agent
            .get(&url)
            .query("q", &query)
            .query("per_page", "100")
            .query("page", page.to_string())
            .force_send_body();
        let payload = send_json_request(client, req, None)?;
        let batch = payload
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        for issue in &batch {
            if issue.get("pull_request").is_some() {
                continue;
            }
            issues.push(issue.clone());
        }
        total_processed += batch.len();
        if batch.len() < 100 {
            break;
        }
        if total_processed >= 1000 {
            warnings.push("GitHub search results truncated at 1000 issues".to_string());
            break;
        }
        page += 1;
    }
    let mut detailed = Vec::new();
    for issue in issues {
        let number = issue.get("number").and_then(|v| v.as_u64());
        if let Some(number) = number {
            match github_fetch_issue(client, repo, number) {
                Ok(full) => detailed.push(full),
                Err(err) => {
                    warnings.push(format!(
                        "GitHub issue {} fetch failed; using search payload: {}",
                        number, err
                    ));
                    detailed.push(issue);
                }
            }
        } else {
            detailed.push(issue);
        }
    }

    Ok(detailed)
}

pub(super) fn github_create_issue(
    client: &SyncClient,
    repo: &str,
    payload: &GithubIssuePayload,
) -> LoTaRResult<u64> {
    let url = format!(
        "{}/repos/{}/issues",
        client.auth.api_base,
        normalize_github_repo(repo)
    );
    let req = client.agent.post(&url);
    let response = send_json_request(client, req, Some(payload.to_json(false)))?;
    response
        .get("number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| LoTaRError::SerializationError("Missing GitHub issue number".to_string()))
}

pub(super) fn github_update_issue(
    client: &SyncClient,
    repo: &str,
    number: u64,
    payload: &GithubIssuePayload,
) -> LoTaRResult<()> {
    let url = format!(
        "{}/repos/{}/issues/{}",
        client.auth.api_base,
        normalize_github_repo(repo),
        number
    );
    let req = client.agent.patch(&url);
    send_json_request(client, req, Some(payload.to_json(true)))?;
    Ok(())
}

pub(super) fn github_fetch_issue(
    client: &SyncClient,
    repo: &str,
    number: u64,
) -> LoTaRResult<JsonValue> {
    let url = format!(
        "{}/repos/{}/issues/{}",
        client.auth.api_base,
        normalize_github_repo(repo),
        number
    );
    let req = client.agent.get(&url).force_send_body();
    send_json_request(client, req, None)
}

pub(super) fn normalize_github_repo(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

pub(super) fn filter_has_state(filter: &str) -> bool {
    filter
        .split_whitespace()
        .any(|token| token.to_ascii_lowercase().starts_with("state:"))
}
