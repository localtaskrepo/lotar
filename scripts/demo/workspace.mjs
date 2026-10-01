// Seed a realistic, deterministic LoTaR demo workspace for screenshots and README
// recordings: one product ("Atlas", a notes app with offline sync), three people,
// one stub agent profile, four sprints (two closed, one active, one planned), authored
// comments, code references, TODOs for the scanner, and a backdated git history.
//
// Everything is driven through the `lotar` CLI except the clock: each operation runs
// "now", then the timestamps it wrote are rewritten to the operation's simulated time
// and the change is committed with that author and date. That gives real burndown,
// history, and git stats without faking the task model.
//
// Used by `scripts/generate-test-tasks.mjs --demo` and `scripts/screenshots.mjs`.

import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const STUB_AGENT_SCRIPT = path.join(HERE, 'stub-agent.mjs');
export const DEMO_PROJECT = 'ATLS';

const PEOPLE = {
    priya: { name: 'Priya Natarajan', email: 'priya@atlas.example' },
    marco: { name: 'Marco Bianchi', email: 'marco@atlas.example' },
    jun: { name: 'Jun Park', email: 'jun@atlas.example' },
};

const DAY_MS = 86_400_000;

// ---------------------------------------------------------------------------
// Scenario data
// ---------------------------------------------------------------------------

// Sprints: offsets are days relative to today.
const SPRINTS = [
    { key: 's21', label: 'Sprint 21', goal: 'CRDT note model and durable sync queue', start: -34, end: -21, close: -21 },
    { key: 's22', label: 'Sprint 22', goal: 'Conflict resolution and search API', start: -20, end: -7, close: -7 },
    { key: 's23', label: 'Sprint 23', goal: 'Reliable offline sync and search-as-you-type', start: -6, end: 7 },
    { key: 's24', label: 'Sprint 24', goal: 'Shared notebooks', start: 8, end: 21, planDay: -2 },
];

// flow: [day, actor, status]; comments: [day, actor, text]; due: day offset.
const TASKS = [
    // Sprint 21 (closed)
    { key: 'crdt', title: 'Adopt a CRDT document model for notes', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'marco', effort: '5pt', tags: ['sync', 'backend'], component: 'sync', created: -38, sprint: 's21',
        flow: [[-33, 'marco', 'InProgress'], [-27, 'marco', 'NeedsReview'], [-26, 'priya', 'Done']],
        comments: [[-30, 'marco', 'Going with Automerge-style text CRDT; benchmarks on a 20k-word note are within budget.']] },
    { key: 'idb', title: 'Persist the sync queue to IndexedDB', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'jun', effort: '3pt', tags: ['sync', 'frontend'], component: 'web', created: -37, sprint: 's21',
        flow: [[-32, 'jun', 'InProgress'], [-29, 'jun', 'NeedsReview'], [-28, 'priya', 'Done']] },
    { key: 'plus', title: 'Login fails when the email contains a plus sign', type: 'Bug', priority: 'Critical', reporter: 'jun', assignee: 'marco', effort: '1pt', tags: ['auth'], component: 'api', created: -35, sprint: 's21',
        flow: [[-34, 'marco', 'InProgress'], [-34, 'marco', 'Done']],
        comments: [[-34, 'marco', 'URL-decoding turned + into a space before lookup. Fixed and added a regression test.']] },
    { key: 'footnotes', title: 'Markdown preview strips footnotes', type: 'Bug', priority: 'Medium', reporter: 'priya', assignee: 'jun', effort: '2pt', tags: ['editor'], component: 'web', created: -36, sprint: 's21',
        flow: [[-30, 'jun', 'InProgress'], [-25, 'jun', 'Done']] },
    { key: 'backup', title: 'Nightly Postgres backups with 30-day retention', type: 'Chore', priority: 'Medium', reporter: 'marco', assignee: 'marco', effort: '2pt', tags: ['infra'], component: 'infra', created: -36, sprint: 's21',
        flow: [[-24, 'marco', 'InProgress'], [-23, 'marco', 'Done']] },
    { key: 'tantivy', title: 'Spike: evaluate Tantivy for full-text search', type: 'Spike', priority: 'Medium', reporter: 'priya', assignee: 'marco', effort: '3pt', tags: ['search'], component: 'search', created: -36, sprint: 's21',
        flow: [[-28, 'marco', 'InProgress'], [-22, 'marco', 'Done']],
        comments: [[-22, 'marco', 'Tantivy indexes our 1M-note fixture in 41s and answers p95 queries in 8ms. Recommending it.']] },

    // Sprint 22 (closed)
    { key: 'conflicts', title: 'Resolve concurrent title edits without losing either change', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'marco', effort: '5pt', tags: ['sync'], component: 'sync', created: -24, sprint: 's22',
        flow: [[-19, 'marco', 'InProgress'], [-13, 'marco', 'NeedsReview'], [-12, 'priya', 'Done']] },
    { key: 'searchapi', title: 'Full-text search API endpoint', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'marco', effort: '5pt', tags: ['search', 'backend'], component: 'search', created: -23, sprint: 's22',
        flow: [[-17, 'marco', 'InProgress'], [-11, 'marco', 'NeedsReview'], [-10, 'priya', 'Done']] },
    { key: 'share', title: 'iOS share extension for saving links', type: 'Feature', priority: 'Medium', reporter: 'priya', assignee: 'jun', effort: '3pt', tags: ['ios'], component: 'ios', created: -23, sprint: 's22',
        flow: [[-18, 'jun', 'InProgress'], [-14, 'jun', 'Done']] },
    { key: 'uploads', title: 'Attachment uploads time out on slow networks', type: 'Bug', priority: 'High', reporter: 'priya', assignee: 'jun', effort: '3pt', tags: ['attachments'], component: 'web', created: -22, sprint: 's22',
        flow: [[-16, 'jun', 'InProgress'], [-9, 'jun', 'Done']],
        comments: [[-15, 'jun', 'Switching to resumable chunked uploads (5 MB parts) instead of raising the timeout.']] },
    { key: 'ratelimit', title: 'Rate-limit public share links', type: 'Feature', priority: 'Medium', reporter: 'marco', assignee: 'marco', effort: '2pt', tags: ['security', 'backend'], component: 'api', created: -22, sprint: 's22',
        flow: [[-10, 'marco', 'InProgress'], [-8, 'marco', 'Done']] },
    { key: 'snippet', title: 'Search results highlight the wrong snippet', type: 'Bug', priority: 'Medium', reporter: 'jun', assignee: 'jun', effort: '2pt', tags: ['search'], component: 'search', created: -21, sprint: 's22', moveTo: 's23',
        flow: [[-9, 'jun', 'InProgress'], [-2, 'jun', 'Done']] },

    // Sprint 23 (active)
    { key: 'offline', title: 'Offline edits are lost when reconnecting mid-sync', type: 'Bug', priority: 'Critical', reporter: 'priya', assignee: 'marco', effort: '3pt', tags: ['sync', 'customer'], component: 'sync', created: -8, sprint: 's23', due: -3,
        description: 'Edits made while offline disappear if the app reconnects while a sync is already in flight.\n\n**Repro**\n1. Open a note on iOS, enable airplane mode\n2. Edit the note, then disable airplane mode during the next sync\n3. The offline edits are gone after the sync completes\n\nSeen by three customers this week.',
        flow: [[-6, 'marco', 'InProgress'], [-5, 'marco', 'NeedsReview'], [-4, 'priya', 'Done']],
        comments: [
            [-7, 'priya', 'Three customer reports this week, all after airplane mode on iOS. Top priority for Sprint 23.'],
            [-6, 'marco', 'Root cause: the queue flush and the reconnect handshake race; the handshake resets the cursor.'],
            [-5, 'marco', 'Fix serializes flush before handshake. Added a property test that replays 500 random offline sessions.'],
            [-4, 'priya', 'Verified on TestFlight build 2.8.1. Shipping.'],
        ],
        links: ['https://github.com/atlas-notes/atlas/pull/412'], code: ['web/src/sync/queue.ts#20-33'] },
    { key: 'sayt', title: 'Search-as-you-type in the web app', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'jun', effort: '5pt', tags: ['search', 'frontend'], component: 'web', created: -9, sprint: 's23', due: 5,
        description: 'Show results while the user types, backed by the new search API. Keep the input responsive on 10k+ note workspaces.',
        flow: [[-4, 'jun', 'InProgress']],
        comments: [[-3, 'jun', 'Debounced at 120ms; results stream in as the index answers. Need a design pass on empty states.']] },
    { key: 'cursor', title: 'Paginate the notes API with stable cursors', type: 'Feature', priority: 'High', reporter: 'marco', assignee: 'marco', effort: '3pt', tags: ['backend'], component: 'api', created: -10, sprint: 's23', due: 2,
        flow: [[-3, 'marco', 'InProgress'], [-1, 'marco', 'NeedsReview']],
        code: ['api/src/routes/notes.rs#3-7'] },
    { key: 'kbd', title: 'iOS: hardware keyboard shortcuts in the editor', type: 'Feature', priority: 'Medium', reporter: 'jun', assignee: 'jun', effort: '3pt', tags: ['ios', 'editor'], component: 'ios', created: -9, sprint: 's23', due: 6 },
    { key: 'history', title: 'Note version history panel', type: 'Feature', priority: 'Medium', reporter: 'priya', assignee: 'jun', effort: '5pt', tags: ['frontend'], component: 'web', created: -11, sprint: 's23', due: 7,
        flow: [[-2, 'jun', 'InProgress']] },
    { key: 'pg17', title: 'Upgrade to Postgres 17', type: 'Chore', priority: 'Low', reporter: 'marco', assignee: 'marco', effort: '2pt', tags: ['infra'], component: 'infra', created: -12, sprint: 's23',
        flow: [[-6, 'marco', 'InProgress'], [-5, 'marco', 'Done']] },
    { key: 'accents', title: 'Tag autocomplete ignores accented characters', type: 'Bug', priority: 'Medium', reporter: 'jun', assignee: 'jun', effort: '1pt', tags: ['editor', 'i18n'], component: 'web', created: -10, sprint: 's23',
        flow: [[-2, 'jun', 'InProgress'], [-1, 'jun', 'Done']] },
    { key: 'guide', title: 'Write the migration guide for sync protocol v2', type: 'Chore', priority: 'Medium', reporter: 'priya', assignee: 'priya', effort: '2pt', tags: ['docs'], component: 'sync', created: -8, sprint: 's23', due: 4,
        flow: [[-3, 'priya', 'InProgress']] },
    { key: 'sharecrash', title: 'Crash when deleting a note that has an open share link', type: 'Bug', priority: 'High', reporter: 'priya', assignee: 'marco', effort: '2pt', tags: ['backend'], component: 'api', created: -7, sprint: 's23', due: 1,
        flow: [[-4, 'marco', 'InProgress'], [-3, 'marco', 'Blocked']],
        comments: [[-3, 'marco', 'Blocked on the share-link revocation job; it still runs on the old worker pool.']] },
    { key: 'encrypt', title: 'Encrypt attachments at rest', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'marco', effort: '5pt', tags: ['security', 'attachments'], component: 'api', created: -9, sprint: 's23', due: 9 },

    // Sprint 24 (planned)
    { key: 'notebooks', title: 'Shared notebooks with per-member permissions', type: 'Epic', priority: 'High', reporter: 'priya', effort: '13pt', tags: ['collaboration'], component: 'api', created: -5, sprint: 's24' },
    { key: 'invite', title: 'Invite flow for shared notebooks', type: 'Feature', priority: 'High', reporter: 'priya', assignee: 'jun', effort: '5pt', tags: ['collaboration', 'frontend'], component: 'web', created: -4, sprint: 's24', due: 12 },
    { key: 'pdf', title: 'Export a notebook as PDF', type: 'Feature', priority: 'Medium', reporter: 'jun', effort: '3pt', tags: ['export'], component: 'web', created: -4, sprint: 's24', due: 18 },
    { key: 'acl', title: 'Permission checks for notebook-scoped API routes', type: 'Feature', priority: 'High', reporter: 'marco', assignee: 'marco', effort: '5pt', tags: ['collaboration', 'backend', 'security'], component: 'api', created: -3, sprint: 's24', due: 15 },

    // Backlog
    { key: 'coldstart', title: 'Slow cold start on iOS with 10k+ notes', type: 'Bug', priority: 'Medium', reporter: 'jun', effort: '3pt', tags: ['ios', 'performance'], component: 'ios', created: -15,
        comments: [[-14, 'jun', 'Profiled: 1.8s spent decoding the full note index on launch. Lazy-loading would fix most of it.']] },
    { key: 'e2ee', title: 'Spike: key recovery for end-to-end encryption', type: 'Spike', priority: 'High', reporter: 'priya', effort: '3pt', tags: ['security'], component: 'api', created: -13 },
    { key: 'onboarding', title: 'Onboarding checklist for new workspaces', type: 'Feature', priority: 'Low', reporter: 'priya', effort: '2pt', tags: ['growth'], component: 'web', created: -12 },
    { key: 'legacy', title: 'Remove the legacy v1 sync endpoints', type: 'Chore', priority: 'Low', reporter: 'marco', effort: '2pt', tags: ['tech-debt', 'backend'], component: 'api', created: -11, due: 25 },
    { key: 'android', title: 'Android app: read-only beta', type: 'Epic', priority: 'Medium', reporter: 'priya', effort: '20pt', tags: ['android'], component: 'android', created: -16 },
];

// Source files committed to the demo repo. TODOs in web/src/sync are converted into a
// task during seeding; the others stay for the Scan page to find.
const SOURCE_FILES = {
    'README.md': '# Atlas\n\nNotes that sync everywhere, even offline.\n',
    'web/src/sync/queue.ts': `import { openQueueStore } from './store';

export interface PendingOp {
    noteId: string;
    patch: Uint8Array;
    attempt: number;
}

const RETRY_MS = 5_000;

export class SyncQueue {
    private flushing = false;

    constructor(private readonly store = openQueueStore()) {}

    async enqueue(op: PendingOp): Promise<void> {
        await this.store.put(op);
    }

    async flush(send: (op: PendingOp) => Promise<void>): Promise<void> {
        if (this.flushing) return;
        this.flushing = true;
        try {
            for (const op of await this.store.pending()) {
                await send(op);
                await this.store.remove(op);
            }
        } catch {
            // TODO: Exponential backoff with jitter for sync retries
            setTimeout(() => void this.flush(send), RETRY_MS);
        } finally {
            this.flushing = false;
        }
    }
}
`,
    'web/src/sync/store.ts': `export function openQueueStore() {
    // FIXME: IndexedDB quota errors are swallowed; surface them to the user
    return {
        put: async (_op: unknown) => {},
        pending: async () => [] as never[],
        remove: async (_op: unknown) => {},
    };
}
`,
    'web/src/editor/Toolbar.tsx': `export function Toolbar() {
    // TODO: Keyboard shortcut hints in toolbar tooltips
    // HACK: force a reflow so Safari repaints the active button state
    return null;
}
`,
    'api/src/routes/notes.rs': `//! Notes REST routes.

pub fn list_notes(cursor: Option<&str>, limit: usize) -> Vec<String> {
    // FIXME: Cursor pagination skips notes deleted during a scroll
    let _ = (cursor, limit);
    Vec::new()
}

pub fn delete_note(id: &str) {
    // TODO: Revoke share links when a note is deleted
    let _ = id;
}
`,
    'ios/Atlas/EditorView.swift': `import SwiftUI

struct EditorView: View {
    var body: some View {
        // TODO: Support hardware keyboard shortcuts (cmd-B, cmd-I, cmd-K)
        Text("Editor")
    }
}
`,
};

const GLOBAL_CONFIG = `default:
  project: ${DEMO_PROJECT}
issue:
  states: [Todo, InProgress, NeedsReview, Blocked, Done]
  types: [Feature, Bug, Chore, Epic, Spike]
  priorities: [Low, Medium, High, Critical]
custom_fields: [component]
members: [priya, marco, jun]
auto:
  assign_on_status: false
  codeowners_assign: false
  tags_from_path: false
  branch_infer_type: false
  branch_infer_status: false
  branch_infer_priority: false
`;

const PROJECT_CONFIG = `project:
  name: Atlas
remotes:
  github:
    provider: github
    repo: atlas-notes/atlas
    auth_profile: github.atlas
    filter: "label:customer"
  jira:
    provider: jira
    project: ATL
    auth_profile: jira.atlas
`;

const AUTOMATION_YAML = `automation:
  rules:
    - name: Agent lifecycle
      when:
        assignee: "@agent"
      on:
        job_started:
          set:
            status: InProgress
        job_completed:
          set:
            status: NeedsReview
            assignee: "@reporter"
          comment: "Agent finished on \${{agent.worktree_branch}}. Ready for review."
        job_failed:
          set:
            status: Blocked
          add:
            tags: [needs-human]
    - name: Critical bugs page on-call
      when:
        all:
          - type: Bug
          - priority: Critical
      on:
        created:
          add:
            tags: [hotfix]
          comment: "Critical bug filed. Tagged hotfix; on-call is \${{ticket.assignee}}."
    - name: Review handoff
      when:
        changes:
          status: { from: InProgress, to: NeedsReview }
      on:
        updated:
          comment: "Moved to review. @\${{ticket.reporter}} please take a look."
`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function startOfTodayUtc(now) {
    const d = new Date(now);
    d.setUTCHours(0, 0, 0, 0);
    return d.getTime();
}

const TS_RE = /\b\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})/g;

function formatTs(ms) {
    return `${new Date(ms).toISOString().slice(0, 23)}000+00:00`;
}

function listYaml(dir) {
    const out = [];
    if (!existsSync(dir)) return out;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) out.push(...listYaml(full));
        else if (entry.name.endsWith('.yml')) out.push(full);
    }
    return out;
}

function writeLauncher(dir) {
    const binDir = path.join(dir, '.demo');
    mkdirSync(binDir, { recursive: true });
    if (process.platform === 'win32') {
        const launcher = path.join(binDir, 'claude-stub.cmd');
        writeFileSync(launcher, `@"${process.execPath}" "${STUB_AGENT_SCRIPT}" %*\r\n`);
        return launcher;
    }
    const launcher = path.join(binDir, 'claude-stub');
    writeFileSync(launcher, `#!${process.execPath}\nawait import(${JSON.stringify(pathToFileURL(STUB_AGENT_SCRIPT).href)});\n`);
    chmodSync(launcher, 0o755);
    return launcher;
}

/**
 * Seed the demo workspace into `dir` (created or replaced).
 *
 * @param {{ dir: string, bin: string, now?: number, log?: (msg: string) => void }} opts
 * @returns {{ dir: string, project: string, ids: Record<string, string>, holdFile: string }}
 */
export function seedDemoWorkspace({ dir, bin, now = Date.now(), log = () => {} }) {
    if (existsSync(dir)) rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });

    const today = startOfTodayUtc(now);
    const at = (day, hour = 10, minute = 0) => today + day * DAY_MS + (hour * 60 + minute) * 60_000;
    const seedStart = Date.now() - 1000;
    const tasksDir = path.join(dir, '.tasks');

    const baseEnv = {
        ...process.env,
        LOTAR_IGNORE_HOME_CONFIG: '1',
        LOTAR_TASKS_DIR: tasksDir,
        GIT_CONFIG_NOSYSTEM: '1',
        GIT_CONFIG_GLOBAL: path.join(dir, '.demo', 'gitconfig'),
    };
    delete baseEnv.LOTAR_PROJECT;
    delete baseEnv.LOTAR_DEFAULT_PROJECT;

    const git = (args, extraEnv = {}) => {
        const res = spawnSync('git', args, { cwd: dir, env: { ...baseEnv, ...extraEnv }, encoding: 'utf8' });
        if (res.status !== 0) throw new Error(`git ${args.join(' ')} failed: ${res.stderr || res.stdout}`);
        return res.stdout;
    };

    const lotar = (actor, args) => {
        const res = spawnSync(bin, args, {
            cwd: dir,
            env: { ...baseEnv, LOTAR_DEFAULT_REPORTER: actor },
            encoding: 'utf8',
        });
        if (res.status !== 0) {
            throw new Error(`lotar ${args.join(' ')} failed (${res.status}): ${res.stderr || res.stdout}`);
        }
        return res.stdout;
    };

    // Rewrite timestamps written by the last operation (wall-clock "now" during
    // seeding) to its simulated time. Planned future dates are left alone.
    const backdate = (whenMs) => {
        const writtenUntil = Date.now() + 1000;
        const fresh = (ts) => {
            const ms = Date.parse(ts);
            return ms >= seedStart && ms <= writtenUntil;
        };
        for (const file of listYaml(tasksDir)) {
            const raw = readFileSync(file, 'utf8');
            const next = raw.replace(TS_RE, (ts) => (fresh(ts) ? formatTs(whenMs) : ts));
            if (next !== raw) writeFileSync(file, next);
        }
    };

    const commit = (actor, whenMs, message) => {
        git(['add', '-A']);
        const staged = spawnSync('git', ['diff', '--cached', '--quiet'], { cwd: dir, env: baseEnv });
        if (staged.status === 0) return;
        const person = PEOPLE[actor] ?? PEOPLE.priya;
        const date = new Date(whenMs).toISOString();
        git(['commit', '-q', '-m', message], {
            GIT_AUTHOR_NAME: person.name,
            GIT_AUTHOR_EMAIL: person.email,
            GIT_COMMITTER_NAME: person.name,
            GIT_COMMITTER_EMAIL: person.email,
            GIT_AUTHOR_DATE: date,
            GIT_COMMITTER_DATE: date,
        });
    };

    // --- repository skeleton -------------------------------------------------
    mkdirSync(path.join(dir, '.demo'), { recursive: true });
    writeFileSync(path.join(dir, '.demo', 'gitconfig'), '[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n');
    git(['init', '-q', '-b', 'main']);
    writeFileSync(path.join(dir, '.gitignore'), '.demo/\n*.lock\n.tasks/**/*.context\n');
    for (const [rel, body] of Object.entries(SOURCE_FILES)) {
        mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
        writeFileSync(path.join(dir, rel), body);
    }
    mkdirSync(path.join(tasksDir, DEMO_PROJECT), { recursive: true });
    writeFileSync(path.join(tasksDir, 'config.yml'), GLOBAL_CONFIG);
    writeFileSync(path.join(tasksDir, DEMO_PROJECT, 'config.yml'), PROJECT_CONFIG);
    writeFileSync(path.join(tasksDir, 'automation.yml'), AUTOMATION_YAML);
    commit('priya', at(-40, 9), 'Initial import of Atlas and the LoTaR workspace');

    // --- build the timeline ---------------------------------------------------
    const ops = [];
    const add = (when, actor, label, run) => ops.push({ when, actor, label, run, seq: ops.length });
    const ids = {};
    const sprintIds = {};
    const byKey = Object.fromEntries(TASKS.map((t) => [t.key, t]));

    TASKS.forEach((t, index) => {
        add(at(t.created, 9, index % 50), t.reporter, `${t.key}: create`, () => {
            const args = ['add', t.title, '-p', DEMO_PROJECT, '--type', t.type, '--priority', t.priority, '--effort', t.effort, '--field', `component=${t.component}`];
            if (t.assignee) args.push('--assignee', t.assignee);
            if (t.description) args.push('--description', t.description);
            for (const tag of t.tags ?? []) args.push('--tag', tag);
            if (t.due !== undefined) args.push('--due', new Date(today + t.due * DAY_MS).toISOString().slice(0, 10));
            const out = lotar(t.reporter, args);
            const match = out.match(/Created task:\s*([A-Z0-9_-]+)/i);
            if (!match) throw new Error(`could not parse task id for ${t.key}: ${out}`);
            ids[t.key] = match[1];
            return `${ids[t.key]}: ${t.title}`;
        });
        (t.flow ?? []).forEach(([day, actor, status], i) => {
            add(at(day, 11 + i * 2, index % 50), actor, `${t.key}: ${status}`, () => {
                lotar(actor, ['status', ids[t.key], status]);
                return `${ids[t.key]}: move to ${status}`;
            });
        });
        (t.comments ?? []).forEach(([day, actor, text], i) => {
            add(at(day, 14, 10 + i * 7), actor, `${t.key}: comment`, () => {
                lotar(actor, ['comment', ids[t.key], '-m', text]);
                return `${ids[t.key]}: comment`;
            });
        });
        (t.links ?? []).forEach((url) => {
            add(at((t.flow?.[0]?.[0] ?? t.created) + 0.2, 16), t.assignee ?? t.reporter, `${t.key}: link`, () => {
                lotar(t.assignee ?? t.reporter, ['task', 'reference', 'add', 'link', ids[t.key], url]);
                return `${ids[t.key]}: link ${url}`;
            });
        });
        (t.code ?? []).forEach((code) => {
            add(at((t.flow?.[0]?.[0] ?? t.created) + 0.2, 16, 30), t.assignee ?? t.reporter, `${t.key}: code`, () => {
                lotar(t.assignee ?? t.reporter, ['task', 'reference', 'add', 'code', ids[t.key], code]);
                return `${ids[t.key]}: reference ${code}`;
            });
        });
    });

    for (const s of SPRINTS) {
        const planDay = s.planDay ?? s.start - 1;
        add(at(planDay, 15), 'priya', `${s.key}: plan`, () => {
            const out = lotar('priya', [
                'sprint', 'create', '--label', s.label, '--goal', s.goal,
                '--starts-at', new Date(at(s.start, 9)).toISOString(),
                '--ends-at', new Date(at(s.end, 17)).toISOString(),
            ]);
            const match = out.match(/sprint #(\d+)/i);
            if (!match) throw new Error(`could not parse sprint id: ${out}`);
            sprintIds[s.key] = match[1];
            const members = TASKS.filter((t) => t.sprint === s.key).map((t) => ids[t.key]);
            if (members.length) lotar('priya', ['sprint', 'add', '--sprint', sprintIds[s.key], ...members]);
            return `Plan ${s.label}`;
        });
        if (s.start < 0) {
            add(at(s.start, 9, 30), 'priya', `${s.key}: start`, () => {
                const moved = TASKS.filter((t) => t.moveTo === s.key).map((t) => ids[t.key]);
                if (moved.length) lotar('priya', ['sprint', 'move', '--sprint', sprintIds[s.key], ...moved]);
                lotar('priya', ['sprint', 'start', sprintIds[s.key], '--at', new Date(at(s.start, 9, 30)).toISOString(), '--no-warn']);
                return `Start ${s.label}`;
            });
        }
        if (s.close !== undefined) {
            add(at(s.close, 17, 30), 'priya', `${s.key}: close`, () => {
                lotar('priya', ['sprint', 'close', sprintIds[s.key], '--at', new Date(at(s.close, 17, 30)).toISOString(), '--no-warn']);
                return `Close ${s.label}`;
            });
        }
    }

    // Turn the sync TODO into a task (writes the key back into the comment and adds a
    // code reference), then pull it into the active sprint.
    add(at(-12, 13), 'marco', 'scan sync', () => {
        lotar('marco', ['scan', 'web/src/sync/queue.ts']);
        const found = listYaml(path.join(tasksDir, DEMO_PROJECT)).find((f) => /^title: .*backoff/im.test(readFileSync(f, 'utf8')));
        if (!found) throw new Error('scan did not create the backoff task');
        ids.backoff = `${DEMO_PROJECT}-${path.basename(found, '.yml')}`;
        lotar('marco', ['task', 'edit', ids.backoff, '--priority', 'Medium', '--effort', '2pt', '--tag', 'sync', '--field', 'component=sync',
            '--description', 'The sync queue retries failed flushes every 5s forever. Under an outage every client hammers the API in lockstep. Use exponential backoff with full jitter, capped at 30s, and reset after a successful flush.']);
        return `${ids.backoff}: capture TODO from web/src/sync/queue.ts`;
    });
    add(at(-6, 9, 45), 'priya', 'backoff to sprint', () => {
        lotar('priya', ['sprint', 'add', '--sprint', sprintIds.s23, ids.backoff]);
        return `${ids.backoff}: add to Sprint 23`;
    });

    ops.sort((a, b) => a.when - b.when || a.seq - b.seq);
    for (const op of ops) {
        const message = op.run();
        backdate(op.when);
        commit(op.actor, op.when, message);
        log(`${new Date(op.when).toISOString().slice(0, 16)} ${op.actor.padEnd(5)} ${message}`);
    }

    // --- agent profile (added last so seeding never queues agent jobs) --------
    const launcher = writeLauncher(dir);
    const holdFile = path.join(dir, '.demo', 'hold');
    const worktrees = path.join(path.dirname(dir), `${path.basename(dir)}-worktrees`);
    const globalConfig = readFileSync(path.join(tasksDir, 'config.yml'), 'utf8');
    writeFileSync(path.join(tasksDir, 'config.yml'), `${globalConfig}agents:
  claude:
    runner: claude
    command: ${JSON.stringify(launcher)}
    env:
      DEMO_AGENT_STEP_MS: "650"
      DEMO_AGENT_HOLD_FILE: ${JSON.stringify(holdFile)}
agent:
  worktree:
    enabled: true
    dir: ${JSON.stringify(worktrees)}
    max_parallel_jobs: 2
`);
    commit('priya', at(-1, 18), 'Add the claude agent profile and worktree isolation');

    return { dir, project: DEMO_PROJECT, ids, sprintIds, holdFile, worktrees, people: PEOPLE, byKey };
}

/** Remove the demo workspace and the agent worktree directory next to it. */
export function removeDemoWorkspace(dir) {
    const worktrees = path.join(path.dirname(dir), `${path.basename(dir)}-worktrees`);
    for (const target of [dir, worktrees]) {
        if (existsSync(target) && statSync(target).isDirectory()) rmSync(target, { recursive: true, force: true });
    }
}
