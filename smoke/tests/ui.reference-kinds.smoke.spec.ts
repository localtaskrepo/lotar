import { expect, type Page } from '@playwright/test';
import fs from 'fs-extra';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-61 durable browser coverage for typed reference kinds:
// - managed attachments (`attachment` key) upload, view, download, and remove
//   through the managed `/api/attachments` routes only,
// - repository file references (`file` key) add, preview anchorless through
//   `/api/references/snippet?code=<path>`, and detach without ever hitting the
//   managed attachment routes or deleting the repository file,
// - same-basename managed and repository entries stay distinct kinds, a
//   wrong-kind attachment removal fails closed with 400 leaving the blob
//   alone, and file references into the managed store are denied,
// - per-project custom attachment stores resolve downloads for their project.
//
// Repository-only endpoints resolve the repo root by walking up from the tasks
// directory until a `.git` entry exists (backend `find_repo_root`). This suite
// reuses the backend's fixture strategy: the workspace is a disposable temp
// directory under `<repo>/target/smoke-scratch`, so the existing checkout's
// `.git` is discovered by the ancestor walk — no git command, no `.git`
// marker, no interception. Repository paths are expressed relative to the
// discovered repo root (`target/smoke-scratch/<tmp>/docs/notes.txt`), and the
// manual dialog path is used because file suggestions intentionally exclude
// `target/`. Each fixture owns its `.tasks` with custom REF/STORE projects;
// the real user `.tasks` and DEV project are never touched. If repo-root
// discovery cannot be achieved these tests fail as blocked — there is no mock
// fallback.

const MANAGED_FILENAME = 'notes.txt';
const MANAGED_CONTENT = 'managed attachment payload';
const SCRATCH_FILE = 'docs/notes.txt';
const REPO_FILE_LINES = 24;
const ANCHOR_LINE_TEXT = 'repo file line 03 ANCHOR_LINE_THREE';

const SPEC_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SPEC_DIR, '..', '..');
const SCRATCH_PARENT = path.join(REPO_ROOT, 'target', 'smoke-scratch');

// Pure filesystem walk mirroring backend `find_repo_root` (existence of a
// `.git` entry only). This is not a git command.
function findRepoRoot(start: string): string {
    let current = path.resolve(start);
    for (;;) {
        if (fs.existsSync(path.join(current, '.git'))) {
            return current;
        }
        const parent = path.dirname(current);
        if (parent === current) {
            throw new Error(`No repository root above ${start}`);
        }
        current = parent;
    }
}

// Workspace whose tasks directory resolves, via backend repo-root discovery,
// to the real checkout. Fails as blocked if ancestry is not available.
async function createRepoAnchoredWorkspace(): Promise<SmokeWorkspace> {
    const workspace = await SmokeWorkspace.create({
        name: 'lotar-ref-kinds-',
        parentDir: SCRATCH_PARENT,
    });
    try {
        const discovered = findRepoRoot(workspace.root);
        if (discovered !== REPO_ROOT) {
            throw new Error(
                `Repo-root discovery found ${discovered}, expected ${REPO_ROOT}; real-browser repository references are blocked in this environment`,
            );
        }
        return workspace;
    } catch (error) {
        await workspace.dispose();
        throw error;
    }
}

// Repository-relative display path of a file inside the scratch workspace
// (e.g. `target/smoke-scratch/<tmp>/docs/notes.txt`).
function repoRelPath(workspace: SmokeWorkspace, workspaceRelative: string): string {
    return path
        .relative(REPO_ROOT, path.join(workspace.root, workspaceRelative))
        .split(path.sep)
        .join('/');
}

function repoFileLine(line: number): string {
    return line === 3 ? ANCHOR_LINE_TEXT : `repo file line ${String(line).padStart(2, '0')}`;
}

async function seedRepoFile(workspace: SmokeWorkspace): Promise<void> {
    const lines = Array.from({ length: REPO_FILE_LINES }, (_, index) => repoFileLine(index + 1));
    await workspace.write(SCRATCH_FILE, `${lines.join('\n')}\n`);
}

interface ApiCall {
    method: string;
    path: string;
    body?: any;
}

function trackApiCalls(page: Page): ApiCall[] {
    const calls: ApiCall[] = [];
    page.on('request', (request) => {
        const url = new URL(request.url());
        if (!url.pathname.startsWith('/api/')) return;
        let body: any;
        try {
            body = request.postDataJSON();
        } catch {
            body = undefined;
        }
        calls.push({ method: request.method(), path: `${url.pathname}${url.search}`, body });
    });
    return calls;
}

function captureUploadStoredPath(page: Page): () => string {
    let storedPath = '';
    page.on('response', async (response) => {
        if (new URL(response.url()).pathname !== '/api/tasks/attachments/upload') return;
        if (!response.ok()) return;
        try {
            storedPath = String((await response.json())?.data?.stored_path ?? '');
        } catch {
            // Polled by the caller; ignore transient JSON failures.
        }
    });
    return () => storedPath;
}

async function dropFileOnPanel(page: Page, filename: string, content: string): Promise<void> {
    await page.evaluate(
        ({ filename: name, content: body }) => {
            const target = document.querySelector('.task-panel');
            if (!target) throw new Error('task panel not found');
            const dt = new DataTransfer();
            dt.items.add(new File([body], name, { type: 'text/plain' }));
            target.dispatchEvent(
                new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }),
            );
        },
        { filename, content },
    );
}

async function openTaskPanel(page: Page, title: string) {
    const row = page.locator('tr', { hasText: title });
    await row.waitFor({ timeout: 20_000 });
    await row.click();
    const panel = page.locator('.task-panel');
    await panel.waitFor({ state: 'visible', timeout: 15_000 });
    return panel;
}

async function openReferencesTab(
    page: Page,
    panel: Awaited<ReturnType<typeof openTaskPanel>>,
): Promise<void> {
    await panel.locator('.task-panel__activity .task-panel__tab', { hasText: 'References' }).click();
    await page.locator('.task-panel__references').waitFor({ state: 'visible', timeout: 15_000 });
}

async function addRepoFileReference(page: Page, repoPath: string): Promise<void> {
    const dialog = page.locator('[data-testid="references-add-dialog"]');
    await page.locator('[data-testid="references-add"]').click();
    await dialog.waitFor({ state: 'visible', timeout: 10_000 });
    await dialog.locator('[data-testid="references-add-tab-file"]').click();
    await dialog.locator('#task-panel-add-file-input').fill(repoPath);
    await dialog.getByRole('button', { name: 'Add file' }).click();
    await dialog.waitFor({ state: 'hidden', timeout: 15_000 });
}

describe('UI reference kinds (DEV-61)', () => {
    it('manages a managed attachment end to end: upload, view, download, remove', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'lotar-ref-kinds-' });
        try {
            await workspace.write('.tasks/REF/.seed', '');
            const task = await workspace.addTask('Managed attachment lifecycle', {
                args: ['--project=REF'],
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=REF`, async (page) => {
                    const apiCalls = trackApiCalls(page);
                    const storedPathGetter = captureUploadStoredPath(page);

                    const panel = await openTaskPanel(page, 'Managed attachment lifecycle');
                    await dropFileOnPanel(page, MANAGED_FILENAME, MANAGED_CONTENT);

                    const chip = panel.locator('.task-panel__attachments .task-panel__attachment-link');
                    await chip.waitFor({ timeout: 15_000 });
                    await expect(
                        panel.locator('.task-panel__attachments .task-panel__attachment-name'),
                    ).toHaveText(MANAGED_FILENAME);
                    const href = (await chip.getAttribute('href')) ?? '';
                    expect(href).toMatch(
                        /^\/api\/attachments\/h\/[0-9a-f]{32}\/notes\.txt\?project=REF$/,
                    );

                    await expect.poll(() => storedPathGetter(), { timeout: 15_000 }).toMatch(
                        /^notes\.[0-9a-f]{32}\.txt$/,
                    );
                    const storedPath = storedPathGetter();
                    const blobPath = path.join(workspace.tasksDir, '@attachments', storedPath);
                    await expect.poll(() => fs.pathExists(blobPath), { timeout: 15_000 }).toBe(true);
                    await expect
                        .poll(() => workspace.readTaskYaml(task.id), { timeout: 15_000 })
                        .toContain(`attachment: ${storedPath}`);

                    const uploadCall = apiCalls.find(
                        (call) => call.path === '/api/tasks/attachments/upload',
                    );
                    expect(uploadCall?.body).toMatchObject({
                        id: task.id,
                        filename: MANAGED_FILENAME,
                    });
                    expect(typeof uploadCall?.body?.content_base64).toBe('string');

                    const view = await page.request.get(new URL(href, server.url).toString());
                    expect(view.status()).toBe(200);
                    expect((await view.text()).trim()).toBe(MANAGED_CONTENT);
                    expect(view.headers()['content-type'] ?? '').toContain('text/plain');

                    const download = await page.request.get(
                        `${new URL(href, server.url).toString()}&download=1`,
                    );
                    expect(download.status()).toBe(200);
                    expect(
                        (download.headers()['content-disposition'] ?? '').startsWith('attachment;'),
                    ).toBe(true);

                    await panel
                        .locator('.task-panel__attachments button[aria-label="Remove attachment"]')
                        .click();
                    await chip.waitFor({ state: 'detached', timeout: 15_000 });
                    await expect.poll(() => fs.pathExists(blobPath), { timeout: 15_000 }).toBe(false);
                    await expect
                        .poll(
                            async () => (await workspace.readTaskYaml(task.id)).includes('attachment:'),
                            { timeout: 15_000 },
                        )
                        .toBe(false);

                    const removeCall = apiCalls.find(
                        (call) => call.path === '/api/tasks/attachments/remove',
                    );
                    expect(removeCall?.body).toMatchObject({
                        id: task.id,
                        stored_path: storedPath,
                    });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('repository file references add, preview anchorless, and detach without managed-store calls', async () => {
        const workspace = await createRepoAnchoredWorkspace();
        try {
            await seedRepoFile(workspace);
            const repoFile = repoRelPath(workspace, SCRATCH_FILE);
            await workspace.write('.tasks/REF/.seed', '');
            const task = await workspace.addTask('Repository file reference flow', {
                args: ['--project=REF'],
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=REF`, async (page) => {
                    const apiCalls = trackApiCalls(page);

                    const panel = await openTaskPanel(page, 'Repository file reference flow');
                    await openReferencesTab(page, panel);
                    await addRepoFileReference(page, repoFile);

                    const fileRow = page
                        .locator('.task-panel__reference-item')
                        .filter({ has: page.locator('button[aria-label="Remove repository file"]') });
                    await fileRow.waitFor({ timeout: 15_000 });
                    await expect(fileRow).toContainText(repoFile);
                    await expect(fileRow.locator('a')).toHaveCount(0);

                    const addCall = apiCalls.find(
                        (call) => call.path === '/api/tasks/references/file/add',
                    );
                    expect(addCall?.body).toMatchObject({ id: task.id, path: repoFile });

                    await expect
                        .poll(() => workspace.readTaskYaml(task.id), { timeout: 15_000 })
                        .toContain(`file: ${repoFile}`);

                    await fileRow.hover();
                    const preview = page.locator('.task-panel__reference-preview');
                    await preview.waitFor({ state: 'visible', timeout: 15_000 });
                    await expect(preview).toContainText('ANCHOR_LINE_THREE');
                    const snippetCall = [...apiCalls]
                        .reverse()
                        .find((call) => call.path.startsWith('/api/references/snippet'));
                    expect(snippetCall).toBeTruthy();
                    const snippetQuery = new URL(`http://local${snippetCall!.path}`).searchParams;
                    expect(snippetQuery.get('code')).toBe(repoFile);
                    expect(snippetQuery.get('before')).toBe('6');
                    expect(snippetQuery.get('after')).toBe('6');

                    const anchored = await page.request.get(
                        `${server.url}/api/references/snippet?code=${encodeURIComponent(
                            `${repoFile}#L3`,
                        )}&before=6&after=6`,
                    );
                    expect(anchored.status()).toBe(200);
                    const anchoredData = (await anchored.json())?.data ?? {};
                    expect(anchoredData.path).toBe(repoFile);
                    expect(anchoredData.highlight_start).toBe(3);
                    expect(anchoredData.highlight_end).toBe(3);

                    await page.mouse.move(0, 0);
                    await fileRow
                        .locator('button[aria-label="Remove repository file"]')
                        .click();
                    await fileRow.waitFor({ state: 'detached', timeout: 15_000 });

                    const removeCall = apiCalls.find(
                        (call) => call.path === '/api/tasks/references/file/remove',
                    );
                    expect(removeCall?.body).toMatchObject({ id: task.id, path: repoFile });

                    await expect
                        .poll(
                            async () =>
                                (await workspace.readTaskYaml(task.id)).includes(`file: ${repoFile}`),
                            { timeout: 15_000 },
                        )
                        .toBe(false);
                    const preserved = await workspace.read(SCRATCH_FILE);
                    expect(preserved).toContain(ANCHOR_LINE_TEXT);
                    expect(preserved.split('\n').length).toBeGreaterThanOrEqual(REPO_FILE_LINES);

                    expect(
                        apiCalls.filter(
                            (call) =>
                                call.path.startsWith('/api/attachments') ||
                                call.path.startsWith('/api/tasks/attachments'),
                        ),
                    ).toHaveLength(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('discriminates same-basename attachment and repository file references', async () => {
        const workspace = await createRepoAnchoredWorkspace();
        try {
            await seedRepoFile(workspace);
            const repoFile = repoRelPath(workspace, SCRATCH_FILE);
            await workspace.write('.tasks/REF/.seed', '');
            const task = await workspace.addTask('Same basename discrimination', {
                args: ['--project=REF'],
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=REF`, async (page) => {
                    const apiCalls = trackApiCalls(page);
                    const storedPathGetter = captureUploadStoredPath(page);

                    const panel = await openTaskPanel(page, 'Same basename discrimination');
                    await dropFileOnPanel(page, MANAGED_FILENAME, MANAGED_CONTENT);
                    await panel
                        .locator('.task-panel__attachments .task-panel__attachment-link')
                        .first()
                        .waitFor({ timeout: 15_000 });

                    await openReferencesTab(page, panel);
                    await addRepoFileReference(page, repoFile);

                    const attachmentRow = page
                        .locator('.task-panel__reference-item')
                        .filter({ has: page.locator('button[aria-label="Remove attachment"]') });
                    await expect(attachmentRow).toHaveCount(1);
                    await expect(attachmentRow.locator('a.task-panel__reference-link')).toHaveCount(1);
                    expect(
                        (await attachmentRow.locator('a').first().getAttribute('href')) ?? '',
                    ).toContain('/api/attachments/h/');

                    const fileRow = page
                        .locator('.task-panel__reference-item')
                        .filter({ has: page.locator('button[aria-label="Remove repository file"]') });
                    await expect(fileRow).toHaveCount(1);
                    await expect(fileRow.locator('a')).toHaveCount(0);
                    await expect(fileRow).toContainText(repoFile);

                    const chipNames = panel.locator('.task-panel__attachments .task-panel__attachment-name');
                    await expect(chipNames).toHaveCount(1);
                    await expect(chipNames).toHaveText(MANAGED_FILENAME);

                    await expect.poll(() => storedPathGetter(), { timeout: 15_000 }).toMatch(
                        /^notes\.[0-9a-f]{32}\.txt$/,
                    );
                    const storedPath = storedPathGetter();
                    const blobPath = path.join(workspace.tasksDir, '@attachments', storedPath);
                    await expect.poll(() => fs.pathExists(blobPath), { timeout: 15_000 }).toBe(true);
                    await expect
                        .poll(() => workspace.readTaskYaml(task.id), { timeout: 15_000 })
                        .toContain(`attachment: ${storedPath}`);
                    await expect
                        .poll(() => workspace.readTaskYaml(task.id), { timeout: 15_000 })
                        .toContain(`file: ${repoFile}`);

                    // Wrong-kind removal fails closed with 400 and leaves the
                    // managed blob untouched. The probe is a stored-file-shaped
                    // name (no path separators, parseable hash tag) sharing the
                    // display basename but not referenced by the task, so the
                    // typed-membership gate is what rejects it.
                    const wrongKind = await page.request.post(
                        `${server.url}/api/tasks/attachments/remove`,
                        {
                            data: {
                                id: task.id,
                                stored_path: `notes.${'0123456789abcdef'.repeat(2)}.txt`,
                            },
                        },
                    );
                    expect(wrongKind.status()).toBe(400);
                    const wrongKindBody = await wrongKind.json();
                    expect(String(wrongKindBody?.error?.message ?? '')).toContain(
                        'does not reference attachment',
                    );
                    expect(await fs.pathExists(blobPath)).toBe(true);

                    // The managed-store guard denies repository file references
                    // that target the attachments store, failing closed.
                    const storeGuard = await page.request.post(
                        `${server.url}/api/tasks/references/file/add`,
                        {
                            data: {
                                id: task.id,
                                path: repoRelPath(
                                    workspace,
                                    path.join('.tasks', '@attachments', storedPath),
                                ),
                            },
                        },
                    );
                    expect(storeGuard.status()).toBe(400);
                    const storeGuardBody = await storeGuard.json();
                    expect(String(storeGuardBody?.error?.message ?? '')).toContain(
                        'inside the managed attachments store',
                    );
                    expect(await fs.pathExists(blobPath)).toBe(true);
                    await expect(chipNames).toHaveCount(1);

                    await fileRow.locator('button[aria-label="Remove repository file"]').click();
                    await fileRow.waitFor({ state: 'detached', timeout: 15_000 });
                    await expect(chipNames).toHaveCount(1);
                    expect(await fs.pathExists(blobPath)).toBe(true);
                    const fileRemoveCall = apiCalls.find(
                        (call) => call.path === '/api/tasks/references/file/remove',
                    );
                    expect(fileRemoveCall?.body).toMatchObject({ id: task.id, path: repoFile });

                    await panel
                        .locator('.task-panel__attachments button[aria-label="Remove attachment"]')
                        .click();
                    await expect(chipNames).toHaveCount(0);
                    await expect.poll(() => fs.pathExists(blobPath), { timeout: 15_000 }).toBe(false);
                    const preserved = await workspace.read(SCRATCH_FILE);
                    expect(preserved).toContain(ANCHOR_LINE_TEXT);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('routes uploads and downloads through a per-project custom attachments store', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'lotar-ref-kinds-' });
        try {
            await workspace.write('.tasks/STORE/config.yml', 'attachments:\n  dir: "@vault"\n');
            const task = await workspace.addTask('Custom store task', {
                args: ['--project=STORE'],
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=STORE`, async (page) => {
                    const apiCalls = trackApiCalls(page);
                    const storedPathGetter = captureUploadStoredPath(page);

                    const panel = await openTaskPanel(page, 'Custom store task');
                    await dropFileOnPanel(page, 'vault-notes.txt', 'vault store payload');

                    const chip = panel.locator('.task-panel__attachments .task-panel__attachment-link');
                    await chip.waitFor({ timeout: 15_000 });
                    const href = (await chip.getAttribute('href')) ?? '';
                    expect(href).toMatch(
                        /^\/api\/attachments\/h\/[0-9a-f]{32}\/vault-notes\.txt\?project=STORE$/,
                    );

                    await expect.poll(() => storedPathGetter(), { timeout: 15_000 }).toMatch(
                        /^vault-notes\.[0-9a-f]{32}\.txt$/,
                    );
                    const storedPath = storedPathGetter();

                    const uploadCall = apiCalls.find(
                        (call) => call.path === '/api/tasks/attachments/upload',
                    );
                    expect(uploadCall?.body).toMatchObject({
                        id: task.id,
                        filename: 'vault-notes.txt',
                    });

                    const vaultPath = path.join(workspace.tasksDir, '@vault', storedPath);
                    await expect.poll(() => fs.pathExists(vaultPath), { timeout: 15_000 }).toBe(true);
                    expect(
                        await fs.pathExists(path.join(workspace.tasksDir, '@attachments', storedPath)),
                    ).toBe(false);

                    const view = await page.request.get(new URL(href, server.url).toString());
                    expect(view.status()).toBe(200);
                    expect((await view.text()).trim()).toBe('vault store payload');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
