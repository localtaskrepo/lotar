//! Jira REST calls plus Jira-specific values: ADF text, user account ids,
//! issue types and transitions.

use super::*;

#[derive(Default)]
pub(super) struct JiraLookupCache {
    pub(super) issue_types: Option<Vec<String>>,
    pub(super) user_cache: HashMap<String, String>,
}

pub(super) fn jira_query_for_remote(remote: &SyncRemoteConfig) -> LoTaRResult<String> {
    let jql = if let Some(filter) = remote.filter.as_deref() {
        let trimmed = filter.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        None
    }
    .or_else(|| {
        remote.project.as_ref().and_then(|project| {
            let trimmed = project.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("project = {}", trimmed))
            }
        })
    })
    .ok_or_else(|| {
        LoTaRError::ValidationError("Jira remote requires filter or project".to_string())
    })?;
    Ok(jql)
}

pub(super) fn jira_validate_query(client: &SyncClient, jql: &str) -> LoTaRResult<()> {
    let url = format!("{}/rest/api/3/search/jql", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("jql", jql)
        .query("startAt", "0")
        .query("maxResults", "1")
        .query("fields", "summary")
        .force_send_body();
    let _ = send_json_request(client, req, None)?;
    Ok(())
}

pub(super) fn jira_fetch_project(client: &SyncClient, key: &str) -> LoTaRResult<JsonValue> {
    let url = format!("{}/rest/api/3/project/{}", client.auth.api_base, key);
    let req = client.agent.get(&url).force_send_body();
    send_json_request(client, req, None)
}

pub(super) fn jira_search_issues(
    client: &SyncClient,
    remote: &SyncRemoteConfig,
) -> LoTaRResult<Vec<JsonValue>> {
    let jql = jira_query_for_remote(remote)?;

    let mut issues = Vec::new();
    let mut start_at = 0usize;
    let max_results = 50usize;
    loop {
        let url = format!("{}/rest/api/3/search/jql", client.auth.api_base);
        let req = client
            .agent
            .get(&url)
            .query("jql", &jql)
            .query("startAt", start_at.to_string())
            .query("maxResults", max_results.to_string())
            .query(
                "fields",
                "summary,description,status,issuetype,priority,assignee,reporter,labels",
            )
            .force_send_body();
        let payload = send_json_request(client, req, None)?;
        let total = payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let batch = payload
            .get("issues")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        issues.extend(batch);
        start_at += max_results;
        if issues.len() >= total {
            break;
        }
    }
    Ok(issues)
}

pub(super) fn jira_fetch_issue(
    client: &SyncClient,
    key: &str,
    fields: &[String],
) -> LoTaRResult<JsonValue> {
    let url = format!("{}/rest/api/3/issue/{}", client.auth.api_base, key);
    let mut req = client.agent.get(&url);
    if !fields.is_empty() {
        req = req.query("fields", fields.join(","));
    }
    send_json_request(client, req.force_send_body(), None)
}

pub(super) fn jira_create_issue(
    client: &SyncClient,
    project: &str,
    payload: &JiraIssuePayload,
) -> LoTaRResult<String> {
    let mut fields = payload.fields.clone();
    if !fields.contains_key("project") {
        fields.insert("project".to_string(), json!({"key": project}));
    }
    let url = format!("{}/rest/api/3/issue", client.auth.api_base);
    let req = client.agent.post(&url);
    let mut body = JsonMap::new();
    body.insert("fields".to_string(), JsonValue::Object(fields));
    let response = send_json_request(client, req, Some(JsonValue::Object(body)))?;
    response
        .get("key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LoTaRError::SerializationError("Missing Jira issue key".to_string()))
}

pub(super) fn jira_update_issue(
    client: &SyncClient,
    key: &str,
    payload: &JiraIssuePayload,
) -> LoTaRResult<()> {
    let url = format!("{}/rest/api/3/issue/{}", client.auth.api_base, key);
    let req = client.agent.put(&url);
    let mut body = JsonMap::new();
    body.insert(
        "fields".to_string(),
        JsonValue::Object(payload.fields.clone()),
    );
    send_json_request(client, req, Some(JsonValue::Object(body)))?;
    Ok(())
}

pub(super) fn jira_transition_issue(
    client: &SyncClient,
    key: &str,
    status: &str,
) -> LoTaRResult<()> {
    let url = format!(
        "{}/rest/api/3/issue/{}/transitions",
        client.auth.api_base, key
    );
    let req = client.agent.get(&url).force_send_body();
    let payload = send_json_request(client, req, None)?;
    let transitions = payload
        .get("transitions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let desired = transitions.into_iter().find(|transition| {
        transition
            .get("name")
            .and_then(|v| v.as_str())
            .map(|name| name.eq_ignore_ascii_case(status))
            .unwrap_or(false)
    });

    let Some(transition) = desired else {
        return Err(LoTaRError::ValidationError(format!(
            "No Jira transition named '{}'",
            status
        )));
    };
    let transition_id = transition
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LoTaRError::SerializationError("Missing transition id".to_string()))?;

    let req = client.agent.post(&url);
    let body = json!({"transition": {"id": transition_id}});
    send_json_request(client, req, Some(body))?;
    Ok(())
}

pub(super) fn jira_adf_to_text(value: &JsonValue) -> String {
    let mut parts = Vec::new();
    if let Some(content) = value.get("content").and_then(|v| v.as_array()) {
        for node in content {
            let mut buffer = String::new();
            collect_adf_text(node, &mut buffer);
            let trimmed = buffer.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    parts.join("\n")
}

pub(super) fn collect_adf_text(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Object(map) => {
            if let Some(JsonValue::String(text)) = map.get("text") {
                out.push_str(text);
            }
            if let Some(JsonValue::Array(content)) = map.get("content") {
                for node in content {
                    collect_adf_text(node, out);
                }
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                collect_adf_text(item, out);
            }
        }
        _ => {}
    }
}

pub(super) fn jira_text_to_adf(text: &str) -> JsonValue {
    if text.trim().is_empty() {
        JsonValue::Null
    } else {
        json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": text}
                    ]
                }
            ]
        })
    }
}

pub(super) fn jira_user_value(
    value: &str,
    field: &str,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) -> Option<JsonValue> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(account_id) = normalize_jira_account_id(trimmed) {
        return Some(json!({"accountId": account_id}));
    }

    if let Some(account_id) = lookup_jira_account_id(trimmed, client, lookup, warnings) {
        return Some(json!({"accountId": account_id}));
    }

    warnings.push(format!(
        "Skipping Jira {} assignment; value '{}' is not an accountId",
        field, trimmed
    ));
    None
}

pub(super) fn lookup_jira_account_id(
    value: &str,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let key = trimmed.to_ascii_lowercase();
    if let Some(account_id) = lookup.user_cache.get(&key) {
        return Some(account_id.clone());
    }

    let client = client?;
    match jira_search_account_id(client, trimmed) {
        Ok(Some(account_id)) => {
            lookup.user_cache.insert(key, account_id.clone());
            Some(account_id)
        }
        Ok(None) => None,
        Err(err) => {
            warnings.push(format!(
                "Failed to resolve Jira user '{}': {}",
                trimmed, err
            ));
            None
        }
    }
}

pub(super) fn jira_search_account_id(
    client: &SyncClient,
    query: &str,
) -> LoTaRResult<Option<String>> {
    let url = format!("{}/rest/api/3/user/search", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("query", query)
        .query("maxResults", "1")
        .force_send_body();
    let payload = send_json_request(client, req, None)?;
    let entries = payload.as_array().cloned().unwrap_or_default();
    Ok(entries
        .iter()
        .find_map(|entry| entry.get("accountId").and_then(|v| v.as_str()))
        .map(|value| value.to_string()))
}

pub(super) fn normalize_jira_account_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = trimmed.strip_prefix("accountId:").unwrap_or(trimmed).trim();
    if candidate.len() < 8 {
        return None;
    }
    if candidate
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':')
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

pub(super) fn resolve_jira_issue_type(
    value: &str,
    lookup: &JiraLookupCache,
    warnings: &mut Vec<String>,
) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    let Some(types) = lookup.issue_types.as_ref() else {
        return trimmed.to_string();
    };
    if let Some(found) = types
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(trimmed))
    {
        return found.clone();
    }

    let fallback = types
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case("Task"))
        .cloned()
        .or_else(|| types.first().cloned());
    if let Some(fallback) = fallback {
        warnings.push(format!(
            "Jira issue type '{}' not found; falling back to '{}'",
            trimmed, fallback
        ));
        return fallback;
    }

    trimmed.to_string()
}

pub(super) fn ensure_jira_issue_types(
    lookup: &mut JiraLookupCache,
    client: Option<&SyncClient>,
    remote: &SyncRemoteConfig,
    warnings: &mut Vec<String>,
) {
    if lookup.issue_types.is_some() {
        return;
    }
    let Some(client) = client else {
        return;
    };
    let Some(project) = remote
        .project
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    match jira_fetch_issue_types(client, project) {
        Ok(types) => lookup.issue_types = Some(types),
        Err(err) => warnings.push(format!(
            "Failed to load Jira issue types for '{}': {}",
            project, err
        )),
    }
}

pub(super) fn jira_fetch_issue_types(
    client: &SyncClient,
    project: &str,
) -> LoTaRResult<Vec<String>> {
    let project_id = jira_resolve_project_id(client, project)?;
    let url = format!("{}/rest/api/3/issuetype/project", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("projectId", &project_id)
        .force_send_body();
    let payload = send_json_request(client, req, None)?;
    let entries = payload.as_array().cloned().unwrap_or_default();
    let mut types: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(|v| v.as_str()))
        .map(|value| value.to_string())
        .collect();
    types.sort();
    types.dedup();
    if types.is_empty() {
        return Err(LoTaRError::ValidationError(format!(
            "No issue types returned for Jira project '{}'",
            project
        )));
    }
    Ok(types)
}

pub(super) fn jira_resolve_project_id(client: &SyncClient, project: &str) -> LoTaRResult<String> {
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(
            "Jira project key is required".to_string(),
        ));
    }
    let url = format!("{}/rest/api/3/project/{}", client.auth.api_base, trimmed);
    let req = client.agent.get(&url).force_send_body();
    let payload = send_json_request(client, req, None)?;
    payload
        .get("id")
        .and_then(|v| v.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| {
            LoTaRError::SerializationError(format!("Missing Jira project id for '{}'", trimmed))
        })
}
