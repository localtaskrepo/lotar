import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function formatLabel(name: string, prefix: string, maxLength = 28): string {
    const normalizedName = (name ?? '').trim();
    const normalizedPrefix = (prefix ?? '').trim();
    const hasName = normalizedName.length > 0;

    let base = hasName ? normalizedName : normalizedPrefix || 'Project';
    if (maxLength > 1 && base.length > maxLength) {
        const slicePoint = Math.max(0, maxLength - 1);
        base = `${base.slice(0, slicePoint).trimEnd()}…`;
    }

    if (!normalizedPrefix) {
        return base;
    }

    if (!hasName) {
        return base;
    }

    return `${base} (${normalizedPrefix})`;
}

describe.concurrent('UI project labels', () => {
    it('shows ellipsized project names with prefix in dropdowns', async () => {
        const qcName = 'Quality and Compliance Platform With Extended Name';
        const opsName = 'Operations Reliability and Support Services';

        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/QC/config.yml': `project_name: "${qcName}"\n`,
                '.tasks/OPS/config.yml': `project_name: "${opsName}"\n`,
            },
        });

        try {
            await workspace.addTask('QC dropdown check', { args: ['--project=QC'] });
            await workspace.addTask('OPS dropdown check', { args: ['--project=OPS'] });

            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 20_000 });
                    await page.waitForSelector('select.ui-select', { timeout: 20_000 });

                    await page.waitForFunction(
                        (value: string) => document.querySelector(`select.ui-select option[value="${value}"]`) !== null,
                        'QC',
                    );
                    await page.waitForFunction(
                        (value: string) => document.querySelector(`select.ui-select option[value="${value}"]`) !== null,
                        'OPS',
                    );

                    const qcLabel = await page.evaluate((value: string) => {
                        const option = document.querySelector(`select.ui-select option[value="${value}"]`);
                        return option?.textContent?.trim() ?? null;
                    }, 'QC');

                    const opsLabel = await page.evaluate((value: string) => {
                        const option = document.querySelector(`select.ui-select option[value="${value}"]`);
                        return option?.textContent?.trim() ?? null;
                    }, 'OPS');

                    expect(qcLabel).toBe(formatLabel(qcName, 'QC'));
                    expect(opsLabel).toBe(formatLabel(opsName, 'OPS'));
                    expect(qcLabel?.includes('…')).toBe(true);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
