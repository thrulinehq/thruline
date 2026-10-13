#!/usr/bin/env node
/* perf-check.mjs — HQ Next's speed budgets (C27), measured against a running
 * `thruline ui`, printing PASS or FAIL per budget.
 *
 *   node scripts/hq/perf-check.mjs --base http://127.0.0.1:7777 [--project <id>]
 *   node scripts/hq/perf-check.mjs --start <thruline> --db <ledger.db> [--home <dir>]
 *        [--thruline-home <dir>] [--port 7790] [--project <id>] [--root <repo root>]
 *        [--register] [--threads 5] [--switches 20] [--json <out.json>]
 *   --reads-only   skip (b) and anything that writes: GET requests only, so
 *                  it is safe against a store that must not be written.
 *
 * Budgets:
 *   (a) inbox   — a loaded project's Inbox data ready: the first
 *                 GET /api/hq/projects/<id> after the server starts   < 2 s
 *   (b) reload  — a no-change reload: POST /api/hq/projects/<id>/load,
 *                 then GET /api/hq/load until it is done              < 5 s
 *   (c) switch  — switching screens on a Change: the reads an HQ Change
 *                 screen makes when it opens (GET /api/hq/projects, the
 *                 project, the thread, /api/changes, /api/lanes),
 *                 together, warm: p95 over --switches switches       < 300 ms
 *                 (C29). The sample is the --threads newest threads and
 *                 the --threads threads with the most edits; each is
 *                 opened once first (not a switch), then --switches
 *                 switches cycle through the sample in order, so each
 *                 switch lands on a different thread than the one before.
 *                 p50, p95 and max are reported; p50 and p95 are
 *                 nearest-rank (the ceil(q*n)-th smallest). /api/storage
 *                 is not among them: the screens state one shared
 *                 reading, at most a minute old, and never wait for a
 *                 new one (ui/src/hq/data/api.js storageNow); its time
 *                 is reported beside the budgets as storage_ms.
 *
 * With --base the server is already running, so (a) is only the first read
 * this script makes, not a read "after start" — it says so. With --start the
 * script starts the server itself (and stops it at the end), so (a) is exact.
 *
 * --register (with --start, on an EMPTY store): first runs the brand-new-user
 * path — GET /api/first-use, POST /api/first-use/register for the candidate
 * whose label is --root — and times "first threads visible", then a first full
 * load (POST load, poll), before the three budgets. Nothing here is a budget;
 * it is reported as measured.
 *
 * The live store (C29). This script never opens a path inside the live
 * store itself, not even read-only: SQLite can create -wal and -shm beside a
 * file it opens, and that is a write to the founder's store. The live store is
 * $HOME/.thruline, resolved through symlinks, and also $THRULINE_HOME when it
 * is set to anything outside the OS temp folder or /tmp (every thruline
 * command treats a set THRULINE_HOME as its home). --db inside it is allowed
 * only with --start, where the path goes to ThruLine's own server and the
 * script never touches it; --register against it is refused, because
 * registering is for an empty scratch store; and --json, --home or
 * --thruline-home inside it are refused. Use an APFS clone (cp -c) instead.
 *
 * Writes nothing but what the server's own routes write (the load, and a
 * registration when asked). Exit 0 when every budget passes, 1 when one fails,
 * 2 when the check itself could not run or was refused.
 */
import { spawn } from 'node:child_process'
import { realpathSync } from 'node:fs'
import { writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const BUDGET_MS = { inbox: 2000, reload: 5000, switch: 300 }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const now = () => Number(process.hrtime.bigint() / 1000n) / 1000

export function parse(argv) {
  const o = { threads: 5, port: 7790, switches: 20 }
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i]
    const v = () => argv[++i]
    if (a === '--base') o.base = v()
    else if (a === '--start') o.start = v()
    else if (a === '--db') o.db = v()
    else if (a === '--home') o.home = v()
    else if (a === '--thruline-home') o.thrulineHome = v()
    else if (a === '--port') o.port = Number(v())
    else if (a === '--project') o.project = v()
    else if (a === '--root') o.root = v()
    else if (a === '--register') o.register = true
    else if (a === '--reads-only') o.readsOnly = true
    else if (a === '--threads') o.threads = Number(v())
    else if (a === '--switches') o.switches = Number(v())
    else if (a === '--json') o.json = v()
    else throw new Error(`unknown argument ${a}`)
  }
  if (!o.base && !o.start) throw new Error('give --base <url> or --start <thruline> --db <ledger.db>')
  if (o.start && !o.db) throw new Error('--start needs --db')
  if (o.register && !o.start) throw new Error('--register needs --start')
  if (o.register && o.readsOnly) throw new Error('--register writes; it cannot run with --reads-only')
  if (!Number.isInteger(o.switches) || o.switches < 1) throw new Error('--switches needs a whole number of at least 1')
  return o
}

/** `p` resolved through symlinks: real when it exists, else its nearest
 *  existing ancestor made real with the rest joined back on. A leading `~/`
 *  is the HOME given, as a shell would have expanded it. */
export function resolveThrough(p, env = process.env) {
  let abs = p
  if (abs === '~' || abs.startsWith('~/')) abs = path.join(env.HOME || os.homedir(), abs.slice(1))
  abs = path.resolve(abs)
  const tail = []
  let at = abs
  for (;;) {
    try {
      return path.join(realpathSync(at), ...tail.reverse())
    } catch {
      const parent = path.dirname(at)
      if (parent === at) return abs
      tail.push(path.basename(at))
      at = parent
    }
  }
}

const inside = (child, root) => child === root || child.startsWith(root.endsWith(path.sep) ? root : root + path.sep)

/** The live-store roots: $HOME/.thruline, and $THRULINE_HOME unless it lies
 *  in a scratch folder (the OS temp folder or /tmp). */
export function liveRoots(env = process.env, scratch = [os.tmpdir(), '/tmp']) {
  const roots = []
  const home = env.HOME || os.homedir()
  if (home) roots.push(resolveThrough(path.join(home, '.thruline'), env))
  if (env.THRULINE_HOME) {
    const th = resolveThrough(env.THRULINE_HOME, env)
    const scratchRoots = scratch.map((s) => resolveThrough(s, env))
    if (!scratchRoots.some((s) => inside(th, s))) roots.push(th)
  }
  return roots
}

/** Why these options would make this script reach into the live store, or
 *  null when they would not (C29). */
export function liveStoreRefusal(o, env = process.env, scratch = undefined) {
  const roots = liveRoots(env, scratch)
  const live = (p) => (p ? roots.find((r) => inside(resolveThrough(p, env), r)) : undefined)
  const clone = (root) => `Clone it first (cp -c ${root}/ledger.db <scratch>/ledger.db) and pass the clone.`
  const dbRoot = live(o.db)
  if (dbRoot && o.register) {
    return `--register against ${o.db} is refused: it is inside the live store ${dbRoot}, and registering is for an empty scratch store. ${clone(dbRoot)}`
  }
  if (dbRoot && !o.start) {
    return `--db ${o.db} is inside the live store ${dbRoot}; it is allowed only with --start, where ThruLine's own server opens it. ${clone(dbRoot)}`
  }
  for (const [flag, p] of [['--json', o.json], ['--home', o.home], ['--thruline-home', o.thrulineHome]]) {
    const root = live(p)
    if (root) return `${flag} ${p} is inside the live store ${root}; this script never writes or opens anything there. Use a clone (cp -c) or a scratch folder.`
  }
  return null
}

/** Nearest-rank quantile: the ceil(q*n)-th smallest of `xs`. */
export function quantile(xs, q) {
  if (!xs.length) return null
  const sorted = [...xs].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length, Math.max(1, Math.ceil(q * sorted.length))) - 1]
}

async function get(base, path) {
  const t = now()
  const res = await fetch(base + path, { headers: { accept: 'application/json' } })
  const text = await res.text()
  const ms = now() - t
  let body = null
  try { body = JSON.parse(text) } catch { body = null }
  return { status: res.status, body, ms, bytes: text.length }
}

async function post(base, path, body) {
  const t = now()
  const res = await fetch(base + path, {
    method: 'POST',
    headers: { origin: base, 'content-type': 'application/json', accept: 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const text = await res.text()
  let parsed = null
  try { parsed = JSON.parse(text) } catch { parsed = null }
  return { status: res.status, body: parsed, ms: now() - t }
}

function startServer(o) {
  const env = { ...process.env }
  if (o.home) env.HOME = o.home
  if (o.thrulineHome) env.THRULINE_HOME = o.thrulineHome
  const child = spawn(o.start, ['--db', o.db, 'ui', '--no-open', '--port', String(o.port), '--address', '127.0.0.1'], {
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  let log = ''
  child.stdout.on('data', (d) => { log += d })
  child.stderr.on('data', (d) => { log += d })
  return { child, log: () => log, started: now(), base: `http://127.0.0.1:${o.port}` }
}

async function waitForServer(base, child, limitMs = 60000) {
  const t = now()
  while (now() - t < limitMs) {
    if (child && child.exitCode !== null) throw new Error(`the server exited (${child.exitCode})`)
    try {
      const r = await fetch(base + '/api/hq/load')
      if (r.ok) { await r.text(); return now() - t }
    } catch { /* not listening yet */ }
    await sleep(25)
  }
  throw new Error('the server did not answer within 60 s')
}

async function stopServer(server) {
  if (!server) return
  server.child.kill('SIGINT')
  for (let i = 0; i < 100 && server.child.exitCode === null; i += 1) await sleep(50)
  if (server.child.exitCode === null) server.child.kill('SIGKILL')
}

async function waitLoad(base, limitMs) {
  const t = now()
  let last = null
  while (now() - t < limitMs) {
    const r = await get(base, '/api/hq/load')
    last = r.body
    if (r.body && r.body.running === false && r.body.finished_at) return { ms: now() - t, status: r.body }
    await sleep(50)
  }
  return { ms: null, status: last }
}

async function pickProject(base, o) {
  if (o.project) return o.project
  const list = await get(base, '/api/hq/projects')
  const rows = (list.body && list.body.projects) || []
  const row = o.root ? rows.find((p) => p.root === o.root) : rows[0]
  if (!row) throw new Error(`no HQ project${o.root ? ` for ${o.root}` : ''} on this store`)
  return row.id
}

async function main() {
  const o = parse(process.argv.slice(2))
  const refusal = liveStoreRefusal(o)
  if (refusal) throw new Error(`refused: ${refusal}`)
  const results = { budgets: {}, measured: {} }
  let server = null
  let base = o.base
  try {
    if (o.start) {
      server = startServer(o)
      base = server.base
      results.measured.server_answers_ms = await waitForServer(base, server.child)
    }

    if (o.register) {
      const t0 = now()
      const fu = await get(base, '/api/first-use')
      const cands = (fu.body && fu.body.candidates) || []
      const pick = cands
        .filter((c) => !o.root || c.label === o.root)
        .sort((a, b) => (b.recordings || 0) - (a.recordings || 0))[0]
      if (!pick) throw new Error(`no first-use candidate${o.root ? ` labelled ${o.root}` : ''}`)
      const reg = await post(base, '/api/first-use/register', { key: pick.key })
      if (reg.status !== 200) throw new Error(`register answered ${reg.status}: ${JSON.stringify(reg.body)}`)
      let threads = 0
      let id = null
      while (now() - t0 < 120000 && threads === 0) {
        try {
          id = await pickProject(base, o)
          const view = await get(base, `/api/hq/projects/${encodeURIComponent(id)}`)
          threads = (view.body && view.body.threads && view.body.threads.length) || 0
        } catch { /* not there yet */ }
        if (threads === 0) await sleep(100)
      }
      results.measured.register_ms = reg.ms
      results.measured.first_threads_visible_ms = now() - t0
      results.measured.first_threads = threads
      o.project = id
      const t1 = now()
      const started = await post(base, `/api/hq/projects/${encodeURIComponent(id)}/load`)
      if (started.status !== 202) throw new Error(`load answered ${started.status}`)
      const done = await waitLoad(base, 3600000)
      results.measured.first_full_load_ms = done.ms === null ? null : now() - t1
      results.measured.first_full_load = done.status
    }

    // (a) Inbox ready after start. With --start, restart so the read is the
    // first after a fresh start of a loaded store.
    if (o.start) {
      await stopServer(server)
      server = startServer(o)
      base = server.base
      await waitForServer(base, server.child)
    }
    const id = await pickProject(base, o)
    const inbox = await get(base, `/api/hq/projects/${encodeURIComponent(id)}`)
    const inboxMs = o.start ? now() - server.started : inbox.ms
    const allThreads = (inbox.body && inbox.body.threads) || []
    const threadIds = allThreads.map((t) => t.id)
    results.budgets.inbox = {
      ms: inboxMs,
      budget_ms: BUDGET_MS.inbox,
      status: inbox.status,
      threads: threadIds.length,
      basis: o.start ? 'server start → first project read answered' : 'first project read by this script (server already running)',
    }

    // (b) No-change reload.
    if (!o.readsOnly) {
    const t = now()
    const started = await post(base, `/api/hq/projects/${encodeURIComponent(id)}/load`)
    let reload = { ms: null, status: started.body }
    if (started.status === 202) {
      const done = await waitLoad(base, 600000)
      reload = { ms: done.ms === null ? null : now() - t, status: done.status }
    }
    results.budgets.reload = { ms: reload.ms, budget_ms: BUDGET_MS.reload, http: started.status, load: reload.status }
    }

    // (c) Screen switching on a Change, warm: the reads a Change screen
    // makes when it opens, together (`loadChange`, the tracked-Change lookup,
    // the lanes line and the ⌘K project list). Storage is the shared reading
    // and is timed on its own below.
    // The newest --threads threads, and the --threads with the most edits:
    // a big thread's view is the slowest read there is, and the newest ones
    // are often small.
    const biggest = [...allThreads].sort((a, b) => (b.edits || 0) - (a.edits || 0)).map((t) => t.id)
    const sample = [...new Set([
      ...threadIds.slice(0, Math.max(1, o.threads)),
      ...biggest.slice(0, Math.max(1, o.threads)),
    ])]
    const reads = (tid) => Promise.all([
      get(base, '/api/hq/projects'),
      get(base, `/api/hq/projects/${encodeURIComponent(id)}`),
      get(base, `/api/hq/projects/${encodeURIComponent(id)}/threads/${encodeURIComponent(tid)}`),
      get(base, '/api/changes'),
      get(base, '/api/lanes'),
    ])
    for (const tid of sample) await reads(tid) // the first open of each thread: not a switch
    const per = []
    for (let i = 0; sample.length && i < o.switches; i += 1) {
      const tid = sample[i % sample.length]
      const t2 = now()
      const rs = await reads(tid)
      per.push({ thread: tid, ms: now() - t2, bytes: rs.reduce((n, r) => n + r.bytes, 0), statuses: rs.map((r) => r.status) })
    }
    const times = per.map((p) => p.ms)
    const p95 = quantile(times, 0.95)
    const ok = per.every((p) => p.statuses.every((c) => c === 200))
    results.budgets.switch = {
      ms: ok ? p95 : null,
      basis: 'p95 (nearest rank)',
      p50_ms: quantile(times, 0.5),
      p95_ms: p95,
      max_ms: times.length ? Math.max(...times) : null,
      switches: per.length,
      sample_threads: sample.length,
      all_answered_200: ok,
      budget_ms: BUDGET_MS.switch,
      per_switch: per,
    }
    const st = now()
    const storage = await get(base, '/api/storage')
    results.measured.storage_ms = storage.status === 200 ? now() - st : null
  } finally {
    await stopServer(server)
  }

  let failed = false
  for (const [name, r] of Object.entries(results.budgets)) {
    const pass = typeof r.ms === 'number' && r.ms < r.budget_ms
    if (!pass) failed = true
    console.log(`${pass ? 'PASS' : 'FAIL'}  ${name.padEnd(7)} ${r.ms === null ? 'did not finish' : `${r.ms.toFixed(0)} ms`}  (budget ${r.budget_ms} ms)`)
  }
  const sw = results.budgets.switch
  if (sw && sw.switches) {
    console.log(`      switch over ${sw.switches} switches (${sw.sample_threads} threads): p50 ${sw.p50_ms.toFixed(0)} ms, p95 ${sw.p95_ms.toFixed(0)} ms, max ${sw.max_ms.toFixed(0)} ms${sw.all_answered_200 ? '' : ' (a read did not answer 200)'}`)
  }
  for (const [name, v] of Object.entries(results.measured)) {
    if (typeof v === 'number') console.log(`      ${name} ${v.toFixed(0)}`)
  }
  if (o.json) await writeFile(o.json, JSON.stringify(results, null, 2))
  process.exit(failed ? 1 : 0)
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main().catch((e) => {
    console.error(`perf-check: ${e.message}`)
    process.exit(2)
  })
}
