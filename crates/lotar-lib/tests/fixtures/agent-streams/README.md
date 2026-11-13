# Agent stream fixtures

Output lines of agent CLIs, replayed by the runner parser tests in
`src/services/agent_runner.rs`.

- `*-edit.jsonl` were captured from real runs (claude 2.1.308, codex 0.163.0,
  copilot 1.0.69) on a throwaway repository; paths and IDs are anonymised.
- `*-docs.jsonl` are assembled from the example events in each CLI's
  documentation (DEV-203). These CLIs were not run live, so replace a docs
  fixture with a captured run when one is available.
