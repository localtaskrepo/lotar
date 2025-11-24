import type { Page } from '@playwright/test';
import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const DAY_IN_MS = 24 * 60 * 60 * 1_000;
const BASE_SPRINT_START = Date.parse('2024-01-01T09:00:00.000Z');

type VelocityEntriesPayload = {
    entries?: Array<{ summary?: { id?: number | null } | null }>;
};

type SprintSeedOptions = {
    startIndex?: number;
};

function sprintWindowForIndex(index: number) {
    const start = BASE_SPRINT_START + index * 7 * DAY_IN_MS;
    return {
        startsAt: new Date(start).toISOString(),
        endsAt: new Date(start + 7 * DAY_IN_MS).toISOString(),
    };
}

function normalizeVelocityPayload(value: unknown): VelocityEntriesPayload {
    if (value && typeof value === 'object') {
        const envelope = value as { data?: unknown };
        if (envelope.data && typeof envelope.data === 'object') {
            return envelope.data as VelocityEntriesPayload;
        }
        return value as VelocityEntriesPayload;
    }
    return { entries: [] };
}

function velocityEntryIds(payload: VelocityEntriesPayload): number[] {
    return (payload.entries ?? [])
        .map((entry) => entry?.summary?.id ?? null)
        .filter((id): id is number => typeof id === 'number');
}

async function createSequentialSprints(workspace: SmokeWorkspace, count: number, options: SprintSeedOptions = {}) {
    const startIndex = options.startIndex ?? 0;

    for (let offset = 0; offset < count; offset += 1) {
        const sprintIndex = startIndex + offset;
        const sprintId = sprintIndex + 1;
        const window = sprintWindowForIndex(sprintIndex);

        await workspace.runLotar([
            'sprint',
            'create',
            '--label',
            `Analytics Sprint ${sprintId}`,
            '--starts-at',
            window.startsAt,
            '--ends-at',
            window.endsAt,
        ]);
        await workspace.runLotar(['sprint', 'start', String(sprintId), '--at', window.startsAt]);
    }
}

async function navigateToSprints(page: Page) {
    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
    await page.click('a[href="/sprints"]');
    await page.waitForSelector('text=Sprints', { timeout: 15_000 });
}

async function openSprintInsights(page: Page, sprintId: number) {
    await page.waitForSelector(`[data-sprint-id="${sprintId}"] button:has-text("Insights")`, { timeout: 15_000 });
    await page.click(`[data-sprint-id="${sprintId}"] button:has-text("Insights")`);
    await page.waitForSelector('.sprint-analytics__overlay', { timeout: 15_000 });
}

async function readVelocityEntriesCount(page: Page) {
    await page.waitForSelector('.velocity-trend__summary div:nth-child(3) .velocity-trend__value', {
        timeout: 15_000,
    });
    const value = await page.textContent('.velocity-trend__summary div:nth-child(3) .velocity-trend__value');
    return value?.trim() ?? null;
}

describe.concurrent('UI sprint analytics smoke scenarios', () => {
    it('shows the current sprint plus the previous three and ignores future sprints', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await createSequentialSprints(workspace, 5);

            const futureWindow = sprintWindowForIndex(5);
            await workspace.runLotar([
                'sprint',
                'create',
                '--label',
                'Analytics Sprint 6',
                '--starts-at',
                futureWindow.startsAt,
                '--ends-at',
                futureWindow.endsAt,
            ]);

            const cliVelocity = await workspace.runLotar(['sprint', 'velocity', '--include-active', '--format', 'json']);
            const cliPayload = normalizeVelocityPayload(JSON.parse(String(cliVelocity.stdout ?? '{}')));
            const cliEntryIds = velocityEntryIds(cliPayload);
            expect(cliEntryIds).toEqual(expect.arrayContaining([2, 3, 4, 5]));

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await navigateToSprints(page);

                    await openSprintInsights(page, 5);
                    await page.click('.sprint-analytics__tabs button:has-text("Velocity")');
                    await page.waitForSelector('.velocity-trend[data-visible-sprint-ids]', { timeout: 15_000 });

                    expect(await readVelocityEntriesCount(page)).toBe('4');

                    const visibleAttr = await page.getAttribute('.velocity-trend', 'data-visible-sprint-ids');
                    expect(visibleAttr?.split(',')).toEqual(['2', '3', '4', '5']);

                    const velocityPayload = await page.evaluate<VelocityEntriesPayload>(async () => {
                        const response = await fetch('/api/sprints/velocity?limit=8&include_active=1&metric=points', {
                            headers: { Accept: 'application/json' },
                        });
                        if (!response.ok) {
                            throw new Error(`Velocity API responded with ${response.status}`);
                        }
                        const payload = await response.json();
                        if (payload && typeof payload === 'object' && 'data' in payload) {
                            const data = (payload as { data?: VelocityEntriesPayload }).data;
                            if (data && typeof data === 'object') {
                                return data as VelocityEntriesPayload;
                            }
                        }
                        return payload as VelocityEntriesPayload;
                    });

                    const entryIds = velocityEntryIds(velocityPayload);
                    expect(entryIds).toEqual(expect.arrayContaining([2, 3, 4, 5]));
                    // The backend still returns pending sprints; the UI filters them out before rendering.
                    expect(entryIds).toContain(6);

                    await page.click('.sprint-analytics__close');
                    await page.waitForSelector('.sprint-analytics__overlay', { state: 'detached', timeout: 10_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('renders populated health metrics when summary data is available', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await createSequentialSprints(workspace, 1);

            const featureTask = await workspace.addTask('Health tab feature task', { args: ['--project', 'WEB'] });
            await workspace.runLotar(['sprint', 'add', featureTask.id, '--sprint', '1']);
            const bugTask = await workspace.addTask('Health tab bug fix', { args: ['--project', 'WEB'] });
            await workspace.runLotar(['sprint', 'add', bugTask.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await navigateToSprints(page);
                    await openSprintInsights(page, 1);

                    await page.click('.sprint-analytics__tabs button:has-text("Health")');
                    await page.waitForSelector('.sprint-health', { timeout: 15_000 });

                    const placeholder = await page.$('.sprint-health__placeholder');
                    expect(placeholder).toBeNull();

                    const overviewTerms = await page.$$eval('.sprint-health__grid dt', (nodes) =>
                        nodes.map((node) => (node.textContent ? node.textContent.trim() : '')).filter(Boolean),
                    );
                    expect(overviewTerms.length).toBeGreaterThan(0);

                    await page.click('.sprint-analytics__close');
                    await page.waitForSelector('.sprint-analytics__overlay', { state: 'detached', timeout: 10_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
