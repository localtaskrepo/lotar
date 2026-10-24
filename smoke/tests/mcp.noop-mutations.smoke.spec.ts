import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { stringify } from 'yaml';
import { callTool, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-102 durable MCP stdio no-op regressions: `task_update` with an empty
// patch, same-value scalars, normalized effort/tags, null-clears of absent
// fields, an identical custom-field map, and `task_bulk_update` over
// unchanged tasks must all succeed through the shared service boundary
// WITHOUT writing: task YAML bytes (including user comments) and the file
// mtime stay frozen, `modified` and history stay untouched, and no
// empty-patch rejection or response-shape change is introduced —
// unchanged bulk entries keep returning successful DTOs. Genuine
// counterparts still write, and invalid patches / unknown ids keep
// failing with the existing envelopes.

const TASK_HEADER_COMMENT = '# User-owned header comment kept byte-for-byte.';
const FIXED_MODIFIED = '2026-10-01T00:00:00Z';

function seededTask(id: string, title: string, extra: Record<string, unknown> = {}): string {
    return [
        TASK_HEADER_COMMENT,
        stringify({
            title,
            status: 'Todo',
            priority: 'Medium',
            type: 'Feature',
            reporter: 'smoke',
            created: '2026-09-01T00:00:00Z',
            modified: FIXED_MODIFIED,
            ...extra,
        }),
    ].join('\n');
}

function seedFiles(): Record<string, string> {
    return {
        '.tasks/config.yml': [
            'default.project: MNP',
            'default.reporter: smoke',
            'issue.states: [Todo, InProgress, Done]',
            'issue.types: [Feature, Bug, Chore]',
            'issue.priorities: [Low, Medium, High]',
            'members: [smoke, alice]',
            'custom.fields: [product]',
        ].join('\n'),
        '.tasks/MNP/1.yml': seededTask('one', 'MCP no-op sentinel', {
            effort: '3.00h',
            tags: ['alpha'],
            custom_fields: { product: 'Core' },
        }),
        '.tasks/MNP/2.yml': seededTask('two', 'MCP bulk no-op sentinel'),
    };
}

async function fileState(file: string): Promise<{ bytes: string; mtimeMs: number }> {
    const [bytes, stat] = await Promise.all([fs.readFile(file, 'utf8'), fs.stat(file)]);
    return { bytes, mtimeMs: stat.mtimeMs };
}

describe.concurrent('MCP no-op mutations (DEV-102)', () => {
    it('task_update empty/same/normalized/null-clear patches keep the shape and freeze bytes', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'mcp-noop-', seedFiles: seedFiles() });
        try {
            const file = path.join(workspace.tasksDir, 'MNP/1.yml');
            const before = await fileState(file);

            await withMcpClient(workspace, async (client) => {
                // Empty patch {} is accepted (no empty-patch rejection) and
                // answers with the unchanged DTO — response shape unchanged.
                const empty = await callTool(client, 1, 'task_update', { id: 'MNP-1', patch: {} });
                expect(empty.error).toBeUndefined();
                expect(empty.result?.isError).not.toBe(true);
                const emptyDto = extractToolPayload(empty) as Record<string, any>;
                expect(emptyDto.id).toBe('MNP-1');
                expect(emptyDto.title).toBe('MCP no-op sentinel');
                expect(emptyDto.modified).toBe(FIXED_MODIFIED);

                // Same-value scalar, normalized effort (180m == 3.00h),
                // whitespace-normalized tags, null-clears of absent fields,
                // and an identical custom-field map.
                const sameTitle = await callTool(client, 2, 'task_update', {
                    id: 'MNP-1',
                    patch: { title: 'MCP no-op sentinel' },
                });
                expect(sameTitle.error).toBeUndefined();

                const normalizedEffort = await callTool(client, 3, 'task_update', {
                    id: 'MNP-1',
                    patch: { effort: '180m' },
                });
                expect(normalizedEffort.error).toBeUndefined();

                const normalizedTags = await callTool(client, 4, 'task_update', {
                    id: 'MNP-1',
                    patch: { tags: ['  alpha  '] },
                });
                expect(normalizedTags.error).toBeUndefined();

                const nullClears = await callTool(client, 5, 'task_update', {
                    id: 'MNP-1',
                    patch: { description: null, due_date: null, assignee: null },
                });
                expect(nullClears.error).toBeUndefined();

                const sameCustom = await callTool(client, 6, 'task_update', {
                    id: 'MNP-1',
                    patch: { custom_fields: { product: 'Core' } },
                });
                expect(sameCustom.error).toBeUndefined();
                const sameCustomDto = extractToolPayload(sameCustom) as Record<string, any>;
                expect(sameCustomDto.modified).toBe(FIXED_MODIFIED);

                // Padded task-id alias resolves to the canonical identity.
                const padded = await callTool(client, 7, 'task_update', { id: 'MNP-001', patch: {} });
                expect(padded.error).toBeUndefined();
                const paddedDto = extractToolPayload(padded) as Record<string, any>;
                expect(paddedDto.id).toBe('MNP-1');
            });

            const after = await fileState(file);
            expect(after.bytes).toBe(before.bytes);
            expect(after.mtimeMs).toBe(before.mtimeMs);
            expect(after.bytes).toContain(TASK_HEADER_COMMENT);

            // Genuine change through the same tool: writes and bumps modified.
            await withMcpClient(workspace, async (client) => {
                const genuine = await callTool(client, 1, 'task_update', {
                    id: 'MNP-1',
                    patch: { title: 'MCP real change' },
                });
                expect(genuine.error).toBeUndefined();
                const dto = extractToolPayload(genuine) as Record<string, any>;
                expect(dto.title).toBe('MCP real change');
                expect(dto.modified).not.toBe(FIXED_MODIFIED);
            });
            const changed = await fileState(file);
            expect(changed.bytes).not.toBe(before.bytes);
            expect(changed.bytes).toContain('title: MCP real change');
        } finally {
            await workspace.dispose();
        }
    });

    it('task_bulk_update returns unchanged tasks as successful DTOs without writing', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'mcp-bulk-noop-', seedFiles: seedFiles() });
        try {
            const fileOne = path.join(workspace.tasksDir, 'MNP/1.yml');
            const fileTwo = path.join(workspace.tasksDir, 'MNP/2.yml');
            const beforeOne = await fileState(fileOne);
            const beforeTwo = await fileState(fileTwo);

            await withMcpClient(workspace, async (client) => {
                // Empty patch across both tasks: both unchanged entries stay
                // in `updated` as successful DTOs — contract unchanged.
                const emptyBulk = await callTool(client, 1, 'task_bulk_update', {
                    ids: ['MNP-1', 'MNP-2'],
                    patch: {},
                });
                expect(emptyBulk.error).toBeUndefined();
                const payload = extractToolPayload(emptyBulk) as Record<string, any>;
                expect(Object.keys(payload).sort()).toEqual(['failed', 'status', 'updated']);
                expect(payload.status).toBe('ok');
                expect(payload.failed).toEqual([]);
                expect(payload.updated).toHaveLength(2);
                expect(new Set((payload.updated as any[]).map((entry: any) => entry.id))).toEqual(
                    new Set(['MNP-1', 'MNP-2']),
                );
                for (const entry of payload.updated as any[]) {
                    expect(entry.modified).toBe(FIXED_MODIFIED);
                }

                // Same-value patch for both tasks: still a no-op.
                const sameBulk = await callTool(client, 2, 'task_bulk_update', {
                    ids: ['MNP-1', 'MNP-2'],
                    patch: { reporter: 'smoke' },
                });
                expect(sameBulk.error).toBeUndefined();
                const samePayload = extractToolPayload(sameBulk) as Record<string, any>;
                expect(samePayload.failed).toEqual([]);
                expect(samePayload.updated).toHaveLength(2);

                // Unknown ids surface as per-item failures; known tasks in
                // the same batch still succeed.
                const mixed = await callTool(client, 3, 'task_bulk_update', {
                    ids: ['MNP-1', 'MNP-404'],
                    patch: {},
                });
                const mixedPayload = extractToolPayload(mixed) as Record<string, any>;
                expect(mixedPayload.updated).toHaveLength(1);
                expect(mixedPayload.failed).toHaveLength(1);
                expect((mixedPayload.failed[0] as any).id).toBe('MNP-404');
            });

            expect(await fileState(fileOne)).toEqual(beforeOne);
            expect(await fileState(fileTwo)).toEqual(beforeTwo);

            // Genuine bulk change: both tasks write and reflect the patch.
            await withMcpClient(workspace, async (client) => {
                const genuine = await callTool(client, 1, 'task_bulk_update', {
                    ids: ['MNP-1', 'MNP-2'],
                    patch: { priority: 'High' },
                });
                const payload = extractToolPayload(genuine) as Record<string, any>;
                expect(payload.failed).toEqual([]);
                expect(payload.updated).toHaveLength(2);
                for (const entry of payload.updated as any[]) {
                    expect(entry.priority).toBe('High');
                    expect(entry.modified).not.toBe(FIXED_MODIFIED);
                }
            });
            expect((await fileState(fileOne)).bytes).not.toBe(beforeOne.bytes);
            expect((await fileState(fileTwo)).bytes).not.toBe(beforeTwo.bytes);
        } finally {
            await workspace.dispose();
        }
    });

    it('comment update with identical text writes nothing; duplicate appends both persist; invalid patches fail', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'mcp-comment-noop-', seedFiles: seedFiles() });
        try {
            const file = path.join(workspace.tasksDir, 'MNP/1.yml');

            await withMcpClient(workspace, async (client) => {
                const added = await callTool(client, 1, 'task_comment_add', {
                    id: 'MNP-1',
                    text: 'stable body',
                });
                expect(added.error).toBeUndefined();

                // Editing a comment to the exact same text: 0 writes.
                const before = await fileState(file);
                const sameText = await callTool(client, 2, 'task_comment_update', {
                    id: 'MNP-1',
                    index: 0,
                    text: 'stable body',
                });
                expect(sameText.error).toBeUndefined();
                const sameDto = (extractToolPayload(sameText) as Record<string, any>).task ?? {};
                expect(Array.isArray(sameDto.comments)).toBe(true);
                const after = await fileState(file);
                expect(after.bytes).toBe(before.bytes);
                expect(after.mtimeMs).toBe(before.mtimeMs);

                // Duplicate explicit appends are intentional (no dedup).
                await callTool(client, 3, 'task_comment_add', { id: 'MNP-1', text: 'stable body' });
                const bytes = await fs.readFile(file, 'utf8');
                expect(bytes.match(/stable body/g)?.length).toBe(2);

                // Invalid patch: existing failure envelope, nothing written.
                const frozen = await fileState(file);
                const invalid = await callTool(client, 4, 'task_update', {
                    id: 'MNP-1',
                    patch: { status: 'Bogus' },
                });
                expect(invalid.error?.code).toBe(-32602);
                expect(JSON.stringify(invalid.error)).toContain('Status validation failed');
                expect(await fileState(file)).toEqual(frozen);
            });
        } finally {
            await workspace.dispose();
        }
    });
});
