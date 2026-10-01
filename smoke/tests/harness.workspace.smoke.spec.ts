import fs from 'fs-extra';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe('smoke workspace helper contracts', () => {
    it('addTask returns the ID-derived task file with the persisted title', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const created = await workspace.addTask('Harness round trip task', {
                args: ['--project', 'WEB'],
            });

            expect(created.id).toMatch(/^WEB-\d+$/);
            expect(created.filePath).toBe(workspace.getTaskFilePath(created.id));
            expect(await fs.pathExists(created.filePath)).toBe(true);

            const taskYaml = parse(await workspace.readTaskYaml(created.id)) as Record<string, any>;
            expect(taskYaml.title).toBe('Harness round trip task');
        } finally {
            await workspace.dispose();
        }
    });

    it('addTask fails bounded when the reported task ID never produces a file', async () => {
        const workspace = await SmokeWorkspace.create();
        const fakeBinary = `${workspace.root}/fake-lotar-no-file`;
        await fs.writeFile(
            fakeBinary,
            '#!/bin/sh\nprintf \'\\u2705 Created task: WEB-99\\n\'\nexit 0\n',
        );
        await fs.chmod(fakeBinary, 0o755);

        const previousBinary = process.env.LOTAR_BINARY_PATH;
        process.env.LOTAR_BINARY_PATH = fakeBinary;
        const startedAt = Date.now();

        try {
            await expect(workspace.addTask('Never created task')).rejects.toThrow(
                /WEB\/99\.yml .* was not created within 5s/s,
            );
            expect(Date.now() - startedAt).toBeLessThan(30_000);
        } finally {
            if (previousBinary === undefined) {
                delete process.env.LOTAR_BINARY_PATH;
            } else {
                process.env.LOTAR_BINARY_PATH = previousBinary;
            }
            await workspace.dispose();
        }
    });

    it('addTask rejects an unexpected new task file instead of adopting it', async () => {
        const workspace = await SmokeWorkspace.create();
        const fakeBinary = `${workspace.root}/fake-lotar-wrong-id`;
        await fs.writeFile(
            fakeBinary,
            [
                '#!/bin/sh',
                'printf \'\\u2705 Created task: WEB-99\\n\'',
                'mkdir -p "$PWD/.tasks/BUG"',
                'printf \'title: decoy\\n\' > "$PWD/.tasks/BUG/1.yml"',
                'exit 0',
                '',
            ].join('\n'),
        );
        await fs.chmod(fakeBinary, 0o755);

        const previousBinary = process.env.LOTAR_BINARY_PATH;
        process.env.LOTAR_BINARY_PATH = fakeBinary;

        try {
            await expect(workspace.addTask('Wrong ID task')).rejects.toThrow(
                /only unexpected task file\(s\) appeared: .*BUG.*1\.yml/s,
            );
        } finally {
            if (previousBinary === undefined) {
                delete process.env.LOTAR_BINARY_PATH;
            } else {
                process.env.LOTAR_BINARY_PATH = previousBinary;
            }
            await workspace.dispose();
        }
    });
});
