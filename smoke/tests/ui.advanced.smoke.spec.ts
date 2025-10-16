import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser, withPage } from '../helpers/ui.js';

type WaitForResponsePredicate = (response: { url(): string; ok(): boolean }) => boolean;

type ResponseWaitTarget = {
  waitForResponse(predicate: WaitForResponsePredicate, options?: { timeout?: number }): Promise<unknown>;
};

const insightsRequiredEndpoint = '/api/tasks/list';

function createOkResponsePredicate(endpoint: string): WaitForResponsePredicate {
  return (response) => response.url().includes(endpoint) && response.ok();
}

function waitForInsightsData(page: ResponseWaitTarget): Promise<unknown> {
  return page.waitForResponse(createOkResponsePredicate(insightsRequiredEndpoint), { timeout: 15_000 });
}

describe.concurrent('UI advanced smoke scenarios', () => {
  it('keeps multiple browser sessions in sync when tasks are added', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();
      await workspace.addTask('Initial UI task');
      await workspace.commitAll('seed ui task');

      const server = await startLotarServer(workspace);
      try {
        await withBrowser({}, async (context) => {
          const pageA = await context.newPage();
          const pageB = await context.newPage();

          await Promise.all([pageA.goto(server.url), pageB.goto(server.url)]);
          await Promise.all([
            pageA.waitForSelector('text=Initial UI task', { timeout: 15_000 }),
            pageB.waitForSelector('text=Initial UI task', { timeout: 15_000 }),
          ]);

          await workspace.addTask('Task added after launch');

          await Promise.all([pageA.reload(), pageB.reload()]);
          await Promise.all([
            pageA.waitForSelector('text=Task added after launch', { timeout: 15_000 }),
            pageB.waitForSelector('text=Task added after launch', { timeout: 15_000 }),
          ]);
        });
      } finally {
        await server.stop();
      }
    } finally {
      await workspace.dispose();
    }
  });

  it('reflects branch switches in the task list UI', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();
      const task = await workspace.addTask('Branch Switch Task');
      await workspace.commitAll('baseline task');

      await workspace.runGit(['checkout', '-b', 'feature/switch-ui']);
      const payload = parse(await workspace.readTaskYaml(task.id)) as Record<string, unknown>;
      payload.title = 'Feature Branch Task';
      await fs.writeFile(task.filePath, stringify(payload));
      await workspace.commitAll('rename on feature branch');

      await workspace.runGit(['checkout', 'main']);
      const server = await startLotarServer(workspace);
      try {
        await withPage(server.url, async (page) => {
          await page.waitForSelector('text=Branch Switch Task', { timeout: 15_000 });

          await workspace.runGit(['checkout', 'feature/switch-ui']);
          await page.reload();
          await page.waitForSelector('text=Feature Branch Task', { timeout: 15_000 });
          const content = await page.content();
          expect(content).not.toContain('Branch Switch Task');
        });
      } finally {
        await server.stop();
      }
    } finally {
      await workspace.dispose();
    }
  });

  it('surfaces analytics on the insights dashboard', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();
      const todo = await workspace.addTask('Insights Todo Task', {
        args: ['--priority', 'high', '--tag', 'analytics', '--assignee', 'insights@example.com'],
      });
      await workspace.commitAll('seed insights base');
      await workspace.runLotar(['status', todo.id, 'done']);
      await workspace.commitAll('mark insights task done');
      await workspace.addTask('Insights In Progress', {
        args: ['--type', 'Bug', '--tag', 'analytics', '--tag', 'ui'],
      });
      await workspace.commitAll('add insights in progress');
      await workspace.addTask('Insights Backlog Task', {
        args: ['--type', 'Chore', '--priority', 'low', '--tag', 'ops'],
      });
      await workspace.commitAll('add insights backlog');

      const server = await startLotarServer(workspace);
      try {
        await withPage(server.url, async (page) => {
          const insightsLink = page.locator('a:has-text("Insights")');
          await insightsLink.waitFor({ state: 'visible', timeout: 15_000 });

          const waitForNavigation = page.waitForURL('**/insights**', { timeout: 15_000 });
          const waitForInitialData = waitForInsightsData(page);

          await Promise.all([waitForNavigation, insightsLink.click()]);
          await waitForInitialData;

          await page.waitForSelector('h1:has-text("Insights")', { timeout: 15_000 });
          const loader = page.locator('text=Loading insights…');
          await loader.waitFor({ state: 'visible', timeout: 5_000 }).catch(() => null);
          await loader.waitFor({ state: 'detached', timeout: 15_000 }).catch(() => null);

          const summary = page.locator('.summary-grid');
          await page.waitForSelector('.summary-grid', { timeout: 15_000 });
          const summaryText = (await summary.innerText()).toLowerCase();
          expect(summaryText).toContain('total tasks');
          expect(summaryText).toContain('tagged');

          const totalValueHandle = await page.waitForFunction(
            () => {
              const el = document.querySelector('.summary-grid .summary-tile strong');
              if (!el) return null;
              const raw = el.textContent?.trim();
              if (!raw) return null;
              const parsed = Number.parseInt(raw, 10);
              return Number.isFinite(parsed) ? parsed : null;
            },
            {},
            { timeout: 15_000 },
          );
          const totalTasksValue = await totalValueHandle.jsonValue();
          expect((totalTasksValue as number)).toBeGreaterThanOrEqual(3);

          const waitForRefreshData = waitForInsightsData(page);
          await page.locator('button:has-text("Refresh")').click();
          await waitForRefreshData;
          await loader.waitFor({ state: 'visible', timeout: 5_000 }).catch(() => null);
          await loader.waitFor({ state: 'detached', timeout: 15_000 }).catch(() => null);
        });
      } finally {
        await server.stop();
      }
    } finally {
      await workspace.dispose();
    }
  });

  it('keeps CSV export aligned with list API results', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();
      await workspace.addTask('Export parity task A', { args: ['--tag', 'export', '--priority', 'high'] });
      await workspace.addTask('Export parity task B', { args: ['--tag', 'export', '--priority', 'low'] });

      const server = await startLotarServer(workspace);
      try {
        const listResponse = await fetch(`${server.url}/api/tasks/list?tags=export`);
        expect(listResponse.ok).toBe(true);
        const listPayload = (await listResponse.json()) as { data: Array<{ id: string }> };
        expect(listPayload.data.length).toBeGreaterThanOrEqual(2);

        const exportResponse = await fetch(`${server.url}/api/tasks/export?tags=export`);
        expect(exportResponse.ok).toBe(true);
        const csv = await exportResponse.text();
        const lines = csv.trim().split('\n').slice(1); // drop header
        const csvIds = lines.map((line) => line.split(',')[0].replace(/"/g, ''));
        const listIds = listPayload.data.map((entry) => entry.id);
        listIds.forEach((id) => {
          expect(csvIds).toContain(id);
        });
      } finally {
        await server.stop();
      }
    } finally {
      await workspace.dispose();
    }
  });

  it('shows externally edited tasks after background updates', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();
      const task = await workspace.addTask('Background original task');
      await workspace.commitAll('initial background state');

      const server = await startLotarServer(workspace);
      try {
        await withPage(server.url, async (page) => {
          await page.waitForSelector('text=Background original task', { timeout: 15_000 });

          const payload = parse(await workspace.readTaskYaml(task.id)) as Record<string, unknown>;
          payload.title = 'Background updated task';
          await fs.writeFile(task.filePath, stringify(payload));
          await workspace.commitAll('background edit');

          await page.reload();
          await page.waitForSelector('text=Background updated task', { timeout: 15_000 });
        });
      } finally {
        await server.stop();
      }
    } finally {
      await workspace.dispose();
    }
  });
});
