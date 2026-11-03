import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-288: keyboard access, modal behaviour and local due dates found by the
// DEV-253 review, checked in a real browser.

function localDateKey(date: Date): string {
    const pad = (value: number) => String(value).padStart(2, '0');
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

describe.concurrent('UI keyboard access and local dates (DEV-288)', () => {
    it('keeps due dates local, Enter on board controls, a real Activity modal and keyboard calendar overflow', async ({
        expect,
    }) => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': 'default.project: KEY\nauto.set_reporter: false\n',
                '.tasks/KEY/config.yml': 'project.name: Keyboard fixture\n',
            },
        });
        try {
            const due = await workspace.addTask('Due on the fifth', { args: ['--due', '2030-11-05'] });
            await workspace.runLotar(['sprint', 'create', '--label', 'Keyboard sprint']);
            await workspace.runLotar(['sprint', 'add', due.id, '--sprint', '1']);
            // Today's calendar cell gets more tasks than it can show inline.
            const today = localDateKey(new Date());
            for (let index = 1; index <= 14; index += 1) {
                await workspace.addTask(`Busy day ${index}`, { args: ['--due', today] });
            }

            const server = await startLotarServer(workspace);
            try {
                await withBrowser({}, async (_context, browser) => {
                    // West of UTC, where UTC-midnight parsing shows the day before.
                    const context = await browser.newContext({
                        timezoneId: 'America/Los_Angeles',
                        viewport: { width: 1280, height: 900 },
                    });
                    try {
                        const page = await context.newPage();

                        // 1. Sprint due dates are the stored calendar day.
                        await page.goto(`${server.url}/sprints`);
                        const dueCell = page
                            .locator('.sprint-group:not(.backlog-group) td.task-cell--due_date')
                            .first();
                        await dueCell.waitFor({ state: 'visible', timeout: 15_000 });
                        const expected = await page.evaluate(() => new Date(2030, 10, 5).toLocaleDateString());
                        expect((await dueCell.textContent())?.trim()).toBe(expected);

                        // 2. Enter activates a button inside a board column.
                        await page.goto(`${server.url}/boards?project=KEY`);
                        const add = page.getByRole('button', { name: /^Add task in / }).first();
                        await add.waitFor({ state: 'visible', timeout: 15_000 });
                        await add.focus();
                        await page.keyboard.press('Enter');
                        await page
                            .getByRole('dialog', { name: 'Create task' })
                            .waitFor({ state: 'visible', timeout: 5_000 });

                        // 3. The Activity drawer behaves as the modal it announces.
                        await page.goto(`${server.url}/`);
                        const opener = page.getByRole('button', { name: 'Activity', exact: true });
                        await opener.waitFor({ state: 'visible', timeout: 15_000 });
                        await opener.click();
                        const drawer = page.getByRole('dialog', { name: 'Recent activity' });
                        await drawer.waitFor({ state: 'visible' });
                        expect(
                            await page.evaluate(() => document.activeElement?.getAttribute('aria-label') ?? null),
                        ).toBe('Close activity drawer');
                        for (const key of ['Shift+Tab', 'Shift+Tab', 'Shift+Tab', 'Tab', 'Tab']) {
                            await page.keyboard.press(key);
                            expect(await drawer.evaluate((el) => el.contains(document.activeElement))).toBe(true);
                        }
                        await page.keyboard.press('Escape');
                        await drawer.waitFor({ state: 'detached' });
                        expect(await opener.evaluate((el) => el === document.activeElement)).toBe(true);

                        // 4. The calendar overflow opens from the keyboard.
                        await page.goto(`${server.url}/calendar?project=KEY`);
                        const more = page.locator(`[data-date="${today}"] li.more`);
                        await more.waitFor({ state: 'visible', timeout: 15_000 });
                        expect(await more.getAttribute('role')).toBe('button');
                        await more.focus();
                        await page.keyboard.press('Enter');
                        const day = page.getByRole('dialog', { name: 'Day tasks' });
                        await day.waitFor({ state: 'visible', timeout: 5_000 });
                        expect(await day.locator('.calendar-day-dialog__item').count()).toBe(14);
                    } finally {
                        await context.close();
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
