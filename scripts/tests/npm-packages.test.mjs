import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
    buildPackages,
    distTag,
    mainManifest,
    npmVersion,
    PLATFORMS,
    platformManifest,
    platformPackageName,
} from '../npm-packages.mjs';

const require = createRequire(import.meta.url);
const launcher = require('../../packaging/npm/lotar/bin/lotar.js');

test('release tags become npm versions and dist-tags', () => {
    assert.equal(npmVersion('v0.12.0'), '0.12.0');
    assert.equal(npmVersion('1.0.0-rc.1'), '1.0.0-rc.1');
    assert.throws(() => npmVersion('main'), /not a release version/);
    assert.throws(() => npmVersion('v1.2'), /not a release version/);
    assert.equal(distTag('0.12.0'), 'latest');
    assert.equal(distTag('1.0.0-rc.1'), 'next');
});

test('the main package depends on every platform package at its own exact version', () => {
    const manifest = mainManifest('0.12.0');
    assert.equal(manifest.name, 'lotar');
    assert.deepEqual(manifest.bin, { lotar: 'bin/lotar.js' });
    assert.equal(manifest.scripts, undefined, 'no install scripts');
    assert.deepEqual(
        manifest.optionalDependencies,
        Object.fromEntries(PLATFORMS.map((platform) => [`@localtaskrepo/lotar-${platform.id}`, '0.12.0'])),
    );
    assert.equal(manifest.repository.url, 'git+https://github.com/localtaskrepo/lotar.git');
});

test('platform packages declare the os and cpu npm installs them for', () => {
    const windows = platformManifest(
        PLATFORMS.find((platform) => platform.id === 'win32-x64'),
        '0.12.0',
    );
    assert.equal(windows.name, '@localtaskrepo/lotar-win32-x64');
    assert.deepEqual(windows.os, ['win32']);
    assert.deepEqual(windows.cpu, ['x64', 'arm64']);
    assert.deepEqual(windows.files, ['bin/lotar.exe']);
    const linux = platformManifest(
        PLATFORMS.find((platform) => platform.id === 'linux-arm64'),
        '0.12.0',
    );
    assert.deepEqual([linux.os, linux.cpu, linux.files], [['linux'], ['arm64'], ['bin/lotar']]);
});

test('the launcher knows exactly the published platform packages', () => {
    const published = new Set(PLATFORMS.map(platformPackageName));
    for (const platform of PLATFORMS) {
        assert.equal(launcher.PACKAGES[platform.id], platformPackageName(platform), platform.id);
    }
    for (const [key, pkg] of Object.entries(launcher.PACKAGES)) {
        assert.ok(published.has(pkg), `${key} -> ${pkg} is not published`);
        const [os, cpu] = key.split('-');
        const platform = PLATFORMS.find((entry) => platformPackageName(entry) === pkg);
        assert.ok(platform.os === os && platform.cpu.includes(cpu), `${pkg} would not install on ${key}`);
    }
});

test('the launcher resolves the binary or explains why it cannot', () => {
    const resolved = [];
    const resolve = (request) => {
        resolved.push(request);
        return `/node_modules/${request}`;
    };
    assert.equal(
        launcher.binaryPath('win32', 'arm64', resolve),
        '/node_modules/@localtaskrepo/lotar-win32-x64/bin/lotar.exe',
    );
    assert.equal(
        launcher.binaryPath('linux', 'x64', resolve),
        '/node_modules/@localtaskrepo/lotar-linux-x64/bin/lotar',
    );
    assert.throws(() => launcher.binaryPath('freebsd', 'x64', resolve), /no npm build for freebsd-x64/);
    const missing = () => {
        throw new Error('Cannot find module');
    };
    assert.throws(
        () => launcher.binaryPath('darwin', 'arm64', missing),
        /@localtaskrepo\/lotar-darwin-arm64 is not installed.*--omit=optional/,
    );
});

test('buildPackages assembles one directory per package from the release archives', (t) => {
    const work = mkdtempSync(path.join(os.tmpdir(), 'lotar-npm-test-'));
    t.after(() => rmSync(work, { recursive: true, force: true }));
    const tag = 'v0.12.0';
    const archives = path.join(work, 'archives');
    mkdirSync(archives);
    for (const platform of PLATFORMS) {
        const stage = path.join(work, 'stage', platform.id);
        mkdirSync(stage, { recursive: true });
        writeFileSync(path.join(stage, platform.binary), `binary for ${platform.id}\n`, { mode: 0o644 });
        const archive = path.join(archives, `lotar-${tag}-${platform.archive}`);
        const result = platform.archive.endsWith('.zip')
            ? spawnSync('zip', ['-q', '-j', archive, path.join(stage, platform.binary)])
            : spawnSync('tar', ['-czf', archive, '-C', stage, platform.binary]);
        assert.equal(result.status, 0, String(result.stderr));
    }

    const out = path.join(work, 'out');
    const summary = buildPackages({ tag, archives, out });
    assert.equal(summary.version, '0.12.0');
    assert.equal(summary.distTag, 'latest');
    assert.deepEqual(
        summary.packages.map((entry) => entry.name),
        [...PLATFORMS.map(platformPackageName), 'lotar'],
        'platform packages publish before the package that depends on them',
    );
    assert.deepEqual(JSON.parse(readFileSync(path.join(out, 'publish-order.json'), 'utf8')), summary);

    for (const platform of PLATFORMS) {
        const dir = path.join(out, `lotar-${platform.id}`);
        const binary = path.join(dir, 'bin', platform.binary);
        assert.equal(readFileSync(binary, 'utf8'), `binary for ${platform.id}\n`);
        assert.equal(statSync(binary).mode & 0o111, 0o111, `${platform.id} binary is executable`);
        assert.deepEqual(
            JSON.parse(readFileSync(path.join(dir, 'package.json'), 'utf8')),
            platformManifest(platform, '0.12.0'),
        );
        assert.match(readFileSync(path.join(dir, 'LICENSE'), 'utf8'), /MIT License/);
    }
    const main = path.join(out, 'lotar');
    assert.deepEqual(JSON.parse(readFileSync(path.join(main, 'package.json'), 'utf8')), mainManifest('0.12.0'));
    assert.equal(statSync(path.join(main, 'bin', 'lotar.js')).mode & 0o111, 0o111);
    assert.match(readFileSync(path.join(main, 'README.md'), 'utf8'), /npx -y lotar/);
});

test('buildPackages refuses a release with a missing archive', (t) => {
    const work = mkdtempSync(path.join(os.tmpdir(), 'lotar-npm-test-'));
    t.after(() => rmSync(work, { recursive: true, force: true }));
    assert.throws(
        () => buildPackages({ tag: 'v0.12.0', archives: work, out: path.join(work, 'out') }),
        /missing archive .*lotar-v0\.12\.0-macos-arm64\.tar\.gz/,
    );
});
