#!/usr/bin/env node
'use strict';
// Runs the lotar binary from the platform package npm installed next to this
// one (DEV-250). npm picks that package from its os/cpu fields; there is no
// postinstall step, so installs with --ignore-scripts work too.
const { spawn } = require('node:child_process');

const INSTALL_DOCS = 'https://github.com/localtaskrepo/lotar/blob/main/docs/help/install.md';

// `${process.platform}-${process.arch}` -> platform package. Windows on ARM runs
// the x64 build under emulation, so that package also allows arm64.
const PACKAGES = {
    'darwin-arm64': '@localtaskrepo/lotar-darwin-arm64',
    'darwin-x64': '@localtaskrepo/lotar-darwin-x64',
    'linux-arm64': '@localtaskrepo/lotar-linux-arm64',
    'linux-x64': '@localtaskrepo/lotar-linux-x64',
    'win32-arm64': '@localtaskrepo/lotar-win32-x64',
    'win32-x64': '@localtaskrepo/lotar-win32-x64',
};

function binaryPath(platform = process.platform, arch = process.arch, resolve = require.resolve) {
    const pkg = PACKAGES[`${platform}-${arch}`];
    if (!pkg) {
        throw new Error(`there is no npm build for ${platform}-${arch}; see ${INSTALL_DOCS} for other ways to install.`);
    }
    const file = platform === 'win32' ? 'lotar.exe' : 'lotar';
    try {
        return resolve(`${pkg}/bin/${file}`);
    } catch {
        throw new Error(
            `${pkg} is not installed. It is an optional dependency of @localtaskrepo/lotar, so reinstall without ` +
                `--omit=optional or --no-optional, or see ${INSTALL_DOCS}.`,
        );
    }
}

function run() {
    let bin;
    try {
        bin = binaryPath();
    } catch (error) {
        console.error(`lotar: ${error.message}`);
        process.exit(1);
    }
    // stdio is inherited, so `lotar mcp` talks to its client directly.
    const child = spawn(bin, process.argv.slice(2), { stdio: 'inherit' });
    // Ctrl+C already reaches the child through the terminal; forward the
    // signals that usually target only this process, such as a client
    // stopping an MCP server, so the binary is never left running.
    for (const signal of ['SIGTERM', 'SIGHUP']) {
        process.on(signal, () => child.kill(signal));
    }
    process.on('SIGINT', () => {});
    child.on('error', (error) => {
        console.error(`lotar: could not run ${bin}: ${error.message}`);
        process.exit(1);
    });
    child.on('exit', (code, signal) => {
        if (signal) {
            // Exit the way the binary did, so callers see the same signal.
            process.removeAllListeners(signal);
            process.kill(process.pid, signal);
            return;
        }
        process.exit(code ?? 1);
    });
}

if (require.main === module) run();

module.exports = { PACKAGES, binaryPath };
