import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, mergeConfig } from 'vitest/config';
import baseConfig from '../vite.config.mts';

const __dirname = dirname(fileURLToPath(import.meta.url));

export default mergeConfig(
    baseConfig,
    defineConfig({
        test: {
            root: __dirname,
            include: ['__tests__/**/*.spec.ts'],
            pool: 'threads',
            environment: 'jsdom',
            globals: true,
            setupFiles: ['__tests__/vitest.setup.ts'],
        },
    }),
);
