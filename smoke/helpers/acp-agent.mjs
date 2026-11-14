#!/usr/bin/env node
// A fake ACP agent on stdio for smoke tests (DEV-503): lotar runs it as the
// harnessmux claude agent through an agents.toml under HARNESSMUX_HOME. It
// answers initialize and session/new, resume and load, and replies to each
// prompt. With FAKE_ASK set it first asks for permission to edit a file and
// replies with the option it got.

import readline from 'node:readline';

const send = (message) => process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);

function reply(id, text) {
    send({
        method: 'session/update',
        params: {
            sessionId: 'smoke-1',
            update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text } },
        },
    });
    send({ id, result: { stopReason: 'end_turn' } });
}

/** The prompt that waits for the permission answer. */
let waiting = null;

readline.createInterface({ input: process.stdin }).on('line', (line) => {
    let message;
    try {
        message = JSON.parse(line);
    } catch {
        return;
    }
    if (message.method === undefined && message.id === 'ask-1' && waiting !== null) {
        reply(waiting, `Permission: ${message.result?.outcome?.optionId ?? 'cancelled'}`);
        waiting = null;
        return;
    }
    switch (message.method) {
        case 'initialize':
            send({
                id: message.id,
                result: {
                    protocolVersion: 1,
                    agentCapabilities: { loadSession: true, sessionCapabilities: { resume: {} } },
                    agentInfo: { name: 'smoke-acp', version: '1.0.0' },
                    authMethods: [],
                },
            });
            break;
        case 'session/new':
            send({ id: message.id, result: { sessionId: 'smoke-1' } });
            break;
        case 'session/resume':
        case 'session/load':
            send({ id: message.id, result: {} });
            break;
        case 'session/prompt':
            if (!process.env.FAKE_ASK) {
                reply(message.id, 'Done.');
                break;
            }
            waiting = message.id;
            send({
                id: 'ask-1',
                method: 'session/request_permission',
                params: {
                    sessionId: 'smoke-1',
                    toolCall: { toolCallId: 'edit-1', title: 'Edit notes.md', kind: 'edit' },
                    options: [
                        { optionId: 'allow', name: 'Allow', kind: 'allow_once' },
                        { optionId: 'reject', name: 'Reject', kind: 'reject_once' },
                    ],
                },
            });
            break;
        default:
            if (message.method && message.id !== undefined) {
                send({ id: message.id, result: {} });
            }
    }
});
