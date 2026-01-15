use serde_json::{Map as JsonMap, Value, json};

use super::hints::{EnumHints, attach_field_hints, insert_field_hint};

fn append_hint_descriptions(tool: &mut Value, sections: &[(&[String], &str)]) {
    let mut sentences = Vec::new();
    for (values, label) in sections {
        if values.is_empty() {
            continue;
        }
        sentences.push(format!("Available {}: {}.", label, values.join(", ")));
    }
    if sentences.is_empty() {
        return;
    }
    if let Value::Object(tool_obj) = tool
        && let Some(Value::String(desc)) = tool_obj.get_mut("description")
    {
        let trimmed = desc.trim_end();
        let mut new_desc = trimmed.to_string();
        if !trimmed.ends_with('.') {
            new_desc.push('.');
        }
        new_desc.push(' ');
        new_desc.push_str(&sentences.join(" "));
        *desc = new_desc;
    }
}

pub(super) fn build_tool_definitions(enum_hints: Option<&EnumHints>) -> Vec<Value> {
    vec![
        make_whoami_tool(),
        make_task_create_tool(enum_hints),
        make_task_get_tool(enum_hints),
        make_task_update_tool(enum_hints),
        make_task_comment_add_tool(enum_hints),
        make_task_comment_update_tool(enum_hints),
        make_task_bulk_update_tool(enum_hints),
        make_task_bulk_comment_add_tool(enum_hints),
        make_task_bulk_reference_add_tool(enum_hints),
        make_task_bulk_reference_remove_tool(enum_hints),
        make_task_reference_add_tool(enum_hints),
        make_task_reference_remove_tool(enum_hints),
        make_task_delete_tool(enum_hints),
        make_task_list_tool(enum_hints),
        make_sprint_list_tool(),
        make_sprint_get_tool(),
        make_sprint_create_tool(),
        make_sprint_update_tool(),
        make_sprint_summary_tool(),
        make_sprint_burndown_tool(),
        make_sprint_velocity_tool(),
        make_sprint_add_tool(),
        make_sprint_remove_tool(),
        make_sprint_delete_tool(),
        make_sprint_backlog_tool(enum_hints),
        make_project_list_tool(enum_hints),
        make_project_stats_tool(enum_hints),
        make_config_show_tool(enum_hints),
        make_config_set_tool(enum_hints),
        make_schema_discover_tool(),
    ]
}

fn make_task_reference_add_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "task_reference_add",
        "description": "Attach a reference to a task. kind must be one of: link, file, code. For link, value is a URL. For file, value is a repo-relative file path. For code, value is a code reference like src/lib.rs#10-12. Returns {task, changed}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "project": {"type": ["string", "null"]},
                "kind": {"type": "string"},
                "value": {"type": "string"}
            },
            "required": ["id", "kind", "value"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    let kinds = vec!["link".to_string(), "file".to_string(), "code".to_string()];
    insert_field_hint(&mut field_hints, "kind", Some(kinds.as_slice()), false);
    attach_field_hints(&mut tool, field_hints);
    tool
}

fn make_task_reference_remove_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "task_reference_remove",
        "description": "Detach a reference from a task. kind must be one of: link, file, code. value should match the stored reference string. Returns {task, changed}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "project": {"type": ["string", "null"]},
                "kind": {"type": "string"},
                "value": {"type": "string"}
            },
            "required": ["id", "kind", "value"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    let kinds = vec!["link".to_string(), "file".to_string(), "code".to_string()];
    insert_field_hint(&mut field_hints, "kind", Some(kinds.as_slice()), false);
    attach_field_hints(&mut tool, field_hints);
    tool
}

fn make_task_create_tool(enum_hints: Option<&EnumHints>) -> Value {
    let description = "Create and persist a task. Any missing priority/type/status fall back to project defaults. reporter/assignee accept '@me'. relationships should follow the TaskRelationships shape (e.g. blocks/relates). Returns the saved task JSON with defaults applied.".to_string();

    let mut properties = JsonMap::new();
    properties.insert("title".into(), json!({"type": "string"}));
    properties.insert("description".into(), json!({"type": ["string", "null"]}));
    properties.insert("project".into(), json!({"type": ["string", "null"]}));
    properties.insert("priority".into(), json!({"type": ["string", "null"]}));
    properties.insert("type".into(), json!({"type": ["string", "null"]}));
    properties.insert("reporter".into(), json!({"type": ["string", "null"]}));
    properties.insert("assignee".into(), json!({"type": ["string", "null"]}));
    properties.insert("due_date".into(), json!({"type": ["string", "null"]}));
    properties.insert("effort".into(), json!({"type": ["string", "null"]}));
    properties.insert(
        "tags".into(),
        json!({"type": "array", "items": {"type": "string"}}),
    );
    properties.insert("relationships".into(), json!({"type": ["object", "null"]}));
    properties.insert(
        "custom_fields".into(),
        json!({
            "type": "object",
            "description": "Assign custom_fields key/value pairs defined in config."
        }),
    );

    let mut tool = json!({
        "name": "task_create",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": Value::Object(properties),
            "required": ["title"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "priority",
        enum_hints.map(|h| h.priorities.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "type",
        enum_hints.map(|h| h.types.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "reporter",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "assignee",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "tags",
        enum_hints.map(|h| h.tags.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "custom_fields",
        enum_hints.map(|h| h.custom_fields.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(
            &mut tool,
            &[
                (hints.projects.as_slice(), "projects"),
                (hints.priorities.as_slice(), "priorities"),
                (hints.types.as_slice(), "types"),
                (hints.members.as_slice(), "members"),
                (hints.tags.as_slice(), "tags"),
                (hints.custom_fields.as_slice(), "custom fields"),
            ],
        );
    }

    tool
}

fn make_task_get_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "task_get",
        "description": "Fetch a task DTO by id (optionally override project prefix). Returns the canonical persisted representation.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "project": {"type": ["string", "null"]}
            },
            "required": ["id"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(&mut tool, &[(hints.projects.as_slice(), "projects")]);
    }

    tool
}

fn make_task_update_tool(enum_hints: Option<&EnumHints>) -> Value {
    let description = "Patch an existing task. Provide fields inside patch; omitted properties stay unchanged. Strings are validated against project config, and reporter/assignee accept '@me'. relationships replaces the full relationship map.".to_string();

    let mut patch_properties = JsonMap::new();
    patch_properties.insert("title".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("description".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("status".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("priority".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("type".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("reporter".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("assignee".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("due_date".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("effort".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert(
        "tags".into(),
        json!({"type": "array", "items": {"type": "string"}}),
    );
    patch_properties.insert("relationships".into(), json!({"type": ["object", "null"]}));
    patch_properties.insert(
        "custom_fields".into(),
        json!({
            "type": "object",
            "description": "Assign custom_fields key/value pairs defined in config."
        }),
    );

    let mut tool = json!({
        "name": "task_update",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "patch": {
                    "type": "object",
                    "properties": Value::Object(patch_properties),
                    "additionalProperties": false
                }
            },
            "required": ["id"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "patch.status",
        enum_hints.map(|h| h.statuses.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "patch.priority",
        enum_hints.map(|h| h.priorities.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "patch.type",
        enum_hints.map(|h| h.types.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "patch.reporter",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "patch.assignee",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "patch.tags",
        enum_hints.map(|h| h.tags.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "patch.custom_fields",
        enum_hints.map(|h| h.custom_fields.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(
            &mut tool,
            &[
                (hints.statuses.as_slice(), "statuses"),
                (hints.priorities.as_slice(), "priorities"),
                (hints.types.as_slice(), "types"),
                (hints.members.as_slice(), "members"),
                (hints.tags.as_slice(), "tags"),
                (hints.custom_fields.as_slice(), "custom fields"),
            ],
        );
    }

    tool
}

fn make_task_delete_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "task_delete",
        "description": "Delete a task by id (optional project override). Returns a text payload indicating deleted=true/false.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "project": {"type": ["string", "null"]}
            },
            "required": ["id"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(&mut tool, &[(hints.projects.as_slice(), "projects")]);
    }

    tool
}

fn make_task_list_tool(enum_hints: Option<&EnumHints>) -> Value {
    let description = "List tasks using optional filters. status/priority/type accept a single string, comma-separated string, or array and are validated via project config. assignee accepts '@me'. tags can be provided as tag (single) or tags (multi). search performs a text match across id/title/description/tags. custom_fields filters require string or array-of-string values. sprints filters by numeric sprint ids.".to_string();

    let mut properties = JsonMap::new();
    properties.insert("project".into(), json!({"type": ["string", "null"]}));
    properties.insert("status".into(), multi_value_string_schema());
    properties.insert("assignee".into(), json!({"type": ["string", "null"]}));
    properties.insert("priority".into(), multi_value_string_schema());
    properties.insert("type".into(), multi_value_string_schema());
    properties.insert("tag".into(), json!({"type": ["string", "null"]}));
    properties.insert("tags".into(), multi_value_string_schema());
    properties.insert("search".into(), json!({"type": ["string", "null"]}));
    properties.insert(
        "custom_fields".into(),
        json!({
            "type": ["object", "null"],
            "description": "Filter by custom_fields; values must be string or array-of-string. Example: {\"team\": [\"infra\"]}."
        }),
    );
    properties.insert(
        "sprints".into(),
        json!({
            "type": ["array", "string", "number", "null"],
            "items": {"type": ["number", "string"]},
            "description": "Filter by sprint ids. Accepts a single number/string, or an array. Strings may be '#<id>' or '<id>'."
        }),
    );
    properties.insert(
        "limit".into(),
        json!({
            "type": ["number", "null"],
            "description": "Maximum number of tasks to return per page (1-200). Defaults to 50."
        }),
    );
    properties.insert(
        "cursor".into(),
        json!({
            // Prefer a type union over oneOf here: some MCP hosts validate with type coercion
            // enabled, which can make values like "1" satisfy *both* branches of a oneOf.
            "type": ["string", "number", "null"],
            "description": "Opaque cursor string returned via nextCursor. Use null/omit for the first page."
        }),
    );
    properties.insert(
        "offset".into(),
        json!({
            "type": ["number", "null"],
            "description": "Alias for cursor (0-based)."
        }),
    );

    let mut tool = json!({
        "name": "task_list",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": Value::Object(properties),
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "status",
        enum_hints.map(|h| h.statuses.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "assignee",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "priority",
        enum_hints.map(|h| h.priorities.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "type",
        enum_hints.map(|h| h.types.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "tag",
        enum_hints.map(|h| h.tags.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "tags",
        enum_hints.map(|h| h.tags.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "custom_fields",
        enum_hints.map(|h| h.custom_fields.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(
            &mut tool,
            &[
                (hints.projects.as_slice(), "projects"),
                (hints.statuses.as_slice(), "statuses"),
                (hints.priorities.as_slice(), "priorities"),
                (hints.types.as_slice(), "types"),
                (hints.tags.as_slice(), "tags"),
                (hints.members.as_slice(), "members"),
            ],
        );
    }

    tool
}

fn make_whoami_tool() -> Value {
    json!({
        "name": "whoami",
        "description": "Resolve the current user identity used for '@me' (default_reporter -> git config -> system username).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "explain": {
                    "type": ["boolean", "null"],
                    "description": "When true, include detection details (sources checked, chosen value)."
                }
            },
            "additionalProperties": false
        }
    })
}

fn make_task_comment_add_tool(_enum_hints: Option<&EnumHints>) -> Value {
    json!({
        "name": "task_comment_add",
        "description": "Append a comment to a task and record a history entry. Returns {task}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "text": {"type": "string"}
            },
            "required": ["id", "text"],
            "additionalProperties": false
        }
    })
}

fn make_task_comment_update_tool(_enum_hints: Option<&EnumHints>) -> Value {
    json!({
        "name": "task_comment_update",
        "description": "Update an existing task comment by 0-based index and record a history entry. Returns {task}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "index": {"type": "number", "description": "0-based comment index."},
                "text": {"type": "string"}
            },
            "required": ["id", "index", "text"],
            "additionalProperties": false
        }
    })
}

fn make_task_bulk_update_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut patch_properties = JsonMap::new();
    patch_properties.insert("title".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("description".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("status".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("priority".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("type".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("reporter".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("assignee".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("due_date".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert("effort".into(), json!({"type": ["string", "null"]}));
    patch_properties.insert(
        "tags".into(),
        json!({"type": ["array", "null"], "items": {"type": "string"}}),
    );
    patch_properties.insert("relationships".into(), json!({"type": ["object", "null"]}));
    patch_properties.insert(
        "custom_fields".into(),
        json!({"type": ["object", "null"], "description": "Set/clear custom_fields. Null clears all."}),
    );
    patch_properties.insert(
        "sprints".into(),
        json!({"type": ["array", "null"], "items": {"type": "number"}}),
    );

    let mut tool = json!({
        "name": "task_bulk_update",
        "description": "Patch multiple tasks in one call. Returns {updated, failed}. stop_on_error=true aborts at first failure.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "ids": {"type": "array", "items": {"type": "string"}},
                "patch": {"type": "object", "properties": Value::Object(patch_properties), "additionalProperties": false},
                "stop_on_error": {"type": ["boolean", "null"]}
            },
            "required": ["ids", "patch"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "status",
        enum_hints.map(|h| h.statuses.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "priority",
        enum_hints.map(|h| h.priorities.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "type",
        enum_hints.map(|h| h.types.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "reporter",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "assignee",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    tool
}

fn make_task_bulk_comment_add_tool(_enum_hints: Option<&EnumHints>) -> Value {
    json!({
        "name": "task_bulk_comment_add",
        "description": "Append the same comment to multiple tasks. Returns {updated, failed}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "ids": {"type": "array", "items": {"type": "string"}},
                "text": {"type": "string"},
                "stop_on_error": {"type": ["boolean", "null"]}
            },
            "required": ["ids", "text"],
            "additionalProperties": false
        }
    })
}

fn make_task_bulk_reference_add_tool(_enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "task_bulk_reference_add",
        "description": "Attach the same reference to multiple tasks. Returns {updated, failed}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "ids": {"type": "array", "items": {"type": "string"}},
                "kind": {"type": "string"},
                "value": {"type": "string"},
                "stop_on_error": {"type": ["boolean", "null"]}
            },
            "required": ["ids", "kind", "value"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    let kinds = vec!["link".to_string(), "file".to_string(), "code".to_string()];
    insert_field_hint(&mut field_hints, "kind", Some(kinds.as_slice()), false);
    attach_field_hints(&mut tool, field_hints);
    tool
}

fn make_task_bulk_reference_remove_tool(_enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "task_bulk_reference_remove",
        "description": "Detach the same reference from multiple tasks. Returns {updated, failed}.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "ids": {"type": "array", "items": {"type": "string"}},
                "kind": {"type": "string"},
                "value": {"type": "string"},
                "stop_on_error": {"type": ["boolean", "null"]}
            },
            "required": ["ids", "kind", "value"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    let kinds = vec!["link".to_string(), "file".to_string(), "code".to_string()];
    insert_field_hint(&mut field_hints, "kind", Some(kinds.as_slice()), false);
    attach_field_hints(&mut tool, field_hints);
    tool
}

fn make_sprint_list_tool() -> Value {
    json!({
        "name": "sprint_list",
        "description": "List sprints. Pagination uses 0-based cursor offsets.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {"type": ["number", "null"], "description": "Max sprints per page."},
                "cursor": {"type": ["string", "number", "null"], "description": "Opaque cursor returned via nextCursor."},
                "offset": {"type": ["number", "null"], "description": "Alias for cursor (0-based)."},
                "include_integrity": {"type": ["boolean", "null"], "description": "When true (default), include missing_sprints/integrity diagnostics."}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_get_tool() -> Value {
    json!({
        "name": "sprint_get",
        "description": "Fetch a sprint by id (sprint_id preferred) and return its computed list item.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {"type": ["string", "null"], "description": "Sprint reference like '#1'."},
                "sprint_id": {"type": ["number", "null"], "description": "Numeric sprint identifier."}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_create_tool() -> Value {
    json!({
        "name": "sprint_create",
        "description": "Create a sprint. Defaults may be applied unless skip_defaults=true.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "label": {"type": ["string", "null"]},
                "goal": {"type": ["string", "null"]},
                "plan_length": {"type": ["string", "null"]},
                "ends_at": {"type": ["string", "null"]},
                "starts_at": {"type": ["string", "null"]},
                "capacity_points": {"type": ["number", "null"]},
                "capacity_hours": {"type": ["number", "null"]},
                "overdue_after": {"type": ["string", "null"]},
                "notes": {"type": ["string", "null"]},
                "skip_defaults": {"type": ["boolean", "null"]}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_update_tool() -> Value {
    json!({
        "name": "sprint_update",
        "description": "Update a sprint. Omit fields to keep unchanged; set fields to null to clear where supported.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {"type": ["string", "number", "null"], "description": "Sprint reference like '#1' or numeric id."},
                "sprint_id": {"type": ["number", "null"], "description": "Numeric sprint id (preferred)."},
                "label": {"type": ["string", "null"]},
                "goal": {"type": ["string", "null"]},
                "plan_length": {"type": ["string", "null"]},
                "ends_at": {"type": ["string", "null"]},
                "starts_at": {"type": ["string", "null"]},
                "capacity_points": {"type": ["number", "null"], "description": "Number sets; null clears; omit leaves unchanged."},
                "capacity_hours": {"type": ["number", "null"], "description": "Number sets; null clears; omit leaves unchanged."},
                "overdue_after": {"type": ["string", "null"]},
                "notes": {"type": ["string", "null"]},
                "actual_started_at": {"type": ["string", "null"], "description": "RFC3339 timestamp, null clears."},
                "actual_closed_at": {"type": ["string", "null"], "description": "RFC3339 timestamp, null clears."}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_summary_tool() -> Value {
    json!({
        "name": "sprint_summary",
        "description": "Compute sprint summary metrics for a sprint.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {"type": ["string", "number", "null"], "description": "Sprint reference like '#1' or numeric id."},
                "sprint_id": {"type": ["number", "null"]}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_burndown_tool() -> Value {
    json!({
        "name": "sprint_burndown",
        "description": "Compute sprint burndown series for a sprint.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {"type": ["string", "number", "null"], "description": "Sprint reference like '#1' or numeric id."},
                "sprint_id": {"type": ["number", "null"]}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_velocity_tool() -> Value {
    json!({
        "name": "sprint_velocity",
        "description": "Compute velocity across recent sprints.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "include_active": {"type": ["boolean", "null"]},
                "limit": {"type": ["number", "null"], "description": "Window size (default 6)."},
                "metric": {"type": ["string", "null"], "description": "tasks | points | hours"}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_add_tool() -> Value {
    json!({
        "name": "sprint_add",
        "description": "Attach one or more tasks to a sprint. When sprint is omitted the active sprint is assumed when unambiguous. Set allow_closed=true to override closed sprint guardrails and cleanup_missing=true to drop references to deleted sprint files before assigning.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {
                    "type": ["string", "null"],
                    "description": "Sprint reference like '#1' or keyword (next/previous/active)."
                },
                "sprint_id": {
                    "type": ["number", "null"],
                    "description": "Numeric sprint identifier. Prefer this over 'sprint' when your host struggles with union schemas."
                },
                "tasks": {"type": "array", "items": {"type": "string"}},
                "allow_closed": {"type": ["boolean", "null"]},
                "cleanup_missing": {"type": ["boolean", "null"]}
            },
            "required": ["tasks"],
            "additionalProperties": false
        }
    })
}

fn make_sprint_remove_tool() -> Value {
    json!({
        "name": "sprint_remove",
        "description": "Detach sprint membership from one or more tasks. When sprint is omitted the active sprint is assumed when unambiguous. Set cleanup_missing=true to prune orphaned sprint references before removing memberships.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {
                    "type": ["string", "null"],
                    "description": "Sprint reference like '#1' or keyword (next/previous/active)."
                },
                "sprint_id": {
                    "type": ["number", "null"],
                    "description": "Numeric sprint identifier. Prefer this over 'sprint' when your host struggles with union schemas."
                },
                "tasks": {"type": "array", "items": {"type": "string"}},
                "cleanup_missing": {"type": ["boolean", "null"]}
            },
            "required": ["tasks"],
            "additionalProperties": false
        }
    })
}

fn make_sprint_delete_tool() -> Value {
    json!({
        "name": "sprint_delete",
        "description": "Delete a sprint by id. Set cleanup_missing=true to drop dangling sprint references from tasks after deletion.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "sprint": {
                    "type": ["string", "null"],
                    "description": "Sprint reference like '#1'."
                },
                "sprint_id": {
                    "type": ["number", "null"],
                    "description": "Numeric sprint identifier. Prefer this over 'sprint' when your host struggles with union schemas."
                },
                "cleanup_missing": {"type": ["boolean", "null"]},
                "force": {"type": ["boolean", "null"]}
            },
            "additionalProperties": false
        }
    })
}

fn make_sprint_backlog_tool(enum_hints: Option<&EnumHints>) -> Value {
    let description = "List tasks without sprint assignments using optional filters (project, status, tag, assignee). Pass cleanup_missing=true to strip references to deleted sprint files before listing.".to_string();

    let mut properties = JsonMap::new();
    properties.insert("project".into(), json!({"type": ["string", "null"]}));
    properties.insert("status".into(), multi_value_string_schema());
    properties.insert("tag".into(), multi_value_string_schema());
    properties.insert("assignee".into(), json!({"type": ["string", "null"]}));
    properties.insert("limit".into(), json!({"type": ["number", "null"]}));
    properties.insert(
        "cleanup_missing".into(),
        json!({"type": ["boolean", "null"]}),
    );
    properties.insert(
        "cursor".into(),
        json!({
            "type": ["string", "number", "null"],
            "description": "Opaque cursor string returned via nextCursor. Use null/omit for the first page."
        }),
    );
    properties.insert(
        "offset".into(),
        json!({
            "type": ["number", "null"],
            "description": "Alias for cursor (0-based)."
        }),
    );

    let mut tool = json!({
        "name": "sprint_backlog",
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": Value::Object(properties),
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    insert_field_hint(
        &mut field_hints,
        "status",
        enum_hints.map(|h| h.statuses.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "tag",
        enum_hints.map(|h| h.tags.as_slice()),
        true,
    );
    insert_field_hint(
        &mut field_hints,
        "assignee",
        enum_hints.map(|h| h.members.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(
            &mut tool,
            &[
                (hints.projects.as_slice(), "projects"),
                (hints.statuses.as_slice(), "statuses"),
                (hints.tags.as_slice(), "tags"),
                (hints.members.as_slice(), "members"),
            ],
        );
    }

    tool
}

fn make_project_list_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut properties = JsonMap::new();
    properties.insert(
        "limit".into(),
        json!({
            "type": ["number", "null"],
            "description": "Maximum number of projects to return per page (1-200). Defaults to 50."
        }),
    );
    properties.insert(
        "cursor".into(),
        json!({
            "type": ["string", "number", "null"],
            "description": "Opaque cursor string returned via nextCursor. Use null/omit for the first page."
        }),
    );
    properties.insert(
        "offset".into(),
        json!({
            "type": ["number", "null"],
            "description": "Alias for cursor (0-based)."
        }),
    );

    let mut tool = json!({
        "name": "project_list",
        "description": "List known projects and their prefixes for the current workspace root.",
        "inputSchema": {
            "type": "object",
            "properties": Value::Object(properties),
            "additionalProperties": false
        }
    });

    if let Some(hints) = enum_hints {
        append_hint_descriptions(&mut tool, &[(hints.projects.as_slice(), "projects")]);
    }

    tool
}

fn make_project_stats_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "project_stats",
        "description": "Return aggregate counts for a project (open/done, recent modified timestamp, top tags).",
        "inputSchema": {
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "name",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(&mut tool, &[(hints.projects.as_slice(), "projects")]);
    }

    tool
}

fn make_config_show_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "config_show",
        "description": "Show the resolved configuration (global or project scope) so callers can discover allowed enum values.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "global": {"type": ["boolean", "null"]},
                "project": {"type": ["string", "null"]}
            },
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(&mut tool, &[(hints.projects.as_slice(), "projects")]);
    }

    tool
}

fn make_config_set_tool(enum_hints: Option<&EnumHints>) -> Value {
    let mut tool = json!({
        "name": "config_set",
        "description": "Update configuration strings at the selected scope. Returns validation warnings/info alongside the outcome.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "global": {"type": ["boolean", "null"]},
                "project": {"type": ["string", "null"]},
                "values": {"type": "object", "additionalProperties": {"type": "string"}}
            },
            "required": ["values"],
            "additionalProperties": false
        }
    });

    let mut field_hints = JsonMap::new();
    insert_field_hint(
        &mut field_hints,
        "project",
        enum_hints.map(|h| h.projects.as_slice()),
        false,
    );
    attach_field_hints(&mut tool, field_hints);

    if let Some(hints) = enum_hints {
        append_hint_descriptions(&mut tool, &[(hints.projects.as_slice(), "projects")]);
    }

    tool
}

fn make_schema_discover_tool() -> Value {
    json!({
        "name": "schema_discover",
        "description": "Return MCP tool definitions (schemas plus hints). Provide an optional tool name to narrow the response.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tool": {
                    "type": ["string", "null"],
                    "description": "Optional tool name to filter (case-insensitive)."
                }
            },
            "additionalProperties": false
        }
    })
}

fn multi_value_string_schema() -> Value {
    json!({
        "oneOf": [
            {"type": "string"},
            {"type": "array", "items": {"type": "string"}}
        ]
    })
}
