#!/usr/bin/env node
// Pre-compress text assets from the Vite build so the server can serve
// Content-Encoding: gzip from the embedded bundle without runtime cost.
//
// Produces two trees:
//   target/web        - raw assets + .gz siblings (used by the dev-mode
//                       filesystem fallback and smoke:ui external serving)
//   target/web-embed  - the tree include_dir embeds into the binary:
//                       .gz variants for compressible text assets plus
//                       byte-exact raw copies of every other Vite-emitted
//                       file (ICO/PNG/JPG/WEBP/...), so assets that must not
//                       be recompressed still ship inside the binary while
//                       each text asset is carried only once (compressed).
import { gzipSync } from 'node:zlib'
import { cpSync, mkdirSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const compressible = new Set(['.js', '.css', '.svg', '.html'])

// Resolve the asset directories relative to a script URL. fileURLToPath is
// load-bearing on Windows: URL.pathname keeps the leading slash and the
// percent-encoding, so joining it onto a drive produces D:\D:\... paths and
// never decodes spaces or Unicode in the checkout path.
export function targetDirPaths(baseUrl, options) {
  return {
    dist: fileURLToPath(new URL('../target/web', baseUrl), options),
    embed: fileURLToPath(new URL('../target/web-embed', baseUrl), options),
  }
}

// Importing the module (unit tests) must not touch the real target/ tree.
function invokedAsScript() {
  if (!process.argv[1]) return false
  try {
    return realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))
  } catch {
    return false
  }
}

// Pack the embed tree for one finished Vite build: compressible text assets
// are gzipped into `<dist>/<rel>.gz` mirrors and embedded as `<rel>.gz`;
// every other dist file is copied into the embed tree byte-exact with no
// recompression. `.gz` files already in dist are mirrors from a previous
// run, never sources, so they are skipped to avoid duplicating assets. The
// embed tree is rebuilt from scratch, so stale entries cannot survive. Pure
// with respect to this checkout: both roots are caller-supplied, so tests
// drive it entirely with temp dirs.
export function packAssets(dist, embed) {
  let total = 0
  let compressed = 0
  const gzFiles = []
  const rawFiles = []

  function walk(dir) {
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry)
      if (statSync(full).isDirectory()) {
        walk(full)
        continue
      }
      if (entry.endsWith('.gz')) continue
      if (compressible.has(entry.slice(entry.lastIndexOf('.')))) {
        const raw = readFileSync(full)
        total += raw.length
        const gz = gzipSync(raw, { level: 9 })
        writeFileSync(`${full}.gz`, gz)
        compressed += gz.length
        gzFiles.push(full.slice(dist.length + 1))
      } else {
        rawFiles.push(full.slice(dist.length + 1))
      }
    }
  }

  walk(dist)

  rmSync(embed, { recursive: true, force: true })
  mkdirSync(embed, { recursive: true })
  for (const rel of gzFiles) {
    const dest = join(embed, `${rel}.gz`)
    mkdirSync(join(dest, '..'), { recursive: true })
    cpSync(join(dist, `${rel}.gz`), dest)
  }
  for (const rel of rawFiles) {
    const dest = join(embed, rel)
    mkdirSync(join(dest, '..'), { recursive: true })
    cpSync(join(dist, rel), dest)
  }

  return {
    gzCount: gzFiles.length,
    rawCount: rawFiles.length,
    totalBytes: total,
    compressedBytes: compressed,
  }
}

function run() {
  const { dist, embed } = targetDirPaths(import.meta.url)
  const { gzCount, rawCount, totalBytes, compressedBytes } = packAssets(dist, embed)
  const saved = totalBytes > 0 ? ((1 - compressedBytes / totalBytes) * 100).toFixed(0) : '0'
  console.log(
    `compressed ${(totalBytes / 1024).toFixed(0)}KiB -> ${(compressedBytes / 1024).toFixed(0)}KiB ` +
      `(${saved}% saved), embedding ${gzCount} gz + ${rawCount} raw assets`,
  )
}

if (invokedAsScript()) run()
