use super::*;
use crate::services::task_service::{TaskService, TaskUpdateContext};

#[test]
fn dev151_typed_unique_id_keeps_isolated_task_before_budget_without_mutation() {
    let f = Fixture::new();
    f.write("REL-2", Default::default());
    f.write("REL-20", Default::default());
    let mut noise = f.task("REL-20");
    noise.title = "Newer mentions 2, REL-2, REL-002, REL2 and REL002".into();
    noise.modified = "2026-10-24T00:00:00Z".into();
    fs::write(
        f.storage.root_path.join("REL/20.yml"),
        serde_yaml_ng::to_string(&noise).unwrap(),
    )
    .unwrap();
    let before = [f.bytes("REL-2"), f.bytes("REL-20")];
    for text in ["2", "002", "REL-2", "REL-002", "REL2", "REL002"] {
        for include_isolated in [false, true] {
            let response = graph(
                &f.storage,
                &RelationshipGraphRequest {
                    project: Some("REL".into()),
                    max_nodes: Some(1),
                    include_isolated: Some(include_isolated),
                    filters: BTreeMap::from([("q".into(), text.into())]),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                response
                    .nodes
                    .iter()
                    .map(|node| node.id.as_str())
                    .collect::<Vec<_>>(),
                ["REL-2"],
                "{text}, include_isolated={include_isolated}"
            );
            assert!(response.nodes[0].matches_filter);
            assert!(response.edges.is_empty());
        }
    }
    assert_eq!(before, [f.bytes("REL-2"), f.bytes("REL-20")]);
    for text in ["2", "REL-002", "REL002"] {
        let response = graph(
            &f.storage,
            &RelationshipGraphRequest {
                project: Some("REL".into()),
                include_isolated: Some(false),
                filters: BTreeMap::from([
                    ("q".into(), text.into()),
                    ("status".into(), "Done".into()),
                ]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            response.nodes.is_empty(),
            "exact ID must not bypass status: {text}"
        );
    }
}

#[test]
fn dev151_typed_id_exception_does_not_guess_case_or_ambiguous_numeric_identity() {
    let f = Fixture::new();
    f.write("REL-2", Default::default());
    f.write("ALT-2", Default::default());
    for text in ["2", "002", "rel-2", "rel-002", "rel2"] {
        let response = graph(
            &f.storage,
            &RelationshipGraphRequest {
                include_isolated: Some(false),
                filters: BTreeMap::from([("q".into(), text.into())]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(response.nodes.is_empty(), "must not guess {text}");
    }
    let scoped = graph(
        &f.storage,
        &RelationshipGraphRequest {
            project: Some("REL".into()),
            include_isolated: Some(false),
            filters: BTreeMap::from([("q".into(), "2".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(scoped.nodes.len(), 1);
    assert_eq!(scoped.nodes[0].id, "REL-2");
    let mut alt = f.task("ALT-2");
    alt.status = "Done".into();
    fs::write(
        f.storage.root_path.join("ALT/2.yml"),
        serde_yaml_ng::to_string(&alt).unwrap(),
    )
    .unwrap();
    let filtered = graph(
        &f.storage,
        &RelationshipGraphRequest {
            include_isolated: Some(false),
            filters: BTreeMap::from([("q".into(), "2".into()), ("status".into(), "Todo".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        filtered.nodes.is_empty(),
        "other predicates must not manufacture unique numeric identity"
    );
}

#[test]
fn dev151_selected_issue_repairs_exact_assertion_not_task_neighbors() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            related: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    f.write(
        "AA-3",
        TaskRelationships {
            blocks: vec!["AA-4".into()],
            ..Default::default()
        },
    );
    f.write("AA-4", Default::default());
    let before = [
        f.bytes("AA-1"),
        f.bytes("AA-2"),
        f.bytes("AA-3"),
        f.bytes("AA-4"),
    ];
    let issue = check(&f.storage, None)
        .unwrap()
        .issues
        .into_iter()
        .find(|i| i.message == "Missing AA-2.depends_on = AA-1")
        .unwrap();
    assert!(!issue.id.is_empty());
    assert!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap()
            .issues
            .iter()
            .any(|i| i.id == issue.id)
    );
    let request = RelationshipRepairRequest {
        issue_ids: vec![issue.id.clone(), issue.id.clone()],
        ..Default::default()
    };
    let preview = repair(&f.storage, &request).unwrap();
    assert_eq!(preview.changed_tasks, ["AA-2"]);
    assert_eq!(preview.issues.len(), 1);
    assert_eq!(
        before,
        [
            f.bytes("AA-1"),
            f.bytes("AA-2"),
            f.bytes("AA-3"),
            f.bytes("AA-4")
        ]
    );
    let unique = RelationshipRepairRequest {
        issue_ids: vec![issue.id],
        ..Default::default()
    };
    assert_eq!(
        preview.revision,
        repair(&f.storage, &unique).unwrap().revision
    );
    let applied = repair(
        &f.storage,
        &RelationshipRepairRequest {
            apply: true,
            revision: Some(preview.revision),
            ..unique
        },
    )
    .unwrap();
    assert_eq!(applied.changed_tasks, ["AA-2"]);
    assert!(applied.issues.is_empty());
    assert_eq!(f.task("AA-2").relationships.depends_on, ["AA-1"]);
    assert!(f.task("AA-2").relationships.related.is_empty());
    assert_eq!(before[0], f.bytes("AA-1"));
    assert_eq!(before[2], f.bytes("AA-3"));
    assert_eq!(before[3], f.bytes("AA-4"));
    assert_eq!(f.task("AA-2").history.len(), 1);
    assert_eq!(
        repair(&f.storage, &RelationshipRepairRequest::default())
            .unwrap()
            .changed_tasks,
        ["AA-2", "AA-4"]
    );
}

#[test]
fn dev151_selected_normalization_is_exact_and_revisions_bind_scope_and_inventory() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-02".into(), "AA-2".into()],
            related: vec!["AA-03".into(), "AA-3".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-3",
        TaskRelationships {
            related: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    f.write("BB-1", Default::default());
    let issues = check(&f.storage, None).unwrap().issues;
    let id = issues
        .iter()
        .find(|i| i.code == "duplicate_assertion" && i.message.contains(".blocks"))
        .unwrap()
        .id
        .clone();
    let request = RelationshipRepairRequest {
        project: Some("AA".into()),
        issue_ids: vec![id.clone()],
        ..Default::default()
    };
    let preview = repair(&f.storage, &request).unwrap();
    for wrong in [
        RelationshipRepairRequest {
            issue_ids: vec!["unknown".into()],
            ..request.clone()
        },
        RelationshipRepairRequest {
            project: Some("BB".into()),
            ..request.clone()
        },
    ] {
        assert!(repair(&f.storage, &wrong).is_err());
    }
    let other = issues
        .iter()
        .find(|i| i.code == "padded_alias" && i.message.contains(".blocks"))
        .unwrap()
        .id
        .clone();
    assert!(
        repair(
            &f.storage,
            &RelationshipRepairRequest {
                apply: true,
                revision: Some(preview.revision.clone()),
                issue_ids: vec![other],
                ..request.clone()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("scope")
    );
    assert!(
        repair(
            &f.storage,
            &RelationshipRepairRequest {
                apply: true,
                revision: Some(preview.revision.clone()),
                project: None,
                ..request.clone()
            }
        )
        .is_err()
    );
    f.write(
        "BB-1",
        TaskRelationships {
            fixes: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    assert!(
        repair(
            &f.storage,
            &RelationshipRepairRequest {
                apply: true,
                revision: Some(preview.revision),
                ..request.clone()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("stale")
    );
    let preview = repair(&f.storage, &request).unwrap();
    repair(
        &f.storage,
        &RelationshipRepairRequest {
            apply: true,
            revision: Some(preview.revision),
            ..request.clone()
        },
    )
    .unwrap();
    assert_eq!(f.task("AA-1").relationships.blocks, ["AA-02"]);
    assert!(
        check(&f.storage, None)
            .unwrap()
            .issues
            .iter()
            .any(|issue| issue.code == "padded_alias" && issue.message.contains(".blocks"))
    );
    assert_eq!(f.task("AA-1").relationships.related, ["AA-03", "AA-3"]);
    assert!(repair(&f.storage, &request).is_err());
}

#[test]
fn dev151_selected_unsafe_refused_and_same_root_cross_project_mirror_allowed() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["BB-1".into()],
            ..Default::default()
        },
    );
    f.write("BB-1", Default::default());
    f.write(
        "AA-2",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    let issues = check(&f.storage, None).unwrap().issues;
    for issue in issues.iter().filter(|i| !i.repairable) {
        assert!(
            repair(
                &f.storage,
                &RelationshipRepairRequest {
                    issue_ids: vec![issue.id.clone()],
                    ..Default::default()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("unsafe")
        );
    }
    let id = issues
        .iter()
        .find(|i| i.repairable && i.tasks.contains(&"BB-1".into()))
        .unwrap()
        .id
        .clone();
    let request = RelationshipRepairRequest {
        project: Some("AA".into()),
        issue_ids: vec![id],
        ..Default::default()
    };
    let preview = repair(&f.storage, &request).unwrap();
    assert_eq!(preview.changed_tasks, ["BB-1"]);
    repair(
        &f.storage,
        &RelationshipRepairRequest {
            apply: true,
            revision: Some(preview.revision),
            ..request
        },
    )
    .unwrap();
    assert_eq!(f.task("BB-1").relationships.depends_on, ["AA-1"]);
}

#[test]
fn dev151_selected_alias_normalizes_only_its_field_target() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            fixes: vec!["AA-02".into(), "AA-2".into(), "AA-03".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    f.write("AA-3", Default::default());
    let id = check(&f.storage, None)
        .unwrap()
        .issues
        .into_iter()
        .find(|i| i.code == "padded_alias" && i.tasks.contains(&"AA-2".into()))
        .unwrap()
        .id;
    let request = RelationshipRepairRequest {
        issue_ids: vec![id],
        ..Default::default()
    };
    let preview = repair(&f.storage, &request).unwrap();
    assert_eq!(preview.changed_tasks, ["AA-1"]);
    let response = repair(
        &f.storage,
        &RelationshipRepairRequest {
            apply: true,
            revision: Some(preview.revision),
            ..request
        },
    )
    .unwrap();
    assert!(response.issues.is_empty());
    assert_eq!(
        f.task("AA-1").relationships.fixes,
        ["AA-2", "AA-2", "AA-03"]
    );
    assert!(
        check(&f.storage, None)
            .unwrap()
            .issues
            .iter()
            .any(
                |issue| issue.code == "duplicate_assertion" && issue.tasks.contains(&"AA-2".into())
            )
    );
}

#[test]
fn dev151_selected_alias_and_duplicate_actions_are_order_independent() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            fixes: vec!["AA-02".into(), "AA-2".into(), "AA-03".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    f.write("AA-3", Default::default());
    let ids: Vec<_> = check(&f.storage, None)
        .unwrap()
        .issues
        .into_iter()
        .filter(|issue| issue.tasks.contains(&"AA-2".into()))
        .map(|issue| issue.id)
        .collect();
    assert_eq!(ids.len(), 2);
    let mut reversed = ids.clone();
    reversed.reverse();
    reversed.push(ids[0].clone());
    let inventory = snapshot_locks(&f.storage)
        .unwrap()
        .load(&f.storage)
        .unwrap();
    for issue_ids in [ids, reversed] {
        let plan = selected_repair_plan(
            &f.storage,
            &inventory,
            &RelationshipRepairRequest {
                issue_ids,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(plan["AA-1"].relationships.fixes, ["AA-2", "AA-03"]);
    }
    let batch = selected_repair_plan(
        &f.storage,
        &inventory,
        &RelationshipRepairRequest::default(),
    )
    .unwrap();
    assert_eq!(batch["AA-1"].relationships.fixes, ["AA-2", "AA-3"]);
}

#[test]
fn dev151_selected_missing_mirror_preserves_same_field_aliases_and_duplicates() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-03".into(), "AA-3".into(), "AA-04".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-3",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-4",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    let issues = check(&f.storage, None).unwrap().issues;
    let selected = issues
        .iter()
        .find(|issue| issue.code == "missing_mirror")
        .unwrap();
    let unselected: Vec<_> = issues
        .iter()
        .filter(|issue| issue.id != selected.id)
        .map(|issue| issue.id.clone())
        .collect();
    assert_eq!(unselected.len(), 3);
    let request = RelationshipRepairRequest {
        issue_ids: vec![selected.id.clone()],
        ..Default::default()
    };
    let preview = repair(&f.storage, &request).unwrap();
    assert_eq!(preview.changed_tasks, ["AA-2"]);
    let response = repair(
        &f.storage,
        &RelationshipRepairRequest {
            apply: true,
            revision: Some(preview.revision),
            ..request
        },
    )
    .unwrap();
    assert_eq!(response.changed_tasks, ["AA-2"]);
    assert!(response.issues.is_empty());
    assert_eq!(
        f.task("AA-2").relationships.depends_on,
        ["AA-03", "AA-3", "AA-04", "AA-1"]
    );
    let remaining = check(&f.storage, None).unwrap().issues;
    assert_eq!(remaining.len(), unselected.len());
    assert!(
        unselected
            .iter()
            .all(|id| remaining.iter().any(|issue| &issue.id == id))
    );
}

#[test]
fn dev151_connected_projection_ignores_recent_isolates_and_is_focus_superset() {
    let f = Fixture::new();
    for n in 100..210 {
        f.write(&format!("AA-{n}"), Default::default());
    }
    // A shortcut makes two root hops span five longest-path layout columns.
    for (n, targets) in [
        (54, vec![55, 58]),
        (55, vec![56]),
        (56, vec![57]),
        (57, vec![58]),
        (58, vec![]),
    ] {
        f.write(
            &format!("AA-{n}"),
            TaskRelationships {
                blocks: targets.into_iter().map(|n| format!("AA-{n}")).collect(),
                ..Default::default()
            },
        );
    }
    let request = RelationshipGraphRequest {
        project: Some("AA".into()),
        depth: Some(2),
        max_nodes: Some(100),
        relationship_kinds: Some(vec![RelationshipKind::Dependency]),
        include_isolated: Some(false),
        filters: BTreeMap::from([
            ("sort_by".into(), "id".into()),
            ("order".into(), "desc".into()),
        ]),
        ..Default::default()
    };
    let all = graph(&f.storage, &request).unwrap();
    assert_eq!(all.nodes.len(), 5);
    assert!(!all.truncated);
    let focus = graph(
        &f.storage,
        &RelationshipGraphRequest {
            task_id: Some("AA-54".into()),
            ..request.clone()
        },
    )
    .unwrap();
    assert_eq!(focus.nodes.len(), 5);
    assert!(
        focus
            .nodes
            .iter()
            .all(|n| all.nodes.iter().any(|a| a.id == n.id))
    );
    let none = graph(
        &f.storage,
        &RelationshipGraphRequest {
            relationship_kinds: Some(vec![]),
            ..request.clone()
        },
    )
    .unwrap();
    assert!(none.nodes.is_empty());
    let focus_zero = graph(
        &f.storage,
        &RelationshipGraphRequest {
            task_id: Some("AA-54".into()),
            depth: Some(0),
            ..request.clone()
        },
    )
    .unwrap();
    assert_eq!(focus_zero.nodes.len(), 1);
    assert!(focus_zero.edges.is_empty());
    let legacy = graph(
        &f.storage,
        &RelationshipGraphRequest {
            max_nodes: Some(500),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(legacy.nodes.len(), 115);
    let exact = graph(
        &f.storage,
        &RelationshipGraphRequest {
            filters: BTreeMap::from([("q".into(), "AA-100".into())]),
            ..request
        },
    )
    .unwrap();
    assert!(exact.nodes.iter().any(|n| n.id == "AA-100"));
}

#[test]
fn dev151_pair_allocation_does_not_spend_bound_on_disjoint_orphans() {
    let f = Fixture::new();
    for n in 1..=5 {
        f.write(
            &format!("AA-{n}"),
            TaskRelationships {
                blocks: vec![format!("BB-{n}")],
                ..Default::default()
            },
        );
        f.write(&format!("BB-{n}"), Default::default());
    }
    for limit in [1, 2, 3, 5, 9, 10] {
        let response = graph(
            &f.storage,
            &RelationshipGraphRequest {
                project: Some("AA".into()),
                max_nodes: Some(limit),
                include_isolated: Some(false),
                relationship_kinds: Some(vec![RelationshipKind::Dependency]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(response.nodes.len() <= limit);
        assert_eq!(response.nodes.len(), (limit / 2) * 2);
        assert_eq!(response.edges.len(), limit / 2);
        assert_eq!(response.truncated, limit < 10);
        assert!(
            response
                .nodes
                .iter()
                .filter(|n| n.project == "BB")
                .all(|n| !n.matches_filter)
        );
    }
}

#[test]
fn dev151_selected_kinds_keep_boundary_diagnostics_and_external_context_read_only() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["BB-1".into(), "AA-2".into(), "AA-99".into()],
            related: vec!["AA-3".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    f.write("AA-3", Default::default());
    let mut deleted = f.task("AA-2");
    deleted.deleted_at = Some("2026-01-01T00:00:00Z".into());
    fs::write(
        f.storage.root_path.join("AA/2.yml"),
        serde_yaml_ng::to_string(&deleted).unwrap(),
    )
    .unwrap();
    let external = f.storage.root_path.parent().unwrap().join("sibling/.tasks");
    fs::create_dir_all(external.join("BB")).unwrap();
    fs::write(
        external.join("BB/1.yml"),
        serde_yaml_ng::to_string(&Task {
            title: "External".into(),
            ..Default::default()
        })
        .unwrap(),
    )
    .unwrap();
    drop(MultiFileTransaction::begin_all(&external, &[]).unwrap());
    let before = fs::read(external.join("BB/1.yml")).unwrap();
    let request = RelationshipGraphRequest {
        project: Some("AA".into()),
        relationship_kinds: Some(vec![RelationshipKind::Dependency]),
        include_isolated: Some(false),
        filters: BTreeMap::from([("q".into(), "AA-1".into())]),
        ..Default::default()
    };
    let response = graph(&f.storage, &request).unwrap();
    assert_eq!(response.nodes.len(), 4);
    assert_eq!(response.edges.len(), 3);
    assert!(!response.complete);
    assert!(!response.truncated);
    for (id, state) in [
        ("BB-1", "external"),
        ("AA-2", "deleted"),
        ("AA-99", "missing"),
    ] {
        let node = response.nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!(node.state, state);
        assert!(!node.matches_filter);
    }
    let external_issue = response
        .issues
        .iter()
        .find(|issue| issue.code == "missing_mirror" && issue.tasks.contains(&"BB-1".into()))
        .unwrap();
    assert!(!external_issue.repairable);
    assert!(
        repair(
            &f.storage,
            &RelationshipRepairRequest {
                issue_ids: vec![external_issue.id.clone()],
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(before, fs::read(external.join("BB/1.yml")).unwrap());
    let no_kinds = graph(
        &f.storage,
        &RelationshipGraphRequest {
            relationship_kinds: Some(vec![]),
            task_id: Some("AA-1".into()),
            ..request
        },
    )
    .unwrap();
    assert_eq!(no_kinds.nodes.len(), 1);
    assert!(no_kinds.edges.is_empty());
    // Kind selection is projection only: integrity problems do not disappear.
    assert!(no_kinds.issues.iter().any(|issue| issue.code == "missing"));
    assert!(!no_kinds.complete);
}

#[test]
fn snapshot_readers_overlap_and_exclude_exclusive_writers() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        for _ in 0..3 {
            let barrier = &barrier;
            let storage = &f.storage;
            scope.spawn(move || {
                let snapshot = snapshot_locks(storage);
                barrier.wait();
                barrier.wait();
                assert!(snapshot.unwrap().load(storage).is_ok());
            });
        }
        barrier.wait();
        let mut excluded = Vec::new();
        for path in ["@sprints/.sprints.lock", "AA/.task.lock"] {
            let file = File::options()
                .write(true)
                .open(f.storage.root_path.join(path))
                .unwrap();
            excluded.push(
                FileExt::try_lock_exclusive(&file)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock),
            );
        }
        barrier.wait();
        assert!(excluded.into_iter().all(|value| value));
    });
    assert!(MultiFileTransaction::begin_all(&f.storage.root_path, &[]).is_ok());
}

#[test]
fn graph_check_and_repair_preview_share_snapshot_without_task_data_writes() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    let before = [f.bytes("AA-1"), f.bytes("AA-2")];
    let config = fs::read(f.storage.root_path.join("config.yml")).unwrap();
    let _reader = snapshot_locks(&f.storage).unwrap();
    assert_eq!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap()
            .edges
            .len(),
        1
    );
    assert!(check(&f.storage, None).unwrap().complete);
    let preview = repair(
        &f.storage,
        &RelationshipRepairRequest {
            project: None,
            apply: false,
            revision: None,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!preview.applied);
    assert_eq!(preview.changed_tasks, ["AA-2"]);
    assert_eq!(before, [f.bytes("AA-1"), f.bytes("AA-2")]);
    assert_eq!(
        config,
        fs::read(f.storage.root_path.join("config.yml")).unwrap()
    );
    assert!(!f.storage.root_path.join(".txn-pending.json").exists());
}

#[test]
fn snapshot_retry_releases_partial_set_and_reads_only_finished_publication() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    // A project-only writer lets the reader acquire the coordinator first.
    let writer =
        crate::storage::safety::acquire_storage_lock(&f.storage.root_path.join("AA"), "task")
            .unwrap();
    let (waiting_tx, waiting_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let storage = &f.storage;
        let reader = scope.spawn(move || {
            let snapshot = snapshot_locks_with_wait(storage, |_| {
                waiting_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
            snapshot.load(storage).unwrap()
        });
        waiting_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let coordinator = File::options()
            .write(true)
            .open(f.storage.root_path.join("@sprints/.sprints.lock"))
            .unwrap();
        FileExt::try_lock_exclusive(&coordinator)
            .expect("retry must release the partial coordinator lock");
        let mut first = f.task("AA-1");
        first.relationships.blocks = vec!["AA-2".into()];
        let mut second = f.task("AA-2");
        second.relationships.depends_on = vec!["AA-1".into()];
        fs::write(
            f.storage.root_path.join("AA/1.yml"),
            serde_yaml_ng::to_string(&first).unwrap(),
        )
        .unwrap();
        fs::write(
            f.storage.root_path.join("AA/2.yml"),
            serde_yaml_ng::to_string(&second).unwrap(),
        )
        .unwrap();
        drop(writer);
        drop(coordinator);
        resume_tx.send(()).unwrap();
        let inventory = reader.join().unwrap();
        assert_eq!(
            inventory.records["AA-1"][0]
                .task
                .as_ref()
                .unwrap()
                .relationships
                .blocks,
            ["AA-2"]
        );
        assert_eq!(
            inventory.records["AA-2"][0]
                .task
                .as_ref()
                .unwrap()
                .relationships
                .depends_on,
            ["AA-1"]
        );
    });
}

#[test]
fn snapshot_unsupported_and_permission_errors_are_not_contention() {
    let temp = tempfile::tempdir().unwrap();
    for kind in [
        std::io::ErrorKind::Unsupported,
        std::io::ErrorKind::PermissionDenied,
    ] {
        let mut busy = false;
        let error = read_lock_with(&temp.path().join("lock"), &mut busy, |_| {
            Err(std::io::Error::from(kind))
        })
        .unwrap_err();
        assert!(!busy);
        assert!(error.to_string().contains("unavailable"));
    }
}

#[test]
fn snapshot_nonregular_and_symlink_locks_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock");
    fs::create_dir(&path).unwrap();
    assert!(
        read_lock(&path, &mut false)
            .unwrap_err()
            .to_string()
            .contains("nonregular")
    );
    #[cfg(unix)]
    {
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(
            read_lock(&link, &mut false)
                .unwrap_err()
                .to_string()
                .contains("symlink")
        );
    }
}

#[test]
fn external_snapshot_contention_is_fail_fast_and_releases_partial_roots() {
    let f = Fixture::new();
    let first = f.storage.root_path.parent().unwrap().join("first/.tasks");
    let second = f.storage.root_path.parent().unwrap().join("second/.tasks");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(second.join("@sprints")).unwrap();
    let _local = MultiFileTransaction::begin_all(&f.storage.root_path, &[]).unwrap();
    let _external = MultiFileTransaction::begin_all(&second, &[]).unwrap();
    assert!(external_locks(&f.storage).is_err());
    let released = File::options()
        .write(true)
        .open(first.join("@sprints/.sprints.lock"))
        .unwrap();
    FileExt::try_lock_exclusive(&released).unwrap();
}

#[test]
fn graph_filters_select_before_bound_and_preserve_cross_project_context() {
    let f = Fixture::new();
    for number in 1..=105 {
        f.write(&format!("AA-{number}"), Default::default());
    }
    f.write("BB-1", Default::default());
    f.write(
        "AA-105",
        TaskRelationships {
            depends_on: vec!["BB-1".into()],
            ..Default::default()
        },
    );
    let before = f.bytes("AA-105");
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            project: Some("AA".into()),
            filters: BTreeMap::from([("q".into(), "AA-105".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(response.nodes.len(), 2);
    assert!(
        response
            .nodes
            .iter()
            .find(|n| n.id == "AA-105")
            .unwrap()
            .matches_filter
    );
    assert!(
        !response
            .nodes
            .iter()
            .find(|n| n.id == "BB-1")
            .unwrap()
            .matches_filter
    );
    assert_eq!(response.edges.len(), 1);
    assert!(!response.truncated);
    assert_eq!(before, f.bytes("AA-105"));
}

#[test]
fn graph_filters_keep_filtered_out_focus_and_deleted_context_explicit() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            task_id: Some("AA-1".into()),
            filters: BTreeMap::from([("status".into(), "done".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(response.nodes.len(), 2);
    assert!(response.nodes.iter().all(|n| !n.matches_filter));
}

#[test]
fn graph_filters_reject_invalid_explicit_values_including_empty_inventory() {
    let f = Fixture::new();
    for (key, value) in [
        ("status", "garbage"),
        ("priority", "garbage"),
        ("type", "garbage"),
        ("sprints", "1,,2"),
        ("sprints", "0"),
        ("due", "yesterday"),
        ("recent", "1d"),
        ("needs", "nonsense"),
        ("deletion", "ALL"),
        ("sort_by", "garbage"),
        ("order", "sideways"),
    ] {
        assert!(
            graph(
                &f.storage,
                &RelationshipGraphRequest {
                    filters: BTreeMap::from([(key.into(), value.into())]),
                    ..Default::default()
                }
            )
            .is_err(),
            "{key}={value}"
        );
    }
}

#[test]
fn graph_summary_is_compact_normalized_and_does_not_mutate_sources() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let path = f.storage.root_path.join("AA/1.yml");
    let mut task = f.task("AA-1");
    task.description = Some("large private description".into());
    task.assignee = Some("Alice-Smith".into());
    task.tags = vec!["Backend".into()];
    task.custom_fields.insert("Team".into(), "Platform".into());
    fs::write(&path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
    let before = fs::read(&path).unwrap();
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            filters: BTreeMap::from([
                ("status".into(), "todo".into()),
                ("priority".into(), "medium".into()),
                ("type".into(), "feature".into()),
                ("assignee".into(), "@alice_smith".into()),
                ("tags".into(), "back".into()),
                ("field:Team".into(), "platform".into()),
            ]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(response.nodes.len(), 1);
    let summary = serde_json::to_value(response.nodes[0].task.as_ref().unwrap()).unwrap();
    assert_eq!(summary["modified"], summary["created"]);
    assert_eq!(summary["custom_fields"]["Team"], "Platform");
    assert!(summary.get("task_state").is_some());
    for field in [
        "description",
        "history",
        "comments",
        "references",
        "relationships",
        "acceptance_criteria",
    ] {
        assert!(summary.get(field).is_none(), "{field}");
    }
    assert_eq!(before, fs::read(path).unwrap());
}

#[test]
fn graph_filter_order_is_applied_before_node_limit() {
    let f = Fixture::new();
    for id in ["AA-1", "AA-2", "AA-3"] {
        f.write(id, Default::default());
    }
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            max_nodes: Some(1),
            filters: BTreeMap::from([
                ("sort_by".into(), "id".into()),
                ("order".into(), "desc".into()),
            ]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(response.nodes[0].id, "AA-3");
    assert!(response.truncated);
    assert!(!response.complete);
}

#[test]
fn graph_filters_share_identity_dates_sprints_and_needs_semantics() {
    let f = Fixture::new();
    fs::write(f.storage.root_path.join("config.yml"), "issue:\n  states: [Todo, Done]\n  priorities: [Medium]\n  types: [Feature]\ndefault.reporter: Alice-Smith\nauto:\n  set_reporter: false\n").unwrap();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    let mut task = f.task("AA-1");
    task.assignee = Some("alice_smith".into());
    task.due_date = Some(chrono::Local::now().date_naive().to_string());
    task.modified = chrono::Utc::now().to_rfc3339();
    fs::write(
        f.storage.root_path.join("AA/1.yml"),
        serde_yaml_ng::to_string(&task).unwrap(),
    )
    .unwrap();
    fs::write(
        f.storage.root_path.join("@sprints/1.yml"),
        "tasks:\n  - id: AA-001\n    order: 3\n",
    )
    .unwrap();
    let before = f.bytes("AA-1");
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            project: Some("AA".into()),
            filters: BTreeMap::from([
                ("assignee".into(), "@ME".into()),
                ("due".into(), "TODAY".into()),
                ("recent".into(), "7d".into()),
                ("needs".into(), "effort".into()),
                ("sprints".into(), "1".into()),
            ]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(response.nodes.len(), 1);
    assert_eq!(response.nodes[0].id, "AA-1");
    assert_eq!(response.nodes[0].task.as_ref().unwrap().sprints, vec![1]);
    assert_eq!(before, f.bytes("AA-1"));
    for (key, value) in [("due", "overdue"), ("sprints", "2"), ("needs", "due")] {
        let response = graph(
            &f.storage,
            &RelationshipGraphRequest {
                filters: BTreeMap::from([("q".into(), "AA-1".into()), (key.into(), value.into())]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(response.nodes.is_empty(), "{key}={value}");
    }
}

#[test]
fn graph_external_summary_uses_actual_root_defaults_sprints_and_project_validation() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["BB-1".into()],
            ..Default::default()
        },
    );
    let root = f.storage.root_path.parent().unwrap().join("nested/.tasks");
    fs::create_dir_all(root.join("BB")).unwrap();
    fs::create_dir_all(root.join("@sprints")).unwrap();
    fs::write(root.join("config.yml"), "issue.states: [Ready, Shipped]\nissue.priorities: [Critical]\nissue.types: [Epic]\ndefault.status: Ready\ndefault.priority: Critical\ndefault.type: Epic\n").unwrap();
    let task = Task {
        title: "external".into(),
        status: "".into(),
        priority: "".into(),
        task_type: "".into(),
        ..Default::default()
    };
    fs::write(
        root.join("BB/1.yml"),
        serde_yaml_ng::to_string(&task).unwrap(),
    )
    .unwrap();
    fs::write(root.join("@sprints/7.yml"), "tasks:\n  - id: BB-1\n").unwrap();
    let before = fs::read(root.join("BB/1.yml")).unwrap();
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            project: Some("AA".into()),
            filters: BTreeMap::from([("q".into(), "AA-1".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    let external = response.nodes.iter().find(|n| n.id == "BB-1").unwrap();
    assert_eq!(external.state, "external");
    assert!(!external.matches_filter);
    let summary = external.task.as_ref().unwrap();
    assert_eq!(summary.status.as_str(), "Ready");
    assert_eq!(summary.priority.as_str(), "Critical");
    assert_eq!(summary.task_type.as_str(), "Epic");
    assert_eq!(summary.sprints, vec![7]);
    // Explicit sibling-project enum values validate against the sibling root,
    // not against the unrelated local Todo/Done configuration.
    assert!(
        graph(
            &f.storage,
            &RelationshipGraphRequest {
                project: Some("BB".into()),
                filters: BTreeMap::from([("status".into(), "ready".into())]),
                ..Default::default()
            }
        )
        .is_ok()
    );
    assert_eq!(before, fs::read(root.join("BB/1.yml")).unwrap());
}

#[test]
fn graph_deletion_filters_retain_referenced_tombstones_without_claiming_a_match() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["AA-2".into(), "AA-99".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    let mut deleted = f.task("AA-2");
    deleted.deleted_at = Some("2026-01-02T00:00:00Z".into());
    fs::write(
        f.storage.root_path.join("AA/2.yml"),
        serde_yaml_ng::to_string(&deleted).unwrap(),
    )
    .unwrap();
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            filters: BTreeMap::from([("deletion".into(), "active".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    let tombstone = response
        .nodes
        .iter()
        .find(|node| node.id == "AA-2")
        .unwrap();
    assert_eq!(tombstone.state, "deleted");
    assert!(!tombstone.matches_filter);
    assert!(!tombstone.is_done);
    let missing = response
        .nodes
        .iter()
        .find(|node| node.id == "AA-99")
        .unwrap();
    assert!(!missing.matches_filter);
    assert!(missing.task.is_none());
    assert!(!response.complete);
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            depth: Some(0),
            filters: BTreeMap::from([("deletion".into(), "deleted".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        response
            .nodes
            .iter()
            .any(|node| node.id == "AA-2" && node.matches_filter)
    );
    assert!(
        response
            .nodes
            .iter()
            .any(|node| node.id == "AA-1" && !node.matches_filter)
    );
}

struct Fixture {
    _temp: tempfile::TempDir,
    storage: Storage,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace/.tasks");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("config.yml"), "issue:\n  states: [Todo, Done]\n  priorities: [Medium]\n  types: [Feature]\nauto:\n  set_reporter: false\n").unwrap();
        let storage = Storage { root_path: root };
        Self {
            _temp: temp,
            storage,
        }
    }

    fn write(&self, id: &str, rel: TaskRelationships) {
        let parsed = TaskId::parse(id).unwrap();
        let dir = self.storage.root_path.join(&parsed.project);
        fs::create_dir_all(&dir).unwrap();
        let task = Task {
            title: id.into(),
            status: "Todo".into(),
            priority: "Medium".into(),
            task_type: "Feature".into(),
            created: "2026-01-01T00:00:00Z".into(),
            relationships: rel,
            ..Task::default()
        };
        fs::write(
            dir.join(format!("{}.yml", parsed.number)),
            serde_yaml_ng::to_string(&task).unwrap(),
        )
        .unwrap();
        // Initialize participating-writer locks as fixture setup, not as an
        // inspection side effect. No task data is changed by this transaction.
        drop(MultiFileTransaction::begin_all(&self.storage.root_path, &[]).unwrap());
    }

    fn task(&self, id: &str) -> Task {
        Inventory::load(&self.storage)
            .unwrap()
            .unique(id)
            .unwrap()
            .task
            .clone()
            .unwrap()
    }

    fn bytes(&self, id: &str) -> Vec<u8> {
        fs::read(
            &Inventory::load(&self.storage)
                .unwrap()
                .unique(id)
                .unwrap()
                .path,
        )
        .unwrap()
    }

    fn operation(
        &self,
        source: &str,
        target: &str,
        kind: RelationshipKind,
        operation: RelationshipOperation,
    ) -> LoTaRResult<RelationshipMutationResponse> {
        mutate(
            &self.storage,
            &RelationshipMutationRequest {
                source: source.into(),
                target: target.into(),
                kind: match kind {
                    RelationshipKind::Dependency => RelationshipField::Blocks,
                    RelationshipKind::Hierarchy => RelationshipField::Children,
                    RelationshipKind::Related => RelationshipField::Related,
                    RelationshipKind::Fixes => RelationshipField::Fixes,
                    RelationshipKind::Duplicate => RelationshipField::DuplicateOf,
                },
                operation,
                from_kind: None,
                expected_current: None,
            },
        )
    }
}

#[test]
fn dependency_reciprocity_remove_and_change_kind_are_atomic() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("BB-1", Default::default());
    let result = f
        .operation(
            "AA-001",
            "BB-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add,
        )
        .unwrap();
    assert_eq!(result.changed_tasks, ["AA-1", "BB-1"]);
    assert_eq!(f.task("AA-1").relationships.blocks, ["BB-1"]);
    assert_eq!(f.task("BB-1").relationships.depends_on, ["AA-1"]);
    assert_eq!(f.task("BB-1").history.len(), 1);
    mutate(
        &f.storage,
        &RelationshipMutationRequest {
            source: "AA-1".into(),
            target: "BB-1".into(),
            kind: RelationshipField::Related,
            operation: RelationshipOperation::ChangeKind,
            from_kind: Some(RelationshipField::Blocks),
            expected_current: None,
        },
    )
    .unwrap();
    assert!(f.task("AA-1").relationships.blocks.is_empty());
    assert!(f.task("BB-1").relationships.depends_on.is_empty());
    assert_eq!(f.task("AA-1").relationships.related, ["BB-1"]);
    assert_eq!(f.task("BB-1").relationships.related, ["AA-1"]);
    f.operation(
        "BB-1",
        "AA-1",
        RelationshipKind::Related,
        RelationshipOperation::Remove,
    )
    .unwrap();
    assert!(f.task("AA-1").relationships.is_empty());
    assert!(f.task("BB-1").relationships.is_empty());
}

#[test]
fn conversion_to_symmetric_kind_preserves_original_direction_for_removal() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    f.operation(
        "AA-2",
        "AA-1",
        RelationshipKind::Dependency,
        RelationshipOperation::Add,
    )
    .unwrap();
    mutate(
        &f.storage,
        &RelationshipMutationRequest {
            source: "AA-2".into(),
            target: "AA-1".into(),
            kind: RelationshipField::Related,
            operation: RelationshipOperation::ChangeKind,
            from_kind: Some(RelationshipField::Blocks),
            expected_current: None,
        },
    )
    .unwrap();
    assert!(f.task("AA-2").relationships.blocks.is_empty());
    assert!(f.task("AA-1").relationships.depends_on.is_empty());
}

#[test]
fn removal_clears_both_legacy_assertions_and_aliases() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-002".into(), "AA-2".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-001".into()],
            ..Default::default()
        },
    );
    f.operation(
        "AA-1",
        "AA-2",
        RelationshipKind::Dependency,
        RelationshipOperation::Remove,
    )
    .unwrap();
    assert!(f.task("AA-1").relationships.is_empty());
    assert!(f.task("AA-2").relationships.is_empty());
}

#[test]
fn dependency_hierarchy_duplicate_cycles_are_separate_and_parent_guard_is_fresh() {
    let f = Fixture::new();
    for id in ["AA-1", "AA-2", "AA-3"] {
        f.write(id, Default::default());
    }
    f.operation(
        "AA-1",
        "AA-2",
        RelationshipKind::Dependency,
        RelationshipOperation::Add,
    )
    .unwrap();
    f.operation(
        "AA-2",
        "AA-1",
        RelationshipKind::Hierarchy,
        RelationshipOperation::Add,
    )
    .unwrap();
    assert!(
        f.operation(
            "AA-2",
            "AA-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .is_err()
    );
    assert!(
        f.operation(
            "AA-1",
            "AA-2",
            RelationshipKind::Hierarchy,
            RelationshipOperation::Add
        )
        .is_err()
    );
    f.operation(
        "AA-1",
        "AA-2",
        RelationshipKind::Duplicate,
        RelationshipOperation::Add,
    )
    .unwrap();
    assert!(
        f.operation(
            "AA-2",
            "AA-1",
            RelationshipKind::Duplicate,
            RelationshipOperation::Add
        )
        .is_err()
    );
    assert!(
        f.operation(
            "AA-3",
            "AA-1",
            RelationshipKind::Hierarchy,
            RelationshipOperation::Add
        )
        .is_err()
    );
    let replace = RelationshipMutationRequest {
        source: "AA-3".into(),
        target: "AA-1".into(),
        kind: RelationshipField::Children,
        operation: RelationshipOperation::Add,
        from_kind: None,
        expected_current: Some("AA-2".into()),
    };
    mutate(&f.storage, &replace).unwrap();
    assert_eq!(f.task("AA-1").relationships.parent.as_deref(), Some("AA-3"));
    assert!(f.task("AA-2").relationships.children.is_empty());
    assert!(mutate(&f.storage, &replace).is_err());
}

#[test]
fn physical_alias_conflicts_and_exact_case_are_not_silently_resolved() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    assert!(
        f.operation(
            "aa-1",
            "AA-2",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .is_err()
    );
    let path = f.storage.root_path.join("AA/001.yml");
    fs::write(&path, f.bytes("AA-1")).unwrap();
    let report = check(&f.storage, None).unwrap();
    assert!(!report.complete);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "ambiguous" && i.tasks == ["AA-1"])
    );
    assert!(
        f.operation(
            "AA-001",
            "AA-2",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .is_err()
    );
    assert!(
        repair(&f.storage, &RelationshipRepairRequest::default())
            .unwrap()
            .changed_tasks
            .is_empty()
    );
}

#[test]
fn strict_graph_distinguishes_missing_deleted_and_corrupt() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["AA-2".into(), "AA-3".into(), "AA-4".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    f.write("AA-3", Default::default());
    let mut deleted = f.task("AA-2");
    deleted.deleted_at = Some("2026-01-01T00:00:00Z".into());
    fs::write(
        f.storage.root_path.join("AA/2.yml"),
        serde_yaml_ng::to_string(&deleted).unwrap(),
    )
    .unwrap();
    fs::write(f.storage.root_path.join("AA/3.yml"), "not: [valid").unwrap();
    let response = graph(&f.storage, &RelationshipGraphRequest::default()).unwrap();
    for (id, state) in [
        ("AA-2", "deleted"),
        ("AA-3", "unreadable"),
        ("AA-4", "missing"),
    ] {
        assert_eq!(
            response.nodes.iter().find(|n| n.id == id).unwrap().state,
            state
        );
    }
    assert!(!response.complete);
    assert!(response.issues.iter().all(|i| !i.repairable));
    assert_eq!(
        blocked_dependencies(&f.storage, "AA-1", &[])
            .unwrap_err()
            .to_string(),
        "Validation error: Dependency graph is incomplete or ambiguous"
    );
}

#[test]
fn safe_repair_preview_is_stale_protected_and_preserves_unrelated_data_and_order() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-003".into(), "AA-3".into(), "AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    f.write("AA-3", Default::default());
    let preview = repair(&f.storage, &RelationshipRepairRequest::default()).unwrap();
    assert!(!preview.applied);
    assert_eq!(preview.changed_tasks, ["AA-1", "AA-2", "AA-3"]);
    let unchanged = f.bytes("AA-1");
    let mut task = f.task("AA-2");
    task.description = Some("keep me".into());
    fs::write(
        f.storage.root_path.join("AA/2.yml"),
        serde_yaml_ng::to_string(&task).unwrap(),
    )
    .unwrap();
    let request = RelationshipRepairRequest {
        apply: true,
        revision: Some(preview.revision),
        project: None,
        ..Default::default()
    };
    assert!(
        repair(&f.storage, &request)
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
    assert_eq!(f.bytes("AA-1"), unchanged);
    let fresh = repair(&f.storage, &RelationshipRepairRequest::default()).unwrap();
    repair(
        &f.storage,
        &RelationshipRepairRequest {
            revision: Some(fresh.revision),
            ..request
        },
    )
    .unwrap();
    assert_eq!(f.task("AA-1").relationships.blocks, ["AA-3", "AA-2"]);
    assert_eq!(f.task("AA-2").description.as_deref(), Some("keep me"));
    assert!(check(&f.storage, None).unwrap().issues.is_empty());
    assert!(
        repair(&f.storage, &RelationshipRepairRequest::default())
            .unwrap()
            .changed_tasks
            .is_empty()
    );
}

#[test]
fn no_op_add_does_not_implicitly_repair_one_sided_edge_or_bytes() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    let a = f.bytes("AA-1");
    let b = f.bytes("AA-2");
    assert!(
        !f.operation(
            "AA-1",
            "AA-2",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .unwrap()
        .changed
    );
    assert_eq!(f.bytes("AA-1"), a);
    assert_eq!(f.bytes("AA-2"), b);
}

#[test]
fn cross_project_publish_failure_rolls_back_every_byte_and_history() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("BB-1", Default::default());
    let a = f.bytes("AA-1");
    let b = f.bytes("BB-1");
    crate::storage::transaction::fault::fail_publish_at(1);
    assert!(
        f.operation(
            "AA-1",
            "BB-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .unwrap_err()
        .to_string()
        .contains("injected publish failure")
    );
    assert_eq!(f.bytes("AA-1"), a);
    assert_eq!(f.bytes("BB-1"), b);
    assert!(!f.storage.root_path.join(".txn-pending.json").exists());
}

#[test]
fn pending_journal_checker_is_unavailable_and_does_not_recover_or_write() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let journal = f.storage.root_path.join(".txn-pending.json");
    fs::write(&journal, "invalid journal retained").unwrap();
    let before = f.bytes("AA-1");
    assert!(
        check(&f.storage, None)
            .unwrap_err()
            .to_string()
            .contains("pending transaction")
    );
    assert!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap_err()
            .to_string()
            .contains("pending transaction")
    );
    assert!(
        repair(
            &f.storage,
            &RelationshipRepairRequest {
                project: None,
                apply: false,
                revision: None,
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("pending transaction")
    );
    assert_eq!(
        fs::read_to_string(&journal).unwrap(),
        "invalid journal retained"
    );
    assert_eq!(f.bytes("AA-1"), before);
}

#[test]
fn fresh_checkout_inspection_initializes_only_locks_without_task_or_config_writes() {
    let f = Fixture::new();
    let project = f.storage.root_path.join("AA");
    fs::create_dir(&project).unwrap();
    let bytes = b"# handwritten task\ntitle: raw task\ncreated: 2026-01-01T00:00:00Z\n";
    fs::write(project.join("1.yml"), bytes).unwrap();
    let config_before = fs::read(f.storage.root_path.join("config.yml")).unwrap();
    assert!(check(&f.storage, None).unwrap().complete);
    assert_eq!(fs::read(project.join("1.yml")).unwrap(), bytes);
    assert_eq!(
        fs::read(f.storage.root_path.join("config.yml")).unwrap(),
        config_before
    );
    assert!(f.storage.root_path.join("@sprints/.sprints.lock").is_file());
    assert!(project.join(".task.lock").is_file());
    assert!(!f.storage.root_path.join(".txn-pending.json").exists());
}

#[test]
fn graph_snapshot_does_not_observe_partially_published_or_locked_graph() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let transaction = MultiFileTransaction::begin_all(&f.storage.root_path, &[]).unwrap();
    let error = check(&f.storage, None).unwrap_err().to_string();
    assert!(error.contains("unavailable"));
    assert!(error.contains("500 ms snapshot wait budget"));
    drop(transaction);
    assert!(check(&f.storage, None).unwrap().complete);
}

#[test]
fn concurrent_opposite_edges_cannot_both_pass_cycle_validation() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("BB-1", Default::default());
    let root = f.storage.root_path.clone();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [("AA-1", "BB-1"), ("BB-1", "AA-1")]
        .into_iter()
        .map(|(a, b)| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                mutate(
                    &Storage { root_path: root },
                    &RelationshipMutationRequest {
                        source: a.into(),
                        target: b.into(),
                        kind: RelationshipField::Blocks,
                        operation: RelationshipOperation::Add,
                        from_kind: None,
                        expected_current: None,
                    },
                )
            })
        })
        .collect();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|o| o.is_ok()).count(), 1);
    assert!(
        !check(&f.storage, None)
            .unwrap()
            .issues
            .iter()
            .any(|i| i.code == "cycle")
    );
}

#[test]
fn normal_whole_map_update_uses_reciprocal_planner_and_metadata_noop_is_preserved() {
    let mut f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("BB-1", Default::default());
    TaskService::update_with_context(
        &mut f.storage,
        "AA-1",
        TaskUpdate {
            relationships: Some(TaskRelationships {
                blocks: vec!["BB-1".into()],
                ..Default::default()
            }),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .unwrap();
    assert_eq!(f.task("BB-1").relationships.depends_on, ["AA-1"]);
    let before = f.bytes("BB-1");
    TaskService::update_with_context(
        &mut f.storage,
        "AA-1",
        TaskUpdate {
            title: Some("renamed".into()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .unwrap();
    assert_eq!(f.bytes("BB-1"), before);
    TaskService::update_with_context(
        &mut f.storage,
        "AA-1",
        TaskUpdate {
            relationships: Some(Default::default()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .unwrap();
    assert!(f.task("BB-1").relationships.depends_on.is_empty());
}

#[test]
fn incoming_blocks_is_effective_before_repair_and_own_project_done_policy_applies() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["BB-1".into()],
            ..Default::default()
        },
    );
    f.write("BB-1", Default::default());
    assert_eq!(
        blocked_dependencies(&f.storage, "BB-1", &[]).unwrap(),
        ["AA-1"]
    );
    let mut prerequisite = f.task("AA-1");
    prerequisite.status = "Done".into();
    fs::write(
        f.storage.root_path.join("AA/1.yml"),
        serde_yaml_ng::to_string(&prerequisite).unwrap(),
    )
    .unwrap();
    assert!(
        blocked_dependencies(&f.storage, "BB-1", &[])
            .unwrap()
            .is_empty()
    );
    fs::write(
        f.storage.root_path.join("AA/config.yml"),
        "issue:\n  states: [Todo, Done, Shipped]\n  done_states: [Shipped]\n",
    )
    .unwrap();
    assert_eq!(
        blocked_dependencies(&f.storage, "BB-1", &[]).unwrap(),
        ["AA-1"]
    );
}

#[test]
fn normal_create_stages_mirrors_and_missing_targets_abort_creation() {
    let mut f = Fixture::new();
    f.write("AA-1", Default::default());
    let dto = TaskService::create(
        &mut f.storage,
        TaskCreate {
            title: "new dependent".into(),
            project: Some("BB".into()),
            relationships: Some(TaskRelationships {
                depends_on: vec!["AA-1".into()],
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(dto.id, "BB-1");
    assert_eq!(f.task("AA-1").relationships.blocks, ["BB-1"]);
    let before = f.bytes("AA-1");
    assert!(
        TaskService::create(
            &mut f.storage,
            TaskCreate {
                title: "invalid".into(),
                project: Some("BB".into()),
                relationships: Some(TaskRelationships {
                    depends_on: vec!["AA-99".into()],
                    ..Default::default()
                }),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(f.bytes("AA-1"), before);
    assert!(!f.storage.root_path.join("BB/2.yml").exists());
}

#[test]
fn legacy_cycles_allow_metadata_edits_and_edge_removal_but_not_new_self_links() {
    let mut f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    let before = f.bytes("AA-2");
    TaskService::update_with_context(
        &mut f.storage,
        "AA-1",
        TaskUpdate {
            description: Some("allowed".into()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .unwrap();
    assert_eq!(f.bytes("AA-2"), before);
    assert!(
        f.operation(
            "AA-1",
            "AA-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .is_err()
    );
    f.operation(
        "AA-1",
        "AA-2",
        RelationshipKind::Dependency,
        RelationshipOperation::Remove,
    )
    .unwrap();
    assert!(
        !check(&f.storage, None)
            .unwrap()
            .issues
            .iter()
            .any(|i| i.code == "cycle")
    );
}

#[test]
fn missing_target_can_be_detached_without_typo_guessing() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["AA-99".into()],
            ..Default::default()
        },
    );
    let before = f.bytes("AA-1");
    assert!(
        repair(&f.storage, &RelationshipRepairRequest::default())
            .unwrap()
            .changed_tasks
            .is_empty()
    );
    assert_eq!(f.bytes("AA-1"), before);
    f.operation(
        "AA-99",
        "AA-1",
        RelationshipKind::Dependency,
        RelationshipOperation::Remove,
    )
    .unwrap();
    assert!(f.task("AA-1").relationships.is_empty());
    assert!(!f.storage.root_path.join("AA/99.yml").exists());
}

#[test]
fn fixes_and_duplicates_are_directional_without_stored_incoming_mirrors() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    let before = f.bytes("AA-2");
    f.operation(
        "AA-1",
        "AA-2",
        RelationshipKind::Fixes,
        RelationshipOperation::Add,
    )
    .unwrap();
    f.operation(
        "AA-1",
        "AA-2",
        RelationshipKind::Duplicate,
        RelationshipOperation::Add,
    )
    .unwrap();
    assert_eq!(f.bytes("AA-2"), before);
    let response = graph(
        &f.storage,
        &RelationshipGraphRequest {
            task_id: Some("AA-2".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(response.edges.len(), 2);
    assert!(
        response
            .edges
            .iter()
            .all(|e| e.source == "AA-1" && e.target == "AA-2" && e.provenance.len() == 1)
    );
    assert!(check(&f.storage, None).unwrap().issues.is_empty());
}

#[test]
fn repair_scope_cannot_be_changed_after_preview_and_bad_cycles_are_not_repaired() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    let preview = repair(
        &f.storage,
        &RelationshipRepairRequest {
            project: Some("AA".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(preview.changed_tasks.is_empty());
    assert!(preview.issues.iter().all(|i| !i.repairable));
    assert!(
        repair(
            &f.storage,
            &RelationshipRepairRequest {
                project: None,
                apply: true,
                revision: Some(preview.revision),
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("scope")
    );
}

#[test]
fn graph_truncation_is_explicit_and_keeps_edge_endpoints_present() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    let graph = graph(
        &f.storage,
        &RelationshipGraphRequest {
            max_nodes: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(graph.truncated);
    assert!(!graph.complete);
    assert!(graph.edges.is_empty());
    assert_eq!(graph.nodes.len(), 1);
}

#[test]
fn external_context_is_read_only_and_cross_root_writes_fail_closed() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let external_root = f.storage.root_path.parent().unwrap().join("nested/.tasks");
    fs::create_dir_all(external_root.join("BB")).unwrap();
    let external_task = Task {
        title: "external".into(),
        status: "Todo".into(),
        created: "2026-01-01T00:00:00Z".into(),
        ..Default::default()
    };
    fs::write(
        external_root.join("BB/1.yml"),
        serde_yaml_ng::to_string(&external_task).unwrap(),
    )
    .unwrap();
    drop(MultiFileTransaction::begin_all(&external_root, &[]).unwrap());
    let before = fs::read(external_root.join("BB/1.yml")).unwrap();
    assert!(
        f.operation(
            "AA-1",
            "BB-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .unwrap_err()
        .to_string()
        .contains("Cross-root")
    );
    assert_eq!(fs::read(external_root.join("BB/1.yml")).unwrap(), before);
    let mut local = f.task("AA-1");
    local.relationships.depends_on.push("BB-1".into());
    fs::write(
        f.storage.root_path.join("AA/1.yml"),
        serde_yaml_ng::to_string(&local).unwrap(),
    )
    .unwrap();
    let response = graph(&f.storage, &RelationshipGraphRequest::default()).unwrap();
    assert!(response.nodes.iter().any(|n| n.id == "BB-1"
        && n.title.as_deref() == Some("external")
        && n.state == "external"));
    assert!(
        response
            .nodes
            .iter()
            .any(|node| node.id == "AA-1" && node.state == "active")
    );
    assert!(
        response
            .edges
            .iter()
            .any(|edge| edge.source == "BB-1" && edge.target == "AA-1")
    );
    assert!(
        !serde_json::to_string(&response)
            .unwrap()
            .contains(external_root.to_str().unwrap())
    );
    assert_eq!(
        blocked_dependencies(&f.storage, "AA-1", &[]).unwrap(),
        ["BB-1"]
    );
    let mut deleted = external_task;
    deleted.deleted_at = Some("2026-01-01T00:00:00Z".into());
    fs::write(
        external_root.join("BB/1.yml"),
        serde_yaml_ng::to_string(&deleted).unwrap(),
    )
    .unwrap();
    assert_eq!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap()
            .nodes
            .iter()
            .find(|node| node.id == "BB-1")
            .unwrap()
            .state,
        "deleted"
    );
    fs::write(external_root.join("BB/001.yml"), &before).unwrap();
    assert_eq!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap()
            .nodes
            .iter()
            .find(|node| node.id == "BB-1")
            .unwrap()
            .state,
        "ambiguous"
    );
    fs::remove_file(external_root.join("BB/001.yml")).unwrap();
    fs::write(external_root.join("BB/1.yml"), "invalid: [yaml").unwrap();
    assert_eq!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap()
            .nodes
            .iter()
            .find(|node| node.id == "BB-1")
            .unwrap()
            .state,
        "unreadable"
    );
    fs::remove_file(external_root.join("BB/1.yml")).unwrap();
    assert_eq!(
        graph(&f.storage, &RelationshipGraphRequest::default())
            .unwrap()
            .nodes
            .iter()
            .find(|node| node.id == "BB-1")
            .unwrap()
            .state,
        "missing"
    );
}

#[test]
fn field_relative_changes_reverse_dependencies_and_guard_duplicate_replacement() {
    let f = Fixture::new();
    for id in ["AA-1", "AA-2", "AA-3"] {
        f.write(id, Default::default());
    }
    let request = RelationshipMutationRequest {
        source: "AA-1".into(),
        target: "AA-2".into(),
        kind: RelationshipField::DependsOn,
        operation: RelationshipOperation::Add,
        from_kind: None,
        expected_current: None,
    };
    mutate(&f.storage, &request).unwrap();
    assert_eq!(f.task("AA-2").relationships.blocks, ["AA-1"]);
    mutate(
        &f.storage,
        &RelationshipMutationRequest {
            kind: RelationshipField::Blocks,
            operation: RelationshipOperation::ChangeKind,
            from_kind: Some(RelationshipField::DependsOn),
            ..request.clone()
        },
    )
    .unwrap();
    assert_eq!(f.task("AA-1").relationships.blocks, ["AA-2"]);
    assert!(f.task("AA-1").relationships.depends_on.is_empty());
    mutate(
        &f.storage,
        &RelationshipMutationRequest {
            kind: RelationshipField::DuplicateOf,
            ..request.clone()
        },
    )
    .unwrap();
    let replacement = RelationshipMutationRequest {
        kind: RelationshipField::DuplicateOf,
        target: "AA-3".into(),
        ..request
    };
    assert!(mutate(&f.storage, &replacement).is_err());
    mutate(
        &f.storage,
        &RelationshipMutationRequest {
            expected_current: Some("AA-002".into()),
            ..replacement.clone()
        },
    )
    .unwrap();
    assert_eq!(
        f.task("AA-1").relationships.duplicate_of.as_deref(),
        Some("AA-3")
    );
    assert!(
        mutate(
            &f.storage,
            &RelationshipMutationRequest {
                expected_current: Some("AA-2".into()),
                ..replacement
            }
        )
        .is_err()
    );
}

#[test]
fn whole_object_noop_preserves_exact_bytes_and_does_not_repair_legacy_mirror() {
    let mut f = Fixture::new();
    let rel = TaskRelationships {
        blocks: vec!["AA-2".into()],
        ..Default::default()
    };
    f.write("AA-1", rel.clone());
    f.write("AA-2", Default::default());
    let path = f.storage.root_path.join("AA/1.yml");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"# user comment retained on no-op\nuser_extension: preserve\n");
    fs::write(&path, &bytes).unwrap();
    let counterpart = f.bytes("AA-2");
    let receipt = TaskService::update_with_receipt(
        &mut f.storage,
        "AA-1",
        TaskUpdate {
            relationships: Some(rel),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .unwrap();
    assert!(!receipt.changed);
    assert_eq!(f.bytes("AA-1"), bytes);
    assert_eq!(f.bytes("AA-2"), counterpart);
}

#[test]
fn counterpart_events_only_follow_successful_commit_and_noops_emit_nothing() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("BB-1", Default::default());
    let events = crate::api_events::subscribe();
    crate::storage::transaction::fault::fail_publish_at(1);
    assert!(
        f.operation(
            "AA-1",
            "BB-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .is_err()
    );
    assert!(events.try_iter().next().is_none());
    f.operation(
        "AA-1",
        "BB-1",
        RelationshipKind::Dependency,
        RelationshipOperation::Add,
    )
    .unwrap();
    let events_after_commit: Vec<_> = events.try_iter().collect();
    assert_eq!(events_after_commit.len(), 2);
    assert!(
        events_after_commit
            .iter()
            .all(|event| event.kind == "task_updated")
    );
    assert!(
        events_after_commit
            .iter()
            .any(|event| event.data["id"] == "BB-1"
                && event.data["relationships"]["depends_on"][0] == "AA-1")
    );
    f.operation(
        "AA-1",
        "BB-1",
        RelationshipKind::Dependency,
        RelationshipOperation::Add,
    )
    .unwrap();
    assert!(events.try_iter().next().is_none());
}

#[test]
fn snapshot_refuses_new_unlocked_projects_instead_of_reading_them_as_coherent() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let snapshot = snapshot_locks(&f.storage).unwrap();
    let dir = f.storage.root_path.join("BB");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("1.yml"),
        "title: concurrent\ncreated: 2026-01-01T00:00:00Z\n",
    )
    .unwrap();
    assert!(
        snapshot
            .load(&f.storage)
            .err()
            .unwrap()
            .to_string()
            .contains("project inventory changed")
    );
}

#[test]
fn repairs_never_touch_tasks_in_cycles_even_for_another_relationship_kind() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            depends_on: vec!["AA-2".into()],
            related: vec!["AA-3".into(), "AA-003".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    f.write("AA-3", Default::default());
    let preview = repair(&f.storage, &RelationshipRepairRequest::default()).unwrap();
    assert!(preview.changed_tasks.is_empty());
    assert!(preview.issues.iter().all(|issue| !issue.repairable));
}

#[test]
fn deleted_mirror_assertion_cannot_be_silently_left_behind_on_logical_removal() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write(
        "AA-2",
        TaskRelationships {
            depends_on: vec!["AA-1".into()],
            ..Default::default()
        },
    );
    let mut task = f.task("AA-2");
    task.deleted_at = Some("2026-01-01T00:00:00Z".into());
    fs::write(
        f.storage.root_path.join("AA/2.yml"),
        serde_yaml_ng::to_string(&task).unwrap(),
    )
    .unwrap();
    let before_a = f.bytes("AA-1");
    let before_b = f.bytes("AA-2");
    assert!(
        f.operation(
            "AA-1",
            "AA-2",
            RelationshipKind::Dependency,
            RelationshipOperation::Remove
        )
        .unwrap_err()
        .to_string()
        .contains("restore it first")
    );
    assert_eq!(f.bytes("AA-1"), before_a);
    assert_eq!(f.bytes("AA-2"), before_b);
}

#[test]
fn forked_dag_is_not_a_cycle_but_closing_long_path_is_rejected() {
    let f = Fixture::new();
    for id in ["AA-1", "AA-2", "AA-3", "AA-4"] {
        f.write(id, Default::default());
    }
    for (source, target) in [
        ("AA-1", "AA-2"),
        ("AA-1", "AA-3"),
        ("AA-2", "AA-4"),
        ("AA-3", "AA-4"),
    ] {
        f.operation(
            source,
            target,
            RelationshipKind::Dependency,
            RelationshipOperation::Add,
        )
        .unwrap();
    }
    assert!(check(&f.storage, None).unwrap().issues.is_empty());
    assert!(
        f.operation(
            "AA-4",
            "AA-1",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .is_err()
    );
}

#[test]
fn unambiguous_physical_padded_identity_is_written_in_place_and_emits_canonical_event() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("BB-1", Default::default());
    fs::rename(
        f.storage.root_path.join("BB/1.yml"),
        f.storage.root_path.join("BB/001.yml"),
    )
    .unwrap();
    let events = crate::api_events::subscribe();
    f.operation(
        "AA-1",
        "BB-001",
        RelationshipKind::Dependency,
        RelationshipOperation::Add,
    )
    .unwrap();
    assert!(!f.storage.root_path.join("BB/1.yml").exists());
    assert_eq!(f.task("BB-1").relationships.depends_on, ["AA-1"]);
    assert!(
        events
            .try_iter()
            .any(|event| event.kind == "task_updated" && event.data["id"] == "BB-1")
    );
}

#[cfg(unix)]
#[test]
fn symlinked_workspace_context_cannot_hide_an_ambiguous_task_identity() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    let workspace = f._temp.path().join("external");
    let root = workspace.join(".tasks");
    fs::create_dir_all(root.join("AA")).unwrap();
    fs::write(root.join("AA/1.yml"), f.bytes("AA-1")).unwrap();
    drop(MultiFileTransaction::begin_all(&root, &[]).unwrap());
    std::os::unix::fs::symlink(
        &workspace,
        f.storage.root_path.parent().unwrap().join("linked"),
    )
    .unwrap();
    assert!(!check(&f.storage, None).unwrap().complete);
    assert!(
        f.operation(
            "AA-1",
            "AA-2",
            RelationshipKind::Dependency,
            RelationshipOperation::Add
        )
        .unwrap_err()
        .to_string()
        .contains("ambiguous")
    );
}

#[test]
fn graph_bounds_reject_out_of_range_values_and_depth_zero_keeps_only_root() {
    let f = Fixture::new();
    f.write(
        "AA-1",
        TaskRelationships {
            blocks: vec!["AA-2".into()],
            ..Default::default()
        },
    );
    f.write("AA-2", Default::default());
    for request in [
        RelationshipGraphRequest {
            depth: Some(9),
            ..Default::default()
        },
        RelationshipGraphRequest {
            max_nodes: Some(0),
            ..Default::default()
        },
        RelationshipGraphRequest {
            max_nodes: Some(501),
            ..Default::default()
        },
    ] {
        assert!(graph(&f.storage, &request).is_err());
    }
    let root = graph(
        &f.storage,
        &RelationshipGraphRequest {
            task_id: Some("AA-1".into()),
            depth: Some(0),
            max_nodes: Some(500),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        root.nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["AA-1"]
    );
    assert!(root.edges.is_empty());
    assert_eq!(
        graph(
            &f.storage,
            &RelationshipGraphRequest {
                task_id: Some("AA-1".into()),
                depth: Some(8),
                ..Default::default()
            }
        )
        .unwrap()
        .nodes
        .len(),
        2
    );
}

#[test]
fn graph_default_node_limit_is_one_hundred() {
    let f = Fixture::new();
    for number in 1..=101 {
        f.write(&format!("AA-{number}"), Default::default());
    }
    let default = graph(&f.storage, &RelationshipGraphRequest::default()).unwrap();
    assert_eq!(default.nodes.len(), 100);
    assert!(default.truncated);
    assert!(!default.complete);
    let larger = graph(
        &f.storage,
        &RelationshipGraphRequest {
            max_nodes: Some(500),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(larger.nodes.len(), 101);
    assert!(!larger.truncated);
}

#[test]
fn typoed_project_scope_is_never_reported_as_clean_or_repairable() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    for project in ["aa", "TYPO", "../AA"] {
        assert!(check(&f.storage, Some(project)).is_err());
        assert!(
            graph(
                &f.storage,
                &RelationshipGraphRequest {
                    project: Some(project.into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            repair(
                &f.storage,
                &RelationshipRepairRequest {
                    project: Some(project.into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            repair(
                &f.storage,
                &RelationshipRepairRequest {
                    project: Some(project.into()),
                    apply: true,
                    revision: Some("anything".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert!(check(&f.storage, Some("AA")).unwrap().complete);
}

#[cfg(unix)]
#[test]
fn symlink_task_file_is_unreadable_context_and_its_target_is_never_modified() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let target = f._temp.path().join("outside.yml");
    let bytes = b"title: outside\ncreated: 2026-01-01T00:00:00Z\n";
    fs::write(&target, bytes).unwrap();
    std::os::unix::fs::symlink(&target, f.storage.root_path.join("AA/2.yml")).unwrap();
    let check = check(&f.storage, None).unwrap();
    assert!(!check.complete);
    assert!(
        check
            .issues
            .iter()
            .any(|issue| issue.code == "unreadable" && issue.tasks == ["AA-2"])
    );
    assert!(
        repair(&f.storage, &RelationshipRepairRequest::default())
            .unwrap()
            .changed_tasks
            .is_empty()
    );
    assert_eq!(fs::read(&target).unwrap(), bytes);
}

#[test]
fn relative_and_absolute_roots_have_identical_snapshots_and_revision() {
    struct RestoreCwd(PathBuf);
    impl Drop for RestoreCwd {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).unwrap();
        }
    }
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let absolute = graph(&f.storage, &RelationshipGraphRequest::default()).unwrap();
    // nextest runs each test in its own process. Restore even on assertion
    // failure, and never change the working directory of another worker.
    let _cwd = RestoreCwd(std::env::current_dir().unwrap());
    std::env::set_current_dir(f.storage.root_path.parent().unwrap()).unwrap();
    for spelling in [".tasks", "./.tasks", "../workspace/.tasks"] {
        let relative = Storage {
            root_path: PathBuf::from(spelling),
        };
        let response = graph(&relative, &RelationshipGraphRequest::default()).unwrap();
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            serde_json::to_value(&absolute).unwrap(),
            "{spelling}"
        );
        assert_eq!(check(&relative, None).unwrap().revision, absolute.revision);
    }
    let missing = Storage {
        root_path: PathBuf::from("missing/.tasks"),
    };
    let error = check(&missing, None).unwrap_err().to_string();
    assert!(
        error.contains("missing/.tasks") && error.contains("cannot resolve tasks root"),
        "{error}"
    );
    assert!(!Path::new("missing").exists());
}

#[test]
fn unavailable_configuration_source_identifies_its_path() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let path = f.storage.root_path.join("config.yml");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let error = check(&f.storage, None).unwrap_err().to_string();
    assert!(
        error.contains("snapshot unavailable")
            && error.contains("configuration source")
            && error.contains("config.yml"),
        "{error}"
    );
}

#[test]
fn external_guard_refuses_new_root_between_lock_acquisition_and_inventory_load() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let before = f.bytes("AA-1");
    let guard = external_locks(&f.storage).unwrap();
    let txn = MultiFileTransaction::begin_all(&f.storage.root_path, &[]).unwrap();
    fs::create_dir_all(
        f.storage
            .root_path
            .parent()
            .unwrap()
            .join("new-workspace/.tasks/BB"),
    )
    .unwrap();
    let error = guard.load(&f.storage).err().unwrap().to_string();
    assert!(error.contains("external roots changed"), "{error}");
    drop(txn);
    assert_eq!(f.bytes("AA-1"), before);
}

#[test]
fn external_guard_refuses_new_project_in_an_existing_locked_root() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    let external = f.storage.root_path.parent().unwrap().join("nested/.tasks");
    fs::create_dir_all(external.join("BB")).unwrap();
    let guard = external_locks(&f.storage).unwrap();
    fs::create_dir(external.join("CC")).unwrap();
    let error = guard.load(&f.storage).err().unwrap().to_string();
    assert!(
        error.contains("external project manifest changed"),
        "{error}"
    );
}

#[test]
fn external_manifest_drift_during_publication_rolls_back_every_staged_file() {
    let f = Fixture::new();
    f.write("AA-1", Default::default());
    f.write("AA-2", Default::default());
    let first = f.bytes("AA-1");
    let second = f.bytes("AA-2");
    let guard = external_locks(&f.storage).unwrap();
    let mut txn = MultiFileTransaction::begin_all(&f.storage.root_path, &[]).unwrap();
    txn.stage(
        &f.storage.root_path.join("AA/1.yml"),
        "title: replaced one\n".into(),
    )
    .unwrap();
    txn.stage(
        &f.storage.root_path.join("AA/2.yml"),
        "title: replaced two\n".into(),
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let error = txn
        .commit_checked(|| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                fs::create_dir_all(
                    f.storage
                        .root_path
                        .parent()
                        .unwrap()
                        .join("appeared/.tasks/BB"),
                )?;
            }
            guard.verify(&f.storage)
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("external roots changed"), "{error}");
    assert_eq!(f.bytes("AA-1"), first);
    assert_eq!(f.bytes("AA-2"), second);
    assert!(!f.storage.root_path.join(".txn-pending.json").exists());
}
