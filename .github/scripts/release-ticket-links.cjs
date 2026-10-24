'use strict';

const WARNING = 'Release ticket links unavailable: the release task snapshot could not be verified.';
const SHA = /^[a-f0-9]{40}$/i;
const PREFIX = /^[\p{Alphabetic}\p{Number}][\p{Alphabetic}\p{Number}_-]*$/u;
const TOKEN = /[\p{Alphabetic}\p{Number}_-]+/gu;
const U64_MAX = 18446744073709551615n;

function validPrefix(prefix) {
  return Buffer.byteLength(prefix, 'utf8') <= 64 && PREFIX.test(prefix);
}

function number(value) {
  if (!/^[0-9]+$/.test(value)) return null;
  const normalized = value.replace(/^0+(?=\d)/, '');
  if (normalized.length > 20) return null;
  const result = BigInt(normalized);
  return result <= U64_MAX ? result.toString() : null;
}

function safePath(path) {
  // eslint-disable-next-line no-control-regex -- control characters make a path unsafe
  return typeof path === 'string' && !/[\\\x00-\x1f\x7f]/.test(path)
    && path.split('/').every((part) => part && part !== '.' && part !== '..');
}

async function loadTicketSnapshot({ github, owner, repo, tag, warn = () => {} }) {
  const snapshot = { owner, repo, revision: null, tickets: new Map() };
  try {
    const { data: commit } = await github.rest.repos.getCommit({ owner, repo, ref: tag });
    if (!SHA.test(commit?.sha) || !SHA.test(commit?.commit?.tree?.sha)) throw new Error('commit');
    snapshot.revision = commit.sha;
    const { data: root } = await github.rest.git.getTree({ owner, repo, tree_sha: commit.commit.tree.sha });
    if (root?.truncated !== false || !Array.isArray(root.tree)) throw new Error('root');
    const roots = root.tree.filter((entry) => entry.path === '.tasks');
    if (!roots.length) return snapshot;
    if (roots.length !== 1 || roots[0].type !== 'tree' || roots[0].mode !== '040000' || !SHA.test(roots[0].sha)) throw new Error('tasks');
    const { data: tree } = await github.rest.git.getTree({ owner, repo, tree_sha: roots[0].sha, recursive: '1' });
    if (tree?.truncated !== false || !Array.isArray(tree.tree)) throw new Error('tree');
    const ambiguous = new Set();
    for (const entry of tree.tree) {
      const modes = { blob: ['100644', '100755', '120000'], tree: ['040000'], commit: ['160000'] };
      if (!safePath(entry.path) || !modes[entry.type]?.includes(entry.mode)
        || !SHA.test(entry.sha)) throw new Error('entry');
      if (entry.type !== 'blob' || !['100644', '100755'].includes(entry.mode)) continue;
      const parts = entry.path.split('/');
      const prefix = parts[0];
      if (parts.length < 2 || !validPrefix(prefix) || parts.slice(1, -1).some((part) => part.startsWith('@'))) continue;
      const match = /^([0-9]+)\.ya?ml$/.exec(parts.at(-1));
      const suffix = match && number(match[1]);
      if (suffix === null || !match) continue;
      const id = `${prefix}-${suffix}`;
      if (snapshot.tickets.has(id) || ambiguous.has(id)) {
        snapshot.tickets.delete(id);
        ambiguous.add(id);
      } else {
        snapshot.tickets.set(id, `.tasks/${entry.path}`);
      }
    }
    return snapshot;
  } catch {
    snapshot.tickets.clear();
    warn(WARNING);
    return snapshot;
  }
}

function encodeSegment(value) {
  return encodeURIComponent(value).replace(/[!'()*]/g, (char) => `%${char.charCodeAt(0).toString(16).toUpperCase()}`);
}

function escapeText(text) {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/[\\`*{}[\]()#+!|~_$]/g, '\\$&');
}

// Only preserve explicit HTTP(S) inline links, never arbitrary Markdown destinations.
function inlineLinkAt(text, start, brackets) {
  const labelStart = text[start] === '!' ? start + 1 : start;
  if (text[labelStart] !== '[') return null;
  const labelEnd = brackets.get(labelStart);
  if (labelEnd === undefined || text[labelEnd + 1] !== '(') return null;
  let i = labelEnd + 2;
  while (text[i] === ' ') i++;
  const angle = text[i] === '<';
  if (angle) i++;
  const destinationStart = i;
  if (!/^https?:\/\//i.test(text.slice(i))) return null;
  let depth = 0;
  for (; i < text.length; i++) {
    const char = text[i];
    if (char === ' ' && (angle || depth)) return null;
    if (angle && char === '>') break;
    if (!angle && depth === 0 && (char === ')' || char === ' ')) break;
    if (!angle && char === '(') depth++;
    if (!angle && char === ')') depth--;
  }
  if (depth || i === text.length || (angle && text[i] !== '>')) return null;
  const destination = text.slice(destinationStart, i);
  if (!/^https?:\/\/[^\s<>"\\]+$/i.test(destination)) return null;
  if (angle) i++;
  const hadSpace = text[i] === ' ';
  while (text[i] === ' ') i++;
  let title = '';
  if (hadSpace && (text[i] === '"' || text[i] === "'")) {
    const quote = text[i++];
    const titleStart = i;
    for (; i < text.length && text[i] !== quote; i++) {
      if (text[i] === '\\') i++;
    }
    if (i >= text.length) return null;
    // Entities preserve title text while removing literal release markers.
    title = ` ${quote}${text.slice(titleStart, i).replace(/</g, '&lt;').replace(/>/g, '&gt;')}${quote}`;
    i++;
    while (text[i] === ' ') i++;
  }
  if (text[i] !== ')') return null;
  const label = text.slice(labelStart + 1, labelEnd);
  // Escape the label rather than allowing nested images, HTML, or links.
  return { end: i + 1, value: `${labelStart > start ? '!' : ''}[${escapeText(label)}](${angle ? `<${destination}>` : destination}${title})` };
}

function linkTicketReferences(subject, snapshot) {
  // eslint-disable-next-line no-control-regex -- control characters are collapsed to spaces
  const text = String(subject).replace(/[\r\n\u2028\u2029\x00-\x1f\x7f]+/g, ' ');
  const canLink = snapshot?.tickets instanceof Map && SHA.test(snapshot.revision);
  // Match label delimiters once, including escapes, rather than rescanning an
  // unmatched suffix at every opening bracket in a commit subject.
  const brackets = new Map();
  const openBrackets = [];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\\') { i++; continue; }
    if (text[i] === '[') openBrackets.push(i);
    else if (text[i] === ']' && openBrackets.length) brackets.set(openBrackets.pop(), i);
  }
  function ordinary(value, offset) {
    let result = '';
    let end = 0;
    for (const match of value.matchAll(TOKEN)) {
      const raw = match[0];
      const split = raw.lastIndexOf('-');
      const prefix = raw.slice(0, split);
      const suffix = split > 0 && validPrefix(prefix) ? number(raw.slice(split + 1)) : null;
      const path = suffix !== null && canLink ? snapshot.tickets.get(`${prefix}-${suffix}`) : null;
      const start = offset + match.index;
      const before = text[start - 1] || '';
      const after = text[start + raw.length] || '';
      result += escapeText(value.slice(end, match.index));
      const pathBoundary = /[/\\@\p{Mark}]/u.test(before + after)
        || before === '.' || (after === '.' && /[\p{Alphabetic}\p{Number}_-]/u.test(text[start + raw.length + 1] || ''));
      if (path && safePath(path) && path.startsWith(`.tasks/${prefix}/`) && !pathBoundary) {
        const url = `https://github.com/${encodeSegment(snapshot.owner)}/${encodeSegment(snapshot.repo)}/blob/${snapshot.revision}/${path.split('/').map(encodeSegment).join('/')}`;
        result += `[${escapeText(raw)}](${url})`;
      } else result += escapeText(raw);
      end = match.index + raw.length;
    }
    return result + escapeText(value.slice(end));
  }
  let result = '';
  let plainStart = 0;
  // Index matching delimiter runs once; unmatched runs never trigger repeated suffix scans.
  const backticks = new Map();
  const nextRun = new Map();
  const runs = [...text.matchAll(/`+/g)];
  for (let index = runs.length - 1; index >= 0; index--) {
    const run = runs[index];
    backticks.set(run.index, { length: run[0].length, end: nextRun.get(run[0].length) });
    nextRun.set(run[0].length, run.index);
  }
  for (let i = 0; i < text.length;) {
    let protectedPart = inlineLinkAt(text, i, brackets);
    if (!protectedPart && text[i] === '`') {
      const run = backticks.get(i);
      if (run.end !== undefined) {
        let content = text.slice(i + run.length, run.end);
        if (content.startsWith(' ') && content.endsWith(' ') && /[^ ]/.test(content)) content = content.slice(1, -1);
        protectedPart = {
          end: run.end + run.length,
          // Entities inside backticks render literally. A safe HTML code wrapper
          // preserves visual text while escaping both Markdown and release markers.
          value: /[&<>]/.test(content)
            ? `<code>${escapeText(content)}</code>`
            : text.slice(i, run.end + run.length),
        };
      } else {
        i += run.length;
        continue;
      }
    }
    if (!protectedPart) {
      const url = /^(?:<https?:\/\/[^\s<>]+>|https?:\/\/[^\s<>]+)/i.exec(text.slice(i));
      if (url) protectedPart = { end: i + url[0].length, value: url[0].startsWith('<') ? url[0] : url[0].replace(/&/g, '&amp;') };
    }
    if (protectedPart) {
      result += ordinary(text.slice(plainStart, i), plainStart) + protectedPart.value;
      i = protectedPart.end;
      plainStart = i;
    } else i++;
  }
  return result + ordinary(text.slice(plainStart), plainStart);
}

/**
 * Commit subjects for the release page: first line of each message (up to
 * `maxCommits` commits), identical subjects collapsed into one line (follow-up
 * commits reuse their ticket's "DEV-N Title" subject), sorted
 * case-insensitively. `remaining` counts commits beyond `maxCommits`.
 */
function releaseCommitSubjects(commits, maxCommits = 100) {
  const list = Array.isArray(commits) ? commits : [];
  const unique = new Set(
    list
      .slice(0, maxCommits)
      .map((c) => (c?.commit?.message ?? '').split('\n')[0].trim() || '(no message)'),
  );
  const subjects = [...unique].sort((a, b) => a.localeCompare(b, 'en', { sensitivity: 'base' }));
  return { subjects, remaining: Math.max(0, list.length - maxCommits) };
}

module.exports = { loadTicketSnapshot, linkTicketReferences, releaseCommitSubjects };
