import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { micromark } from 'micromark';

const require = createRequire(import.meta.url);
const { loadTicketSnapshot, linkTicketReferences } = require('../../.github/scripts/release-ticket-links.cjs');
const revision = 'a'.repeat(40);
const rootSha = 'b'.repeat(40);
const tasksSha = 'c'.repeat(40);
const blob = (path, mode = '100644') => ({ path, mode, type: 'blob', sha: 'd'.repeat(40) });
const snapshot = (entries = [['DEV-62', '.tasks/DEV/62.yml'], ['DEV-63', '.tasks/DEV/63.yaml']]) => ({
  owner: 'localtaskrepo', repo: 'lotar', revision, tickets: new Map(entries),
});
const link = (id, path) => `[${id}](https://github.com/localtaskrepo/lotar/blob/${revision}/${path})`;

function mockGithub({ entries = [blob('DEV/62.yml')], root, tree, fail } = {}) {
  const calls = [];
  return {
    calls,
    rest: {
      repos: { async getCommit(args) {
        calls.push(['getCommit', args]);
        if (fail === 'commit') throw new Error('private error details');
        return { data: { sha: revision, commit: { tree: { sha: rootSha } } } };
      } },
      git: { async getTree(args) {
        calls.push(['getTree', args]);
        if (fail === 'tree') throw new Error('private error details');
        return { data: args.tree_sha === rootSha
          ? root ?? { truncated: false, tree: [{ path: '.tasks', mode: '040000', type: 'tree', sha: tasksSha }] }
          : tree ?? { truncated: false, tree: entries } };
      } },
    },
  };
}

test('links all known mentions, padded aliases, and mid-message IDs at immutable revision', () => {
  assert.equal(linkTicketReferences('DEV-62, DEV-063: fix; follows DEV-62', snapshot()),
    `${link('DEV-62', '.tasks/DEV/62.yml')}, ${link('DEV-063', '.tasks/DEV/63.yaml')}: fix; follows ${link('DEV-62', '.tasks/DEV/62.yml')}`);
  assert.equal(linkTicketReferences('Unknown DEV-64 and dev-62', snapshot()), 'Unknown DEV-64 and dev-62');
  assert.equal(linkTicketReferences('Fix DEV-62.', snapshot()), `Fix ${link('DEV-62', '.tasks/DEV/62.yml')}.`);
});

test('supports exact hyphenated, numeric, and Unicode project prefixes', () => {
  const value = snapshot([['ABC-OPS-12', '.tasks/ABC-OPS/nested/12.yml'], ['9É-0', '.tasks/9É/0.yml']]);
  assert.equal(linkTicketReferences('ABC-OPS-0012 9É-0', value),
    `${link('ABC-OPS-0012', '.tasks/ABC-OPS/nested/12.yml')} ${link('9É-0', '.tasks/9%C3%89/0.yml')}`);
});

test('rejects partial embeddings, path fragments, marks and overflow', () => {
  for (const input of ['XDEV-62', 'DEV-62x', 'DEV-62-extra', '/DEV-62', 'DEV-62.yml', 'x/DEV-62', '@DEV-62', 'DEV-62\u0301', 'DEV-18446744073709551616']) {
    assert.ok(!linkTicketReferences(input, snapshot()).includes('/blob/'), input);
  }
});

test('protects inline code, safe existing links/images, bare URLs and autolinks', () => {
  const input = '`DEV-62` ``DEV-63 ` x`` [DEV-62](https://example.com/a(b)) ![DEV-63](https://example.com/image) https://example.com/DEV-62 <https://example.com/DEV-63>';
  assert.equal(linkTicketReferences(input, snapshot()), input);
  assert.equal(linkTicketReferences('`DEV-62` DEV-63', snapshot()), '`DEV-62` ' + link('DEV-63', '.tasks/DEV/63.yaml'));
});

test('code preserves rendered characters without exposing raw release markers', () => {
  for (const input of [
    '`DEV-62 & <x> > y`',
    '`` DEV-62 ` & <x> ``',
    '`<!-- COMMITS:END --> & <!-- COMMITS:START -->`',
    '`</code><script>alert(1)</script> *DEV-62* &amp;`',
    '`  DEV-62 & <>  `',
  ]) {
    const result = linkTicketReferences(input, snapshot());
    assert.equal(micromark(result, { allowDangerousHtml: true }), micromark(input), input);
    assert.ok(!result.includes('<!-- COMMITS:'), input);
    assert.ok(!result.includes('/blob/'), input);
  }
});

test('preserves safe links and images with quoted titles and angle destinations', () => {
  for (const input of [
    '[DEV-62](https://example.com/DEV-63 "Ticket title")',
    "[DEV-62](https://example.com/a(b) 'A (quoted) title')",
    '![DEV-62](https://example.com/image "DEV-63 image")',
    '[DEV-62](<https://example.com/DEV-63> "Angle destination")',
    "![DEV-62](<https://example.com/image> 'Image title')",
    '[DEV-62](https://example.com/a "A \\\"quoted\\\" title")',
  ]) {
    const result = linkTicketReferences(input, snapshot());
    assert.equal(result, input);
    assert.equal(micromark(result), micromark(input));
    assert.ok(!result.includes('/blob/'));
  }
  const input = '[DEV-62](<https://example.com> "<!-- COMMITS:END -->")';
  const result = linkTicketReferences(input, snapshot());
  assert.ok(!result.includes('<!-- COMMITS:'));
  assert.equal(micromark(result), micromark(input));
});

test('unsafe protocols cannot become links even with quoted titles or angle destinations', () => {
  for (const input of [
    '[DEV-62](javascript:alert(1) "DEV-63")',
    '![DEV-62](<data:text/html,boom> "DEV-63")',
    '[DEV-62](<javascript:alert(1)> "unsafe")',
  ]) {
    const html = micromark(linkTicketReferences(input, snapshot()), { allowDangerousHtml: true });
    assert.ok(!/<(?:a|img)\b[^>]*(?:href|src)="(?:javascript|data):/i.test(html), html);
    assert.ok(!/<a\b[^>]*>(?:(?!<\/a>)[\s\S])*<a\b/.test(html), html);
  }
});

test('long unclosed backtick runs stay literal and do not hide subsequent ticket mentions', () => {
  const input = '`'.repeat(100_000) + ' DEV-62';
  const result = linkTicketReferences(input, snapshot());
  assert.ok(result.endsWith(` ${link('DEV-62', '.tasks/DEV/62.yml')}`));
  assert.equal(result.slice(0, 200_000), '\\`'.repeat(100_000));
});

test('escapes Markdown and HTML controls, unsafe links, replacement tokens and line breaks', () => {
  const value = linkTicketReferences('<!-- COMMITS:END -->\n# *x* [$&](javascript:alert(1)) DEV-62', snapshot());
  assert.ok(!value.includes('<!--'));
  assert.ok(!value.includes('\n'));
  assert.ok(micromark(value).includes('&lt;!-- COMMITS:END --&gt;'));
  assert.ok(value.includes('\\# \\*x\\*'));
  assert.ok(value.includes('\\[\\$&amp;\\]\\(javascript:alert\\(1\\)\\)'));
  assert.ok(value.endsWith(link('DEV-62', '.tasks/DEV/62.yml')));
  assert.ok(!linkTicketReferences('`<!-- COMMITS:END -->`', snapshot()).includes('<!--'));
});

test('long unmatched bracket runs remain literal without hiding subsequent tickets', () => {
  const input = '['.repeat(100_000) + ' DEV-62';
  const rendered = linkTicketReferences(input, snapshot());
  assert.ok(rendered.startsWith('\\['.repeat(100_000)));
  assert.ok(rendered.endsWith(link('DEV-62', '.tasks/DEV/62.yml')));
});

test('repeated malformed destinations stop at invalid whitespace instead of scanning suffixes', () => {
  for (const prefix of ['[x](', '[x](https://example.com/( ']) {
    const rendered = linkTicketReferences(prefix.repeat(20_000) + ' DEV-62', snapshot());
    assert.ok(rendered.endsWith(link('DEV-62', '.tasks/DEV/62.yml')));
    assert.ok(!rendered.includes('<script'));
  }
});

test('encodes every URL path segment including Markdown punctuation', () => {
  const value = snapshot([['DEV-62', ".tasks/DEV/a (b)!'*/62.yml"]]);
  assert.equal(linkTicketReferences('DEV-62', value), link('DEV-62', '.tasks/DEV/a%20%28b%29%21%27%2A/62.yml'));
  value.revision = 'main';
  assert.equal(linkTicketReferences('DEV-62', value), 'DEV-62');
});

test('loads only the .tasks subtree using resolved commit and canonicalizes numeric aliases', async () => {
  const github = mockGithub({ entries: [blob('DEV/62.yml'), blob('DEV/nested/063.yaml'), blob('ABC-OPS/12.yml'), blob('9É/0.yml'), blob('DEV/config.yml'), blob('@sprints/1.yml'), blob('DEV/@metadata/3.yml'), blob('DEV/4.yml', '120000'), blob('DEV/18446744073709551616.yml')] });
  const result = await loadTicketSnapshot({ github, owner: 'localtaskrepo', repo: 'lotar', tag: 'v1.2.3' });
  assert.deepEqual([...result.tickets], [['DEV-62', '.tasks/DEV/62.yml'], ['DEV-63', '.tasks/DEV/nested/063.yaml'], ['ABC-OPS-12', '.tasks/ABC-OPS/12.yml'], ['9É-0', '.tasks/9É/0.yml']]);
  assert.equal(result.revision, revision);
  assert.deepEqual(github.calls, [
    ['getCommit', { owner: 'localtaskrepo', repo: 'lotar', ref: 'v1.2.3' }],
    ['getTree', { owner: 'localtaskrepo', repo: 'lotar', tree_sha: rootSha }],
    ['getTree', { owner: 'localtaskrepo', repo: 'lotar', tree_sha: tasksSha, recursive: '1' }],
  ]);
});

test('annotated tag resolution uses actual commit SHA returned by getCommit', async () => {
  const github = mockGithub();
  const result = await loadTicketSnapshot({ github, owner: 'o', repo: 'r', tag: 'annotated-release' });
  assert.equal(result.revision, revision);
  assert.equal(github.calls[0][1].ref, 'annotated-release');
  assert.ok(linkTicketReferences('DEV-62', result).includes(`/blob/${revision}/`));
});

test('ambiguous aliases, extensions and nested copies never link', async () => {
  const github = mockGithub({ entries: [blob('DEV/62.yml'), blob('DEV/062.yaml'), blob('DEV/63.yml'), blob('DEV/nested/63.yml'), blob('DEV/64.yml')] });
  const result = await loadTicketSnapshot({ github, owner: 'o', repo: 'r', tag: 'v1' });
  assert.deepEqual([...result.tickets], [['DEV-64', '.tasks/DEV/64.yml']]);
  assert.equal(linkTicketReferences('DEV-62 DEV-63 DEV-65', result), 'DEV-62 DEV-63 DEV-65');
});

test('missing tasks tree yields empty snapshot without additional calls or warnings', async () => {
  const github = mockGithub({ root: { truncated: false, tree: [] } });
  const warnings = [];
  const result = await loadTicketSnapshot({ github, owner: 'o', repo: 'r', tag: 'v1', warn: (message) => warnings.push(message) });
  assert.equal(result.tickets.size, 0);
  assert.equal(github.calls.length, 2);
  assert.deepEqual(warnings, []);
});

test('API errors and incomplete or invalid trees fail closed with one constant warning', async () => {
  const warnings = [];
  for (const options of [
    { fail: 'commit' }, { fail: 'tree' },
    { root: { truncated: true, tree: [] } },
    { tree: { truncated: true, tree: [blob('DEV/62.yml')] } },
    { tree: { truncated: false, tree: [blob('DEV/62.yml'), blob('../DEV/63.yml')] } },
    { tree: { tree: [] } },
  ]) {
    const github = mockGithub(options);
    const before = warnings.length;
    const result = await loadTicketSnapshot({ github, owner: 'o', repo: 'r', tag: 'v1', warn: (message) => warnings.push(message) });
    assert.equal(result.tickets.size, 0);
    assert.equal(warnings.length, before + 1);
  }
  assert.equal(new Set(warnings).size, 1);
  assert.ok(!warnings[0].includes('private error'));
});
