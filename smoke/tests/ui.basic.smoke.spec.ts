import { describe, expect, it } from 'vitest';
import { SmokeWorkspace } from '../helpers/workspace.js';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';

describe.concurrent('UI smoke harness', () => {
  it('renders tasks created via the CLI', async () => {
    const workspace = await SmokeWorkspace.create();
    const taskTitle = 'UI Smoke Task';

    try {
      await workspace.addTask(taskTitle);

      const server = await startLotarServer(workspace);

      try {
        await withPage(server.url, async (page) => {
          await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
          await page.waitForSelector(`text=${taskTitle}`, { timeout: 15_000 });

          const content = await page.content();
          expect(content).toContain(taskTitle);
        });
      } finally {
        await server.stop();
      }
    } finally {
      await workspace.dispose();
    }
  });
});
