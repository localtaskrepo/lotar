import { execa } from 'execa';
import { once } from 'node:events';
import readline from 'node:readline';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { ensureBinaryExists } from '../helpers/binary.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const STRICT_MEMBERS_CONFIG = `default:
  project: CLI
  reporter: allowed@example.com
  assignee: allowed@example.com
  members:
    - allowed@example.com
    - reviewer@example.com
  strict_members: true
auto:
    populate_members: false
issue:
  states: [Todo, InProgress, Done]
  types: [Feature, Bug, Chore]
  priorities: [Low, Medium, High]
`;

describe.concurrent('MCP strict member smoke scenarios', () => {
    it('validates members when creating tasks', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', STRICT_MEMBERS_CONFIG);

            const binary = await ensureBinaryExists();
            const child = execa(binary, ['mcp'], {
                cwd: workspace.root,
                env: workspace.env,
                stdio: 'pipe',
            });

            if (!child.stdin || !child.stdout) {
                throw new Error('Failed to start MCP server with stdio pipes');
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
                    try {
                        return JSON.parse(trimmed);
                    } catch (error) {
                        throw new Error(`Failed to parse MCP response: ${trimmed}\n${String(error)}`);
                    }
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

            send({
                jsonrpc: '2.0',
                id: 2,
                method: 'task/create',
                params: {
                    title: 'MCP strict members invalid assignee',
                    project: 'CLI',
                    reporter: 'allowed@example.com',
                    assignee: 'intruder@example.com',
                },
            });
            const invalid = await nextResponse();
            expect(invalid.error?.message).toBe('Task create failed');
            const errorMessage = invalid.error?.data?.message ?? '';
            expect(errorMessage).toContain("Assignee 'intruder@example.com' is not in configured members");

            send({
                jsonrpc: '2.0',
                id: 3,
                method: 'task/create',
                params: {
                    title: 'MCP strict members allowed assignee',
                    project: 'CLI',
                    reporter: 'allowed@example.com',
                    assignee: 'allowed@example.com',
                },
            });
            const success = await nextResponse();
            expect(success.error).toBeUndefined();

            const content = success.result?.content?.[0]?.text ?? '{}';
            let dto: Record<string, any> = {};
            try {
                dto = JSON.parse(content);
            } catch (error) {
                throw new Error(`Failed to parse task payload: ${content}\n${String(error)}`);
            }

            expect(dto.id).toMatch(/^CLI-\d+$/);
            expect(dto.assignee).toBe('allowed@example.com');
            expect(dto.reporter).toBe('allowed@example.com');

            reader.close();
            child.stdin.end();
            const result = await child;
            expect(result.exitCode).toBe(0);

            const files = await workspace.listTaskFiles();
            expect(files).toHaveLength(1);

            const yaml = await workspace.readTaskYaml(dto.id as string);
            const task = parse(yaml) as Record<string, any>;
            expect(task.assignee).toBe('allowed@example.com');
            expect(task.reporter).toBe('allowed@example.com');
        } finally {
            await workspace.dispose();
        }
    });
});
