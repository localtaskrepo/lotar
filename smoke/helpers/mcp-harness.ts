import { execa } from 'execa';

import { ensureBinaryExists } from './binary.js';
import type { McpClientLike, McpFrame } from './mcp.js';
import { FramedMcpClient, NdjsonMcpClient } from './mcp.js';
import type { SmokeWorkspace } from './workspace.js';

export const MCP_PROTOCOL_VERSION = '2025-06-18';
export const MCP_CLIENT_INFO = { name: 'lotar-smoke', version: '1.0.0' } as const;

type McpContentEntry = { type?: string; text?: string; json?: unknown };

let mcpRequestIdCounter = 0;

export function nextMcpRequestId(prefix: string): string {
    mcpRequestIdCounter += 1;
    return `${prefix}-${mcpRequestIdCounter}`;
}

export async function spawnMcpClient(
    workspace: SmokeWorkspace,
    options: { env?: NodeJS.ProcessEnv } = {},
): Promise<NdjsonMcpClient> {
    const binary = await ensureBinaryExists();
    const child = execa(binary, ['mcp'], {
        cwd: workspace.root,
        env: {
            ...workspace.env,
            LOTAR_MCP_AUTORELOAD: '0',
            ...options.env,
        },
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });

    return new NdjsonMcpClient(child);
}

export async function spawnFramedMcp(workspace: SmokeWorkspace): Promise<FramedMcpClient> {
    const binary = await ensureBinaryExists();
    const child = execa(binary, ['mcp'], {
        cwd: workspace.root,
        env: {
            ...workspace.env,
            LOTAR_MCP_AUTORELOAD: '0',
        },
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });

    return new FramedMcpClient(child);
}

export interface InitializeMcpOptions {
    readonly protocolVersion?: string;
    readonly capabilities?: Record<string, unknown>;
    readonly clientInfo?: Record<string, unknown>;
}

export async function initializeMcp(
    client: McpClientLike,
    options: InitializeMcpOptions = {},
): Promise<any> {
    const initializeId = nextMcpRequestId('init');
    await client.send({
        jsonrpc: '2.0',
        id: initializeId,
        method: 'initialize',
        params: {
            protocolVersion: options.protocolVersion ?? MCP_PROTOCOL_VERSION,
            capabilities: options.capabilities ?? {},
            clientInfo: options.clientInfo ?? MCP_CLIENT_INFO,
        },
    });
    const initializeResponse = await client.awaitResponse(initializeId);
    if (initializeResponse?.error) {
        throw new Error(`MCP initialize failed: ${JSON.stringify(initializeResponse.error)}`);
    }

    await client.send({
        jsonrpc: '2.0',
        method: 'notifications/initialized',
    });

    const pingId = nextMcpRequestId('ping');
    await client.send({
        jsonrpc: '2.0',
        id: pingId,
        method: 'ping',
    });
    const pong = await client.awaitResponse(pingId);
    if (pong?.error) {
        throw new Error(`MCP ready ping failed: ${JSON.stringify(pong.error)}`);
    }

    return initializeResponse;
}

export const initializeFramedMcp = initializeMcp;

export async function callTool(
    client: McpClientLike,
    id: number | string,
    name: string,
    args: Record<string, unknown>,
): Promise<any> {
    await client.send({
        jsonrpc: '2.0',
        id,
        method: 'tools/call',
        params: {
            name,
            arguments: args,
        },
    });

    return await client.awaitResponse(id);
}

export async function withMcpClient<T>(
    workspace: SmokeWorkspace,
    body: (client: NdjsonMcpClient) => Promise<T>,
    options: { env?: NodeJS.ProcessEnv } = {},
): Promise<T> {
    const client = await spawnMcpClient(workspace, options);
    try {
        await initializeMcp(client);
        return await body(client);
    } finally {
        await client.dispose();
    }
}

export async function withFramedMcpClient<T>(
    workspace: SmokeWorkspace,
    body: (client: FramedMcpClient) => Promise<T>,
): Promise<T> {
    const client = await spawnFramedMcp(workspace);
    try {
        await initializeMcp(client);
        return await body(client);
    } finally {
        await client.dispose();
    }
}

export function toolResultContent(message: any): McpContentEntry[] {
    return message?.result?.functionResponse?.response?.content ?? message?.result?.content ?? [];
}

export function toolErrorText(message: any): string {
    if (message?.error) {
        const data = message.error?.data;
        const dataText =
            typeof data === 'string'
                ? data
                : typeof data?.message === 'string'
                  ? data.message
                  : data === undefined || data === null
                    ? ''
                    : JSON.stringify(data);
        return [message.error?.message ?? '', dataText].filter(Boolean).join(': ');
    }
    if (message?.result?.isError) {
        const text = toolResultContent(message)
            .filter((entry) => typeof entry?.text === 'string')
            .map((entry) => entry.text)
            .join('\n');
        if (text) {
            return text;
        }
    }
    return '';
}

export function expectLifecycleRejection(message: any): void {
    if (!message?.error) {
        throw new Error(
            `Expected the server to reject a pre-ready request, received: ${JSON.stringify(message)}`,
        );
    }
    if (message.result !== undefined) {
        throw new Error(`Expected no result on a pre-ready rejection: ${JSON.stringify(message)}`);
    }
    if (message.error?.code !== -32002 || message.error?.message !== 'Server not initialized') {
        throw new Error(
            `Expected -32002 "Server not initialized", received code ${message.error?.code}: ${message.error?.message}`,
        );
    }
}

export function expectProtocolError(message: any, code: number, ...substrings: string[]): void {
    if (!message?.error) {
        throw new Error(`Expected a JSON-RPC protocol error, received: ${JSON.stringify(message)}`);
    }
    if (message.error.code !== code) {
        throw new Error(`Expected protocol error code ${code}, received ${message.error.code}: ${message.error.message}`);
    }
    const text = toolErrorText(message);
    for (const substring of substrings) {
        if (!text.includes(substring)) {
            throw new Error(`Expected protocol error text to include "${substring}", received: ${text}`);
        }
    }
}

export function expectToolFailure(message: any, ...substrings: string[]): void {
    if (message?.error) {
        throw new Error(`Expected a tool-execution failure (result.isError), received a protocol error: ${JSON.stringify(message.error)}`);
    }
    if (message?.result?.isError !== true) {
        throw new Error(`Expected result.isError=true, received: ${JSON.stringify(message?.result ?? message)}`);
    }
    const text = toolErrorText(message);
    for (const substring of substrings) {
        if (!text.includes(substring)) {
            throw new Error(`Expected tool failure text to include "${substring}", received: ${text}`);
        }
    }
}

export function extractToolPayload(source: McpFrame | McpClientLike | any): unknown {
    const message = isFrame(source) ? source.message : source;
    const content: McpContentEntry[] =
        message?.result?.functionResponse?.response?.content ?? message?.result?.content ?? [];

    const entry = content.find((candidate) => typeof candidate?.text === 'string' || candidate?.json);
    if (!entry) {
        throw new Error(`No content payload returned: ${JSON.stringify(message?.result ?? {}, null, 2)}`);
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

function isFrame(source: unknown): source is McpFrame {
    return (
        typeof source === 'object' &&
        source !== null &&
        'message' in source &&
        typeof (source as Record<string, unknown>).message === 'object'
    );
}
