#!/usr/bin/env node
// Install a build from source (`npm run install:local`, after `npm run build`)
// and on macOS sign it first, so macOS keeps the firewall and file-access
// grants it ties to the signature across new builds (DEV-563).
//
// Both binaries are copied into staging files next to their destination,
// signed and checked there, and only then renamed over the installed ones:
// a failed step leaves both old binaries in place, and a running server
// keeps the file it started from.
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync, realpathSync, renameSync, rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// The release's identity and identifier (DEV-482), so a source build and a
// release are the same app to macOS.
export const DEFAULT_IDENTITY = 'Developer ID Application: Ravi Gairola (3HKD37M482)';
export const BINARIES = [
    { name: 'lotar', identifier: 'net.pyxzl.lotar' },
    { name: 'lotar-agent-wrapper', identifier: 'net.pyxzl.lotar-agent-wrapper' },
];

export function defaultInstallDir(platform, env) {
    if (platform === 'win32') return join(env.LOCALAPPDATA ?? join(homedir(), 'AppData', 'Local'), 'Programs', 'lotar');
    return join(homedir(), '.local', 'bin');
}

// `security find-identity -v -p codesigning` lists `  2) <SHA-1> "<name>"`.
export function parseIdentities(output) {
    const identities = [];
    for (const line of output.split('\n')) {
        const match = /^\s*\d+\)\s+([0-9A-Fa-f]{40})\s+"(.+)"\s*$/.exec(line);
        if (match) identities.push({ hash: match[1].toUpperCase(), name: match[2] });
    }
    return identities;
}

// Returns the codesign `--sign` value and a warning to print, or throws when
// an identity set with LOTAR_SIGNING_IDENTITY is not in the keychain: an
// explicit choice is never downgraded to ad hoc.
export function chooseIdentity(requested, identities) {
    const has = (wanted) => identities.some((id) => id.name === wanted || id.hash === wanted.toUpperCase());
    if (requested === '-') return { sign: '-', warning: null };
    if (requested) {
        if (has(requested)) return { sign: requested, warning: null };
        throw new Error(
            `LOTAR_SIGNING_IDENTITY "${requested}" is not a valid code-signing identity in the keychain ` +
                '(security find-identity -v -p codesigning); nothing was installed.',
        );
    }
    if (has(DEFAULT_IDENTITY)) return { sign: DEFAULT_IDENTITY, warning: null };
    return {
        sign: '-',
        warning:
            `warning: signing identity "${DEFAULT_IDENTITY}" not found, so the build is signed ad hoc. ` +
            'macOS treats it as a new app and asks for firewall and file-access grants again. ' +
            'Set LOTAR_SIGNING_IDENTITY to use another identity.',
    };
}

// codesign waits without end while macOS asks at the Mac whether it may use
// the signing key, so an unattended install would hang.
export const COMMAND_TIMEOUT_MS = 120_000;

function runCommand(command, args) {
    const result = spawnSync(command, args, { encoding: 'utf8', timeout: COMMAND_TIMEOUT_MS });
    return {
        status: result.error || result.signal ? null : result.status,
        stdout: result.stdout ?? '',
        stderr: result.stderr ?? '',
        error: result.error,
        timedOut: result.error?.code === 'ETIMEDOUT',
    };
}

function check(result, what) {
    if (result.status === 0) return result;
    if (result.timedOut) {
        throw new Error(
            `${what} did not finish within ${COMMAND_TIMEOUT_MS / 1000} s: macOS is probably asking at the Mac ` +
                'whether codesign may use the signing key. Choose Always Allow there, then run the install again.',
        );
    }
    const detail = (result.stderr || result.stdout || result.error?.message || '').trim();
    throw new Error(`${what} failed${detail ? `:\n${detail}` : ''}`);
}

export function installLocal({
    sourceDir,
    installDir,
    platform = process.platform,
    env = process.env,
    run = runCommand,
    log = console.log,
    warn = console.error,
}) {
    const exe = platform === 'win32' ? '.exe' : '';
    const files = BINARIES.map((binary) => ({
        ...binary,
        source: join(sourceDir, binary.name + exe),
        target: join(installDir, binary.name + exe),
        staged: join(installDir, `.${binary.name}${exe}.install-${process.pid}`),
    }));
    for (const file of files) {
        if (!existsSync(file.source)) throw new Error(`${file.source} is missing: run npm run build first.`);
    }

    let identity = null;
    if (platform === 'darwin') {
        const listed = run('security', ['find-identity', '-v', '-p', 'codesigning']);
        identity = chooseIdentity(
            env.LOTAR_SIGNING_IDENTITY,
            listed.status === 0 ? parseIdentities(listed.stdout) : [],
        );
        if (identity.warning) warn(identity.warning);
    }

    mkdirSync(installDir, { recursive: true });
    try {
        for (const file of files) {
            copyFileSync(file.source, file.staged);
            chmodSync(file.staged, 0o755);
            if (identity) {
                // A Developer ID signature gets a secure timestamp, as in the
                // release; ad hoc can't have one.
                const timestamp = identity.sign === '-' ? '--timestamp=none' : '--timestamp';
                check(
                    run('codesign', [
                        '--force',
                        '--options',
                        'runtime',
                        timestamp,
                        '--identifier',
                        file.identifier,
                        '--sign',
                        identity.sign,
                        file.staged,
                    ]),
                    `codesign of ${file.name}`,
                );
                check(run('codesign', ['--verify', '--strict', file.staged]), `codesign --verify of ${file.name}`);
            }
        }
        // The wrapper has no --version: it starts the new lotar's, so this
        // checks that both new binaries start.
        const [lotar, wrapper] = files;
        check(run(wrapper.staged, ['--', lotar.staged, '--version']), 'the new lotar-agent-wrapper -- lotar --version');
        for (const file of files) renameSync(file.staged, file.target);
    } catch (error) {
        for (const file of files) {
            // codesign signs into `<file>.cstemp` and leaves it when it is stopped.
            rmSync(file.staged, { force: true });
            rmSync(`${file.staged}.cstemp`, { force: true });
        }
        throw error;
    }

    const how = !identity ? 'unsigned' : identity.sign === '-' ? 'signed ad hoc' : `signed with "${identity.sign}"`;
    log(`Installed ${files.map((file) => file.target).join(' and ')} (${how}).`);
    const onPath = (env.PATH ?? '').split(delimiter).some((dir) => dir && resolve(dir) === resolve(installDir));
    if (!onPath) warn(`warning: ${installDir} is not on PATH.`);
}

// Importing the module (unit tests) must not install anything.
function invokedAsScript() {
    if (!process.argv[1]) return false;
    try {
        return realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url));
    } catch {
        return false;
    }
}

if (invokedAsScript()) {
    try {
        installLocal({
            sourceDir: fileURLToPath(new URL('../target/release', import.meta.url)),
            installDir: process.env.LOTAR_INSTALL_DIR || defaultInstallDir(process.platform, process.env),
        });
    } catch (error) {
        console.error(`install:local: ${error.message}`);
        process.exit(1);
    }
}
