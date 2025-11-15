import { execa } from 'execa';
import fs from 'fs-extra';
import { once } from 'node:events';
import path from 'node:path';
import readline from 'node:readline';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { ensureBinaryExists } from '../helpers/binary.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP sprint smoke scenarios', () => {
    it('deletes a sprint via MCP tools and cleans memberships', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const first = await workspace.addTask('MCP Sprint Delete Task A');
            const second = await workspace.addTask('MCP Sprint Delete Task B');

            await workspace.runLotar(['sprint', 'create', '--label', 'MCP Delete Smoke Sprint']);
            await workspace.runLotar(['sprint', 'add', first.id, second.id, '--sprint', '1']);

            const binary = await ensureBinaryExists();
            const child = execa(binary, ['mcp'], {
                cwd: workspace.root,
                env: workspace.env,
                stdio: 'pipe',
            });

            if (!child.stdin || !child.stdout) {
                throw new Error('Failed to spawn MCP server with stdio pipes');
            }

            child.stdout.setEncoding('utf8');
            child.stderr?.setEncoding('utf8');
            child.stderr?.resume();

            const reader = readline.createInterface({
                input: child.stdout,
            });

            const nextResponse = async (): Promise<any> => {
                while (true) {
                    const [line] = (await once(reader, 'line')) as [string];
                    const trimmed = line.trim();
                    if (!trimmed) {
                        continue;
                    }
                    let parsed: any;
                    try {
                        parsed = JSON.parse(trimmed);
                    } catch (error) {
                        throw new Error(`Failed to parse MCP response: ${trimmed}\n${String(error)}`);
                    }
                    if (parsed?.id === undefined) {
                        continue;
                    }
                    return parsed;
                }
            };

            const send = (message: Record<string, unknown>) => {
                child.stdin!.write(`${JSON.stringify(message)}\n`);
            };

            send({
                jsonrpc: '2.0',
                id: 1,
                method: 'initialize',
                params: {
                    protocolVersion: '2025-06-18',
                },
            });
            const init = await nextResponse();
            expect(init.error).toBeUndefined();
            expect(init.result?.capabilities).toBeDefined();

            send({
                jsonrpc: '2.0',
                id: 2,
                method: 'tools/call',
                params: {
                    name: 'sprint/delete',
                    arguments: {
                        sprint: 1,
                        cleanup_missing: true,
                    },
                },
            });
            const deletion = await nextResponse();
            expect(deletion.error).toBeUndefined();

            const content = deletion.result?.content;
            expect(Array.isArray(content)).toBe(true);

            const summaryText = content?.[0]?.text ?? '';
            expect(summaryText).toContain('Deleted');

            const detailsText = content?.[1]?.text ?? '{}';
            let details: Record<string, any> = {};
            try {
                details = JSON.parse(detailsText);
            } catch (error) {
                throw new Error(`Failed to parse MCP delete payload: ${detailsText}\n${String(error)}`);
            }

            expect(details.deleted).toBe(true);
            expect(details.sprint_id).toBe(1);
            expect(details.removed_references).toBeGreaterThanOrEqual(0);
            expect(details.updated_tasks).toBeGreaterThanOrEqual(0);

            reader.close();
            child.stdin.end();
            const result = await child;
            expect(result.exitCode).toBe(0);

            const sprintPath = path.join(workspace.tasksDir, '@sprints', '1.yml');
            expect(await fs.pathExists(sprintPath)).toBe(false);

            const firstYaml = parse(await workspace.readTaskYaml(first.id)) as Record<string, any>;
            const secondYaml = parse(await workspace.readTaskYaml(second.id)) as Record<string, any>;

            const firstMembership = Array.isArray(firstYaml.sprints) ? firstYaml.sprints : [];
            const secondMembership = Array.isArray(secondYaml.sprints) ? secondYaml.sprints : [];

            expect(firstMembership).not.toContain(1);
            expect(secondMembership).not.toContain(1);
        } finally {
            await workspace.dispose();
        }
    });
});
