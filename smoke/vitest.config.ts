import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export default defineConfig({
    test: {
        root: __dirname,
        include: ['tests/**/*.smoke.spec.ts'],
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
        globalSetup: ['../scripts/vitest-temp-guard.mjs'],
        setupFiles: ['helpers/setup.ts'],
        env: {
            LOTAR_TEST_SILENT: '1',
        },
        reporters: 'default',
    },
    resolve: {
        alias: {
            '@smoke': path.resolve(__dirname),
            '@smoke/helpers': path.resolve(__dirname, 'helpers'),
        },
    },
});
