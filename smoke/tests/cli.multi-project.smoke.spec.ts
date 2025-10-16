import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { SmokeWorkspace } from '../helpers/workspace.js';

function frontendManifest(): string {
  return JSON.stringify({ name: 'frontend-app', private: true, version: '0.0.0' }, null, 2);
}

function apiManifest(): string {
  return [
    '[package]',
    'name = "api-service"',
    'version = "0.1.0"',
  ].join('\n');
}

describe.concurrent('CLI multi-project smoke scenarios', () => {
  it('auto-detects project prefixes across monorepo directories', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();

      await workspace.write('apps/frontend/package.json', frontendManifest());
      await workspace.write('services/api/Cargo.toml', apiManifest());
      await workspace.write('services/api/src/lib.rs', '// api lib');

      const frontendTask = await workspace.addTask('Frontend default-detected task', {
        cwd: path.join(workspace.root, 'apps', 'frontend'),
      });

      const apiTask = await workspace.addTask('API default-detected task', {
        cwd: path.join(workspace.root, 'services', 'api'),
      });

      expect(frontendTask.project).not.toBe(apiTask.project);
      expect(path.dirname(frontendTask.filePath)).toBe(path.join(workspace.tasksDir, frontendTask.project));
      expect(path.dirname(apiTask.filePath)).toBe(path.join(workspace.tasksDir, apiTask.project));

      const listAll = await workspace.runLotar(['list']);
      expect(listAll.stdout).toContain(frontendTask.id);
      expect(listAll.stdout).toContain(apiTask.id);

      const frontendList = await workspace.runLotar(['list', '--project', frontendTask.project]);
      expect(frontendList.stdout).toContain(frontendTask.id);
      expect(frontendList.stdout).not.toContain(apiTask.id);

      const apiList = await workspace.runLotar(['list', '--project', apiTask.project]);
      expect(apiList.stdout).toContain(apiTask.id);
      expect(apiList.stdout).not.toContain(frontendTask.id);
    } finally {
      await workspace.dispose();
    }
  });

  it('respects explicit project overrides even when defaults exist', async () => {
    const workspace = await SmokeWorkspace.create();

    try {
      await workspace.initGit();

      await workspace.write('apps/frontend/package.json', frontendManifest());

      const defaultTask = await workspace.addTask('Default project inference', {
        cwd: path.join(workspace.root, 'apps', 'frontend'),
      });

      const overrideTask = await workspace.addTask('Explicit override task', {
        cwd: path.join(workspace.root, 'apps', 'frontend'),
        args: ['--project=QA'],
      });

      expect(defaultTask.project).not.toBe('QA');
      expect(overrideTask.project).toBe('QA');
      expect(path.dirname(overrideTask.filePath)).toBe(path.join(workspace.tasksDir, 'QA'));

      const overrideList = await workspace.runLotar(['list', '--project', 'QA']);
      expect(overrideList.stdout).toContain(overrideTask.id);
      expect(overrideList.stdout).not.toContain(defaultTask.id);

      const defaultList = await workspace.runLotar(['list', '--project', defaultTask.project]);
      expect(defaultList.stdout).toContain(defaultTask.id);
      expect(defaultList.stdout).not.toContain(overrideTask.id);
    } finally {
      await workspace.dispose();
    }
  });
});
