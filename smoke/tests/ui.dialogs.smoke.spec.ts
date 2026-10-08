import fs from 'fs-extra';
import path from 'node:path';
import type { Locator } from '@playwright/test';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('Accessible shared dialogs (DEV-69)', () => {
    for (const viewport of [
        { label: 'desktop', width: 1280, height: 900 },
        { label: 'mobile', width: 390, height: 844 },
    ]) {
        it(`contains focus and safely dismisses scoped dialogs on ${viewport.label}`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ seedFiles: {
                '.tasks/config.yml': 'default.project: MOD\ndefault.reporter: fixture-user\nmembers: [fixture-user]\nauto.set_reporter: false\n',
                '.tasks/MOD/config.yml': 'project.name: Modal fixture\n',
                'scan-source.txt': '// TODO: Keyboard confirmation fixture\n',
            } });
            const configBefore = await workspace.read('.tasks/config.yml');
            const artifacts = path.resolve('target/smoke-artifacts/dialogs');
            await fs.ensureDir(artifacts);
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/config`, async page => {
                        await page.setViewportSize(viewport);
                        const mutations: string[] = [];
                        page.on('request', request => {
                            const pathname = new URL(request.url()).pathname;
                            if (request.method() === 'POST' && [
                                '/api/projects/create', '/api/config/set', '/api/automation/set', '/api/scan/run',
                            ].includes(pathname)) mutations.push(pathname);
                        });

                        async function exercise(trigger: Locator, title: string, screenshot: string, safeCancel = false) {
                            await trigger.click();
                            const dialog = page.getByRole('dialog', { name: title, exact: true });
                            await dialog.waitFor({ state: 'visible' });
                            expect(await dialog.getAttribute('aria-modal')).toBe('true');
                            expect(await dialog.evaluate(el => {
                                const titleElement = document.getElementById(el.getAttribute('aria-labelledby') || '');
                                return titleElement !== null && el.contains(titleElement);
                            })).toBe(true);
                            expect(await dialog.ariaSnapshot()).toContain(title);
                            expect(await page.evaluate(() => document.body.style.overflow)).toBe('hidden');
                            for (const direction of ['Tab', 'Shift+Tab']) {
                                for (let step = 0; step < 20; step += 1) {
                                    await page.keyboard.press(direction);
                                    expect(await dialog.evaluate(el => el.contains(document.activeElement))).toBe(true);
                                }
                            }
                            const close = dialog.getByRole('button', { name: safeCancel ? 'Cancel' : 'Close dialog', exact: true });
                            await close.focus();
                            await dialog.evaluate(el => { el.scrollTop = 0; });
                            const box = await dialog.boundingBox();
                            expect(box).not.toBeNull();
                            expect(box!.width).toBeLessThanOrEqual(viewport.width);
                            const sizing = await dialog.evaluate(el => {
                                const style = getComputedStyle(el);
                                return { max: style.maxHeight, min: style.minHeight, box: style.boxSizing, padding: style.padding, innerHeight, scroll: el.scrollHeight };
                            });
                            expect(box!.height, JSON.stringify(sizing)).toBeLessThanOrEqual(viewport.height - 48);
                            await dialog.screenshot({ path: path.join(artifacts, `${screenshot}-${viewport.label}.png`) });
                            await page.keyboard.press('Escape');
                            await dialog.waitFor({ state: 'detached' });
                            expect(await trigger.evaluate(el => el === document.activeElement)).toBe(true);
                            expect(await page.evaluate(() => document.body.style.overflow)).not.toBe('hidden');
                            await trigger.click();
                            await dialog.waitFor({ state: 'visible' });
                            if (safeCancel) {
                                expect(await dialog.getByRole('button', { name: 'Cancel', exact: true }).evaluate(el => el === document.activeElement)).toBe(true);
                                await page.keyboard.press('Enter');
                            } else {
                                await page.mouse.click(4, 4);
                            }
                            await dialog.waitFor({ state: 'detached' });
                            expect(await trigger.evaluate(el => el === document.activeElement)).toBe(true);
                        }

                        await exercise(page.getByRole('button', { name: 'Open help', exact: true }), 'Configuration help', 'config-help');
                        await exercise(page.getByRole('button', { name: 'New project', exact: true }), 'Create a project', 'config-create');
                        await page.goto(`${server.url}/scan`);
                        await exercise(page.getByRole('button', { name: 'Run', exact: true }), 'Run scan?', 'scan-confirm', true);
                        await page.goto(`${server.url}/automations`);
                        await exercise(page.getByRole('button', { name: 'New rule', exact: true }), 'Create automation rule', 'automation');
                        await page.goto(`${server.url}/sync`);
                        await exercise(page.getByRole('button', { name: 'Add remote', exact: true }), 'Add remote', 'sync-remote');
                        expect(mutations).toEqual([]);
                        expect(await workspace.read('.tasks/config.yml')).toBe(configBefore);
                        expect(await workspace.listTaskFiles()).toEqual([]);

                        // Hold a real request, not a mocked response, to verify busy dismissal.
                        await page.goto(`${server.url}/config`);
                        const trigger = page.getByRole('button', { name: 'New project', exact: true });
                        await trigger.click();
                        const dialog = page.getByRole('dialog', { name: 'Create a project', exact: true });
                        await dialog.getByLabel('Project name', { exact: true }).fill('Keyboard project');
                        await dialog.getByLabel('Project prefix', { exact: true }).fill('KEY');
                        let release!: () => void;
                        let reached!: () => void;
                        const held = new Promise<void>(resolve => { release = resolve; });
                        const entered = new Promise<void>(resolve => { reached = resolve; });
                        await page.route('**/api/projects/create', async route => {
                            reached();
                            await held;
                            await route.continue();
                        });
                        try {
                            await dialog.getByRole('button', { name: 'Create project', exact: true }).click();
                            await entered;
                            await page.keyboard.press('Escape');
                            await page.mouse.click(4, 4);
                            expect(await dialog.isVisible()).toBe(true);
                            expect(await dialog.getByRole('button', { name: 'Cancel', exact: true }).isDisabled()).toBe(true);
                            expect(await dialog.evaluate(el => el.contains(document.activeElement))).toBe(true);
                        } finally {
                            release();
                        }
                        await dialog.waitFor({ state: 'detached' });
                        expect(mutations).toEqual(['/api/projects/create']);
                        const projects = await fetch(`${server.url}/api/projects/list`).then(response => response.json());
                        expect(projects.data.projects.some((project: { prefix: string }) => project.prefix === 'KEY')).toBe(true);
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
