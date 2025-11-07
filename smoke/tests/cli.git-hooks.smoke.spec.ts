import fs from 'fs-extra';
import path from 'node:path';
import os from 'node:os';
import { describe, expect, it } from 'vitest';
import { SmokeWorkspace } from '../helpers/workspace.js';

async function copyRepositoryHooks(targetDir: string): Promise<void> {
    const repoRoot = process.cwd();
    const source = path.join(repoRoot, '.githooks');
    await fs.copy(source, targetDir, { overwrite: true });
}

describe.concurrent('Git hooks smoke scenarios', () => {
    it('installs bundled git hooks once and reports idempotency on repeat invocations', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'git-hooks-smoke-' });

        try {
            await workspace.initGit();
            await copyRepositoryHooks(path.join(workspace.root, '.githooks'));

            const first = await workspace.runLotar(['git', 'hooks', 'install']);
            expect(first.exitCode).toBe(0);
            expect(first.stdout).toContain("Configured git core.hooksPath to '.githooks'");

            const config = await workspace.runGit(['config', '--local', '--get', 'core.hooksPath']);
            const hooksPath = (config.stdout ?? '').trim();
            expect(hooksPath).toBe('.githooks');

            const second = await workspace.runLotar(['git', 'hooks', 'install']);
            expect(second.exitCode).toBe(0);
            expect(second.stdout).toContain('already configured');

            if (os.platform() !== 'win32') {
                const script = path.join(workspace.root, '.githooks', 'pre-commit');
                const stat = await fs.stat(script);
                expect(stat.mode & 0o111).not.toBe(0);
            }
        } finally {
            await workspace.dispose();
        }
    });
});
