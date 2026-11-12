import { describe, it } from 'vitest';
import { parse } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe('UI tasks in another repo (DEV-389)', () => {
    it('shows a cross-repo dependency with its status and link, and adds one by reference', async ({ expect }) => {
        const other = await SmokeWorkspace.create({
            name: 'ui-cross-repo-other-',
            seedFiles: { '.tasks/config.yml': 'default.project: EXT\nserver.public_url: https://other.example\n' },
        });
        const workspace = await SmokeWorkspace.create({ name: 'ui-cross-repo-' });
        try {
            await other.addTask('Release elsewhere');
            await other.addTask('Docs elsewhere');
            await workspace.write(
                '.tasks/config.yml',
                `default.project: REL\nrepos:\n  other: ${JSON.stringify(other.root)}\n`,
            );
            const source = await workspace.addTask('Waits for the other repo');
            await workspace.runLotar([
                'task',
                'relationships',
                'add',
                source.id,
                'other:EXT-1',
                '--kind',
                'depends-on',
            ]);
            const server = await startLotarServer(workspace);
            try {
                await withPage('about:blank', async (page) => {
                    await page.setViewportSize({ width: 1440, height: 1000 });
                    await page.goto(`${server.url}/task/${source.id}`);
                    await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                    const depends = page.getByRole('region', { name: 'Depends on', exact: true });
                    const link = depends.getByRole('link', { name: 'other:EXT-1', exact: true });
                    expect(await link.getAttribute('href')).toBe('https://other.example/task/EXT-1');
                    expect(await link.getAttribute('target')).toBe('_blank');
                    expect(await link.getAttribute('title')).toContain('[Todo] Release elsewhere');
                    expect(await depends.locator('.relationship-status').textContent()).toBe('Todo');

                    const related = page.getByRole('region', { name: 'Related', exact: true });
                    await related.getByRole('button', { name: 'Add related', exact: true }).click();
                    await related.getByRole('combobox', { name: 'Add related', exact: true }).fill('other:EXT-2');
                    const added = page.waitForResponse(
                        (response) => new URL(response.url()).pathname === '/api/relationships/mutate',
                    );
                    await related.getByRole('combobox', { name: 'Add related', exact: true }).press('Enter');
                    expect((await added).status()).toBe(200);
                    await related.getByRole('link', { name: 'other:EXT-2', exact: true }).waitFor();
                    const stored = parse(await workspace.readTaskYaml(source.id)) as {
                        relationships: Record<string, string[]>;
                    };
                    expect(stored.relationships.related).toEqual(['other:EXT-2']);
                    // The other repo is never written.
                    expect(await other.readTaskYaml('EXT-2')).not.toContain('related');

                    // Only Depends on and Related take a task in another repo.
                    const posts: string[] = [];
                    page.on('request', (request) => {
                        if (request.method() === 'POST') posts.push(new URL(request.url()).pathname);
                    });
                    await related.getByRole('button', { name: 'Add related', exact: true }).click();
                    await related.getByRole('combobox').waitFor({ state: 'detached' });
                    const chip = (await depends.locator('li[draggable="true"] .relationship-status').boundingBox())!;
                    const blocks = (await page.getByRole('region', { name: 'Blocks', exact: true }).boundingBox())!;
                    await page.mouse.move(chip.x + 4, chip.y + 4);
                    await page.mouse.down();
                    await page.mouse.move(chip.x + 30, chip.y + 20);
                    await page.mouse.move(blocks.x + 8, blocks.y + 8, { steps: 5 });
                    // Chromium needs a second move over the target before it drops.
                    await page.mouse.move(blocks.x + 12, blocks.y + 12);
                    await page.mouse.up();
                    await page.getByRole('alert').getByText('only be in Depends on or Related').waitFor();
                    expect(posts).toEqual([]);

                    // The other repo's status is read again on the next visit.
                    await other.runLotar(['status', 'EXT-1', 'Done']);
                    await page.reload();
                    await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                    await expect
                        .poll(() =>
                            page
                                .getByRole('region', { name: 'Depends on', exact: true })
                                .locator('.relationship-status')
                                .textContent(),
                        )
                        .toBe('Done');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await other.dispose();
        }
    });
});
