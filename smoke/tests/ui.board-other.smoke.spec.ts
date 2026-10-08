import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function seed(unknownCount = 32): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': '{}\n',
        '.tasks/BRD/config.yml': stringify({
            project_name: 'Board fixtures',
            issue: { states: ['Todo', 'Done', '__other__'], priorities: ['Low', 'High'], types: ['Task', 'Bug'], done_states: ['Done'] },
        }),
        '.tasks/ALT/config.yml': 'project_name: Alternate\n',
    };
    for (let index = 1; index <= unknownCount; index++) {
        files[`.tasks/BRD/${index}.yml`] = stringify({
            title: `Unknown ${String(index).padStart(3, '0')}`,
            status: 'LegacyWorkflow',
            priority: index % 2 ? 'High' : 'Low',
            type: index % 2 ? 'Bug' : 'Task',
            reporter: 'Smoke',
            assignee: index === 1 ? null : 'Ben',
            created: '2026-09-01T00:00:00Z',
            modified: new Date(Date.UTC(2026, 9, 1, 0, index)).toISOString(),
        });
    }
    for (const [number, title, status] of [[101, 'Known Todo', 'Todo'], [102, 'Configured Other', '__other__']] as const) {
        files[`.tasks/BRD/${number}.yml`] = stringify({
            title, status, priority: 'Low', type: 'Task', reporter: 'Smoke', assignee: 'Ada',
            created: '2026-09-01T00:00:00Z', modified: '2026-10-01T00:00:00Z',
        });
    }
    files['.tasks/ALT/1.yml'] = stringify({ title: 'Alternate unknown', status: 'OldAlternate', priority: 'Low', type: 'Task', reporter: 'Smoke', created: '2026-09-01T00:00:00Z', modified: '2026-10-01T00:00:00Z' });
    return files;
}

describe.concurrent('Board Other across supported rendering modes', () => {
    it('orders and paginates Other, includes it in every swimlane dimension, and keeps project boundaries', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'board-other-modes-', seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=BRD`, async page => {
                    const other = page.locator('.column[data-column-kind="other"]');
                    await expect.poll(() => other.locator('article.task').count()).toBe(30);
                    expect(await other.locator('article.task .title').allTextContents()).toEqual(
                        Array.from({ length: 30 }, (_, index) => `Unknown ${String(32 - index).padStart(3, '0')}`),
                    );
                    await other.locator('.show-more-btn').click();
                    await expect.poll(() => other.locator('article.task').count()).toBe(32);
                    expect(await page.locator('[data-column-kind="status"][data-status="__other__"] article.task .title').allTextContents()).toEqual(['Configured Other']);

                    for (const dimension of ['assignee', 'priority', 'type']) {
                        await page.getByTestId('board-groupby').selectOption(dimension);
                        const groupedOther = page.locator('.column-group-cell[data-column-kind="other"] article.task');
                        await expect.poll(() => groupedOther.count()).toBe(32);
                        expect(await page.locator('.swimlane-count').evaluateAll(nodes => nodes.reduce((sum, node) => sum + Number(node.textContent), 0))).toBe(34);
                        expect(await page.locator('.board-col-header[data-column-kind="other"]').count()).toBe(1);
                        expect(await page.locator('article.task').count()).toBe(34);
                    }
                    await page.evaluate(() => window.scrollTo(0, 0));
                    await page.screenshot({ path: path.join(process.cwd(), 'target/smoke-artifacts/board-other/grouped-desktop.png'), animations: 'disabled' });
                    await page.setViewportSize({ width: 375, height: 812 });
                    const control = await page.getByTestId('board-groupby').boundingBox();
                    expect(control).not.toBeNull();
                    expect(control!.width).toBeLessThanOrEqual(375);
                    await page.locator('.column-group-cell[data-column-kind="other"]').first().scrollIntoViewIfNeeded();
                    await page.evaluate(() => window.scrollTo(window.scrollX, 0));
                    await page.screenshot({ path: path.join(process.cwd(), 'target/smoke-artifacts/board-other/grouped-mobile.png'), animations: 'disabled' });

                    await page.goto(`${server.url}/boards?project=ALT`);
                    await expect.poll(() => page.locator('[data-column-kind="other"] article.task').count()).toBe(1);
                    expect(await page.locator('article.task .title').allTextContents()).toEqual(['Alternate unknown']);
                });
            } finally { await server.stop(); }
        } finally { await workspace.dispose(); }
    });

    it('uses the same server-filtered membership for Other in flat and grouped views', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'board-other-filter-', seedFiles: seed(3) });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=BRD`, async page => {
                    await expect.poll(() => page.locator('article.task').count()).toBe(5);
                    await page.getByTestId('filter-toggle').click();
                    await page.getByRole('button', { name: 'No assignee', exact: true }).click();
                    await expect.poll(() => page.locator('article.task').count()).toBe(1);
                    expect(await page.locator('article.task .title').allTextContents()).toEqual(['Unknown 001']);
                    await page.getByTestId('board-groupby').selectOption('assignee');
                    await expect.poll(() => page.locator('.column-group-cell[data-column-kind="other"] article.task').count()).toBe(1);
                    expect(await page.locator('.swimlane-count').allTextContents()).toEqual(['1']);
                });
            } finally { await server.stop(); }
        } finally { await workspace.dispose(); }
    });

    it('never writes the synthetic status but permits the configured literal __other__', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'board-other-drop-', seedFiles: seed(2) });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=BRD`, async page => {
                    const card = page.locator('article.task').filter({ hasText: 'Known Todo' });
                    await expect.poll(() => card.count()).toBe(1);
                    const writes: string[] = [];
                    page.on('request', request => {
                        if (request.method() === 'POST' && request.url().includes('/api/tasks/status')) writes.push(request.postData() ?? '');
                    });
                    const file = path.join(workspace.tasksDir, 'BRD/101.yml');
                    const before = await fs.readFile(file, 'utf8');
                    await card.dispatchEvent('dragstart');
                    await page.locator('.column[data-column-kind="other"]').dispatchEvent('drop');
                    await page.locator('.column[data-column-kind="status"][data-status="Todo"]').press('Enter');
                    expect(writes).toEqual([]);
                    expect(await fs.readFile(file, 'utf8')).toBe(before);
                    await card.dispatchEvent('dragstart');
                    await page.locator('.column[data-column-kind="status"][data-status="__other__"]').dispatchEvent('drop');
                    await expect.poll(() => writes.length).toBe(1);
                    expect(JSON.parse(writes[0]).status).toBe('__other__');
                    await expect.poll(async () => {
                        const response = await fetch(`${server.url}/api/tasks/get?id=BRD-101`);
                        return (await response.json() as { data: { status: string } }).data.status;
                    }).toBe('__other__');
                });
            } finally { await server.stop(); }
        } finally { await workspace.dispose(); }
    });
});
