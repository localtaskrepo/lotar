import { it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

it('selects a Board project directly from the empty state on mobile and desktop', async ({ expect }) => {
    const workspace = await SmokeWorkspace.create({ name: 'board-project-picker-' });
    try {
        await workspace.addTask('First project board task', { args: ['--project=ONE'] });
        await workspace.addTask('Second project board task', { args: ['--project=TWO'] });
        const server = await startLotarServer(workspace);
        try {
            await withPage(`${server.url}/boards`, async (page) => {
                const selector = page.locator('#board-project-select');
                await expect.poll(() => selector.isVisible()).toBe(true);
                await expect.poll(() => selector.isEnabled()).toBe(true);
                expect(await selector.inputValue()).toBe('');
                expect(await selector.evaluate(element =>
                    Array.from((element as HTMLSelectElement).labels ?? []).map(label => label.textContent?.trim()),
                )).toEqual(['Project']);
                expect(await selector.locator('option').evaluateAll(options => options.map(option =>
                    (option as HTMLOptionElement).value,
                ))).toEqual(['', 'ONE', 'TWO']);

                await page.setViewportSize({ width: 375, height: 812 });
                const box = await selector.boundingBox();
                expect(box).not.toBeNull();
                expect(box!.x).toBeGreaterThanOrEqual(0);
                expect(box!.x + box!.width).toBeLessThanOrEqual(375);
                await selector.selectOption('TWO');
                await page.waitForURL(url => url.pathname === '/boards' && url.searchParams.get('project') === 'TWO');
                const cards = page.locator('article.card.task');
                await expect.poll(() => cards.filter({ hasText: 'Second project board task' }).count()).toBe(1);
                expect(await cards.filter({ hasText: 'First project board task' }).count()).toBe(0);
                expect(await selector.count()).toBe(0);

                await page.goBack();
                await expect.poll(() => selector.isVisible()).toBe(true);
                expect(new URL(page.url()).searchParams.get('project')).toBeNull();
                await page.setViewportSize({ width: 1280, height: 800 });
                await selector.selectOption('ONE');
                await page.waitForURL(url => url.searchParams.get('project') === 'ONE');
                await expect.poll(() => cards.filter({ hasText: 'First project board task' }).count()).toBe(1);
                expect(await cards.filter({ hasText: 'Second project board task' }).count()).toBe(0);
            });
        } finally {
            await server.stop();
        }
    } finally {
        await workspace.dispose();
    }
});

it('shows a disabled chooser rather than an unusable selection when no projects exist', async ({ expect }) => {
    const workspace = await SmokeWorkspace.create({ name: 'board-no-projects-' });
    try {
        const server = await startLotarServer(workspace);
        try {
            await withPage(`${server.url}/boards`, async (page) => {
                const selector = page.locator('#board-project-select');
                await expect.poll(() => selector.isVisible()).toBe(true);
                await expect.poll(() => selector.isDisabled()).toBe(true);
                await expect.poll(() => selector.textContent()).toContain('No projects available');
                expect(await page.locator('article.card.task').count()).toBe(0);
            });
        } finally {
            await server.stop();
        }
    } finally {
        await workspace.dispose();
    }
});
