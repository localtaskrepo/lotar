#!/usr/bin/env node
// Assemble the npm packages for a release from its archives (DEV-250).
//
//   node scripts/npm-packages.mjs --tag v0.12.0 --archives downloads --out npm-dist
//
// --archives holds the release archives (lotar-<tag>-<platform>.tar.gz / .zip).
// The output has one directory per package and a publish-order.json listing them
// platform packages first, so the main package never points at versions that
// are not published yet. Each platform package carries one binary and the
// os/cpu fields npm uses to install only the matching one; the main `lotar`
// package holds the launcher (packaging/npm/lotar) and depends on all of them
// as optionalDependencies.

import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export const MAIN_PACKAGE = 'lotar';
export const SCOPE = '@localtaskrepo';

// npm platform -> release archive. Linux uses the static musl builds, which run
// on glibc and musl distributions alike. Windows on ARM runs the x64 build
// under emulation, so its package allows arm64 too (the launcher maps it).
export const PLATFORMS = [
    {
        id: 'darwin-arm64',
        os: 'darwin',
        cpu: ['arm64'],
        archive: 'macos-arm64.tar.gz',
        binary: 'lotar',
        label: 'macOS on Apple Silicon',
    },
    {
        id: 'darwin-x64',
        os: 'darwin',
        cpu: ['x64'],
        archive: 'macos-x64.tar.gz',
        binary: 'lotar',
        label: 'macOS on Intel',
    },
    {
        id: 'linux-arm64',
        os: 'linux',
        cpu: ['arm64'],
        archive: 'linux-musl-arm64.tar.gz',
        binary: 'lotar',
        label: 'Linux on arm64',
    },
    {
        id: 'linux-x64',
        os: 'linux',
        cpu: ['x64'],
        archive: 'linux-musl-x64.tar.gz',
        binary: 'lotar',
        label: 'Linux on x64',
    },
    {
        id: 'win32-x64',
        os: 'win32',
        cpu: ['x64', 'arm64'],
        archive: 'windows-x64.zip',
        binary: 'lotar.exe',
        label: 'Windows on x64 and ARM64',
    },
];

const SHARED = {
    license: 'MIT',
    author: 'Ravi Gairola <mallox@pyxzl.net>',
    homepage: 'https://localtaskrepo.github.io/lotar/',
    // Must name this repository for npm provenance to verify.
    repository: { type: 'git', url: 'git+https://github.com/localtaskrepo/lotar.git' },
    bugs: { url: 'https://github.com/localtaskrepo/lotar/issues' },
};

export function platformPackageName(platform) {
    return `${SCOPE}/lotar-${platform.id}`;
}

/** v0.12.0 -> 0.12.0; rejects anything that is not a release version. */
export function npmVersion(tag) {
    const version = String(tag).replace(/^v/, '');
    if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
        throw new Error(`not a release version: ${tag}`);
    }
    return version;
}

/** Prereleases go to `next`, so `npm install lotar` keeps getting stable releases. */
export function distTag(version) {
    return version.includes('-') ? 'next' : 'latest';
}

export function platformManifest(platform, version) {
    return {
        name: platformPackageName(platform),
        version,
        description: `The lotar binary for ${platform.label}. Install the lotar package instead.`,
        ...SHARED,
        os: [platform.os],
        cpu: platform.cpu,
        files: [`bin/${platform.binary}`],
        // Yarn Plug'n'Play must unpack the binary to run it.
        preferUnplugged: true,
    };
}

export function mainManifest(version) {
    return {
        name: MAIN_PACKAGE,
        version,
        description:
            'Git-native task management: tasks as YAML files in your repository, with a CLI, web UI, and MCP server.',
        keywords: ['cli', 'task-management', 'git', 'yaml', 'issue-tracker', 'mcp'],
        ...SHARED,
        bin: { lotar: 'bin/lotar.js' },
        files: ['bin/lotar.js'],
        engines: { node: '>=18' },
        // Exact versions: the launcher and binary always come from one release.
        optionalDependencies: Object.fromEntries(PLATFORMS.map((platform) => [platformPackageName(platform), version])),
    };
}

function run(command, args) {
    const result = spawnSync(command, args, { stdio: ['ignore', 'inherit', 'inherit'] });
    if (result.error) throw new Error(`${command} failed: ${result.error.message}`);
    if (result.status !== 0) throw new Error(`${command} ${args.join(' ')} exited with ${result.status}`);
}

function extract(archive, destination) {
    mkdirSync(destination, { recursive: true });
    if (archive.endsWith('.zip')) run('unzip', ['-o', '-q', archive, '-d', destination]);
    else run('tar', ['-xzf', archive, '-C', destination]);
}

function writeJson(file, value) {
    writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

/** Build every package directory and return them in publish order. */
export function buildPackages({ tag, archives, out }) {
    const version = npmVersion(tag);
    rmSync(out, { recursive: true, force: true });
    mkdirSync(out, { recursive: true });
    const scratch = mkdtempSync(path.join(os.tmpdir(), 'lotar-npm-'));
    const order = [];
    try {
        for (const platform of PLATFORMS) {
            const archive = path.join(archives, `lotar-${tag}-${platform.archive}`);
            if (!existsSync(archive)) throw new Error(`missing archive ${archive}`);
            const unpacked = path.join(scratch, platform.id);
            extract(archive, unpacked);
            const binary = path.join(unpacked, platform.binary);
            if (!existsSync(binary)) throw new Error(`${archive} has no ${platform.binary}`);

            const dir = path.join(out, `lotar-${platform.id}`);
            mkdirSync(path.join(dir, 'bin'), { recursive: true });
            copyFileSync(binary, path.join(dir, 'bin', platform.binary));
            // npm keeps the executable bit from the packed file.
            chmodSync(path.join(dir, 'bin', platform.binary), 0o755);
            copyFileSync(path.join(ROOT, 'LICENSE'), path.join(dir, 'LICENSE'));
            writeFileSync(
                path.join(dir, 'README.md'),
                `# ${platformPackageName(platform)}\n\nThe lotar binary for ${platform.label}. ` +
                    'Install [`lotar`](https://www.npmjs.com/package/lotar) instead; it picks this package for you.\n',
            );
            writeJson(path.join(dir, 'package.json'), platformManifest(platform, version));
            order.push({ name: platformPackageName(platform), dir });
        }

        const dir = path.join(out, MAIN_PACKAGE);
        cpSync(path.join(ROOT, 'packaging', 'npm', 'lotar'), dir, { recursive: true });
        chmodSync(path.join(dir, 'bin', 'lotar.js'), 0o755);
        copyFileSync(path.join(ROOT, 'LICENSE'), path.join(dir, 'LICENSE'));
        writeJson(path.join(dir, 'package.json'), mainManifest(version));
        order.push({ name: MAIN_PACKAGE, dir });
    } finally {
        rmSync(scratch, { recursive: true, force: true });
    }
    const summary = { version, distTag: distTag(version), packages: order };
    writeJson(path.join(out, 'publish-order.json'), summary);
    return summary;
}

function parseArgs(argv) {
    const opts = {};
    for (let i = 0; i < argv.length; i += 1) {
        const flag = argv[i];
        const value = argv[i + 1];
        if (!['--tag', '--archives', '--out'].includes(flag)) throw new Error(`unknown option ${flag}`);
        if (!value || value.startsWith('--')) throw new Error(`${flag} needs a value`);
        opts[flag.slice(2)] = value;
        i += 1;
    }
    for (const required of ['tag', 'archives', 'out']) {
        if (!opts[required]) throw new Error(`--${required} is required`);
    }
    return opts;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try {
        const summary = buildPackages(parseArgs(process.argv.slice(2)));
        console.log(JSON.stringify(summary, null, 2));
    } catch (error) {
        console.error(`npm-packages: ${error.message}`);
        process.exit(1);
    }
}
