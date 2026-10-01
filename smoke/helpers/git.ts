import { execa } from 'execa';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

export interface GitCommandOptions {
    readonly env?: NodeJS.ProcessEnv;
    readonly stdio?: 'pipe' | 'inherit';
    readonly reject?: boolean;
    readonly timeout?: number;
}

export interface GitInitOptions extends GitCommandOptions {
    readonly name?: string;
    readonly email?: string;
    readonly initialBranch?: string;
    readonly initialCommitMessage?: string;
}

// Some sandboxed runtimes (e.g. certain agent harnesses) forbid creating
// anything named `.git`, which `git init` requires. Probe the real operation
// once — `git init` in an isolated temp dir plus a `.git` artifact check — so
// callers can skip git-dependent tests honestly instead of surfacing false
// negatives. Designated CI sets LOTAR_SMOKE_REQUIRE_GIT=1, which turns a
// failed probe into a fatal error instead of a skip (DEV-79).
export interface GitProbeResult {
    readonly available: boolean;
    readonly reason: string;
}

let gitProbeCache: GitProbeResult | undefined;

// The probe's repository writes and user/system Git config must stay isolated
// to its owned temp dir. Inherited GIT_* variables (GIT_DIR, GIT_WORK_TREE,
// GIT_OBJECT_DIRECTORY, GIT_CONFIG*/redirections, traces, ...) could reroute
// `git init` into repositories or object stores outside the probe dir, and
// inherited HOME/USERPROFILE/XDG_CONFIG_HOME entries under any casing could
// resurface a parent user config, so both groups are dropped before the
// probe adds its own empty global config, disables the system config, and
// installs the owned canonical values (DEV-80). Other parent entries (PATH
// included) pass through untouched; the git binary itself may still read its
// own program files and templates.
const ownedEnvKeys = new Set(['HOME', 'USERPROFILE', 'XDG_CONFIG_HOME']);

export function buildGitProbeEnv(parentEnv: NodeJS.ProcessEnv, ownedDir: string): NodeJS.ProcessEnv {
    const env: NodeJS.ProcessEnv = {};
    for (const [key, value] of Object.entries(parentEnv)) {
        if (value === undefined) {
            continue;
        }
        const upperKey = key.toUpperCase();
        if (upperKey.startsWith('GIT_') || ownedEnvKeys.has(upperKey)) {
            continue;
        }
        env[key] = value;
    }
    env.GIT_CONFIG_NOSYSTEM = '1';
    env.GIT_CONFIG_GLOBAL = join(ownedDir, 'gitconfig');
    env.HOME = ownedDir;
    env.XDG_CONFIG_HOME = join(ownedDir, '.config');
    env.USERPROFILE = ownedDir;
    return env;
}

export function probeGit(gitBinary?: string): GitProbeResult {
    let dir: string | undefined;
    try {
        dir = mkdtempSync(join(tmpdir(), 'lotar-git-probe-'));
        const repo = join(dir, 'repo');
        mkdirSync(repo);
        const gitconfig = join(dir, 'gitconfig');
        writeFileSync(gitconfig, '');
        const result = spawnSync(gitBinary ?? 'git', ['init', '--quiet', repo], {
            cwd: dir,
            env: buildGitProbeEnv(process.env, dir),
            timeout: 30_000,
            stdio: 'pipe',
            encoding: 'utf8',
        });
        if (result.error) {
            return { available: false, reason: `cannot run git: ${result.error.message}` };
        }
        if (result.status !== 0) {
            return {
                available: false,
                reason: `\`git init\` failed with status ${result.status}: ${(result.stderr ?? '').trim()}`,
            };
        }
        if (!existsSync(join(repo, '.git'))) {
            return {
                available: false,
                reason: '`git init` exited 0 but left no .git artifact',
            };
        }
        return { available: true, reason: '' };
    } catch (error) {
        return { available: false, reason: `git probe failed: ${String(error)}` };
    } finally {
        if (dir) {
            try {
                rmSync(dir, { recursive: true, force: true });
            } catch {
                // Ignore cleanup failures.
            }
        }
    }
}

export function gitProbe(): GitProbeResult {
    if (gitProbeCache === undefined) {
        gitProbeCache = probeGit();
    }
    return gitProbeCache;
}

export function assertGitSupport(probe: GitProbeResult): boolean {
    if (!probe.available && process.env.LOTAR_SMOKE_REQUIRE_GIT === '1') {
        throw new Error(
            `LOTAR_SMOKE_REQUIRE_GIT=1 but the Git capability probe failed: ${probe.reason}. ` +
                'Designated CI requires Git; failing honestly instead of skipping (DEV-79).',
        );
    }
    return probe.available;
}

export function gitAvailable(): boolean {
    return assertGitSupport(gitProbe());
}

export async function runGitCommand(
    cwd: string,
    args: readonly string[],
    options: GitCommandOptions = {},
) {
    return execa('git', args as string[], {
        cwd,
        env: options.env,
        stdio: options.stdio ?? 'pipe',
        reject: options.reject ?? true,
        timeout: options.timeout ?? 60_000,
    });
}

export async function initGitRepository(
    cwd: string,
    options: GitInitOptions = {},
): Promise<void> {
    // Pin the initial branch (default main) via the init flag so fixture
    // repositories are deterministic across runners regardless of the git
    // version, distribution default, or an inherited init.defaultBranch —
    // without writing to any global or system Git config. `--initial-branch`
    // needs git >= 2.28, which every supported runner ships. Specs that model
    // a non-main default branch rename it consciously (e.g. `branch -m trunk`).
    await runGitCommand(cwd, ['init', '--initial-branch', options.initialBranch ?? 'main'], options);

    const name = options.name ?? 'Smoke Tester';
    const email = options.email ?? 'smoke@example.com';
    await runGitCommand(cwd, ['config', 'user.name', name], options);
    await runGitCommand(cwd, ['config', 'user.email', email], options);

    // Ensure smoke repositories don't inherit host commit-signing requirements.
    await runGitCommand(cwd, ['config', 'commit.gpgsign', 'false'], options);
    await runGitCommand(cwd, ['config', 'tag.gpgSign', 'false'], options);

    await runGitCommand(cwd, ['add', '.'], options);

    const hasCommit = await runGitCommand(cwd, ['rev-parse', '--verify', 'HEAD'], {
        ...options,
        reject: false,
    });

    if (hasCommit.exitCode !== 0) {
        await runGitCommand(
            cwd,
            ['commit', '--allow-empty', '-m', options.initialCommitMessage ?? 'Initial commit'],
            options,
        );
    }

    await runGitCommand(cwd, ['config', 'core.worktree', cwd], options);
    await runGitCommand(cwd, ['config', 'pull.rebase', 'false'], options);
}
