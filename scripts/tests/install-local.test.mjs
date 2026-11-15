#!/usr/bin/env node
// Tests for scripts/install-local.mjs (DEV-563): which identity signs a
// build from source, and that a failed step replaces no installed binary.
// codesign and security are faked; the install runs against temp dirs.

import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { DEFAULT_IDENTITY, chooseIdentity, installLocal, parseIdentities } from '../install-local.mjs';

const LISTING = `  1) A249626292A512C3C762C35999659014201AF8F3 "Apple Development: Ravi Gairola (3KAUY3X329)"
  2) 3AC6606E556C89E17F8A0969A06E8B0B1E2E84A3 "${DEFAULT_IDENTITY}"
     2 valid identities found
`;

function workspace(t) {
    const root = mkdtempSync(join(tmpdir(), 'lotar-install-local-'));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    const sourceDir = join(root, 'release');
    const installDir = join(root, 'bin');
    mkdirSync(sourceDir);
    mkdirSync(installDir);
    writeFileSync(join(sourceDir, 'lotar'), 'new lotar');
    writeFileSync(join(sourceDir, 'lotar-agent-wrapper'), 'new wrapper');
    writeFileSync(join(installDir, 'lotar'), 'old lotar');
    writeFileSync(join(installDir, 'lotar-agent-wrapper'), 'old wrapper');
    return { sourceDir, installDir };
}

function fakeRun({ listing = LISTING, failSign, hangSign, failStart } = {}) {
    const calls = [];
    const run = (command, args) => {
        calls.push([command, ...args]);
        if (command === 'security') return { status: 0, stdout: listing, stderr: '' };
        if (command === 'codesign' && failSign && args.includes('--sign') && args.at(-1).includes(failSign)) {
            return { status: 1, stdout: '', stderr: 'errSecInternalComponent' };
        }
        if (command === 'codesign' && hangSign && args.includes('--sign')) {
            writeFileSync(`${args.at(-1)}.cstemp`, 'partial');
            return { status: null, stdout: '', stderr: '', timedOut: true };
        }
        if (failStart && command.endsWith(`.${failStart}.install-${process.pid}`)) {
            return {
                status: null,
                stdout: '',
                stderr: '',
                error: new Error('killed by SIGKILL (code signature invalid)'),
            };
        }
        return { status: 0, stdout: '', stderr: '' };
    };
    return { run, calls };
}

const quiet = { log: () => {}, warn: () => {} };

test('parseIdentities reads names and hashes from security find-identity', () => {
    assert.deepEqual(parseIdentities(LISTING), [
        { hash: 'A249626292A512C3C762C35999659014201AF8F3', name: 'Apple Development: Ravi Gairola (3KAUY3X329)' },
        { hash: '3AC6606E556C89E17F8A0969A06E8B0B1E2E84A3', name: DEFAULT_IDENTITY },
    ]);
});

test('the Developer ID signs by default, LOTAR_SIGNING_IDENTITY wins over it', () => {
    const ids = parseIdentities(LISTING);
    assert.deepEqual(chooseIdentity(undefined, ids), { sign: DEFAULT_IDENTITY, warning: null });
    const other = 'Apple Development: Ravi Gairola (3KAUY3X329)';
    assert.equal(chooseIdentity(other, ids).sign, other);
    assert.equal(
        chooseIdentity('3ac6606e556c89e17f8a0969a06e8b0b1e2e84a3', ids).sign,
        '3ac6606e556c89e17f8a0969a06e8b0b1e2e84a3',
    );
    assert.deepEqual(chooseIdentity('-', []), { sign: '-', warning: null });
});

test('a missing default identity falls back to ad hoc with a warning', () => {
    const { sign, warning } = chooseIdentity(undefined, []);
    assert.equal(sign, '-');
    assert.match(warning, new RegExp(DEFAULT_IDENTITY.replace(/[()]/g, '\\$&')));
    assert.match(warning, /firewall and file-access grants again/);
});

test('a missing LOTAR_SIGNING_IDENTITY stops instead of signing ad hoc', () => {
    assert.throws(
        () => chooseIdentity('Developer ID Application: Someone (XXXXXXXXXX)', []),
        /not a valid code-signing identity/,
    );
});

test('macOS: both binaries are signed with hardened runtime and fixed identifiers, then installed', (t) => {
    const { sourceDir, installDir } = workspace(t);
    const { run, calls } = fakeRun();
    installLocal({ sourceDir, installDir, platform: 'darwin', env: { PATH: installDir }, run, ...quiet });

    assert.equal(readFileSync(join(installDir, 'lotar'), 'utf8'), 'new lotar');
    assert.equal(readFileSync(join(installDir, 'lotar-agent-wrapper'), 'utf8'), 'new wrapper');
    assert.deepEqual(readdirSync(installDir).sort(), ['lotar', 'lotar-agent-wrapper']);
    const signs = calls.filter((call) => call[0] === 'codesign' && call.includes('--sign'));
    assert.deepEqual(
        signs.map((call) => call.slice(1, -1)),
        [
            [
                '--force',
                '--options',
                'runtime',
                '--timestamp',
                '--identifier',
                'net.pyxzl.lotar',
                '--sign',
                DEFAULT_IDENTITY,
            ],
            [
                '--force',
                '--options',
                'runtime',
                '--timestamp',
                '--identifier',
                'net.pyxzl.lotar-agent-wrapper',
                '--sign',
                DEFAULT_IDENTITY,
            ],
        ],
    );
    const staged = (name) => join(installDir, `.${name}.install-${process.pid}`);
    assert.ok(
        calls.some(
            (call) =>
                call[0] === staged('lotar-agent-wrapper') &&
                call[1] === '--' &&
                call[2] === staged('lotar') &&
                call[3] === '--version',
        ),
    );
});

test('macOS without the identity: ad hoc signed and installed, with the warning', (t) => {
    const { sourceDir, installDir } = workspace(t);
    const { run, calls } = fakeRun({ listing: '     0 valid identities found\n' });
    const warnings = [];
    installLocal({
        sourceDir,
        installDir,
        platform: 'darwin',
        env: { PATH: installDir },
        run,
        log: () => {},
        warn: (m) => warnings.push(m),
    });

    assert.equal(readFileSync(join(installDir, 'lotar'), 'utf8'), 'new lotar');
    assert.ok(
        calls
            .filter((call) => call.includes('--sign'))
            .every((call) => call.includes('-') && call.includes('--timestamp=none')),
    );
    assert.equal(warnings.length, 1);
    assert.match(warnings[0], /signed ad hoc/);
});

test('a codesign failure stops with its error and replaces neither binary', (t) => {
    const { sourceDir, installDir } = workspace(t);
    const { run } = fakeRun({ failSign: 'lotar-agent-wrapper' });
    assert.throws(
        () => installLocal({ sourceDir, installDir, platform: 'darwin', env: {}, run, ...quiet }),
        /codesign of lotar-agent-wrapper failed:\nerrSecInternalComponent/,
    );
    assert.equal(readFileSync(join(installDir, 'lotar'), 'utf8'), 'old lotar');
    assert.equal(readFileSync(join(installDir, 'lotar-agent-wrapper'), 'utf8'), 'old wrapper');
    assert.deepEqual(readdirSync(installDir).sort(), ['lotar', 'lotar-agent-wrapper']);
});

test('Linux: installed without signing', (t) => {
    const { sourceDir, installDir } = workspace(t);
    const { run, calls } = fakeRun();
    installLocal({ sourceDir, installDir, platform: 'linux', env: { PATH: installDir }, run, ...quiet });

    assert.equal(readFileSync(join(installDir, 'lotar'), 'utf8'), 'new lotar');
    assert.ok(!calls.some((call) => call[0] === 'codesign' || call[0] === 'security'));
});

test('a codesign that waits for the keychain prompt stops with a hint and leaves no temp files', (t) => {
    const { sourceDir, installDir } = workspace(t);
    const { run } = fakeRun({ hangSign: true });
    assert.throws(
        () => installLocal({ sourceDir, installDir, platform: 'darwin', env: {}, run, ...quiet }),
        /codesign of lotar did not finish .*Always Allow/,
    );
    assert.equal(readFileSync(join(installDir, 'lotar'), 'utf8'), 'old lotar');
    assert.deepEqual(readdirSync(installDir).sort(), ['lotar', 'lotar-agent-wrapper']);
});

test('a new wrapper that does not start replaces neither binary', (t) => {
    const { sourceDir, installDir } = workspace(t);
    const { run } = fakeRun({ failStart: 'lotar-agent-wrapper' });
    assert.throws(
        () => installLocal({ sourceDir, installDir, platform: 'darwin', env: {}, run, ...quiet }),
        /the new lotar-agent-wrapper -- lotar --version failed:\nkilled by SIGKILL/,
    );
    assert.equal(readFileSync(join(installDir, 'lotar'), 'utf8'), 'old lotar');
    assert.equal(readFileSync(join(installDir, 'lotar-agent-wrapper'), 'utf8'), 'old wrapper');
    assert.deepEqual(readdirSync(installDir).sort(), ['lotar', 'lotar-agent-wrapper']);
});
