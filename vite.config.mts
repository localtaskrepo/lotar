import vue from '@vitejs/plugin-vue'
import { readFileSync } from 'fs'
import { dirname, resolve } from 'path'
import { fileURLToPath } from 'url'
import type { UserConfig } from 'vite'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

// Read Cargo.toml version once and inject into import.meta.env
function readCargoVersion(): string {
    try {
        const cargoToml = readFileSync(resolve(__dirname, 'Cargo.toml'), 'utf8')
        // naive regex: first version = "x.y.z" occurrence
        const m = cargoToml.match(/^\s*version\s*=\s*"(.*?)"/m)
        return m?.[1] || '0.0.0'
    } catch {
        return '0.0.0'
    }
}
const CARGO_VERSION = readCargoVersion()

const baseConfig: UserConfig = {
    plugins: [vue()],
    root: resolve(__dirname, 'view'),
    build: {
        outDir: resolve(__dirname, 'target/web'),
        emptyOutDir: true,
    },
    define: {
        'import.meta.env.VITE_CARGO_VERSION': JSON.stringify(CARGO_VERSION),
    },
    server: {
        port: 5173,
        open: false,
    },
}

export default baseConfig
export { baseConfig }
