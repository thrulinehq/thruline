/* C30 R-ROUTES "Step view": every receipt chip is a way into the step it names.
 *
 *  - Every-Statement 20: Said / Run / Edit / Version / How decided chips open
 *    the step they name; "Accepted" names a version of what counts as done,
 *    which sits at no step, so it is text.
 *  - What-Changed 11: the record chips and each hunk's "Edit step …›" open
 *    their step (they used to open the Brief).
 *  - Whole-Story 4: "Run step N ›" opens step N.
 *  - Change-Brief 8: the plates' record chips and the How-decided list open
 *    their step; a version recorded at no step draws no chip at all.
 *  - Repair-Loop: the run chip opens the run's step; a check ThruLine ran
 *    has no step, so it never links to one.
 *
 * Every chip's address is `href('step', …)` for its step, with the project, and
 * is absent when the step is. The design gallery draws the canvas's own
 * element, unchanged. Every record is synthetic (step-fixture.js). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { thread, NOW, TID } from './step-fixture.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document

const here = dirname(fileURLToPath(import.meta.url))
const SCREENS = ['Change-Brief', 'Every-Statement', 'What-Changed', 'Whole-Story', 'Repair-Loop']

const M = await (async () => {
  const imports = SCREENS.map((n, i) => `import T${i} from '../${n}/template.jsx'\nimport L${i} from '../${n}/logic.js'`).join('\n')
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as R from '../../routes.js'
import * as SC from '../../screen.jsx'
import { designLink } from '../../artboards.jsx'
import { DcScreen } from '../../dc/runtime.jsx'
import { withFrame } from '../../data/project.js'
import { changeSnapshot } from '../../data/change.js'
import { everyStatementFromChange } from '../Every-Statement/from-change.js'
import { whatChangedFromChange } from '../What-Changed/from-change.js'
import { wholeStoryFrom } from '../Whole-Story/from-change.js'
import { selectRepairLoop } from '../Repair-Loop/from-change.js'
${imports}
const S = { ${SCREENS.map((n, i) => `'${n}': [T${i}, L${i}]`).join(', ')} }
export { R, SC, changeSnapshot, everyStatementFromChange, whatChangedFromChange, wholeStoryFrom, selectRepairLoop }
export function vals(name, props, state) {
  const L = withFrame(S[name][1])
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}
export function render(name, props, link, state) {
  return renderToStaticMarkup(React.createElement(S[name][0], { v: vals(name, props, state), link }))
}
export function renderDesign(name) {
  return renderToStaticMarkup(React.createElement(DcScreen, { Logic: S[name][1], Template: S[name][0], props: {}, link: designLink }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-step-chips-')), 'chips.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const R = M.R

const snap = (o) => M.changeSnapshot(thread(o), null, NOW, { storageBytes: 2.1e9 })
const unescape = (x) => x.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const KEY = { 'Change-Brief': 'brief', 'Every-Statement': 'statements', 'What-Changed': 'changed', 'Whole-Story': 'story', 'Repair-Loop': 'check' }
/* the address names the Change and not its project: the chips must carry the snapshot's */
const atOf = (name) => ({ screen: KEY[name], change: TID })
const linkOf = (name, s) => R.linker(M.SC.linkAt(atOf(name), M.SC.projectOf(s)))
const NOW_TL = NOW
function dataOf(name, s) {
  switch (name) {
    case 'Change-Brief': return s
    case 'Every-Statement': return M.everyStatementFromChange(s)
    case 'What-Changed': return M.whatChangedFromChange(s)
    case 'Whole-Story': return M.wholeStoryFrom(s, null)
    case 'Repair-Loop': return M.selectRepairLoop(s, null, { now: NOW_TL })
    default: throw new Error(name)
  }
}
const live = (name, { s = snap(), data, state } = {}) =>
  unescape(M.render(name, { data: data || dataOf(name, s), frame: {}, at: atOf(name) }, linkOf(name, s), state))
const stepHref = (n) => R.href('step', { change: TID, step: n, project: 'p1' })
const STEP_ADDRESS = /^#\/changes\/[^/]+\/step\/\d+/

/** Every anchor, button and span chip in the markup: tag, href, text. */
function elements(html, re) {
  const out = []
  let m
  while ((m = re.exec(html))) {
    const attr = (k) => { const x = m[2].match(new RegExp('\\b' + k + '="([^"]*)"')); return x ? x[1] : null }
    out.push({ tag: m[1], href: attr('href'), cls: attr('class') || '', text: m[3].replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim() })
  }
  return out
}
const anchors = (html) => elements(html, /<(a)\b([^>]*)>([\s\S]*?)<\/a>/g)
const stepLinks = (html) => anchors(html).filter((a) => a.href && STEP_ADDRESS.test(a.href))
const stepOf = (a) => R.parse(a.href).step

/* ======================= Every-Statement 20 ======================= */

test('Every-Statement 20 · a moved proof: Said, Run, Edit and How decided open the steps they name', () => {
  const s = snap({ moved: true })
  const html = live('Every-Statement', { s })
  const chips = anchors(html).filter((a) => /^(Said|Run|Edit|Version|How this was decided)\b/.test(a.text))
  const by = (kind) => chips.filter((c) => c.text.startsWith(kind))
  assert.deepEqual(by('Said').map((c) => c.href), [stepHref(5)], 'the statement’s own step')
  assert.deepEqual(by('Run').map((c) => c.href), [stepHref(3)], 'the run it rests on')
  assert.deepEqual(by('Edit').map((c) => c.href), [stepHref(7)], 'the edit that moved the proof')
  assert.deepEqual(by('How this was decided').map((c) => c.href), [stepHref(7)], 'the record the verdict turns on')
  assert.equal(by('Version').length, 0, 'a version recorded at no step draws no chip')
  for (const c of chips) assert.match(c.text, / ›$/, 'a chip that opens a step ends in the arrow: ' + c.text)
  assert.match(by('Said')[0].text, /^Said step 5 ›$/)
  assert.match(by('Run')[0].text, /^Run step 3 ›$/)
})

test('Every-Statement 20 · a statement with no run before it: Said opens its step; "How this was decided" has no single step to open, so it is not drawn', () => {
  const s = snap({ noRun: true })
  const html = live('Every-Statement', { s })
  const chips = anchors(html).filter((a) => /^(Said|Run|Edit|How this was decided)\b/.test(a.text))
  assert.deepEqual(chips.map((c) => c.text.replace(/\s+/g, ' ')), ['Said step 4 ›'], 'only the statement’s own step: ' + JSON.stringify(chips))
  assert.equal(chips[0].href, stepHref(4))
  assert.equal(html.includes('How this was decided'), false)
})

test('Every-Statement 20 · every step a chip names is a step of this Change’s record', () => {
  const s = snap({ moved: true })
  const n = s.change.stepCount
  for (const a of stepLinks(live('Every-Statement', { s }))) {
    const r = R.parse(a.href)
    assert.equal(r.change, TID)
    assert.equal(r.project, 'p1')
    assert.ok(r.step >= 1 && r.step <= n, a.href)
    assert.equal(a.href, stepHref(r.step))
  }
})

test('Every-Statement 20 · a chip with no step is text, not a button or link that does nothing', () => {
  const s = snap({ moved: true })
  const d = M.everyStatementFromChange(s)
  const run = d.runs.find((r) => r.step === 3)
  run.step = null // a check ThruLine ran: it sits at no step
  const html = live('Every-Statement', { s, data: d })
  assert.equal(anchors(html).some((a) => /^Run\b/.test(a.text)), false, 'no link for a run at no step')
  assert.ok(html.includes('step not recorded'), 'the run is still named, as text')
  assert.equal(html.includes('step not recorded ›'), false, 'and carries no arrow')
  const named = elements(html, /<(a|button)\b([^>]*)>([\s\S]*?)<\/(?:a|button)>/g).filter((c) => /step not recorded/.test(c.text))
  assert.deepEqual(named, [], 'no link or button holds it')
})

test('Every-Statement 20 · what counts as done: "Accepted" names a version of it, which sits at no step, so it is text', () => {
  const s = snap({ moved: true })
  s.criteria = { ...s.criteria, accepted: true, acceptedAt: '10:30', version: 'v1', lineCount: 1, acceptedOnVersion: null, provenOn: null }
  const d = M.everyStatementFromChange(s)
  d.criteria.lines = d.criteria.lines && d.criteria.lines.length ? d.criteria.lines : []
  const html = live('Every-Statement', { s, data: d })
  assert.equal(anchors(html).some((a) => /^Accepted\b/.test(a.text)), false)
})

test('Every-Statement 20 · the design gallery keeps the canvas’s own chip buttons', () => {
  const html = M.renderDesign('Every-Statement')
  assert.ok(/<button[^>]*class="tl-chip"/.test(html), 'a chip is a button in the gallery')
  assert.equal(STEP_ADDRESS.test(html), false)
})

/* ======================= What-Changed 11 ======================= */

test('What-Changed 11 · the record chips and each hunk’s "Edit step …›" open their step, not the Brief', () => {
  const s = snap({ moved: true })
  const html = live('What-Changed', { s })
  const chips = anchors(html).filter((a) => /tl-chip/.test(a.cls) || /^(Run|Edit)\b/.test(a.text))
  const links = chips.filter((c) => c.href && STEP_ADDRESS.test(c.href))
  assert.ok(links.length >= 2, 'the Run and the Edit chips link: ' + JSON.stringify(chips))
  const steps = new Set(links.map(stepOf))
  assert.ok(steps.has(3), 'the run at step 3')
  assert.ok(steps.has(7) || steps.has(2), 'an edit step')
  for (const c of chips) {
    assert.equal(R.parse(c.href || '#/inbox').screen === 'brief', false, 'a chip never lands on the Brief: ' + c.text)
    if (c.href) assert.equal(c.href, stepHref(stepOf(c)))
  }
  for (const c of links) assert.match(c.text, /step \d+.* ›$/, c.text)
})

test('What-Changed 11 · the hunk chip of an edit opens that edit’s step with its time', () => {
  const s = snap()
  const html = live('What-Changed', { s, state: { from: 'base' } }) // the whole Change, so its first edit is in range
  const hunk = anchors(html).find((a) => /^Edit step 2\b/.test(a.text))
  assert.ok(hunk, 'the first edit’s hunk has an Edit chip: ' + anchors(html).map((a) => a.text).join(' | ').slice(0, 400))
  assert.equal(hunk.href, stepHref(2))
})

test('What-Changed 11 · the design gallery’s chips are the canvas’s own links', () => {
  assert.equal(STEP_ADDRESS.test(M.renderDesign('What-Changed')), false)
})

/* ======================= Whole-Story 4 ======================= */

test('Whole-Story 4 · "Run step N ›" opens step N (a proven statement rests on the run at step 3)', () => {
  const s = snap()
  const html = live('Whole-Story', { s })
  const run = anchors(html).find((a) => /^Run\b.*step 3/.test(a.text))
  assert.ok(run, 'the run chip is a link: ' + anchors(html).map((a) => a.text).filter((t) => /Run/.test(t)).join(' | '))
  assert.equal(run.href, stepHref(3))
  assert.match(run.text, /step 3 ›$/)
})

test('Whole-Story 4 · a line of what counts as done has no run of its own to open yet: text, with no arrow, and no dead button', () => {
  const s = snap()
  s.criteria = { accepted: true, acceptedAt: '10:30', acceptedOnVersion: null, acceptedAfterStep: null, version: 'v1', lineCount: 1, proposedCount: 0, proposedFromStep: null, provenOn: null }
  const d = M.wholeStoryFrom(s, null)
  const html = live('Whole-Story', { s, data: d })
  assert.equal(/<button[^>]*class="tl-rec"/.test(html), false, 'no chip button is left that opens nothing')
})

test('Whole-Story 4 · the design gallery keeps the canvas’s chip buttons', () => {
  const html = M.renderDesign('Whole-Story')
  assert.ok(/<button[^>]*class="tl-rec"/.test(html))
  assert.equal(STEP_ADDRESS.test(html), false)
})

/* ======================= Change-Brief 8 ======================= */

test('Change-Brief 8 · the plates’ record chips open their steps; the check ThruLine ran and a version at no step are not links', () => {
  const s = snap({ moved: true })
  const html = live('Change-Brief', { s })
  const chips = anchors(html).filter((a) => /tl-ghost/.test(a.cls) && /^(Said|Run|Edit|Version)\b/.test(a.text))
  const by = (kind) => chips.filter((c) => c.text.startsWith(kind)).map((c) => c.href)
  assert.deepEqual(by('Said'), [stepHref(5)])
  assert.deepEqual(by('Run'), [stepHref(3)])
  assert.deepEqual(by('Edit'), [stepHref(7)])
  assert.deepEqual(by('Version'), [], 'a version recorded at no step draws no chip')
  for (const c of chips) assert.match(c.text, / ›$/)
})

test('Change-Brief 8 · "How decided" names its steps as links', () => {
  const s = snap({ moved: true })
  const html = live('Change-Brief', { s, state: { open: { why: true } } })
  const why = anchors(html).filter((a) => a.href && STEP_ADDRESS.test(a.href) && !/tl-ghost/.test(a.cls))
  assert.deepEqual(why.map(stepOf), [5, 3, 7], 'the statement, the run it rests on, the edit after it')
  assert.deepEqual(why.map((a) => a.text), ['step 5 ›', 'step 3 ›', 'step 7 ›'])
})

test('Change-Brief 8 · the design gallery still pins the record in the Technical drawer (a button)', () => {
  const html = M.renderDesign('Change-Brief')
  assert.ok(/<button[^>]*title="Pin this record in the Technical drawer"/.test(html))
  assert.equal(STEP_ADDRESS.test(html), false)
})

/* ======================= Repair-Loop ======================= */

test('Repair-Loop · the run chip opens the step of the run that proves the statement', () => {
  const s = snap()
  const d = M.selectRepairLoop(s, null, { now: NOW })
  assert.equal(d.run.state, 'passed')
  const html = live('Repair-Loop', { s, data: d })
  const run = anchors(html).find((a) => /^Run\b/.test(a.text) && /step/.test(a.text))
  assert.ok(run)
  assert.equal(run.href, stepHref(3))
  assert.match(run.text, /step 3 ›$/)
})

test('Repair-Loop · a check ThruLine ran has no step: its chip is not a step link and says what it is', () => {
  const s = snap()
  const d = M.selectRepairLoop(s, null, { now: NOW })
  d.run = { ...d.run, step: null }
  const html = live('Repair-Loop', { s, data: d })
  const run = anchors(html).find((a) => /^Run\b/.test(a.text) && /this check/.test(a.text))
  assert.ok(run, 'still named')
  assert.equal(STEP_ADDRESS.test(run.href || ''), false, 'never an address for a step it does not have')
  assert.equal(R.parse(run.href).screen, 'story', 'it keeps its link to the whole story, where the run is listed')
})

test('Repair-Loop · the design gallery’s run chip is the canvas’s own link', () => {
  assert.equal(STEP_ADDRESS.test(M.renderDesign('Repair-Loop')), false)
})

/* ======================= every screen ======================= */

for (const name of SCREENS) {
  test(`${name} · the page draws no step address a person could not open: each is a step the record numbers`, () => {
    const s = snap({ moved: true })
    for (const a of stepLinks(live(name, { s }))) {
      const r = R.parse(a.href)
      assert.equal(r.screen, 'step')
      assert.ok(s.steps.some((x) => x.step === r.step), a.href)
    }
  })
}
