//! Scan source rewrites and inline attributes; run natively on Windows CI (DEV-495).

mod reference_mutation_tests {
    use lotar::api_types::ScanRequest;
    use lotar::cli::ScanArgs;
    use lotar::cli::handlers::{CommandHandler, ScanHandler};
    use lotar::output::{LogLevel, OutputFormat, OutputRenderer};
    use lotar::services::scan_service::ScanService;
    use lotar::storage::manager::Storage;
    use lotar::storage::operations::before_task_write;
    use lotar::storage::task::Task;
    use lotar::types::{Priority, ReferenceEntry, TaskComment, custom_value_string};
    use lotar::workspace::TasksDirectoryResolver;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::rc::Rc;

    #[derive(Clone, Copy)]
    enum Surface {
        Cli,
        Service,
    }

    fn run(surface: Surface, resolver: &TasksDirectoryResolver, source: &std::path::Path) {
        let paths = vec![source.to_string_lossy().to_string()];
        match surface {
            Surface::Cli => {
                ScanHandler::execute(
                    ScanArgs {
                        paths,
                        include: Vec::new(),
                        exclude: Vec::new(),
                        detailed: false,
                        context: 0,
                        dry_run: false,
                        strip_attributes: None,
                        reanchor: false,
                        modified_only: false,
                    },
                    Some("SAFE"),
                    resolver,
                    &OutputRenderer::new(OutputFormat::Json, LogLevel::Error),
                )
                .unwrap();
            }
            Surface::Service => {
                let response = ScanService::run(resolver, ScanRequest { paths, project: Some("SAFE".into()), ..Default::default() }).unwrap();
                assert_eq!(response.summary.failed, 0);
                assert_eq!(response.entries.len(), 1);
                assert_eq!(response.entries[0].task_id.as_deref(), Some("SAFE-1"));
                assert_eq!(response.summary.created + response.summary.updated, 1);
            }
        }
    }

    fn preserves_concurrent_edits(surface: Surface, source: &str, create: bool) {
        for already_anchored in [false, true] {
            // A private parent prevents lookup from discovering sibling test workspaces.
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("workspace");
            let tasks = root.join(".tasks");
            fs::create_dir_all(&tasks).unwrap();
            fs::write(tasks.join("config.yml"), "default:\n  project: SAFE\n  reporter: tester\nauto:\n  identity: false\n").unwrap();
            let source_path = root.join("main.rs");
            fs::write(&source_path, source).unwrap();
            let resolver = TasksDirectoryResolver::resolve(Some(tasks.to_str().unwrap()), None).unwrap();
            if !create {
                let mut storage = Storage::new(&tasks);
                let task = Task::new(tasks.clone(), "existing task".into(), Priority::new("medium"));
                assert_eq!(storage.add(&task, "SAFE", None).unwrap(), "SAFE-1");
            }

            let edited = Rc::new(Cell::new(false));
            let was_edited = Rc::clone(&edited);
            let committed_bytes = Rc::new(RefCell::new(None));
            let editor_bytes = Rc::clone(&committed_bytes);
            let editor_tasks = tasks.clone();
            let desired_anchor = format!("{}#1", source_path.display());
            let _guard = before_task_write::install(&tasks, "SAFE-1", move || {
                let mut editor = Storage::new(&editor_tasks);
                editor
                    .mutate_task("SAFE-1", |fresh| {
                        fresh.comments.push(TaskComment {
                            extra: Default::default(),
                            author: None,
                            date: "2026-10-20T00:00:00Z".into(),
                            text: "concurrent comment".into(),
                        });
                        fresh.references.push(ReferenceEntry { code: Some("other.rs#7".into()), ..Default::default() });
                        if already_anchored {
                            fresh.references.push(ReferenceEntry { code: Some(desired_anchor), ..Default::default() });
                        }
                        fresh.modified = "2000-01-01T00:00:00Z".into();
                        fresh.custom_fields.insert("severity".into(), custom_value_string("high"));
                        fresh.extra_fields.insert("user_meta".into(), serde_yaml_ng::from_str("nested: {enabled: true, labels: [one, two]}").unwrap());
                        Ok(true)
                    })
                    .unwrap();
                let task_path = editor_tasks.join("SAFE/1.yml");
                let formatted = format!("# Concurrent editor formatting\n{}", fs::read_to_string(&task_path).unwrap());
                fs::write(&task_path, formatted).unwrap();
                *editor_bytes.borrow_mut() = Some(fs::read(&task_path).unwrap());
                was_edited.set(true);
            });

            run(surface, &resolver, &source_path);
            assert!(edited.get(), "concurrent editor was not exercised");
            let updated = Storage::new(&tasks).get("SAFE-1", "SAFE").unwrap();
            if already_anchored {
                assert_eq!(updated.modified, "2000-01-01T00:00:00Z");
                assert_eq!(fs::read(tasks.join("SAFE/1.yml")).unwrap(), *committed_bytes.borrow().as_ref().unwrap());
            } else {
                assert_ne!(updated.modified, "2000-01-01T00:00:00Z");
            }
            assert_eq!(updated.comments.len(), 1, "concurrent comment was lost");
            assert_eq!(updated.comments[0].text, "concurrent comment");
            assert_eq!(updated.custom_fields.get("severity"), Some(&custom_value_string("high")));
            assert_eq!(updated.extra_fields.get("user_meta"), Some(&serde_yaml_ng::from_str("nested: {enabled: true, labels: [one, two]}").unwrap()));
            assert_eq!(updated.references.len(), 2, "concurrent reference was lost");
            assert!(updated.references.iter().any(|r| r.code.as_deref() == Some("other.rs#7")));
            assert!(updated.references.iter().any(|r| { r.code.as_deref().is_some_and(|code| code.ends_with("main.rs#1")) }));
            let expected_source = if create { "// TODO (SAFE-1): create task\n" } else { source };
            assert_eq!(fs::read_to_string(&source_path).unwrap(), expected_source);

            // An unchanged repeat must preserve user formatting and both file mtimes.
            let task_path = tasks.join("SAFE/1.yml");
            let formatted = format!("# User-owned formatting\n{}user_layout: {{flag: 'yes'}}\n", fs::read_to_string(&task_path).unwrap());
            fs::write(&task_path, &formatted).unwrap();
            let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
            for path in [&task_path, &source_path] {
                fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
            }
            run(surface, &resolver, &source_path);
            assert_eq!(fs::read_to_string(&task_path).unwrap(), formatted);
            assert_eq!(fs::read_to_string(&source_path).unwrap(), expected_source);
            for path in [&task_path, &source_path] {
                assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), old);
            }
        }
    }

    #[test]
    fn cli_create_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Cli, "// TODO: create task\n", true);
    }

    #[test]
    fn cli_known_todo_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Cli, "// TODO (SAFE-1): existing task\n", false);
    }

    #[test]
    fn cli_bare_mention_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Cli, "// SAFE-1 bare mention\n", false);
    }

    #[test]
    fn service_create_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Service, "// TODO: create task\n", true);
    }

    #[test]
    fn service_known_todo_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Service, "// TODO (SAFE-1): existing task\n", false);
    }

    #[test]
    fn service_bare_mention_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Service, "// SAFE-1 bare mention\n", false);
    }
}

mod scan_utils_tests {
    use lotar::utils::scan::{parse_inline_attributes, refresh_code_reference, strip_bracket_attributes, write_source, write_source_with_prepared};

    #[test]
    fn refresh_relocates_stale_anchors_to_unoccupied_live_mentions_and_keeps_metadata() {
        use lotar::types::ReferenceEntry;
        let mut references = vec![
            ReferenceEntry { code: Some("main.rs#1".into()), ..Default::default() },
            ReferenceEntry { code: Some("main.rs#2".into()), link: Some("https://example.invalid/a".into()), ..Default::default() },
            ReferenceEntry { code: Some("main.rs#2".into()), github: Some("example/repo#1".into()), ..Default::default() },
        ];
        let live = vec!["main.rs#1".into(), "main.rs#4".into()];
        assert!(refresh_code_reference(&mut references, "main.rs#1", &live, &[]));
        assert_eq!(references.len(), 3);
        assert_eq!(references[0].code.as_deref(), Some("main.rs#1"));
        assert_eq!(references[1].code.as_deref(), Some("main.rs#4"));
        assert_eq!(references[1].link.as_deref(), Some("https://example.invalid/a"));
        assert_eq!(references[2].code.as_deref(), Some("main.rs#4"));
        assert_eq!(references[2].github.as_deref(), Some("example/repo#1"));
        let before = references.clone();
        assert!(!refresh_code_reference(&mut references, "main.rs#4", &live, &[]));
        assert_eq!(references, before);
    }

    #[cfg(unix)]
    #[test]
    fn locked_inode_rename_preserves_handles_on_unix() {
        use lotar::utils::scan::lock_source;
        use std::io::Read;
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.rs");
        let replacement = temp.path().join("replacement.rs");
        std::fs::write(&source, "original").unwrap();
        std::fs::write(&replacement, "replacement").unwrap();
        let mut old = lock_source(&source).unwrap();
        let mut new = lock_source(&replacement).unwrap();
        std::fs::rename(&replacement, &source).expect("rename while both read/write inodes are locked");
        let published = std::fs::OpenOptions::new().read(true).write(true).open(&source).unwrap();
        assert_eq!(published.metadata().unwrap().ino(), new.metadata().unwrap().ino());
        assert_ne!(published.metadata().unwrap().ino(), old.metadata().unwrap().ino());
        assert_eq!(fs2::FileExt::try_lock_exclusive(&published).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
        let mut old_bytes = String::new();
        let mut new_bytes = String::new();
        old.read_to_string(&mut old_bytes).unwrap();
        new.read_to_string(&mut new_bytes).unwrap();
        assert_eq!(old_bytes, "original");
        assert_eq!(new_bytes, "replacement");
        drop(new);
        fs2::FileExt::try_lock_exclusive(&published).unwrap();
    }

    #[test]
    fn cooperating_source_writers_compare_after_prepare_and_rename() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, "original").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                write_source_with_prepared(&path, "original", "first", || {
                    barrier.wait();
                    Ok(())
                })
            });
            let second = scope.spawn(|| {
                write_source_with_prepared(&path, "original", "second", || {
                    barrier.wait();
                    Ok(())
                })
            });
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(results.iter().filter_map(|result| result.as_ref().err()).all(|error| error.contains("Source changed")));
        let winner = std::fs::read_to_string(&path).unwrap();
        assert_eq!(winner, if results[0].is_ok() { "first" } else { "second" });
        assert!(write_source(&path, "original", "stale").is_err());
        write_source(&path, &winner, "next inode").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "next inode");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn strip_preserves_leading_indentation() {
        let input = "\t    // TODO: Do it [assignee=me]  [priority=high]";
        let out = strip_bracket_attributes(input);
        assert!(out.starts_with("\t    // TODO: Do it"));
        // Ensure no brackets remain
        assert!(!out.contains('[') && !out.contains(']'));
        // Ensure indentation didn't collapse
        assert_eq!(&out[..5], "\t    ");
    }

    #[test]
    fn strip_does_not_collapse_spacing_or_alignments() {
        let input = "    signal_words: Vec<String>,                     // TODO handle words [tag=scan]  [due=2025-12-31]";
        let out = strip_bracket_attributes(input);
        // Leading spaces preserved
        assert!(out.starts_with("    signal_words: Vec<String>,                     // TODO handle words"));
        // No brackets remain
        assert!(!out.contains('[') && !out.contains(']'));
        // The run of spaces before the comment should still be long (>= 5)
        let after_comma = out.split("Vec<String>,").nth(1).unwrap_or("");
        // Expect at least 5 spaces before the // comment after the comma
        assert!(after_comma.starts_with("     "));
    }

    #[test]
    fn parse_inline_attributes_preserves_custom_field_key() {
        let attrs = parse_inline_attributes("// TODO tidy [product=Platform]");
        assert_eq!(attrs.fields, vec![("product".to_string(), "Platform".to_string())]);
    }
}
