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
