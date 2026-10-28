'use strict';

const WARNING = 'Release ticket links unavailable: the release task snapshot could not be verified.';
const SHA = /^[a-f0-9]{40}$/i;
const PREFIX = /^[\p{Alphabetic}\p{Number}][\p{Alphabetic}\p{Number}_-]*$/u;
const TOKEN = /[\p{Alphabetic}\p{Number}_-]+/gu;
const U64_MAX = 18446744073709551615n;
// Attachment images a release page may show (DEV-225).
const IMAGE_FILE = /\.(png|jpe?g|gif|webp)$/i;

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
  const snapshot = { owner, repo, revision: null, tickets: new Map(), blobs: new Map(), images: new Map() };
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
    const ambiguousImages = new Set();
    for (const entry of tree.tree) {
      const modes = { blob: ['100644', '100755', '120000'], tree: ['040000'], commit: ['160000'] };
      if (!safePath(entry.path) || !modes[entry.type]?.includes(entry.mode)
        || !SHA.test(entry.sha)) throw new Error('entry');
      if (entry.type !== 'blob' || !['100644', '100755'].includes(entry.mode)) continue;
      const parts = entry.path.split('/');
      // Attachment stores are '@' folders (.tasks/@attachments, DEV/@attachments).
      const name = parts.at(-1);
      if (IMAGE_FILE.test(name) && parts.slice(0, -1).some((part) => part.startsWith('@'))) {
        if (snapshot.images.has(name) || ambiguousImages.has(name)) {
          snapshot.images.delete(name);
          ambiguousImages.add(name);
        } else {
          snapshot.images.set(name, { path: `.tasks/${entry.path}`, sha: entry.sha });
        }
        continue;
      }
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
        snapshot.blobs.set(`.tasks/${entry.path}`, entry.sha);
      }
    }
    return snapshot;
  } catch {
    snapshot.tickets.clear();
    snapshot.blobs.clear();
    snapshot.images.clear();
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
 * case-insensitively with numbers in numeric order. `remaining` counts
 * commits beyond `maxCommits`.
 */
function releaseCommitSubjects(commits, maxCommits = 100) {
  const list = Array.isArray(commits) ? commits : [];
  const unique = new Set(
    list
      .slice(0, maxCommits)
      .map((c) => (c?.commit?.message ?? '').split('\n')[0].trim() || '(no message)'),
  );
  // Numeric: DEV-12 sorts before DEV-111 (DEV-177).
  const subjects = [...unique].sort((a, b) => a.localeCompare(b, 'en', { sensitivity: 'base', numeric: true }));
  return { subjects, remaining: Math.max(0, list.length - maxCommits) };
}

// ---------------------------------------------------------------------------
// Release page sections (DEV-177) and feature screenshots (DEV-225)
// ---------------------------------------------------------------------------

const DETAILS_WARNING = 'Release ticket types unavailable for some tickets; they are listed under Other changes.';
const SECTIONS = [
  { type: 'feature', title: 'Features' },
  { type: 'bug', title: 'Bug fixes' },
  { type: 'chore', title: 'Chores' },
];
const OTHER_SECTION = 'Other changes';
const IMAGES_PER_FEATURE = 2;
const IMAGES_PER_RELEASE = 12;
const MAX_IMAGE_WIDTH = 720;

function unquote(value) {
  const text = value.trim();
  if (text.length >= 2 && text.startsWith("'") && text.endsWith("'")) return text.slice(1, -1).replace(/''/g, "'");
  if (text.length >= 2 && text.startsWith('"') && text.endsWith('"')) return text.slice(1, -1).replace(/\\"/g, '"');
  return text;
}

/**
 * The few ticket fields a release page needs, read from LoTaR's own YAML
 * layout: top-level `type` and `title`, and `attachment:` entries in the
 * top-level `references` list.
 */
function parseTicketYaml(text) {
  const ticket = { type: null, title: null, attachments: [] };
  let inReferences = false;
  for (const line of String(text).split(/\r?\n/)) {
    const top = /^([A-Za-z_][\w-]*):(.*)$/.exec(line);
    if (top) {
      inReferences = top[1] === 'references';
      if (top[1] === 'type' && top[2].trim()) ticket.type = unquote(top[2]);
      if (top[1] === 'title' && top[2].trim()) ticket.title = unquote(top[2]);
      continue;
    }
    if (!inReferences) continue;
    const attachment = /^\s*(?:-\s+)?attachment:\s*(.+?)\s*$/.exec(line);
    if (attachment) ticket.attachments.push(unquote(attachment[1]));
  }
  return ticket;
}

/** Ticket IDs mentioned in `subject` that exist in the snapshot, in order. */
function ticketIdsIn(subject, snapshot) {
  const ids = [];
  for (const match of String(subject).matchAll(TOKEN)) {
    const raw = match[0];
    const split = raw.lastIndexOf('-');
    const prefix = raw.slice(0, split);
    const suffix = split > 0 && validPrefix(prefix) ? number(raw.slice(split + 1)) : null;
    const id = suffix === null ? null : `${prefix}-${suffix}`;
    if (id && snapshot?.tickets?.has(id) && !ids.includes(id)) ids.push(id);
  }
  return ids;
}

async function readBlob(github, owner, repo, sha) {
  const { data } = await github.rest.git.getBlob({ owner, repo, file_sha: sha });
  if (data?.encoding !== 'base64' || typeof data.content !== 'string') throw new Error('blob');
  return Buffer.from(data.content, 'base64');
}

/** Type, title and attachments of every snapshot ticket the subjects mention. */
async function loadTicketDetails({ github, owner, repo, snapshot, subjects, warn = () => {} }) {
  const details = new Map();
  if (!SHA.test(snapshot?.revision ?? '')) return details;
  let failed = false;
  for (const id of new Set(subjects.flatMap((subject) => ticketIdsIn(subject, snapshot)))) {
    const sha = snapshot.blobs?.get(snapshot.tickets.get(id));
    if (!SHA.test(sha ?? '')) continue;
    try {
      details.set(id, parseTicketYaml((await readBlob(github, owner, repo, sha)).toString('utf8')));
    } catch {
      failed = true;
    }
  }
  if (failed) warn(DETAILS_WARNING);
  return details;
}

/** Pixel size from a PNG, GIF, WebP or JPEG header; null when unknown. */
function imageSize(bytes) {
  const b = Buffer.from(bytes);
  if (b.length >= 24 && b.readUInt32BE(0) === 0x89504e47) return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
  if (b.length >= 10 && b.toString('latin1', 0, 4) === 'GIF8') return { width: b.readUInt16LE(6), height: b.readUInt16LE(8) };
  if (b.length >= 30 && b.toString('latin1', 0, 4) === 'RIFF' && b.toString('latin1', 8, 12) === 'WEBP') {
    const chunk = b.toString('latin1', 12, 16);
    if (chunk === 'VP8X') return { width: 1 + b.readUIntLE(24, 3), height: 1 + b.readUIntLE(27, 3) };
    if (chunk === 'VP8 ') return { width: b.readUInt16LE(26) & 0x3fff, height: b.readUInt16LE(28) & 0x3fff };
    if (chunk === 'VP8L') {
      const bits = b.readUInt32LE(21);
      return { width: (bits & 0x3fff) + 1, height: ((bits >> 14) & 0x3fff) + 1 };
    }
    return null;
  }
  if (b.length >= 4 && b[0] === 0xff && b[1] === 0xd8) {
    for (let i = 2; i + 9 < b.length;) {
      if (b[i] !== 0xff) return null;
      const marker = b[i + 1];
      const length = b.readUInt16BE(i + 2);
      if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) {
        return { width: b.readUInt16BE(i + 7), height: b.readUInt16BE(i + 5) };
      }
      i += 2 + length;
    }
  }
  return null;
}

function escapeAttribute(text) {
  return String(text).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** An <img> for a snapshot attachment at its CSS size: `name-2x.*` (a scale-2 capture) is halved. */
function screenshotTag(snapshot, image, size, alt) {
  if (!safePath(image.path) || !size?.width) return null;
  const scale = Number(/-([1-4])x\./.exec(image.path.split('/').at(-1))?.[1] ?? 1);
  const width = Math.max(1, Math.min(MAX_IMAGE_WIDTH, Math.round(size.width / scale)));
  const url = `https://raw.githubusercontent.com/${encodeSegment(snapshot.owner)}/${encodeSegment(snapshot.repo)}/${snapshot.revision}/${image.path.split('/').map(encodeSegment).join('/')}`;
  return `<img src="${url}" alt="${escapeAttribute(alt)}" width="${width}">`;
}

/**
 * The release page's change list. With ticket details, subjects are grouped
 * into Features, Bug fixes, Chores and Other changes by the type of the first
 * ticket they name, and features show up to two attached screenshots. Without
 * details (no snapshot) it stays one alphabetical list.
 */
async function renderReleaseChanges({ github, owner, repo, subjects, remaining = 0, snapshot, details, warn = () => {} }) {
  const line = (subject) => `- ${linkTicketReferences(subject, snapshot)}`;
  const tail = remaining > 0 ? [`- …and ${remaining} more`] : [];
  if (!details?.size) return [...subjects.map(line), ...tail];

  const groups = new Map([...SECTIONS.map((section) => [section.title, []]), [OTHER_SECTION, []]]);
  for (const subject of subjects) {
    const ticket = ticketIdsIn(subject, snapshot).map((id) => ({ id, ...details.get(id) })).find((t) => t.type);
    const type = (ticket?.type ?? '').toLowerCase();
    const section = SECTIONS.find((s) => s.type === type)?.title ?? OTHER_SECTION;
    groups.get(section).push({ subject, ticket });
  }

  let imageBudget = IMAGES_PER_RELEASE;
  let imagesFailed = false;
  const lines = [];
  for (const [title, entries] of groups) {
    if (!entries.length) continue;
    if (lines.length) lines.push('');
    lines.push(`### ${title}`);
    for (const { subject, ticket } of entries) {
      lines.push(line(subject));
      if (title !== 'Features' || !ticket) continue;
      const tags = [];
      for (const name of ticket.attachments ?? []) {
        if (tags.length >= IMAGES_PER_FEATURE || imageBudget <= 0) break;
        const image = snapshot.images?.get(name);
        if (!image || !SHA.test(image.sha)) continue;
        try {
          const tag = screenshotTag(snapshot, image, imageSize(await readBlob(github, owner, repo, image.sha)), `${ticket.id} ${ticket.title ?? ''}`.trim());
          if (tag) {
            tags.push(tag);
            imageBudget -= 1;
          }
        } catch {
          imagesFailed = true;
        }
      }
      if (tags.length) lines.push(`  <br>${tags.join(' ')}`);
    }
  }
  if (imagesFailed) warn('Some release screenshots could not be read and were left out.');
  // A blank line separates the first heading from the "Changes:" label.
  return ['', ...lines, ...tail];
}

module.exports = {
  imageSize,
  linkTicketReferences,
  loadTicketDetails,
  loadTicketSnapshot,
  parseTicketYaml,
  releaseCommitSubjects,
  renderReleaseChanges,
  ticketIdsIn,
};
