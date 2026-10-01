#!/usr/bin/env node
// Regression tests for scripts/compress-web.mjs path handling and gzip
// embedding. Run with: node --test scripts/tests/compress-web.test.mjs
// Path cases exercise real URL values through fileURLToPath, including
// windows: true semantics so Windows drives, spaces, and Unicode are proven
// on every host. The end-to-end case runs the real script as a child process
// against a fixture tree in the OS temp dir; importing this file (or the
// script module) never touches the real target/ tree.

import test from 'node:test'
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { gunzipSync } from 'node:zlib'
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve, win32 } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { targetDirPaths } from '../compress-web.mjs'

const CI_SCRIPT_URL = new URL('file:///D:/a/lotar/lotar/scripts/compress-web.mjs')
const SPACED_SCRIPT_URL = new URL(
  'file:///D:/a%20r%C3%A9po/%E4%BB%BB%E5%8A%A1/scripts/compress-web.mjs',
)

test('windows drive-letter URLs resolve to real filesystem paths', () => {
  const { dist, embed } = targetDirPaths(CI_SCRIPT_URL, { windows: true })
  assert.equal(dist, 'D:\\a\\lotar\\lotar\\target\\web')
  assert.equal(embed, 'D:\\a\\lotar\\lotar\\target\\web-embed')
})

test('windows paths decode spaces and unicode instead of percent-encoding', () => {
  const { dist, embed } = targetDirPaths(SPACED_SCRIPT_URL, { windows: true })
  assert.equal(dist, 'D:\\a répo\\任务\\target\\web')
  assert.equal(embed, 'D:\\a répo\\任务\\target\\web-embed')
})

test('posix URLs resolve to absolute posix paths on every host', () => {
  const posixUrl = new URL('file:///srv/ci/lotar/scripts/compress-web.mjs')
  const { dist, embed } = targetDirPaths(posixUrl, { windows: false })
  assert.equal(dist, '/srv/ci/lotar/target/web')
  assert.equal(embed, '/srv/ci/lotar/target/web-embed')
})

test('default resolution matches the vite/include_dir contract for this checkout', () => {
  const scriptPath = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'compress-web.mjs')
  const { dist, embed } = targetDirPaths(pathToFileURL(scriptPath))
  const repoTarget = resolve(scriptPath, '..', '..', 'target')
  assert.equal(dist, join(repoTarget, 'web'))
  assert.equal(embed, join(repoTarget, 'web-embed'))
})

test('negative control: the old URL.pathname calculation corrupts windows paths', () => {
  const legacy = new URL('../target/web', CI_SCRIPT_URL).pathname
  assert.equal(legacy, '/D:/a/lotar/lotar/target/web')
  // A rooted but drive-less path re-anchors on the current drive: exactly
  // the D:\D:\... duplication observed in the failing Windows CI run.
  assert.equal(
    win32.resolve('D:\\a\\lotar\\lotar', legacy),
    'D:\\D:\\a\\lotar\\lotar\\target\\web',
  )
  // Percent-encoding was never decoded either.
  assert.equal(
    new URL('../target/web', SPACED_SCRIPT_URL).pathname,
    '/D:/a%20r%C3%A9po/%E4%BB%BB%E5%8A%A1/target/web',
  )
})

test('script compresses the raw tree and builds a gz-only embed tree end to end', () => {
  const root = mkdtempSync(join(tmpdir(), 'compress web é-'))
  try {
    const scriptsDir = join(root, 'scripts')
    mkdirSync(scriptsDir)
    cpSync(
      resolve(dirname(fileURLToPath(import.meta.url)), '..', 'compress-web.mjs'),
      join(scriptsDir, 'compress-web.mjs'),
    )
    const web = join(root, 'target', 'web')
    mkdirSync(join(web, 'assets'), { recursive: true })
    writeFileSync(join(web, 'index.html'), '<!doctype html><title>lötar</title>')
    writeFileSync(join(web, 'assets', 'app.js'), 'console.log("täsk")')
    writeFileSync(join(web, 'assets', 'app.css'), 'body{margin:0}')
    writeFileSync(join(web, 'assets', 'logo.svg'), '<svg xmlns="http://www.w3.org/2000/svg"/>')
    writeFileSync(join(web, 'assets', 'data.json'), '{"not":"compressed"}')

    const stdout = execFileSync(process.execPath, [join(scriptsDir, 'compress-web.mjs')], {
      encoding: 'utf8',
    })

    const compressible = ['index.html', 'assets/app.js', 'assets/app.css', 'assets/logo.svg']
    for (const rel of compressible) {
      const rawPath = join(web, ...rel.split('/'))
      assert.ok(existsSync(`${rawPath}.gz`), `missing ${rel}.gz`)
      assert.deepEqual(gunzipSync(readFileSync(`${rawPath}.gz`)), readFileSync(rawPath))
    }
    assert.ok(!existsSync(join(web, 'assets', 'data.json.gz')))

    const embedRoot = join(root, 'target', 'web-embed')
    const embedded = []
    const collect = (dir) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const full = join(dir, entry.name)
        if (entry.isDirectory()) collect(full)
        else embedded.push(full)
      }
    }
    collect(embedRoot)
    assert.equal(embedded.length, compressible.length)
    assert.ok(embedded.every((f) => f.endsWith('.gz')), 'embed tree must contain .gz files only')
    for (const rel of compressible) {
      assert.deepEqual(
        readFileSync(join(embedRoot, `${rel}.gz`)),
        readFileSync(`${join(web, ...rel.split('/'))}.gz`),
      )
    }
    assert.match(stdout, new RegExp(`embedding ${compressible.length} gz assets`))
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})
