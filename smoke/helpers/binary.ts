import fs from 'fs-extra';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const BIN_NAME = process.platform === 'win32' ? 'lotar.exe' : 'lotar';

// Prefer the fast-to-build `smoke` profile binary when present, falling back to
// the full `release` build. This lets `npm run build:smoke` produce the binary
// smoke tests use without paying for the size-optimized release build.
const DEFAULT_BINARY_CANDIDATES: readonly (readonly string[])[] = [
    ['..', 'target', 'smoke', BIN_NAME],
    ['..', 'target', 'release', BIN_NAME],
];

export function resolveRepositoryRoot(): string {
    return path.resolve(__dirname, '..');
}

export function resolveBinaryPath(): string {
    const explicit = process.env.LOTAR_BINARY_PATH || process.env.LOTAR_BIN;
    if (explicit) {
        return explicit;
    }

    const root = resolveRepositoryRoot();
    for (const candidate of DEFAULT_BINARY_CANDIDATES) {
        const resolved = path.resolve(root, ...candidate);
        if (fs.pathExistsSync(resolved)) {
            return resolved;
        }
    }

    // Nothing built yet; point at the preferred candidate for a helpful error.
    return path.resolve(root, ...DEFAULT_BINARY_CANDIDATES[0]);
}

export async function ensureBinaryExists(): Promise<string> {
    const binaryPath = resolveBinaryPath();
    const exists = await fs.pathExists(binaryPath);

    if (!exists) {
        throw new Error(
            `LoTaR binary was not found at ${binaryPath}. Run \`npm run build:smoke\` (fast) or \`npm run build\` before \`npm run smoke\`, or set LOTAR_BINARY_PATH to a custom location.`,
        );
    }

    return binaryPath;
}
