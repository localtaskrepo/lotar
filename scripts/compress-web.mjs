#!/usr/bin/env node
// Pre-compress text assets from the Vite build so the server can serve
// Content-Encoding: gzip from the embedded bundle without runtime cost.
//
// Produces two trees:
//   target/web        - raw assets + .gz siblings (used by the dev-mode
//                       filesystem fallback and smoke:ui external serving)
//   target/web-embed  - .gz variants ONLY; this is what include_dir embeds
//                       into the binary, so the shipped binary carries each
//                       asset once (compressed) instead of twice.
import { gzipSync } from 'node:zlib'
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const dist = new URL('../target/web', import.meta.url).pathname
const embed = new URL('../target/web-embed', import.meta.url).pathname
const compressible = new Set(['.js', '.css', '.svg', '.html'])

let total = 0
let compressed = 0
const gzFiles = []

function walk(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      walk(full)
      continue
    }
    if (!compressible.has(entry.slice(entry.lastIndexOf('.')))) continue
    if (entry.endsWith('.gz')) continue
    const raw = readFileSync(full)
    total += raw.length
    const gz = gzipSync(raw, { level: 9 })
    writeFileSync(`${full}.gz`, gz)
    compressed += gz.length
    gzFiles.push(full.slice(dist.length + 1))
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

console.log(
  `compressed ${(total / 1024).toFixed(0)}KiB -> ${(compressed / 1024).toFixed(0)}KiB ` +
  `(${((1 - compressed / total) * 100).toFixed(0)}% saved), embedding ${gzFiles.length} gz assets`,
)
