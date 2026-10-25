import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-167/DEV-197: the Tasks page search uses the shared rules (every word,
// phrases, exclusions, typos, field scopes, regex), and Best match orders the
// results by relevance.
function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': stringify({
            issue: { states: ['Todo', 'Done'], priorities: ['Low', 'Medium', 'High'], types: ['Feature', 'Bug'] },
            default: { reporter: 'Fixture', project: 'SRC' },
            auto: { set_reporter: false },
        }),
        '.tasks/SRC/config.yml': 'project.name: Search fixture\n',
    };
    const tasks: Array<[number, string, string?]> = [
        [1, 'Fix the login redirect loop', 'Happens after OAuth sign-in'],
        [2, 'Login'],
        [3, 'Login page copy'],
        [4, 'Redirect users after logout'],
    ];
    for (const [number, title, description] of tasks) {
        files[`.tasks/SRC/${number}.yml`] = stringify({
            title,
            ...(description ? { description } : {}),
            status: 'Todo',
            priority: 'Medium',
            type: 'Feature',
            reporter: 'Fixture',
            created: '2026-01-01T00:00:00Z',
            modified: `2026-01-0${number}T00:00:00Z`,
        });
    }
    return files;
}

describe('Task search (DEV-167)', () => {
    it('matches every word, phrases, exclusions and typos, and orders by best match on demand', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        const server = await startLotarServer(workspace);
        try {
            await withPage(`${server.url}/`, async (page) => {
                const listQueries: string[] = [];
                page.on('request', (request) => {
                    const url = new URL(request.url());
                    if (url.pathname === '/api/tasks/list') listQueries.push(url.search);
                });
                // With a single project the table shows bare task numbers.
                const ids = () => page.locator('tbody tr td:first-child strong').allTextContents();
                const search = page.getByTestId('filter-search');
                await search.waitFor({ state: 'visible' });
                await expect.poll(ids).toHaveLength(4);

                const searchFor = async (text: string) => {
                    await search.fill(text);
                    await search.press('Enter');
                };

                await searchFor('redirect login');
                await expect.poll(ids).toEqual(['1']);
                await searchFor('"login redirect"');
                await expect.poll(ids).toEqual(['1']);
                const sentQueries = () => listQueries.map((query) => new URLSearchParams(query).get('q'));
                await expect.poll(sentQueries).toContain('"login redirect"');
                await searchFor('login -oauth');
                await expect.poll(ids).toEqual(['3', '2']);
                await searchFor('redirct');
                await expect.poll(ids).toEqual(['4', '1']);

                // Default order (modified desc) until Best match is switched on.
                await searchFor('login');
                await expect.poll(ids).toEqual(['3', '2', '1']);
                const bestMatch = page.getByTestId('tasks-best-match');
                await bestMatch.click();
                await expect.poll(ids).toEqual(['2', '3', '1']);
                expect(await bestMatch.getAttribute('aria-pressed')).toBe('true');
                await bestMatch.click();
                await expect.poll(ids).toEqual(['3', '2', '1']);

                // DEV-197: field scopes and regular expressions, and a clear
                // message for an invalid pattern.
                await searchFor('title:/^login/');
                await expect.poll(ids).toEqual(['3', '2']);
                await searchFor('desc:oauth');
                await expect.poll(ids).toEqual(['1']);
                await searchFor('/fix(/');
                const searchError = page.getByTestId('tasks-search-error');
                await searchError.waitFor({ state: 'visible' });
                expect(await searchError.textContent()).toContain('Invalid search pattern /fix(/');
                expect(await page.getByRole('button', { name: 'Retry', exact: true }).count()).toBe(0);

                // The toggle only exists while searching.
                await page.getByRole('button', { name: 'Clear search', exact: true }).click();
                await expect.poll(ids).toHaveLength(4);
                expect(await bestMatch.count()).toBe(0);
            });
        } finally {
            await server.stop();
            await workspace.dispose();
        }
    });
});
