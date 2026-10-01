import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
    assertGitSupport,
    buildGitProbeEnv,
    probeGit,
    type GitProbeResult,
} from '../helpers/git.js';

const isUnix = process.platform !== 'win32';
const fakeDirs: string[] = [];

function writeFakeGit(script: string): string {
    const dir = mkdtempSync(join(tmpdir(), 'lotar-git-fake-'));
    fakeDirs.push(dir);
    const git = join(dir, 'git');
    writeFileSync(git, script);
    chmodSync(git, 0o755);
    return git;
}

afterEach(() => {
    while (fakeDirs.length > 0) {
        const dir = fakeDirs.pop();
        if (dir) {
            rmSync(dir, { recursive: true, force: true });
        }
    }
    vi.unstubAllEnvs();
});

const ownedEnvCaseKeys = ['HOME', 'USERPROFILE', 'XDG_CONFIG_HOME'] as const;

const redirectedEnvKeys = [
    'GIT_DIR',
    'GIT_WORK_TREE',
    'GIT_COMMON_DIR',
    'GIT_INDEX_FILE',
    'GIT_OBJECT_DIRECTORY',
    'GIT_ALTERNATE_OBJECT_DIRECTORIES',
    'GIT_CONFIG_COUNT',
    'GIT_CONFIG_KEY_0',
    'GIT_CONFIG_VALUE_0',
    'GIT_TRACE',
] as const;

describe('smoke git capability probe', () => {
    it('reports a missing git binary as unavailable with a reason', () => {
        const missing = join(tmpdir(), `lotar-git-missing-${process.pid}`, 'git');
        const probe = probeGit(missing);
        expect(probe.available).toBe(false);
        expect(probe.reason).toContain('git');
    });

    it('returns a consistent verdict for repeated probes', () => {
        const first = probeGit();
        const second = probeGit();
        expect(second.available).toBe(first.available);
        if (!first.available) {
            expect(second.reason).not.toBe('');
        }
    });

    it('does not throw for unavailable git unless required', () => {
        vi.stubEnv('LOTAR_SMOKE_REQUIRE_GIT', undefined);
        const unavailable: GitProbeResult = { available: false, reason: 'unit-test reason' };
        expect(assertGitSupport(unavailable)).toBe(false);
    });

    it('throws when git is required but unavailable', () => {
        vi.stubEnv('LOTAR_SMOKE_REQUIRE_GIT', '1');
        const unavailable: GitProbeResult = { available: false, reason: 'unit-test reason' };
        expect(() => assertGitSupport(unavailable)).toThrow(/LOTAR_SMOKE_REQUIRE_GIT/);
    });

    it('keeps available probes usable under the require flag', () => {
        vi.stubEnv('LOTAR_SMOKE_REQUIRE_GIT', '1');
        expect(assertGitSupport({ available: true, reason: '' })).toBe(true);
    });

    it('builds a probe env free of inherited GIT_* and home redirection', () => {
        const ownedDir = join(tmpdir(), 'lotar-probe-owned');
        const parentEnv: NodeJS.ProcessEnv = {
            PATH: '/usr/bin:/bin',
            HOME: '/home/parent',
            home: '/home/parent-lower',
            Home: '/home/parent-mixed',
            USERPROFILE: 'C:\\Users\\parent',
            userprofile: 'C:\\Users\\parent-lower',
            XDG_CONFIG_HOME: '/home/parent/.config',
            xdg_config_home: '/home/parent/.config-lower',
            GIT_CONFIG: '/outside/gitconfig',
            GIT_EXEC_PATH: '/outside/libexec',
            git_index_file: '/outside/lowercased',
        };
        for (const key of redirectedEnvKeys) {
            parentEnv[key] = `/outside/${key.toLowerCase()}`;
        }
        parentEnv.GIT_CONFIG_COUNT = '1';
        parentEnv.GIT_CONFIG_KEY_0 = 'core.hooksPath';
        parentEnv.GIT_CONFIG_VALUE_0 = '/outside/hooks';

        const env = buildGitProbeEnv(parentEnv, ownedDir);

        expect(env.PATH).toBe('/usr/bin:/bin');
        for (const key of Object.keys(parentEnv)) {
            if (key.toUpperCase().startsWith('GIT_')) {
                expect(env[key]).toBeUndefined();
            }
        }
        const gitKeys = Object.keys(env)
            .filter((key) => key.toUpperCase().startsWith('GIT_'))
            .sort();
        expect(gitKeys).toEqual(['GIT_CONFIG_GLOBAL', 'GIT_CONFIG_NOSYSTEM']);
        expect(env.GIT_CONFIG_NOSYSTEM).toBe('1');
        expect(env.GIT_CONFIG_GLOBAL).toBe(join(ownedDir, 'gitconfig'));
        expect(env.home).toBeUndefined();
        expect(env.Home).toBeUndefined();
        expect(env.userprofile).toBeUndefined();
        expect(env.xdg_config_home).toBeUndefined();
        expect(env.HOME).toBe(ownedDir);
        expect(env.USERPROFILE).toBe(ownedDir);
        expect(env.XDG_CONFIG_HOME).toBe(join(ownedDir, '.config'));
        for (const key of ownedEnvCaseKeys) {
            expect(Object.keys(env).filter((entry) => entry.toUpperCase() === key)).toEqual([
                key,
            ]);
        }
    });

    it.skipIf(!isUnix)('captures only whitelisted probe-child env without GIT_* redirection', () => {
        const dumpDir = mkdtempSync(join(tmpdir(), 'lotar-git-envdump-'));
        fakeDirs.push(dumpDir);
        const dumpPath = join(dumpDir, 'child-env.txt');
        vi.stubEnv('LOTAR_PROBE_ENV_DUMP', dumpPath);
        for (const key of redirectedEnvKeys) {
            vi.stubEnv(key, `/outside/${key.toLowerCase()}`);
        }
        const captureKeys: readonly string[] = [
            ...redirectedEnvKeys,
            ...ownedEnvCaseKeys,
            'GIT_CONFIG_GLOBAL',
            'GIT_CONFIG_NOSYSTEM',
            'LOTAR_PROBE_ENV_DUMP',
        ];
        const git = writeFakeGit(
            [
                '#!/bin/sh',
                ...captureKeys.map(
                    (key) =>
                        `if [ -n "\${${key}+x}" ]; then printf '${key}=%s\\n' "$${key}" >> "$LOTAR_PROBE_ENV_DUMP"; fi`,
                ),
                'if [ -f "$GIT_CONFIG_GLOBAL" ]; then',
                '    echo global-config-present >> "$LOTAR_PROBE_ENV_DUMP"',
                'fi',
                'exit 0',
            ].join('\n') + '\n',
        );

        const probe = probeGit(git);

        expect(probe.available).toBe(false);
        expect(probe.reason).toContain('.git');
        const lines = readFileSync(dumpPath, 'utf8').split('\n');
        for (const key of redirectedEnvKeys) {
            expect(lines.some((line) => line.startsWith(`${key}=`))).toBe(false);
        }
        expect(lines).toContain('GIT_CONFIG_NOSYSTEM=1');
        expect(lines).toContain('global-config-present');
        expect(lines.some((line) => line.startsWith('LOTAR_PROBE_ENV_DUMP='))).toBe(true);
        expect(lines.find((line) => line.startsWith('GIT_CONFIG_GLOBAL='))).toMatch(
            /lotar-git-probe-[^/]*\/gitconfig$/,
        );
        for (const key of ownedEnvCaseKeys) {
            expect(lines.find((line) => line.startsWith(`${key}=`))).toMatch(
                /lotar-git-probe-/,
            );
        }
        expect(lines.find((line) => line.startsWith('HOME='))).not.toBe(
            `HOME=${process.env.HOME}`,
        );
        const allowedPrefixes = captureKeys.map((key) => `${key}=`);
        const unexpected = lines.filter(
            (line) =>
                line.length > 0 &&
                line !== 'global-config-present' &&
                !allowedPrefixes.some((prefix) => line.startsWith(prefix)),
        );
        expect(unexpected).toEqual([]);
    });

    it.skipIf(!isUnix)('rejects a fake git that exits zero without creating .git', () => {
        const git = writeFakeGit('#!/bin/sh\nexit 0\n');
        const probe = probeGit(git);
        expect(probe.available).toBe(false);
        expect(probe.reason).toContain('.git');
    });

    it.skipIf(!isUnix)('surfaces fake git failure diagnostics', () => {
        const git = writeFakeGit('#!/bin/sh\necho smoke-probe-boom >&2\nexit 3\n');
        const probe = probeGit(git);
        expect(probe.available).toBe(false);
        expect(probe.reason).toContain('smoke-probe-boom');
    });
});
