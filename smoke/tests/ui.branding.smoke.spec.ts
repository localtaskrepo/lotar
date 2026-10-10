import fs from 'fs-extra';
import http from 'node:http';
import path from 'node:path';
import { gunzipSync } from 'node:zlib';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function asset(url: string, encoding = 'identity'): Promise<{ status: number; headers: http.IncomingHttpHeaders; bytes: Buffer }> {
    return new Promise((resolve, reject) => {
        http.get(url, { agent: false, headers: { 'Accept-Encoding': encoding, Connection: 'close' } }, response => {
            const chunks: Buffer[] = [];
            response.on('data', chunk => chunks.push(Buffer.from(chunk)));
            response.on('error', reject);
            response.on('end', () => resolve({ status: response.statusCode || 0, headers: response.headers, bytes: Buffer.concat(chunks) }));
        }).on('error', reject);
    });
}

describe.concurrent('Embedded branding (DEV-100)', () => {
    it('serves SVG and binary ICO favicons without a filesystem fallback', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            const server = await startLotarServer(workspace, { env: { LOTAR_WEB_UI_EMBEDDED: '1' } });
            try {
                expect(await fs.pathExists(path.join(workspace.root, 'target/web'))).toBe(false);
                const index = await asset(server.url);
                expect(index.status).toBe(200);
                const tags = index.bytes.toString('utf8').match(/<link\b[^>]*rel="icon"[^>]*>/g) || [];
                expect(tags).toHaveLength(2);
                for (const [type, file] of [
                    ['image/x-icon', 'favicon.ico'], ['image/svg+xml', 'lotar-app-icon.svg'],
                ]) {
                    const tag = tags.find(tag => tag.includes(`type="${type}"`))!;
                    expect(tag).toBeTruthy();
                    const href = tag.match(/href="([^"]+)"/)![1]!;
                    expect(href).toMatch(/^\/assets\//);
                    const expected = await fs.readFile(path.resolve('view/assets/branding', file!));
                    for (const encoding of ['identity', 'gzip']) {
                        const response = await asset(new URL(href, server.url).href, encoding);
                        expect(response.status).toBe(200);
                        expect(response.headers['content-type']).toBe(type);
                        expect(response.headers['cache-control']).toContain('immutable');
                        const bytes = response.headers['content-encoding'] === 'gzip' ? gunzipSync(response.bytes) : response.bytes;
                        expect(bytes.equals(expected)).toBe(true);
                        if (file === 'favicon.ico') expect(response.headers['content-encoding']).toBeUndefined();
                    }
                }
                const readme = await fs.readFile(path.resolve('README.md'), 'utf8');
                expect(readme).toContain('srcset="view/assets/branding/lotar-logo-dark.svg"');
                expect(readme).toContain('src="view/assets/branding/lotar-logo.svg"');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    for (const viewport of [
        { name: 'desktop', width: 1280, height: 900 },
        { name: 'mobile', width: 390, height: 844 },
    ]) {
        it(`keeps branding readable across system and explicit themes on ${viewport.name}`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ seedFiles: {
                '.tasks/config.yml': 'default.project: BRAND\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
                '.tasks/BRAND/config.yml': 'project.name: Branding fixture\n',
            } });
            const artifacts = path.resolve('target/smoke-artifacts/branding');
            await fs.ensureDir(artifacts);
            try {
                const server = await startLotarServer(workspace, { env: { LOTAR_WEB_UI_EMBEDDED: '1' } });
                try {
                    await withPage(`${server.url}/preferences?project=BRAND`, async page => {
                        await page.setViewportSize(viewport);
                        const home = page.getByRole('link', { name: 'LoTaR home', exact: true });
                        const light = home.locator('.brand__logo--light');
                        const dark = home.locator('.brand__logo--dark');
                        for (const mode of [
                            { preference: 'System (follow OS)', os: 'light', logo: 'light' },
                            { preference: 'System (follow OS)', os: 'dark', logo: 'dark' },
                            { preference: 'Light', os: 'dark', logo: 'light' },
                            { preference: 'Dark', os: 'light', logo: 'dark' },
                        ] as const) {
                            await page.emulateMedia({ colorScheme: mode.os });
                            await page.getByRole('radio', { name: mode.preference, exact: true }).check();
                            await expect.poll(() => (mode.logo === 'light' ? light : dark).isVisible()).toBe(true);
                            expect(await (mode.logo === 'light' ? dark : light).isVisible()).toBe(false);
                            expect(await (mode.logo === 'light' ? light : dark).evaluate(img => img instanceof HTMLImageElement && img.complete && img.naturalWidth > 0)).toBe(true);
                            const box = await home.boundingBox();
                            expect(box!.x).toBeGreaterThanOrEqual(0);
                            expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width);
                            expect(await page.getByTestId('global-new-task').isVisible()).toBe(true);
                            if (mode.preference !== 'System (follow OS)') {
                                await page.locator('header.topbar').screenshot({ path: path.join(artifacts, `${viewport.name}-${mode.logo}.png`) });
                            }
                        }
                        await page.getByTestId('global-new-task').click();
                        const editor = page.getByRole('dialog', { name: 'Create task', exact: true });
                        await editor.waitFor({ state: 'visible' });
                        await expect.poll(() => editor.locator('select').first().inputValue()).toBe('BRAND');
                        await editor.getByRole('button', { name: 'Close panel', exact: true }).click();
                        await editor.waitFor({ state: 'detached' });
                        await home.click();
                        await expect.poll(() => new URL(page.url()).pathname).toBe('/');
                        expect(await workspace.listTaskFiles()).toEqual([]);
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
