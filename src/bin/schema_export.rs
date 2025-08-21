#[cfg(feature = "schema")]
fn main() {
    use schemars::schema_for;
    use std::fs;
    use std::path::PathBuf;

    // Prepare output dir
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out.push("docs/schemas");
    let _ = fs::create_dir_all(&out);

    // Generate schemas for public DTOs used by Web/MCP
    let dto_files: &[(&str, schemars::schema::RootSchema)] = &[
        ("TaskDTO.json", schema_for!(lotar::api_types::TaskDTO)),
        ("TaskCreate.json", schema_for!(lotar::api_types::TaskCreate)),
        ("TaskUpdate.json", schema_for!(lotar::api_types::TaskUpdate)),
        (
            "TaskListFilter.json",
            schema_for!(lotar::api_types::TaskListFilter),
        ),
        ("ProjectDTO.json", schema_for!(lotar::api_types::ProjectDTO)),
        (
            "ProjectStatsDTO.json",
            schema_for!(lotar::api_types::ProjectStatsDTO),
        ),
        (
            "ApiErrorPayload.json",
            schema_for!(lotar::api_types::ApiErrorPayload),
        ),
    ];

    for (name, schema) in dto_files {
        let path = out.join(name);
        let text = serde_json::to_string_pretty(schema).expect("schema to json");
        fs::write(&path, text).expect("write schema file");
        eprintln!("wrote {}", path.display());
    }
}

#[cfg(not(feature = "schema"))]
fn main() {
    eprintln!("schema-export requires the 'schema' feature");
}
