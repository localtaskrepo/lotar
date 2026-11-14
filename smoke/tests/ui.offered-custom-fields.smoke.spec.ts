import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-485: a wildcard project offers the custom fields its tasks use.
function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': '{}\n',
        '.tasks/INB/config.yml': stringify({ project: { name: 'Inbox' } }),
    };
    const fields = [
        { source: 'DEV-1', source_repo: 'lotar', finding: 'inbox:DEV-1' },
        { source: 'DEV-2', Source_Repo: 'lotar' },
        { source: 'STD-3', source_repo: 'standards' },
    ];
    for (const [index, custom_fields] of fields.entries()) {
        files[`.tasks/INB/${index + 1}.yml`] = stringify({
            title: `Inbox ${index + 1}`,
            status: 'Todo',
            priority: 'Medium',
            type: 'Feature',
            created: '2026-11-13T00:00:00Z',
            modified: '2026-11-13T00:00:00Z',
            custom_fields,
        });
    }
    return files;
}

describe('Offered custom fields (DEV-485)', () => {
    it('offers used fields on a wildcard project until the setting says config', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'dev485-offered-', seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                const fieldOptions = async (page: import('playwright').Page) =>
                    (
                        await page
                            .getByTestId('board-groupby')
                            .locator('option')
                            .evaluateAll((options) => options.map((option) => (option as HTMLOptionElement).value))
                    ).filter((value) => value.startsWith('field:'));

                await withPage(`${server.url}/boards?project=INB`, async (page) => {
                    await expect.poll(() => page.locator('article.task').count()).toBe(3);
                    await expect
                        .poll(() => fieldOptions(page))
                        .toEqual(['field:source', 'field:source_repo', 'field:finding']);
                    // Source_Repo and source_repo are one field: two lotar cards in one lane.
                    await page.getByTestId('board-groupby').selectOption('field:source_repo');
                    await expect
                        .poll(() => page.locator('.swimlane-label').allTextContents())
                        .toEqual(['lotar', 'standards']);
                    expect(await page.locator('.swimlane-count').allTextContents()).toEqual(['2', '1']);
                });

                await withPage(`${server.url}/config?section=fields&project=INB`, async (page) => {
                    const select = page.getByTestId('config-custom-fields-from').locator('select');
                    await select.waitFor({ timeout: 15_000 });
                    expect(await select.inputValue()).toBe('');
                    await select.selectOption('config');
                    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
                    await expect
                        .poll(async () => parse(await workspace.read('.tasks/INB/config.yml'))?.ui?.custom_fields_from)
                        .toBe('config');
                });

                await withPage(`${server.url}/boards?project=INB`, async (page) => {
                    // The board renders cards only after the config has loaded.
                    await expect.poll(() => page.locator('article.task').count()).toBe(3);
                    expect(await fieldOptions(page)).toEqual([]);
                });
                const show = await fetch(`${server.url}/api/config/show?project=INB`).then((res) => res.json());
                expect(show.data.custom_fields_offered).toEqual([]);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
