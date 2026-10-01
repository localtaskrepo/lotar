#!/usr/bin/env node
// Self-tests for scripts/rust-test-runner.mjs (DEV-79).
// Run with: node --test scripts/tests/rust-test-runner.test.mjs
// Pure builder/plan coverage plus fake-launcher probe fixtures; no network,
// no repository mutations (temporary probe directories use the OS temp dir).

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';

import {
  RunnerError,
  buildProbeEnv,
  parseWrapperArgs,
  validateRequireGitEnv,
  resolveExplicitProfile,
  resolvePlan,
  buildRunArgs,
  inventoryAllowed,
  buildInventoryArgs,
  parseListTestNames,
  probeGitCapability,
  main,
} from '../rust-test-runner.mjs';

const AVAILABLE = { available: true, reason: '' };
const UNAVAILABLE = { available: false, reason: 'denied by sandbox' };

test('parse: passthrough keeps nextest args verbatim', () => {
  const parsed = parseWrapperArgs(['--cargo-profile', 'ci', '--retries', '0']);
  assert.deepEqual(parsed.passthrough, ['--cargo-profile', 'ci', '--retries', '0']);
  assert.equal(parsed.explicitProfile, null);
  assert.equal(parsed.agent, false);
  assert.equal(parsed.dryRun, false);
  assert.equal(parsed.hasFilterset, false);
  assert.equal(parsed.hasIgnoreDefaultFilter, false);
  assert.equal(parsed.hasPositionalFilter, false);
});

test('parse: --cargo-profile is not a nextest profile', () => {
  const parsed = parseWrapperArgs(['--cargo-profile', 'ci']);
  assert.equal(parsed.explicitProfile, null);
});

test('parse: wrapper flags are consumed, not forwarded', () => {
  const parsed = parseWrapperArgs(['--agent', '--dry-run', '--cargo-profile', 'ci']);
  assert.equal(parsed.agent, true);
  assert.equal(parsed.dryRun, true);
  assert.deepEqual(parsed.passthrough, ['--cargo-profile', 'ci']);
});

test('parse: profile spellings -P, --profile, --profile=x, -Px', () => {
  assert.equal(parseWrapperArgs(['-P', 'gitless']).explicitProfile, 'gitless');
  assert.equal(parseWrapperArgs(['--profile', 'perf']).explicitProfile, 'perf');
  assert.equal(parseWrapperArgs(['--profile=perf']).explicitProfile, 'perf');
  assert.equal(parseWrapperArgs(['-Pperf']).explicitProfile, 'perf');
});

test('parse: duplicate --profile flags, last wins', () => {
  const parsed = parseWrapperArgs(['--profile', 'default', '--profile', 'perf']);
  assert.equal(parsed.explicitProfile, 'perf');
});

test('parse: malformed --profile errors', () => {
  assert.throws(() => parseWrapperArgs(['--profile']), RunnerError);
  assert.throws(() => parseWrapperArgs(['-P']), RunnerError);
  assert.throws(() => parseWrapperArgs(['--profile', '--', 'x']), RunnerError);
});

test('parse: --force-probe validation and dry-run-only rule', () => {
  assert.throws(() => parseWrapperArgs(['--force-probe', 'maybe']), RunnerError);
  assert.throws(() => parseWrapperArgs(['--force-probe=yes']), RunnerError);
  assert.throws(() => parseWrapperArgs(['--force-probe', 'unavailable']), RunnerError);
  const ok = parseWrapperArgs(['--dry-run', '--force-probe', 'unavailable']);
  assert.equal(ok.forceProbe, 'unavailable');
  const okCompact = parseWrapperArgs(['--dry-run', '--force-probe=available']);
  assert.equal(okCompact.forceProbe, 'available');
});

test('parse: tokens after -- are never wrapper or profile flags', () => {
  const parsed = parseWrapperArgs(['--', '-P', 'gitless', '--profile=perf', '--agent']);
  assert.equal(parsed.explicitProfile, null);
  assert.equal(parsed.agent, false);
  assert.equal(parsed.sawSeparator, true);
  assert.deepEqual(parsed.passthrough, ['--', '-P', 'gitless', '--profile=perf', '--agent']);
});

test('parse: multiple -E preserved in order, never rewritten', () => {
  const parsed = parseWrapperArgs(['-E', 'test(a)', '-E', 'test(b)', '--filterset', 'all()']);
  assert.deepEqual(parsed.passthrough, ['-E', 'test(a)', '-E', 'test(b)', '--filterset', 'all()']);
  assert.equal(parsed.hasFilterset, true);
});

test('parse: --ignore-default-filter and positional filters detected', () => {
  const parsed = parseWrapperArgs(['--ignore-default-filter', 'some_test_name']);
  assert.equal(parsed.hasIgnoreDefaultFilter, true);
  assert.equal(parsed.hasPositionalFilter, true);
});

test('require-git env: strict build.rs semantics', () => {
  assert.equal(validateRequireGitEnv(undefined), 'off');
  assert.equal(validateRequireGitEnv(''), 'off');
  assert.equal(validateRequireGitEnv('1'), 'required');
  assert.throws(() => validateRequireGitEnv('yes'), RunnerError);
  assert.throws(() => validateRequireGitEnv('true'), RunnerError);
  assert.throws(() => validateRequireGitEnv('0'), RunnerError);
});

test('explicit profile: env used when no flag, flag beats env, empty ignored', () => {
  const parsed = parseWrapperArgs([]);
  assert.equal(resolveExplicitProfile(parsed, 'gitless'), 'gitless');
  assert.equal(resolveExplicitProfile(parsed, ''), null);
  assert.equal(resolveExplicitProfile(parsed, undefined), null);
  const withFlag = parseWrapperArgs(['--profile', 'perf']);
  assert.equal(resolveExplicitProfile(withFlag, 'gitless'), 'perf');
});

test('plan: available capability keeps default profile untouched', () => {
  const parsed = parseWrapperArgs(['--cargo-profile', 'ci']);
  const plan = resolvePlan({ probe: AVAILABLE, requireGit: 'off', explicitProfile: null, parsed });
  assert.equal(plan.gitlessSelected, false);
  assert.equal(plan.injectProfile, false);
  assert.deepEqual(plan.warnings, []);
});

test('plan: unavailable capability auto-selects gitless injection', () => {
  const parsed = parseWrapperArgs(['--cargo-profile', 'ci']);
  const plan = resolvePlan({ probe: UNAVAILABLE, requireGit: 'off', explicitProfile: null, parsed });
  assert.equal(plan.gitlessSelected, true);
  assert.equal(plan.injectProfile, true);
  assert.deepEqual(plan.warnings, []);
});

test('plan: LOTAR_REQUIRE_GIT=1 aborts before selection when probe fails', () => {
  const parsed = parseWrapperArgs([]);
  assert.throws(
    () => resolvePlan({ probe: UNAVAILABLE, requireGit: 'required', explicitProfile: null, parsed }),
    /designated full-coverage/,
  );
});

test('plan: require-git rejects explicit gitless exclusion mode', () => {
  const parsed = parseWrapperArgs(['--profile', 'gitless']);
  assert.throws(
    () => resolvePlan({ probe: AVAILABLE, requireGit: 'required', explicitProfile: 'gitless', parsed }),
    /forbids the gitless exclusion profile/,
  );
});

test('plan: --ignore-default-filter rejected without capability', () => {
  const parsed = parseWrapperArgs(['--ignore-default-filter']);
  assert.throws(
    () => resolvePlan({ probe: UNAVAILABLE, requireGit: 'off', explicitProfile: null, parsed }),
    /--ignore-default-filter/,
  );
});

test('plan: --ignore-default-filter allowed with capability', () => {
  const parsed = parseWrapperArgs(['--ignore-default-filter']);
  const plan = resolvePlan({ probe: AVAILABLE, requireGit: 'off', explicitProfile: null, parsed });
  assert.equal(plan.gitlessSelected, false);
});

test('plan: explicit non-gitless profile respected with warning, no override', () => {
  const parsed = parseWrapperArgs(['--profile', 'perf']);
  const plan = resolvePlan({ probe: UNAVAILABLE, requireGit: 'off', explicitProfile: 'perf', parsed });
  assert.equal(plan.gitlessSelected, false);
  assert.equal(plan.injectProfile, false);
  assert.equal(plan.warnings.length, 1);
  assert.match(plan.warnings[0], /explicit profile perf respected/);
});

test('plan: explicit gitless via NEXTEST_PROFILE runs gitless without injection', () => {
  const parsed = parseWrapperArgs([]);
  const explicit = resolveExplicitProfile(parsed, 'gitless');
  const plan = resolvePlan({ probe: UNAVAILABLE, requireGit: 'off', explicitProfile: explicit, parsed });
  assert.equal(plan.gitlessSelected, true);
  assert.equal(plan.injectProfile, false);
});

test('build: agent mode reproduces the old test:rust:agent contract', () => {
  const parsed = parseWrapperArgs(['--agent']);
  const plan = resolvePlan({ probe: AVAILABLE, requireGit: 'off', explicitProfile: null, parsed });
  assert.deepEqual(buildRunArgs(parsed, plan), [
    '--cargo-profile',
    'ci',
    '--color',
    'never',
    '--failure-output',
    'immediate-final',
    '--retries',
    '0',
  ]);
});

test('build: gitless injection prepends the profile, user -E untouched', () => {
  const parsed = parseWrapperArgs(['--cargo-profile', 'ci', '-E', 'test(pure_case)']);
  const plan = resolvePlan({ probe: UNAVAILABLE, requireGit: 'off', explicitProfile: null, parsed });
  const args = buildRunArgs(parsed, plan);
  assert.deepEqual(args.slice(0, 2), ['--profile', 'gitless']);
  assert.deepEqual(args.slice(2), ['--cargo-profile', 'ci', '-E', 'test(pure_case)']);
  assert.equal(args.filter((a) => a === '-E').length, 1);
});

test('build: available capability injects nothing', () => {
  const parsed = parseWrapperArgs(['--cargo-profile', 'ci']);
  const plan = resolvePlan({ probe: AVAILABLE, requireGit: 'off', explicitProfile: null, parsed });
  assert.deepEqual(buildRunArgs(parsed, plan), ['--cargo-profile', 'ci']);
});

test('inventory gating: only selector-free runs get the detailed listing', () => {
  assert.equal(inventoryAllowed(parseWrapperArgs(['--cargo-profile', 'ci'])), true);
  assert.equal(inventoryAllowed(parseWrapperArgs(['--cargo-profile', 'ci', '--tests'])), true);
  assert.equal(inventoryAllowed(parseWrapperArgs(['-E', 'test(x)'])), false);
  assert.equal(inventoryAllowed(parseWrapperArgs(['some_name'])), false);
  assert.equal(inventoryAllowed(parseWrapperArgs(['--', 'name'])), false);
  assert.equal(inventoryAllowed(parseWrapperArgs(['--ignore-default-filter'])), false);
});

test('inventory args: safe selector and build flags are mirrored exactly', () => {
  const parsed = parseWrapperArgs([
    '--cargo-profile',
    'ci',
    '--tests',
    '-p',
    'lotar',
    '--all-features',
  ]);
  assert.deepEqual(buildInventoryArgs(parsed), [
    'nextest',
    'list',
    '--cargo-profile',
    'ci',
    '--tests',
    '-p',
    'lotar',
    '--all-features',
    '--profile',
    'default',
    '-E',
    'test(/(^|::)git_required::/)',
    '--message-format',
    'json',
  ]);
});

test('inventory args: unknown or run-only flags disable the mirror', () => {
  assert.equal(buildInventoryArgs(parseWrapperArgs(['--retries', '0'])), null);
  assert.equal(buildInventoryArgs(parseWrapperArgs(['--color', 'never'])), null);
  assert.equal(buildInventoryArgs(parseWrapperArgs(['-E', 'test(x)'])), null);
  // A trailing value flag without its value is malformed input, rejected at
  // parse time rather than silently treated as an inventory-safe command.
  assert.throws(() => parseWrapperArgs(['--cargo-profile']), RunnerError);
});

test('list parsing: real nextest list document shape', () => {
  const stdout = JSON.stringify({
    'rust-build-meta': {},
    'test-count': 2,
    'rust-suites': {
      'lotar::b_test': {
        'binary-name': 'b_test',
        testcases: {
          'git_required::z': { kind: 'test', ignored: false, 'filter-match': { status: 'matches' } },
          'pure_case': { kind: 'test', ignored: false, 'filter-match': { status: 'mismatch' } },
        },
      },
      'lotar::a_test': {
        'binary-name': 'a_test',
        testcases: {
          'git_required::a': { kind: 'test', ignored: false, 'filter-match': { status: 'matches' } },
        },
      },
    },
  });
  assert.deepEqual(parseListTestNames(stdout), [
    'a_test::git_required::a',
    'b_test::git_required::z',
  ]);
  assert.deepEqual(parseListTestNames(JSON.stringify({ 'rust-suites': {} })), []);
  assert.throws(() => parseListTestNames('not json'), RunnerError);
  assert.throws(() => parseListTestNames(JSON.stringify({})), RunnerError);
});

const makeLauncher = (behavior) => async (cwd, gitBinary, env, timeoutMs) => {
  behavior.probeDir = cwd;
  behavior.seenEnv = env;
  behavior.seenBinary = gitBinary;
  behavior.seenTimeout = timeoutMs;
  if (behavior.mode === 'hang') {
    return new Promise(() => {});
  }
  if (behavior.mode === 'spawn-error') {
    throw Object.assign(new Error('spawn git ENOENT'), { code: 'ENOENT' });
  }
  if (behavior.mode === 'exit-1') {
    return { code: 1, stderr: 'fatal: bad object HEAD' };
  }
  return { code: 0, stderr: '' };
};

// Sandbox-independent artifact stubs: the production check inspects the real
// .git path; tests inject the outcome so .git creation policy cannot flip a
// unit result.
const artifact = (present) => () => present;

test('probe: success requires exit 0 AND the .git artifact', async () => {
  const behavior = { mode: 'ok' };
  const probe = await probeGitCapability({
    launcher: makeLauncher(behavior),
    artifactExists: artifact(true),
  });
  assert.equal(probe.available, true);
  assert.equal(probe.reason, '');
  assert.equal(behavior.seenEnv.GIT_CONFIG_NOSYSTEM, '1');
  assert.equal(behavior.seenEnv.HOME, behavior.probeDir);
  assert.ok(behavior.seenEnv.GIT_CONFIG_GLOBAL.startsWith(behavior.probeDir));
  assert.equal(behavior.seenBinary, 'git');
  assert.equal(behavior.seenTimeout, 5000);
});

test('probe: nonzero exit reports the git failure', async () => {
  const probe = await probeGitCapability({ launcher: makeLauncher({ mode: 'exit-1' }) });
  assert.equal(probe.available, false);
  assert.match(probe.reason, /status 1/);
  assert.match(probe.reason, /fatal: bad object HEAD/);
});

test('probe: exit 0 without .git artifact is unavailable', async () => {
  const probe = await probeGitCapability({
    launcher: makeLauncher({ mode: 'ok' }),
    artifactExists: artifact(false),
  });
  assert.equal(probe.available, false);
  assert.match(probe.reason, /no \.git artifact/);
});

test('probe: hanging git is bounded by the timeout', async () => {
  const behavior = { mode: 'hang' };
  const probe = await probeGitCapability({
    launcher: makeLauncher(behavior),
    timeoutMs: 40,
    artifactExists: artifact(true),
  });
  assert.equal(probe.available, false);
  assert.match(probe.reason, /timed out/);
  assert.equal(existsSync(behavior.probeDir), false);
});

test('probe: missing git binary is unavailable, not a crash', async () => {
  const probe = await probeGitCapability({ launcher: makeLauncher({ mode: 'spawn-error' }) });
  assert.equal(probe.available, false);
  assert.match(probe.reason, /cannot run git/);
});

test('probe: temp directory cleaned up on success and failure', async () => {
  for (const mode of ['ok', 'exit-1']) {
    const behavior = { mode };
    await probeGitCapability({
      launcher: makeLauncher(behavior),
      artifactExists: artifact(true),
    });
    assert.equal(existsSync(behavior.probeDir), false, `cleanup failed for ${mode}`);
  }
});

async function dryRun({ args, env = {} }) {
  let out = '';
  const stdout = { write: (chunk) => { out += chunk; } };
  const code = await main({
    argv: args,
    env,
    stdout,
    stderr: { write: () => {} },
  });
  return { code, summary: JSON.parse(out) };
}

test('main dry-run: unavailable probe injects gitless and reports it', async () => {
  const { code, summary } = await dryRun({
    args: ['--dry-run', '--force-probe', 'unavailable', '--cargo-profile', 'ci'],
  });
  assert.equal(code, 0);
  assert.equal(summary.probe.available, false);
  assert.equal(summary.plan.gitlessSelected, true);
  assert.equal(summary.plan.injectProfile, true);
  assert.match(summary.command, /^cargo nextest run --profile gitless --cargo-profile ci$/);
});

test('main dry-run: available probe keeps the plain default profile', async () => {
  const { summary } = await dryRun({
    args: ['--dry-run', '--force-probe', 'available', '--cargo-profile', 'ci'],
  });
  assert.equal(summary.plan.gitlessSelected, false);
  assert.equal(summary.plan.injectProfile, false);
  assert.match(summary.command, /^cargo nextest run --cargo-profile ci$/);
});

test('main dry-run: agent mode carries the full old contract', async () => {
  const { summary } = await dryRun({
    args: ['--dry-run', '--force-probe', 'available', '--agent'],
  });
  assert.match(summary.command, /--cargo-profile ci/);
  assert.match(summary.command, /--color never/);
  assert.match(summary.command, /--failure-output immediate-final/);
  assert.match(summary.command, /--retries 0/);
  assert.deepEqual(summary.envAdditions, ['NO_COLOR=1', 'FORCE_COLOR=0', 'CARGO_TERM_COLOR=never']);
});

test('main dry-run: require-git still aborts with a forced-unavailable probe', async () => {
  await assert.rejects(
    dryRun({
      args: ['--dry-run', '--force-probe', 'unavailable'],
      env: { LOTAR_REQUIRE_GIT: '1' },
    }),
    /designated full-coverage/,
  );
});

test('main dry-run: explicit NEXTEST_PROFILE bypasses injection', async () => {
  const { summary } = await dryRun({
    args: ['--dry-run', '--force-probe', 'unavailable', '--cargo-profile', 'ci'],
    env: { NEXTEST_PROFILE: 'perf' },
  });
  assert.equal(summary.explicitProfile, 'perf');
  assert.equal(summary.plan.injectProfile, false);
  assert.match(summary.command, /^cargo nextest run --cargo-profile ci$/);
});

test('probe env: Windows bootstrap vars pass through case-insensitively', () => {
  const env = buildProbeEnv(
    { PATH: '/bin', SYSTEMROOT: 'C:\\Windows', Temp: 'C:\\scratch', SystemRoot_Extra: 'no' },
    '/owned/tmp',
    '/owned/tmp/gitconfig',
    { windows: true },
  );
  assert.equal(env.PATH, '/bin');
  assert.equal(env.SYSTEMROOT, 'C:\\Windows');
  assert.equal(env.Temp, 'C:\\scratch');
  assert.equal(env.SystemRoot_Extra, undefined);
});

test('probe env: no parent Git routing or config keys leak in any case', () => {
  const parent = {
    PATH: '/bin',
    GIT_DIR: '/elsewhere',
    git_work_tree: '/elsewhere',
    GIT_CONFIG_COUNT: '2',
    git_config_global: '/home/user/.gitconfig',
    TEMP: 'C:\\tmp',
  };
  const env = buildProbeEnv(parent, '/owned/tmp', '/owned/tmp/gitconfig', { windows: true });
  const owned = new Set(['GIT_CONFIG_NOSYSTEM', 'GIT_CONFIG_GLOBAL']);
  for (const key of Object.keys(env)) {
    if (owned.has(key)) {
      continue;
    }
    assert.doesNotMatch(key, /^git_/i, `leaked parent Git key: ${key}`);
  }
  assert.equal(env.GIT_CONFIG_GLOBAL, '/owned/tmp/gitconfig');
  assert.equal(env.TEMP, 'C:\\tmp');
});

test('probe env: owned overrides beat parent values on every platform', () => {
  for (const windows of [true, false]) {
    const env = buildProbeEnv(
      { PATH: '/bin', HOME: '/home/user', USERPROFILE: 'C:\\Users\\user', GIT_CONFIG_GLOBAL: '/home/user/.gitconfig' },
      '/owned/tmp',
      '/owned/tmp/gitconfig',
      { windows },
    );
    assert.equal(env.HOME, '/owned/tmp');
    assert.equal(env.USERPROFILE, '/owned/tmp');
    assert.equal(env.GIT_CONFIG_GLOBAL, '/owned/tmp/gitconfig');
    assert.equal(env.GIT_CONFIG_NOSYSTEM, '1');
    if (!windows) {
      assert.equal(env.SystemRoot, undefined);
    }
  }
});
