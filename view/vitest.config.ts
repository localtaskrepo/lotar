import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, mergeConfig } from 'vitest/config';
import baseConfig from '../vite.config.mts';

const __dirname = dirname(fileURLToPath(import.meta.url));

const nodeMajorVersion = Number.parseInt(process.versions.node.split('.')[0] ?? '0', 10);

function createLocalStorageFile(prefix: string) {
    if (Number.isNaN(nodeMajorVersion) || nodeMajorVersion < 22) {
        return undefined;
    }

    try {
        const dir = mkdtempSync(join(tmpdir(), prefix));
        const file = join(dir, 'localstorage.json');
        writeFileSync(file, '', { flag: 'w' });
        return file;
    } catch {
        return undefined;
    }
}

const localStorageFile = createLocalStorageFile('lotar-vitest-');
const poolPreference = process.env.VITEST_POOL ?? (process.platform === 'win32' ? 'forks' : 'threads');

if (localStorageFile) {
    const flag = `--localstorage-file=${localStorageFile}`;
    if (!process.execArgv.includes(flag)) {
        process.execArgv.push(flag);
    }
}

export default mergeConfig(
    baseConfig,
    defineConfig({
        test: {
            root: __dirname,
            include: ['__tests__/**/*.spec.ts'],
            pool: poolPreference,
            environment: 'jsdom',
            globals: true,
            setupFiles: ['__tests__/vitest.setup.ts'],
        },
    }),
);
