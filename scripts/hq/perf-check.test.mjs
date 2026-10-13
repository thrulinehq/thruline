// C29: perf-check never reaches into the live store itself.
//   node --test scripts/hq/perf-check.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, realpathSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import { parse, liveStoreRefusal, liveRoots, quantile } from './perf-check.mjs'

const SCRIPT = fileURLToPath(new URL('./perf-check.mjs', import.meta.url))

/** A fake HOME with a live store in it, and a scratch folder beside it. */
function machine() {
  const dir = realpathSync(mkdtempSync(path.join(os.tmpdir(), 'perf-check-guard-')))
  mkdirSync(path.join(dir, 'home/.thruline'), { recursive: true })
  writeFileSync(path.join(dir, 'home/.thruline/ledger.db'), '')
  mkdirSync(path.join(dir, 'scratch'))
  writeFileSync(path.join(dir, 'scratch/ledger.db'), '')
  return { dir, env: { HOME: path.join(dir, 'home') }, live: path.join(dir, 'home/.thruline/ledger.db') }
}

const refusal = (m, argv, env = m.env) => liveStoreRefusal(parse(argv), env)

test('--db in the live store is allowed only with --start', () => {
  const m = machine()
  assert.equal(refusal(m, ['--start', '/bin/thruline', '--db', m.live, '--reads-only']), null)
  const why = refusal(m, ['--base', 'http://127.0.0.1:1', '--db', m.live])
  assert.match(why, /allowed only with --start/)
  assert.match(why, /cp -c/)
})

test('--register against the live store is refused, even with --start', () => {
  const m = machine()
  const why = refusal(m, ['--start', '/bin/thruline', '--db', m.live, '--register'])
  assert.match(why, /registering is for an empty scratch store/)
  // ... and a scratch store is fine.
  assert.equal(refusal(m, ['--start', '/bin/thruline', '--db', path.join(m.dir, 'scratch/ledger.db'), '--register']), null)
})

test('a path that reaches the live store another way is refused', () => {
  const m = machine()
  symlinkSync(path.join(m.dir, 'home/.thruline'), path.join(m.dir, 'scratch/alias'))
  const viaLink = path.join(m.dir, 'scratch/alias/ledger.db')
  assert.match(refusal(m, ['--start', 'x', '--db', viaLink, '--register']), /registering/)
  const dotted = path.join(m.dir, 'scratch/../home/.thruline/ledger.db')
  assert.match(refusal(m, ['--start', 'x', '--db', dotted, '--register']), /registering/)
  // Not there yet: judged by where it would be made.
  assert.match(refusal(m, ['--base', 'u', '--json', path.join(m.dir, 'home/.thruline/new/out.json')]), /--json/)
  // A leading ~/ is that HOME.
  assert.match(refusal(m, ['--start', 'x', '--db', '~/.thruline/ledger.db', '--register']), /registering/)
})

test('--json, --home and --thruline-home inside the live store are refused', () => {
  const m = machine()
  const inLive = path.join(m.dir, 'home/.thruline')
  assert.match(refusal(m, ['--base', 'u', '--json', path.join(inLive, 'out.json')]), /--json .* never writes or opens/)
  assert.match(refusal(m, ['--start', 'x', '--db', path.join(m.dir, 'scratch/ledger.db'), '--home', inLive]), /--home/)
  assert.match(refusal(m, ['--start', 'x', '--db', path.join(m.dir, 'scratch/ledger.db'), '--thruline-home', inLive]), /--thruline-home/)
  assert.equal(refusal(m, ['--base', 'u', '--json', path.join(m.dir, 'scratch/out.json')]), null)
  // A sibling whose name only starts the same way is not inside it.
  assert.equal(refusal(m, ['--base', 'u', '--json', path.join(m.dir, 'home/.thruline-bench/o.json')]), null)
})

test('THRULINE_HOME is live unless it is a scratch folder', () => {
  const m = machine()
  const th = path.join(m.dir, 'scratch')
  const env = { ...m.env, THRULINE_HOME: th }
  // With no scratch roots, the set THRULINE_HOME is the live store too.
  const roots = liveRoots(env, [])
  assert.ok(roots.includes(th), JSON.stringify(roots))
  assert.match(liveStoreRefusal(parse(['--start', 'x', '--db', path.join(th, 'ledger.db'), '--register']), env, []), /registering/)
  // Under the OS temp folder (the default scratch roots) it is a scratch store.
  assert.ok(!liveRoots(env).includes(th))
  assert.equal(liveStoreRefusal(parse(['--start', 'x', '--db', path.join(th, 'ledger.db'), '--register']), env), null)
})

test('the script itself refuses, exits 2, and starts nothing', () => {
  const m = machine()
  const r = spawnSync(process.execPath, [SCRIPT, '--start', '/nonexistent/thruline', '--db', m.live, '--register'], {
    env: { ...process.env, HOME: m.env.HOME, THRULINE_HOME: '' },
    encoding: 'utf8',
  })
  assert.equal(r.status, 2, r.stderr)
  assert.match(r.stderr, /perf-check: refused: --register/)
})

test('p50 and p95 are nearest-rank, and --switches defaults to 20', () => {
  const xs = Array.from({ length: 20 }, (_, i) => i + 1) // 1..20
  assert.equal(quantile(xs, 0.5), 10)
  assert.equal(quantile(xs, 0.95), 19)
  assert.equal(quantile([7], 0.95), 7)
  assert.equal(quantile([], 0.95), null)
  assert.equal(parse(['--base', 'u']).switches, 20)
  assert.equal(parse(['--base', 'u', '--switches', '5']).switches, 5)
  assert.throws(() => parse(['--base', 'u', '--switches', '0']), /--switches/)
})
