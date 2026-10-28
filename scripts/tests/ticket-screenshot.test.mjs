import assert from 'node:assert/strict';
import test from 'node:test';
import { fileName, paddedClip, parseArgs } from '../ticket-screenshot.mjs';

const base = ['--url', 'http://localhost:8080/boards', '--selector', '.app-page-header'];

test('defaults to a light, 1280px, padded snippet attached to the ticket', () => {
    const opts = parseArgs(['--ticket', 'DEV-1', ...base]);
    assert.equal(opts.ticket, 'DEV-1');
    assert.equal(opts.width, 1280);
    assert.equal(opts.theme, 'light');
    assert.equal(opts.scale, 1);
    assert.equal(opts.pad, 8);
    assert.equal(opts.attach, true);
    assert.equal(opts.tasksDir, '.tasks');
    assert.equal(opts.name, 'app-page-header');
});

test('requires a selector, so tickets get snippets rather than full pages', () => {
    assert.throws(() => parseArgs(['--ticket', 'DEV-1', '--url', 'http://x']), /--selector is required/);
});

test('requires a ticket unless the image is only written to --out', () => {
    assert.throws(() => parseArgs(base), /--ticket is required/);
    assert.throws(() => parseArgs([...base, '--no-attach']), /--no-attach needs --out/);
    assert.equal(parseArgs([...base, '--no-attach', '--out', 'shots']).attach, false);
});

test('rejects unknown options, missing values and out-of-range numbers', () => {
    assert.throws(() => parseArgs([...base, '--ticket', 'DEV-1', '--colour', 'red']), /Unknown option --colour/);
    assert.throws(() => parseArgs([...base, '--ticket']), /--ticket needs a value/);
    assert.throws(() => parseArgs([...base, '--ticket', 'DEV-1', '--width', '20']), /--width must be a number/);
    assert.throws(() => parseArgs([...base, '--ticket', 'DEV-1', '--theme', 'sepia']), /--theme must be light or dark/);
});

test('scale-2 captures are named -2x so pages can halve them', () => {
    assert.equal(parseArgs(['--ticket', 'DEV-1', ...base, '--scale', '2', '--name', 'header']).name, 'header-2x');
    assert.equal(parseArgs(['--ticket', 'DEV-1', ...base, '--scale', '1.5', '--name', 'header']).name, 'header');
});

test('file names are safe stems', () => {
    assert.equal(fileName('.task-panel__group > legend'), 'task-panel-group-legend');
    assert.equal(fileName('Board header (phone)'), 'board-header-phone');
    assert.equal(fileName('###'), 'screenshot');
    assert.equal(fileName('x'.repeat(100)).length, 60);
});

test('padding grows the crop but stays inside the page', () => {
    const page = { width: 1000, height: 800 };
    assert.deepEqual(paddedClip({ x: 100, y: 50, width: 200, height: 40 }, 8, page), {
        x: 92,
        y: 42,
        width: 216,
        height: 56,
    });
    assert.deepEqual(paddedClip({ x: 2, y: 790, width: 1000, height: 30 }, 8, page), {
        x: 0,
        y: 782,
        width: 1000,
        height: 18,
    });
});
