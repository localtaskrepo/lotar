import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { parse } from 'yaml';

const workflow = parse(readFileSync(new URL('../../.github/workflows/release.yml', import.meta.url), 'utf8'));
const steps = workflow.jobs['update-release-page'].steps;
const renderer = steps.find((step) => step.uses?.startsWith('actions/github-script@'));
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const run = new AsyncFunction('github', 'context', 'core', 'process', 'require', renderer.with.script);
const require = createRequire(new URL('../../package.json', import.meta.url));
const revision = 'a'.repeat(40);
const rootTree = 'b'.repeat(40);
const tasksTree = 'c'.repeat(40);
const tag = 'v0.9.1';
const markerCount = (body, marker) => body.split(marker).length - 1;

function fixture({
    body = '',
    subjects = ['DEV-109 Fix backlog ordering'],
    previous = true,
    snapshotError = false,
    types = { 109: 'Feature', 62: 'Bug', 63: 'Chore' },
    attachments = {},
    images = {},
} = {}) {
    const updates = [];
    const calls = [];
    const warnings = [];
    // Blob contents by SHA: ticket YAML (DEV-177) and attachment images (DEV-225).
    const blobs = new Map();
    const ticketSha = (id) => id.toString(16).padStart(40, '0');
    for (const id of [109, 62, 63]) {
        const references = (attachments[id] ?? []).map((name) => `- attachment: ${name}`);
        const yaml = [`title: Ticket ${id}`, `status: Done`, ...(types[id] ? [`type: ${types[id]}`] : [])];
        if (references.length) yaml.push('references:', ...references);
        blobs.set(ticketSha(id), Buffer.from(`${yaml.join('\n')}\n`));
    }
    const imageEntries = Object.entries(images).map(([name, bytes], index) => {
        const sha = (index + 1).toString(16).padStart(40, 'e');
        blobs.set(sha, bytes);
        return { path: `DEV/@attachments/${name}`, type: 'blob', mode: '100644', sha };
    });
    const github = {
        rest: {
            repos: {
                getReleaseByTag: async (params) => {
                    assert.deepEqual(params, { owner: 'localtaskrepo', repo: 'lotar', tag });
                    return { data: { id: 17, body } };
                },
                listTags: async () => {
                    throw new Error('listTags must be paginated');
                },
                compareCommits: async (params) => {
                    assert.deepEqual(params, { owner: 'localtaskrepo', repo: 'lotar', base: 'v0.9.0', head: tag });
                    return { data: { commits: subjects.map((message) => ({ commit: { message } })) } };
                },
                getCommit: async (params) => {
                    calls.push({ method: 'getCommit', ...params });
                    assert.equal(params.ref, tag);
                    if (snapshotError) throw new Error('Snapshot API unavailable');
                    return { data: { sha: revision, commit: { tree: { sha: rootTree } } } };
                },
                updateRelease: async (params) => {
                    updates.push(params);
                    return { data: params };
                },
            },
            git: {
                getTree: async (params) => {
                    calls.push({ method: 'getTree', ...params });
                    if (params.tree_sha === revision || params.tree_sha === rootTree) {
                        return {
                            data: {
                                truncated: false,
                                tree: [{ path: '.tasks', type: 'tree', mode: '040000', sha: tasksTree }],
                            },
                        };
                    }
                    assert.equal(params.tree_sha, tasksTree, 'read only the release snapshot task subtree');
                    return {
                        data: {
                            truncated: false,
                            tree: [
                                ...[109, 62, 63].map((id) => ({
                                    path: `DEV/${id}.yml`,
                                    type: 'blob',
                                    mode: '100644',
                                    sha: ticketSha(id),
                                })),
                                ...imageEntries,
                            ],
                        },
                    };
                },
                getBlob: async (params) => {
                    calls.push({ method: 'getBlob', ...params });
                    assert.deepEqual(Object.keys(params).sort(), ['file_sha', 'owner', 'repo']);
                    const content = blobs.get(params.file_sha);
                    if (!content) throw new Error(`unknown blob ${params.file_sha}`);
                    return { data: { encoding: 'base64', content: content.toString('base64') } };
                },
            },
        },
        paginate: async (method, params) => {
            assert.equal(method, github.rest.repos.listTags);
            assert.deepEqual(params, { owner: 'localtaskrepo', repo: 'lotar', per_page: 100 });
            return [{ name: tag }, ...(previous ? [{ name: 'v0.9.0' }] : [])];
        },
    };
    return {
        updates,
        calls,
        warnings,
        async render() {
            await run(
                github,
                { repo: { owner: 'localtaskrepo', repo: 'lotar' } },
                { warning: (message) => warnings.push(message) },
                { env: { GITHUB_REF_NAME: tag } },
                require,
            );
            assert.equal(updates.length, 1, 'exactly one release-body write through the mocked client');
            assert.equal(updates[0].release_id, 17);
            assert.equal(updates[0].owner, 'localtaskrepo');
            assert.equal(updates[0].repo, 'lotar');
            return updates[0].body;
        },
    };
}

test('release job checks out its own trusted ref without persistent credentials', () => {
    const checkout = steps.findIndex((step) => step.uses?.startsWith('actions/checkout@'));
    assert.ok(checkout >= 0 && checkout < steps.indexOf(renderer));
    assert.equal(steps[checkout].with.ref, '${{ github.ref }}');
    assert.equal(steps[checkout].with['persist-credentials'], false);
});

test('actual workflow links all known mentions to immutable release files and preserves other sections', async () => {
    const retained =
        'Handwritten highlights remain here.\n\n**Full Changelog**: https://github.com/localtaskrepo/lotar/compare/v0.9.0...v0.9.1';
    const sample = fixture({
        body: retained,
        subjects: [
            'Security Pass DEV-109 and DEV-404',
            'DEV-62, DEV-63 Fix protocol\nIgnored commit body DEV-404',
            'A maintenance change',
        ],
    });
    const body = await sample.render();
    for (const id of [109, 62, 63]) {
        assert.ok(
            body.includes(`[DEV-${id}](https://github.com/localtaskrepo/lotar/blob/${revision}/.tasks/DEV/${id}.yml)`),
        );
    }
    assert.ok(body.includes('DEV-404'));
    assert.ok(!body.includes('[DEV-404]('));
    assert.ok(!body.includes('Ignored commit body'));
    assert.ok(body.includes(retained));
    assert.ok(body.includes(`releases/download/${tag}/lotar-${tag}-macos-arm64.tar.gz`));
    assert.ok(body.includes('mallox/lotar:0.9.1'));
    // Grouped by the type of the first ticket a subject names (DEV-177).
    const order = [
        '### Features',
        '- Security Pass',
        '### Bug fixes',
        '- [DEV-62]',
        '### Other changes',
        '- A maintenance change',
    ];
    for (let i = 1; i < order.length; i += 1) {
        assert.ok(body.indexOf(order[i - 1]) < body.indexOf(order[i]), `${order[i - 1]} before ${order[i]}`);
    }
    assert.ok(!body.includes('### Chores'), 'empty sections are left out');
    assert.equal(sample.warnings.length, 0);
    assert.ok(sample.calls.some((call) => call.method === 'getCommit'));
    assert.ok(
        sample.calls
            .filter((call) => call.method === 'getTree')
            .every((call) => ![tag, 'main'].includes(call.tree_sha)),
    );
});

test('workflow updates markers idempotently and treats replacement tokens as literal commit text', async () => {
    const retained = 'Release introduction.\n\n**Full Changelog**: https://example.com/compare';
    const subjects = ['DEV-109 Preserve $& and $1', 'DEV-62 <!-- COMMITS:END -->'];
    const once = await fixture({ body: retained, subjects }).render();
    const twice = await fixture({ body: once, subjects }).render();
    assert.equal(twice, once);
    assert.ok(twice.includes('$&'));
    assert.ok(twice.includes('$1'));
    assert.ok(twice.includes(retained));
    for (const marker of [
        '<!-- QUICK-LINKS:START -->',
        '<!-- QUICK-LINKS:END -->',
        '<!-- COMMITS:START -->',
        '<!-- COMMITS:END -->',
    ]) {
        assert.equal(markerCount(twice, marker), 1, marker);
    }
});

test('repeated commit subjects appear once on the release page (DEV-131)', async () => {
    const sample = fixture({
        subjects: [
            'DEV-118 Share automation parsing\n\nStep 1 only.',
            'DEV-62 Fix protocol',
            'DEV-118 Share automation parsing',
            'DEV-118 Share automation parsing  \nFollow-up body',
        ],
    });
    const body = await sample.render();
    assert.equal(body.split('Share automation parsing').length - 1, 1, body);
    assert.ok(body.indexOf('- [DEV-118]') < body.indexOf('- [DEV-62]'));
});

test('missing previous tag preserves first-release behavior without reading ticket files', async () => {
    const sample = fixture({ previous: false, body: 'Initial release highlights.' });
    const body = await sample.render();
    assert.ok(body.includes('_No previous tag found to compare against._'));
    assert.ok(body.includes('Initial release highlights.'));
    assert.equal(sample.calls.length, 0);
});

test('optional snapshot read failure warns but still renders downloads and commit text', async () => {
    const sample = fixture({ snapshotError: true, body: 'Keep me.' });
    const body = await sample.render();
    assert.ok(body.includes('DEV-109 Fix backlog ordering'));
    assert.ok(!body.includes('[DEV-109]('));
    assert.ok(body.includes('Download Quick Links'));
    assert.ok(body.includes('Keep me.'));
    assert.ok(sample.warnings.length > 0);
});

test('existing selection cap remains 100 subjects before alphabetical rendering', async () => {
    const subjects = Array.from({ length: 101 }, (_, index) => `DEV-109 Subject${String(index).padStart(3, '0')}`);
    const body = await fixture({ subjects }).render();
    assert.equal(
        body.split(`[DEV-109](https://github.com/localtaskrepo/lotar/blob/${revision}/.tasks/DEV/109.yml)`).length - 1,
        100,
    );
    assert.ok(!body.includes('Subject100'));
    assert.ok(body.includes('and 1 more'));
});

test('release renderer regression suite runs in the existing Node-enabled CI lanes', () => {
    const ci = parse(readFileSync(new URL('../../.github/workflows/ci.yml', import.meta.url), 'utf8'));
    for (const job of ['ui_tests', 'windows_checks']) {
        assert.ok(
            ci.jobs[job].steps.some((step) => step.run === 'npm run test:release'),
            job,
        );
    }
});

// 1x1 PNG header with a 640x120 size, and a 600x80 one for a scale-2 capture.
function png(width, height) {
    const bytes = Buffer.alloc(33);
    bytes.writeUInt32BE(0x89504e47, 0);
    bytes.writeUInt32BE(0x0d0a1a0a, 4);
    bytes.writeUInt32BE(13, 8);
    bytes.write('IHDR', 12, 'latin1');
    bytes.writeUInt32BE(width, 16);
    bytes.writeUInt32BE(height, 20);
    return bytes;
}

test('features come first, then bug fixes, chores and other changes (DEV-177)', async () => {
    const body = await fixture({
        subjects: ['DEV-63 Tidy the build', 'DEV-62 Fix a crash', 'Bump a dependency', 'DEV-109 Add themes'],
    }).render();
    const sections = [...body.matchAll(/^### (.+)$/gm)].map((match) => match[1]);
    assert.deepEqual(sections, ['Features', 'Bug fixes', 'Chores', 'Other changes']);
    assert.ok(body.indexOf('Add themes') < body.indexOf('Fix a crash'));
    assert.ok(body.indexOf('Fix a crash') < body.indexOf('Tidy the build'));
    assert.ok(body.indexOf('Tidy the build') < body.indexOf('Bump a dependency'));
});

test('tickets without a known type and unreadable tickets land in Other changes', async () => {
    const body = await fixture({ subjects: ['DEV-109 Mystery', 'DEV-62 Fix'], types: { 62: 'Bug' } }).render();
    assert.ok(body.indexOf('### Bug fixes') < body.indexOf('### Other changes'));
    assert.ok(body.indexOf('### Other changes') < body.indexOf('Mystery'));
});

test('feature tickets show up to two attached screenshots from the release snapshot (DEV-225)', async () => {
    const sample = fixture({
        subjects: ['DEV-109 Add themes', 'DEV-62 Fix a crash'],
        attachments: {
            109: ['notes.txt', 'theme-picker-2x.abc.png', 'missing.png', 'theme-dark.def.png', 'third.ghi.png'],
            62: ['crash.jkl.png'],
        },
        images: {
            'theme-picker-2x.abc.png': png(1200, 160),
            'theme-dark.def.png': png(2000, 400),
            'third.ghi.png': png(10, 10),
            'crash.jkl.png': png(10, 10),
        },
    });
    const body = await sample.render();
    const raw = `https://raw.githubusercontent.com/localtaskrepo/lotar/${revision}/.tasks/DEV/%40attachments`;
    assert.ok(body.includes(`<img src="${raw}/theme-picker-2x.abc.png" alt="DEV-109 Ticket 109" width="600">`), body);
    // Capped to the page width.
    assert.ok(body.includes(`<img src="${raw}/theme-dark.def.png" alt="DEV-109 Ticket 109" width="720">`));
    assert.ok(!body.includes('third.ghi.png'), 'at most two per feature');
    assert.ok(!body.includes('crash.jkl.png'), 'only features show screenshots');
    assert.ok(body.indexOf('Add themes') < body.indexOf('theme-picker-2x'));
    assert.equal(sample.warnings.length, 0);
});

test('screenshots are optional: an unreadable image is left out with a warning', async () => {
    const sample = fixture({
        subjects: ['DEV-109 Add themes'],
        attachments: { 109: ['broken.png'] },
        images: { 'broken.png': Buffer.from('not an image') },
    });
    const body = await sample.render();
    assert.ok(body.includes('Add themes'));
    assert.ok(!body.includes('<img'));
});
