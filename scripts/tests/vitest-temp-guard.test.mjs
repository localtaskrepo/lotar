// The vitest temp guard (DEV-315) fails a run on what tests leave in the temp folder, but not on what Chromium itself
// leaves there (DEV-589).
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';

import setup from '../vitest-temp-guard.mjs';

function withGuard(callback) {
    const savedEnv = process.env;
    process.env = { ...savedEnv };
    const teardown = setup();
    const root = process.env.TMPDIR;
    try {
        callback(root);
        return { root, teardown };
    } finally {
        process.env = savedEnv;
    }
}

test('passes and removes the folder when nothing is left', () => {
    const { root, teardown } = withGuard(() => {});
    teardown();
    assert.equal(existsSync(root), false);
});

test('fails on an entry a test left', () => {
    const { root, teardown } = withGuard((dir) => writeFileSync(path.join(dir, 'lotar-leak'), ''));
    assert.throws(teardown, /Tests left 1 entries in the temp folder; .*: lotar-leak$/);
    assert.equal(existsSync(root), false);
});

test("ignores Chromium's own leftovers", () => {
    const { root, teardown } = withGuard((dir) => {
        mkdirSync(path.join(dir, '.org.chromium.Chromium.fnw9Zj'));
        writeFileSync(path.join(dir, '.org.chromium.Chromium.Ab12Cd'), '');
    });
    teardown();
    assert.equal(existsSync(root), false);
});
