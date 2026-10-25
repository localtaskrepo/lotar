import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-186: every page uses the shared header. Title-row controls line up with
// the title, second-row controls share the control height and label style,
// and nothing spills out of the page card on desktop or phone widths.
const PAGES = [
    '/',
    '/sprints',
    '/boards',
    '/calendar',
    '/dependencies',
    '/insights',
    '/agents',
    '/automations',
    '/sync',
    '/scan',
    '/config',
    '/preferences',
];

describe.concurrent('Shared page header layout (DEV-186)', () => {
    for (const width of [1280, 390]) {
        it(`keeps header controls aligned and contained at ${width}px`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create();
            try {
                await workspace.addTask('Header layout task');
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/`, async (page) => {
                        await page.setViewportSize({ width, height: 800 });
                        for (const path of PAGES) {
                            await page.goto(`${server.url}${path}`);
                            await page.locator('.app-page-header__title').waitFor();
                            await page.waitForLoadState('networkidle');
                            const report = await page.evaluate(() => {
                                const header = document.querySelector('.app-page-header')!;
                                const box = header.getBoundingClientRect();
                                const title = header.querySelector('.app-page-header__title')!.getBoundingClientRect();
                                const firstControl = header.querySelector('.app-page-header__controls > *');
                                const controls = [
                                    ...document.querySelectorAll(
                                        '.filter-bar__actions select, .filter-bar__actions button, .app-page-header__toolbar select, .app-page-header__toolbar button',
                                    ),
                                ] as HTMLElement[];
                                const outside = [
                                    ...document.querySelectorAll('.app-page-header *, .filter-bar__actions *'),
                                ].filter((el) => {
                                    const rect = el.getBoundingClientRect();
                                    return rect.width > 0 && (rect.left < box.left - 1 || rect.right > box.right + 1);
                                });
                                return {
                                    titleTop: Math.round(title.top),
                                    controlTop: firstControl
                                        ? Math.round(firstControl.getBoundingClientRect().top)
                                        : null,
                                    heights: [
                                        ...new Set(controls.map((el) => Math.round(el.getBoundingClientRect().height))),
                                    ],
                                    labelColors: [
                                        ...new Set(
                                            [...document.querySelectorAll('.toolbar-field__label')].map(
                                                (el) => getComputedStyle(el).color,
                                            ),
                                        ),
                                    ],
                                    outside: outside.length,
                                };
                            });
                            expect(report.outside, `${path} overflow`).toBe(0);
                            expect(
                                report.heights.every((h) => h === 36),
                                `${path} heights ${report.heights}`,
                            ).toBe(true);
                            expect(report.labelColors.length, `${path} label colours`).toBeLessThanOrEqual(1);
                            if (width > 640 && report.controlTop !== null) {
                                expect(report.controlTop, `${path} control alignment`).toBe(report.titleTop);
                            }
                        }
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
