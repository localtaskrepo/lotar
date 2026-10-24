// DEV-162: the git hooks are read-only and only check what is being
// committed or pushed. Each case runs the real hook script in a throwaway
// repository that reuses this repo's tool configs and node_modules.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CONFIGS = [
    'rustfmt.toml',
    '.prettierrc.json',
    '.prettierignore',
    'eslint.config.mjs',
    'eslint-suppressions.json',
];

function git(cwd, ...args) {
    return execFileSync('git', args, { cwd, encoding: 'utf8', env: gitEnv() });
}

function gitEnv() {
    return {
        ...process.env,
        GIT_AUTHOR_NAME: 'Hook Test',
        GIT_AUTHOR_EMAIL: 'hooks@example.invalid',
        GIT_COMMITTER_NAME: 'Hook Test',
        GIT_COMMITTER_EMAIL: 'hooks@example.invalid',
        GIT_CONFIG_GLOBAL: '/dev/null',
        GIT_CONFIG_NOSYSTEM: '1',
    };
}

function fixtureRepo() {
    const dir = mkdtempSync(path.join(tmpdir(), 'lotar-hooks-'));
    git(dir, 'init', '-q', '-b', 'main');
    git(dir, 'config', 'commit.gpgsign', 'false');
    for (const file of CONFIGS) copyFileSync(path.join(ROOT, file), path.join(dir, file));
    symlinkSync(path.join(ROOT, 'node_modules'), path.join(dir, 'node_modules'), 'dir');
    writeFileSync(path.join(dir, '.gitignore'), 'node_modules\n');
    mkdirSync(path.join(dir, 'view'), { recursive: true });
    mkdirSync(path.join(dir, 'src'), { recursive: true });
    writeFileSync(path.join(dir, 'view/ok.ts'), "export const value = 'ok'\n");
    writeFileSync(path.join(dir, 'src/lib.rs'), 'pub fn value() -> u8 {\n    1\n}\n');
    git(dir, 'add', '-A');
    git(dir, 'commit', '-q', '-m', 'init');
    return dir;
}

function runHook(dir, hook, input = '') {
    return spawnSync('bash', [path.join(ROOT, '.githooks', hook)], {
        cwd: dir,
        input,
        encoding: 'utf8',
        env: gitEnv(),
    });
}

function snapshot(dir) {
    return {
        status: git(dir, 'status', '--porcelain=v1'),
        index: git(dir, 'diff', '--cached'),
        stash: git(dir, 'stash', 'list'),
    };
}

test('pre-commit passes clean staged files without touching the repository', (t) => {
    const dir = fixtureRepo();
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    writeFileSync(path.join(dir, 'view/ok.ts'), "export const value = 'still ok'\n");
    git(dir, 'add', 'view/ok.ts');
    const before = snapshot(dir);
    const result = runHook(dir, 'pre-commit');
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.deepEqual(snapshot(dir), before);
});

test('pre-commit rejects unformatted Rust and TypeScript without fixing them', (t) => {
    const dir = fixtureRepo();
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    const badRust = 'pub fn value()->u8{1}\n';
    const badTs = 'export const value = "double quotes";\n';
    writeFileSync(path.join(dir, 'src/lib.rs'), badRust);
    writeFileSync(path.join(dir, 'view/ok.ts'), badTs);
    git(dir, 'add', '-A');
    const before = snapshot(dir);
    const result = runHook(dir, 'pre-commit');
    assert.equal(result.status, 1);
    assert.match(result.stdout, /Rust formatting/);
    assert.match(result.stdout, /Frontend formatting/);
    assert.equal(readFileSync(path.join(dir, 'src/lib.rs'), 'utf8'), badRust);
    assert.equal(readFileSync(path.join(dir, 'view/ok.ts'), 'utf8'), badTs);
    assert.deepEqual(snapshot(dir), before);
});

test('pre-commit reports ESLint findings and leaves partially staged files alone', (t) => {
    const dir = fixtureRepo();
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    writeFileSync(path.join(dir, 'view/ok.ts'), "const unused = 1\nexport const value = 'ok'\n");
    git(dir, 'add', 'view/ok.ts');
    writeFileSync(path.join(dir, 'view/ok.ts'), "const unused = 2\nexport const value = 'ok'\n");
    const before = snapshot(dir);
    const result = runHook(dir, 'pre-commit');
    assert.equal(result.status, 1);
    assert.match(result.stdout, /partially staged/);
    assert.match(result.stdout + result.stderr, /no-unused-vars/);
    assert.deepEqual(snapshot(dir), before);
});

test('pre-push skips all checks when only non-code files changed', (t) => {
    const dir = fixtureRepo();
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    const base = git(dir, 'rev-parse', 'HEAD').trim();
    writeFileSync(path.join(dir, 'NOTES.md'), '# notes\n');
    git(dir, 'add', 'NOTES.md');
    git(dir, 'commit', '-q', '-m', 'docs');
    const head = git(dir, 'rev-parse', 'HEAD').trim();
    const started = Date.now();
    const result = runHook(dir, 'pre-push', `refs/heads/main ${head} refs/heads/main ${base}\n`);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.stdout, /nothing to check/);
    assert.ok(Date.now() - started < 5_000);
});

test('pre-push runs the frontend checks when frontend files changed', (t) => {
    const dir = fixtureRepo();
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    const base = git(dir, 'rev-parse', 'HEAD').trim();
    writeFileSync(path.join(dir, 'view/ok.ts'), 'export const value = "unformatted";\n');
    git(dir, 'commit', '-q', '-am', 'bad format');
    const head = git(dir, 'rev-parse', 'HEAD').trim();
    const result = runHook(dir, 'pre-push', `refs/heads/main ${head} refs/heads/main ${base}\n`);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /Frontend checks failed/);
    assert.doesNotMatch(result.stdout, /Rust/);
});
