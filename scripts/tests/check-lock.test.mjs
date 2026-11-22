// The machine's check lock (DEV-784): full runs never overlap, a run under it doesn't wait again, filtered Vitest runs
// don't take it.
import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import test from 'node:test';
import { setTimeout as sleep } from 'node:timers/promises';
import { acquireCheckLock, CHECK_LOCK_HELD_ENV, isFilteredRun } from '../check-lock.mjs';

function freePort() {
    return new Promise((resolve, reject) => {
        const server = createServer();
        server.once('error', reject);
        server.listen({ port: 0, host: '127.0.0.1' }, () => {
            const { port } = server.address();
            server.close(() => resolve(port));
        });
    });
}

test('two runs never hold the lock at once, and the waiting one names the holder', async () => {
    const port = await freePort();
    const logs = [];
    const spans = [];
    const run = async (label) => {
        const env = {};
        const release = await acquireCheckLock({ label, port, env, log: (line) => logs.push(line), retryMs: 50 });
        assert.equal(env[CHECK_LOCK_HELD_ENV], '1', 'the run tells its processes it holds the lock');
        const start = Date.now();
        await sleep(300);
        spans.push([start, Date.now()]);
        await release();
    };
    await Promise.all([run('first run'), run('second run'), run('third run')]);
    spans.sort((a, b) => a[0] - b[0]);
    for (let i = 1; i < spans.length; i += 1) {
        assert.ok(spans[i - 1][1] <= spans[i][0], `runs overlapped: ${JSON.stringify(spans)}`);
    }
    const waits = logs.filter((line) => line.startsWith('Waiting for the machine'));
    assert.ok(waits.length > 0, 'a run waited');
    assert.ok(
        waits.every((line) => /held by (first|second|third) run in .+ \(pid \d+\), since \d\d:\d\d UTC$/.test(line)),
        waits.join('\n'),
    );
    assert.ok(
        logs.some((line) => line.startsWith('Took the check lock after')),
        logs.join('\n'),
    );
});

test('a run under a holder does not wait for the lock again', async () => {
    const port = await freePort();
    const release = await acquireCheckLock({ label: 'outer', port, env: {}, log: () => {} });
    try {
        const logs = [];
        const inner = await acquireCheckLock({
            label: 'inner',
            port,
            env: { [CHECK_LOCK_HELD_ENV]: '1' },
            log: (line) => logs.push(line),
            maxWaitMs: 0,
        });
        await inner();
        assert.deepEqual(logs, []);
    } finally {
        await release();
    }
});

test('a run that waits too long fails naming the holder', async () => {
    const port = await freePort();
    const release = await acquireCheckLock({ label: 'the long run', port, env: {}, log: () => {} });
    try {
        await assert.rejects(
            acquireCheckLock({ label: 'waiting', port, env: {}, log: () => {}, maxWaitMs: 100, retryMs: 50 }),
            /stayed held .* by the long run in .*lsof -i :\d+/,
        );
    } finally {
        await release();
    }
});

test('a holder that sends no line is another check run', async () => {
    const silent = createServer(() => {});
    await new Promise((resolve) => silent.listen({ port: 0, host: '127.0.0.1' }, resolve));
    const { port } = silent.address();
    const logs = [];
    try {
        await assert.rejects(
            acquireCheckLock({ label: 'waiting', port, env: {}, log: (line) => logs.push(line), maxWaitMs: 0 }),
        );
        assert.deepEqual(logs, [`Waiting for the machine's check lock (port ${port}), held by another check run`]);
    } finally {
        silent.close();
        silent.unref();
    }
});

test('Vitest runs filtered by file or test name count as filtered', () => {
    assert.equal(isFilteredRun({ vitest: {}, config: {} }), false);
    assert.equal(isFilteredRun({ vitest: { filenamePattern: [] }, config: {} }), false);
    assert.equal(isFilteredRun({ vitest: { filenamePattern: ['Board'] }, config: {} }), true);
    assert.equal(isFilteredRun({ vitest: {}, config: { testNamePattern: /moves/ } }), true);
});
