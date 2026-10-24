use lotar::api_server::{ApiServer, HttpRequest};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use serde_json::Value;
use std::collections::HashMap;
mod common;
use common::env_mutex::EnvVarGuard;

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for project in ["AA", "ZZ", "abc-team", "ABC-OPS", "Équipe"] {
        let dir = tmp.path().join(project);
        std::fs::create_dir_all(&dir).unwrap();
        for number in [9_u64, 10, 54, 154, 540, 9007199254740993, u64::MAX] {
            std::fs::write(dir.join(format!("{number}.yml")), format!(
                "title: Mention 54 repair {number}\nsubtitle: secondary phrase\ndescription: deeper phrase\ntags: [searchable]\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n"
            )).unwrap();
        }
    }
    tmp
}

fn suggest(query: &str, project: Option<&str>, limit: usize) -> Vec<String> {
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let mut params = HashMap::from([
        ("q".into(), query.into()),
        ("limit".into(), limit.to_string()),
    ]);
    if let Some(project) = project {
        params.insert("project".into(), project.into());
    }
    let response = api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: "/api/tasks/suggest".into(),
        query: params,
        headers: HashMap::new(),
        body: Vec::new(),
    });
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            assert_eq!(item.as_object().unwrap().len(), 2);
            assert!(item["title"].is_string());
            item["id"].as_str().unwrap().to_owned()
        })
        .collect()
}

#[test]
fn suggestion_exact_ids_and_numeric_aliases_rank_before_limit() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    for query in ["ZZ-54", "ZZ-00054"] {
        assert_eq!(suggest(query, None, 1), ["ZZ-54"]);
    }
    for query in ["54", "00054"] {
        assert_eq!(suggest(query, Some("ZZ"), 1), ["ZZ-54"]);
        assert_eq!(suggest(query, None, 2), ["AA-54", "ABC-OPS-54"]);
    }
    let ids = suggest("54", Some("ZZ"), 20);
    assert_eq!(&ids[..3], ["ZZ-54", "ZZ-540", "ZZ-154"]);
    assert_eq!(suggest("AA-54", Some("ZZ"), 1), Vec::<String>::new());
}

#[test]
fn suggestion_identity_case_unicode_and_u64_are_preserved() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    for project in ["abc-team", "ABC-OPS", "Équipe"] {
        assert_eq!(
            suggest(&format!("{project}-00054"), None, 1),
            [format!("{project}-54")]
        );
    }
    assert!(suggest("abc-ops-00054", None, 1).is_empty());
    assert!(suggest("ABC-OPS-54", Some("abc-team"), 1).is_empty());
    for number in ["9007199254740993", "18446744073709551615"] {
        assert_eq!(suggest(number, Some("ZZ"), 1), [format!("ZZ-{number}")]);
    }
    assert!(suggest("18446744073709551616", None, 20).is_empty());
}

#[test]
fn suggestion_empty_and_text_are_stable_without_mutating_list_order_or_files() {
    let tmp = fixture();
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    let storage = Storage::new(tmp.path());
    let filter = lotar::api_types::TaskListFilter {
        project: Some("ZZ".into()),
        ..Default::default()
    };
    let before = TaskService::list(&storage, &filter)
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let path = tmp.path().join("ZZ/54.yml");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(suggest("", Some("ZZ"), 2), ["ZZ-9", "ZZ-10"]);
    for query in ["repair", "secondary", "deeper", "searchable"] {
        assert_eq!(suggest(query, Some("ZZ"), 2), ["ZZ-9", "ZZ-10"]);
    }
    assert_eq!(suggest("", Some("ZZ"), 2), suggest("", Some("ZZ"), 2));
    assert_eq!(
        before,
        TaskService::list(&storage, &filter)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
    );
    assert_eq!(bytes, std::fs::read(path).unwrap());
}

#[test]
fn suggestion_text_relevance_and_rest_service_order_agree() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("TP");
    std::fs::create_dir_all(&dir).unwrap();
    for (number, fields) in [
        (1, "title: Other\ntags: [repair]"),
        (2, "title: Other\ndescription: repair"),
        (3, "title: Other\nsubtitle: repair"),
        (4, "title: Please repair this"),
        (5, "title: Repair this"),
        (6, "title: Repair"),
    ] {
        std::fs::write(dir.join(format!("{number}.yml")), format!(
            "{fields}\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n"
        )).unwrap();
    }
    let _env = EnvVarGuard::set("LOTAR_TASKS_DIR", tmp.path().to_str().unwrap());
    let expected = ["TP-6", "TP-5", "TP-4", "TP-3", "TP-2", "TP-1"];
    assert_eq!(suggest("REPAIR", None, 20), expected);
    assert_eq!(
        TaskService::suggest(&Storage::new(tmp.path()), "REPAIR", None, 20)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(suggest("repair", None, 0).is_empty());

    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let list = || {
        let response = api.handle_request(&HttpRequest {
            method: "GET".into(),
            path: "/api/tasks/list".into(),
            query: HashMap::from([
                ("project".into(), "TP".into()),
                ("q".into(), "repair".into()),
                ("sort_by".into(), "title".into()),
                ("order".into(), "asc".into()),
            ]),
            headers: HashMap::new(),
            body: Vec::new(),
        });
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_slice(&response.body).unwrap();
        body["data"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| task["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let before = list();
    assert_eq!(before, ["TP-1", "TP-2", "TP-3", "TP-4", "TP-6", "TP-5"]);
    assert_eq!(suggest("repair", None, 20), expected);
    assert_eq!(list(), before);
}
