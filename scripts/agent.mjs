import { execa } from 'execa'
import process from 'node:process'

function usage(exitCode = 0) {
  const msg = `Usage: node scripts/agent.mjs <command>

Commands:
  lint            Run lint with reduced noise
  test            Run unit/integration tests (Rust + UI)
  test:rust       Run cargo-nextest (CI profile) with reduced noise
  test:ui         Run Vitest UI suite with dot reporter
  test:smoke      Run smoke suite (builds first)
  test:smoke:quick Run smoke suite without rebuilding

Notes:
  - Sets NO_COLOR/CARGO_TERM_COLOR to reduce ANSI noise.
  - Keeps behavior aligned with existing npm scripts.`
  // eslint-disable-next-line no-console
  console.error(msg)
  process.exit(exitCode)
}

const command = process.argv[2]
if (!command || command === '-h' || command === '--help') usage(0)

const env = {
  ...process.env,
  NO_COLOR: '1',
  FORCE_COLOR: '0',
  CARGO_TERM_COLOR: 'never',
}

async function run(cmd, args, opts = {}) {
  return execa(cmd, args, {
    stdio: 'inherit',
    env,
    ...opts,
  })
}

async function main() {
  switch (command) {
    case 'lint':
      await run('npm', ['run', 'lint:frontend'])
      await run('cargo', ['clippy', '--all-targets', '--all-features', '--color', 'never', '--', '-D', 'warnings'])
      return

    case 'test':
      await run(process.execPath, ['scripts/agent.mjs', 'test:rust'])
      await run(process.execPath, ['scripts/agent.mjs', 'test:ui'])
      return

    case 'test:rust':
      await run('cargo', [
        'nextest',
        'run',
        '--cargo-profile',
        'ci',
        '--color',
        'never',
        '--failure-output',
        'immediate-final',
        '--retries',
        '0',
      ])
      return

    case 'test:ui':
      await run('vitest', ['run', '--config', 'view/vitest.config.ts', '--reporter', 'dot', '--no-color'])
      return

    case 'test:smoke':
      await run('npm', ['run', 'build'])
      await run(process.execPath, ['scripts/agent.mjs', 'test:smoke:quick'])
      return

    case 'test:smoke:quick':
      await run('vitest', ['run', '--config', 'smoke/vitest.config.ts', '--reporter', 'dot', '--no-color'])
      return

    default:
      usage(1)
  }
}

main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error(err?.stack || String(err))
  process.exit(typeof err?.exitCode === 'number' ? err.exitCode : 1)
})
