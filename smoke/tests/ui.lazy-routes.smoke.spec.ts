import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-274: pages other than the default Tasks list load as separate chunks on
// first visit. Every page must still render, from a direct load and from the
// navigation, and the Tasks page must not pull in the others.

const PAGES = [
    { path: '/sprints', title: 'Sprints' },
    { path: '/boards', title: 'Boards' },
    { path: '/calendar', title: 'Calendar' },
    { path: '/dependencies', title: 'Dependencies' },
    { path: '/insights', title: 'Insights' },
    { path: '/agents', title: 'Agents' },
    { path: '/automations', title: 'Automations' },
    { path: '/sync', title: 'Sync' },
    { path: '/scan', title: 'Scan' },
    { path: '/config', title: 'Configuration' },
    { path: '/preferences', title: 'Preferences' },
] as const;

describe.concurrent('UI route chunks (DEV-274)', () => {
    it('loads each page chunk on demand and renders every route', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': 'default.project: LAZY\nauto.set_reporter: false\n' },
        });
        try {
            await workspace.addTask('Lazy fixture');
            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    const errors: string[] = [];
                    page.on('pageerror', (error) => errors.push(error.message));
                    const scripts: string[] = [];
                    page.on('request', (request) => {
                        const pathname = new URL(request.url()).pathname;
                        if (request.resourceType() === 'script' || pathname.endsWith('.js')) scripts.push(pathname);
                    });
                    const heading = page.locator('h1.app-page-header__title');

                    // Fresh load of the default page.
                    await page.reload();
                    await heading.filter({ hasText: 'Tasks' }).waitFor({ timeout: 15_000 });
                    const entryScripts = scripts.length;
                    expect(entryScripts).toBeGreaterThan(0);
                    expect(scripts.filter((script) => /\/(ProjectInsights|ConfigView)-/.test(script))).toEqual([]);

                    // In-app navigation fetches the page's own chunk.
                    await page.click('a.nav__link[href="/insights"]');
                    await heading.filter({ hasText: 'Insights' }).waitFor({ timeout: 15_000 });
                    expect(scripts.slice(entryScripts).some((script) => /\/ProjectInsights-/.test(script))).toBe(true);

                    // Direct loads of every page.
                    for (const { path, title } of PAGES) {
                        await page.goto(`${server.url}${path}`);
                        await heading.filter({ hasText: title }).first().waitFor({ timeout: 15_000 });
                    }
                    await page.goto(`${server.url}/no-such-page`);
                    await page.locator('#not-found-title').waitFor({ timeout: 15_000 });

                    expect(errors).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
