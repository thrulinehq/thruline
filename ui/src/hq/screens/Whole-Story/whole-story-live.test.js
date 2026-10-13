/* W2-E, HQ Next wave 2: the whole story from the ONE Change snapshot.
 * `from-change.js` reads the snapshot (data/change.js, shaped as
 * Change-Brief's DEMO) and the tracked Change's own model; the page then draws
 * only that, with loops for findings, decisions and gaps, no design notes and
 * no +00:00 offset. Its export is pinned in export.test.js and export-live.test.js. */
import { wholeStoryFrom, MAX_RUNS } from './from-change.js'
import { DEMO as BRIEF } from '../Change-Brief/demo.js'
import { changeSnapshot } from '../../data/change.js'
import { objectionsOf } from '../../data/objections.js'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
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
  const file = join(await mkdtemp(join(tmpdir(), 'w2e-ws-')), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const text = (props, state) => unescape(M.render(props, state))

const S1 = BRIEF.states.s1
const READY = BRIEF.states.ready

test('the statement the page leads with is the snapshot’s own, with its chain as the snapshot states it', () => {
  const d = wholeStoryFrom(S1, null)
  assert.equal(d.s1.quote, 'Tests pass.')
  assert.equal(d.s1.verdict, 'not_proven')
  assert.equal(d.s1.stopsAt, 5)
  assert.deepEqual(d.s1.since, { file: 'checkout/api.py', step: 56 })
  assert.equal(d.s1.rested.step, 46)
  assert.equal(d.s2.quote, 'Checkout total fixed.')
  const r = wholeStoryFrom(READY, null)
  assert.equal(r.s1.verdict, 'proven')
  assert.equal(r.s1.rested.step, 62)
})

test('the page leads with the statement the snapshot names, not one it picks itself', () => {
  const later = { ...S1.statements[0], id: 'later', text: 'Every test passes now.', step: 70, proof: { ...S1.statements[0].proof, verdict: 'proven', stopsAt: null } }
  const snap = { ...S1, statements: [S1.statements[0], S1.statements[1], later], brief: { statementId: S1.statements[0].id } }
  assert.equal(wholeStoryFrom(snap, null).s1.quote, 'Tests pass.')
})

test('with nothing tracked, what counts as done is not accepted, and there is no decision', () => {
  const d = wholeStoryFrom(S1, null)
  assert.equal(d.criteria.accepted, false)
  assert.deepEqual(d.decisions, [])
  assert.equal(d.approval, null)
  assert.ok(d.gaps.some((g) => g.kind === 'criteria_not_accepted'))
  const html = text({ data: d })
  assert.ok(html.includes('Not approved · '))
  assert.ok(html.includes('What counts as done is not accepted yet'))
  assert.ok(html.includes('No decision has been recorded for this work.'))
})

test('findings, decisions and gaps are loops: every one the data holds is drawn', () => {
  const d = wholeStoryFrom(S1, null)
  const many = { ...d, findings: Array.from({ length: 7 }, (_, i) => ({ text: 'Finding number ' + (i + 1) + '.', where: 'step ' + i, settled: i === 0 })) }
  const html = text({ data: many })
  for (let i = 1; i <= 7; i++) assert.ok(html.includes('Finding number ' + i + '.'), 'finding ' + i)
  assert.ok(html.includes('7 found · 1 settled · 6 open'))
  const tracked = {
    change: {
      id: 'chg_1', accepted_scope: { acceptance: { accepted_at: '2026-09-26T10:36:00Z' } },
      decisions: {
        standing: { id: 'a2', target_revision: 'e71c5b0aaaa', actor: 'Sam', decided_at: '2026-09-26T10:40:00Z', scope_version: 1 },
        history: [
          { id: 'a1', act: 'approved', target_revision: '9f2ad3eaaaa', actor: 'Sam', decided_at: '2026-09-26T10:37:00Z', scope_version: 1, record_path: '.thruline/decisions/9f2ad3e.json' },
          { id: 'w1', act: 'withdrawn', withdraws: 'a1', target_revision: '9f2ad3eaaaa', actor: 'Sam', decided_at: '2026-09-26T10:38:00Z' },
          { id: 'a2', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'Sam', decided_at: '2026-09-26T10:40:00Z', scope_version: 1, record_path: '.thruline/decisions/e71c5b0.json' },
        ],
      },
    },
    hq: { criteria: [{ n: 1, text: 'Totals are right', word: 'Proven' }] },
  }
  const t = wholeStoryFrom(READY, tracked)
  assert.equal(t.decisions.length, 4)
  const th = text({ data: t })
  assert.ok(th.includes('Approval of 9f2ad3e withdrawn by Sam.'))
  assert.ok(th.includes('.thruline/decisions/e71c5b0.json'))
  assert.ok(th.includes('Totals are right'))
  assert.ok(th.includes('Approved on '))
  assert.ok(th.includes('2026-09-26T10:40:00Z'), 'the file preview states the approval in UTC')
})

test('the live page carries no design note, no fixed offset and no trial, and its export works (C30: never "Not available in this version")', () => {
  const d = wholeStoryFrom(READY, null)
  for (const fmt of ['md', 'html', 'json']) {
    const html = text({ data: d }, { fmt })
    for (const never of ['Design notes', 'NEEDS ENGINE', 'ILLUSTRATIVE', '+00:00', 'UTC+00:00', 'IST', 'trial', 'detector v2', 'Not available in this version']) {
      assert.ok(!html.includes(never), fmt + ' shows ' + never)
    }
    assert.ok(html.includes('Made in this browser from exactly the text above. Nothing is sent anywhere.'))
    assert.ok(html.includes('Download .' + fmt))
  }
})

test('Record integrity states the result of the per-Change chain check and its exact scope', () => {
  const d = wholeStoryFrom(READY, null)
  const base = { schema_version: 1, checked_at: '2026-10-09T07:00:00Z', sessions_checked: 2, links_checked: 11 }
  const good = text({ data: d, integrity: { status: 'ready', doc: { ...base, state: 'INTACT' } } })
  assert.ok(good.includes('The recorded chain adds up.'))
  assert.ok(good.includes('11 recorded links in 2 confirmed sessions'))
  assert.ok(good.includes('did not re-read the agent’s source files or verify this saved export'))
  const bad = text({ data: d, integrity: { status: 'ready', doc: { ...base, state: 'MISMATCH' } } })
  assert.ok(bad.includes('The recorded chain does not add up.'))
  const unavailable = text({ data: d, integrity: { status: 'failed' } })
  assert.ok(unavailable.includes('Could not check the recorded chain'))
  assert.ok(!unavailable.includes('The recorded chain adds up.'))
})

test('the runs table lists at most the last MAX_RUNS test runs and says how many it left out', () => {
  const runs = Array.from({ length: MAX_RUNS + 5 }, (_, i) => ({ id: 'r' + i, step: i + 1, kind: 'test', command: 'pytest -q', version: 'e71c5b0', status: 'finished', outcome: 'succeeded', passed: 1, failed: 0, time: '10:00' }))
  const d = wholeStoryFrom({ ...S1, runs }, null)
  assert.equal(d.runs.length, MAX_RUNS)
  assert.equal(d.testsTotal, MAX_RUNS + 5)
  assert.ok(text({ data: d }).includes('These are the last ' + MAX_RUNS + ' of ' + (MAX_RUNS + 5) + ' test runs.'))
})

test('it reads the real snapshot: a ThreadView through data/change.js', () => {
  const view = {
    project: { id: 'p1', name: 'shop', root: '/w/shop' },
    thread: { id: 'abcdef0123456789', title: 'Fix the cart', whose_move: 'you', proof: { edited_after_last_pass: [] }, current_version_id: 'wt-2' },
    prompts: [{ event_id: 'e1', session_id: 's1', ts: '2026-10-06T10:00:00Z', text: 'fix the cart', work: true, raw_file: 'f', raw_line: 1 }],
    edits: [{ id: 'ed1', session_id: 's1', ts: '2026-10-06T10:01:00Z', file: '/w/shop/cart.py', lines_added: 3, lines_removed: 1, ok: true, version_id: 'wt-2' }],
    commands: [{ id: 'c1', session_id: 's1', ts: '2026-10-06T10:02:00Z', command: 'pytest -q', outcome: 'succeeded', is_test: true, tally: { passed: 4, failed: 0 }, version_id: 'wt-2' }],
    claims: [{ id: 'k1#0', event_id: 'k1', session_id: 's1', ts: '2026-10-06T10:03:00Z', text: 'All 4 tests pass.', kind: 'tests_pass', has_verifier: true, rests_on_command_id: 'c1', version_id: 'wt-2' }],
    versions: [{ id: 'wt-2', committed: false, session_id: 's1' }],
    sessions: [{ id: 's1', source: 'claude_code', origin: 'person', started_at: '2026-10-06T10:00:00Z' }],
    masking: { masked: 0, families: [], complete: true },
  }
  const snap = changeSnapshot(view, null, Date.parse('2026-10-06T10:05:00Z'))
  const d = wholeStoryFrom(snap, null)
  assert.equal(d.change.title, 'Fix the cart')
  assert.equal(d.s1.quote, 'All 4 tests pass.')
  assert.equal(d.s1.verdict, snap.statements[0].proof.verdict)
  assert.equal(d.runs.length, 1)
  const html = text({ data: d })
  assert.ok(html.includes('Fix the cart'))
  assert.ok(html.includes('4 passed, 0 failed'))
  assert.ok(html.includes('1 session · Claude Code'))
})

/* C30 R-LEDGER: the notes people kept on the Change are listed, with who and
 * when, and are words only: no count on the page moves because of them. */
test('the notes the record holds are listed with who and when, and moved nothing the page says', () => {
  const notes = [
    { id: 'cn_1', who: 'alex', at: '2026-10-07T11:30:00Z', when: '2026-10-07 11:30', body: 'Read it twice; fine by me.', decisionId: null },
    { id: 'cn_2', who: 'sam', at: '2026-10-07T11:45:00Z', when: '2026-10-07 11:45', body: 'The agent changed the tests after I looked.', decisionId: 'cd_1' },
  ]
  const without = wholeStoryFrom({ ...READY, notes: [] }, null)
  const withNotes = wholeStoryFrom({ ...READY, notes }, null)
  assert.deepEqual(withNotes.notes, notes)
  assert.deepEqual(without.notes, [])
  /* nothing but the notes differs: every count and verdict is the same */
  const { notes: _a, ...restWith } = withNotes
  const { notes: _b, ...restWithout } = without
  assert.deepEqual(restWith, restWithout)
  const html = text({ data: withNotes })
  assert.ok(html.includes('Notes'))
  assert.ok(html.includes('Read it twice; fine by me.') && html.includes('alex · 2026-10-07 11:30'))
  assert.ok(html.includes('The agent changed the tests after I looked.'))
  assert.ok(html.includes('sam · 2026-10-07 11:45 · kept with an approval or a withdrawal'))
  /* no notes, no section: the page is what it was */
  const plain = text({ data: without })
  assert.ok(!plain.includes('words people kept'))
  assert.ok(!text({}).includes('words people kept'), 'the gallery draws none')
})

/* C30 R-LEDGER: the statements people said were wrong are listed with the verdict each
 * already had, and are words only: no count and no verdict on the page moves. */
test('the statements said to be wrong are listed beside the verdict they already had, and moved nothing the page says', () => {
  const served = [
    { id: 'obj_1', actor: 'sam', at: '2026-10-07T11:30:00Z', reason: 'The test never ran.', matches_sentence: true },
    { id: 'obj_2', actor: 'robin', at: '2026-10-07T11:45:00Z', reason: 'I meant the older sentence.', matches_sentence: false },
  ]
  const objected = { ...READY, statements: READY.statements.map((s, i) => (i === 0 ? { ...s, objections: objectionsOf(served) } : s)) }
  const without = wholeStoryFrom(READY, null)
  const withIt = wholeStoryFrom(objected, null)
  assert.equal(withIt.objected.length, 1)
  assert.equal(withIt.objected[0].verdict, READY.statements[0].proof.verdict, 'the verdict it already had, carried through')
  assert.deepEqual(without.objected, [])
  /* nothing but the objected list differs: every count and verdict is the same */
  const { objected: _a, ...restWith } = withIt
  const { objected: _b, ...restWithout } = without
  assert.deepEqual(restWith, restWithout)
  const html = text({ data: withIt }).replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ')
  assert.ok(html.includes('What you said was wrong'))
  assert.ok(html.includes('You said this is wrong: The test never ran.'))
  assert.ok(html.includes('sam · 20'), 'who and when')
  assert.ok(html.includes('You said an earlier wording of this was wrong: I meant the older sentence.'))
  assert.ok(html.includes('The sentence changed since'))
  assert.ok(html.includes('What ThruLine says about it, unchanged: Proven'))
  assert.ok(html.includes('Your objection is kept beside this statement. It does not change what ThruLine says about it.'))
  /* nobody objected, no section: the page is what it was */
  assert.ok(!text({ data: without }).includes('What you said was wrong'))
  assert.ok(!text({}).includes('What you said was wrong'), 'the gallery draws none')
})

test('Whole-Story reaches no write address', () => {
  for (const f of ['index.jsx', 'from-change.js', 'logic.js']) {
    const src = readFileSync(join(here, f), 'utf8')
    assert.ok(!/method:\s*['"](POST|PUT|PATCH|DELETE)/i.test(src), f + ' writes')
  }
})

test('the live page claims nothing about the saved file that no command checks: no "thruline verify <file>", no "unchanged since export"', () => {
  const live = text({ data: wholeStoryFrom(READY, null) })
  assert.ok(!live.includes('thruline verify <file>'), '`thruline verify` takes no file')
  assert.ok(!live.includes('internally consistent and unchanged since export'))
  assert.ok(live.includes('Nothing in this file proves it is unchanged since it was saved.'))
  // the gallery keeps the canvas's words
  const gallery = text({})
  assert.ok(gallery.includes('thruline verify <file>'))
  assert.ok(gallery.includes('This file is internally consistent and unchanged since export.'))
})
