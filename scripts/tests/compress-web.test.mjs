#!/usr/bin/env node
// Regression tests for scripts/compress-web.mjs path handling and asset
// packing. Run with: node --test scripts/tests/compress-web.test.mjs
// Path cases exercise real URL values through fileURLToPath, including
// windows: true semantics so Windows drives, spaces, and Unicode are proven
// on every host. Packing cases run the real script as a child process and
// the exported packAssets entry point against fixture trees in the OS temp
// dir; importing this file (or the script module) never touches the real
// target/ tree.

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

import { packAssets, targetDirPaths } from '../compress-web.mjs'

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

// Binary fixtures carry NUL/high bytes so any text mangling or recompression
// would break byte equality.
const ICO_BYTES = Buffer.from([0x00, 0x00, 0x01, 0x00, 0x03, 0x00, 0xfe, 0xff, 0x00, 0x7f])
const PNG_BYTES = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xc3, 0xa9])
const WEBP_BYTES = Buffer.from([0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50])

const TEXT_ASSETS = ['index.html', 'assets/app.js', 'assets/app.css', 'assets/logo.svg']
const RAW_ASSETS = [
  'assets/data.json',
  'assets/branding/favicon.ico',
  'assets/branding/dark logo ünïcode.png',
  'assets/photo.webp',
]
// The embed tree is exactly one entry per non-gz dist file: `<rel>.gz` for
// compressible text, byte-exact `<rel>` for everything else.
const EXPECTED_EMBED = [...TEXT_ASSETS.map((rel) => `${rel}.gz`), ...RAW_ASSETS].sort()

function writeFixture(web) {
  mkdirSync(join(web, 'assets', 'branding'), { recursive: true })
  writeFileSync(join(web, 'index.html'), '<!doctype html><title>lötar</title>')
  writeFileSync(join(web, 'assets', 'app.js'), 'console.log("täsk")')
  writeFileSync(join(web, 'assets', 'app.css'), 'body{margin:0}')
  writeFileSync(join(web, 'assets', 'logo.svg'), '<svg xmlns="http://www.w3.org/2000/svg"/>')
  writeFileSync(join(web, 'assets', 'data.json'), '{"not":"compressed"}')
  writeFileSync(join(web, 'assets', 'branding', 'favicon.ico'), ICO_BYTES)
  writeFileSync(join(web, 'assets', 'branding', 'dark logo ünïcode.png'), PNG_BYTES)
  writeFileSync(join(web, 'assets', 'photo.webp'), WEBP_BYTES)
}

function listTree(root) {
  const files = []
  const collect = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name)
      if (entry.isDirectory()) collect(full)
      else files.push(full.slice(root.length + 1))
    }
  }
  collect(root)
  return files.sort()
}

test('script packs a gz-for-text, raw-for-everything-else embed tree end to end', () => {
  const root = mkdtempSync(join(tmpdir(), 'compress web é-'))
  try {
    const scriptsDir = join(root, 'scripts')
    mkdirSync(scriptsDir)
    cpSync(
      resolve(dirname(fileURLToPath(import.meta.url)), '..', 'compress-web.mjs'),
      join(scriptsDir, 'compress-web.mjs'),
    )
    const web = join(root, 'target', 'web')
    writeFixture(web)

    const stdout = execFileSync(process.execPath, [join(scriptsDir, 'compress-web.mjs')], {
      encoding: 'utf8',
    })

    for (const rel of TEXT_ASSETS) {
      const rawPath = join(web, ...rel.split('/'))
      assert.ok(existsSync(`${rawPath}.gz`), `missing ${rel}.gz`)
      assert.deepEqual(gunzipSync(readFileSync(`${rawPath}.gz`)), readFileSync(rawPath))
    }
    for (const rel of RAW_ASSETS) {
      assert.ok(!existsSync(`${join(web, ...rel.split('/'))}.gz`), `unexpected ${rel}.gz`)
    }

    const embedRoot = join(root, 'target', 'web-embed')
    assert.deepEqual(listTree(embedRoot), EXPECTED_EMBED)

    for (const rel of TEXT_ASSETS) {
      assert.deepEqual(
        readFileSync(join(embedRoot, `${rel}.gz`)),
        readFileSync(`${join(web, ...rel.split('/'))}.gz`),
      )
    }
    for (const rel of RAW_ASSETS) {
      assert.deepEqual(
        readFileSync(join(embedRoot, ...rel.split('/'))),
        readFileSync(join(web, ...rel.split('/'))),
      )
    }
    assert.match(stdout, new RegExp(`embedding ${TEXT_ASSETS.length} gz \\+ ${RAW_ASSETS.length} raw assets`))
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('packAssets rebuilds the embed tree exactly: stale entries removed, no dist asset lost', () => {
  const root = mkdtempSync(join(tmpdir(), 'pack-assets stale-'))
  try {
    const web = join(root, 'target', 'web')
    const embed = join(root, 'target', 'web-embed')
    writeFixture(web)
    mkdirSync(join(embed, 'assets', 'old'), { recursive: true })
    writeFileSync(join(embed, 'stale.html.gz'), 'stale')
    writeFileSync(join(embed, 'assets', 'old', 'gone.bin'), Buffer.from([0xff]))

    const result = packAssets(web, embed)

    assert.deepEqual(listTree(embed), EXPECTED_EMBED)
    assert.equal(result.gzCount, TEXT_ASSETS.length)
    assert.equal(result.rawCount, RAW_ASSETS.length)
    for (const rel of RAW_ASSETS) {
      assert.deepEqual(
        readFileSync(join(embed, ...rel.split('/'))),
        readFileSync(join(web, ...rel.split('/'))),
      )
    }
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test('packAssets skips existing gz mirrors and embeds nested, spaced, unicode raw paths byte-exact', () => {
  const root = mkdtempSync(join(tmpdir(), 'pack-assets gz-skip-'))
  try {
    const web = join(root, 'web')
    const embed = join(root, 'embed')
    mkdirSync(join(web, 'assets', 'branding', '深 图标'), { recursive: true })
    writeFileSync(join(web, 'assets', 'app.js'), 'console.log(1)')
    // Decoy mirror from a previous run plus an orphan whose raw source is
    // gone: neither may be embedded, and the decoy is regenerated from the
    // real source instead of copied through.
    writeFileSync(join(web, 'assets', 'app.js.gz'), 'not real gzip')
    writeFileSync(join(web, 'assets', 'gone.js.gz'), 'orphan mirror')
    const ico = Buffer.from([0x00, 0x00, 0x01, 0x00, 0x00, 0x80])
    writeFileSync(join(web, 'assets', 'branding', '深 图标', 'fav icon.ico'), ico)

    packAssets(web, embed)

    assert.deepEqual(listTree(embed), ['assets/app.js.gz', 'assets/branding/深 图标/fav icon.ico'])
    assert.deepEqual(
      gunzipSync(readFileSync(join(embed, 'assets', 'app.js.gz'))),
      Buffer.from('console.log(1)'),
    )
    assert.deepEqual(
      readFileSync(join(embed, 'assets', 'branding', '深 图标', 'fav icon.ico')),
      ico,
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})
