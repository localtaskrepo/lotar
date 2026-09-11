mod common;

use lotar::api_types::{SyncReport, SyncSummary};
use lotar::config::types::ResolvedConfig;
use lotar::services::sync_report_service::SyncReportService;

fn report(id: &str) -> SyncReport {
    SyncReport {
        id: id.into(),
        created_at: "2026-09-06T09:00:00Z".into(),
        status: "ok".into(),
        direction: "push".into(),
        provider: "github".into(),
        remote: "origin".into(),
        project: Some("DEV".into()),
        dry_run: false,
        summary: SyncSummary::default(),
        warnings: vec![],
        info: vec![],
        entries: vec![],
    }
}

#[test]
fn same_timestamp_reports_and_reused_ids_never_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let config = ResolvedConfig::from_global(Default::default());
    let first = report("one");
    let second = report("two");
    let a = SyncReportService::write_report(root.path(), &config, &first, true)
        .unwrap()
        .unwrap();
    let b = SyncReportService::write_report(root.path(), &config, &second, true)
        .unwrap()
        .unwrap();
    let c = SyncReportService::write_report(root.path(), &config, &first, true)
        .unwrap()
        .unwrap();
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_eq!(
        SyncReportService::read_report(root.path(), &config, &a)
            .unwrap()
            .id,
        "one"
    );
    assert_eq!(
        SyncReportService::read_report(root.path(), &config, &b)
            .unwrap()
            .id,
        "two"
    );
}

#[test]
fn hostile_ids_are_single_bounded_components_and_collisions_are_safe() {
    let root = tempfile::tempdir().unwrap();
    let config = ResolvedConfig::from_global(Default::default());
    for id in ["../../escape/\\report", "..", "", &"a".repeat(1000)] {
        let report = report(id);
        let a = SyncReportService::write_report(root.path(), &config, &report, true)
            .unwrap()
            .unwrap();
        let b = SyncReportService::write_report(root.path(), &config, &report, true)
            .unwrap()
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(std::path::Path::new(&a).components().count(), 1);
        assert!(a.len() < 200);
        assert_eq!(
            SyncReportService::read_report(root.path(), &config, &a)
                .unwrap()
                .id,
            id
        );
    }
}

#[test]
fn disabled_report_does_not_create_directory() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        SyncReportService::write_report(
            root.path(),
            &ResolvedConfig::from_global(Default::default()),
            &report("dry"),
            false
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn sync_push_dry_run_does_not_persist_even_when_reports_requested() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        lotar::utils::paths::global_config_path(root.path()),
        "default.project: TEST\nremotes:\n  origin:\n    provider: github\n    repo: org/repo\n",
    )
    .unwrap();
    let resolver = lotar::workspace::TasksDirectoryResolver {
        path: root.path().to_path_buf(),
        source: lotar::workspace::TasksDirectorySource::CommandLineFlag,
    };
    let before = std::fs::read_dir(root.path()).unwrap().count();
    let response = lotar::services::sync_service::SyncService::push(
        &resolver,
        "origin",
        Some("TEST"),
        true,
        None,
        None,
        Some(true),
        true,
        Some("../../dry"),
    )
    .unwrap();
    assert!(response.report.unwrap().stored_path.is_none());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), before);
}

#[cfg(unix)]
#[test]
fn existing_report_symlink_is_not_followed_or_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let config = ResolvedConfig::from_global(Default::default());
    let report = report("same");
    let name = SyncReportService::write_report(root.path(), &config, &report, true)
        .unwrap()
        .unwrap();
    let reports = SyncReportService::compute_reports_root(root.path(), &config).unwrap();
    let target = root.path().join("sentinel");
    std::fs::write(&target, "keep").unwrap();
    std::fs::remove_file(reports.join(&name)).unwrap();
    std::os::unix::fs::symlink(&target, reports.join(&name)).unwrap();
    let second = SyncReportService::write_report(root.path(), &config, &report, true)
        .unwrap()
        .unwrap();
    assert_ne!(name, second);
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
}

#[test]
fn concurrent_same_id_reports_get_distinct_files() {
    let root = tempfile::tempdir().unwrap();
    let names = std::thread::scope(|scope| {
        let threads = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    SyncReportService::write_report(
                        root.path(),
                        &ResolvedConfig::from_global(Default::default()),
                        &report("same"),
                        true,
                    )
                    .unwrap()
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<std::collections::HashSet<_>>()
    });
    assert_eq!(names.len(), 8);
}
