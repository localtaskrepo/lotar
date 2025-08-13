# Environment Variables

Complete list of LOTAR_* variables and behavior.

- LOTAR_TASKS_DIR: path to tasks directory (overrides discovery)
- LOTAR_PORT: web server port override
- LOTAR_PROJECT: default project name (mapped to prefix)
- LOTAR_DEFAULT_ASSIGNEE: default assignee for new tasks
- LOTAR_DEFAULT_REPORTER: default reporter used in identity resolution
- LOTAR_SSE_DEBOUNCE_MS: default debounce for SSE endpoint

Diagnostics/testing variables:
- LOTAR_TEST_SILENT: suppress noisy logs in tests
- LOTAR_VERBOSE: enable extra setup logs
- LOTAR_TEST_FAST_IO: accelerate IO timers in server during tests
- LOTAR_TEST_FAST_NET: accelerate client timeouts in tests

Notes
- Env variables participate in precedence: CLI > env > home > project > global > defaults.
- Some env vars provide values (e.g., default_reporter); toggles (auto_*) must be set in config files.

See also: [Configuration Reference](./config-reference.md) and [Resolution & Precedence](./precedence.md).
