use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/agents/profiles", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        // Without a project, use the default project: agent profiles usually
        // live in a project's config rather than the global one.
        let project = match req.query.get("project") {
            Some(project) => Some(project.clone()),
            None => crate::config::resolution::config_for_project(&resolver.path, None)
                .ok()
                .map(|config| config.default_project)
                .filter(|project| !project.trim().is_empty()),
        };
        let config =
            match crate::config::resolution::config_for_project(&resolver.path, project.as_deref())
            {
                Ok(c) => c,
                Err(e) => {
                    return internal_error(e.to_string());
                }
            };

        let mut profiles: Vec<crate::api_types::AgentProfileInfo> = config
            .agent_profiles
            .iter()
            .map(|(name, profile)| crate::api_types::AgentProfileInfo {
                name: name.clone(),
                runner: profile.runner.clone(),
                description: None,
                model: profile.model.clone(),
                effort: profile.effort.clone(),
                permissions: match profile.permissions.unwrap_or_default() {
                    crate::config::types::AgentPermissions::ReadOnly => "read-only",
                    crate::config::types::AgentPermissions::Workspace => "workspace",
                    crate::config::types::AgentPermissions::Full => "full",
                }
                .to_string(),
            })
            .collect();
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        let installed = crate::services::agent_runner::installed_agent_clis();

        ok_json(
            200,
            json!({"data": {"profiles": profiles, "installed_runners": installed, "project": project}}),
        )
    });

    // POST /api/agents/profiles — add one agent profile to the config.
    api_server.register_handler("POST", "/api/agents/profiles", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: crate::api_types::AgentProfileCreateRequest =
            match serde_json::from_slice(&req.body) {
                Ok(body) => body,
                Err(e) => return bad_request(format!("Invalid request: {e}")),
            };
        let permissions = match body.permissions.as_deref().map(str::trim) {
            None | Some("") => None,
            Some("read-only") => Some(crate::config::types::AgentPermissions::ReadOnly),
            Some("workspace") => Some(crate::config::types::AgentPermissions::Workspace),
            Some("full") => Some(crate::config::types::AgentPermissions::Full),
            Some(other) => {
                return bad_request(format!(
                    "Invalid permissions '{other}'. Expected read-only, workspace or full"
                ));
            }
        };
        let non_blank = |value: Option<String>| {
            value
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let (profile_model, profile_effort) = (non_blank(body.model), non_blank(body.effort));
        for (label, value) in [("model", &profile_model), ("effort", &profile_effort)] {
            if let Some(value) = value
                && let Err(e) = crate::services::agent_runner::validate_run_setting(label, value)
            {
                return bad_request(e.to_string());
            }
        }
        let profile = crate::config::types::AgentProfileDetail {
            runner: body.runner.trim().to_ascii_lowercase(),
            permissions,
            model: profile_model,
            effort: profile_effort,
            ..Default::default()
        };
        match crate::services::config_service::ConfigService::add_agent_profile(
            &resolver,
            body.project.as_deref(),
            body.global,
            &body.name,
            profile,
        ) {
            Ok(outcome) => config_write_response(&resolver, outcome.updated, &outcome.validation),
            Err(e) => bad_request(e.to_string()),
        }
    });

    // POST /api/config/set
}
