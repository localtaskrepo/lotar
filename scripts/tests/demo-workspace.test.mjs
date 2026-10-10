import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from 'node:test';
import { demoEnvironment } from '../demo/workspace.mjs';

test('demo environment removes repository redirects and private task defaults', () => {
    const inherited = {
        PATH: 'fixture-path',
        TZ: 'UTC',
        GIT_DIR: 'private-repository',
        GIT_WORK_TREE: 'private-worktree',
        GIT_INDEX_FILE: 'private-index',
        GIT_CONFIG_COUNT: '1',
        GIT_CONFIG_KEY_0: 'core.worktree',
        GIT_CONFIG_VALUE_0: 'private-worktree',
        GIT_CONFIG_GLOBAL: 'private-config',
        GIT_CONFIG_NOSYSTEM: '0',
        LOTAR_TASKS_DIR: 'private-tasks',
        LOTAR_PROJECT: 'PRIVATE',
        LOTAR_DEFAULT_ASSIGNEE: 'private-person',
        LOTAR_DEFAULT_REPORTER: 'private-person',
    };
    const dir = path.resolve('atlas-fixture');
    const env = demoEnvironment(dir, inherited);
    assert.equal(env.PATH, inherited.PATH);
    assert.equal(env.TZ, inherited.TZ);
    assert.equal(env.LOTAR_TASKS_DIR, path.join(dir, '.tasks'));
    assert.equal(env.LOTAR_IGNORE_HOME_CONFIG, '1');
    assert.equal(env.GIT_CONFIG_GLOBAL, path.join(dir, '.demo', 'gitconfig'));
    assert.equal(env.GIT_CONFIG_NOSYSTEM, '1');
    assert.deepEqual(Object.keys(env).filter(key => key.startsWith('GIT_') || key.startsWith('LOTAR_')).sort(), [
        'GIT_CONFIG_GLOBAL', 'GIT_CONFIG_NOSYSTEM', 'LOTAR_IGNORE_HOME_CONFIG', 'LOTAR_TASKS_DIR',
    ]);
});

test('demo environment does not mutate inherited process state', () => {
    const inherited = Object.freeze({ PATH: 'fixture-path', GIT_DIR: 'private-repository', LOTAR_PROJECT: 'PRIVATE' });
    const env = demoEnvironment(path.resolve('atlas-fixture'), inherited);
    assert.notEqual(env, inherited);
    assert.deepEqual(inherited, { PATH: 'fixture-path', GIT_DIR: 'private-repository', LOTAR_PROJECT: 'PRIVATE' });
});
