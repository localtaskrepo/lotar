import fs from 'fs-extra';
import path from 'node:path';
import type { Locator, Page } from 'playwright';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function seedFiles(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: UX\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
        '.tasks/UX/config.yml': 'project.name: Responsive fixture\n',
    };
    const memberships = [[], []] as string[][];
    for (let index = 1; index <= 195; index++) {
        const sprint = index <= 65 ? 1 : index <= 130 ? 2 : null;
        const id = `UX-${index}`;
        if (sprint) memberships[sprint - 1]!.push(id);
        files[`.tasks/UX/${index}.yml`] = stringify({
            title: `Responsive task ${String(index).padStart(3, '0')}`,
            status: 'Todo', priority: 'Medium', task_type: 'Feature', reporter: 'Fixture',
            created: '2026-10-01T00:00:00Z', modified: '2026-10-01T00:00:00Z',
            sprints: sprint ? [sprint] : [],
        });
    }
    for (const id of [1, 2]) {
        files[`.tasks/@sprints/${id}.yml`] = stringify({
            created: '2026-10-01T00:00:00Z',
            plan: { label: `Responsive sprint ${id} with a deliberately long planning label`, goal: 'Keep planning controls reachable at every viewport width' },
            tasks: memberships[id - 1],
        });
    }
    return files;
}

async function initializePage(page: Page, url: string, width: number, height: number) {
    await page.setViewportSize({ width, height });
    await page.addInitScript(() => {
        localStorage.setItem('lotar.preferences.tasks.pageSize', '25');
        localStorage.setItem('lotar.sprints.window.v2', 'all');
    });
    await page.goto(`${url}/sprints?project=UX`);
    await page.locator('[data-sprint-id="1"] tr.task-row').first().waitFor({ state: 'visible' });
}

async function geometry(dialog: Locator) {
    return dialog.evaluate(element => {
        const bounds = element.getBoundingClientRect();
        return {
            x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height,
            clientWidth: element.clientWidth, scrollWidth: element.scrollWidth,
            fields: Array.from(element.querySelectorAll('.input')).map(field => {
                const box = field.getBoundingClientRect();
                return { x: box.x, right: box.right, width: box.width };
            }),
            buttons: Array.from(element.querySelectorAll('button.btn')).map(button => {
                const box = button.getBoundingClientRect();
                return { name: button.getAttribute('aria-label') || button.textContent?.trim(), height: box.height, width: box.width, iconOnly: button.classList.contains('icon-only'), x: box.x, right: box.right };
            }),
        };
    });
}

describe.concurrent('Responsive sprint controls and standardized pagination (DEV-105)', () => {
    for (const viewport of [
        { width: 1440, height: 900 },
        { width: 1024, height: 768 },
        { width: 768, height: 1024 },
        { width: 844, height: 390 },
        { width: 390, height: 844 },
        { width: 320, height: 640 },
    ]) {
        it(`fits controls and dialogs at ${viewport.width}px`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ name: 'responsive-sprints-', seedFiles: seedFiles() });
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(server.url, async page => {
                        await initializePage(page, server.url, viewport.width, viewport.height);
                        const phase = process.env.LOTAR_LAYOUT_PHASE === 'before' ? 'before' : 'after';
                        const artifacts = path.resolve(`target/smoke-artifacts/sprint-layout/${phase}/${viewport.width}`);
                        await fs.ensureDir(artifacts);
                        const headerButtons = await page.locator('.group-actions .btn, .create-sprint-button, .collapse-btn').evaluateAll(buttons => buttons.map(button => {
                            const bounds = button.getBoundingClientRect();
                            return { name: button.getAttribute('aria-label') || button.textContent?.trim(), height: bounds.height, width: bounds.width, iconOnly: button.classList.contains('icon-only') || button.classList.contains('collapse-btn'), x: bounds.x, right: bounds.right };
                        }));
                        const pageWidth = await page.evaluate(() => document.documentElement.scrollWidth);
                        const rootFontSize = await page.evaluate(() => getComputedStyle(document.documentElement).fontSize);
                        const headerMetadata = await page.locator('[data-sprint-id="1"] .group-header .meta-item').evaluateAll(items => items.map(item => {
                            const box = item.getBoundingClientRect();
                            const title = item.closest('.group-title')!.getBoundingClientRect();
                            return { name: item.getAttribute('aria-label') || item.textContent?.trim(), right: box.right, titleRight: title.right };
                        }));
                        await page.screenshot({ path: path.join(artifacts, 'sections.png'), animations: 'disabled' });
                        const sprintPager = page.locator('[data-sprint-id="1"]').getByRole('navigation');
                        if (await sprintPager.count()) {
                            await sprintPager.scrollIntoViewIfNeeded();
                            await page.screenshot({ path: path.join(artifacts, 'pagination.png'), animations: 'disabled' });
                        }

                        await page.getByRole('button', { name: 'Create sprint', exact: true }).click();
                        const create = page.getByRole('dialog', { name: 'Create sprint', exact: true });
                        await create.waitFor({ state: 'visible' });
                        const createGeometry = await geometry(create);
                        await page.screenshot({ path: path.join(artifacts, 'create-sprint.png'), animations: 'disabled' });
                        const createSubmit = create.getByRole('button', { name: 'Create sprint', exact: true });
                        await createSubmit.scrollIntoViewIfNeeded();
                        const submitBounds = await createSubmit.boundingBox();
                        expect(submitBounds).not.toBeNull();
                        expect(submitBounds!.y).toBeGreaterThanOrEqual(0);
                        expect(submitBounds!.y + submitBounds!.height).toBeLessThanOrEqual(viewport.height + 1);
                        await page.screenshot({ path: path.join(artifacts, 'create-sprint-footer.png'), animations: 'disabled' });
                        await create.getByRole('button', { name: 'Close dialog', exact: true }).click();
                        await create.waitFor({ state: 'detached' });

                        await page.locator('[data-sprint-id="1"]').getByRole('button', { name: 'Edit sprint', exact: true }).click();
                        const edit = page.getByRole('dialog', { name: 'Edit sprint', exact: true });
                        await edit.waitFor({ state: 'visible' });
                        await expect.poll(() => edit.getByPlaceholder('Sprint name', { exact: true }).inputValue()).toContain('Responsive sprint 1');
                        const editGeometry = await geometry(edit);
                        await page.screenshot({ path: path.join(artifacts, 'edit-sprint.png'), animations: 'disabled' });
                        await edit.getByRole('button', { name: 'Save changes', exact: true }).scrollIntoViewIfNeeded();
                        await edit.getByRole('button', { name: 'Close dialog', exact: true }).click();
                        await edit.waitFor({ state: 'detached' });

                        await page.getByTestId('backlog-add-task').click();
                        const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                        await panel.waitFor({ state: 'visible' });
                        await panel.locator('input[placeholder="Title"]').fill('Responsive draft stays intact');
                        const sprintTrigger = panel.locator('.task-panel__sprint-field .chip-field__add');
                        await sprintTrigger.click();
                        const assign = page.getByRole('dialog', { name: 'Assign to sprint', exact: true });
                        await assign.waitFor({ state: 'visible' });
                        const assignGeometry = await geometry(assign);
                        await page.screenshot({ path: path.join(artifacts, 'assign-sprint.png'), animations: 'disabled' });
                        await fs.writeJson(path.join(artifacts, 'geometry.json'), { viewport, rootFontSize, pageWidth, headerButtons, headerMetadata, create: createGeometry, edit: editGeometry, assign: assignGeometry }, { spaces: 2 });

                        expect(pageWidth).toBeLessThanOrEqual(viewport.width + 1);
                        for (const item of headerMetadata) expect(item.right, item.name).toBeLessThanOrEqual(item.titleRight + 1);
                        for (const button of headerButtons.filter(button => button.height > 0)) {
                            expect(button.height, button.name).toBeCloseTo(36, 0);
                            if (button.iconOnly) expect(button.width, button.name).toBeCloseTo(36, 0);
                            expect(button.x, button.name).toBeGreaterThanOrEqual(0);
                            expect(button.right, button.name).toBeLessThanOrEqual(viewport.width + 1);
                        }
                        for (const measured of [createGeometry, editGeometry, assignGeometry]) {
                            expect(measured.x).toBeGreaterThanOrEqual(0);
                            expect(measured.x + measured.width).toBeLessThanOrEqual(viewport.width + 1);
                            expect(measured.y).toBeGreaterThanOrEqual(0);
                            expect(measured.y + measured.height).toBeLessThanOrEqual(viewport.height + 1);
                            expect(measured.scrollWidth).toBeLessThanOrEqual(measured.clientWidth + 1);
                            for (const field of measured.fields) {
                                expect(field.x).toBeGreaterThanOrEqual(measured.x);
                                expect(field.right).toBeLessThanOrEqual(measured.x + measured.width + 1);
                            }
                            for (const button of measured.buttons) {
                                expect(button.height, button.name).toBeCloseTo(36, 0);
                                if (button.iconOnly) expect(button.width, button.name).toBeCloseTo(36, 0);
                            }
                        }

                        await expect.poll(() => assign.evaluate(element => element.contains(document.activeElement))).toBe(true);
                        await assign.getByRole('button', { name: 'Cancel', exact: true }).focus();
                        await page.keyboard.press('Tab');
                        expect(await assign.evaluate(element => element.contains(document.activeElement))).toBe(true);
                        await page.keyboard.press('Shift+Tab');
                        expect(await assign.evaluate(element => element.contains(document.activeElement))).toBe(true);
                        await page.keyboard.press('Escape');
                        await assign.waitFor({ state: 'detached' });
                        expect(await panel.isVisible()).toBe(true);
                        expect(await panel.locator('input[placeholder="Title"]').inputValue()).toBe('Responsive draft stays intact');
                        await expect.poll(() => sprintTrigger.evaluate(element => element === document.activeElement)).toBe(true);
                        await panel.getByRole('button', { name: 'Close panel', exact: true }).click();
                        await panel.waitFor({ state: 'detached' });

                        await page.goto(`${server.url}/boards?project=UX`);
                        const boardAdd = page.locator('.board-col-add').first();
                        await boardAdd.waitFor({ state: 'attached' });
                        expect(await boardAdd.evaluate(element => element.getBoundingClientRect().height)).toBeCloseTo(36, 0);
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }

    it('pages each sprint and the backlog independently using the same footer controls as Tasks', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'shared-pagination-', seedFiles: seedFiles() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async page => {
                    await initializePage(page, server.url, 390, 844);
                    const first = page.locator('[data-sprint-id="1"]');
                    const second = page.locator('[data-sprint-id="2"]');
                    const backlog = page.locator('[data-sprint-id="backlog"]');
                    for (const group of [first, second, backlog]) {
                        await expect.poll(() => group.locator('tr.task-row').count()).toBe(25);
                        const footer = group.getByRole('navigation');
                        await footer.waitFor({ state: 'attached' });
                        expect(await footer.evaluate(element => {
                            const table = element.closest('[data-sprint-id]')?.querySelector('table');
                            return !!table && !!(table.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING);
                        })).toBe(true);
                        expect(await footer.locator('[aria-current="page"]').textContent()).toBe('1');
                        for (const button of await footer.getByRole('button').all()) {
                            const box = await button.boundingBox();
                            expect(box).not.toBeNull();
                            expect(box!.height).toBeCloseTo(36, 0);
                            expect(box!.width).toBeCloseTo(36, 0);
                        }
                    }
                    const firstIds = await first.locator('tr.task-row').evaluateAll(rows => rows.map(row => row.getAttribute('data-task-id')));
                    await first.getByRole('navigation').getByRole('button', { name: 'Page 2', exact: true }).click();
                    await expect.poll(() => first.getByRole('navigation').locator('[aria-current="page"]').textContent()).toBe('2');
                    expect(await first.locator('tr.task-row').evaluateAll(rows => rows.map(row => row.getAttribute('data-task-id')))).not.toEqual(firstIds);
                    expect(await second.getByRole('navigation').locator('[aria-current="page"]').textContent()).toBe('1');
                    expect(await backlog.getByRole('navigation').locator('[aria-current="page"]').textContent()).toBe('1');
                    await backlog.getByRole('navigation').getByRole('button', { name: 'Page 3', exact: true }).click();
                    await expect.poll(() => backlog.locator('tr.task-row').count()).toBe(15);
                    expect(await first.getByRole('navigation').locator('[aria-current="page"]').textContent()).toBe('2');
                    expect(await backlog.getByRole('navigation').getByRole('button', { name: 'Next page', exact: true }).isDisabled()).toBe(true);
                    const paginationWidth = await backlog.getByRole('navigation').evaluate(element => element.scrollWidth - element.clientWidth);
                    expect(paginationWidth).toBeLessThanOrEqual(1);

                    await page.goto(`${server.url}/?project=UX`);
                    const tasksPager = page.getByRole('navigation', { name: 'Task list pagination', exact: true });
                    await tasksPager.waitFor({ state: 'visible' });
                    await tasksPager.getByRole('button', { name: 'Page 2', exact: true }).click();
                    await expect.poll(() => new URL(page.url()).searchParams.get('offset')).toBe('25');
                    expect(await tasksPager.locator('[aria-current="page"]').textContent()).toBe('2');
                    await page.goBack();
                    await expect.poll(() => tasksPager.locator('[aria-current="page"]').textContent()).toBe('1');
                    expect(new URL(page.url()).searchParams.get('offset')).toBeNull();
                    await page.goForward();
                    await expect.poll(() => tasksPager.locator('[aria-current="page"]').textContent()).toBe('2');
                    expect(new URL(page.url()).searchParams.get('offset')).toBe('25');
                    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(391);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
