import fs from 'fs-extra';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('UI SSE reactivity', () => {
    it('board view moves a card when task status changes on disk', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.initGit();
            await workspace.addTask('Board SSE task');
            await workspace.commitAll('seed');

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(server.url + '/boards', async (page) => {
                    // Wait for the board to render with the task in the Todo column
                    await page.waitForSelector('[data-status="Todo"] article', { timeout: 15_000 });
                    const todoCards = await page.$$('[data-status="Todo"] article');
                    expect(todoCards.length).toBe(1);

                    // Change the task status on disk
                    const taskFiles = await workspace.listTaskFiles();
                    expect(taskFiles.length).toBe(1);
                    const yaml = parse(await fs.readFile(taskFiles[0], 'utf8')) as Record<string, unknown>;
                    yaml.status = 'InProgress';
                    await fs.writeFile(taskFiles[0], stringify(yaml));

                    // Wait for the card to appear in the InProgress column
                    await page.waitForSelector('[data-status="InProgress"] article', { timeout: 15_000 });

                    // Wait for the Todo column to be empty (card fully moved)
                    await page.waitForFunction(
                        () => document.querySelectorAll('[data-status="Todo"] article').length === 0,
                        { timeout: 15_000 },
                    );
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('task list view reflects status change from disk', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.initGit();
            await workspace.addTask('SSE List Task');
            await workspace.commitAll('seed');

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(server.url, async (page) => {
                    // Wait for the task row with status "Todo"
                    await page.waitForSelector('td:has-text("Todo")', { timeout: 15_000 });

                    // Change the status on disk
                    const taskFiles = await workspace.listTaskFiles();
                    const yaml = parse(await fs.readFile(taskFiles[0], 'utf8')) as Record<string, unknown>;
                    yaml.status = 'Done';
                    await fs.writeFile(taskFiles[0], stringify(yaml));

                    // Wait for the status cell to change to "Done"
                    await page.waitForSelector('td:has-text("Done")', { timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('shows error toast when task file has invalid YAML', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.initGit();
            await workspace.addTask('Error Toast Task');
            await workspace.commitAll('seed');

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(server.url, async (page) => {
                    // Wait for the task to appear
                    await page.waitForSelector('text=Error Toast Task', { timeout: 15_000 });

                    // Corrupt the YAML file
                    const taskFiles = await workspace.listTaskFiles();
                    await fs.writeFile(taskFiles[0], 'title: broken\n  bad indent: here\n: orphan\n');

                    // Wait for the error toast to appear
                    await page.waitForSelector('.toast-card', { timeout: 15_000 });
                    const toastText = await page.textContent('.toast-card');
                    expect(toastText).toContain('File Error');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('board view applies transition classes during card movement', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.initGit();
            await workspace.addTask('Animated card');
            await workspace.commitAll('seed');

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(server.url + '/boards', async (page) => {
                    await page.waitForSelector('[data-status="Todo"] article', { timeout: 15_000 });

                    // Set up class observer
                    await page.evaluate(() => {
                        (window as any).__transitionSeen = false;
                        const board = document.querySelector('.board');
                        if (!board) return;
                        new MutationObserver((mutations) => {
                            for (const m of mutations) {
                                if (m.type === 'attributes' && m.attributeName === 'class') {
                                    const cls = (m.target as HTMLElement).className || '';
                                    if (cls.includes('task-list-enter') || cls.includes('task-list-leave')) {
                                        (window as any).__transitionSeen = true;
                                    }
                                }
                            }
                        }).observe(board, { attributes: true, subtree: true });
                    });

                    // Change status on disk
                    const taskFiles = await workspace.listTaskFiles();
                    const yaml = parse(await fs.readFile(taskFiles[0], 'utf8')) as Record<string, unknown>;
                    yaml.status = 'Done';
                    await fs.writeFile(taskFiles[0], stringify(yaml));

                    // Wait for the card to move to Done
                    await page.waitForSelector('[data-status="Done"] article', { timeout: 15_000 });

                    // Verify transition classes were applied
                    const transitionSeen = await page.evaluate(() => (window as any).__transitionSeen);
                    expect(transitionSeen).toBe(true);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
