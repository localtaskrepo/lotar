#!/usr/bin/env node
// Copies the files two workspace crates read from outside their folder into
// the folder, so `cargo package` and `cargo publish` carry them (DEV-507):
//   docs/help, docs/openapi.json -> crates/lotar-lib/docs/
//   target/web-embed             -> crates/lotar-web-assets/web-embed/
// Inside the workspace the build scripts read the originals; a packaged crate
// reads these copies (both are gitignored). Run `npm run build:web` first.
import { cpSync, existsSync, realpathSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Each copy is rebuilt from scratch, so files deleted at the source don't
// survive in it. Throws before copying anything when a source is missing.
export function stageCrates(root) {
    const copies = [
        [join(root, 'docs', 'help'), join(root, 'crates', 'lotar-lib', 'docs', 'help')],
        [join(root, 'docs', 'openapi.json'), join(root, 'crates', 'lotar-lib', 'docs', 'openapi.json')],
        [join(root, 'target', 'web-embed'), join(root, 'crates', 'lotar-web-assets', 'web-embed')],
    ];
    if (!existsSync(join(root, 'target', 'web-embed', 'index.html.gz'))) {
        throw new Error('target/web-embed has no index.html.gz: run `npm run build:web` first');
    }
    for (const [source] of copies) {
        if (!existsSync(source)) throw new Error(`${source} is missing`);
    }
    for (const [source, dest] of copies) {
        rmSync(dest, { recursive: true, force: true });
        cpSync(source, dest, { recursive: true });
    }
    return copies.map(([, dest]) => dest);
}

function invokedAsScript() {
    if (!process.argv[1]) return false;
    try {
        return realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url));
    } catch {
        return false;
    }
}

if (invokedAsScript()) {
    for (const dest of stageCrates(fileURLToPath(new URL('..', import.meta.url)))) console.log(`staged ${dest}`);
}
