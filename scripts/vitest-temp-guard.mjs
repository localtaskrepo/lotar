// Vitest global setup for the UI and smoke suites (DEV-315): tests clean up after themselves. The run gets a fresh
// temp folder (workers, the lotar binary and browsers inherit it), and the run fails if anything is left in it.
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';

// os.tmpdir() reads TMPDIR on POSIX and TEMP/TMP on Windows.
const TEMP_VARS = ['TMPDIR', 'TEMP', 'TMP'];

export default function setup() {
    const root = mkdtempSync(path.join(tmpdir(), 'lotar-vitest-temp-'));
    for (const name of TEMP_VARS) {
        process.env[name] = root;
    }

    return () => {
        const leftovers = readdirSync(root);
        rmSync(root, { recursive: true, force: true });
        if (leftovers.length > 0) {
            throw new Error(
                `Tests left ${leftovers.length} entries in the temp folder; remove them where they are created: ${leftovers.join(', ')}`,
            );
        }
    };
}
