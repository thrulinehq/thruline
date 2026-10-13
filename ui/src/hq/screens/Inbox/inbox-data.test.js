/* W1-A, HQ Next wave 1: the Inbox's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas word for word; the live
 * path (`data` present) draws only what `data` says and none of the canvas's
 * design annotations. The pixel match against docs/hq-next/reference/Inbox.png
 * is checked by scripts/hq/pixdiff.py, not here. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { parse } from '@babel/parser'

const here = dirname(fileURLToPath(import.meta.url))

/* The screen, bundled for node exactly as the repo's other render tests do:
 * its canvas logic over its template, rendered to static markup with or
 * without `data` (the live path vs the design gallery). */
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
import { DEMO } from './demo.js'
export { DEMO, Logic }
export function render(props, state) {
  const inst = new Logic(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'inbox-data-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))
const clone = (x) => JSON.parse(JSON.stringify(x))

/** Every string literal in template.jsx, with whether it sits under a `v?.isDemo` gate. */
function templateLiterals() {
  const src = readFileSync(join(here, 'template.jsx'), 'utf8')
  const ast = parse(src, { sourceType: 'module', plugins: ['jsx'] })
  const out = []
  const visit = (node, gated) => {
    if (!node || typeof node.type !== 'string') return
    if (node.type === 'ConditionalExpression' && src.slice(node.test.start, node.test.end) === 'v?.isDemo') gated = true
    if (node.type === 'StringLiteral' || node.type === 'JSXText') out.push({ value: node.value, gated })
    for (const k of Object.keys(node)) {
      if (k === 'loc' || k === 'start' || k === 'end') continue
      const c = node[k]
      if (Array.isArray(c)) c.forEach((x) => visit(x, gated))
      else if (c && typeof c === 'object' && typeof c.type === 'string') visit(c, gated)
    }
  }
  visit(ast.program, false)
  return out
}

import { inboxText, fmtBytes, projectLine, threadSentence } from './text.js'

/** Every content string the canvas drew that now arrives from DEMO. */
const CANVAS = ["3 projects · counted by Change","Recording · 2 projects","read 16:10 ↻","2 Changes are your move","2","checkout-web","recording · 4 Changes","demo","recorded 5 Sep · 1 dismissed","notes-cli","recording · nothing yet"," · hook v0.4"," on","Tried again at 16:10. Still not answering, so nothing from Codex is being recorded.","Two things are your move. One is the agent’s.","Counted by Change: the discount fix needs a check and a prompt answered.","Add postcode validation to the address form. Ready for approval, 7c01d9e.","version 7c01d9e","7c01d9e","Add postcode validation to the address form","since 15:48","Approve 7c01d9e. Opens a sheet; nothing is recorded until you confirm.","checkout-web · since 15:48","Rests on 3 of 3 things that count as done, each with a run on "," · 3 of 3","Your turn in Claude Code, 6 minutes. Fix double-applied discount on checkout total. Step 60 has no result yet.","Your turn in Claude Code","Fix double-applied discount on checkout total","6 min","checkout-web · waiting 6 min","rm -rf .pytest_cache && pytest -q","· asks to remove a directory · requested 16:04","step 60 · no result yet","1","Fix double-applied discount on checkout total. Needs a re-check, version e71c5b0. 4 things still need evidence.","version e71c5b0","e71c5b0","since 16:04","checkout-web · since 16:04","4 things still need evidence","checkout/api.py","⑤ RE-CHECKED ×3","Upgrade Stripe SDK to v14. Not proven, version 3e8b1d4. You copied a note for the agent at 16:02.","version 3e8b1d4","3e8b1d4","Upgrade Stripe SDK to v14","since 16:02","checkout-web · since 16:02","② RESTS ON","Claude Code session in checkout-web, recording. Last step recorded at 16:04.","Claude Code session · 16:01 to now","Last step recorded at 16:04","checkout-web · hook v0.4","19 steps · waiting at step 60 · part of Fix double-applied discount on checkout total","3","1 dismissed","1 command that removes files ran","5a91c3e","checkout-web ›"," Round line-item prices to cents","by you, yesterday","0e4b7d2","demo ›"," Seed the demo store with sample carts","by you, 5 Sep"," 1 statement not proven","rm -rf .venv","· Upgrade Stripe SDK to v14 · 15:58","Change · checkout-web · 1 session · Claude Code","· version e71c5b0","4 things still need evidence before e71c5b0 can be approved.","Claude Code said ","Checkout total fixed.","Tests pass.","16:03 · step 52","3 rest on step 46, which ran on ","9f2ad3e"," changed at step 56. No run has included ","checkout/"," for line 3.","What counts as done · v1 · 4 lines","Proof strip, 4 lines","Line 1, needs a re-check","◭","Line 2, needs a re-check","Line 3, in progress","◐","Line 4, the gate, needs a re-check","4","3 need a re-check · 1 in progress · 0 proven on ","② RESTS ON ×1","Three versions. 4b19a0c: a failed run at step 26. 9f2ad3e: a passing run at step 46 and the statement at step 52. e71c5b0, now: checkout/api.py changed at step 56, step 60 has no result yet, and no run has happened on it.","4b19a0c","e71c5b0 · now","26","46","52","56","60","Lit from step 52 back to 46. Hatched where "," changed at step 56; stops at ⑤.","pytest -q tests/test_totals.py checkout/"," on e71c5b0 · reaches all 4 lines · asks once for this project","Approval of e71c5b0 waits on all 4.","Change · checkout-web · session ended 15:48","· 7c01d9e","Ready for approval for version 7c01d9e. It rests on:","3 of 3 things that count as done, each with a run on ","What counts as done · 3 lines","Proof strip, 3 lines","Line 1, proven on 7c01d9e","●","Line 2, proven on 7c01d9e","Line 3, proven on 7c01d9e","3 of 3 proven on ","6 links hold · 3 of 3 lines","Approving records your decision on 7c01d9e only. It does not merge, deploy or tell the agent anything.","Step 60 · command · Fix double-applied discount on checkout total · Claude Code","Your turn in Claude Code · 6 min","Step 60 has no result yet."," was requested at 16:04 and nothing after it is recorded. It removes a directory."," on e71c5b0.","Change · checkout-web · since 16:02","· version 3e8b1d4","You copied a note for the agent at 16:02. No later work has been recorded since.","Claude Code session in checkout-web","Session · part of Fix double-applied discount on checkout total","16:01 · hook v0.4","19","16:04","step 60, which has no result yet","Before 16:01","not recorded; the hook was installed at 16:01","2 Changes are your move · 1 is the agent’s · 1 session recording · read 16:10","ThruLine is keeping 1.4 GB of records on this machine · ","Answer it in Claude Code, where the agent asked. If it runs, its result shows up here, and at ⑤ RE-CHECKED if it covers "]

test('the gallery draws every canvas string from DEMO', () => {
  // the canvas draws some strings only once a row is picked, a section expanded or Codex retried
  const html = [{}, { nothingOpen: true }, { sel: 'post' }, { sel: 'prompt' }, { sel: 'stripe' }, { sel: 'live' }, { codexTried: true }].map((st) => text({}, st)).join('\n')
  const missing = CANVAS.filter((s) => !html.includes(s))
  assert.deepEqual(missing, [])
})

test('the gallery keeps the design annotations; the live path drops every one', () => {
  const gallery = text({})
  for (const a of ['Demo records.', 'Needs engine: no Codex App Server client exists yet.', 'needs engine: prompt capture', 'needs engine: reads “Recent”', 'needs engine: kept in this browser only']) assert.ok(gallery.includes(a), a)
  for (const st of [{}, { nothingOpen: true }, { sel: 'post' }, { sel: 'prompt' }, { sel: 'stripe' }, { sel: 'live' }]) {
    const live = text({ data: clone(M.DEMO) }, st)
    assert.doesNotMatch(live, /needs engine/i, JSON.stringify(st))
    assert.doesNotMatch(live, /Illustrative/, JSON.stringify(st))
    assert.doesNotMatch(live, /Demo records/, JSON.stringify(st))
  }
})

test('the live path draws what data says, not the canvas', () => {
  const data = clone(M.DEMO)
  data.changes.disc.title = 'Stand-in title from the store'
  data.changes.post.version = 'abc1234'
  data.projects[0].name = 'stand-in-project'
  data.storage.bytes = 2_300_000_000
  data.readAt = '09:41'
  const html = text({ data })
  assert.ok(html.includes('Stand-in title from the store'))
  assert.doesNotMatch(html, /Fix double-applied discount on checkout total/)
  assert.ok(html.includes('Approve abc1234'))
  assert.doesNotMatch(html, /7c01d9e/)
  assert.ok(html.includes('stand-in-project'))
  assert.ok(html.includes('ThruLine is keeping 2.3 GB of records on this machine'))
  assert.ok(html.includes('read 09:41'))
})

test('template.jsx holds no demo content outside an isDemo gate', () => {
  const DEMO_WORDS = ['7c01d9e', 'e71c5b0', '3e8b1d4', '9f2ad3e', '4b19a0c', '5a91c3e', '0e4b7d2', 'checkout-web', 'notes-cli', 'checkout/api.py', 'pytest', 'rm -rf', 'Stripe', 'postcode', 'discount', '16:0', '16:1', '15:4', '15:5', '1.4 GB']
  const leaks = templateLiterals().filter((l) => !l.gated && DEMO_WORDS.some((w) => l.value.includes(w)))
  assert.deepEqual(leaks.map((l) => l.value), [])
})

test('every design annotation in template.jsx sits under an isDemo gate', () => {
  const ungated = templateLiterals().filter((l) => !l.gated && /needs engine|illustrative|staged/i.test(l.value))
  assert.deepEqual(ungated.map((l) => l.value), [])
})

test('the counts are counted from the data, by Change and by whose move', () => {
  const data = clone(M.DEMO)
  data.changes.extra = { id: 'extra', title: 'Another Change', project: 'checkout-web', state: 'notProven', move: 'agent', version: '1111111', since: '16:05' }
  const t = inboxText(data)
  assert.equal(t.n.agentMove, '2')
  assert.equal(t.headline, 'Two things are your move. Two are the agent’s.')
  assert.equal(t.footer, '2 Changes are your move · 2 are the agent’s · 1 session recording · read 16:10')
  assert.equal(t.n.yourMove, '2')
})

test('a missing fact reads as nothing, never as undefined, null or NaN', () => {
  const walk = (x, p, bad) => {
    if (typeof x === 'string') { if (/undefined|null|NaN/.test(x)) bad.push(`${p}: ${x}`) } else if (x && typeof x === 'object') for (const k of Object.keys(x)) walk(x[k], `${p}.${k}`, bad)
    return bad
  }
  assert.deepEqual(walk(inboxText({}), '', []), [])
  assert.doesNotMatch(text({ data: { projects: [], changes: {} } }), /undefined|NaN/)
})

test('the small formatters', () => {
  assert.equal(fmtBytes(1_400_000_000), '1.4 GB')
  assert.equal(fmtBytes(5_200_000), '5.2 MB')
  assert.equal(fmtBytes(undefined), null)
  assert.equal(projectLine({ recording: true, changeCount: 1 }), 'recording · 1 Change')
  assert.equal(projectLine({ recording: false, lastRecordedOn: '5 Sep', dismissedCount: 0 }), 'recorded 5 Sep')
  assert.equal(threadSentence([{ version: 'a', now: true, ran: false, events: [] }]), 'One version. a, now: no run has happened on it.')
})

/* ---- W2-A: the Inbox on the real record's shape (data/project.js) ---- */
import { inboxData } from '../../data/project.js'
import { LIST, NOW, PROJECT, VIEW } from '../../data/project-fixture.js'

const liveData = () => inboxData({ projects: LIST, view: VIEW, storage: { bytes: 5_400_000_000 }, now: NOW })

test('live: every thread is drawn, grouped by whose move, with no design notes', () => {
  const html = text({ data: liveData() }, { nothingOpen: true })
  for (const t of VIEW.threads) assert.ok(html.includes(t.title), t.title)
  assert.doesNotMatch(html, /needs engine|Illustrative|Demo records|checkout-web|e71c5b0/i)
  // the group the record has no rows for is not drawn; the others carry the project's counts
  assert.match(html, /aria-label="Waiting on the agent" style="display:none/)
  assert.ok(html.includes('Three things are your move. One is the agent’s.'))
  assert.ok(html.includes(`every thread of work in shop, ${PROJECT.threads} in all`))
})

test('live: a row opens its thread as a Change in this project', () => {
  const html = M.render({ data: liveData() }, { sel: 're1' })
  assert.ok(html.includes('href="#/changes/re1/check?project=p1"'))
  assert.ok(html.includes('href="#/changes/re1?project=p1"'))
  const inst = new M.Logic({ data: liveData() })
  const v = inst.renderVals()
  assert.equal(v.groups.check.length, 2)
  assert.equal(v.groups.check[1].openHref, '#/changes/np1?project=p1')
})

test('live: a Recording row offers Follow live at its own thread address', () => {
  const html = M.render({ data: liveData() }, { sel: 'live-live1' })
  assert.match(html, /href="#\/changes\/live1\?project=p1"[^>]*>Follow live<\/a>/)
})

test('live: a hundred threads are a hundred rows (no cap but the design’s collapse)', () => {
  const view = clone(VIEW)
  view.threads = Array.from({ length: 100 }, (_, i) => Object.assign(clone(VIEW.threads[2]), { id: `t${i}`, title: `Thread number ${i}` }))
  view.project.whose_move = { you: 100, agent: 0, done: 0 }
  const data = inboxData({ projects: LIST, view, now: NOW })
  const v = new M.Logic({ data }).renderVals()
  assert.equal(v.groups.check.length, 100)
  assert.equal(v.t.n.yourMove, '100')
  const html = text({ data })
  assert.ok(html.includes('Thread number 99'))
})

test('live: a quick project row in the sidebar reads "not yet counted", never 0', async () => {
  const { LIST_QUICK } = await import('../../data/project-fixture.js')
  const html = text({ data: inboxData({ projects: LIST_QUICK, view: VIEW, now: NOW }) })
  assert.ok(html.includes('recorded 1 Oct · not yet counted'))
  assert.doesNotMatch(html, /notes<\/span>[^]{0,400}(recording · nothing yet|· 0 )/)
})

test('live: a waiting row opens its thread’s copy sheet, whether or not a tracked Change has been read yet', () => {
  const view = clone(VIEW)
  view.threads[2].told_agent = { last_act: 'copied', at: view.threads[2].last_activity_at, version_id: 'wt-1', act_id: 'cam-1', later_work: false }
  for (const tracked of [{}, { np1: 'chg_own' }, { np1: { id: 'chg_own' } }]) {
    const data = inboxData({ projects: LIST, view, ready: {}, tracked, now: NOW })
    const row = inboxText(data).groups.agent.find((r) => r.id === 'np1')
    assert.equal(row.t.tellHref, '#/changes/np1/check?project=p1')
    const inst = new M.Logic({ data })
    inst.state = { ...inst.state, sel: 'np1' }
    assert.equal(inst.renderVals().sp.tellHref, '#/changes/np1/check?project=p1')
  }
})

test('live: Check it again and Tell the agent carry the row’s thread address, never a tracked Change’s (the address cannot depend on a read that has not landed)', () => {
  const alone = inboxData({ projects: LIST, view: VIEW, now: NOW })
  const tracked = inboxData({ projects: LIST, view: VIEW, tracked: { re1: { id: 'chg_proof', criteria: { version: 'v1', lines: [{ n: 1, state: 'recheck', gate: false }] } } }, now: NOW })
  assert.equal(alone.changes.re1.checkHref, '#/changes/re1/check?project=p1')
  assert.equal(tracked.changes.re1.checkHref, '#/changes/re1/check?project=p1')
  const html = M.render({ data: tracked }, { sel: 're1' })
  assert.ok(html.includes('href="#/changes/re1/check?project=p1"'))
  assert.ok(!html.includes('chg_proof'), 'no link names the tracked Change')
  const inst = new M.Logic({ data: tracked })
  inst.state = { ...inst.state, sel: 're1' }
  const oldWindow = globalThis.window
  globalThis.window = { location: { hash: '#/inbox?project=p1' } }
  try {
    inst.renderVals().sp.copyAct()
    assert.equal(globalThis.window.location.hash, '#/changes/re1/check?project=p1')
  } finally {
    globalThis.window = oldWindow
  }
})

test('live: Tell the agent on a check row opens its real Repair-Loop', () => {
  const data = liveData()
  const oldWindow = globalThis.window
  globalThis.window = { location: { hash: '#/inbox?project=p1' } }
  try {
    const inst = new M.Logic({ data })
    inst.state = { ...inst.state, sel: 're1' }
    inst.renderVals().sp.copyAct()
    assert.equal(globalThis.window.location.hash, '#/changes/re1/check?project=p1')
  } finally {
    globalThis.window = oldWindow
  }
})

test('live: an unanswered command carries its masked command and numbered step into the prompt', () => {
  const view = clone(VIEW)
  view.threads.find((t) => t.id === 'ask1').waiting_command = { command: 'rm -rf .pytest_cache && pytest -q', step: 3, at: view.threads.find((t) => t.id === 'ask1').last_activity_at }
  const data = inboxData({ projects: LIST, view, now: NOW })
  const prompt = inboxText(data).groups.you.find((r) => r.kind === 'prompt')
  assert.equal(prompt.t.command, 'rm -rf .pytest_cache && pytest -q')
  assert.equal(prompt.t.pvNoResult, 'Step 3 has no result yet.')
  assert.match(prompt.t.asks, /asks to remove a directory/)
})

test('live: capture health drives the Capture card and its retry action', () => {
  const data = inboxData({ projects: LIST, view: VIEW, capture: { codex: { answering: false } }, now: NOW })
  let reads = 0
  const retry = () => { reads += 1 }
  const inst = new M.Logic({ data, retryCapture: retry })
  const vals = inst.renderVals()
  assert.equal(vals.t.captureShow, true)
  vals.codex.retry()
  assert.equal(reads, 1)
  assert.match(M.render({ data, retryCapture: retry }), /Try again/)
})

test('live: the read stamp re-reads the project when pressed', () => {
  const reload = () => {}
  const inst = new M.Logic({ data: liveData(), reloadInbox: reload })
  assert.equal(inst.renderVals().readAgain, reload)
  assert.match(M.render({ data: liveData(), reloadInbox: reload }), /<button[^>]*>read 16:10 ↻<\/button>/)
})

test('live: a claim with served proof and accepted criteria fills its preview', () => {
  const view = clone(VIEW)
  view.threads[1].latest_claim = { agent: 'Claude Code', text: 'Checkout total fixed.', at: view.threads[1].last_activity_at, step: 4 }
  view.threads[1].proof.last_test = { command: 'pytest -q', outcome: 'succeeded' }
  const tracked = { re1: { id: 'chg_proof', criteria: { version: 'v1', lines: [{ n: 1, state: 'recheck', gate: false }] } } }
  const data = inboxData({ projects: LIST, view, tracked, now: NOW })
  const row = inboxText(data).groups.check.find((r) => r.id === 're1')
  assert.equal(row.t.claim, true)
  assert.equal(row.t.quote0, 'Checkout total fixed.')
  assert.match(row.t.saidAt, /step 4/)
  assert.equal(row.t.criteria, true)
  assert.equal(row.t.checkCommand, 'pytest -q')
})

test('live: the quote says when it was said as HH:MM · step N, even when that was on an earlier day', () => {
  const clock = (d) => `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
  for (const hoursAgo of [0.5, 26, 24 * 40]) {
    const said = new Date(NOW.getTime() - hoursAgo * 3600_000)
    const view = clone(VIEW)
    view.threads[1].latest_claim = { agent: 'Claude Code', text: 'Checkout total fixed.', at: said.toISOString(), step: 4 }
    const row = inboxText(inboxData({ projects: LIST, view, now: NOW })).groups.check.find((r) => r.id === 're1')
    assert.equal(row.t.saidAt, `${clock(said)} · step 4`, `${hoursAgo} h ago`)
  }
})

test('live: a served version trace draws the claim preview thread', () => {
  const view = clone(VIEW)
  view.threads[1].version_thread = [{ version: 'a'.repeat(64), now: true, events: [{ kind: 'claim', step: 4 }] }]
  const row = inboxText(inboxData({ projects: LIST, view, now: NOW })).groups.check.find((r) => r.id === 're1')
  assert.equal(row.t.hasThread, true)
  assert.match(row.t.threadAria, /statement at step 4/)
})

test('live: a hidden row is counted and says hidden by you until the record changes', () => {
  const view = clone(VIEW)
  const hides = { project: 'p1', rows: [{ thread_id: 'np1', row_kind: 'thread', row_sha256: 'a'.repeat(64), hidden: true, actor: 'Machine account', recorded_at: NOW.toISOString() }] }
  const data = inboxData({ projects: LIST, view, hides, now: NOW })
  const row = inboxText(data).nn.items.find((item) => item.kind === 'dismissed')
  assert.match(row.by, /hidden by you until the record changes/)
})

test('live: an approval for the current version appears as an approved Nothing needed item', () => {
  const view = clone(VIEW)
  const done = view.threads.find((t) => t.id === 'done1')
  const tracked = { done1: { id: 'chg_done', approved: { version: done.current_version_id, actor: 'you', at: NOW.toISOString() } } }
  const data = inboxData({ projects: LIST, view, tracked, now: NOW })
  const item = inboxText(data).nn.items.find((i) => i.title && i.title.includes('Thread done1'))
  assert.equal(item.kind, 'approved')
  assert.equal(item.word, 'Approved')
  assert.match(item.by, /by you, 16:10/)
})

test('live: an approval current for the Change’s own target is Approved, though the thread’s version id is another id space', () => {
  const view = clone(VIEW)
  const done = view.threads.find((t) => t.id === 'done1')
  done.current_version_id = 'wt-343854182f687eee'
  const TARGET = '01M4R2GNG5VFDFC1MK93PE9QWM'
  const approved = (over) => ({ done1: { id: 'chg_done', approved: { version: TARGET, actor: 'you', at: NOW.toISOString(), ...over } } })
  const item = (tracked) => inboxText(inboxData({ projects: LIST, view, tracked, now: NOW })).nn.items.find((i) => i.title && i.title.includes('Thread done1'))
  const yes = item(approved({ current: true }))
  assert.equal(yes.kind, 'approved')
  assert.equal(yes.word, 'Approved')
  assert.equal(yes.glyph, '◆')
  assert.equal(yes.version, TARGET, 'it names the version the Approve row named')
  assert.match(yes.by, /by you, 16:10/)
  // an approval the Change has moved past is not Approved, whatever the thread says
  assert.equal(item(approved({ current: false, version: done.current_version_id })).kind, 'done')
  // no verdict on the Change's target (its readiness was not read): the thread's own version decides, as before
  assert.equal(item(approved({ version: done.current_version_id })).kind, 'approved')
  assert.equal(item(approved({})).kind, 'done')
})

test('live: a recorded file-removal command is counted but never judged', () => {
  const view = clone(VIEW)
  view.threads.find((t) => t.id === 'done1').file_removals = [{ command: 'rm -rf .venv', at: NOW.toISOString() }]
  const data = inboxData({ projects: LIST, view, now: NOW })
  const t = inboxText(data)
  assert.equal(t.nn.destructive, '1 command that removes files ran')
  assert.equal(t.nn.cmds[0].command, 'rm -rf .venv')
  assert.match(M.render({ data }, { nothingOpen: true }), /recorded, not judged/)
})
