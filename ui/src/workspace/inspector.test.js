import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

// Exercise the actual inspector and navigation methods. JSX/CSS are bundled
// without a browser, and every source relationship is explicit in the fixture.
const temporary = await mkdtemp(join(tmpdir(), 'thruline-inspector-'))
after(() => rm(temporary, { recursive: true, force: true }))
const bundle = await build({
  entryPoints: [fileURLToPath(new URL('./shell.jsx', import.meta.url))],
  bundle: true, write: false, format: 'esm', platform: 'node',
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const compiled = join(temporary, 'workspace.mjs')
await writeFile(compiled, bundle.outputFiles[0].text)
const { Workspace } = await import(pathToFileURL(compiled).href)

function workspace() {
  const session = {
    id: 'session', key: 'session', capture: {}, narr: {}, ended: '2026-09-08T22:59:00Z',
    finalTree: 'tree-final',
    events: [{ seq: 10, kind: 'session_start', text: 'Started' }, { seq: 55, kind: 'user_prompt', text: 'An unrelated prompt' }],
    claims: [
      { id: 'claim-one', node: 'clm', text: 'First claim', kind: 'tests_pass', word: 'proven', verdict: 'supported', evidence: ['check-one'], findings: [] },
      { id: 'claim-two', node: 'clm2', text: 'Second claim', kind: 'tests_pass', word: 'not_proven', verdict: 'unsupported', eventId: 'claim-event-two', scope: 'file:second.js', evidence: ['check-two'], findings: [], reasons: ['Second reason'] },
    ],
    checks: [
      { id: 'check-one', node: 'chk', command: 'first-test', category: 'test', runner: 'first', exit: 0, inputTree: 'tree-final' },
      { id: 'check-two', node: 'chk2', command: 'second-test', category: 'test', runner: 'second', exit: 1, inputTree: 'tree-old', parsed: { passed: 2, failed: 1 } },
    ],
    promises: [
      { id: 'promise-one', node: 'prm', text: 'First promise', status: 'open', word: 'loose', eventId: 'promise-event-one' },
      { id: 'promise-two', node: 'prm2', text: 'Second promise', status: 'fulfilled', eventId: 'promise-event-two' },
    ],
    asks: [
      { id: 'ask-one', node: 'ask', text: 'First request', kind: 'feature', status: 'untouched', word: 'loose', eventId: 'ask-event-one' },
      { id: 'ask-two', node: 'ask2', text: 'Second request', kind: 'constraint', status: 'held', word: 'kept', eventId: 'ask-event-two', offset: [4, 20] },
    ],
  }
  session.nodes = ['claim', 'check', 'promise', 'ask'].flatMap((kind) => session[{ claim: 'claims', check: 'checks', promise: 'promises', ask: 'asks' }[kind]].map((row) => ({ id: row.node, objId: row.id, objType: kind, title: row.text || row.command })))
  session.nodes.push({ id: 'ev55', objType: 'event', seq: 55, title: 'An unrelated prompt' })
  const w = Object.create(Workspace.prototype)
  w.props = { commentary: false }
  w.state = { session: 'session', sel: null, rep: 'review', doc: 'trace', drawer: 'claims', panels: { right: false, bottom: true }, expanded: {}, docs: [], compare: { a: 'session', b: 'session' }, corpus: { order: ['session', 'other'], sessions: { session, other: { key: 'other' } } } }
  w.S = () => session
  w.reg = () => 'mgr'
  w.nodeById = (id) => session.nodes.find((node) => node.id === id)
  w.dbPath = () => '/record.sqlite'
  w.stopReplay = () => {}
  w.centerOn = (id) => { w.centered = id }
  w.setState = (update, then) => {
    w.state = { ...w.state, ...(typeof update === 'function' ? update(w.state) : update) }
    if (then) then()
  }
  return { w, session }
}

const row = (rows, key) => rows.find((entry) => entry.k === key)?.v

test('second claim resolves its own words and explicitly referenced check', () => {
  const { w } = workspace()
  const view = w.inspector('clm2')
  assert.equal(view.id, 'claim-two')
  assert.equal(view.title, '“Second claim”')
  assert.equal(row(view.details, 'Reasons'), 'Second reason')
  assert.match(row(view.evidence, 'The check'), /check-two.*second.*exit 1/)
  assert.doesNotMatch(JSON.stringify(view), /First claim|first-test|check-one|about everything/)
  assert.match(row(view.history, 'Asserted'), /claim-event-two/)
  assert.doesNotMatch(row(view.history, 'Asserted'), /22:59|seq 55/)
  assert.equal(JSON.parse(view.raw).id, 'claim-two')
})

test('a claim without a referenced check never borrows the first check', () => {
  const { w, session } = workspace()
  session.claims[1].evidence = []
  const view = w.inspector('clm2')
  assert.doesNotMatch(JSON.stringify(view.evidence), /check-one|first-test|check-two|second-test/)
})

test('second fulfilled promise uses its own identity and does not invent missing later work', () => {
  const { w } = workspace()
  const view = w.inspector('prm2')
  assert.equal(view.id, 'promise-two')
  assert.equal(view.title, '“Second promise”')
  assert.equal(row(view.details, 'Status'), 'fulfilled')
  assert.match(row(view.history, 'Opened'), /promise-event-two/)
  assert.doesNotMatch(JSON.stringify(view), /First promise|promise-event-one|Loose thread|none in this repository|22:59|seq 55/)
  assert.equal(JSON.parse(view.raw).id, 'promise-two')
})

test('second held request keeps its recorded status and exact source, without a guessed prompt position', () => {
  const { w } = workspace()
  const view = w.inspector('ask2')
  assert.equal(view.id, 'ask-two')
  assert.equal(view.title, '“Second request”')
  assert.equal(view.wordSlug, 'kept')
  assert.match(row(view.details, 'Source position'), /ask-event-two.*characters 4–20/)
  assert.doesNotMatch(JSON.stringify(view), /First request|ask-event-one|Loose thread|prompt 1|seq 55|no later object/)
})

test('second check displays its own outcome without inventing scope or importing another claim', () => {
  const { w } = workspace()
  const view = w.inspector('chk2')
  assert.equal(view.id, 'check-two')
  assert.equal(view.title, 'second-test')
  assert.equal(row(view.details, 'Exit code'), '1')
  assert.match(row(view.details, 'Binding'), /differs/)
  assert.equal(row(view.history, 'Referenced by claims'), 'claim-two')
  assert.doesNotMatch(JSON.stringify(view), /undefined|first-test|check-one|whole suite|subset of the suite/)
})

test('source rows may match recorded id, and missing rows never fall back to another item', () => {
  const { w, session } = workspace()
  session.claims[1].node = undefined
  assert.equal(w.inspector('clm2').id, 'claim-two')
  session.claims.pop()
  const view = w.inspector('clm2')
  assert.match(view.sentences[0].text, /no matching source row/)
  assert.doesNotMatch(JSON.stringify(view), /First claim/)
})

test('an unpositioned request cannot assign its event id to the second session event', () => {
  const { w } = workspace()
  const view = w.inspector('ev55')
  assert.equal(row(view.details, 'Event id'), 'event id not exported for this row')
})

test('same-detector findings retain their own claim and never acquire evidence from a shared path', () => {
  const { w, session } = workspace()
  session.claims[0].findings = [{ node: 'f_assertion_removed', key: 'assertion_removed', severity: 'warn', detail: 'First finding', path: 'shared.js' }]
  session.claims[1].findings = [{ node: 'f_assertion_removed_clm2', key: 'assertion_removed', severity: 'fail', detail: 'Second finding', path: 'shared.js' }]
  session.nodes.push(
    { id: 'f_assertion_removed', objType: 'finding', finding: 'assertion_removed', title: 'First finding' },
    { id: 'f_assertion_removed_clm2', objType: 'finding', finding: 'assertion_removed', title: 'Second finding' },
    { id: 'ev999', objType: 'event', seq: 999, file: 'shared.js', title: 'Unrelated change to shared.js' },
  )
  const view = w.inspector('f_assertion_removed_clm2')
  assert.equal(view.id, 'f_assertion_removed_clm2')
  assert.equal(view.title, 'assertion_removed')
  assert.equal(row(view.details, 'On claim'), 'claim-two')
  assert.equal(row(view.details, 'Recorded detail'), 'Second finding')
  assert.equal(row(view.evidence, 'Claim evidence references'), 'check-two')
  assert.match(row(view.details, 'Related check'), /direct check reference is not included/)
  assert.match(row(view.history, 'Source position'), /event position is not included/)
  assert.doesNotMatch(JSON.stringify(view), /First finding|First claim|claim-one|check-one|seq 999|beside the recorded change/)
  assert.equal(JSON.parse(view.raw).claim_id, 'claim-two')
  assert.equal(JSON.parse(view.raw).sequence, null)
})

test('selecting a source from review reveals the graph and inspector', () => {
  const { w } = workspace()
  w.select('prm2', { center: true, doc: true })
  assert.equal(w.state.sel, 'prm2')
  assert.equal(w.state.rep, 'graph')
  assert.equal(w.state.doc, 'trace')
  assert.equal(w.state.drawer, null)
  assert.equal(w.state.panels.right, true)
  assert.equal(w.state.panels.bottom, false)
  assert.equal(w.centered, 'prm2')
})

test('opening comparison chooses a distinct loaded session and preserves an existing distinct pair', () => {
  const { w } = workspace()
  w.openDoc('compare')
  assert.deepEqual(w.state.compare, { a: 'session', b: 'other' })
  w.state.corpus.order.push('third')
  w.openDoc('compare')
  assert.deepEqual(w.state.compare, { a: 'session', b: 'other' })
})

test('A3-NOTES-NEVER-OPEN · the findings count is typed: fail and warn are findings, info are notes, and an info-only claim counts one note', () => {
  const { w, session } = workspace()
  session.claims[0].findings = [
    { node: 'f_a', key: 'assertion_removed', severity: 'fail', detail: 'a' },
    { node: 'f_b', key: 'coverage_not_measured', severity: 'info', detail: 'b' },
    { node: 'f_c', key: 'skip_marker_added', severity: 'warn', detail: 'c' },
  ]
  const mixed = w.inspector('clm')
  assert.deepEqual(mixed.findingsCount, { findings: 2, notes: 1 })
  assert.deepEqual(mixed.findings.map((f) => f.key), ['assertion_removed', 'skip_marker_added'])
  assert.deepEqual(mixed.notes.map((f) => f.key), ['coverage_not_measured'])
  session.claims[0].findings = [{ node: 'f_b', key: 'coverage_not_measured', severity: 'info', detail: 'b' }]
  const infoOnly = w.inspector('clm')
  assert.deepEqual(infoOnly.findingsCount, { findings: 0, notes: 1 })
  assert.equal(infoOnly.hasFindings, true)
  assert.deepEqual(infoOnly.findings, [])
})
