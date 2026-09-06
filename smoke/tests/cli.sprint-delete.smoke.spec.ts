import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe('Sprint deletion confirmation', () => {
    it.each(['text', 'json'])('requires explicit force in non-interactive %s output', async (format) => {
        const original = 'plan:\n  label: Preserve until confirmed\n';
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/@sprints/1.yml': original },
        });

        try {
            const sprintPath = path.join(workspace.tasksDir, '@sprints', '1.yml');
            const refused = await workspace.runLotar(
                ['--format', format, 'sprint', 'delete', '1', '--cleanup-missing'],
                { acceptExitCodes: [1], timeout: 5_000 },
            );
            expect(refused.exitCode).toBe(1);
            expect(String(refused.stderr)).toContain('--force');
            expect(await fs.readFile(sprintPath, 'utf8')).toBe(original);

            const confirmed = await workspace.runLotar(
                ['--format', format, 'sprint', 'delete', '1', '--force'],
            );
            expect(confirmed.exitCode).toBe(0);
            expect(await fs.pathExists(sprintPath)).toBe(false);
            if (format === 'json') {
                expect(JSON.parse(String(confirmed.stdout))).toMatchObject({
                    status: 'ok', deleted: true, sprint_id: 1,
                });
            } else {
                expect(String(confirmed.stdout)).toContain('Deleted');
            }
        } finally {
            await workspace.dispose();
        }
    });
});
