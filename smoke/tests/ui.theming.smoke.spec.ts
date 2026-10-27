import { expect, type Page } from '@playwright/test';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-218: colour themes with light and dark modes, chosen per person in
// Preferences or for everyone in Config, plus repository logo, name and icon.

const ICON_SVG =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#2f7d4f"/></svg>';
const DARK_LOGO_SVG =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="30" fill="#e4e8df"/></svg>';

function rootState(page: Page) {
    return page.evaluate(() => ({
        theme: document.documentElement.dataset.themeName,
        mode: document.documentElement.dataset.mode,
        bg: getComputedStyle(document.documentElement).getPropertyValue('--color-bg').trim().toLowerCase(),
    }));
}

describe('UI theming and repository branding (DEV-218)', () => {
    it('applies personal and repository themes, logo, name and icon', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.addTask('Theme me');
            await workspace.write('assets/app-icon.svg', ICON_SVG);
            await workspace.write('assets/logo.svg', ICON_SVG);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/preferences`, async (page) => {
                    // Built-in default until someone picks a theme.
                    await expect.poll(async () => (await rootState(page)).theme).toBe('lotar');

                    const picker = page.getByTestId('theme-picker');
                    await picker.getByText('Ocean', { exact: true }).click();
                    await page.getByLabel('Light').check();
                    await expect.poll(() => rootState(page)).toEqual({ theme: 'ocean', mode: 'light', bg: '#f4f7fa' });
                    await page.getByLabel('Dark', { exact: true }).check();
                    await expect.poll(() => rootState(page)).toEqual({ theme: 'ocean', mode: 'dark', bg: '#1c232c' });
                    // The built-in logo follows the mode too.
                    await expect(page.locator('img.brand__logo--dark')).toBeVisible();
                    await expect(page.locator('img.brand__logo--light')).toBeHidden();

                    // Applied before the app starts, so a reload never flashes the default.
                    await page.reload({ waitUntil: 'commit' });
                    await page.waitForFunction(() => document.documentElement.dataset.themeName !== undefined);
                    expect((await rootState(page)).theme).toBe('ocean');

                    // Back to following the repository.
                    await page.getByTestId('theme-picker').getByText('Configured default').click();
                    // DEV-219: the palette button can be hidden; the shortcut still works.
                    await page.getByTestId('preferences-palette-button').uncheck();
                    await expect(page.getByTestId('command-palette-trigger')).toHaveCount(0);
                    await page.getByTestId('preferences-palette-button').check();
                    await expect(page.getByTestId('command-palette-trigger')).toBeVisible();
                    await page.getByLabel('Light').check();
                    await expect.poll(async () => (await rootState(page)).theme).toBe('lotar');

                    // Repository branding in Config.
                    await page.goto(`${server.url}/config`);
                    await page.locator('.config-nav__link', { hasText: 'Appearance' }).click();
                    const section = page.getByTestId('config-appearance');
                    await section.getByLabel('Default theme').selectOption('forest');
                    await section.getByLabel('Name').fill('Acme Tracker');
                    await section.getByLabel('Logo', { exact: true }).fill('assets/logo.svg');
                    await section.getByLabel('App icon').fill('assets/app-icon.svg');
                    await section.getByTestId('config-ui-logo-dark-file').setInputFiles({
                        name: 'Dark Mark.svg',
                        mimeType: 'image/svg+xml',
                        buffer: Buffer.from(DARK_LOGO_SVG),
                    });
                    await expect(section.getByLabel('Logo for dark mode')).toHaveValue(
                        '.tasks/@branding/logo-dark.svg',
                    );
                    await expect(section.getByAltText('Logo for dark mode preview')).toBeVisible();
                    await page.getByRole('button', { name: 'Save changes' }).click();

                    // Everyone following the repository default gets Forest.
                    await expect.poll(async () => (await rootState(page)).theme, { timeout: 15_000 }).toBe('forest');
                    const brand = page.locator('a.brand');
                    await expect(brand).toHaveAttribute('aria-label', 'Acme Tracker home');
                    await expect(brand.locator('.brand__name')).toHaveText('Acme Tracker');
                    await expect(brand.locator('img.brand__logo--light')).toHaveAttribute(
                        'src',
                        /\/api\/branding\/logo\?v=/,
                    );
                    await expect(brand.locator('img.brand__logo--dark')).toHaveAttribute(
                        'src',
                        /\/api\/branding\/logo-dark\?v=/,
                    );
                    await expect(page).toHaveTitle('Acme Tracker');
                    await expect
                        .poll(() =>
                            page.evaluate(() =>
                                Array.from(document.querySelectorAll<HTMLLinkElement>('link[rel~="icon"]')).map(
                                    (link) => new URL(link.href).pathname,
                                ),
                            ),
                        )
                        .toContain('/api/branding/icon');

                    const logo = await page.request.get(`${server.url}/api/branding/logo`);
                    expect(logo.status()).toBe(200);
                    expect(await logo.text()).toBe(ICON_SVG);
                });
            } finally {
                await server.stop();
            }
            const saved = await workspace.read('.tasks/config.yml');
            expect(saved).toContain('theme: forest');
            expect(saved).toContain('logo_dark: .tasks/@branding/logo-dark.svg');
        } finally {
            await workspace.dispose();
        }
    });
});
