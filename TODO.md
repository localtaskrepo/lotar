Features:
- Task relationship queries and graphs
- Sprints
- Shell completion with install command
- Git Hooks (e.g. for scanner)
- Project members property (for auto fill in web interface)
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in web ui and cli
- lock issue file in git if in progress? (or provide a command e.g. lotar task un-/lock <Task-ID>)
- custom properties can be used to filter and query. Custom properties are accessed like any other property (no custom: prefix anywhere)
- Localization

---------- NEXT ----------
- Tests names are a mess, way to many single test files, need to speed up whole suite
--------------------------

Chores:
- Replace parcel with vite
- Check if we're Windows compatible
- Test release workflow
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)
- Config validation may need an update
- properties that don't have any special functions associated with them (e.g. categories) should be custom properties that just allow generic querying by matching terms like all custom properties should support. Only when we add special function should we promote them to standard fields.
- we have src/utils_git.rs, why is this not in src/utils/git.rs?

Bugs:
- We have an operation that creates an empty config.yml and nothing else
- CI job is failing because of clippy for some reason
- Help output shows raw markdown (Maybe we should split docs from direct help and more detailed help linked to)
- `lotar scan src` in this project throws an error

# Implementation Roadmap

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

## Chore: Test Cleanup

Goals:
- Reduce number of test binaries by consolidating many single-test files.
- Standardize file and test names for quick grepability and consistency.
- Keep all-tests-green and clippy clean; no behavior changes.

Naming conventions:
- File names: group by domain and intent: cli_*, config_*, scanner_*, stats_*, storage_*, web_*, mcp_*.
- Prefer pattern: {domain}_{topic}_tests.rs for suites; use module blocks inside for subtopics.
- Test names: verb-phrases, snake_case, e.g., parses_branch_alias_maps, infers_status_from_branch_alias.

Low-risk first merges (source -> target suite):
- stats_time_in_status_single_task_test.rs -> stats_git_tests.rs (module time_in_status_single_task)
- stats_time_in_status_git_test.rs -> stats_git_tests.rs (module time_in_status_git)
- stats_effort_transitions_window_test.rs -> stats_git_tests.rs (module effort_transitions)
- stats_effort_unit_option_test.rs, stats_effort_points_and_auto_and_filters_test.rs, stats_effort_comments_custom_test.rs -> stats_snapshot_tests.rs (modules effort_unit, effort_points_auto_filters, effort_comments_custom)
- changelog_range_test.rs, changelog_range_json_test.rs, changelog_smoke_test.rs, changelog_working_tree_test.rs -> changelog_tests.rs (modules range_text, range_json, smoke, working_tree)
- list_effort_filters_and_sort_test.rs -> cli_tests.rs (module list_effort)
- effort_normalization_on_write_test.rs -> cli_unit_tests.rs (module effort_normalization)
- help_module_unit_test.rs -> cli_unit_tests.rs (module help_module)
- output_format_consistency_test_simple.rs -> output_format_consistency_test.rs (module simple)

Next merges:
- scanner_block_comments_test.rs, scanner_custom_ticket_patterns_test.rs, scanner_inline_effort_test.rs, scanner_insertion_suggestion_test.rs, scanner_signal_words_test.rs, scanner_ticket_extraction_test.rs, scanner_ticket_words_toggle_test.rs -> scanner_tests_new.rs (modules block_comments, custom_patterns, inline_effort, insertion_suggestion, signal_words, ticket_extraction, words_toggle)
- scan_bidir_references_test.rs, scan_ignore_and_filters_test.rs -> scanner_tests.rs (modules bidir_references, ignore_and_filters)

Housekeeping:
- Gate ad-hoc debug file logging behind LOTAR_DEBUG (done) to avoid FS I/O during tests.
- Add cargo test -- --list checks in CI to track test count trend.
- Consider #[cfg(test)] feature-gating of heavy logs or use tracing with env filter.

Success metric:
- Reduce total tests binaries by ~30-40% without flakiness; test wall time noticeably lower locally and in CI.

## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image
