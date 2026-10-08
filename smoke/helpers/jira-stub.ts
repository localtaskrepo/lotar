import http from 'node:http';
import { AddressInfo } from 'node:net';

/** A single request recorded by the stub, with the exact query the backend sent. */
export interface StubRequest {
    readonly method: string;
    readonly path: string;
    readonly query: Record<string, string>;
    readonly body: unknown;
    readonly authorization: string | undefined;
}

export interface JiraIssue {
    readonly key: string;
    readonly summary: string;
}

export interface JiraStubOptions {
    /** Jira project key the stub advertises for issue creation. */
    readonly projectKey: string;
    /** Issues returned by /rest/api/3/search/jql. */
    readonly issues?: readonly JiraIssue[];
}

const HOST = '127.0.0.1';
/** Safety net only: a held request auto-releases so a failing test cannot leak
 * a hung connection past teardown. Real releases come from the test body. */
const BARRIER_AUTO_RELEASE_MS = 30_000;

/**
 * Minimal local HTTP Jira stub owned by the smoke suite. It speaks exactly the
 * REST surface the backend sync client uses (search/jql, project, issue,
 * issuetype, transitions), records every request (method, path, query, body,
 * Authorization header) so specs can assert real request scope, and supports a
 * deterministic barrier: while armed, search requests are recorded and HELD
 * until the test releases them, keeping a backend sync run observably active
 * without any sleeps. No request ever leaves loopback.
 */
export class JiraStub {
    static async start(options: JiraStubOptions): Promise<JiraStub> {
        const stub = new JiraStub(options);
        await new Promise<void>((resolve, reject) => {
            stub.server.once('error', reject);
            stub.server.listen(0, HOST, () => resolve());
        });
        stub.port = (stub.server.address() as AddressInfo).port;
        return stub;
    }

    private readonly requests: StubRequest[] = [];
    private readonly waiters: Array<(request: StubRequest) => void> = [];
    private readonly issues: JiraIssue[];
    private readonly server: http.Server;
    private readonly projectKey: string;
    /** Base URL the seeded auth profiles point at. */
    get url(): string { return `http://${HOST}:${this.port}`; }
    private port = 0;
    private nextCreatedId = 4000;
    private gate: Promise<void> | null = null;
    private gateRelease: (() => void) | null = null;

    private constructor(options: JiraStubOptions) {
        this.projectKey = options.projectKey;
        this.issues = [...(options.issues ?? [])];
        this.server = http.createServer((req, res) => {
            void this.handle(req, res);
        });
    }

    /** Resolves with the record of the first matching request (bounded wait). */
    async waitForRequest(
        match: (request: StubRequest) => boolean,
        timeoutMs = 15_000,
    ): Promise<StubRequest> {
        const existing = this.requests.find(match);
        if (existing) {
            return existing;
        }
        return new Promise<StubRequest>((resolve, reject) => {
            const timer = setTimeout(() => {
                const index = this.waiters.indexOf(waiter);
                if (index >= 0) {
                    this.waiters.splice(index, 1);
                }
                reject(new Error(`Timed out waiting for stub request; saw ${JSON.stringify(this.requests.map((r) => `${r.method} ${r.path}`))}`));
            }, timeoutMs);
            const waiter = (request: StubRequest) => {
                if (!match(request)) {
                    return;
                }
                clearTimeout(timer);
                const index = this.waiters.indexOf(waiter);
                if (index >= 0) {
                    this.waiters.splice(index, 1);
                }
                resolve(request);
            };
            this.waiters.push(waiter);
        });
    }

    /** Hold every incoming search request until `release()` (or the safety timer). */
    armSearchBarrier(): void {
        if (this.gate) {
            throw new Error('Search barrier is already armed');
        }
        this.gate = new Promise((resolve) => {
            this.gateRelease = resolve;
        });
    }

    releaseSearchBarrier(): void {
        const release = this.gateRelease;
        this.gate = null;
        this.gateRelease = null;
        release?.();
    }

    allRequests(): readonly StubRequest[] {
        return this.requests;
    }

    searchRequests(): StubRequest[] {
        return this.requests.filter((request) => request.path === '/rest/api/3/search/jql');
    }

    async stop(): Promise<void> {
        this.releaseSearchBarrier();
        this.server.closeAllConnections?.();
        await new Promise<void>((resolve) => {
            this.server.close(() => resolve());
        });
    }

    private async handle(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
        const url = new URL(req.url ?? '/', this.url);
        const path = url.pathname;
        const query: Record<string, string> = {};
        url.searchParams.forEach((value, key) => {
            query[key] = value;
        });

        let body: unknown = null;
        if (req.method === 'POST' || req.method === 'PUT') {
            const raw = await readBody(req);
            if (raw.length) {
                try {
                    body = JSON.parse(raw);
                } catch {
                    body = raw;
                }
            }
        }

        const record: StubRequest = {
            method: req.method ?? 'GET',
            path,
            query,
            body,
            authorization: req.headers.authorization,
        };
        this.requests.push(record);
        for (const waiter of [...this.waiters]) {
            waiter(record);
        }

        if (path === '/rest/api/3/project' || path.startsWith('/rest/api/3/project/')) {
            const key = path.split('/').pop() ?? this.projectKey;
            this.sendJson(res, 200, { id: '10000', key });
            return;
        }
        if (path === '/rest/api/3/issuetype/project') {
            this.sendJson(res, 200, [{ id: '10001', name: 'Task', subtask: false }]);
            return;
        }
        if (path === '/rest/api/3/search/jql') {
            if (this.gate) {
                const safety = new Promise((resolve) => setTimeout(resolve, BARRIER_AUTO_RELEASE_MS));
                await Promise.race([this.gate, safety]);
            }
            const startAt = Number.parseInt(query.startAt ?? '0', 10) || 0;
            const maxResults = Number.parseInt(query.maxResults ?? '50', 10) || 50;
            const issues = this.issues.map(toJiraIssueJson).slice(startAt, startAt + maxResults);
            this.sendJson(res, 200, { total: this.issues.length, issues });
            return;
        }
        if (path.startsWith('/rest/api/3/issue/')) {
            const segments = path.slice('/rest/api/3/issue/'.length).split('/');
            const key = segments[0];
            if (segments[1] === 'transitions') {
                if (req.method === 'POST') {
                    this.sendJson(res, 204, {});
                    return;
                }
                this.sendJson(res, 200, {
                    transitions: [{ id: '31', name: 'Done' }, { id: '11', name: 'In Progress' }],
                });
                return;
            }
            if (req.method === 'PUT') {
                this.sendJson(res, 204, {});
                return;
            }
            const issue = this.issues.find((entry) => entry.key === key);
            if (!issue) {
                this.sendJson(res, 404, { errors: { issue: 'not found' } });
                return;
            }
            this.sendJson(res, 200, toJiraIssueJson(issue));
            return;
        }
        if (path === '/rest/api/3/issue' && req.method === 'POST') {
            const key = `${this.projectKey}-${this.nextCreatedId}`;
            this.nextCreatedId += 1;
            const summary =
                (body &&
                    typeof body === 'object' &&
                    (body as { fields?: { summary?: unknown } }).fields?.summary) ||
                key;
            this.issues.push({ key, summary: String(summary) });
            this.sendJson(res, 201, { id: String(this.nextCreatedId), key });
            return;
        }

        this.sendJson(res, 404, { errors: { path: 'not found' } });
    }

    private sendJson(res: http.ServerResponse, status: number, payload: unknown): void {
        const body = status === 204 ? '' : JSON.stringify(payload);
        res.writeHead(status, {
            'Content-Type': 'application/json',
            ...(body ? { 'Content-Length': Buffer.byteLength(body) } : {}),
        });
        res.end(body);
    }
}

/**
 * Reserve a loopback port and immediately free it, yielding an endpoint that
 * deterministically refuses connections (a "dead remote") without touching any
 * network beyond 127.0.0.1.
 */
export async function deadLoopbackPort(): Promise<number> {
    const probe = http.createServer(() => {});
    const port = await new Promise<number>((resolve, reject) => {
        probe.once('error', reject);
        probe.listen(0, HOST, () => {
            resolve((probe.address() as AddressInfo).port);
        });
    });
    await new Promise<void>((resolve) => probe.close(() => resolve()));
    return port;
}

function toJiraIssueJson(issue: JiraIssue) {
    return {
        id: issue.key,
        key: issue.key,
        fields: {
            summary: issue.summary,
            status: { name: 'To Do' },
            issuetype: { name: 'Task' },
        },
    };
}

function readBody(req: http.IncomingMessage): Promise<string> {
    return new Promise((resolve) => {
        let data = '';
        req.setEncoding('utf8');
        req.on('data', (chunk: string) => {
            data += chunk;
        });
        req.on('end', () => resolve(data));
        req.on('error', () => resolve(data));
    });
}
