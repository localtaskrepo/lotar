use lotar::Storage;
use lotar::services::sprint_service::SprintService;
use lotar::storage::sprint::{
    Sprint, SprintActual, SprintCanonicalizationWarning, SprintCapacity, SprintPlan,
};

#[test]
fn sprint_create_canonicalizes_length_when_ends_at_present() {
    let temp = tempfile::tempdir().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    let mut storage = Storage::new(tasks_dir.clone());
    let sprint = Sprint {
        plan: Some(SprintPlan {
            goal: Some("Ship new onboarding flow".to_string()),
            length: Some("2w".to_string()),
            ends_at: Some("2025-10-24T17:00:00-04:00".to_string()),
            notes: Some(" Kickoff on Monday. ".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let outcome = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    assert_eq!(outcome.record.id, 1);
    assert_eq!(
        outcome.warnings,
        vec![SprintCanonicalizationWarning::LengthDiscardedInFavorOfEndsAt]
    );
    assert!(outcome.applied_defaults.is_empty());

    let plan = outcome.record.sprint.plan.expect("plan exists");
    assert!(
        plan.length.is_none(),
        "length should be cleared when ends_at present"
    );
    assert!(plan.ends_at.is_some());
    assert_eq!(plan.notes.unwrap(), "Kickoff on Monday.");

    let sprint_path = lotar::storage::sprint::Sprint::path_for_id(&storage.root_path, 1);
    let contents = std::fs::read_to_string(&sprint_path).expect("sprint file exists");
    assert!(contents.contains("ends_at:"));
    assert!(!contents.contains("length:"));
}

#[test]
fn sprint_create_and_list_trims_empty_sections() {
    let temp = tempfile::tempdir().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    let mut storage = Storage::new(tasks_dir.clone());
    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint 42".to_string()),
            capacity: Some(SprintCapacity {
                points: Some(0),
                hours: None,
            }),
            overdue_after: Some("".to_string()),
            ..SprintPlan::default()
        }),
        actual: Some(SprintActual {
            started_at: Some("".to_string()),
            closed_at: None,
        }),
        ..Sprint::default()
    };

    let outcome = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    assert_eq!(outcome.record.id, 1);
    assert!(outcome.warnings.is_empty());
    assert!(outcome.applied_defaults.is_empty());

    let plan = outcome.record.sprint.plan.expect("plan exists");
    assert!(plan.capacity.is_none(), "empty capacity should be removed");
    assert!(plan.overdue_after.is_none());

    let actual = outcome.record.sprint.actual;
    assert!(actual.is_none(), "empty actual should be dropped");

    let all = SprintService::list(&storage).expect("list sprints");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, 1);
}

#[test]
fn sprint_service_migrates_legacy_directory() {
    let temp = tempfile::tempdir().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    let legacy_dir = tasks_dir.join("sprints");
    std::fs::create_dir_all(&legacy_dir).unwrap();

    let legacy_file = legacy_dir.join("1.yml");
    std::fs::write(
        &legacy_file,
        r#"plan:
  label: Legacy Sprint
"#,
    )
    .unwrap();

    let storage = Storage::new(tasks_dir.clone());
    let records = SprintService::list(&storage).expect("list legacy sprints");

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, 1);

    let new_dir = tasks_dir.join("@sprints");
    assert!(new_dir.exists(), "migrated directory should exist");
    assert!(!legacy_dir.exists(), "legacy directory should be moved");
}
