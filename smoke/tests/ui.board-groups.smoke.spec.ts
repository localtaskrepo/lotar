import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function projectConfig(priorities: string[], types: string[]) {
    return stringify({ default: { priority: priorities[0] }, issue: { states: ['Todo', 'Done'], priorities, types, done_states: ['Done'] } });
}

function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': '{}\n',
        '.tasks/BRD/config.yml': projectConfig(['Low', 'Medium', 'High'], ['Feature', 'Bug', 'Chore']),
        '.tasks/ALT/config.yml': projectConfig(['Later', 'Soon', 'Urgent'], ['Chore', 'Bug', 'Feature']),
    };
    const priorities = ['High', 'Low', 'Medium', 'Zulu', 'Alpha', 'High', 'Critical'];
    const types = ['Bug', 'Feature', 'Chore', 'Zeta', 'Alpha', 'Bug', 'Feature'];
    const assignees = ['zoe', 'ben', 'amy', null, 'dan', 'cara', 'zoe'];
    for (let index = 0; index < priorities.length; index += 1) {
        files[`.tasks/BRD/${index + 1}.yml`] = stringify({
            title: `Card ${index + 1}`, status: index === 1 ? 'Legacy' : 'Todo',
            priority: priorities[index], type: types[index], assignee: assignees[index], reporter: 'Fixture',
            created: '2026-10-01T00:00:00Z', modified: `2026-10-01T00:00:0${index}Z`,
        });
    }
    files['.tasks/BRD/99.yml'] = stringify({
        title: 'Deleted card', status: 'Todo', priority: 'Deleted group', type: 'Deleted type',
        created: '2026-10-01T00:00:00Z', modified: '2026-10-01T00:00:00Z', deleted_at: '2026-10-02T00:00:00Z',
    });
    for (const [index, priority] of ['Later', 'Soon', 'Urgent'].entries()) {
        files[`.tasks/ALT/${index + 1}.yml`] = stringify({
            title: `Alternate ${index + 1}`, status: 'Todo', priority, type: ['Feature', 'Bug', 'Chore'][index],
            reporter: 'Fixture', created: '2026-10-01T00:00:00Z', modified: '2026-10-01T00:00:00Z',
        });
    }
    return files;
}

describe.concurrent('Project-configured Board swimlane order (DEV-87)', () => {
    for (const viewport of [
        { name: 'desktop', width: 1280, height: 900 },
        { name: 'mobile', width: 390, height: 844 },
    ]) {
        it(`orders present groups without changing membership on ${viewport.name}`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
            const artifacts = path.resolve('target/smoke-artifacts/board-groups');
            await fs.ensureDir(artifacts);
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/boards?project=BRD`, async page => {
                        await page.setViewportSize(viewport);
                        const group = page.getByTestId('board-groupby');
                        const labels = () => page.locator('.swimlane-label').allTextContents();
                        const writes: string[] = [];
                        page.on('request', request => {
                            if (['POST', 'PUT', 'DELETE'].includes(request.method())) writes.push(new URL(request.url()).pathname);
                        });
                        await expect.poll(() => page.locator('article.task').count()).toBe(7);
                        for (const [mode, expected, counts] of [
                            ['priority', ['High', 'Medium', 'Low', 'Alpha', 'Critical', 'Zulu'], ['2', '1', '1', '1', '1', '1']],
                            ['type', ['Feature', 'Bug', 'Chore', 'Alpha', 'Zeta'], ['2', '2', '1', '1', '1']],
                            ['assignee', ['amy', 'ben', 'cara', 'dan', 'zoe', '(none)'], ['1', '1', '1', '1', '2', '1']],
                        ] as const) {
                            await group.selectOption(mode);
                            await expect.poll(labels).toEqual([...expected]);
                            expect(await page.locator('.swimlane-count').allTextContents()).toEqual([...counts]);
                            expect(await page.locator('article.task').count()).toBe(7);
                            expect(await page.locator('[data-column-kind="other"] article.task').count()).toBe(1);
                        }
                        await group.selectOption('priority');
                        await page.locator('.swimlane-header').filter({ hasText: 'Low' }).click();
                        expect(await page.locator('article.task').count()).toBe(6);
                        await workspace.write('.tasks/BRD/config.yml', projectConfig(['Low', 'Medium', 'High', 'Critical'], ['Feature', 'Bug', 'Chore']));
                        await page.getByRole('button', { name: 'Refresh board', exact: true }).click();
                        await expect.poll(labels).toEqual(['Critical', 'High', 'Medium', 'Low', 'Alpha', 'Zulu']);
                        expect(await page.locator('.swimlane-header.collapsed .swimlane-label').textContent()).toBe('Low');
                        await page.locator('.swimlane-header.collapsed').click();
                        expect(await page.locator('article.task').count()).toBe(7);
                        await page.locator('.board.grid').evaluate(el => { el.scrollLeft = 0; });
                        await page.evaluate(() => window.scrollTo(0, 0));
                        await page.screenshot({ path: path.join(artifacts, `${viewport.name}.png`), animations: 'disabled' });

                        await page.getByTestId('filter-toggle').click();
                        await page.getByRole('button', { name: 'No assignee', exact: true }).click();
                        await expect.poll(labels).toEqual(['Zulu']);
                        expect(await page.locator('.swimlane-count').allTextContents()).toEqual(['1']);
                        expect(await page.locator('article.task .title').allTextContents()).toEqual(['Card 4']);

                        await page.goto(`${server.url}/boards?project=ALT`);
                        await group.selectOption('priority');
                        await expect.poll(labels).toEqual(['Urgent', 'Soon', 'Later']);
                        await group.selectOption('type');
                        await expect.poll(labels).toEqual(['Chore', 'Bug', 'Feature']);
                        expect(await page.locator('article.task').count()).toBe(3);
                        expect(await page.locator('.swimlane-count').allTextContents()).toEqual(['1', '1', '1']);
                        expect(writes).toEqual([]);
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
