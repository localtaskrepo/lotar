import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-142: with "Use the same search and filters on every page" on, a search
// made on one page follows the user to the other task views.
function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': stringify({
            issue: { states: ['Todo', 'Done'], priorities: ['Low', 'Medium', 'High'], types: ['Feature'] },
            default: { reporter: 'Fixture', project: 'SHR' },
            auto: { set_reporter: false },
        }),
        '.tasks/SHR/config.yml': 'project.name: Shared fixture\n',
    };
    for (const [number, title] of [
        [1, 'Alpha checkout flow'],
        [2, 'Beta onboarding'],
    ] as const) {
        files[`.tasks/SHR/${number}.yml`] = stringify({
            title,
            status: 'Todo',
            priority: 'Medium',
            type: 'Feature',
            reporter: 'Fixture',
            created: '2026-01-01T00:00:00Z',
            modified: '2026-01-01T00:00:00Z',
        });
    }
    return files;
}

describe('Shared search and filters (DEV-142)', () => {
    it('carries the search between pages only while the preference is on', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        const server = await startLotarServer(workspace);
        try {
            await withPage(`${server.url}/preferences`, async (page) => {
                const nav = (name: string) => page.getByRole('link', { name, exact: true }).first().click();
                const search = page.getByTestId('filter-search');
                const share = page.getByTestId('preferences-share-filters');

                await share.waitFor({ state: 'visible' });
                expect(await share.isChecked()).toBe(false);
                await share.check();

                await nav('Tasks');
                await search.waitFor({ state: 'visible' });
                await search.fill('alpha');
                await search.press('Enter');
                await expect.poll(() => page.getByText('Beta onboarding', { exact: true }).count()).toBe(0);

                for (const view of ['Boards', 'Calendar', 'Sprints']) {
                    await nav(view);
                    await expect.poll(() => search.inputValue()).toBe('alpha');
                }
                await nav('Boards');
                await expect.poll(() => page.getByText('Alpha checkout flow', { exact: true }).count()).toBe(1);
                await expect.poll(() => page.getByText('Beta onboarding', { exact: true }).count()).toBe(0);

                // Off again: Boards goes back to its own (empty) search.
                await nav('Preferences');
                await share.uncheck();
                await nav('Boards');
                await search.waitFor({ state: 'visible' });
                await expect.poll(() => page.getByText('Beta onboarding', { exact: true }).count()).toBe(1);
                expect(await search.inputValue()).toBe('');
            });
        } finally {
            await server.stop();
            await workspace.dispose();
        }
    });
});
