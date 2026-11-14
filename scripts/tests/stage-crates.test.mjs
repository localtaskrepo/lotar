#!/usr/bin/env node
// Tests for scripts/stage-crates.mjs against fixture trees in the OS temp dir.
// Run with: node --test scripts/tests/stage-crates.test.mjs

import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { stageCrates } from '../stage-crates.mjs';

function fixture() {
    const root = mkdtempSync(join(tmpdir(), 'lotar-stage-crates-'));
    mkdirSync(join(root, 'docs', 'help'), { recursive: true });
    writeFileSync(join(root, 'docs', 'help', 'main.md'), '# main');
    writeFileSync(join(root, 'docs', 'openapi.json'), '{}');
    mkdirSync(join(root, 'target', 'web-embed', 'assets'), { recursive: true });
    writeFileSync(join(root, 'target', 'web-embed', 'index.html.gz'), 'gz');
    writeFileSync(join(root, 'target', 'web-embed', 'assets', 'app.js.gz'), 'gz');
    return root;
}

test('copies the docs and the web embed into the crate folders', (t) => {
    const root = fixture();
    t.after(() => rmSync(root, { recursive: true, force: true }));

    stageCrates(root);

    assert.equal(readFileSync(join(root, 'crates', 'lotar-lib', 'docs', 'help', 'main.md'), 'utf8'), '# main');
    assert.equal(readFileSync(join(root, 'crates', 'lotar-lib', 'docs', 'openapi.json'), 'utf8'), '{}');
    assert.ok(existsSync(join(root, 'crates', 'lotar-web-assets', 'web-embed', 'index.html.gz')));
    assert.ok(existsSync(join(root, 'crates', 'lotar-web-assets', 'web-embed', 'assets', 'app.js.gz')));
});

test('a file removed at the source leaves the copy on the next run', (t) => {
    const root = fixture();
    t.after(() => rmSync(root, { recursive: true, force: true }));

    stageCrates(root);
    rmSync(join(root, 'target', 'web-embed', 'assets', 'app.js.gz'));
    stageCrates(root);

    assert.ok(!existsSync(join(root, 'crates', 'lotar-web-assets', 'web-embed', 'assets', 'app.js.gz')));
});

test('refuses without a built web UI and copies nothing', (t) => {
    const root = fixture();
    t.after(() => rmSync(root, { recursive: true, force: true }));
    rmSync(join(root, 'target', 'web-embed', 'index.html.gz'));

    assert.throws(() => stageCrates(root), /npm run build:web/);
    assert.ok(!existsSync(join(root, 'crates')));
});
