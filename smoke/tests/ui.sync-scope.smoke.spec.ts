import fs from 'fs-extra';
import path from 'node:path';
import type { Page } from '@playwright/test';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { deadLoopbackPort, JiraStub } from '../helpers/jira-stub.js';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-67 durable browser coverage: SyncHub runs, reports, selections, and the
// remote editor must stay scoped by project. Every sync below is a REAL run:
// pulls/checks start from the page against a local HTTP Jira stub (real REST
// sync start in the server process, real SSE sync events back to the page),
// the dry-run push goes through the actual REST API, and failures use a dead
// loopback endpoint so genuine backend errors surface in the UI. No route
// mocking for core behavior; the only controlled hold is the stub's
// deterministic request barrier (explicitly armed and released by the test).
//
// Isolation: each test owns a temporary workspace with explicit project A/B
// configs (same remote name, different stub URLs and sync_reports_dir roots).
// The server's home dir is pointed INSIDE that workspace with
// LOTAR_IGNORE_HOME_CONFIG=0 so auth profiles come from an isolated .lotar
// fixture with harmless fake credentials only — no real home config, no
// network beyond 127.0.0.1 stubs.
describe('UI sync scope isolation (DEV-67)', () => {
    const STATUS_PILL = '.remote-title .pill--interactive';

    function remoteYaml(project: string, auth: string) {
        return [
            '    provider: jira',
            `    project: ${project}`,
            `    auth_profile: ${auth}`,
            '    mapping:',
            '      title: summary',
        ].join('\n');
    }

    interface ProjectSeed {
        readonly name: string;
        readonly remotes?: Readonly<Record<string, { project: string; auth: string }>>;
        readonly reportsDir?: string;
    }

    /**
     * Builds the workspace seed: per-project configs with homonym remotes and
     * distinct report roots, an optional global remote (editor tests), and the
     * isolated `.lotar` home fixture holding only stub auth profiles.
     */
    function buildSeedFiles(options: {
        projects: Readonly<Record<string, ProjectSeed>>;
        globalRemotes?: Readonly<Record<string, { project: string; auth: string }>>;
        authProfiles: Readonly<Record<string, string>>;
        defaultProject?: string;
    }): Record<string, string> {
        const files: Record<string, string> = {};

        const globalRemoteNames = Object.keys(options.globalRemotes ?? {});
        const globalLines = ['sync_write_reports: true'];
        if (options.defaultProject) {
            globalLines.push(`default_project: ${options.defaultProject}`);
        }
        if (globalRemoteNames.length) {
            globalLines.push('remotes:');
            for (const name of globalRemoteNames) {
                const remote = options.globalRemotes![name]!;
                globalLines.push(`  ${name}:`, remoteYaml(remote.project, remote.auth));
            }
        }
        files['.tasks/config.yml'] = globalLines.join('\n') + '\n';

        for (const [prefix, project] of Object.entries(options.projects)) {
            const lines = [`project_name: ${project.name}`];
            if (project.reportsDir) {
                lines.push(`sync_reports_dir: "${project.reportsDir}"`);
            }
            const remoteNames = Object.keys(project.remotes ?? {});
            if (remoteNames.length) {
                lines.push('remotes:');
                for (const name of remoteNames) {
                    const remote = project.remotes![name]!;
                    lines.push(`  ${name}:`, remoteYaml(remote.project, remote.auth));
                }
            }
            files[`.tasks/${prefix}/config.yml`] = lines.join('\n') + '\n';
        }

        const profileLines = ['auth_profiles:'];
        for (const [name, apiUrl] of Object.entries(options.authProfiles)) {
            profileLines.push(
                `  ${name}:`,
                '    provider: jira',
                '    method: basic',
                '    email_env: dev67-smoke@example.test',
                `    token_env: dev67-smoke-fake-${name}`,
                `    api_url: ${apiUrl}`,
            );
        }
        files['.lotar'] = profileLines.join('\n') + '\n';

        return files;
    }

    /** Server env: isolated home inside the workspace, home config enabled. */
    function isolatedHomeEnv(workspace: SmokeWorkspace): NodeJS.ProcessEnv {
        return {
            HOME: workspace.root,
            LOTAR_IGNORE_HOME_CONFIG: '0',
        };
    }

    function remoteRow(page: Page, remote: string) {
        return page.locator(`.remote-row:has(.remote-title strong:text-is("${remote}"))`);
    }

    /** Status pill text for a remote row ('' when the row has no run pill). */
    async function pillText(page: Page, remote: string): Promise<string> {
        const pills = remoteRow(page, remote).locator(STATUS_PILL);
        if ((await pills.count()) === 0) {
            return '';
        }
        return ((await pills.first().textContent()) ?? '').trim();
    }

    async function selectScope(page: Page, scope: string): Promise<void> {
        await page.selectOption('#sync-scope', scope);
    }

    async function apiGet<T>(serverUrl: string, path: string): Promise<T> {
        const res = await fetch(`${serverUrl}${path}`, {
            headers: { Accept: 'application/json' },
        });
        const payload = (await res.json()) as { data?: T; error?: { message?: string } };
        if (!res.ok) {
            throw new Error(`GET ${path} failed: ${res.status} ${payload.error?.message ?? ''}`);
        }
        return payload.data as T;
    }

    async function apiPost<T>(
        serverUrl: string,
        path: string,
        body: Record<string, unknown>,
    ): Promise<T> {
        const res = await fetch(`${serverUrl}${path}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
            body: JSON.stringify(body),
        });
        const payload = (await res.json()) as { data?: T; error?: { message?: string } };
        if (!res.ok) {
            throw new Error(`POST ${path} failed: ${res.status} ${payload.error?.message ?? ''}`);
        }
        return payload.data as T;
    }

    /**
     * Reads a project config in either the flat seed form or the canonical
     * nested form the config writer emits (project.name / sync.reports_dir).
     */
    function readProjectConfig(raw: string) {
        const parsed = parse(raw) as Record<string, any>;
        const project = parsed.project ?? {};
        const sync = parsed.sync ?? {};
        return {
            projectName: project.name ?? parsed.project_name,
            reportsDir: sync.reports_dir ?? parsed.sync_reports_dir,
            remotes: (parsed.remotes ?? {}) as Record<string, { project?: string; filter?: string }>,
        };
    }

    async function projectTaskFiles(workspace: SmokeWorkspace, prefix: string): Promise<string[]> {
        const dir = path.join(workspace.tasksDir, prefix);
        if (!(await fs.pathExists(dir))) {
            return [];
        }
        const entries = await fs.readdir(dir);
        return entries.filter((entry) => entry.endsWith('.yml') && entry !== 'config.yml');
    }

    it('pulls through the local Jira stub into the requesting project only', async () => {
        const stub = await JiraStub.start({
            projectKey: 'JAA',
            issues: [{ key: 'JAA-101', summary: 'Dev67 stub issue alpha' }],
        });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        reportsDir: '@reports-alpha',
                        remotes: { 'shared-jira': { project: 'JAA', auth: 'smoke-jira-a' } },
                    },
                    BETA: { name: 'Beta' },
                },
                authProfiles: { 'smoke-jira-a': stub.url },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                // Fixture validation: the isolated home profile override is
                // actually live in the server process (secrets stay stripped).
                const inspect = await apiGet<{ auth_profiles: Record<string, { api_url?: string }> }>(
                    server.url,
                    '/api/config/inspect',
                );
                expect(inspect.auth_profiles['smoke-jira-a']?.api_url).toBe(stub.url);

                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'shared-jira').waitFor({ timeout: 15_000 });
                    await expect
                        .poll(() => remoteRow(page, 'shared-jira').locator('.remote-provider').textContent())
                        .toContain('JAA');

                    await remoteRow(page, 'shared-jira')
                        .getByRole('button', { name: 'Pull', exact: true })
                        .click();

                    await expect.poll(() => pillText(page, 'shared-jira'), { timeout: 15_000 }).toBe('Success');
                    await page.waitForSelector('.toast-card:has-text("PULL shared-jira")', { timeout: 10_000 });

                    // Stored report: listed, openable, project-scoped root.
                    await page.waitForSelector('.report-item', { timeout: 15_000 });
                    await page.locator('.report-item').first().click();
                    // Created entries render the local task id; the Jira
                    // reference linkage is asserted on the task YAML below.
                    await page.waitForSelector('.report-entry:has-text("ALPHA-1")', { timeout: 10_000 });
                    await expect
                        .poll(() => page.locator('.report-path__value').textContent())
                        .toContain('.tasks/@reports-alpha/');

                    // The pull created exactly one task in ALPHA, linked to the stub issue.
                    const files = await projectTaskFiles(workspace, 'ALPHA');
                    expect(files.length).toBe(1);
                    const taskYaml = await fs.readFile(`${workspace.tasksDir}/ALPHA/${files[0]}`, 'utf8');
                    expect(taskYaml).toContain('JAA-101');
                    expect(taskYaml).toContain('Dev67 stub issue alpha');

                    // The stub saw exactly one scoped, authenticated search.
                    const searches = stub.searchRequests();
                    expect(searches.length).toBe(1);
                    expect(searches[0]!.query.jql).toBe('project = JAA');
                    expect(searches[0]!.authorization?.startsWith('Basic ')).toBe(true);

                    // Other scopes see neither the remote nor the report.
                    await selectScope(page, 'BETA');
                    await page.waitForSelector('text=No remotes configured for this scope.', { timeout: 15_000 });
                    await page.waitForSelector('text=No reports yet.', { timeout: 15_000 });
                    await selectScope(page, '');
                    await page.waitForSelector('text=No remotes configured for this scope.', { timeout: 15_000 });
                    await page.waitForSelector('text=No reports yet.', { timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stub.stop();
        }
    });

    it('keeps an active pull scoped while the user switches projects', async () => {
        const stubAlpha = await JiraStub.start({
            projectKey: 'JAA',
            issues: [{ key: 'JAA-201', summary: 'Dev67 barrier issue alpha' }],
        });
        const stubBeta = await JiraStub.start({ projectKey: 'JAB', issues: [] });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        reportsDir: '@reports-alpha',
                        remotes: { 'shared-jira': { project: 'JAA', auth: 'smoke-jira-a' } },
                    },
                    BETA: {
                        name: 'Beta',
                        reportsDir: '@reports-beta',
                        remotes: { 'shared-jira': { project: 'JAB', auth: 'smoke-jira-b' } },
                    },
                },
                authProfiles: { 'smoke-jira-a': stubAlpha.url, 'smoke-jira-b': stubBeta.url },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'shared-jira').waitFor({ timeout: 15_000 });

                    // Hold the stub's search so the pull stays observably active.
                    stubAlpha.armSearchBarrier();
                    await remoteRow(page, 'shared-jira')
                        .getByRole('button', { name: 'Pull', exact: true })
                        .click();

                    const search = await stubAlpha.waitForRequest(
                        (request) => request.path === '/rest/api/3/search/jql',
                    );
                    expect(search.query.jql).toBe('project = JAA');

                    await expect.poll(() => pillText(page, 'shared-jira'), { timeout: 15_000 }).toBe('Running');
                    await expect
                        .poll(() =>
                            remoteRow(page, 'shared-jira')
                                .getByRole('button', { name: 'Pull', exact: true })
                                .isDisabled(),
                        )
                        .toBe(true);

                    // Switch scope mid-run: the homonym row must lose the ALPHA
                    // run pill (scope-keyed attribution, no relabeling).
                    await selectScope(page, 'BETA');
                    await expect.poll(() => pillText(page, 'shared-jira')).toBe('');

                    stubAlpha.releaseSearchBarrier();

                    // BETA loads its own config; the completed ALPHA run must
                    // not steal state, selection, or reports here.
                    await page.waitForSelector('.remote-provider:has-text("JAB")', { timeout: 15_000 });
                    await expect.poll(() => pillText(page, 'shared-jira')).toBe('');
                    await page.waitForSelector('text=No reports yet.', { timeout: 15_000 });
                    expect(await page.locator('.report-item').count()).toBe(0);
                    expect(stubBeta.allRequests().length).toBe(0);

                    // Back in ALPHA: the run finalized in its own scope with a
                    // stored report under the ALPHA root.
                    await selectScope(page, 'ALPHA');
                    await expect.poll(() => pillText(page, 'shared-jira'), { timeout: 15_000 }).toBe('Success');
                    await page.waitForSelector('.report-item', { timeout: 15_000 });
                    await page.locator('.report-item').first().click();
                    await page.waitForSelector('.report-entry:has-text("ALPHA-1")', { timeout: 10_000 });
                    await expect
                        .poll(() => page.locator('.report-path__value').textContent())
                        .toContain('.tasks/@reports-alpha/');
                    expect(stubAlpha.searchRequests().length).toBe(1);
                    expect(await fs.pathExists(`${workspace.tasksDir}/@reports-beta`)).toBe(false);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stubAlpha.stop();
            await stubBeta.stop();
        }
    });

    it('runs check as a real dry-run pull and surfaces dead-remote failures in scope', async () => {
        const stub = await JiraStub.start({
            projectKey: 'JAA',
            issues: [{ key: 'JAA-301', summary: 'Dev67 check issue' }],
        });
        const deadPort = await deadLoopbackPort();
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        remotes: { 'shared-jira': { project: 'JAA', auth: 'smoke-jira-a' } },
                    },
                    BETA: {
                        name: 'Beta',
                        reportsDir: '@reports-beta',
                        remotes: { 'shared-jira': { project: 'JDEAD', auth: 'smoke-jira-dead' } },
                    },
                },
                authProfiles: {
                    'smoke-jira-a': stub.url,
                    'smoke-jira-dead': `http://127.0.0.1:${deadPort}`,
                },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'shared-jira').waitFor({ timeout: 15_000 });

                    // Check = dry-run pull: real stub request, nothing persisted.
                    await remoteRow(page, 'shared-jira')
                        .getByRole('button', { name: 'Check', exact: true })
                        .click();
                    await expect.poll(() => pillText(page, 'shared-jira'), { timeout: 15_000 }).toBe('Success');
                    await page.waitForSelector('.report-item', { timeout: 15_000 });
                    await page.locator('.report-item').first().click();
                    await page.waitForSelector('.report-header__status .pill:has-text("Dry run")', {
                        timeout: 10_000,
                    });
                    // Dry-run entries have no local task id, so the Jira
                    // reference renders as the entry headline.
                    await page.waitForSelector('.report-entry:has-text("JAA-301")', { timeout: 10_000 });
                    expect((await projectTaskFiles(workspace, 'ALPHA')).length).toBe(0);
                    expect(await fs.pathExists(`${workspace.tasksDir}/@reports-alpha`)).toBe(false);
                    expect(stub.searchRequests().length).toBe(1);

                    // BETA's homonym points at a dead endpoint: genuine
                    // backend failure, captured in BETA only.
                    await selectScope(page, 'BETA');
                    await page.waitForSelector('.remote-provider:has-text("JDEAD")', { timeout: 15_000 });
                    await remoteRow(page, 'shared-jira')
                        .getByRole('button', { name: 'Check', exact: true })
                        .click();
                    await expect.poll(() => pillText(page, 'shared-jira'), { timeout: 15_000 }).toBe('Failed');
                    await page.waitForSelector('.toast-card:has-text("Remote API request failed")', {
                        timeout: 10_000,
                    });
                    await page.waitForSelector('.report-item:has-text("Failed")', { timeout: 15_000 });
                    expect(await fs.pathExists(`${workspace.tasksDir}/@reports-beta`)).toBe(false);

                    // The failure never leaked into ALPHA's run state.
                    await selectScope(page, 'ALPHA');
                    await expect.poll(() => pillText(page, 'shared-jira'), { timeout: 15_000 }).toBe('Success');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stub.stop();
        }
    });

    it('surfaces an API dry-run push as an external run without touching remote state', async () => {
        const stub = await JiraStub.start({ projectKey: 'JAA', issues: [] });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        remotes: { 'shared-jira': { project: 'JAA', auth: 'smoke-jira-a' } },
                    },
                },
                authProfiles: { 'smoke-jira-a': stub.url },
            }),
        });
        try {
            const seeded = await workspace.addTask('Dev67 push anchor', {
                args: ['--project=ALPHA'],
            });
            expect(seeded.project).toBe('ALPHA');

            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'shared-jira').waitFor({ timeout: 15_000 });

                    // Real REST push, dry run: offline by contract — the stub
                    // must observe zero requests.
                    const result = await apiPost<{ summary: { created: number }; report: { dry_run: boolean } }>(
                        server.url,
                        '/api/sync/push',
                        {
                            remote: 'shared-jira',
                            project: 'ALPHA',
                            dry_run: true,
                            include_report: true,
                        },
                    );
                    expect(result.summary.created).toBe(1);
                    expect(result.report.dry_run).toBe(true);

                    // External runs surface as tagged report items only. The
                    // item appears on sync_started; wait for the completed
                    // summary so the entry snapshot is settled before opening.
                    await page.waitForSelector('.report-item:has-text("PUSH")', { timeout: 15_000 });
                    await page.waitForSelector('.report-item:has-text("External")', { timeout: 10_000 });
                    await page.waitForSelector('.report-item:has-text("1 created")', { timeout: 15_000 });
                    await expect.poll(() => pillText(page, 'shared-jira')).toBe('');

                    await page.locator('.report-item').first().click();
                    await page.waitForSelector('.report-header__status .pill:has-text("Dry run")', {
                        timeout: 10_000,
                    });
                    // The preserved progress entry carries the actual pushed
                    // task id from the real report entries.
                    await page.waitForSelector(`.report-entry:has-text("${seeded.id}")`, { timeout: 10_000 });
                    await page.waitForSelector('.report-entry:has-text("would create")', { timeout: 10_000 });

                    expect(stub.allRequests().length).toBe(0);
                    expect(await fs.pathExists(`${workspace.tasksDir}/@reports`)).toBe(false);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stub.stop();
        }
    });

    it('editor rejects invalid YAML and saves a project override without mutating the global map', async () => {
        const stub = await JiraStub.start({ projectKey: 'JG', issues: [] });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        remotes: { 'alpha-jira': { project: 'JA2', auth: 'smoke-jira-a' } },
                    },
                },
                globalRemotes: { 'inherited-jira': { project: 'JG', auth: 'smoke-jira-a' } },
                authProfiles: { 'smoke-jira-a': stub.url },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'inherited-jira').waitFor({ timeout: 15_000 });
                    await page.waitForSelector('.remote-origin-chip:has-text("inherited")', { timeout: 10_000 });

                    await remoteRow(page, 'inherited-jira')
                        .getByRole('button', { name: 'Edit', exact: true })
                        .click();
                    await page.waitForSelector('[data-testid="remote-dialog-choice"]', { timeout: 10_000 });
                    await page.waitForSelector('fieldset.sync-remote-dialog__fieldset[disabled]', { timeout: 10_000 });
                    await expect
                        .poll(() => page.locator('[data-testid="remote-dialog-target"]').textContent())
                        .toContain('Inherited from Global');

                    // Explicit override choice unlocks the fields.
                    await page.getByRole('button', { name: 'Override in project ALPHA' }).click();
                    await page.waitForSelector('[data-testid="remote-dialog-choice"]', { state: 'detached' });
                    await expect
                        .poll(() => page.locator('.sync-textarea').isDisabled())
                        .toBe(false);

                    // Invalid mapping YAML is rejected client-side, no save.
                    await page.locator('.sync-textarea').fill('title: [unclosed');
                    await page.waitForSelector('.sync-remote-dialog__form p.error:not([data-testid])', {
                        timeout: 10_000,
                    });
                    await page.getByRole('button', { name: 'Save remote' }).click();
                    await page.waitForSelector(
                        '[data-testid="remote-dialog-error"]:has-text("Mapping must be valid YAML.")',
                        { timeout: 10_000 },
                    );

                    // A valid edit persists ONLY into the project map.
                    await page.locator('.sync-textarea').fill('title: summary');
                    await page.locator('input[placeholder="Optional filter"]').fill('labels in dev67');
                    await page.getByRole('button', { name: 'Save remote' }).click();
                    await page.waitForSelector('.toast-card:has-text("Remote saved")', { timeout: 15_000 });
                    await page.waitForSelector('.sync-remote-dialog__overlay', { state: 'detached' });

                    const globalConfig = parse(await workspace.read('.tasks/config.yml')) as {
                        remotes?: Record<string, { project?: string; filter?: string }>;
                    };
                    expect(globalConfig.remotes?.['inherited-jira']?.project).toBe('JG');
                    expect(globalConfig.remotes?.['inherited-jira']?.filter ?? null).toBeNull();

                    const projectConfig = readProjectConfig(await workspace.read('.tasks/ALPHA/config.yml'));
                    expect(projectConfig.projectName).toBe('Alpha');
                    // Sibling project remote is intact and the override landed.
                    expect(projectConfig.remotes['alpha-jira']?.project).toBe('JA2');
                    expect(projectConfig.remotes['inherited-jira']?.filter).toBe('labels in dev67');

                    // After the override the row is project-owned: no inherited chip.
                    await page.waitForSelector('.remote-origin-chip', { state: 'detached' });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stub.stop();
        }
    });

    it('editor saves global edits without leaking into the project config', async () => {
        const stub = await JiraStub.start({ projectKey: 'JG', issues: [] });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        remotes: { 'alpha-jira': { project: 'JA2', auth: 'smoke-jira-a' } },
                    },
                },
                globalRemotes: { 'inherited-jira': { project: 'JG', auth: 'smoke-jira-a' } },
                authProfiles: { 'smoke-jira-a': stub.url },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'inherited-jira').waitFor({ timeout: 15_000 });

                    await remoteRow(page, 'inherited-jira')
                        .getByRole('button', { name: 'Edit', exact: true })
                        .click();
                    await page.waitForSelector('[data-testid="remote-dialog-choice"]', { timeout: 10_000 });
                    await page.getByRole('button', { name: 'Edit in Global scope' }).click();
                    await expect
                        .poll(() => page.locator('[data-testid="remote-dialog-target"]').textContent())
                        .toContain('Saves the remote in Global config');

                    await page.locator('input[placeholder="DEMO"]').fill('JG2');
                    await page.getByRole('button', { name: 'Save remote' }).click();
                    await page.waitForSelector('.toast-card:has-text("Remote saved")', { timeout: 15_000 });
                    await page.waitForSelector('.sync-remote-dialog__overlay', { state: 'detached' });

                    const globalConfig = parse(await workspace.read('.tasks/config.yml')) as {
                        remotes?: Record<string, { project?: string }>;
                    };
                    expect(globalConfig.remotes?.['inherited-jira']?.project).toBe('JG2');

                    const projectConfig = readProjectConfig(await workspace.read('.tasks/ALPHA/config.yml'));
                    expect(projectConfig.projectName).toBe('Alpha');
                    expect(Object.keys(projectConfig.remotes)).toEqual(['alpha-jira']);

                    // The remote is still inherited in project scope, with the
                    // edited provider key rendered.
                    await page.waitForSelector('.remote-origin-chip:has-text("inherited")', { timeout: 10_000 });
                    await expect
                        .poll(() => remoteRow(page, 'inherited-jira').locator('.remote-provider').textContent())
                        .toContain('JG2');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stub.stop();
        }
    });

    it('blocks Global runs when the default project overrides the same-named remote', async () => {
        const stubGlobal = await JiraStub.start({ projectKey: 'JG', issues: [] });
        const stubOverride = await JiraStub.start({ projectKey: 'JAA', issues: [] });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                defaultProject: 'ALPHA',
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        remotes: { 'shared-jira': { project: 'JAA', auth: 'smoke-jira-b' } },
                    },
                },
                globalRemotes: { 'shared-jira': { project: 'JG', auth: 'smoke-jira-a' } },
                authProfiles: { 'smoke-jira-a': stubGlobal.url, 'smoke-jira-b': stubOverride.url },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync`, async (page) => {
                    // Global scope shows the GLOBAL definition of the homonym.
                    await remoteRow(page, 'shared-jira').waitFor({ timeout: 15_000 });
                    await expect
                        .poll(() => remoteRow(page, 'shared-jira').locator('.remote-provider').textContent())
                        .toContain('JG');

                    // Run-start requests only; report listings are legit
                    // reads and excluded from the "no sync started" contract.
                    const syncUrls: string[] = [];
                    page.on('request', (request) => {
                        const pathname = new URL(request.url()).pathname;
                        if (pathname === '/api/sync/pull' || pathname === '/api/sync/push' || pathname === '/api/sync/validate') {
                            syncUrls.push(request.url());
                        }
                    });

                    // Every action from the Global scope must be blocked BEFORE
                    // any sync starts: the preflight inspects the default
                    // project, detects the divergent override, and explains it.
                    const actions = ['Pull', 'Push', 'Check'] as const;
                    for (const [index, action] of actions.entries()) {
                        await remoteRow(page, 'shared-jira')
                            .getByRole('button', { name: action, exact: true })
                            .click();
                        await page.waitForSelector('.toast-card:has-text("overridden in default project ALPHA")', {
                            timeout: 10_000,
                        });
                        const expectedToasts = index + 1;
                        await expect
                            .poll(
                                () =>
                                    page
                                        .locator('.toast-card:has-text("overridden in default project ALPHA")')
                                        .count(),
                                { timeout: 10_000 },
                            )
                            .toBeGreaterThanOrEqual(expectedToasts);

                        // Blocked preflight: no sync request left the page and
                        // no run state was created.
                        expect(syncUrls.length).toBe(0);
                        expect(await pillText(page, 'shared-jira')).toBe('');
                    }

                    // The full guidance names the override and the fix.
                    const toastText = await page
                        .locator('.toast-card:has-text("overridden in default project ALPHA")')
                        .first()
                        .textContent();
                    expect(toastText).toContain("Remote 'shared-jira' is overridden in default project ALPHA");
                    expect(toastText).toContain('Switch to ALPHA to run it, or align the definitions.');

                    // Neither definition was contacted by the blocked runs.
                    expect(stubGlobal.allRequests().length).toBe(0);
                    expect(stubOverride.allRequests().length).toBe(0);
                    expect(syncUrls.length).toBe(0);

                    // Preflight busy state clears, leaving the row actionable.
                    await expect
                        .poll(() =>
                            remoteRow(page, 'shared-jira')
                                .getByRole('button', { name: 'Pull', exact: true })
                                .isDisabled(),
                        )
                        .toBe(false);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stubGlobal.stop();
            await stubOverride.stop();
        }
    });

    it('hides stale remote rows while a scope config load is pending or failed', async () => {
        const stubAlpha = await JiraStub.start({ projectKey: 'JAA', issues: [] });
        const stubBeta = await JiraStub.start({ projectKey: 'JAB', issues: [] });
        const workspace = await SmokeWorkspace.create({
            seedFiles: buildSeedFiles({
                projects: {
                    ALPHA: {
                        name: 'Alpha',
                        remotes: { 'shared-jira': { project: 'JAA', auth: 'smoke-jira-a' } },
                    },
                    BETA: {
                        name: 'Beta',
                        remotes: { 'shared-jira': { project: 'JAB', auth: 'smoke-jira-b' } },
                    },
                },
                authProfiles: { 'smoke-jira-a': stubAlpha.url, 'smoke-jira-b': stubBeta.url },
            }),
        });
        try {
            const server = await startLotarServer(workspace, { env: isolatedHomeEnv(workspace) });
            try {
                await withPage(`${server.url}/sync?project=ALPHA`, async (page) => {
                    await remoteRow(page, 'shared-jira').waitFor({ timeout: 15_000 });
                    const addRemote = page.getByRole('button', { name: 'Add remote' });
                    await expect.poll(() => addRemote.isDisabled()).toBe(false);

                    // Run-start requests only; report listings are legit
                    // reads and excluded from the "no sync started" contract.
                    const syncUrls: string[] = [];
                    page.on('request', (request) => {
                        const pathname = new URL(request.url()).pathname;
                        if (pathname === '/api/sync/pull' || pathname === '/api/sync/push' || pathname === '/api/sync/validate') {
                            syncUrls.push(request.url());
                        }
                    });

                    // Hold ONLY the BETA-scoped config inspections; every other
                    // request (reports, SSE, stub auth) continues for real.
                    const held: import('@playwright/test').Route[] = [];
                    let holding = true;
                    await page.route('**/api/config/inspect*', async (route) => {
                        const params = new URL(route.request().url()).searchParams;
                        if (holding && params.get('project') === 'BETA') {
                            held.push(route);
                            return;
                        }
                        await route.continue();
                    });

                    await selectScope(page, 'BETA');

                    // While the BETA config is pending: no stale ALPHA rows are
                    // rendered or actionable, and the loading hint shows.
                    await page.waitForSelector('[data-testid="scope-unavailable"]', { timeout: 10_000 });
                    await expect
                        .poll(() => page.locator('[data-testid="scope-unavailable"]').textContent())
                        .toContain('Loading sync settings');
                    await expect.poll(() => page.locator('.remote-row').count()).toBe(0);
                    await expect.poll(() => page.locator('.remote-provider:has-text("JAA")').count()).toBe(0);
                    await expect.poll(() => addRemote.isDisabled()).toBe(true);

                    // Real endpoints stay live: the BETA report list resolves
                    // (empty) through the untouched route.
                    await page.waitForSelector('text=No reports yet.', { timeout: 15_000 });

                    // Controlled failure for the race window alone: fulfill the
                    // held inspections with an explicit 500 envelope.
                    holding = false;
                    for (const route of held) {
                        await route.fulfill({
                            status: 500,
                            contentType: 'application/json',
                            body: JSON.stringify({ error: { code: 'INTERNAL', message: 'controlled scope load failure' } }),
                        });
                    }
                    await page.waitForSelector('[data-testid="scope-load-error"]', { timeout: 10_000 });
                    await expect
                        .poll(() => page.locator('[data-testid="scope-load-error"]').textContent())
                        .toContain('controlled scope load failure');

                    // A failed load never validates a scope: rows stay hidden,
                    // actions stay disabled, and no sync call ever left.
                    await expect.poll(() => page.locator('.remote-row').count()).toBe(0);
                    await expect
                        .poll(() => page.locator('[data-testid="scope-unavailable"]').textContent())
                        .toContain('unavailable (load failed or was superseded)');
                    await expect.poll(() => addRemote.isDisabled()).toBe(true);
                    expect(syncUrls.length).toBe(0);
                    expect(stubAlpha.allRequests().length).toBe(0);
                    expect(stubBeta.allRequests().length).toBe(0);

                    await page.unroute('**/api/config/inspect*');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await stubAlpha.stop();
            await stubBeta.stop();
        }
    });
});
