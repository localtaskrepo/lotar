import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// Run by tests/harness.failed-test-cleanup.smoke.spec.ts (DEV-586): the smoke suite's guards and setup, around one
// test that times out while its browser, server and workspace are open.
const smoke = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

export default defineConfig({
    test: {
        root: path.dirname(fileURLToPath(import.meta.url)),
        include: ['*.fixture.ts'],
        testTimeout: 5_000,
        hookTimeout: 60_000,
        globalSetup: [
            path.join(smoke, 'helpers/server-guard.ts'),
            path.join(smoke, '../scripts/vitest-temp-guard.mjs'),
        ],
        setupFiles: [path.join(smoke, 'helpers/setup.ts')],
    },
});
