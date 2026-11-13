import { expect } from '@playwright/test';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-418: the board's Relationships card field lists each card's related IDs.
function seed(): Record<string, string> {
    const task = (title: string, relationships?: Record<string, string[]>) =>
        stringify({
            title,
            status: 'Todo',
            priority: 'Medium',
            type: 'Feature',
            created: '2026-10-01T00:00:00Z',
            ...(relationships ? { relationships } : {}),
        });
    return {
        '.tasks/config.yml': 'default.project: REL\n',
        '.tasks/REL/config.yml': stringify({ issue: { states: ['Todo', 'Done'], done_states: ['Done'] } }),
        '.tasks/REL/1.yml': task('Schema migration', { blocks: ['REL-2', 'REL-3'] }),
        '.tasks/REL/2.yml': task('API endpoint', { depends_on: ['REL-1'] }),
        '.tasks/REL/3.yml': task('Report page', { depends_on: ['REL-1'] }),
    };
}

describe('Board relationships field (DEV-418)', () => {
    it('shows relationship chips on cards when the field is turned on, and remembers it', async () => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=REL`, async (page) => {
                    const card = (id: string) =>
                        page
                            .locator('[data-board-card]')
                            .filter({ has: page.locator('.id', { hasText: new RegExp(`^${id}$`) }) });
                    await expect(card('REL-1')).toBeVisible({ timeout: 15_000 });
                    // Off by default.
                    await expect(page.locator('.relation-chip')).toHaveCount(0);

                    await page.getByRole('button', { name: 'Configure card fields' }).click();
                    const toggle = page
                        .getByRole('dialog', { name: 'Card fields' })
                        .locator('label', { hasText: 'Relationships' })
                        .locator('input');
                    await toggle.check();
                    await expect(card('REL-1').locator('.relation-chip--blocks')).toHaveAttribute(
                        'aria-label',
                        'Blocks REL-2, REL-3',
                    );
                    await expect(card('REL-2').locator('.relation-chip--depends_on')).toHaveAttribute(
                        'aria-label',
                        'Depends on REL-1',
                    );

                    await page.reload();
                    await expect(card('REL-1').locator('.relation-chip--blocks')).toBeVisible({ timeout: 15_000 });

                    await page.getByRole('button', { name: 'Configure card fields' }).click();
                    await toggle.uncheck();
                    await expect(page.locator('.relation-chip')).toHaveCount(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
