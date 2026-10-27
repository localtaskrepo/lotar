import { relative } from 'node:path';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

async function fixture() {
    const workspace = await SmokeWorkspace.create({
        name: 'ui-relationships-',
        seedFiles: {
            '.tasks/config.yml': 'default.project: REL\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
            '.tasks/REL/config.yml': 'project.name: Relationship fixture\n',
        },
    });
    try {
        const source = await workspace.addTask('Dependent fixture');
        const target = await workspace.addTask('Prerequisite fixture');
        const third = await workspace.addTask('Other fixture');
        return { workspace, source, target, third };
    } catch (error) {
        await workspace.dispose();
        throw error;
    }
}

async function relationships(workspace: SmokeWorkspace, id: string) {
    return (
        (parse(await workspace.readTaskYaml(id)) as { relationships?: Record<string, string | string[]> })
            .relationships || {}
    );
}

async function writeRelationships(workspace: SmokeWorkspace, id: string, value: Record<string, string | string[]>) {
    const task = parse(await workspace.readTaskYaml(id));
    task.relationships = value;
    await workspace.write(relative(workspace.root, workspace.getTaskFilePath(id)), stringify(task));
}

describe.concurrent('UI relationship integrity and dependencies (DEV-15/DEV-8/DEV-9)', () => {
    for (const viewport of [
        { width: 1440, height: 1000 },
        { width: 390, height: 844 },
    ]) {
        it(`edits categories atomically and preserves graph navigation at ${viewport.width}px`, async ({ expect }) => {
            const { workspace, source, target, third } = await fixture();
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage('about:blank', async (page) => {
                        await page.setViewportSize(viewport);
                        const posts: string[] = [];
                        const graphRequests: string[] = [];
                        const listRequests: string[] = [];
                        page.on('request', (request) => {
                            if (request.method() === 'POST') posts.push(new URL(request.url()).pathname);
                            if (new URL(request.url()).pathname === '/api/relationships/graph')
                                graphRequests.push(request.url());
                            // Ownership suggestions use an unfiltered list snapshot, not the task-list view query.
                            const url = new URL(request.url());
                            if (
                                url.pathname === '/api/tasks/list' &&
                                (url.searchParams.has('q') ||
                                    url.searchParams.has('sort_by') ||
                                    url.searchParams.has('taskPanel'))
                            )
                                listRequests.push(request.url());
                        });
                        await page.goto(`${server.url}/task/${source.id}`);
                        await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                        const editor = page.locator('.relationship-editor');
                        expect((await editor.boundingBox())!.height).toBeLessThan(480);
                        const tabs = await page.getByRole('tablist').boundingBox();
                        expect((await editor.locator('.relationship-toolbar').boundingBox())!.y).toBeGreaterThanOrEqual(
                            tabs!.y + tabs!.height,
                        );
                        expect(await editor.getByRole('combobox').count()).toBe(0);
                        expect(await editor.locator('.dependency-view').count()).toBe(0);
                        expect(await editor.getByRole('button', { name: /reload|actions for/i }).count()).toBe(0);
                        expect(graphRequests).toEqual([]);
                        const depends = page.getByRole('region', { name: 'Depends on', exact: true });
                        await depends.getByRole('button', { name: 'Add depends on', exact: true }).click();
                        await depends.getByRole('combobox', { name: 'Add depends on', exact: true }).fill(target.id);
                        const added = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/relationships/mutate',
                        );
                        await depends.getByRole('button', { name: 'Add', exact: true }).click();
                        expect((await added).status()).toBe(200);
                        await expect
                            .poll(async () => (await relationships(workspace, target.id)).blocks)
                            .toEqual([source.id]);
                        await expect
                            .poll(() => depends.getByRole('link', { name: target.id, exact: true }).count())
                            .toBe(1);
                        expect(await page.getByRole('dialog', { name: `Task ${source.id}`, exact: true }).count()).toBe(
                            1,
                        );

                        const listCount = listRequests.length;
                        await depends.getByRole('link', { name: target.id, exact: true }).click();
                        await page.waitForURL(`**/task/${target.id}`);
                        await page.getByRole('dialog', { name: `Task ${target.id}`, exact: true }).waitFor();
                        await page.goBack();
                        await page.waitForURL(`**/task/${source.id}`);
                        await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                        await depends.getByRole('link', { name: target.id, exact: true }).waitFor();
                        expect(listRequests.length).toBe(listCount);

                        const fixes = page.getByRole('region', { name: 'Fixes', exact: true });
                        await fixes.getByRole('button', { name: 'Add fixes', exact: true }).click();

                        const unsaved = fixes.getByRole('combobox', { name: 'Add fixes', exact: true });
                        await unsaved.fill('keep this draft');
                        await unsaved.scrollIntoViewIfNeeded();
                        const body = page.locator('.task-panel__body');
                        const scrollBefore = await body.evaluate((element) => element.scrollTop);
                        expect(scrollBefore).toBeGreaterThan(0);
                        const live = await page.request.post(`${server.url}/api/relationships/mutate`, {
                            data: { source: source.id, target: third.id, kind: 'related', operation: 'add' },
                        });
                        expect(live.status()).toBe(200);
                        await page
                            .getByRole('region', { name: 'Related', exact: true })
                            .getByRole('link', { name: third.id, exact: true })
                            .waitFor();
                        expect(await unsaved.inputValue()).toBe('keep this draft');
                        expect(
                            Math.abs((await body.evaluate((element) => element.scrollTop)) - scrollBefore),
                        ).toBeLessThan(32);
                        await fixes.getByRole('button', { name: 'Add fixes', exact: true }).click();

                        const moved = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/relationships/mutate',
                        );
                        await depends
                            .locator('li[draggable="true"]')
                            .dragTo(
                                page
                                    .getByRole('region', { name: 'Blocks', exact: true })
                                    .getByRole('heading', { name: 'Blocks', exact: true }),
                                { sourcePosition: { x: 8, y: 8 }, targetPosition: { x: 8, y: 8 } },
                            );
                        const confirmation = page.getByRole('alertdialog', { name: 'Confirm relationship change' });
                        await confirmation.waitFor();
                        expect(await confirmation.textContent()).toContain('reverses dependency direction');
                        await confirmation.getByRole('button', { name: 'Confirm change', exact: true }).click();
                        const moveResponse = await moved;
                        expect(moveResponse.status()).toBe(200);
                        expect(moveResponse.request().postDataJSON()).toMatchObject({
                            source: source.id,
                            target: target.id,
                            from_kind: 'depends_on',
                            kind: 'blocks',
                            operation: 'change_kind',
                        });
                        await expect
                            .poll(async () => (await relationships(workspace, source.id)).blocks)
                            .toEqual([target.id]);
                        await expect
                            .poll(async () => (await relationships(workspace, target.id)).depends_on)
                            .toEqual([source.id]);
                        expect((await relationships(workspace, source.id)).depends_on || []).toEqual([]);
                        expect((await relationships(workspace, target.id)).blocks || []).toEqual([]);

                        const blocks = page.getByRole('region', { name: 'Blocks', exact: true });
                        const related = page.getByRole('region', { name: 'Related', exact: true });
                        const converted = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/relationships/mutate',
                        );
                        await blocks
                            .locator('li[draggable="true"]')
                            .dragTo(related.getByRole('heading', { name: 'Related', exact: true }), {
                                sourcePosition: { x: 8, y: 8 },
                                targetPosition: { x: 8, y: 8 },
                            });
                        expect((await converted).status()).toBe(200);
                        await expect
                            .poll(async () => (await relationships(workspace, target.id)).related)
                            .toEqual([source.id]);
                        expect((await relationships(workspace, target.id)).depends_on || []).toEqual([]);
                        await related
                            .getByRole('button', { name: `Remove ${third.id} from Related`, exact: true })
                            .click();
                        await expect
                            .poll(async () => (await relationships(workspace, third.id)).related || [])
                            .toEqual([]);
                        expect(posts.filter((path) => path.includes('/attachments/'))).toEqual([]);
                        expect(posts.filter((path) => path === '/api/tasks/update')).toEqual([]);

                        await page.getByRole('link', { name: 'Open dependency view', exact: true }).click();
                        await page.waitForURL('**/dependencies?**');
                        await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                        await page
                            .getByRole('group', { name: 'Relationship types', exact: true })
                            .getByRole('checkbox', { name: 'Related tasks', exact: true })
                            .check();
                        await page.waitForURL((url) =>
                            (url.searchParams.get('relationships') || '').split(',').includes('related'),
                        );
                        await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                        if (
                            !(await page
                                .locator('.dependency-lists')
                                .evaluate((element) => (element as HTMLDetailsElement).open))
                        )
                            await page.locator('.dependency-lists > summary').click();
                        const edgeList = page.getByRole('list', { name: 'Dependency relationships', exact: true });
                        await expect.poll(() => edgeList.textContent()).toContain(`${source.id} related ${target.id}`);
                        const graphUrl = page.url();
                        const searchElement = await page.getByTestId('filter-search').elementHandle();
                        if (viewport.width > 700) {
                            await page.locator(`.dependency-diagram a[href="/task/${target.id}"]`).click();
                        } else {
                            const tasks = page.getByRole('list', { name: 'Dependency tasks', exact: true });
                            await tasks.locator(`a[href="/task/${target.id}"]`).click();
                        }
                        await page.waitForURL(
                            (url) =>
                                url.searchParams.get('root') === target.id &&
                                url.searchParams.get('taskPanel') === target.id,
                        );
                        await page.getByRole('dialog', { name: `Task ${target.id}`, exact: true }).waitFor();
                        expect(new URL(page.url()).searchParams.get('q')).toBe(target.id);
                        expect(await searchElement!.evaluate((element) => element.isConnected)).toBe(true);
                        await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                        await page
                            .getByRole('region', { name: 'Related', exact: true })
                            .getByRole('link', { name: source.id, exact: true })
                            .click();
                        await page.waitForURL(
                            (url) =>
                                url.searchParams.get('root') === source.id &&
                                url.searchParams.get('taskPanel') === source.id,
                        );
                        await page.getByRole('dialog', { name: `Task ${source.id}`, exact: true }).waitFor();
                        await page.goBack();
                        await page.waitForURL(
                            (url) =>
                                url.searchParams.get('root') === target.id &&
                                url.searchParams.get('taskPanel') === target.id,
                        );
                        await page.getByRole('dialog', { name: `Task ${target.id}`, exact: true }).waitFor();
                        expect(await searchElement!.evaluate((element) => element.isConnected)).toBe(true);
                        await page.getByRole('button', { name: 'Close panel', exact: true }).click();
                        await page.waitForURL((url) => !url.searchParams.has('taskPanel'));
                        expect(new URL(page.url()).searchParams.get('root')).toBe(target.id);
                        await page.goBack();
                        await page.waitForURL(
                            (url) => url.searchParams.get('root') === source.id && !url.searchParams.has('taskPanel'),
                        );
                        expect(page.url()).toBe(graphUrl);
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }

    // DEV-147: a suggestion whose title wraps used to inherit the centered
    // .btn layout, so some searches showed centered rows.
    it('left-aligns wrapped relationship suggestions as full-width rows', async ({ expect }) => {
        const { workspace, source } = await fixture();
        try {
            const long = await workspace.addTask(
                'Wrapped suggestion fixture with a title long enough to need several lines inside a phone width panel',
            );
            const server = await startLotarServer(workspace);
            try {
                await withPage('about:blank', async (page) => {
                    await page.setViewportSize({ width: 390, height: 844 });
                    await page.goto(`${server.url}/task/${source.id}`);
                    await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                    const depends = page.getByRole('region', { name: 'Depends on', exact: true });
                    await depends.getByRole('button', { name: 'Add depends on', exact: true }).click();
                    await depends.getByRole('combobox', { name: 'Add depends on', exact: true }).fill('fixture');
                    const option = depends.getByRole('option').filter({ hasText: long.id });
                    await option.waitFor();
                    const layout = await option.evaluate((item) => {
                        const button = item.querySelector('button')!;
                        const style = getComputedStyle(button);
                        const range = document.createRange();
                        range.selectNodeContents(button);
                        const lines = Array.from(range.getClientRects());
                        return {
                            itemWidth: item.getBoundingClientRect().width,
                            buttonWidth: button.getBoundingClientRect().width,
                            textAlign: style.textAlign,
                            lineLefts: [...new Set(lines.map((rect) => Math.round(rect.left)))],
                            lineCount: new Set(lines.map((rect) => Math.round(rect.top))).size,
                        };
                    });
                    expect(layout.lineCount).toBeGreaterThan(1);
                    expect(layout.textAlign).toBe('left');
                    expect(Math.abs(layout.buttonWidth - layout.itemWidth)).toBeLessThan(1);
                    expect(layout.lineLefts).toHaveLength(1);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('previews safe reciprocal repair without writes and applies only after confirmation', async ({ expect }) => {
        const { workspace, source, target } = await fixture();
        try {
            await writeRelationships(workspace, source.id, { related: [target.id] });
            const before = await Promise.all([workspace.readTaskYaml(source.id), workspace.readTaskYaml(target.id)]);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?root=${source.id}&project=REL&type=all`, async (page) => {
                    await page.getByRole('button', { name: 'Fix relationships', exact: true }).click();
                    const preview = page.getByRole('region', { name: 'Safe repair preview', exact: true });
                    await preview.waitFor();
                    expect(await preview.textContent()).toContain(target.id);
                    expect(
                        await Promise.all([workspace.readTaskYaml(source.id), workspace.readTaskYaml(target.id)]),
                    ).toEqual(before);
                    expect((await relationships(workspace, target.id)).related || []).toEqual([]);
                    const applied = page.waitForResponse(
                        (response) =>
                            new URL(response.url()).pathname === '/api/relationships/repair' &&
                            response.request().postDataJSON().apply === true,
                    );
                    await preview.getByRole('button', { name: 'Confirm and apply safe repair', exact: true }).click();
                    const response = await applied;
                    expect(response.status()).toBe(200);
                    expect(response.request().postDataJSON().revision).toEqual(expect.any(String));
                    await expect
                        .poll(async () => (await relationships(workspace, target.id)).related)
                        .toEqual([source.id]);
                    await expect
                        .poll(() => page.getByRole('status').allTextContents())
                        .toContain('Safe repair applied to 1 task.');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps cycles, missing targets and deleted prerequisites visible without repairing on view', async ({
        expect,
    }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            await workspace.runLotar(['task', 'delete', third.id, '--yes']);
            await writeRelationships(workspace, source.id, {
                depends_on: [target.id, third.id],
                related: ['ABSENT-99'],
            });
            await writeRelationships(workspace, target.id, { depends_on: [source.id] });
            const before = await Promise.all([source.id, target.id, third.id].map((id) => workspace.readTaskYaml(id)));
            const server = await startLotarServer(workspace);
            try {
                await withPage(
                    `${server.url}/dependencies?root=${source.id}&project=REL&type=all&depth=2`,
                    async (page) => {
                        if (
                            !(await page
                                .locator('.dependency-lists')
                                .evaluate((element) => (element as HTMLDetailsElement).open))
                        )
                            await page.locator('.dependency-lists > summary').click();
                        const nodes = page.getByRole('list', { name: 'Dependency tasks', exact: true });
                        await expect.poll(() => nodes.textContent()).toContain('ABSENT-99');
                        expect(await nodes.textContent()).toContain('Missing target');
                        expect(await nodes.textContent()).toContain('Deleted');
                        const issues = page.getByRole('region', { name: 'Dependency issues', exact: true });
                        await issues.locator('summary').click();
                        await expect.poll(() => issues.textContent()).toContain('Circular dependency');
                        const problems = issues.getByRole('list', { name: 'Problems in displayed graph', exact: true });
                        expect(await problems.evaluate((element) => getComputedStyle(element).listStyleType)).toBe(
                            'none',
                        );
                        expect(await problems.locator('.dependency-problem--error').count()).toBeGreaterThan(0);
                        expect(await problems.locator('.dependency-problem-severity').allTextContents()).toContain(
                            'Error',
                        );
                        expect(
                            await Promise.all([source.id, target.id, third.id].map((id) => workspace.readTaskYaml(id))),
                        ).toEqual(before);
                    },
                );
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('exposes Dependencies in navigation and persists its Preferences visibility without blocking the route', async ({
        expect,
    }) => {
        const { workspace } = await fixture();
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    const nav = page.getByRole('navigation');
                    const taskButton = await page.getByTestId('global-new-task').boundingBox();
                    expect((await nav.boundingBox())!.x).toBeGreaterThanOrEqual(taskButton!.x + taskButton!.width);
                    await nav.getByRole('link', { name: 'Dependencies', exact: true }).click();
                    await page.getByRole('heading', { name: /^Dependencies/, level: 1 }).waitFor();
                    expect(await page.getByRole('button', { name: 'Fix relationships', exact: true }).isVisible()).toBe(
                        true,
                    );
                    await nav.getByRole('link', { name: 'Preferences', exact: true }).click();
                    await page.getByRole('checkbox', { name: 'Dependencies', exact: true }).uncheck();
                    expect(await nav.getByRole('link', { name: 'Dependencies', exact: true }).count()).toBe(0);
                    await page.reload();
                    expect(await page.getByRole('checkbox', { name: 'Dependencies', exact: true }).isChecked()).toBe(
                        false,
                    );
                    await page.goto(`${server.url}/dependencies`);
                    await page.getByRole('heading', { name: /^Dependencies/, level: 1 }).waitFor();
                    expect(await nav.getByRole('link', { name: 'Dependencies', exact: true }).count()).toBe(0);
                    expect(await page.getByRole('button', { name: 'Fix relationships', exact: true }).isVisible()).toBe(
                        true,
                    );
                    await nav.getByRole('link', { name: 'Preferences', exact: true }).click();
                    await page.getByRole('checkbox', { name: 'Dependencies', exact: true }).check();
                    await page.reload();
                    expect(await nav.getByRole('link', { name: 'Dependencies', exact: true }).count()).toBe(1);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('uses dependency stages, task autocomplete and shared filters, details and control sizes', async ({
        expect,
    }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            const fourth = await workspace.addTask('Fourth stage fixture');
            const fifth = await workspace.addTask('Fifth stage fixture');
            const side = await workspace.addTask('Alpha side fixture', {
                args: ['--type', 'Bug', '--priority', 'High'],
            });
            const chain = [source, target, third, fourth, fifth];
            for (let index = 1; index < chain.length; index++) {
                await workspace.runLotar([
                    'task',
                    'relationships',
                    'add',
                    chain[index]!.id,
                    chain[index - 1]!.id,
                    '--kind',
                    'depends-on',
                ]);
            }
            await workspace.runLotar(['task', 'relationships', 'add', side.id, source.id, '--kind', 'depends-on']);
            await workspace.runLotar(['task', 'status', side.id, 'Done']);
            await workspace.runLotar(['--project', 'ALT', 'add', 'Alternate fixture']);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?project=REL&layers=dependency&depth=4`, async (page) => {
                    const nodes = page.locator('.dependency-node');
                    await expect.poll(() => nodes.count()).toBe(6);
                    const ranks = await nodes.evaluateAll((elements) =>
                        elements.map((element) => Number((element as HTMLElement).dataset.rank)),
                    );
                    expect(new Set(ranks).size).toBe(5);
                    await page.getByText('Stage 5', { exact: true }).waitFor();
                    expect(await page.getByRole('button', { name: 'Explore', exact: true }).count()).toBe(0);
                    expect(await page.getByRole('combobox', { name: 'Focus task', exact: true }).count()).toBe(0);
                    for (const name of ['Validate project', 'Fix relationships']) {
                        const style = await page.getByRole('button', { name, exact: true }).evaluate((element) => ({
                            border: getComputedStyle(element).borderTopWidth,
                            color: getComputedStyle(element).borderTopColor,
                        }));
                        expect(style.border).toBe('1px');
                        expect(style.color).not.toBe('rgba(0, 0, 0, 0)');
                    }
                    const firstCard = page.locator(`.dependency-node[data-task-id="${source.id}"]`);
                    await expect.poll(() => firstCard.locator('.priority').count()).toBe(1);
                    expect(await page.getByRole('tooltip').count()).toBe(0);
                    await page.getByRole('button', { name: 'Configure card fields', exact: true }).click();
                    await page.getByRole('checkbox', { name: 'Priority', exact: true }).uncheck();
                    await page.getByRole('button', { name: 'Configure card fields', exact: true }).click();
                    await expect.poll(() => firstCard.locator('.priority').count()).toBe(0);
                    await page.getByRole('combobox', { name: 'Sort tasks', exact: true }).selectOption('title');
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    await page.getByRole('combobox', { name: 'Sort direction', exact: true }).selectOption('asc');
                    await page.waitForURL(
                        (url) => url.searchParams.get('sort_by') === 'title' && url.searchParams.get('order') === 'asc',
                    );
                    await page.getByRole('button', { name: 'Status', exact: true }).click();
                    await page
                        .getByRole('group', { name: 'Status choices', exact: true })
                        .getByRole('checkbox', { name: 'Todo', exact: true })
                        .check();
                    await page.getByRole('button', { name: 'Status', exact: true }).click();
                    await page.waitForURL((url) => url.searchParams.get('status') === 'Todo');
                    await expect
                        .poll(() => page.locator('.dependency-node--context').allTextContents())
                        .toEqual(expect.arrayContaining([expect.stringContaining(side.id)]));
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    const focus = page.getByTestId('filter-search');
                    const searchElement = await focus.elementHandle();
                    await focus.fill('Fifth stage');
                    const option = page
                        .getByRole('option')
                        .filter({ hasText: fifth.id })
                        .filter({ hasText: 'Fifth stage fixture' });
                    await option.waitFor();
                    await expect.poll(() => new URL(page.url()).searchParams.get('q')).toBe('Fifth stage');
                    expect(
                        await searchElement!.evaluate(
                            (element) => element.isConnected && element === document.activeElement,
                        ),
                    ).toBe(true);
                    expect(await option.isVisible()).toBe(true);
                    await focus.press('Enter');
                    await page.waitForURL((url) => url.searchParams.get('root') === fifth.id);
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    await page.getByRole('combobox', { name: 'Project', exact: true }).selectOption('ALT');
                    await page.waitForURL(
                        (url) => url.searchParams.get('project') === 'ALT' && !url.searchParams.has('root'),
                    );
                    await expect
                        .poll(() =>
                            page
                                .getByRole('region', { name: 'Dependency view', exact: true })
                                .getAttribute('aria-busy'),
                        )
                        .toBe('false');
                    expect(await page.locator('.dependency-node').count()).toBe(0);
                    expect(await page.locator('.dependency-diagram').count()).toBe(0);
                    expect(await page.locator('a[href="/task/ALT-1"]').count()).toBe(0);
                    await page.getByRole('button', { name: 'Show unlinked tasks', exact: true }).click();
                    expect(
                        await page
                            .getByRole('button', { name: 'Show unlinked tasks', exact: true })
                            .getAttribute('aria-pressed'),
                    ).toBe('true');
                    await page.waitForURL((url) => url.searchParams.get('isolated') === '1');
                    await expect.poll(() => page.locator('[data-task-id="ALT-1"]').count()).toBeGreaterThan(0);
                    if (
                        !(await page
                            .locator('.dependency-lists')
                            .evaluate((element) => (element as HTMLDetailsElement).open))
                    )
                        await page.locator('.dependency-lists > summary').click();
                    await page
                        .getByRole('list', { name: 'Dependency tasks', exact: true })
                        .locator('a[href="/task/ALT-1"]')
                        .waitFor();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('returns through relationship history over the task list without reloading its filtered rows', async ({
        expect,
    }) => {
        const { workspace, source, target } = await fixture();
        try {
            await workspace.runLotar(['task', 'relationships', 'add', source.id, target.id, '--kind', 'related']);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?q=Dependent&sort_by=title&order=asc`, async (page) => {
                    const listReads: string[] = [];
                    page.on('request', (request) => {
                        const url = new URL(request.url());
                        if (
                            url.pathname === '/api/tasks/list' &&
                            (url.searchParams.has('q') ||
                                url.searchParams.has('sort_by') ||
                                url.searchParams.has('taskPanel'))
                        )
                            listReads.push(request.url());
                    });
                    const rowTitle = page.locator('tbody').getByText('Dependent fixture', { exact: true });
                    const row = await rowTitle.elementHandle();
                    await rowTitle.click();
                    await page.getByRole('dialog', { name: `Task ${source.id}`, exact: true }).waitFor();
                    await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                    const original = page.url(),
                        reads = listReads.length;
                    await page
                        .getByRole('region', { name: 'Related', exact: true })
                        .getByRole('link', { name: target.id, exact: true })
                        .click();
                    await page.waitForURL((url) => url.searchParams.get('taskPanel') === target.id);
                    await page.getByRole('dialog', { name: `Task ${target.id}`, exact: true }).waitFor();
                    await page.goBack();
                    await page.waitForURL((url) => url.searchParams.get('taskPanel') === source.id);
                    await page.getByRole('dialog', { name: `Task ${source.id}`, exact: true }).waitFor();
                    expect(listReads.length).toBe(reads);
                    expect(await row!.evaluate((element) => element.isConnected)).toBe(true);
                    expect(new URL(page.url()).searchParams.get('q')).toBe('Dependent');
                    expect(new URL(page.url()).searchParams.get('sort_by')).toBe('title');
                    await page.getByRole('button', { name: 'Close panel', exact: true }).click();
                    await page.waitForURL((url) => !url.searchParams.has('taskPanel'));
                    expect(page.url()).toBe(original);
                    expect(listReads.length).toBe(reads);
                    expect(listReads.some((url) => new URL(url).searchParams.has('taskPanel'))).toBe(false);
                    await page.locator('tbody').getByText('Dependent fixture', { exact: true }).waitFor();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps one focused task and hides disabled-kind-only cards for the DEV-54 relationship shape', async ({
        expect,
    }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            const dependents = [target];
            for (let index = 2; index <= 5; index++) dependents.push(await workspace.addTask(`Dependent ${index}`));
            const related = await workspace.addTask('Second related fixture');
            const parent = await workspace.addTask('Parent fixture');
            for (const dependent of dependents)
                await workspace.runLotar(['task', 'relationships', 'add', source.id, dependent.id, '--kind', 'blocks']);
            for (const task of [third, related])
                await workspace.runLotar(['task', 'relationships', 'add', source.id, task.id, '--kind', 'related']);
            await workspace.runLotar(['task', 'relationships', 'add', source.id, parent.id, '--kind', 'parent']);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?project=REL`, async (page) => {
                    const input = page.getByTestId('filter-search');
                    await input.fill(source.id);
                    await page
                        .getByRole('option')
                        .filter({ hasText: source.id })
                        .filter({ hasText: 'Dependent fixture' })
                        .waitFor();
                    expect(new URL(page.url()).searchParams.has('root')).toBe(false);
                    await input.press('Enter');
                    await page.waitForURL((url) => url.searchParams.get('root') === source.id);
                    const cards = page.locator('.dependency-node');
                    await expect.poll(() => cards.count()).toBe(6);
                    expect(
                        (
                            await cards.evaluateAll((elements) =>
                                elements.map((element) => (element as HTMLElement).dataset.taskId),
                            )
                        ).sort(),
                    ).toEqual([source.id, ...dependents.map((task) => task.id)].sort());
                    expect(
                        await page.locator('.dependency-node[data-focused="true"]').getAttribute('data-task-id'),
                    ).toBe(source.id);
                    expect(await page.locator('.dependency-node--selected').count()).toBe(1);
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    const choices = page.getByRole('group', { name: 'Relationship types', exact: true });
                    await choices.getByRole('checkbox', { name: 'All relationship types', exact: true }).check();
                    await expect.poll(() => cards.count()).toBe(9);
                    expect(await page.locator('.dependency-node[data-focused="true"]').count()).toBe(1);
                    await choices.getByRole('checkbox', { name: 'Blocking dependencies', exact: true }).uncheck();
                    await expect.poll(() => cards.count()).toBe(4);
                    await choices.getByRole('checkbox', { name: 'Blocking dependencies', exact: true }).check();
                    await page.getByRole('combobox', { name: 'Connection range', exact: true }).selectOption('0');
                    await page.waitForURL((url) => url.searchParams.get('depth') === '0');
                    await expect.poll(() => cards.count()).toBe(1);
                    if (
                        !(await page
                            .locator('.dependency-lists')
                            .evaluate((element) => (element as HTMLDetailsElement).open))
                    )
                        await page.locator('.dependency-lists > summary').click();
                    const rows = page
                        .getByRole('list', { name: 'Dependency tasks', exact: true })
                        .locator(':scope > li');
                    await expect.poll(() => rows.count()).toBe(1);
                    expect(await rows.getAttribute('data-task-id')).toBe(source.id);
                    expect(await rows.getAttribute('data-focused')).toBe('true');
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    await page.getByRole('button', { name: 'Clear search', exact: true }).click();
                    await page.waitForURL((url) => !url.searchParams.has('root'));
                    await expect.poll(() => page.locator('.dependency-node').count()).toBe(9);
                    expect(await page.locator('.dependency-node[data-focused="true"]').count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps filter fields orderly and focus input bounded at wide, tablet and mobile sizes', async ({ expect }) => {
        const { workspace } = await fixture();
        try {
            const server = await startLotarServer(workspace);
            try {
                for (const width of [1920, 1280, 768, 390]) {
                    await withPage('about:blank', async (page) => {
                        await page.setViewportSize({ width, height: 1000 });
                        await page.goto(`${server.url}/dependencies?project=REL`);
                        expect(await page.getByRole('combobox', { name: 'Focus task', exact: true }).count()).toBe(0);
                        expect(await page.locator('input[type="text"]').count()).toBe(1);
                        expect(
                            await page.getByRole('button', { name: 'Show unlinked tasks', exact: true }).count(),
                        ).toBe(0);
                        await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                        const panel = page.getByTestId('filter-panel');
                        await panel.waitFor();
                        const include = await panel
                            .getByRole('button', { name: 'Show unlinked tasks', exact: true })
                            .boundingBox();
                        const range = await panel
                            .getByRole('combobox', { name: 'Connection range', exact: true })
                            .boundingBox();
                        expect(Math.abs(include!.height - range!.height)).toBeLessThan(1);
                        if (width > 640) expect(Math.abs(include!.y - range!.y)).toBeLessThan(1);
                        else {
                            expect(Math.abs(include!.x - range!.x)).toBeLessThan(1);
                            expect(Math.abs(include!.width - range!.width)).toBeLessThan(1);
                            expect(include!.y).toBeGreaterThanOrEqual(range!.y + range!.height + 8);
                        }
                        const bounds = await panel.boundingBox();
                        const fields = await panel.locator('.filter-bar__field').evaluateAll((elements) =>
                            elements.map((element) => {
                                const rect = element.getBoundingClientRect();
                                return {
                                    x: rect.x,
                                    y: rect.y,
                                    right: rect.right,
                                    bottom: rect.bottom,
                                    width: rect.width,
                                };
                            }),
                        );
                        expect(fields.length).toBeGreaterThan(5);
                        for (const field of fields) {
                            expect(field.x).toBeGreaterThanOrEqual(bounds!.x);
                            expect(field.right).toBeLessThanOrEqual(bounds!.x + bounds!.width + 1);
                            if (width >= 1280) expect(field.width).toBeGreaterThanOrEqual(180);
                        }
                        for (let index = 0; index < fields.length; index++)
                            for (const other of fields.slice(index + 1)) {
                                const field = fields[index]!;
                                expect(
                                    Math.min(field.right, other.right) - Math.max(field.x, other.x) > 1 &&
                                        Math.min(field.bottom, other.bottom) - Math.max(field.y, other.y) > 1,
                                ).toBe(false);
                            }
                        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
                            width + 1,
                        );
                    });
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps shared search autocomplete alive and identifies every relationship kind on visible cards and connections', async ({
        expect,
    }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            for (const kind of ['blocks', 'children', 'related', 'fixes', 'duplicate-of']) {
                await workspace.runLotar(['task', 'relationships', 'add', source.id, target.id, '--kind', kind]);
            }
            await workspace.runLotar(['task', 'relationships', 'add', source.id, third.id, '--kind', 'blocks']);
            for (let index = 0; index < 3; index++) {
                const extra = await workspace.addTask(`Additional connected fixture ${index}`);
                await workspace.runLotar(['task', 'relationships', 'add', source.id, extra.id, '--kind', 'blocks']);
            }
            for (const task of [target, third]) {
                const data = parse(await workspace.readTaskYaml(task.id));
                data.type = 'Bug';
                data.priority = 'High';
                await workspace.write(relative(workspace.root, workspace.getTaskFilePath(task.id)), stringify(data));
            }
            const details = parse(await workspace.readTaskYaml(third.id));
            details.assignee = 'Long Owner '.repeat(30).trim();
            await workspace.write(relative(workspace.root, workspace.getTaskFilePath(third.id)), stringify(details));
            const server = await startLotarServer(workspace);
            try {
                await withPage(
                    `${server.url}/dependencies?root=${source.id}&project=REL&relationships=dependency,hierarchy,related,fixes,duplicate&sort_by=id&order=asc`,
                    async (page) => {
                        await page.setViewportSize({ width: 1440, height: 1100 });
                        const navigations: string[] = [];
                        page.on('request', (request) => {
                            if (request.isNavigationRequest() && request.frame() === page.mainFrame())
                                navigations.push(request.url());
                        });
                        const search = page.getByTestId('filter-search');
                        const element = await search.elementHandle();
                        const legend = page.getByRole('list', { name: 'Relationship legend', exact: true });
                        for (const label of ['blocks', 'parent of', 'related', 'fixes', 'duplicate of'])
                            expect(await legend.textContent()).toContain(label);
                        await expect.poll(() => page.locator('.dependency-node').count()).toBe(6);
                        await expect
                            .poll(async () => (await page.locator('.dependency-diagram').boundingBox())?.height || 0)
                            .toBeGreaterThan(500);
                        const pairs = await page.locator('.dependency-edge').evaluateAll(
                            (edges, id) =>
                                edges
                                    .filter((edge) => edge.querySelector('title')?.textContent?.includes(id))
                                    .map((edge) => ({
                                        path: edge.querySelector('path')?.getAttribute('d'),
                                        title: edge.querySelector('title')?.textContent,
                                        dash: getComputedStyle(edge.querySelector('path')!).strokeDasharray,
                                    })),
                            target.id,
                        );
                        expect(pairs).toHaveLength(5);
                        expect(new Set(pairs.map((edge) => edge.path)).size).toBe(5);
                        expect(new Set(pairs.map((edge) => edge.dash)).size).toBe(5);
                        for (const label of ['blocks', 'duplicate of', 'fixes', 'parent of', 'related'])
                            expect(pairs.some((edge) => edge.title?.includes(label))).toBe(true);
                        expect(await page.locator('.dependency-edge > text').count()).toBe(0);
                        expect(
                            await page
                                .getByText('Arrowheads show direction; related links have no arrow.', { exact: true })
                                .count(),
                        ).toBe(0);
                        const diagramBounds = await page.locator('.dependency-canvas').evaluate((canvas) => {
                            const svg = canvas.querySelector('svg')!;
                            const paths = Array.from(
                                svg.querySelectorAll<SVGPathElement>('.dependency-edge > path'),
                            ).map((path) => path.getBBox());
                            const nodes = Array.from(canvas.querySelectorAll<HTMLElement>('.dependency-node'));
                            return {
                                width: svg.viewBox.baseVal.width,
                                height: svg.viewBox.baseVal.height,
                                paths: paths.map((box) => ({
                                    left: box.x,
                                    top: box.y,
                                    right: box.x + box.width,
                                    bottom: box.y + box.height,
                                })),
                                contentRight: Math.max(
                                    ...paths.map((box) => box.x + box.width),
                                    ...nodes.map((node) => node.offsetLeft + node.offsetWidth),
                                ),
                            };
                        });
                        for (const box of diagramBounds.paths) {
                            expect(box.left).toBeGreaterThanOrEqual(0);
                            expect(box.top).toBeGreaterThanOrEqual(0);
                            expect(box.right).toBeLessThanOrEqual(diagramBounds.width);
                            expect(box.bottom).toBeLessThanOrEqual(diagramBounds.height);
                        }
                        expect(diagramBounds.width - diagramBounds.contentRight).toBeLessThanOrEqual(64);
                        const frame = await page.locator('.dependency-diagram').evaluate((element) => ({
                            width: element.getBoundingClientRect().width,
                            chrome: (element as HTMLElement).offsetWidth - (element as HTMLElement).clientWidth,
                        }));
                        expect(frame.width).toBeLessThanOrEqual(diagramBounds.width + frame.chrome + 1);
                        const cards = page.locator('.dependency-node');
                        await expect
                            .poll(async () => {
                                const rects = await cards.evaluateAll((elements) =>
                                    elements.map((node) => {
                                        const rect = node.getBoundingClientRect();
                                        return {
                                            left: rect.left,
                                            right: rect.right,
                                            top: rect.top,
                                            bottom: rect.bottom,
                                        };
                                    }),
                                );
                                return rects.every((rect, index) =>
                                    rects
                                        .slice(index + 1)
                                        .every(
                                            (other) =>
                                                !(
                                                    Math.min(rect.right, other.right) >
                                                        Math.max(rect.left, other.left) &&
                                                    Math.min(rect.bottom, other.bottom) > Math.max(rect.top, other.top)
                                                ),
                                        ),
                                );
                            })
                            .toBe(true);
                        await page.locator('.dependency-lists > summary').click();
                        const widths = await page
                            .getByRole('list', { name: 'Dependency tasks', exact: true })
                            .locator(':scope > li')
                            .evaluateAll((elements) =>
                                elements.map((element) => element.getBoundingClientRect().width),
                            );
                        expect(widths.every((width) => width <= 321)).toBe(true);
                        const connections = page.getByRole('list', { name: 'Dependency relationships', exact: true });
                        expect(await connections.locator('.dependency-relationship-chip').count()).toBe(9);
                        const connectionBoxes = await connections.locator(':scope > li').evaluateAll((rows) =>
                            rows.map((row) => {
                                const box = row.getBoundingClientRect();
                                return { x: box.x, y: box.y, right: box.right, width: box.width };
                            }),
                        );
                        expect(Math.abs(connectionBoxes[0]!.y - connectionBoxes[1]!.y)).toBeLessThan(1);
                        expect(connectionBoxes[1]!.x).toBeGreaterThanOrEqual(connectionBoxes[0]!.right);
                        const margins = await connections.locator(':scope > li').evaluateAll((rows) =>
                            rows.map((row) => {
                                const box = row.getBoundingClientRect(),
                                    style = getComputedStyle(row);
                                const contentRight = Math.max(
                                    ...Array.from(row.children).map((child) => child.getBoundingClientRect().right),
                                );
                                return (
                                    box.right -
                                    contentRight -
                                    parseFloat(style.paddingRight) -
                                    parseFloat(style.borderRightWidth)
                                );
                            }),
                        );
                        expect(margins.every((margin) => margin <= 2)).toBe(true);
                        // The app follows an OS mode change via <html data-mode> (DEV-218),
                        // a moment after the media change; wait for it before reading colours.
                        const modeIs = (mode: string) =>
                            page.waitForFunction((value) => document.documentElement.dataset.mode === value, mode);
                        await page.emulateMedia({ colorScheme: 'dark' });
                        await modeIs('dark');
                        const darkText = await page.evaluate(() => getComputedStyle(document.body).color);
                        await expect
                            .poll(() =>
                                connections
                                    .locator('a')
                                    .first()
                                    .evaluate((element) => getComputedStyle(element).color),
                            )
                            .toBe(darkText);
                        await page.emulateMedia({ colorScheme: 'light' });
                        await modeIs('light');
                        const lightText = await page.evaluate(() => getComputedStyle(document.body).color);
                        await expect
                            .poll(() =>
                                connections
                                    .locator('a')
                                    .first()
                                    .evaluate((element) => getComputedStyle(element).color),
                            )
                            .toBe(lightText);
                        await page.locator('.dependency-lists > summary').click();
                        await search.fill('type:Bu priority:High');
                        await search.evaluate((input) => (input as HTMLInputElement).setSelectionRange(7, 7));
                        await search.press('Backspace');
                        await page.getByRole('option').filter({ hasText: 'Bug' }).waitFor();
                        expect(
                            await element!.evaluate((input) => input.isConnected && input === document.activeElement),
                        ).toBe(true);
                        await search.press('Enter');
                        await page.waitForURL(
                            (url) =>
                                url.searchParams.get('type') === 'Bug' && url.searchParams.get('priority') === 'High',
                        );
                        expect(navigations).toEqual([]);
                        expect(await element!.evaluate((input) => input.isConnected)).toBe(true);
                        expect(new URL(page.url()).searchParams.has('root')).toBe(false);
                    },
                );
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('ranks exact IDs first across lookup fields and restores searched content after leaving the view', async ({
        expect,
    }) => {
        const { workspace, source, target } = await fixture();
        try {
            await workspace.addTask(`Newer note mentioning ${target.id} and ticket 2`);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?project=REL`, async (page) => {
                    const search = page.getByTestId('filter-search');
                    await search.fill('2');
                    await expect.poll(() => page.getByRole('option').first().textContent()).toContain(target.id);
                    await expect
                        .poll(() => page.locator(`.dependency-selected[data-task-id="${target.id}"]`).count())
                        .toBe(1);
                    expect(new URL(page.url()).searchParams.has('root')).toBe(false);
                    const response = await page.request.get(`${server.url}/api/tasks/suggest?q=2&project=REL&limit=1`);
                    expect(response.status()).toBe(200);
                    expect((await response.json()).data[0].id).toBe(target.id);
                    await search.fill(target.id);
                    await expect.poll(() => page.getByRole('option').first().textContent()).toContain(target.id);
                    await page.waitForURL((url) => url.searchParams.get('q') === target.id);
                    await page.getByRole('navigation').getByRole('link', { name: 'Preferences', exact: true }).click();
                    await page.waitForURL('**/preferences');
                    await page.getByRole('navigation').getByRole('link', { name: 'Dependencies', exact: true }).click();
                    await page.waitForURL(
                        (url) => url.pathname === '/dependencies' && url.searchParams.get('q') === target.id,
                    );
                    expect(await page.getByTestId('filter-search').inputValue()).toBe(target.id);
                    await expect
                        .poll(() => page.locator(`.dependency-selected[data-task-id="${target.id}"]`).count())
                        .toBe(1);
                    expect(
                        await page
                            .getByRole('list', { name: 'Dependency tasks', exact: true })
                            .locator(`li[data-task-id="${source.id}"]`)
                            .count(),
                    ).toBe(0);
                    await page.goto(`${server.url}/task/${source.id}`);
                    await page.getByRole('tab', { name: 'Relationships', exact: true }).click();
                    const related = page.getByRole('region', { name: 'Related', exact: true });
                    await related.getByRole('button', { name: 'Add related', exact: true }).click();
                    await related.getByRole('combobox', { name: 'Add related', exact: true }).fill('2');
                    await expect.poll(() => related.getByRole('option').first().textContent()).toContain(target.id);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('restores saved shared filters into Board and Calendar result requests on unscoped routes', async ({
        expect,
    }) => {
        const { workspace } = await fixture();
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/preferences`, async (page) => {
                    for (const name of ['Boards', 'Calendar']) {
                        await page
                            .getByRole('navigation')
                            .getByRole('link', { name: 'Preferences', exact: true })
                            .click();
                        await page.evaluate(
                            (view) =>
                                localStorage.setItem(
                                    view === 'Boards' ? 'lotar.boards.filter' : 'lotar.calendar.filter',
                                    JSON.stringify({ q: 'Prerequisite', type: 'Feature', project: 'REL' }),
                                ),
                            name,
                        );
                        const filtered = page.waitForResponse((response) => {
                            const url = new URL(response.url());
                            return (
                                url.pathname === '/api/tasks/list' &&
                                url.searchParams.get('q') === 'Prerequisite' &&
                                url.searchParams.get('type') === 'Feature' &&
                                url.searchParams.get('project') === 'REL'
                            );
                        });
                        await page.getByRole('navigation').getByRole('link', { name, exact: true }).click();
                        expect((await filtered).status()).toBe(200);
                        expect(await page.getByTestId('filter-search').inputValue()).toBe('Prerequisite');
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps relationship snapshots available during concurrent reads and navigation back to the view', async ({
        expect,
    }) => {
        const { workspace, source, target } = await fixture();
        try {
            await workspace.runLotar(['task', 'relationships', 'add', source.id, target.id, '--kind', 'blocks']);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?project=REL&root=${source.id}`, async (page) => {
                    for (let round = 0; round < 3; round++) {
                        const reads = Promise.all(
                            Array.from({ length: 8 }, (_, index) =>
                                page.request.get(
                                    `${server.url}/api/relationships/${index % 2 ? 'check?project=REL' : `graph?project=REL&task_id=${source.id}`}`,
                                ),
                            ),
                        );
                        await page
                            .getByRole('navigation')
                            .getByRole('link', { name: 'Preferences', exact: true })
                            .click();
                        await page
                            .getByRole('navigation')
                            .getByRole('link', { name: 'Dependencies', exact: true })
                            .click();
                        for (const response of await reads) {
                            expect(response.status(), await response.text()).toBe(200);
                            expect((await response.json()).data.complete).toBe(true);
                        }
                        await expect
                            .poll(() =>
                                page
                                    .getByRole('region', { name: 'Dependency view', exact: true })
                                    .getAttribute('aria-busy'),
                            )
                            .toBe('false');
                        await expect
                            .poll(() => page.locator('.dependency-node, .dependency-task-list > li').count())
                            .toBeGreaterThan(0);
                        expect(await page.getByText(/Relationship snapshot unavailable/).count()).toBe(0);
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('contains a many-stage graph with internal horizontal scrolling on desktop and mobile', async ({ expect }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            const tasks = [source, target, third];
            while (tasks.length < 9) tasks.push(await workspace.addTask(`Stage fixture ${tasks.length + 1}`));
            for (let index = 1; index < tasks.length; index++)
                await workspace.runLotar([
                    'task',
                    'relationships',
                    'add',
                    tasks[index]!.id,
                    tasks[index - 1]!.id,
                    '--kind',
                    'depends-on',
                ]);
            const server = await startLotarServer(workspace);
            try {
                for (const width of [1600, 1024, 390]) {
                    await withPage('about:blank', async (page) => {
                        await page.setViewportSize({ width, height: 1000 });
                        await page.goto(`${server.url}/dependencies?project=REL&depth=8`);
                        if (width < 700) await page.getByRole('button', { name: 'Show diagram', exact: true }).click();
                        await expect.poll(() => page.locator('.dependency-node').count()).toBe(9);
                        const pageBox = await page.locator('.dependencies-page').boundingBox();
                        const viewBox = await page
                            .getByRole('region', { name: 'Dependency view', exact: true })
                            .boundingBox();
                        const diagram = page.locator('.dependency-diagram');
                        const plotBox = await diagram.boundingBox();
                        expect(pageBox!.width).toBeLessThanOrEqual(width);
                        expect(viewBox!.width).toBeLessThanOrEqual(pageBox!.width + 1);
                        expect(plotBox!.width).toBeLessThanOrEqual(viewBox!.width + 1);
                        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
                            width + 1,
                        );
                        const scroll = await diagram.evaluate((element) => {
                            element.scrollLeft = element.scrollWidth;
                            return {
                                left: element.scrollLeft,
                                content: element.scrollWidth,
                                viewport: element.clientWidth,
                            };
                        });
                        expect(scroll.content).toBeGreaterThan(scroll.viewport);
                        expect(scroll.left).toBeGreaterThan(0);
                    });
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('preserves plot height and page/plot scroll through a delayed root reload', async ({ expect }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            const chain = [source, target, third];
            while (chain.length < 9) chain.push(await workspace.addTask(`Deep fixture ${chain.length + 1}`));
            for (let index = 1; index < chain.length; index++)
                await workspace.runLotar([
                    'task',
                    'relationships',
                    'add',
                    chain[index]!.id,
                    chain[index - 1]!.id,
                    '--kind',
                    'depends-on',
                ]);
            const branches = [];
            for (let index = 0; index < 6; index++) {
                const branch = await workspace.addTask(`Branch fixture ${index}`);
                branches.push(branch);
                await workspace.runLotar([
                    'task',
                    'relationships',
                    'add',
                    branch.id,
                    chain[chain.length - 2]!.id,
                    '--kind',
                    'depends-on',
                ]);
            }
            const nextRoot = branches[0]!;
            const server = await startLotarServer(workspace);
            try {
                await withPage('about:blank', async (page) => {
                    await page.setViewportSize({ width: 1440, height: 1000 });
                    await page.goto(`${server.url}/dependencies?project=REL&depth=1`);
                    await expect.poll(() => page.locator('.dependency-node').count()).toBe(15);
                    let previousGeometry = '';
                    await expect
                        .poll(async () => {
                            const geometry = await page.locator('.dependency-node').evaluateAll((elements) =>
                                JSON.stringify(
                                    elements.map((element) => {
                                        const box = element.getBoundingClientRect();
                                        return [box.x, box.y, box.width, box.height];
                                    }),
                                ),
                            );
                            const settled = geometry === previousGeometry;
                            previousGeometry = geometry;
                            return settled;
                        })
                        .toBe(true);
                    const diagram = page.locator('.dependency-diagram');
                    const card = page.locator(`.dependency-node[data-task-id="${nextRoot.id}"]`);
                    await diagram.evaluate((plot, id) => {
                        const node = plot.querySelector<HTMLElement>(`[data-task-id="${id}"]`)!;
                        plot.scrollLeft = Math.max(0, node.offsetLeft - 100);
                        plot.scrollTop = Math.max(0, node.offsetTop - 180);
                        window.scrollTo(0, window.scrollY + plot.getBoundingClientRect().top - 80);
                    }, nextRoot.id);
                    const before = await diagram.evaluate((element) => ({
                        windowY: window.scrollY,
                        top: element.scrollTop,
                        left: element.scrollLeft,
                        height: element.getBoundingClientRect().height,
                        plotTop: element.getBoundingClientRect().top,
                        documentHeight: document.documentElement.scrollHeight,
                    }));
                    expect(before.windowY).toBeGreaterThan(0);
                    expect(before.top).toBeGreaterThan(0);
                    expect(before.left).toBeGreaterThan(0);
                    let release!: () => void, arrived!: () => void;
                    const held = new Promise<void>((resolve) => {
                        release = resolve;
                    });
                    const started = new Promise<void>((resolve) => {
                        arrived = resolve;
                    });
                    await page.route('**/api/relationships/graph?**', async (route) => {
                        if (new URL(route.request().url()).searchParams.get('task_id') === nextRoot.id) {
                            arrived();
                            await held;
                        }
                        await route.continue();
                    });
                    try {
                        await card.locator('a').click();
                        await started;
                        expect(
                            await page
                                .getByRole('region', { name: 'Dependency view', exact: true })
                                .getAttribute('aria-busy'),
                        ).toBe('true');
                        expect(await page.locator('.dependency-render').getAttribute('inert')).not.toBeNull();
                        expect(await page.locator('.dependency-render').getAttribute('aria-hidden')).toBe('true');
                        const pending = await diagram.evaluate((element) => ({
                            windowY: window.scrollY,
                            top: element.scrollTop,
                            left: element.scrollLeft,
                            height: element.getBoundingClientRect().height,
                            plotTop: element.getBoundingClientRect().top,
                            documentHeight: document.documentElement.scrollHeight,
                        }));
                        // Browser scroll anchoring may adjust document coordinates while keeping the plot visually stationary.
                        expect(
                            Math.abs(pending.plotTop - before.plotTop),
                            JSON.stringify({ before, pending }),
                        ).toBeLessThan(2);
                        expect(pending.windowY).toBeGreaterThanOrEqual(before.windowY - 1);
                        expect(Math.abs(pending.top - before.top)).toBeLessThan(2);
                        expect(Math.abs(pending.left - before.left)).toBeLessThan(2);
                        expect(pending.height).toBeGreaterThanOrEqual(before.height - 1);
                        release();
                        await page.waitForURL((url) => url.searchParams.get('root') === nextRoot.id);
                        await expect
                            .poll(() =>
                                page
                                    .getByRole('region', { name: 'Dependency view', exact: true })
                                    .getAttribute('aria-busy'),
                            )
                            .toBe('false');
                        await expect
                            .poll(() =>
                                page.locator('.dependency-node[data-focused="true"]').getAttribute('data-task-id'),
                            )
                            .toBe(nextRoot.id);
                        expect(
                            await page
                                .getByRole('region', { name: 'Dependency view', exact: true })
                                .getByRole('alert')
                                .count(),
                        ).toBe(0);
                        expect(await page.evaluate(() => window.scrollY)).toBeGreaterThanOrEqual(before.windowY - 1);
                        expect((await diagram.boundingBox())!.height).toBeGreaterThanOrEqual(before.height - 1);
                        const plot = (await diagram.boundingBox())!,
                            focused = (await page.locator('.dependency-node[data-focused="true"]').boundingBox())!;
                        expect(
                            Math.min(plot.x + plot.width, focused.x + focused.width) - Math.max(plot.x, focused.x),
                        ).toBeGreaterThan(0);
                        expect(
                            Math.min(plot.y + plot.height, focused.y + focused.height) - Math.max(plot.y, focused.y),
                        ).toBeGreaterThan(0);
                        await page.getByRole('button', { name: 'Close panel', exact: true }).click();
                        await page.waitForURL((url) => !url.searchParams.has('taskPanel'));
                        await page.getByRole('dialog').waitFor({ state: 'hidden' });
                        expect(await page.evaluate(() => window.scrollY)).toBeGreaterThanOrEqual(before.windowY - 1);
                    } finally {
                        release();
                        await page.unrouteAll({ behavior: 'wait' });
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('previews a problem-card fix and changes only that reciprocal assertion after confirmation', async ({
        expect,
    }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            await writeRelationships(workspace, source.id, { related: [target.id, third.id] });
            const before = await Promise.all([source.id, target.id, third.id].map((id) => workspace.readTaskYaml(id)));
            const server = await startLotarServer(workspace);
            try {
                await withPage(
                    `${server.url}/dependencies?project=REL&root=${source.id}&relationships=related`,
                    async (page) => {
                        const problems = page.getByRole('region', { name: 'Dependency issues', exact: true });
                        await problems.locator('summary').click();
                        const card = problems
                            .getByRole('list', { name: 'Problems in displayed graph', exact: true })
                            .locator(':scope > li')
                            .filter({ hasText: target.id });
                        const previewed = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/relationships/repair',
                        );
                        await card.getByRole('button', { name: 'Fix', exact: true }).click();
                        const response = await previewed;
                        expect(response.status()).toBe(200);
                        const ids = response.request().postDataJSON().issue_ids;
                        expect(ids).toHaveLength(1);
                        expect((await response.json()).data.changed_tasks).toEqual([target.id]);
                        const preview = page.getByRole('region', { name: 'Safe repair preview', exact: true });
                        await preview.waitFor();
                        expect(await preview.textContent()).toContain('Only this problem is included');
                        await expect
                            .poll(
                                async () =>
                                    (await preview.boundingBox())!.y -
                                    (await page.locator('.topbar').boundingBox())!.height,
                            )
                            .toBeGreaterThanOrEqual(0);
                        expect(
                            await Promise.all([source.id, target.id, third.id].map((id) => workspace.readTaskYaml(id))),
                        ).toEqual(before);
                        const applied = page.waitForResponse(
                            (result) =>
                                new URL(result.url()).pathname === '/api/relationships/repair' &&
                                result.request().postDataJSON().apply === true,
                        );
                        await preview
                            .getByRole('button', { name: 'Confirm and apply safe repair', exact: true })
                            .click();
                        const receipt = await applied;
                        expect(receipt.status()).toBe(200);
                        expect(receipt.request().postDataJSON().issue_ids).toEqual(ids);
                        await expect
                            .poll(async () => (await relationships(workspace, target.id)).related)
                            .toEqual([source.id]);
                        expect((await relationships(workspace, third.id)).related || []).toEqual([]);
                        expect(await workspace.readTaskYaml(source.id)).toBe(before[0]);
                    },
                );
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('selects connected tasks before the node budget instead of spending it on recent isolated tickets', async ({
        expect,
    }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            for (const task of [target, third])
                await workspace.runLotar(['task', 'relationships', 'add', source.id, task.id, '--kind', 'blocks']);
            for (let index = 0; index < 12; index++) await workspace.addTask(`Recent isolated fixture ${index}`);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?project=REL`, async (page) => {
                    const base = new URLSearchParams({
                        project: 'REL',
                        depth: '1',
                        max_nodes: '6',
                        kinds: 'dependency',
                        include_isolated: 'false',
                        filters: JSON.stringify({ deletion: 'active', sort_by: 'modified', order: 'desc' }),
                    });
                    const all = await page.request.get(`${server.url}/api/relationships/graph?${base}`);
                    expect(all.status(), await all.text()).toBe(200);
                    const graph = (await all.json()).data;
                    expect(graph.nodes.map((node: { id: string }) => node.id).sort()).toEqual(
                        [source.id, target.id, third.id].sort(),
                    );
                    expect(graph.truncated).toBe(false);
                    base.set('task_id', source.id);
                    const selected = await page.request.get(`${server.url}/api/relationships/graph?${base}`);
                    expect(selected.status()).toBe(200);
                    expect((await selected.json()).data.nodes.map((node: { id: string }) => node.id).sort()).toEqual(
                        graph.nodes.map((node: { id: string }) => node.id).sort(),
                    );
                    base.delete('task_id');
                    base.set('include_isolated', 'true');
                    const including = await page.request.get(`${server.url}/api/relationships/graph?${base}`);
                    expect(including.status()).toBe(200);
                    expect((await including.json()).data.nodes).toHaveLength(6);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('explains why two hops around a middle task can produce four ordering stages', async ({ expect }) => {
        const { workspace, source, target, third } = await fixture();
        try {
            const fourth = await workspace.addTask('Fourth chain task');
            const chain = [source, target, third, fourth];
            for (let index = 1; index < chain.length; index++)
                await workspace.runLotar([
                    'task',
                    'relationships',
                    'add',
                    chain[index - 1]!.id,
                    chain[index]!.id,
                    '--kind',
                    'blocks',
                ]);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/dependencies?project=REL&root=${third.id}&depth=2`, async (page) => {
                    await expect.poll(() => page.locator('.dependency-node').count()).toBe(4);
                    await page.getByText('Stage 4', { exact: true }).waitFor();
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    expect(
                        await page.getByRole('combobox', { name: 'Connection range', exact: true }).inputValue(),
                    ).toBe('2');
                    expect(await page.getByText(/Stages arrange the returned tasks/).textContent()).toContain(
                        'not hop counts',
                    );
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
