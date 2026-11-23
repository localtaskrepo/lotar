// `npm run seed:test-tasks` must seed its own folder even when the caller (an agent job)
// exports LOTAR_TASKS_DIR / LOTAR_WORKSPACE_ROOT for the real backlog (DEV-818).
// Run with: node --test scripts/tests/seed-test-tasks.test.mjs

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { seedEnvironment } from '../demo/workspace.mjs';

const SCRIPT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'generate-test-tasks.mjs');

// Records each call's arguments and the task folder it would use, and answers `add` like lotar.
const STUB = `#!/usr/bin/env node
const fs = require('node:fs');
const args = process.argv.slice(2);
fs.appendFileSync(process.env.SEED_STUB_LOG, JSON.stringify({
    args,
    tasksDir: process.env.LOTAR_TASKS_DIR ?? null,
    workspaceRoot: process.env.LOTAR_WORKSPACE_ROOT ?? null,
    project: process.env.LOTAR_PROJECT ?? null,
}) + '\\n');
if (args[0] === 'add') console.log('Created task: DEMO-' + fs.readFileSync(process.env.SEED_STUB_LOG, 'utf8').split('\\n').length);
`;

test('seeding and --serve ignore an inherited LOTAR_TASKS_DIR', { skip: process.platform === 'win32' }, () => {
    const root = realpathSync(mkdtempSync(path.join(os.tmpdir(), 'lotar-seed-test-')));
    try {
        const decoy = path.join(root, 'backlog', '.tasks');
        const stub = path.join(root, 'lotar-stub.cjs');
        const log = path.join(root, 'calls.jsonl');
        writeFileSync(stub, STUB.replace('#!/usr/bin/env node', `#!${process.execPath}`));
        chmodSync(stub, 0o755);

        // A relative --dir, so the test also covers lotar running with cwd = the seed dir.
        const res = spawnSync(process.execPath, [SCRIPT, '--count', '3', '--dir', 'seed', '--serve'], {
            cwd: root,
            env: {
                ...process.env,
                LOTAR_BINARY_PATH: stub,
                LOTAR_TASKS_DIR: decoy,
                LOTAR_WORKSPACE_ROOT: path.dirname(decoy),
                LOTAR_PROJECT: 'DEV',
                SEED_STUB_LOG: log,
            },
            encoding: 'utf8',
        });
        assert.equal(res.status, 0, res.stderr);

        const calls = readFileSync(log, 'utf8')
            .trim()
            .split('\n')
            .map((line) => JSON.parse(line));
        assert.equal(calls.filter((call) => call.args[0] === 'add').length, 3);
        assert.ok(
            calls.some((call) => call.args[0] === 'serve'),
            'the --serve call ran',
        );
        for (const call of calls) {
            assert.deepEqual(
                { tasksDir: call.tasksDir, workspaceRoot: call.workspaceRoot, project: call.project },
                { tasksDir: path.join(root, 'seed', '.tasks'), workspaceRoot: null, project: null },
                `lotar ${call.args.join(' ')}`,
            );
        }
        assert.deepEqual(readdirSync(path.join(root, 'seed', '.tasks')), ['config.yml']);
    } finally {
        rmSync(root, { recursive: true, force: true });
    }
});

test('seed environment points lotar at the seed dir and drops inherited LOTAR_* settings', () => {
    const dir = path.resolve('seed-fixture');
    const env = seedEnvironment(dir, {
        PATH: 'fixture-path',
        GIT_DIR: 'kept-for-plain-seeding',
        LOTAR_TASKS_DIR: 'private-tasks',
        LOTAR_WORKSPACE_ROOT: 'private-root',
        LOTAR_DEFAULT_ASSIGNEE: '@fix',
    });
    assert.deepEqual(env, {
        PATH: 'fixture-path',
        GIT_DIR: 'kept-for-plain-seeding',
        LOTAR_IGNORE_HOME_CONFIG: '1',
        LOTAR_TASKS_DIR: path.join(dir, '.tasks'),
    });
});
