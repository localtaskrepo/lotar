import path from 'node:path';
import { execa } from 'execa';
import { describe, expect, it } from 'vitest';
import { resolveRepositoryRoot } from '../helpers/binary.js';

// DEV-586: a test Vitest times out keeps running, so its own `finally` comes too late. The file's cleanup closes its
// browser and server and removes its workspace, so the run reports the timeout and nothing left behind.
describe('a timed-out test', () => {
    it('leaves no browser profile, server or workspace behind', async () => {
        const repo = path.resolve(resolveRepositoryRoot(), '..');
        const result = await execa(
            process.execPath,
            [
                path.join(repo, 'node_modules/vitest/vitest.mjs'),
                'run',
                '--config',
                path.join(repo, 'smoke/fixtures/timed-out-test/vitest.config.ts'),
                '--reporter',
                'dot',
                '--no-color',
            ],
            { cwd: repo, reject: false, all: true, env: { NO_COLOR: '1', FORCE_COLOR: '0' } },
        );
        expect(result.all).toContain('Test timed out in 5000ms');
        expect(result.all).not.toContain('Tests left');
        expect(result.exitCode).toBe(1);
    });
});
