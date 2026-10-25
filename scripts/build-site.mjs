// Assemble the project homepage (site/) with the images it uses into
// target/site, ready for GitHub Pages. Run `npm run site:build`, then open
// target/site/index.html or serve the folder to preview it.
import { copyFileSync, cpSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'target', 'site');

rmSync(out, { recursive: true, force: true });
cpSync(path.join(root, 'site'), out, { recursive: true });

const branding = path.join(root, 'view', 'assets', 'branding');
mkdirSync(path.join(out, 'assets', 'branding'), { recursive: true });
for (const file of ['favicon.ico', 'lotar-app-icon.svg', 'lotar-logo-dark.svg', 'lotar-symbol.svg']) {
    copyFileSync(path.join(branding, file), path.join(out, 'assets', 'branding', file));
}
copyFileSync(path.join(root, 'docs', 'assets', 'social-preview.png'), path.join(out, 'assets', 'social-preview.png'));

// Only the screenshots the page references, so the site stays small.
const html = readFileSync(path.join(out, 'index.html'), 'utf8');
const screenshots = path.join(root, 'docs', 'assets', 'screenshots');
mkdirSync(path.join(out, 'assets', 'screenshots'), { recursive: true });
let copied = 0;
for (const file of readdirSync(screenshots)) {
    if (!html.includes(`assets/screenshots/${file}`)) continue;
    copyFileSync(path.join(screenshots, file), path.join(out, 'assets', 'screenshots', file));
    copied += 1;
}

const missing = [...html.matchAll(/assets\/screenshots\/([\w.-]+)/g)]
    .map((match) => match[1])
    .filter((file) => !readdirSync(screenshots).includes(file));
if (missing.length) {
    console.error(`Missing screenshots: ${[...new Set(missing)].join(', ')}`);
    process.exit(1);
}
console.log(`Built ${path.relative(root, out)} with ${copied} screenshots`);
