import fs from 'fs-extra';
import path from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function seed(): Record<string, string> {
    const today = new Date().toISOString().slice(0, 10);
    const files: Record<string, string> = {
        '.tasks/config.yml': stringify({
            issue: {
                states: ['Todo', 'In Progress', 'Done'],
                priorities: ['Low', 'Medium', 'High'],
                types: ['Feature', 'Bug', 'Chore'],
            },
            custom_fields: ['iteration', 'sprint'],
            default: { reporter: 'Fixture' }, auto: { set_reporter: false },
        }),
        '.tasks/ALPHA/config.yml': [
            'project.name: Alpha fixture',
            'issue:',
            '  tags:',
            '    - ui',
            '    - api',
            '    - release candidate',
            'members:',
            '  - alice',
            '  - bob',
            '',
        ].join('\n'),
        '.tasks/BETA/config.yml': [
            'project.name: Beta fixture',
            'issue:',
            '  tags:',
            '    - infra',
            '    - ops',
            'members:',
            '  - carol',
            '',
        ].join('\n'),
        '.tasks/@sprints/1.yml': stringify({
            created: `${today}T00:00:00Z`, plan_length: '1w',
            plan: { label: 'Alpha sprint', starts_at: `${today}T00:00:00Z` },
            actual: { started_at: `${today}T00:00:00Z` }, tasks: [{ id: 'ALPHA-1' }, { id: 'ALPHA-2' }],
        }),
    };
    for (const [id, title, status, assignee, tags, iteration, sprintValue] of [
        ['ALPHA-1', 'Alpha ui task', 'In Progress', 'alice', ['ui', 'release candidate'], 'wave one', 'relay'],
        ['ALPHA-2', 'Alpha api task', 'Todo', 'bob', ['api'], 'wave two', 'core'],
        ['BETA-1', 'Beta sibling task', 'In Progress', 'carol', ['infra', 'ops'], 'wave one', 'beta'],
    ] as const) {
        const [project, number] = id.split('-');
        files[`.tasks/${project}/${number}.yml`] = stringify({
            title, status, priority: 'High', type: 'Feature', reporter: 'Fixture', assignee,
            due_date: today, created: `${today}T00:00:00Z`, modified: `${today}T00:00:00Z`,
            custom_fields: { iteration, sprint: sprintValue },
            tags,
            ...(project === 'ALPHA' ? { sprints: [1] } : {}),
        });
    }
    return files;
}

const HELP_KEYS = [
    'status', 'priority', 'type', 'sprints', 'project', 'assignee', 'tags', 'due',
    'recent', 'needs', 'mine', 'field', 'q', 'deletion', 'order', 'sort_by',
];

function suggestionList(page: Page): Locator {
    return page.getByRole('listbox', { name: 'Filter suggestions', exact: true });
}

function suggestionLabels(page: Page): Promise<string[]> {
    return page.evaluate(() =>
        Array.from(document.querySelectorAll('[role="option"]')).map((option) => {
            const label = option.querySelector('.filter-bar__suggestion-label');
            return (label?.textContent ?? option.textContent ?? '').trim();
        }),
    );
}

async function pickWithKeyboard(page: Page, search: Locator, name: string): Promise<void> {
    const labels = await suggestionLabels(page);
    const index = labels.indexOf(name);
    if (index < 0) throw new Error(`Expected suggestion "${name}", got ${JSON.stringify(labels)}`);
    for (let move = 0; move < index; move++) await search.press('ArrowDown');
    const highlighted = await page.evaluate(
        () => document.querySelector('[role="option"][aria-selected="true"] .filter-bar__suggestion-label')?.textContent?.trim() ?? null,
    );
    if (highlighted !== name) throw new Error(`Expected highlight "${name}", got "${highlighted}"`);
    await search.press('Enter');
}

async function pickWithMouse(page: Page, search: Locator, name: string): Promise<void> {
    const labels = await suggestionLabels(page);
    if (!labels.includes(name)) throw new Error(`Expected suggestion "${name}", got ${JSON.stringify(labels)}`);
    await page.locator('[role="option"]').filter({ hasText: name }).first().click();
    if (!(await search.evaluate(el => el === document.activeElement))) throw new Error('mouse pick must keep focus in the search input');
}

function appliedListQuery(listQueries: string[], match: (params: URLSearchParams) => boolean): URLSearchParams | undefined {
    for (const query of listQueries.slice().reverse()) {
        const params = new URLSearchParams(query);
        if (match(params)) return params;
    }
    return undefined;
}

describe.concurrent('Filter autocomplete and search help (DEV-103)', () => {
    for (const viewport of [
        { label: 'desktop', width: 1600, height: 1000 },
        { label: 'mobile', width: 390, height: 844 },
    ]) {
        it(`offers scoped filter suggestions and compact help beside search on ${viewport.label}`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
            const before = await workspace.readTaskYaml('ALPHA-1');
            const artifacts = path.resolve('target/smoke-artifacts/filters');
            await fs.ensureDir(artifacts);
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/`, async page => {
                        await page.setViewportSize(viewport);
                        const mutations: string[] = [];
                        const listQueries: string[] = [];
                        page.on('request', request => {
                            const url = new URL(request.url());
                            if (['POST', 'PUT', 'DELETE'].includes(request.method())) mutations.push(url.pathname);
                            if (url.pathname === '/api/tasks/list') listQueries.push(url.search);
                        });
                        const search = page.getByTestId('filter-search');
                        const helpToggle = page.getByTestId('filter-help-toggle');
                        const panel = page.getByRole('region', { name: 'Filters', exact: true });
                        const alphaUi = page.getByText('Alpha ui task', { exact: true });
                        const alphaApi = page.getByText('Alpha api task', { exact: true });
                        const betaSibling = page.getByText('Beta sibling task', { exact: true });
                        const pickTag = viewport.label === 'desktop' ? pickWithKeyboard : pickWithMouse;

                        await page.getByTestId('filter-toggle').click();
                        await page.getByTestId('filter-project').selectOption('ALPHA');
                        await page.getByTestId('filter-toggle').click();
                        await panel.waitFor({ state: 'detached' });
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(1);
                        await expect.poll(() => betaSibling.count()).toBe(0);

                        const help = page.getByTestId('filter-help');
                        const closedHelpColor = await helpToggle.evaluate(el => getComputedStyle(el).color);
                        const helpStyle = await helpToggle.evaluate(el => {
                            const style = getComputedStyle(el);
                            return { border: style.borderTopWidth, width: style.width, height: style.height };
                        });
                        expect(helpStyle).toEqual({ border: '0px', width: '32px', height: '32px' });
                        await helpToggle.click();
                        const helpRegion = page.getByRole('region', { name: 'Search filters', exact: true });
                        await helpRegion.waitFor({ state: 'visible' });
                        const searchBounds = await search.boundingBox();
                        const helpToggleBounds = await helpToggle.boundingBox();
                        expect(Math.abs(helpToggleBounds!.y - searchBounds!.y)).toBeLessThanOrEqual(1);
                        expect(helpToggleBounds!.x).toBeGreaterThanOrEqual(searchBounds!.x + searchBounds!.width - 1);
                        expect(helpToggleBounds!.x - (searchBounds!.x + searchBounds!.width)).toBeLessThanOrEqual(4.5);
                        expect(await helpToggle.getAttribute('aria-expanded')).toBe('true');
                        await expect.poll(() => helpToggle.evaluate(el => getComputedStyle(el).color)).not.toBe(closedHelpColor);

                        await page.getByTestId('filter-toggle').click();
                        await page.getByTestId('filter-status').click();
                        expect(await help.isVisible()).toBe(true);
                        await page.getByTestId('filter-status').click();
                        await page.getByTestId('filter-toggle').click();
                        await panel.waitFor({ state: 'detached' });
                        expect(await help.isVisible()).toBe(true);
                        expect(await helpToggle.getAttribute('aria-expanded')).toBe('true');
                        await page.getByTestId('filter-toggle').click();
                        await helpToggle.click();
                        await help.waitFor({ state: 'detached' });
                        expect(await panel.isVisible()).toBe(true);
                        await helpToggle.click();
                        await page.getByTestId('filter-status').focus();
                        await page.keyboard.press('Escape');
                        await panel.waitFor({ state: 'detached' });
                        expect(await help.isVisible()).toBe(true);
                        const helpText = (await helpRegion.textContent()) ?? '';
                        for (const key of HELP_KEYS) {
                            expect(helpText, `help mentions ${key}`).toMatch(new RegExp(`\\b${key}\\b`));
                        }
                        const helpBounds = await helpRegion.boundingBox();
                        expect(helpBounds!.x).toBeGreaterThanOrEqual(0);
                        expect(helpBounds!.x + helpBounds!.width).toBeLessThanOrEqual(viewport.width);
                        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
                        await page.screenshot({ path: path.join(artifacts, `help-${viewport.label}.png`), animations: 'disabled' });
                        await helpRegion.focus();
                        await page.keyboard.press('Escape');
                        await help.waitFor({ state: 'detached' });
                        expect(await helpToggle.getAttribute('aria-expanded')).toBe('false');
                        await expect.poll(() => helpToggle.evaluate(el => getComputedStyle(el).color)).toBe(closedHelpColor);
                        expect(await helpToggle.evaluate(el => el === document.activeElement)).toBe(true);
                        await helpToggle.click();
                        await helpRegion.waitFor({ state: 'visible' });
                        await search.click();
                        await help.waitFor({ state: 'detached' });

                        await search.fill('tags:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        let labels = await suggestionLabels(page);
                        expect(labels).toContain('ui');
                        expect(labels).toContain('release candidate');
                        await page.screenshot({ path: path.join(artifacts, `autocomplete-${viewport.label}.png`), animations: 'disabled' });
                        await search.fill('tags:release');
                        await pickTag(page, search, 'release candidate');
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(0);
                        expect(await betaSibling.count()).toBe(0);
                        const tagQuery = appliedListQuery(listQueries, params => params.get('tags') === 'release candidate');
                        expect(tagQuery).toBeDefined();
                        expect(tagQuery!.get('q') ?? '').toBe('');
                        expect(tagQuery!.get('project')).toBe('ALPHA');
                        expect(await page.getByTestId('filter-chips').textContent()).toContain('release candidate');
                        if (viewport.label === 'desktop') {
                            await page.getByRole('button', { name: 'Remove filter Tag release candidate', exact: true }).click();
                        } else {
                            await page.getByTestId('filter-toggle').click();
                            await panel.getByRole('button', { name: 'Clear conditions', exact: true }).click();
                            await page.getByTestId('filter-toggle').click();
                            await panel.waitFor({ state: 'detached' });
                        }
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(1);

                        await search.fill('status:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels).toContain('In Progress');
                        await pickTag(page, search, 'In Progress');
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(0);
                        const statusQuery = appliedListQuery(listQueries, params => params.get('status') === 'In Progress');
                        expect(statusQuery).toBeDefined();
                        expect(statusQuery!.get('project')).toBe('ALPHA');
                        await page.getByRole('button', { name: 'Remove filter Status In Progress', exact: true }).click();
                        await expect.poll(() => alphaApi.count()).toBe(1);

                        await search.fill('assignee:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels).toContain('alice');
                        expect(labels).toContain('bob');
                        await pickTag(page, search, 'alice');
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(0);
                        const assigneeQuery = appliedListQuery(listQueries, params => params.get('assignee') === 'alice');
                        expect(assigneeQuery).toBeDefined();
                        await page.getByRole('button', { name: 'Remove filter Assignee alice', exact: true }).click();
                        await expect.poll(() => alphaApi.count()).toBe(1);

                        await search.fill('needs:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels.slice().sort()).toEqual(['due', 'effort']);
                        await search.fill('due:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels.slice().sort()).toEqual(['later', 'overdue', 'soon', 'today']);
                        expect(labels).not.toContain('week');
                        expect(labels).not.toContain('month');
                        await search.fill('recent:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels).toEqual(['7d']);
                        expect(labels).not.toContain('1d');
                        expect(labels).not.toContain('30d');
                        await search.press('Escape');
                        await suggestionList(page).waitFor({ state: 'detached' });
                        expect(await search.evaluate(el => el === document.activeElement)).toBe(true);

                        await search.fill('field:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels).toContain('iteration');
                        expect(labels).toContain('sprint');
                        await pickTag(page, search, 'iteration');
                        expect(await search.inputValue()).toBe('field:iteration=');
                        expect(await search.evaluate(el => el === document.activeElement)).toBe(true);
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels).toContain('wave one');
                        await pickTag(page, search, 'wave one');
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(0);
                        const iterationQuery = appliedListQuery(listQueries, params => params.get('field:iteration') === 'wave one');
                        expect(iterationQuery).toBeDefined();
                        await page.getByRole('button', { name: 'Remove filter iteration wave one', exact: true }).click();
                        await expect.poll(() => alphaApi.count()).toBe(1);

                        await search.fill('field:"sprint"=relay');
                        await search.press('Enter');
                        await expect.poll(() => alphaUi.count()).toBe(1);
                        await expect.poll(() => alphaApi.count()).toBe(0);
                        const customSprintQuery = appliedListQuery(listQueries, params => params.get('field:sprint') === 'relay');
                        expect(customSprintQuery).toBeDefined();
                        expect(customSprintQuery!.get('sprints')).toBeNull();
                        await page.getByRole('button', { name: 'Remove filter sprint relay', exact: true }).click();
                        await expect.poll(() => alphaApi.count()).toBe(1);

                        await search.fill('deletion=');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels.slice().sort()).toEqual(['active', 'all', 'deleted']);
                        await search.fill('order=');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels.slice().sort()).toEqual(['asc', 'desc']);
                        await search.fill('mystery:blue');
                        expect(await suggestionList(page).count()).toBe(0);
                        await search.press('Enter');
                        await expect.poll(() => alphaUi.count()).toBe(0);
                        await expect.poll(() => alphaApi.count()).toBe(0);
                        const mysteryQuery = appliedListQuery(listQueries, params => params.get('q') === 'mystery:blue');
                        expect(mysteryQuery).toBeDefined();
                        expect(mysteryQuery!.has('mystery')).toBe(false);
                        expect(mysteryQuery!.has('mystery:blue')).toBe(false);
                        await page.getByRole('button', { name: 'Clear search', exact: true }).click();
                        expect(await search.inputValue()).toBe('');
                        await expect.poll(() => alphaApi.count()).toBe(1);

                        await page.getByTestId('filter-toggle').click();
                        await page.getByTestId('filter-project').selectOption('BETA');
                        await page.getByTestId('filter-toggle').click();
                        await panel.waitFor({ state: 'detached' });
                        await expect.poll(() => betaSibling.count()).toBe(1);
                        await expect.poll(() => alphaUi.count()).toBe(0);
                        await search.fill('tags:');
                        await suggestionList(page).waitFor({ state: 'visible' });
                        labels = await suggestionLabels(page);
                        expect(labels).toContain('infra');
                        expect(labels).not.toContain('ui');
                        expect(labels).not.toContain('api');
                        expect(labels).not.toContain('release candidate');
                        await pickTag(page, search, 'infra');
                        await expect.poll(() => betaSibling.count()).toBe(1);
                        await expect.poll(() => appliedListQuery(listQueries, params => params.get('project') === 'BETA' && params.get('tags') === 'infra')).toBeDefined();
                        const scopedTagQuery = appliedListQuery(listQueries, params => params.get('tags') === 'infra');
                        expect(scopedTagQuery).toBeDefined();
                        expect(scopedTagQuery!.get('project')).toBe('BETA');
                        await page.getByRole('button', { name: 'Remove filter Tag infra', exact: true }).click();
                        await expect.poll(() => betaSibling.count()).toBe(1);

                        for (const route of ['/boards?project=ALPHA', '/calendar?project=ALPHA', '/sprints?project=ALPHA']) {
                            await page.goto(`${server.url}${route}`);
                            const titles = page.locator(route.startsWith('/boards') ? 'article.task .title'
                                : route.startsWith('/calendar') ? '.task-item .task-inline > .title' : 'tr.task-row .task-title');
                            await expect.poll(() => titles.filter({ hasText: /^\s*Alpha ui task\s*$/ }).count(), { message: route }).toBeGreaterThanOrEqual(1);
                            await page.getByTestId('filter-search').fill('tag:');
                            await suggestionList(page).waitFor({ state: 'visible' });
                            labels = await suggestionLabels(page);
                            expect(labels, route).toContain('ui');
                            await page.keyboard.press('Escape');
                            await suggestionList(page).waitFor({ state: 'detached' });
                        }

                        expect(mutations).toEqual([]);
                        expect(await workspace.readTaskYaml('ALPHA-1')).toBe(before);
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
