import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-549: repository files named in a description open in a read-only viewer:
// code highlighted at the referenced line, markdown rendered with its own
// relative links, and git-ignored files out of reach.
describe('UI file viewer (DEV-549)', () => {
    it('opens files named in a description at the referenced line', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            // Without a repository of its own the workspace would resolve to lotar's.
            await workspace.initGit();
            await workspace.bootstrap({
                '.gitignore': '.env\n',
                '.env': 'SECRET=1\n',
                'docs/guide.md': '# Guide\n\nThe entry point is [the app](../src/app.rs).\n',
                'src/app.rs': 'use std::io;\n\n/// Starts the app.\nfn main() {\n    println!("hello");\n}\n',
            });
            const task = await workspace.addTask('Read the repository files', {
                args: ['--description', 'See `docs/guide.md` and src/app.rs:4. Not .env or `missing.md`.'],
            });
            const server = await startLotarServer(workspace);
            try {
                const secret = await fetch(`${server.url}/api/files/content?path=.env`);
                expect(secret.status).toBe(404);

                await withPage(`${server.url}/?taskPanel=${task.id}`, async (page) => {
                    const links = page.locator('.task-panel__description-display a.file-ref');
                    await expect
                        .poll(() => links.allTextContents(), { timeout: 15_000 })
                        .toEqual(['docs/guide.md', 'src/app.rs:4']);

                    await links.filter({ hasText: 'src/app.rs:4' }).click();
                    const viewer = page.getByRole('dialog', { name: 'File src/app.rs' });
                    await viewer.waitFor({ timeout: 15_000 });
                    const selected = viewer.locator('.file-viewer__line--selected');
                    await expect.poll(() => selected.getAttribute('data-line'), { timeout: 15_000 }).toBe('4');
                    await expect
                        .poll(() => viewer.locator('.hljs-keyword').count(), { timeout: 15_000 })
                        .toBeGreaterThan(0);
                    expect(new URL(page.url()).searchParams.get('file')).toBe('src/app.rs:4');
                    // The list behind keeps its rows: `file` is not a filter.
                    await expect
                        .poll(() => page.locator('tr', { hasText: 'Read the repository files' }).count())
                        .toBe(1);

                    // Escape closes the viewer and leaves the task open.
                    await page.keyboard.press('Escape');
                    await viewer.waitFor({ state: 'hidden' });
                    expect(new URL(page.url()).searchParams.get('file')).toBeNull();
                    await expect.poll(() => links.count()).toBe(2);

                    // Markdown renders, and its relative links open in the viewer too.
                    await links.filter({ hasText: 'docs/guide.md' }).click();
                    const guide = page.getByRole('dialog', { name: 'File docs/guide.md' });
                    await guide.getByRole('heading', { name: 'Guide' }).waitFor({ timeout: 15_000 });
                    await guide.getByRole('link', { name: 'the app' }).click();
                    await page.getByRole('dialog', { name: 'File src/app.rs' }).waitFor({ timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
