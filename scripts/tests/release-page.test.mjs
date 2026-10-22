import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { parse } from 'yaml';

const workflow = parse(readFileSync(new URL('../../.github/workflows/release.yml', import.meta.url), 'utf8'));
const steps = workflow.jobs['update-release-page'].steps;
const renderer = steps.find(step => step.uses?.startsWith('actions/github-script@'));
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const run = new AsyncFunction('github', 'context', 'core', 'process', 'require', renderer.with.script);
const require = createRequire(new URL('../../package.json', import.meta.url));
const revision = 'a'.repeat(40);
const rootTree = 'b'.repeat(40);
const tasksTree = 'c'.repeat(40);
const tag = 'v0.9.1';
const markerCount = (body, marker) => body.split(marker).length - 1;

function fixture({ body = '', subjects = ['DEV-109 Fix backlog ordering'], previous = true, snapshotError = false } = {}) {
    const updates = [];
    const calls = [];
    const warnings = [];
    const github = {
        rest: {
            repos: {
                getReleaseByTag: async params => {
                    assert.deepEqual(params, { owner: 'localtaskrepo', repo: 'lotar', tag });
                    return { data: { id: 17, body } };
                },
                listTags: async () => { throw new Error('listTags must be paginated'); },
                compareCommits: async params => {
                    assert.deepEqual(params, { owner: 'localtaskrepo', repo: 'lotar', base: 'v0.9.0', head: tag });
                    return { data: { commits: subjects.map(message => ({ commit: { message } })) } };
                },
                getCommit: async params => {
                    calls.push({ method: 'getCommit', ...params });
                    assert.equal(params.ref, tag);
                    if (snapshotError) throw new Error('Snapshot API unavailable');
                    return { data: { sha: revision, commit: { tree: { sha: rootTree } } } };
                },
                updateRelease: async params => {
                    updates.push(params);
                    return { data: params };
                },
            },
            git: {
                getTree: async params => {
                    calls.push({ method: 'getTree', ...params });
                    if (params.tree_sha === revision || params.tree_sha === rootTree) {
                        return { data: { truncated: false, tree: [{ path: '.tasks', type: 'tree', mode: '040000', sha: tasksTree }] } };
                    }
                    assert.equal(params.tree_sha, tasksTree, 'read only the release snapshot task subtree');
                    return { data: { truncated: false, tree: [109, 62, 63].map(id => ({
                        path: `DEV/${id}.yml`, type: 'blob', mode: '100644', sha: 'd'.repeat(40),
                    })) } };
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
        updates, calls, warnings,
        async render() {
            await run(github, { repo: { owner: 'localtaskrepo', repo: 'lotar' } },
                { warning: message => warnings.push(message) }, { env: { GITHUB_REF_NAME: tag } }, require);
            assert.equal(updates.length, 1, 'exactly one release-body write through the mocked client');
            assert.equal(updates[0].release_id, 17);
            assert.equal(updates[0].owner, 'localtaskrepo');
            assert.equal(updates[0].repo, 'lotar');
            return updates[0].body;
        },
    };
}

test('release job checks out its own trusted ref without persistent credentials', () => {
    const checkout = steps.findIndex(step => step.uses?.startsWith('actions/checkout@'));
    assert.ok(checkout >= 0 && checkout < steps.indexOf(renderer));
    assert.equal(steps[checkout].with.ref, '${{ github.ref }}');
    assert.equal(steps[checkout].with['persist-credentials'], false);
});

test('actual workflow links all known mentions to immutable release files and preserves other sections', async () => {
    const retained = 'Handwritten highlights remain here.\n\n**Full Changelog**: https://github.com/localtaskrepo/lotar/compare/v0.9.0...v0.9.1';
    const sample = fixture({ body: retained, subjects: [
        'Security Pass DEV-109 and DEV-404',
        'DEV-62, DEV-63 Fix protocol\nIgnored commit body DEV-404',
        'A maintenance change',
    ] });
    const body = await sample.render();
    for (const id of [109, 62, 63]) {
        assert.ok(body.includes(`[DEV-${id}](https://github.com/localtaskrepo/lotar/blob/${revision}/.tasks/DEV/${id}.yml)`));
    }
    assert.ok(body.includes('DEV-404'));
    assert.ok(!body.includes('[DEV-404]('));
    assert.ok(!body.includes('Ignored commit body'));
    assert.ok(body.includes(retained));
    assert.ok(body.includes(`releases/download/${tag}/lotar-${tag}-macos-arm64.tar.gz`));
    assert.ok(body.includes('mallox/lotar:0.9.1'));
    assert.ok(body.indexOf('- A maintenance change') < body.indexOf('- [DEV-62]'));
    assert.ok(body.indexOf('- [DEV-62]') < body.indexOf('- Security Pass'));
    assert.equal(sample.warnings.length, 0);
    assert.ok(sample.calls.some(call => call.method === 'getCommit'));
    assert.ok(sample.calls.filter(call => call.method === 'getTree').every(call => ![tag, 'main'].includes(call.tree_sha)));
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
    for (const marker of ['<!-- QUICK-LINKS:START -->', '<!-- QUICK-LINKS:END -->', '<!-- COMMITS:START -->', '<!-- COMMITS:END -->']) {
        assert.equal(markerCount(twice, marker), 1, marker);
    }
});

test('repeated commit subjects appear once on the release page (DEV-131)', async () => {
    const sample = fixture({ subjects: [
        'DEV-118 Share automation parsing\n\nStep 1 only.',
        'DEV-62 Fix protocol',
        'DEV-118 Share automation parsing',
        'DEV-118 Share automation parsing  \nFollow-up body',
    ] });
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
    assert.equal(body.split(`[DEV-109](https://github.com/localtaskrepo/lotar/blob/${revision}/.tasks/DEV/109.yml)`).length - 1, 100);
    assert.ok(!body.includes('Subject100'));
    assert.ok(body.includes('and 1 more'));
});

test('release renderer regression suite runs in the existing Node-enabled CI lanes', () => {
    const ci = parse(readFileSync(new URL('../../.github/workflows/ci.yml', import.meta.url), 'utf8'));
    for (const job of ['ui_tests', 'windows_checks']) {
        assert.ok(ci.jobs[job].steps.some(step => step.run === 'npm run test:release'), job);
    }
});
