import fs from 'fs-extra';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const DEFAULT_BINARY_RELATIVE = ['..', 'target', 'release', process.platform === 'win32' ? 'lotar.exe' : 'lotar'];

export function resolveRepositoryRoot(): string {
    return path.resolve(__dirname, '..');
}

export function resolveBinaryPath(): string {
    const explicit = process.env.LOTAR_BINARY_PATH || process.env.LOTAR_BIN;
    if (explicit) {
        return explicit;
    }

    return path.resolve(resolveRepositoryRoot(), ...DEFAULT_BINARY_RELATIVE);
}

export async function ensureBinaryExists(): Promise<string> {
    const binaryPath = resolveBinaryPath();
    const exists = await fs.pathExists(binaryPath);

    if (!exists) {
        throw new Error(
            `LoTaR binary was not found at ${binaryPath}. Run \`npm run build\` before \`npm run smoke\`, or set LOTAR_BINARY_PATH to a custom location.`,
        );
    }

    return binaryPath;
}
