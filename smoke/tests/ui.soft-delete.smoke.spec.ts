import fs from 'fs-extra';
import path from 'node:path';
import type { Page } from '@playwright/test';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { callTool, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

interface TaskSnapshot {
    id: string;
    title: string;
    modified: string;
    deleted_at?: string;
    history?: unknown[];
}

interface TaskPage {
    total: number;
    tasks?: TaskSnapshot[];
}

async function api<T>(url: string, route: string, body?: Record<string, unknown>): Promise<T> {
    const response = await fetch(`${url}${route}`, {
        method: body ? 'POST' : 'GET',
        headers: body ? { 'Content-Type': 'application/json' } : {},
        body: body ? JSON.stringify(body) : undefined,
    });
    const text = await response.text();
    if (!response.ok) throw new Error(`${route}: ${response.status} ${text}`);
    return (JSON.parse(text) as { data: T }).data;
}

async function withLiveTaskPage(
    url: string,
    expect: typeof import('vitest').expect,
    callback: (page: Page) => Promise<void>,
): Promise<void> {
    await withBrowser({}, async context => {
        const page = await context.newPage();
        await page.addInitScript(() => {
            const state = window as Window & { taskEventsReady?: boolean; taskEvents?: string[] };
            state.taskEvents = [];
            const NativeEventSource = window.EventSource;
            window.EventSource = class extends NativeEventSource {
                constructor(url: string | URL, options?: EventSourceInit) {
                    super(url, options);
                    if (new URL(String(url), window.location.href).searchParams.get('kinds')?.includes('task_deleted')) {
                        this.addEventListener('ready', () => { state.taskEventsReady = true; });
                        for (const kind of ['task_created', 'task_updated', 'task_deleted']) {
                            this.addEventListener(kind, event => {
                                const id = JSON.parse((event as MessageEvent).data).id;
                                state.taskEvents!.push(`${kind}:${id}`);
                            });
                        }
                    }
                }
            };
        });
        try {
            await page.goto(url, { waitUntil: 'domcontentloaded' });
            await expect.poll(() => page.evaluate(() => (
                window as Window & { taskEventsReady?: boolean }
            ).taskEventsReady)).toBe(true);
            await callback(page);
        } finally {
            await page.close();
        }
    });
}

describe.concurrent('DEV-92 soft deletion (actual backend/browser)', () => {
    it('shares soft-delete and restore semantics between CLI and initialized MCP', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'cli-mcp-deletion-', env: { LOTAR_DEFAULT_REPORTER: 'Smoke' } });
        try {
            const task = await workspace.addTask('Cross-surface recoverable task', { args: ['--project=LIFE'] });
            const before = parse(await workspace.readTaskYaml(task.id)) as Record<string, unknown>;
            before.modified = before.created;
            await fs.writeFile(task.filePath, stringify(before));
            await workspace.runLotar(['--format', 'json', 'task', 'delete', task.id, '--yes']);
            const deleted = parse(await workspace.readTaskYaml(task.id)) as Record<string, unknown>;
            expect(deleted.deleted_at).toEqual(expect.any(String));
            expect(deleted.modified).toBe(before.modified);
            const listed = await workspace.runLotar(['--format', 'json', 'list', '--project=LIFE', '--deleted']);
            const deletedRows = JSON.parse(String(listed.stdout)).tasks as TaskSnapshot[];
            expect(deletedRows.map(entry => entry.id)).toEqual([task.id]);
            expect(deletedRows[0].deleted_at).toBe(deleted.deleted_at);
            expect(deletedRows[0].modified).toBe(before.modified);
            await withMcpClient(workspace, async client => {
                const hidden = await callTool(client, 'hidden', 'task_get', { id: task.id });
                expect(hidden.error).toBeUndefined();
                expect(hidden.result?.isError).toBe(true);
                const inspect = await callTool(client, 'inspect', 'task_get', { id: task.id, include_deleted: true });
                expect(inspect.error).toBeUndefined();
                expect(extractToolPayload(inspect)).toMatchObject({ id: task.id, deleted_at: deleted.deleted_at, modified: before.modified });
                const noOp = await callTool(client, 'delete-again', 'task_delete', { id: task.id });
                expect(noOp.error).toBeUndefined();
                expect(extractToolPayload(noOp)).toMatchObject({ deleted: true, hard: false, warnings: [] });
                const restored = await callTool(client, 'restore', 'task_restore', { id: task.id });
                expect(restored.error).toBeUndefined();
                expect(restored.result?.isError).not.toBe(true);
                const snapshot = extractToolPayload(restored) as TaskSnapshot;
                expect(snapshot.id).toBe(task.id);
                expect(snapshot.modified).toBe(before.modified);
                expect(snapshot.deleted_at).toBeUndefined();
            });
            await workspace.runLotar(['--format', 'json', 'task', 'delete', task.id, '--hard', '--yes']);
            expect(await fs.pathExists(task.filePath)).toBe(false);
        } finally {
            await workspace.dispose();
        }
    });

    it('hides a live task, opens its read-only deleted detail, and restores without modified churn', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'soft-delete-ui-', env: { LOTAR_DEFAULT_REPORTER: 'Smoke' } });
        try {
            const task = await workspace.addTask('Recoverable task content', { args: ['--project=SOFT', '--reporter=Smoke'] });
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                const before = await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${task.id}`);
                await withLiveTaskPage(`${server.url}/?project=SOFT`, expect, async page => {
                    const title = page.locator('.table-wrap').getByText('Recoverable task content', { exact: true });
                    await expect.poll(() => title.count()).toBe(1);
                    await title.click();
                    const livePanel = page.locator('.task-panel');
                    await expect.poll(() => livePanel.isVisible()).toBe(true);
                    expect(await livePanel.locator('fieldset.task-panel__form-fieldset').getAttribute('disabled')).toBeNull();
                    const deletion = await api<{ deleted: boolean; hard: boolean; warnings: string[] }>(
                        server.url, '/api/tasks/delete', { id: task.id },
                    );
                    expect(deletion).toMatchObject({ deleted: true, hard: false, warnings: [] });
                    await expect.poll(() => title.count()).toBe(0);
                    await expect.poll(() => livePanel.getByTestId('task-deleted-banner').isVisible()).toBe(true);
                    expect(await livePanel.locator('fieldset.task-panel__form-fieldset').getAttribute('disabled')).toBe('');
                    await page.press('body', 'Escape');
                    await expect.poll(() => livePanel.count()).toBe(0);
                    expect((await fetch(`${server.url}/api/tasks/get?id=${task.id}`)).status).toBe(404);

                    const deleted = await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${task.id}&include_deleted=true`);
                    expect(deleted.deleted_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
                    expect(deleted.modified).toBe(before.modified);
                    const bytes = await fs.readFile(task.filePath, 'utf8');
                    expect(await api(server.url, '/api/tasks/delete', { id: task.id })).toMatchObject({ deleted: true, hard: false });
                    expect(await fs.readFile(task.filePath, 'utf8')).toBe(bytes);

                    await page.getByTestId('filter-toggle').click();
                    const visibility = page.locator('[data-testid="task-deletion-filter"]');
                    await visibility.selectOption('deleted');
                    await expect.poll(() => title.count()).toBe(1);
                    await title.click();
                    const panel = page.locator('.task-panel');
                    const restore = panel.locator('[data-testid="task-restore"]');
                    await expect.poll(() => restore.isVisible()).toBe(true);
                    expect(await panel.locator('fieldset.task-panel__form-fieldset').getAttribute('disabled')).toBe('');
                    expect(await panel.locator('fieldset.task-panel__form-fieldset input').first().isDisabled()).toBe(true);
                    const rejectedEdit = await fetch(`${server.url}/api/tasks/update`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ id: task.id, title: 'Must not modify a deleted task' }),
                    });
                    expect(rejectedEdit.status).toBeGreaterThanOrEqual(400);
                    expect(rejectedEdit.status).toBeLessThan(500);
                    expect(await panel.locator('button[aria-label="Remove attachment"]').count()).toBe(0);
                    await page.screenshot({ path: path.join(process.cwd(), 'target/smoke-artifacts/soft-delete/trash-desktop.png'), animations: 'disabled' });
                    await restore.click();
                    await expect.poll(async () => (
                        await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${task.id}&include_deleted=true`)
                    ).deleted_at).toBeUndefined();
                    const restored = await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${task.id}`);
                    expect(restored.modified).toBe(before.modified);
                    expect(restored.title).toBe(before.title);
                    expect(restored.history?.length).toBe((before.history?.length ?? 0) + 2);
                    await expect.poll(() => restore.isVisible()).toBe(false);
                    await page.press('body', 'Escape');
                    await expect.poll(() => panel.count()).toBe(0);
                    await visibility.selectOption('active');
                    await expect.poll(() => title.count()).toBe(1);
                    await page.setViewportSize({ width: 375, height: 812 });
                    const box = await visibility.boundingBox();
                    expect(box).not.toBeNull();
                    expect(box!.x).toBeGreaterThanOrEqual(0);
                    expect(box!.x + box!.width).toBeLessThanOrEqual(375);
                    await page.screenshot({ path: path.join(process.cwd(), 'target/smoke-artifacts/soft-delete/visibility-mobile.png'), animations: 'disabled', mask: [page.locator('.session-touch > span:nth-child(3)')] });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('warns about retained attachments and incoming relationships on hard deletion', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'hard-delete-warnings-', env: { LOTAR_DEFAULT_REPORTER: 'Smoke' } });
        try {
            const target = await workspace.addTask('Permanently removed target', { args: ['--project=KEEP'] });
            const dependent = await workspace.addTask('Retained dependent', { args: ['--project=KEEP'] });
            const server = await startLotarServer(workspace);
            try {
                await api(server.url, '/api/tasks/update', {
                    id: dependent.id, relationships: { depends_on: [target.id] },
                });
                const attachment = await api<{ stored_path: string }>(server.url, '/api/tasks/attachments/upload', {
                    id: target.id, filename: 'retained.txt', content_base64: Buffer.from('keep this attachment').toString('base64'),
                });
                await api(server.url, '/api/tasks/delete', { id: target.id });
                const removed = await api<{ deleted: boolean; hard: boolean; warnings: string[] }>(
                    server.url, '/api/tasks/delete', { id: target.id, hard: true },
                );
                expect(removed).toMatchObject({ deleted: true, hard: true });
                expect(removed.warnings.join('\n')).toContain(attachment.stored_path);
                expect(removed.warnings.join('\n')).toContain(dependent.id);
                expect(await fs.pathExists(target.filePath)).toBe(false);
                expect(await fs.pathExists(path.join(workspace.tasksDir, '@attachments', attachment.stored_path))).toBe(true);
                const downloaded = await fetch(`${server.url}/api/attachments/get?path=${encodeURIComponent(attachment.stored_path)}&project=KEEP`);
                expect(downloaded.status).toBe(200);
                expect(await downloaded.text()).toBe('keep this attachment');
                expect((await fetch(`${server.url}/api/tasks/get?id=${target.id}&include_deleted=true`)).status).toBe(404);
                const survivor = await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${dependent.id}`);
                expect(survivor.id).toBe(dependent.id);
                expect(await api<TaskPage>(server.url, '/api/tasks/list?project=KEEP')).toMatchObject({ total: 1 });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps a shared blob reachable through a soft-deleted task', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'deleted-attachment-reachability-', env: { LOTAR_DEFAULT_REPORTER: 'Smoke' } });
        try {
            const first = await workspace.addTask('Deleted attachment owner', { args: ['--project=BLOB'] });
            const second = await workspace.addTask('Active attachment owner', { args: ['--project=BLOB'] });
            const server = await startLotarServer(workspace);
            try {
                const payload = { filename: 'shared.txt', content_base64: Buffer.from('shared content').toString('base64') };
                const uploaded = await api<{ stored_path: string }>(server.url, '/api/tasks/attachments/upload', { id: first.id, ...payload });
                const duplicate = await api<{ stored_path: string }>(server.url, '/api/tasks/attachments/upload', { id: second.id, ...payload });
                expect(duplicate.stored_path).toBe(uploaded.stored_path);
                await api(server.url, '/api/tasks/delete', { id: first.id });
                const detached = await api<{ deleted: boolean; still_referenced: boolean }>(server.url, '/api/tasks/attachments/remove', {
                    id: second.id, stored_path: uploaded.stored_path,
                });
                expect(detached).toMatchObject({ deleted: false, still_referenced: true });
                await api(server.url, '/api/tasks/restore', { id: first.id });
                const download = await fetch(`${server.url}/api/attachments/get?path=${encodeURIComponent(uploaded.stored_path)}&project=BLOB`);
                expect(download.status).toBe(200);
                expect(await download.text()).toBe('shared content');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('reconciles external soft deletion and physical removal without task-error notifications', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'external-deletion-', env: { LOTAR_DEFAULT_REPORTER: 'Smoke' } });
        try {
            const task = await workspace.addTask('Externally deleted task', { args: ['--project=EXTD', '--reporter=Smoke'] });
            const original = parse(await fs.readFile(task.filePath, 'utf8')) as Record<string, unknown>;
            original.modified = original.created;
            await fs.writeFile(task.filePath, stringify(original));
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1', LOTAR_ENABLE_POLL_WATCH: '1' } });
            try {
                await withLiveTaskPage(`${server.url}/?project=EXTD`, expect, async page => {
                    const responses: string[] = [];
                    page.on('response', response => {
                        if (response.url().includes('/api/tasks/')) responses.push(`${response.status()} ${response.url()}`);
                    });
                    const title = page.locator('.table-wrap').getByText('Externally deleted task', { exact: true });
                    await expect.poll(() => title.count()).toBe(1);
                    const data = parse(await fs.readFile(task.filePath, 'utf8')) as Record<string, unknown>;
                    const modified = data.modified;
                    data.deleted_at = '2026-10-01T00:00:00+00:00';
                    await fs.writeFile(task.filePath, stringify(data));
                    await expect.poll(() => title.count()).toBe(0);
                    expect(await page.locator('.toast', { hasText: /Task .*not found|Failed to parse/ }).count()).toBe(0);
                    await page.getByTestId('filter-toggle').click();
                    await page.locator('[data-testid="task-deletion-filter"]').selectOption('deleted');
                    await expect.poll(() => title.count()).toBe(1);
                    expect((await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${task.id}&include_deleted=true`)).modified).toBe(modified);
                    await fs.remove(task.filePath);
                    try {
                        await expect.poll(() => title.count()).toBe(0);
                    } catch (error) {
                        await fs.outputJson(path.join(process.cwd(), 'target/dev92-20261006/external-delete-diagnostics.json'), {
                            events: await page.evaluate(() => (window as Window & { taskEvents?: string[] }).taskEvents),
                            responses,
                            rows: await page.locator('.table-wrap').innerText(),
                            serverList: await api<TaskPage>(server.url, '/api/tasks/list?project=EXTD&deletion=all'),
                        });
                        throw error;
                    }
                    expect(await api<TaskPage>(server.url, '/api/tasks/list?project=EXTD&deletion=all')).toMatchObject({ total: 0 });
                    const replacement = await workspace.addTask('Replacement after physical deletion', { args: ['--project=EXTD', '--reporter=Smoke'] });
                    expect(replacement.id).toBe(task.id);
                    expect((await api<TaskSnapshot>(server.url, `/api/tasks/get?id=${task.id}`)).title).toBe('Replacement after physical deletion');
                    await page.locator('[data-testid="task-deletion-filter"]').selectOption('active');
                    await expect.poll(() => page.getByText('Replacement after physical deletion', { exact: true }).count()).toBe(1);
                    expect(await title.count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
