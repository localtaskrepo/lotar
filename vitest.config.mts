import { dirname, resolve } from 'path'
import { fileURLToPath } from 'url'
import { defineConfig, mergeConfig } from 'vitest/config'
import baseViteConfig from './vite.config.mts'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

export default mergeConfig(
    baseViteConfig,
    defineConfig({
        test: {
            environment: 'jsdom',
            globals: true,
            root: resolve(__dirname),
            include: ['view/__tests__/**/*.spec.ts'],
        },
    }),
)
