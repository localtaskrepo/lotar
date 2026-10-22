import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-119: "Remove dead anchors" previews and then removes anchors whose
// file is gone, keeping live anchors and non-code references.
describe.concurrent('UI scan dead-anchor cleanup (DEV-119)', () => {
    it('previews and removes dead anchors from the Scan page', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: {
            '.tasks/config.yml': 'default.project: SCAN\ndefault.reporter: fixture-user\nauto.set_reporter: false\n',
            '.tasks/SCAN/config.yml': 'project.name: Scan fixture\n',
            '.tasks/SCAN/1.yml': [
                'title: Anchored task',
                'status: Todo',
                'priority: Medium',
                'type: Feature',
                'reporter: fixture-user',
                'created: 2026-10-01T00:00:00Z',
                'modified: 2026-10-01T00:00:00Z',
                'references:',
                '- code: live.ts#1',
                '- code: gone.ts#4',
                '- link: https://example.com/spec',
                '',
            ].join('\n'),
            'live.ts': '// SCAN-1 still mentioned here\n',
        } });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/scan`, async page => {
                    await page.getByLabel('Remove dead anchors', { exact: true }).check();
                    await page.getByRole('button', { name: 'Dry run', exact: true }).click();
                    const info = page.getByTestId('scan-info');
                    await info.waitFor({ timeout: 10_000 });
                    expect(await info.textContent()).toContain('Would remove dead anchor gone.ts#4 from SCAN-1.');
                    expect(await workspace.readTaskYaml('SCAN-1')).toContain('gone.ts#4');

                    await page.getByRole('button', { name: 'Run', exact: true }).click();
                    const dialog = page.getByRole('dialog', { name: 'Run scan?', exact: true });
                    await dialog.waitFor({ state: 'visible' });
                    await dialog.getByRole('button', { name: 'Run', exact: true }).click();
                    await expect.poll(async () => (await info.textContent()) ?? '', { timeout: 10_000 })
                        .toContain('Removed dead anchor gone.ts#4 from SCAN-1.');
                    const yaml = await workspace.readTaskYaml('SCAN-1');
                    expect(yaml).not.toContain('gone.ts');
                    expect(yaml).toContain('live.ts#1');
                    expect(yaml).toContain('https://example.com/spec');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
