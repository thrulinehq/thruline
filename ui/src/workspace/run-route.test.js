import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* P6-5 — Check it again, without a terminal: the run sheet, rendered.
 *
 * The served views below are the shape `crates/cli/src/uiserve/run_route.rs`
 * answers with; the Rust side proves those fields come from real processes
 * (`uiserve/tests/run_route_guards.rs`). Here every assertion is about the
 * STRINGS AND CONTROLS A PERSON MEETS in each of the six outcomes. A probe
 * bundle swaps two copy strings, so a literal in the component cannot pass.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-run-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { RunSheetBody } from './run-sheet.jsx';
  import { runSheetModel, outcomeView, oneAtATime, OUTCOMES, agentPrompt, consentView, refusalText } from './run-route.js';
  import { HQ_COPY } from './hq-copy.js';
  export { runSheetModel, outcomeView, oneAtATime, OUTCOMES, agentPrompt, consentView, refusalText, HQ_COPY };
  export const render = (props) => renderToStaticMarkup(React.createElement(RunSheetBody, props));
`
const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
async function load(name, plugins) {
  const result = await build({ ...COMMON, stdin: { contents: ENTRY, resolveDir: here }, plugins })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const M = await load('run.mjs')
const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('run-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("allow_and_run: 'Allow and run the check'", "allow_and_run: 'probe-allow-and-run'")
        .replace("title: 'Tool not found'", "title: 'probe-tool-not-found'")
        .replace("take_back: 'Take back permission'", "take_back: 'probe-take-back'")
        .replace("heading: 'Who allowed checks here'", "heading: 'probe-consent-heading'"),
      loader: 'js',
    }))
  },
}])

const { runSheetModel, outcomeView, oneAtATime, OUTCOMES, HQ_COPY, render } = M
const R = HQ_COPY.run

const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()
const outsideTechnical = (html) => html.replace(/<details class="ws-run-technical">[\s\S]*?<\/details>/g, '')
const enabledPrimaries = (html) => (html.match(/<button[^>]*class="ws-change-primary"[^>]*>/g) || []).filter((b) => !/disabled/.test(b))

const PERMISSION = { granted_by: 'sam', granted_at: '2026-09-23T10:00:00Z' }
const ENVS = [
  { id: 'hq', kind: 'hq', folder: null, has: [] },
  { id: 'home-cargo', kind: 'machine', folder: '~/.cargo/bin', has: ['cargo'] },
  { id: 'project-venv', kind: 'project', folder: '~/shop/.venv/bin', has: [] },
]
function view(last, over = {}) {
  return {
    schema_version: 1,
    change: 'c1',
    project_folder: '~/shop',
    permission: PERMISSION,
    checks: [{ id: 'cargo-test', program: 'cargo', category: 'tests', command: 'cargo test', last }],
    not_runnable_here: [],
    environments: ENVS,
    remembered_environment: null,
    job: null,
    not_started: null,
    actor: 'sam',
    ...over,
  }
}
const LAST = {
  passed: { run: 'r1', state: 'completed', outcome: 'passed', why: null, exit_code: 0, tests_passed: 4, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:05:00Z', settles: true },
  failed: { run: 'r2', state: 'completed', outcome: 'failed', why: null, exit_code: 101, tests_passed: 3, tests_failed: 1, suites: 1, failure_line: "assertion `left == right` failed", recorded_at: '2026-09-23T10:06:00Z', settles: true },
  checked_nothing: { run: 'r3', state: 'completed', outcome: 'checked_nothing', why: 'no_test_ran', exit_code: 0, tests_passed: 0, tests_failed: 0, suites: 1, recorded_at: '2026-09-23T10:07:00Z', settles: true },
  cancelled: { run: 'r4', state: 'cancelled', outcome: 'cancelled', reason: 'cancelled_by_person', exit_code: null, recorded_at: '2026-09-23T10:08:00Z', settles: false },
  interrupted: { run: 'r5', state: 'cancelled', outcome: 'interrupted', reason: 'hq_shut_down', exit_code: null, recorded_at: '2026-09-23T10:09:00Z', settles: false },
  tool_not_found: { run: 'r6', state: 'blocked', outcome: 'tool_not_found', reason: 'prerequisite_absent', exit_code: null, recorded_at: '2026-09-23T10:10:00Z', settles: false },
}
const sheet = (v, ctx = {}, props = {}) => render({ model: runSheetModel(v, { version: { short: 'e71c5b0' }, ...ctx }), ...props })

/* ── the six outcomes ──────────────────────────────────────────────────── */

test('each of the six outcomes renders its own title and sentence, and only Passed reads as a pass', () => {
  assert.deepEqual([...OUTCOMES], ['passed', 'failed', 'checked_nothing', 'cancelled', 'interrupted', 'tool_not_found'])
  for (const kind of OUTCOMES) {
    const html = sheet(view(LAST[kind]))
    const body = text(html)
    assert.ok(body.includes(R.outcomes[kind].title), `${kind}: ${body}`)
    assert.match(html, new RegExp(`data-outcome="${kind}" data-passes="${kind === 'passed' ? 'yes' : 'no'}"`), kind)
    const o = outcomeView(LAST[kind], { program: 'cargo' })
    assert.equal(o.passes, kind === 'passed', `${kind} passes=${o.passes}`)
    if (kind !== 'passed') {
      assert.ok(!/\bPassed\b/.test(body), `${kind} reads as a pass: ${body}`)
    }
  }
  assert.ok(text(sheet(view(LAST.passed))).includes('4 tests ran and none failed.'))
  assert.ok(text(sheet(view(LAST.failed))).includes('1 of 4 tests failed.'))
  assert.ok(text(sheet(view(LAST.failed))).includes("assertion `left == right` failed"), 'the failing line is the record speaking')
  assert.ok(text(sheet(view(LAST.checked_nothing))).includes('Ran, checked nothing (not a pass)'))
  assert.ok(text(sheet(view(LAST.checked_nothing))).includes('no test ran, so it proves nothing'))
  assert.ok(text(sheet(view(LAST.cancelled))).includes('You stopped it before it finished. It settles nothing.'))
  assert.ok(text(sheet(view(LAST.interrupted))).includes('ThruLine shut down while it was running.'))
  assert.ok(text(sheet(view(LAST.tool_not_found))).includes("couldn't find cargo"))
  assert.ok(text(sheet(view(LAST.tool_not_found))).includes('Nothing was installed.'))
})

/* OFFLINE-BUILD-OUTCOME (D-051): a build that could not fetch what it needs is
 * the couldn't-check outcome, said in the ruled sentence the product already
 * has for "something this check needs isn't on this machine" — never
 * "Failed", never a pass, and never the no-test sentence (which would be a
 * claim about tests the run never reached). */
test('an offline build that could not fetch its dependencies reads checked-nothing, in the ruled words', () => {
  const last = { run: 'r7', state: 'completed', outcome: 'checked_nothing', why: 'prerequisite_absent', reason: 'prerequisite_absent', exit_code: 101, recorded_at: '2026-09-27T10:00:00Z', settles: true }
  const o = outcomeView(last, { program: 'cargo' })
  assert.equal(o.kind, 'checked_nothing')
  assert.equal(o.passes, false)
  const ruled = HQ_COPY.change.model.execution.reasons.execution_reason_prerequisite_absent
  assert.deepEqual(o.lines, [ruled])
  const body = text(sheet(view(last)))
  assert.ok(body.includes('Ran, checked nothing (not a pass)'), body)
  assert.ok(body.includes("Something this check needs isn't on this machine."), body)
  assert.ok(!body.includes('no test ran'), body)
  assert.ok(!/\bFailed\b/.test(body), body)
})

/* OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4): when ThruLine's own fetch step did
 * not run, the sheet says so beside the outcome, with the served reason (the
 * program, where it was found, what would allow it). A failure stays Failed. */
test('a skipped owned fetch is said on the sheet, and a failure still reads Failed', () => {
  const why = 'cargo found at /opt/tools/cargo is not in a place ThruLine takes cargo from; installing the toolchain with rustup would allow it'
  const last = { ...LAST.failed, fetch_skipped: why }
  const o = outcomeView(last, { program: 'cargo' })
  assert.equal(o.kind, 'failed')
  assert.equal(o.passes, false)
  const said = R.outcomes.fetch_skipped.replace('{why}', why)
  assert.ok(o.lines.includes(said), JSON.stringify(o.lines))
  const body = text(sheet(view(last)))
  assert.ok(body.includes(said), body)
  assert.ok(body.includes(R.outcomes.failed.title), body)
  assert.ok(!text(sheet(view(LAST.failed))).includes(R.outcomes.fetch_skipped.split('{why}')[0].trim()), 'said without a served reason')
})

test('a count on the sheet is a count the record holds: no count, no number', () => {
  const noCount = { ...LAST.failed, tests_passed: null, tests_failed: null }
  const body = text(sheet(view(noCount)))
  assert.ok(body.includes(R.outcomes.failed.line_no_count), body)
  assert.ok(!/\d+ of \d+ tests/.test(body), body)
  assert.equal(outcomeView({ outcome: 'running' }), null, 'a run still going has no outcome')
  assert.equal(outcomeView({ outcome: 'made-up' }), null, 'a word outside the six is not an outcome')
})

/* ── the sheet: what, where, can, cannot, permission ───────────────────── */

test('the sheet says what will run, where, what it can and cannot establish, and asks permission once', () => {
  const html = sheet(view(null, { permission: null }))
  const body = text(html)
  for (const s of [R.what_tests, 'In ~/shop, with the programs ThruLine was started with.', 'for version e71c5b0', R.cannot, R.permission_heading, R.permission]) {
    assert.ok(body.includes(s), `missing: ${s}\n${body}`)
  }
  assert.ok(body.includes('It does not install anything and does not touch the network.'), 'the promise, word for word')
  assert.ok(body.includes(R.allow_and_run), 'the one control says it allows and runs')
  /* Once given, it is not asked again — it is said. */
  const given = text(sheet(view(null)))
  assert.ok(!given.includes(R.permission_heading), given)
  assert.ok(given.includes('You allowed checks in this project on'), given)
  assert.ok(given.includes(R.run), given)
})

test('the raw command is only on the Technical face', () => {
  for (const kind of [null, ...OUTCOMES]) {
    const html = sheet(view(kind ? LAST[kind] : null))
    assert.ok(html.includes('cargo test'), 'the command is on the Technical face')
    assert.ok(!outsideTechnical(html).includes('cargo test'), `${kind}: the raw command leaked onto the plain face`)
  }
})

/* ── one control: Run the check → Cancel; a double-click is one job ────── */

test('one primary control: Run the check, Cancel while running, and every off state says why', () => {
  const idle = sheet(view(LAST.passed))
  assert.equal(enabledPrimaries(idle).length, 1, idle)
  assert.ok(text(idle).includes(R.run_again))

  const running = sheet(view({ ...LAST.passed, outcome: 'running', state: 'running', started_at: '2026-09-23T10:00:00Z' }, { job: { run: 'r9', check: 'cargo-test', for_this_change: true, phase: 'running', started_at: '2026-09-23T10:00:00Z' } }))
  assert.equal(enabledPrimaries(running).length, 1)
  assert.ok(text(running).includes(R.cancel), running)
  assert.ok(!text(running).includes(R.run_again), 'the run button became Cancel')
  assert.ok(text(running).includes('Nothing is settled until it finishes.'))

  const starting = sheet(view(null, { job: { run: 'r9', check: 'cargo-test', for_this_change: true, phase: 'starting', started_at: '2026-09-23T10:00:00Z' } }))
  assert.equal(enabledPrimaries(starting).length, 0)
  assert.ok(text(starting).includes(R.blocked.cancel_wait.reason) && text(starting).includes(R.blocked.cancel_wait.enabler))

  const elsewhere = sheet(view(null, { job: { run: 'r8', check: 'cargo-test', for_this_change: false, phase: 'running' } }))
  assert.ok(text(elsewhere).includes(R.blocked.busy.reason) && text(elsewhere).includes(R.blocked.busy.enabler))

  const noFolder = sheet(view(null, { project_folder: null }))
  assert.ok(text(noFolder).includes(R.blocked.no_folder.reason) && text(noFolder).includes(R.blocked.no_folder.enabler))

  const noCheck = sheet(view(null, { checks: [] }))
  assert.ok(text(noCheck).includes(R.blocked.no_check.reason) && text(noCheck).includes(R.blocked.no_check.enabler))

  /* Every disabled button has its reason attached. */
  for (const html of [starting, elsewhere, noFolder, noCheck]) {
    for (const b of html.match(/<button[^>]*disabled[^>]*>/g) || []) assert.match(b, /aria-describedby="/, b)
  }
})

test('a double-click starts one job: a second press while the first is handled is dropped', async () => {
  let calls = 0
  let release
  const gate = new Promise((r) => { release = r })
  const press = oneAtATime(async () => { calls += 1; await gate })
  const first = press()
  const second = press()
  const third = press()
  release()
  await Promise.all([first, second, third])
  assert.equal(calls, 1, 'two clicks in one frame started two jobs')
  /* And once it is handled, the next press goes through. */
  await press()
  assert.equal(calls, 2)
})

/* ── Tool not found ────────────────────────────────────────────────────── */

test('Tool not found never sends Sam to a shell: ask the agent, or look somewhere else', () => {
  const html = sheet(view(LAST.tool_not_found), {}, { showPrompt: true, showElsewhere: true })
  const body = text(html)
  assert.ok(body.includes(R.tool.ask_agent), 'primary: ask the agent to set it up')
  assert.equal(enabledPrimaries(html).length, 1)
  assert.match(enabledPrimaries(html)[0] + html.slice(html.indexOf(enabledPrimaries(html)[0])), new RegExp(R.tool.ask_agent))
  assert.ok(body.includes(R.tool.look_elsewhere), 'secondary: look somewhere else')
  assert.ok(body.includes(R.tool.prompt_heading) && body.includes('prepared, not sent'))
  assert.ok(body.includes(R.tool.copy) && body.includes(R.tool.pasted))
  assert.ok(body.includes('could not find the program cargo'), body)
  assert.ok(body.includes(R.tool.elsewhere_note) && body.includes('Nothing is installed.'))
  assert.ok(body.includes('On this machine: ~/.cargo/bin') && body.includes('Has cargo.'))
  assert.ok(body.includes('In this project: ~/shop/.venv/bin') && body.includes('Does not have cargo.'))
  /* Exactly one place can be picked, and the one that cannot says why. */
  const picks = (html.match(/<button[^>]*>Run it here<\/button>/g) || [])
  assert.equal(picks.filter((b) => !/disabled/.test(b)).length, 1)
  assert.ok(body.includes(R.tool.pick_off.reason))
  /* Nothing on the plain face tells Sam to install or type anything. */
  const plain = text(outsideTechnical(html)).toLowerCase()
  for (const shell of ['brew install', 'pip install', 'npm install', 'sudo', 'curl ', 'terminal and', 'run this command']) {
    assert.ok(!plain.includes(shell), `the sheet sends Sam to a shell: ${shell}`)
  }
})

test('after "I pasted it", the reminder shows and the one primary is Run the check again', () => {
  const html = sheet(view(LAST.tool_not_found), { pending: '2026-09-23T10:11:00Z' })
  const body = text(html)
  assert.ok(body.includes('Waiting for the agent to set it up.'), body)
  assert.ok(body.includes(R.tool.pending_scope), 'says where the reminder is kept')
  assert.equal(enabledPrimaries(html).length, 1)
  assert.ok(enabledPrimaries(html)[0] !== undefined && html.includes('>' + R.run_again + '<'), body)
})

/* ── words ─────────────────────────────────────────────────────────────── */

test('every sheet string comes from the copy file (probe) and no engine noun reaches the plain face', () => {
  const probeModel = PROBED.runSheetModel(view(LAST.tool_not_found, { permission: null }))
  const probed = text(PROBED.render({ model: probeModel }))
  assert.ok(probed.includes('probe-allow-and-run'), probed)
  assert.ok(probed.includes('probe-tool-not-found'), probed)
  const ENGINE = /\b(events?|receipts?|candidates?|applicability|disposition|packet version|ledger|session id|tree hash|predicate|binding|exit code)\b/i
  const plainCopy = []
  const walk = (o, path) => {
    for (const [k, v] of Object.entries(o)) {
      if (path.concat(k)[0] === 'technical') continue
      if (typeof v === 'string') plainCopy.push([path.concat(k).join('.'), v])
      else walk(v, path.concat(k))
    }
  }
  walk(R, [])
  assert.ok(plainCopy.length > 40)
  for (const [path, s] of plainCopy) assert.doesNotMatch(s, ENGINE, `run.${path}: ${s}`)
  for (const kind of OUTCOMES) {
    const plain = text(outsideTechnical(sheet(view(LAST[kind]))))
    assert.doesNotMatch(plain, ENGINE, `${kind}: ${plain}`)
    assert.doesNotMatch(plain, /\br\d\b/, `${kind}: a run key on the plain face`)
  }
})

test('the run sheet writes to exactly its three declared addresses, all with the token', () => {
  const src = readFileSync(join(here, 'run-sheet.jsx'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  const addresses = (src.match(/'\/api\/[^']*'(?: \+ [^\n]*)?/g) || []).map((a) => a.trim()).sort()
  assert.deepEqual(addresses, [
    "'/api/changes/' + encodeURIComponent(id) + '/run'",
    "'/api/changes/' + encodeURIComponent(id) + '/run/permission'",
    "'/api/changes/' + encodeURIComponent(id) + '/run/permission/revoke'",
    "'/api/run/token'",
  ])
  assert.equal((src.match(/method: 'POST'/g) || []).length, 1, 'one writer')
  assert.match(src, /'X-ThruLine-Run-Token': token\.doc\.token/, 'the writer carries the token')
  const writes = (src.match(/postWithToken\((\w+)\(/g) || []).sort()
  assert.deepEqual(writes, ['postWithToken(RUN_PERMISSION(', 'postWithToken(RUN_PERMISSION_REVOKE(', 'postWithToken(RUN_SHEET('])
  /* The request carries a check id and an environment id, never a command. */
  assert.match(src, /const body = \{ check: m\.check\.id \}/)
  assert.ok(!/command:|args:|argv:|cwd:|env:/.test(src.replace(/technical\.command/g, '')), 'the request could carry a command')
})

/* ── the sheet survives the screen underneath changing ─────────────────── */

test('a run that changes the screen cannot unmount the open sheet: the host holds it, not the door', async () => {
  const { JSDOM } = await import('jsdom')
  const dom = new JSDOM('<!doctype html><body><div id="root"></div></body>', { url: 'http://127.0.0.1:7781/#/changes/c1' })
  const keep = {}
  for (const k of ['window', 'document', 'navigator', 'HTMLElement', 'Node', 'fetch', 'localStorage', 'IS_REACT_ACT_ENVIRONMENT']) keep[k] = Object.getOwnPropertyDescriptor(globalThis, k)
  const set = (k, v) => Object.defineProperty(globalThis, k, { value: v, configurable: true, writable: true })
  set('window', dom.window); set('document', dom.window.document); set('navigator', dom.window.navigator)
  set('HTMLElement', dom.window.HTMLElement); set('Node', dom.window.Node); set('localStorage', dom.window.localStorage)
  set('IS_REACT_ACT_ENVIRONMENT', true)
  set('fetch', async () => ({ ok: true, status: 200, json: async () => view(LAST.passed) }))
  /* React's `act` opens a MessageChannel per flush and never closes it; the
   * same tracking checks-panel.test.js uses, so node can exit. */
  const heldChannel = globalThis.MessageChannel
  const channels = []
  globalThis.MessageChannel = class extends heldChannel { constructor() { super(); channels.push(this) } }
  try {
    const result = await build({
      ...COMMON,
      stdin: {
        contents: `
          import React from 'react';
          import { createRoot } from 'react-dom/client';
          import { act } from 'react';
          import { RunDoor, RunSheetHost } from './run-sheet.jsx';
          export { React, createRoot, act, RunDoor, RunSheetHost };
        `,
        resolveDir: here,
      },
    })
    const file = join(temporary, 'host.mjs')
    await writeFile(file, result.outputFiles[0].text)
    const { React, createRoot, act, RunDoor, RunSheetHost } = await import(pathToFileURL(file).href)
    const h = React.createElement
    const screen = (doorShown) => h(RunSheetHost, null, doorShown
      ? h(RunDoor, { changeId: 'c1', label: 'Run the check again', primary: true })
      : h('button', { type: 'button' }, 'Approve this version'))
    const root = createRoot(dom.window.document.getElementById('root'))
    await act(async () => { root.render(screen(true)) })
    const door = [...dom.window.document.querySelectorAll('button')].find((b) => b.textContent === 'Run the check again')
    assert.ok(door, 'the door is on the screen')
    await act(async () => { door.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true })) })
    await act(async () => { await new Promise((r) => setTimeout(r, 20)) })
    assert.ok(dom.window.document.querySelector('.ws-run-sheet'), 'the sheet opened')
    /* The run ends; the record says the Change is ready; the Inspector's one
     * primary becomes Approve and the door is no longer rendered. */
    await act(async () => { root.render(screen(false)) })
    await act(async () => { await new Promise((r) => setTimeout(r, 20)) })
    assert.ok(dom.window.document.querySelector('.ws-run-sheet'), 'the sheet vanished when the door stopped being rendered')
    assert.ok(dom.window.document.querySelector('.ws-run-sheet').textContent.includes('Passed'), 'and it still shows the outcome')
    /* Closing it is still Sam's act. */
    const close = dom.window.document.querySelector('.ws-run-close')
    await act(async () => { close.dispatchEvent(new dom.window.MouseEvent('click', { bubbles: true })) })
    assert.equal(dom.window.document.querySelector('.ws-run-sheet'), null)
    await act(async () => { root.unmount() })
  } finally {
    for (const c of channels) { try { c.port1.close(); c.port2.close() } catch { /* already closed */ } }
    globalThis.MessageChannel = heldChannel
    for (const [k, d] of Object.entries(keep)) {
      if (d) Object.defineProperty(globalThis, k, d)
      else delete globalThis[k]
    }
    dom.window.close()
  }
})

/* ── there is always a way to run it again from Brief ──────────────────── */

test('a Change that needs a re-check and is ready to approve still has a door to Check it again', async () => {
  const result = await build({ ...COMMON, stdin: { contents: "export { runsFromHere, showsDecision } from './change-workspace.jsx';", resolveDir: here } })
  const file = join(temporary, 'doors.mjs')
  await writeFile(file, result.outputFiles[0].text)
  const { runsFromHere, showsDecision } = await import(pathToFileURL(file).href)
  const ready = { ready: true }
  const approvable = { canApprove: true, canWithdraw: false }
  /* Needs a re-check, nothing to approve: the Inspector's primary is the door. */
  assert.equal(runsFromHere({ id: 'c1', state: 'needs_recheck', decision: { canApprove: false }, readiness: { ready: false } }), true)
  /* Needs a re-check AND ready to approve (seen in the walk, 08-tool-not-found):
   * the Inspector shows Approve, so the door is not the Inspector's — Brief
   * carries it (change-workspace.jsx renders the Brief door when !runsFromHere). */
  const both = { id: 'c1', state: 'needs_recheck', decision: approvable, readiness: ready }
  assert.equal(showsDecision(both), true)
  assert.equal(runsFromHere(both), false, 'Brief would hide its door while the Inspector shows Approve: no way to run it again')
  /* An approval can be taken back: that is the Inspector's primary too. */
  assert.equal(runsFromHere({ id: 'c1', state: 'recorded', decision: { canWithdraw: true }, readiness: { ready: false } }), false)
})

/* ── P6-5B: the permission, from the record ────────────────────────────── */

const GRANT_ROW = { act: 'granted', actor: 'sam', at: '2026-09-24T09:02:00Z', revokes: null, receipt: { table: 'run_permissions', seq: 0, id: 'rpm_01AAAAAAAAAAAAAAAAAAAAAAAA', chain_hash: 'ab'.repeat(32) } }
const REVOKE_ROW = { act: 'revoked', actor: 'sam', at: '2026-09-24T09:07:00Z', revokes: GRANT_ROW.receipt.id, receipt: { table: 'run_permissions', seq: 1, id: 'rpm_01BBBBBBBBBBBBBBBBBBBBBBBB', chain_hash: 'cd'.repeat(32) } }
const given = { state: 'given', history: [GRANT_ROW], file: '.thruline/run-permission.json' }
const revoked = { state: 'revoked', history: [GRANT_ROW, REVOKE_ROW], file: '.thruline/run-permission.json' }

test('P6-5B: the sheet shows who allowed checks here and when, from the record, with a secondary way to take it back', () => {
  const html = sheet(view(LAST.passed, { consent: given }))
  const plain = text(outsideTechnical(html))
  const at = M.consentView(given).lines[0]
  assert.match(at, /^sam allowed checks in this project on 24 Sept? 2026 at \d\d:\d\d\.$/, at)
  assert.ok(plain.includes(R.consent.heading) && plain.includes(at), plain)
  assert.ok(html.includes('class="ws-change-secondary ws-run-take-back"'), 'take back is secondary')
  assert.ok(plain.includes(R.consent.take_back) && plain.includes(R.consent.take_back_note), plain)
  assert.equal(enabledPrimaries(html).length, 1, 'taking back does not compete with the one primary')
  /* The row each line comes from is on the Technical face, and only there. */
  const tech = text(html.match(/<details class="ws-run-technical">[\s\S]*?<\/details>/)[0])
  assert.ok(tech.includes('Allowed · row 0 of run_permissions') && tech.includes(GRANT_ROW.receipt.id) && tech.includes(GRANT_ROW.receipt.chain_hash), tech)
  assert.ok(tech.includes('.thruline/run-permission.json'), tech)
  assert.ok(!plain.includes('run_permissions') && !plain.includes('rpm_'), plain)
})

test('P6-2 Part 0: "allowed checks" is said once, on one line that carries who and when', () => {
  const plain = text(outsideTechnical(sheet(view(LAST.passed, { consent: given }))))
  const said = plain.match(/allowed checks in this project on/g) || []
  assert.equal(said.length, 1, plain)
  assert.ok(!plain.includes('You allowed checks in this project on'), 'the date-only sentence is not repeated above the record line')
  assert.match(plain, /sam allowed checks in this project on 24 Sept? 2026 at \d\d:\d\d\./)
  assert.ok(plain.includes(R.promise), 'the promise is still said')
  /* With no history to show (an older server), the date-only sentence is the one line. */
  const bare = text(outsideTechnical(sheet(view(LAST.passed))))
  assert.equal((bare.match(/allowed checks in this project on/g) || []).length, 1, bare)
  assert.ok(bare.includes('You allowed checks in this project on'), bare)
})

test('P6-5B: after it is taken back, the sheet says so and asks again; the one primary is Allow and run', () => {
  const html = sheet(view(LAST.passed, { permission: null, consent: revoked }))
  const plain = text(outsideTechnical(html))
  const lines = M.consentView(revoked).lines
  assert.equal(lines.length, 2)
  assert.match(lines[1], /^sam took that permission back on 24 Sept? 2026 at \d\d:\d\d\.$/)
  assert.ok(plain.includes(lines[0]) && plain.includes(lines[1]), plain)
  assert.ok(plain.includes(R.consent.state_revoked), plain)
  assert.ok(!plain.includes(R.consent.take_back), 'nothing standing to take back')
  assert.equal(enabledPrimaries(html).length, 1)
  assert.ok(html.includes('>' + R.allow_and_run + '<'), plain)
  const tech = text(html.match(/<details class="ws-run-technical">[\s\S]*?<\/details>/)[0])
  assert.ok(tech.includes('Taken back · row 1 of run_permissions') && tech.includes(REVOKE_ROW.receipt.id), tech)
})

test('P6-5B: a note the record does not hold, and a missing copy, are each said in words', () => {
  const notHeld = text(outsideTechnical(sheet(view(LAST.passed, { permission: null, consent: { state: 'not_in_record', history: [] } }))))
  assert.ok(notHeld.includes(R.consent.state_not_in_record), notHeld)
  const missing = sheet(view(LAST.passed, { permission: null, consent: { state: 'file_missing', history: [GRANT_ROW] } }))
  assert.ok(text(missing).includes(R.consent.state_file_missing))
  assert.ok(text(missing).includes(R.consent.take_back), 'the record still holds a grant, so it can be taken back')
  assert.equal(M.consentView(null).lines.length, 0, 'no history served, none invented')
  assert.equal(M.consentView({ state: 'never', history: [] }).stateLine, null)
})

test('P6-5B: a refused run after taking permission back is said in Sam\'s words, never as an error code', () => {
  for (const slug of ['permission_revoked', 'permission_not_in_record', 'no_permission_to_revoke']) {
    const words = M.refusalText(slug)
    assert.notEqual(words, R.refusals.other, slug)
    assert.doesNotMatch(words, /_|ledger|event|record of the row/i, `${slug}: ${words}`)
  }
  assert.match(M.refusalText('permission_revoked'), /^Nothing ran: permission for checks in this project was taken back\./)
})

test('ADV-run-security-1: an Allow refused over a planted link is said in words, never as an error code', () => {
  const words = M.refusalText('permission_file_not_plain')
  assert.notEqual(words, R.refusals.other, 'the refusal fell through to the generic sentence')
  assert.doesNotMatch(words, /_|ledger|event|symlink|errno|nofollow/i, words)
  assert.match(words, /^Nothing was allowed: /)
})

test('ADV-run-security-2: a run refused because its program is inside the project names that program, in words', () => {
  const words = M.refusalText('program_in_project', { error: 'program_in_project', program: 'cargo', found_at: '~/shop/node_modules/.bin/cargo' })
  assert.notEqual(words, R.refusals.other, 'the refusal fell through to the generic sentence')
  assert.match(words, /^Nothing ran: the cargo this check would use is inside this project/)
  assert.doesNotMatch(words, /_|\{|\}|ledger|event|PATH|node_modules/i, words)
  // Never a bare placeholder, whatever was served.
  for (const doc of [undefined, null, {}, { program: '' }, { program: 7 }]) {
    assert.doesNotMatch(M.refusalText('program_in_project', doc), /\{|\}/, JSON.stringify(doc))
  }
  // A sentence with no program in it is unchanged by a served document.
  assert.equal(M.refusalText('permission_revoked', { program: 'cargo' }), M.refusalText('permission_revoked'))
})

/* D-049 (founder ruling 3): refusing a program that lives inside the project
 * is accepted for launch; the refusal names the program, says it was refused
 * because it lives inside the project, and says what would allow it. The
 * route test `the_program_in_project_refusal_names_the_program_says_why_and_says_what_would_allow_it`
 * (run_route_guards.rs) holds the served refusal, this same sentence, and that
 * doing what it says lets the run go. */
const IN_PROJECT_CARGO = "Nothing ran: the cargo this check would use is inside this project, where the project's own files can put anything, so ThruLine refused to start it. To run this check, install cargo outside the project folder, then press Look somewhere else and pick that place."

test('D-049: the refusal for a program inside the project names it, says why, and says what would allow it, word for word', () => {
  const doc = { error: 'program_in_project', program: 'cargo', found_at: '~/shop/node_modules/.bin/cargo' }
  assert.equal(M.refusalText('program_in_project', doc), IN_PROJECT_CARGO)
  /* The program named is the one served, wherever it is named. */
  const other = M.refusalText('program_in_project', { ...doc, program: 'pytest' })
  assert.equal(other, IN_PROJECT_CARGO.replaceAll('cargo', 'pytest'))
  /* The control it tells Sam to press is the sheet's own, by the copy file's name for it. */
  assert.ok(IN_PROJECT_CARGO.includes(`press ${R.tool.look_elsewhere} and pick`), R.tool.look_elsewhere)
})

test('D-049: after that refusal the sheet says it, and offers the places outside the project to pick', () => {
  const words = M.refusalText('program_in_project', { error: 'program_in_project', program: 'cargo' })
  const closed = sheet(view(LAST.passed), { refused: 'program_in_project' }, { refusal: words })
  const body = text(closed)
  assert.ok(body.includes(IN_PROJECT_CARGO), body)
  assert.ok(body.includes(R.tool.look_elsewhere), `the sentence says to press ${R.tool.look_elsewhere} and the sheet has no such control: ${body}`)
  const open = text(sheet(view(LAST.passed), { refused: 'program_in_project' }, { refusal: words, showElsewhere: true }))
  assert.ok(open.includes(R.tool.elsewhere_heading), open)
  assert.ok(open.includes('On this machine: ~/.cargo/bin') && open.includes('Has cargo.'), open)
  assert.equal((open.match(/Run it here/g) || []).length >= 1, true, open)
  /* Without that refusal a sheet whose last run passed offers no place to pick, as before. */
  assert.ok(!text(sheet(view(LAST.passed))).includes(R.tool.look_elsewhere))
  assert.ok(!text(sheet(view(LAST.passed), { refused: 'permission_revoked' })).includes(R.tool.look_elsewhere))
})

test('P6-5B: the consent strings come from the copy file (probe)', () => {
  const model = PROBED.runSheetModel(view(LAST.passed, { consent: given }))
  const body = text(PROBED.render({ model }))
  assert.ok(body.includes('probe-take-back') && body.includes('probe-consent-heading'), body)
})
