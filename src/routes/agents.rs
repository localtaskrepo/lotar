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

        ok_json(200, json!({"data": {"profiles": profiles}}))
    });

    // POST /api/config/set
}
