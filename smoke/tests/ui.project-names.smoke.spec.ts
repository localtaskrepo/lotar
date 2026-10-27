import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const LONG_NAME = 'Platform Engineering and Developer Experience Infrastructure';

// DEV-184: pages show the project's name instead of its prefix. A long name
// must not widen the page or push the create panel past a phone screen.
describe.concurrent('UI project names', () => {
    for (const width of [1280, 390]) {
        it(`shows project names without overflowing at ${width}px`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/PLAT/config.yml': `project:\n  name: ${LONG_NAME}\n` },
            });
            try {
                await workspace.addTask('Wire up the onboarding flow', { args: ['--project=PLAT'] });
                // A second project, so the project filter shows as a chip.
                await workspace.addTask('Rotate the on-call schedule', { args: ['--project=OPS'] });
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/boards?project=PLAT`, async (page) => {
                        await page.setViewportSize({ width, height: 900 });
                        const heading = page.locator('h1 .app-page-header__project');
                        await expect.poll(() => heading.textContent(), { timeout: 15_000 }).toContain(LONG_NAME);
                        expect(await heading.getAttribute('title')).toContain('(PLAT)');
                        await expect
                            .poll(() => page.getByTestId('filter-chips').textContent())
                            .toContain('Platform Engineering');

                        const overflow = () =>
                            page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
                        expect(await overflow()).toBeLessThanOrEqual(0);
                        // The heading stays inside its card, cut short if needed.
                        const box = (await heading.boundingBox())!;
                        expect(box.x + box.width).toBeLessThanOrEqual(width);

                        await page.getByTestId('global-new-task').click();
                        const panel = page.locator('.task-panel');
                        await panel.locator('input[placeholder="Title"]').waitFor({ timeout: 10_000 });
                        // Every form group fits the panel's width.
                        const spill = await panel.evaluate((root) => {
                            const limit = root.getBoundingClientRect().right;
                            return [...root.querySelectorAll('.task-panel__group')]
                                .map((group) => group.getBoundingClientRect().right - limit)
                                .filter((excess) => excess > 1);
                        });
                        expect(spill).toEqual([]);

                        // The 404 page also fits, even with a long unknown path.
                        await page.goto(`${server.url}/a/very/long/missing/path/that/keeps/going?project=PLAT`);
                        await page.locator('#not-found-title').waitFor({ timeout: 10_000 });
                        expect(await overflow()).toBeLessThanOrEqual(0);
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }
});
