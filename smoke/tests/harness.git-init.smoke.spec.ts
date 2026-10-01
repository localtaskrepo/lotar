import { mkdirSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { gitAvailable, initGitRepository, runGitCommand } from '../helpers/git.js';

// Durable regression for the CI branch-dependency failures (cli.advanced /
// ui.advanced `git checkout main` pathspec errors): fixture repositories must
// initialize a deterministic `main` branch via the init flag itself, never by
// writing init.defaultBranch into any global or system Git config. The hostile
// inherited config lives inside an isolated sandbox owned by this test, so the
// real git invocations below stay confined to temporary workspaces.
const HOSTILE_DEFAULT_BRANCH = 'hostile-trunk';

const sandboxes: string[] = [];

function makeSandbox(): { sandbox: string; home: string; env: NodeJS.ProcessEnv } {
    const sandbox = mkdtempSync(join(tmpdir(), 'lotar-git-init-'));
    sandboxes.push(sandbox);
    const home = join(sandbox, 'home');
    mkdirSync(home, { recursive: true });
    // GIT_CONFIG_GLOBAL is authoritative for global config reads and writes,
    // and HOME/USERPROFILE/XDG_CONFIG_HOME are redirected into the sandbox as
    // defense in depth so no fixture git invocation can touch the real user
    // environment even if the redirect were ignored.
    const gitconfig = join(home, 'gitconfig');
    writeFileSync(gitconfig, `[init]\n\tdefaultBranch = ${HOSTILE_DEFAULT_BRANCH}\n`);
    const configHome = join(home, '.config');
    const env: NodeJS.ProcessEnv = {
        ...process.env,
        HOME: home,
        USERPROFILE: home,
        XDG_CONFIG_HOME: configHome,
        GIT_CONFIG_NOSYSTEM: '1',
        GIT_CONFIG_GLOBAL: gitconfig,
    };
    return { sandbox, home, env };
}

function snapshotTree(root: string): Map<string, string> {
    const snapshot = new Map<string, string>();
    const walk = (dir: string) => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
            const child = join(dir, entry.name);
            if (entry.isDirectory()) {
                walk(child);
                continue;
            }
            const stat = statSync(child);
            snapshot.set(child, `${stat.size}:${stat.mtimeMs}`);
        }
    };
    walk(root);
    return snapshot;
}

async function currentBranch(cwd: string, env: NodeJS.ProcessEnv): Promise<string> {
    const result = await runGitCommand(cwd, ['symbolic-ref', 'HEAD'], { env });
    return (result.stdout ?? '').trim();
}

afterEach(() => {
    while (sandboxes.length > 0) {
        const sandbox = sandboxes.pop();
        if (sandbox) {
            rmSync(sandbox, { recursive: true, force: true });
        }
    }
});

describe.skipIf(!gitAvailable())('smoke git fixture init', () => {
    it('initializes main deterministically under a hostile inherited init.defaultBranch', async () => {
        const { sandbox, home, env } = makeSandbox();

        // The control repo proves the hostile inheritance is genuinely in
        // effect for a plain init: without the fixture's explicit flag this
        // branch name is exactly what initGitRepository would produce.
        const control = join(sandbox, 'control');
        mkdirSync(control);
        await runGitCommand(control, ['init'], { env });
        expect(await currentBranch(control, env)).toBe(`refs/heads/${HOSTILE_DEFAULT_BRANCH}`);

        const externalBefore = snapshotTree(home);

        const repo = join(sandbox, 'repo');
        mkdirSync(repo);
        await initGitRepository(repo, { env });

        expect(await currentBranch(repo, env)).toBe('refs/heads/main');

        // The fixture's initial commit landed on the deterministic branch.
        const head = await runGitCommand(repo, ['rev-parse', '--verify', 'HEAD'], { env });
        expect(head.exitCode).toBe(0);

        // Zero external Git writes: the isolated home tree (the only area
        // outside the fixture repositories reachable through the redirected
        // environment) is byte-for-byte unchanged, so the deterministic branch
        // never came from mutating a global config.
        expect(snapshotTree(home)).toEqual(externalBefore);
    });

    it('honors an explicit initialBranch override', async () => {
        const { sandbox, env } = makeSandbox();

        const repo = join(sandbox, 'repo');
        mkdirSync(repo);
        await initGitRepository(repo, { env, initialBranch: 'smoke-trunk' });

        expect(await currentBranch(repo, env)).toBe('refs/heads/smoke-trunk');
    });
});
