import { execa } from 'execa';

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
