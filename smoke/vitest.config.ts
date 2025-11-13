import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const nodeMajorVersion = Number.parseInt(process.versions.node.split('.')[0] ?? '0', 10);

function createLocalStorageFile(prefix: string) {
    if (Number.isNaN(nodeMajorVersion) || nodeMajorVersion < 22) {
        return undefined;
    }

    try {
        const dir = mkdtempSync(path.join(tmpdir(), prefix));
        const file = path.join(dir, 'localstorage.json');
        writeFileSync(file, '', { flag: 'w' });
        return file;
    } catch {
        return undefined;
    }
}

const localStorageFile = createLocalStorageFile('lotar-vitest-smoke-');

if (localStorageFile) {
    const flag = `--localstorage-file=${localStorageFile}`;
    if (!process.execArgv.includes(flag)) {
        process.execArgv.push(flag);
    }
}

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
