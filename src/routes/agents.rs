use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/agents/profiles", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let project = req.query.get("project").map(|s| s.as_str());
        let cfg_mgr =
            match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                &resolver.path,
            ) {
                Ok(m) => m,
                Err(e) => {
                    return internal(
                        json!({"error": {"code": "INTERNAL", "message": e.to_string()}}),
                    );
                }
            };
        let config = if let Some(prefix) = project {
            cfg_mgr
                .get_project_config(prefix)
                .unwrap_or_else(|_| cfg_mgr.get_resolved_config().clone())
        } else {
            cfg_mgr.get_resolved_config().clone()
        };

        let profiles: Vec<crate::api_types::AgentProfileInfo> = config
            .agent_profiles
            .iter()
            .map(|(name, profile)| crate::api_types::AgentProfileInfo {
                name: name.clone(),
                runner: profile.runner.clone(),
                description: None,
            })
            .collect();

        ok_json(200, json!({"data": {"profiles": profiles}}))
    });

    // POST /api/config/set
}
