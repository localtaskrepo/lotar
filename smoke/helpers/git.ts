import { execa } from 'execa';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export interface GitCommandOptions {
    readonly env?: NodeJS.ProcessEnv;
    readonly stdio?: 'pipe' | 'inherit';
    readonly reject?: boolean;
    readonly timeout?: number;
}

export interface GitInitOptions extends GitCommandOptions {
    readonly name?: string;
    readonly email?: string;
    readonly initialCommitMessage?: string;
}

// Some sandboxed runtimes (e.g. certain agent harnesses) forbid creating
// anything named `.git`, which `git init` requires. Probe that once so callers
// can skip git-dependent tests instead of surfacing them as false negatives.
let gitAvailableCache: boolean | undefined;

export function gitAvailable(): boolean {
    if (gitAvailableCache !== undefined) {
        return gitAvailableCache;
    }
    let dir: string | undefined;
    try {
        dir = mkdtempSync(join(tmpdir(), 'lotar-git-probe-'));
        mkdirSync(join(dir, '.git'));
        gitAvailableCache = true;
    } catch {
        gitAvailableCache = false;
    } finally {
        if (dir) {
            try {
                rmSync(dir, { recursive: true, force: true });
            } catch {
                // Ignore cleanup failures.
            }
        }
    }
    return gitAvailableCache;
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
    await runGitCommand(cwd, ['init'], options);

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
