//! DEV-280: the MCP help's tool table lists exactly the advertised tools,
//! and the server no longer advertises a logging capability it never uses.

use std::collections::BTreeSet;

fn call(method: &str) -> serde_json::Value {
    let req = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": {}});
    let line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&req).unwrap());
    serde_json::from_str(&line).unwrap()
}

#[test]
fn docs_tool_table_matches_tools_list() {
    let advertised: BTreeSet<String> = call("tools/list")["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();

    let docs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/help/mcp.md"),
    )
    .unwrap();
    let section = docs
        .split("## Tool Surface")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap();
    let mut documented = BTreeSet::new();
    for row in section.lines().filter(|line| line.starts_with("| `")) {
        let first_cell = row.split('|').nth(1).unwrap();
        for name in first_cell.split('`').skip(1).step_by(2) {
            documented.insert(name.to_string());
        }
    }
    assert_eq!(
        documented, advertised,
        "docs/help/mcp.md Tool Surface table"
    );
}

#[test]
fn initialize_does_not_advertise_logging() {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }
    });
    let line = lotar::mcp::server::handle_json_line(&serde_json::to_string(&req).unwrap());
    let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
    let capabilities = &resp["result"]["capabilities"];
    assert!(capabilities.get("tools").is_some(), "{resp}");
    assert!(capabilities.get("logging").is_none(), "{resp}");
}
