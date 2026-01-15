import { execa } from 'execa';

import { ensureBinaryExists } from './binary.js';
import type { McpFrame } from './mcp.js';
import { FramedMcpClient } from './mcp.js';
import type { SmokeWorkspace } from './workspace.js';

export const MCP_PROTOCOL_VERSION = '2025-06-18';

type McpContentEntry = { type?: string; text?: string; json?: unknown };

export async function spawnFramedMcp(workspace: SmokeWorkspace): Promise<FramedMcpClient> {
    const binary = await ensureBinaryExists();
    const child = execa(binary, ['mcp'], {
        cwd: workspace.root,
        env: workspace.env,
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });

    child.stderr?.setEncoding('utf8');
    child.stderr?.resume();

    return new FramedMcpClient(child);
}

export async function initializeFramedMcp(
    client: FramedMcpClient,
    id = 1,
    protocolVersion = MCP_PROTOCOL_VERSION,
): Promise<McpFrame> {
    await client.send({
        jsonrpc: '2.0',
        id,
        method: 'initialize',
        params: {
            protocolVersion,
        },
    });

    return await client.readUntil((frame) => frame.message?.id === id);
}

export async function callTool(client: FramedMcpClient, id: number, name: string, args: Record<string, unknown>): Promise<McpFrame> {
    await client.send({
        jsonrpc: '2.0',
        id,
        method: 'tools/call',
        params: {
            name,
            arguments: args,
        },
    });

    return await client.readUntil((frame) => frame.message?.id === id);
}

export async function withFramedMcpClient<T>(
    workspace: SmokeWorkspace,
    body: (client: FramedMcpClient) => Promise<T>,
): Promise<T> {
    const client = await spawnFramedMcp(workspace);
    try {
        return await body(client);
    } finally {
        await client.dispose();
    }
}

export function extractToolPayload(frame: McpFrame): unknown {
    const content: McpContentEntry[] =
        frame?.message?.result?.functionResponse?.response?.content ?? frame?.message?.result?.content ?? [];

    const entry = content.find((candidate) => typeof candidate?.text === 'string' || candidate?.json);
    if (!entry) {
        throw new Error(`No content payload returned: ${JSON.stringify(frame?.message?.result ?? {}, null, 2)}`);
    }

    if (typeof entry.text === 'string') {
        try {
            return JSON.parse(entry.text);
        } catch (error) {
            throw new Error(`Failed to parse MCP payload JSON: ${entry.text}\n${String(error)}`);
        }
    }

    return entry.json;
}
