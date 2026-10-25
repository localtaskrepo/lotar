//! Auth profile resolution and the HTTP client shared by both providers.

use super::*;

#[derive(Clone)]
pub(super) struct AuthContext {
    pub(super) provider: SyncProvider,
    pub(super) api_base: String,
    pub(super) auth_header: Option<String>,
    pub(super) user_agent: String,
}

pub(super) struct SyncClient {
    pub(super) agent: ureq::Agent,
    pub(super) auth: AuthContext,
}

impl SyncClient {
    pub(super) fn new(auth: AuthContext) -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(30)))
                .http_status_as_error(false)
                .build()
                .into(),
            auth,
        }
    }
}

pub(super) fn build_auth_context(
    remote: &SyncRemoteConfig,
    profile_name: &str,
    profile: &SyncAuthProfile,
    warnings: &mut Vec<String>,
) -> LoTaRResult<AuthContext> {
    let provider = profile.provider.unwrap_or(remote.provider);

    if profile.provider.is_some() && provider != remote.provider {
        warnings.push(format!(
            "Auth profile '{}' provider does not match remote provider",
            profile_name
        ));
    }

    let method = profile
        .method
        .as_deref()
        .unwrap_or(match provider {
            SyncProvider::Jira => "basic",
            SyncProvider::Github => "token",
        })
        .trim()
        .to_ascii_lowercase();

    let api_base = match provider {
        SyncProvider::Jira => profile
            .api_url
            .as_deref()
            .or(profile.base_url.as_deref())
            .map(strip_trailing_slash)
            .ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set api_url or base_url",
                    profile_name
                ))
            })?,
        SyncProvider::Github => profile
            .api_url
            .as_deref()
            .map(strip_trailing_slash)
            .unwrap_or_else(|| "https://api.github.com".to_string()),
    };

    let auth_header = match method.as_str() {
        "basic" => {
            let email_env = profile.email_env.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set email_env (env var name or literal) for basic auth",
                    profile_name
                ))
            })?;
            let token_env = profile.token_env.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set token_env (env var name or literal) for basic auth",
                    profile_name
                ))
            })?;
            let email = resolve_auth_value(email_env, profile_name, "email", warnings)?;
            let token = resolve_auth_value(token_env, profile_name, "token", warnings)?;
            let raw = format!("{}:{}", email, token);
            Some(format!("Basic {}", BASE64_STANDARD.encode(raw.as_bytes())))
        }
        "bearer" | "token" | "pat" => {
            let token_env = profile.token_env.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set token_env (env var name or literal)",
                    profile_name
                ))
            })?;
            let token = resolve_auth_value(token_env, profile_name, "token", warnings)?;
            Some(format!("Bearer {}", token))
        }
        other => {
            return Err(LoTaRError::ValidationError(format!(
                "Unsupported auth method '{}'",
                other
            )));
        }
    };

    Ok(AuthContext {
        provider,
        api_base,
        auth_header,
        user_agent: "lotar".to_string(),
    })
}

pub(super) fn resolve_auth_value(
    name_or_value: &str,
    profile_name: &str,
    label: &str,
    warnings: &mut Vec<String>,
) -> LoTaRResult<String> {
    let trimmed = name_or_value.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(format!(
            "Auth profile '{}' {} value is empty",
            profile_name, label
        )));
    }

    match env::var(trimmed) {
        Ok(value) => {
            let resolved = value.trim();
            if resolved.is_empty() {
                Err(LoTaRError::ValidationError(format!(
                    "Environment variable '{}' is empty",
                    trimmed
                )))
            } else {
                Ok(resolved.to_string())
            }
        }
        Err(_) => {
            if looks_like_env_var(trimmed) {
                warnings.push(format!(
                    "Auth profile '{}' {} env var is not set; using the literal value from home config",
                    profile_name, label
                ));
            }
            Ok(trimmed.to_string())
        }
    }
}

pub(super) fn looks_like_env_var(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

pub(super) fn strip_trailing_slash(raw: &str) -> String {
    raw.trim_end_matches('/').to_string()
}

pub(super) fn request_with_headers<B>(
    client: &SyncClient,
    req: ureq::RequestBuilder<B>,
) -> ureq::RequestBuilder<B> {
    let mut req = req.header("Accept", "application/json");
    req = req.header("Content-Type", "application/json");
    req = req.header("User-Agent", &client.auth.user_agent);
    if client.auth.provider == SyncProvider::Github {
        req = req.header("Accept", "application/vnd.github+json");
        req = req.header("X-GitHub-Api-Version", "2022-11-28");
    }
    if let Some(auth) = client.auth.auth_header.as_ref() {
        req = req.header("Authorization", auth);
    }
    req
}

pub(super) fn send_json_request(
    client: &SyncClient,
    req: ureq::RequestBuilder<ureq::typestate::WithBody>,
    body: Option<JsonValue>,
) -> LoTaRResult<JsonValue> {
    let req = request_with_headers(client, req);
    let mut resp = match body {
        Some(payload) => req.send_json(payload),
        None => req.send_empty(),
    }
    .map_err(|e| LoTaRError::ValidationError(format!("Remote API request failed: {}", e)))?;

    let code = resp.status().as_u16();
    if code >= 400 {
        let payload = resp
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());
        let message = payload.trim();
        let detail = if message.is_empty() {
            format!("Remote API error (HTTP {})", code)
        } else {
            format!("Remote API error (HTTP {}): {}", code, message)
        };
        return Err(LoTaRError::ValidationError(detail));
    }

    parse_json_response(resp.body_mut())
}

pub(super) fn parse_json_response(body: &mut ureq::Body) -> LoTaRResult<JsonValue> {
    let payload = body.read_to_string().map_err(|e| {
        LoTaRError::SerializationError(format!("Failed to read response body: {}", e))
    })?;
    if payload.trim().is_empty() {
        return Ok(JsonValue::Null);
    }
    serde_json::from_str(&payload)
        .map_err(|e| LoTaRError::SerializationError(format!("Invalid JSON response: {}", e)))
}
