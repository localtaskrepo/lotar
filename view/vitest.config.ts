import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, mergeConfig } from 'vitest/config'
import baseConfig from '../vite.config.mts'

const __dirname = dirname(fileURLToPath(import.meta.url))

// One time zone for every UI test on every machine, west of UTC so date-only
// values that are wrongly parsed as UTC midnight show the day before. It must
// be set here, in the main process before workers start: assigning TZ inside a
// worker thread does not change that worker's zone (DEV-288 test passed on
// Pacific-time machines and failed on UTC CI runners).
process.env.TZ = 'America/Los_Angeles'

const poolPreference = process.env.VITEST_POOL ?? (process.platform === 'win32' ? 'forks' : 'threads')

export default mergeConfig(
    baseConfig,
    defineConfig({
        test: {
            root: __dirname,
            include: ['__tests__/**/*.spec.ts'],
            pool: poolPreference,
            environment: 'jsdom',
            globals: true,
            globalSetup: ['../scripts/vitest-temp-guard.mjs'],
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
        },
    }),
)
