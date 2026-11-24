import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { configDefaults, defineConfig } from 'vitest/config';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Specs that wait on wall-clock budgets (DEV-785): each failed in a full run under
// load and passed alone (DEV-750, DEV-751, DEV-755, DEV-775, DEV-781, DEV-785
// review, DEV-811 merge, DEV-777 merge). They run in the `wall-clock` project after the parallel files finish,
// one file at a time.
// A spec that flakes under load joins this list instead of getting its own ticket
// (AGENTS.md).
const WALL_CLOCK_SPECS = [
    'tests/api.automation.smoke.spec.ts',
    'tests/api.review-policy.smoke.spec.ts',
    'tests/cli.automation.smoke.spec.ts',
    'tests/harness.agent-launcher.smoke.spec.ts',
    'tests/ui.advanced.smoke.spec.ts',
    'tests/ui.agent-jobs.smoke.spec.ts',
    'tests/ui.attachment-previews.smoke.spec.ts',
    'tests/ui.config-coverage.smoke.spec.ts',
    'tests/ui.dialogs.smoke.spec.ts',
    'tests/ui.filter-autocomplete.smoke.spec.ts',
    'tests/ui.panel-safety.smoke.spec.ts',
    'tests/ui.sort-export.smoke.spec.ts',
    'tests/ui.sprints-responsive.smoke.spec.ts',
    'tests/ui.sync-scope.smoke.spec.ts',
    'tests/ui.theming.smoke.spec.ts',
];

export default defineConfig({
    test: {
        root: __dirname,
        globals: true,
        pool: 'threads',
        maxConcurrency: 4,
        testTimeout: 120_000,
        hookTimeout: 120_000,
        // Vitest's default poll budget (1 s) also bounds the FIRST call of the
        // polled function. Under full-parallel load (load average 40+), one
        // Playwright round-trip alone can exceed it, so a poll fails with
        // "function didn't resolve in time" even when the condition already
        // holds. Polls stay condition-based; they only get a realistic budget.
        // Explicit per-poll timeouts still override this.
        expect: { poll: { timeout: 15_000 } },
        globalSetup: ['../scripts/check-lock.mjs', 'helpers/server-guard.ts', '../scripts/vitest-temp-guard.mjs'],
        setupFiles: ['helpers/setup.ts'],
        env: {
            LOTAR_TEST_SILENT: '1',
        },
        reporters: 'default',
        projects: [
            {
                extends: true,
                test: {
                    name: 'parallel',
                    include: ['tests/**/*.smoke.spec.ts'],
                    exclude: [...configDefaults.exclude, ...WALL_CLOCK_SPECS],
                },
            },
            {
                extends: true,
                test: {
                    name: 'wall-clock',
                    include: WALL_CLOCK_SPECS,
                    fileParallelism: false,
                    sequence: { groupOrder: 1 },
                },
            },
        ],
    },
    resolve: {
        alias: {
            '@smoke': path.resolve(__dirname),
            '@smoke/helpers': path.resolve(__dirname, 'helpers'),
        },
    },
});
