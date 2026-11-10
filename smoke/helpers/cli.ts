import { execa, type Result } from 'execa';
import process from 'node:process';
import { ensureBinaryExists } from './binary.js';

export interface CliRunOptions {
    readonly env?: NodeJS.ProcessEnv;
    readonly cwd?: string;
    readonly timeout?: number;
    readonly acceptExitCodes?: readonly number[];
}

// Output is always piped and decoded, so stdout and stderr are strings.
export type CliRunResult = Result<{ reject: false }>;

export async function runLotar(args: readonly string[], options: CliRunOptions): Promise<CliRunResult> {
    const binary = await ensureBinaryExists();

    const child = execa(binary, args as string[], {
        cwd: options.cwd,
        env: {
            LOTAR_TEST_SILENT: '1',
            RUST_TEST_THREADS: undefined,
            ...process.env,
            ...options.env,
        },
        stdio: 'pipe',
        timeout: options.timeout ?? 60_000,
        reject: false,
    });

    const result = await child;

    const acceptable = options.acceptExitCodes ?? [];

    if (result.exitCode !== 0 && !acceptable.includes(result.exitCode ?? -1)) {
        throw new Error(
            `lotar ${args.join(' ')} failed with exit code ${result.exitCode}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
        );
    }

    return result;
}
