import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { configDefaults, defineConfig, mergeConfig } from 'vitest/config'
import baseConfig from '../vite.config.mts'

const __dirname = dirname(fileURLToPath(import.meta.url))

// One time zone for every UI test on every machine, west of UTC so date-only
// values that are wrongly parsed as UTC midnight show the day before. It must
// be set here, in the main process before workers start: assigning TZ inside a
// worker thread does not change that worker's zone (DEV-288 test passed on
// Pacific-time machines and failed on UTC CI runners).
process.env.TZ = 'America/Los_Angeles'

// Specs that wait on wall-clock budgets (DEV-785): they failed in full runs under
// load and passed alone (DEV-764). They run in the `wall-clock` project after the
// parallel files finish, one file at a time. A spec that flakes under load joins
// this list instead of getting its own ticket (AGENTS.md).
const WALL_CLOCK_SPECS = ['__tests__/SprintsList.backlogPagination.spec.ts']

const poolPreference = process.env.VITEST_POOL ?? (process.platform === 'win32' ? 'forks' : 'threads')

export default mergeConfig(
    baseConfig,
    defineConfig({
        test: {
            root: __dirname,
            pool: poolPreference,
            environment: 'jsdom',
            globals: true,
            globalSetup: ['../scripts/check-lock.mjs', '../scripts/vitest-temp-guard.mjs'],
            setupFiles: ['__tests__/vitest.setup.ts'],
            // A hang guard, not a speed budget. The slowest specs render for
            // ~1 s on an idle machine, but the machine stalls for tens of
            // seconds at a time (memory pressure, Time Machine's deep scans),
            // with worker startup 50x slower and 0.5 s tests passing 30 s
            // (DEV-402, DEV-779). Same bound as the Rust suite's waits (DEV-753).
            testTimeout: 120_000,
            hookTimeout: 120_000,
            // Report-only coverage (DEV-164): `npm run coverage:ui`. No
            // thresholds; the numbers inform where tests are missing.
            coverage: {
                provider: 'v8',
                include: ['**/*.{ts,vue}'],
                exclude: ['__tests__/**', '**/*.d.ts', 'main.ts'],
                reporter: ['text-summary', 'json-summary', 'html', 'lcov'],
                reportsDirectory: '../target/coverage/ui',
            },
            projects: [
                {
                    extends: true,
                    test: {
                        name: 'parallel',
                        include: ['__tests__/**/*.spec.ts'],
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
    }),
)
