import fs from 'fs-extra';
import path from 'node:path';
import process from 'node:process';
import { dir, DirectoryResult } from 'tmp-promise';
import { runLotar } from './cli.js';
import type { CliRunResult } from './cli.js';
import { initGitRepository, runGitCommand, type GitCommandOptions } from './git.js';

export interface WorkspaceOptions {
  readonly name?: string;
  readonly seedFiles?: Record<string, string>;
}

export interface LotarCommandOptions {
  readonly env?: NodeJS.ProcessEnv;
  readonly timeout?: number;
  readonly stdio?: 'pipe' | 'inherit';
  readonly acceptExitCodes?: readonly number[];
  readonly cwd?: string;
}

export interface AddTaskOptions extends LotarCommandOptions {
  readonly args?: readonly string[];
}

export interface CreatedTask {
  readonly id: string;
  readonly project: string;
  readonly sequence: string;
  readonly filePath: string;
  readonly result: CliRunResult;
}

export class SmokeWorkspace {
  static async create(options: WorkspaceOptions = {}): Promise<SmokeWorkspace> {
    const tmp = await dir({ prefix: options.name ?? 'lotar-smoke-', unsafeCleanup: true });
    const workspace = new SmokeWorkspace(tmp, options);
    await workspace.bootstrap(options.seedFiles ?? {});
    return workspace;
  }

  private constructor(
    private readonly temporary: DirectoryResult,
    private readonly options: WorkspaceOptions,
  ) {}

  get root(): string {
    return this.temporary.path;
  }

  get tasksDir(): string {
    return path.join(this.root, '.tasks');
  }

  get env(): NodeJS.ProcessEnv {
    return {
      ...process.env,
      LOTAR_TEST_SILENT: '1',
      LOTAR_TASKS_DIR: this.tasksDir,
      LOTAR_HOME: this.root,
    };
  }

  async bootstrap(seedFiles: Record<string, string>): Promise<void> {
    await fs.ensureDir(this.tasksDir);

    await Promise.all(
      Object.entries(seedFiles).map(async ([relative, contents]) => {
        const target = path.join(this.root, relative);
        await fs.ensureDir(path.dirname(target));
        await fs.writeFile(target, contents);
      }),
    );
  }

  async runLotar(args: readonly string[], options: LotarCommandOptions = {}) {
    if (process.env.SMOKE_DEBUG === '1') {
      const mergedEnv = {
        ...this.env,
        ...options.env,
      } as Record<string, string | undefined>;
      console.debug('[smoke] runLotar', {
        args,
        cwd: this.root,
        env: {
          LOTAR_TASKS_DIR: mergedEnv.LOTAR_TASKS_DIR,
          LOTAR_HOME: mergedEnv.LOTAR_HOME,
          LOTAR_TEST_SILENT: mergedEnv.LOTAR_TEST_SILENT,
          LOTAR_DEBUG_STATUS: mergedEnv.LOTAR_DEBUG_STATUS,
        },
      });
    }

    const cwd = options.cwd ?? this.root;
    return runLotar(args, {
      cwd,
      env: {
        ...this.env,
        ...options.env,
      },
      stdio: options.stdio,
      timeout: options.timeout,
      acceptExitCodes: options.acceptExitCodes,
    });
  }

  async runLotarIn(relativePath: string, args: readonly string[], options: LotarCommandOptions = {}) {
    const target = path.resolve(this.root, relativePath);
    return this.runLotar(args, {
      ...options,
      cwd: target,
    });
  }

  async addTask(title: string, options: AddTaskOptions = {}): Promise<CreatedTask> {
    const existingFiles = new Set(await this.listTaskFiles());

    const args: string[] = ['task', 'add', title];
    if (options.args?.length) {
      args.push(...options.args);
    }

    const result = await this.runLotar(args, options);
    if (process.env.SMOKE_DEBUG === '1') {
      console.debug('[smoke] workspace context', {
        root: this.root,
        tasksDir: this.tasksDir,
      });
    }
    const id = SmokeWorkspace.extractCreatedTaskId(result.stdout);
    const { project, sequence } = SmokeWorkspace.splitTaskId(id);
    const candidatePath = path.join(this.tasksDir, project, `${sequence}.yml`);

    let filePath: string | null = null;
    const attempts = 20;
    for (let i = 0; i < attempts; i += 1) {
      if (await fs.pathExists(candidatePath)) {
        filePath = candidatePath;
        break;
      }

      const nextFiles = await this.listTaskFiles();
      const newFiles = nextFiles.filter((file) => !existingFiles.has(file));
      if (process.env.SMOKE_DEBUG === '1') {
        console.debug('[smoke] addTask poll', {
          attempt: i,
          candidatePath,
          nextFiles,
          newFiles,
        });
      }
      if (newFiles.length) {
        filePath = newFiles[0];
        break;
      }

      await new Promise((resolve) => setTimeout(resolve, 50));
    }

    if (!filePath) {
      throw new Error(
        `Expected task file to be created for ${id}, but none was found.\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
      );
    }

    return { id, project, sequence, filePath, result };
  }

  async readTaskYaml(taskId: string): Promise<string> {
    const filePath = this.getTaskFilePath(taskId);
    return fs.readFile(filePath, 'utf8');
  }

  getTaskFilePath(taskId: string): string {
    const { project, sequence } = SmokeWorkspace.splitTaskId(taskId);
    return path.join(this.tasksDir, project, `${sequence}.yml`);
  }

  async listTaskFiles(): Promise<string[]> {
    const entries = await fs.readdir(this.tasksDir);
    const files: string[] = [];

    for (const entry of entries) {
      const projectPath = path.join(this.tasksDir, entry);
      const stat = await fs.stat(projectPath);
      if (!stat.isDirectory()) {
        continue;
      }

      const candidates = await fs.readdir(projectPath);
      for (const candidate of candidates) {
        if (candidate.endsWith('.yml') && candidate !== 'config.yml') {
          files.push(path.join(projectPath, candidate));
        }
      }
    }
    files.sort();
    return files;
  }

  async initGit(options: { name?: string; email?: string } = {}): Promise<void> {
    await initGitRepository(this.root, options);
  }

  async runGit(args: readonly string[], options: GitCommandOptions = {}) {
    if (process.env.SMOKE_DEBUG === '1') {
      console.debug('[smoke] runGit', {
        args,
        cwd: this.root,
      });
    }
    return runGitCommand(this.root, args, options);
  }

  async commitAll(message: string, options: GitCommandOptions = {}) {
    await this.runGit(['add', '--all'], options);
    return this.runGit(['commit', '-m', message], options);
  }

  async write(relativePath: string, contents: string): Promise<void> {
    const target = path.join(this.root, relativePath);
    await fs.ensureDir(path.dirname(target));
    await fs.writeFile(target, contents);
  }

  async read(relativePath: string): Promise<string> {
    const target = path.join(this.root, relativePath);
    return fs.readFile(target, 'utf8');
  }

  async remove(relativePath: string): Promise<void> {
    const target = path.join(this.root, relativePath);
    await fs.remove(target);
  }

  async dispose(): Promise<void> {
    await this.temporary.cleanup();
  }

  private static extractCreatedTaskId(stdout: string): string {
    const match = stdout.match(/Created task:\s*([A-Z0-9_-]+)/i);
    if (!match || !match[1]) {
      throw new Error(`Unable to parse task identifier from CLI output:\n${stdout}`);
    }
    return match[1].trim();
  }

  private static splitTaskId(taskId: string): { project: string; sequence: string } {
    const trimmed = taskId.trim();
    const lastDash = trimmed.lastIndexOf('-');
    if (lastDash <= 0 || lastDash === trimmed.length - 1) {
      throw new Error(`Unexpected task identifier format: ${taskId}`);
    }
    const project = trimmed.slice(0, lastDash);
    const sequence = trimmed.slice(lastDash + 1);
    return { project, sequence };
  }
}
