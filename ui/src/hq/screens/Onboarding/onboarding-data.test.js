/* W1-A, HQ Next wave 1: First run's content arrives through one object.
 * The design gallery (no `data`) still draws the canvas word for word,
 * DEMO STATE switcher included; the live path (`data` present) draws what
 * `data` says, takes the capture state from the chosen folder rather than the
 * switcher, and shows none of the canvas's design annotations. */
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
  const file = join(await mkdtemp(join(tmpdir(), 'onboarding-data-')), 'screen.mjs')
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

/** Every content string the canvas drew that now arrives from DEMO. */
const CANVAS = ["Folders where an agent has worked · 2","checkout-web","~/code/checkout-web","Claude Code","Git repository","Not recorded yet","notes-cli","~/code/notes-cli","No Git repository","“Tests pass.”: claimed, and no proof found in the record on e71c5b0.","step 52","checkout/api.py changed after the last run that passed.","step 56","The command at step 60 has no result yet.","step 60","~/.thruline","Session started 16:01 · Claude Code (hook v0.4)","· step 6","“the checkout total is wrong when a discount code is applied — fix it”","Fix double-applied discount on checkout total","Claude Code said ","Tests pass.","16:03","step 46","pytest -q tests/test_totals.py","4 passed, 0 failed · exit 0","step 38","version 9f2ad3e","· 1 version older","checkout/api.py","16:04","e71c5b0","· version e71c5b0","Check it again, with checkout/api.py in scope.","Each link above ends at its step: 52, 46, 38, 56.","Example: the proof chain for Tests pass"]

test('the gallery draws every canvas string from DEMO', () => {
  const html = text({}, { step: 1 }) + text({}) + text({}, { step: 3, installed: true, health: 'recording' })
  const missing = CANVAS.filter((s) => !html.includes(s))
  assert.deepEqual(missing, [])
})

test('the gallery keeps the design annotations; the live path drops every one', () => {
  const gallery = text({})
  for (const a of ['Demo state', 'Removal: NEEDS ENGINE (L1.1)', 'The Install button needs an install route', 'Illustrative: from the demo record']) assert.ok(gallery.includes(a), a)
  for (const st of [{ step: 1 }, { step: 2 }, { step: 3 }]) {
    const live = text({ data: clone(M.DEMO) }, st)
    assert.doesNotMatch(live, /NEEDS ENGINE/, JSON.stringify(st))
    assert.doesNotMatch(live, /Illustrative/, JSON.stringify(st))
    assert.doesNotMatch(live, /Demo state|Hooks can’t run/, JSON.stringify(st))
  }
})

test('the live path draws the chosen folder from data', () => {
  const data = clone(M.DEMO)
  data.folders[0].name = 'stand-in-folder'
  data.folders[0].path = '~/code/stand-in-folder'
  data.home = '~/elsewhere/thruline'
  const html = text({ data })
  assert.ok(html.includes('Everything ThruLine will add to stand-in-folder'))
  assert.ok(html.includes('cd ~/code/stand-in-folder'))
  assert.ok(html.includes('~/elsewhere/thruline'))
  assert.doesNotMatch(html, /checkout-web/)
})

test('in the product the capture state is the record’s, not the switcher’s', () => {
  const data = clone(M.DEMO)
  data.folders[0].capture = { installed: true, health: 'recording' }
  data.storage = { sessions: 7 }
  const html = text({ data }, { step: 3 })
  assert.ok(html.includes('checkout-web is recording.'))
  assert.ok(html.includes('ThruLine is keeping the record of 7 sessions on this machine.'))
  // the same UI state over DEMO stays on the switcher's default: nothing installed
  assert.ok(text({}, { step: 3 }).includes('Nothing is installed in checkout-web yet'))
})

test('template.jsx holds no demo content outside an isDemo gate', () => {
  const DEMO_WORDS = ['e71c5b0', '9f2ad3e', 'checkout-web', 'notes-cli', '~/code/', 'checkout/api.py', 'pytest -q', 'discount', '16:0', 'step 52', 'step 6', '~/.thruline']
  const leaks = templateLiterals().filter((l) => !l.gated && DEMO_WORDS.some((w) => l.value.includes(w)))
  assert.deepEqual(leaks.map((l) => l.value), [])
})

test('every design annotation in template.jsx sits under an isDemo gate', () => {
  const ungated = templateLiterals().filter((l) => !l.gated && /needs engine|illustrative:|demo state/i.test(l.value))
  assert.deepEqual(ungated.map((l) => l.value), [])
})

test('a folder with no first session reads as nothing, never as undefined', () => {
  const data = clone(M.DEMO)
  delete data.folders[0].firstSession
  delete data.example
  data.speakerExample = []
  for (const st of [{ step: 1 }, { step: 2 }, { step: 3 }]) assert.doesNotMatch(text({ data }, st), /undefined|NaN/, JSON.stringify(st))
})

/* ---- W2-A: First run on the real record's shape (data/project.js) ---- */
import { firstRunData } from '../../data/project.js'
import { LIST, NOW } from '../../data/project-fixture.js'

const runData = (load) => firstRunData({ projects: LIST, load, storage: { path: '/Users/me/.thruline' }, now: NOW })

test('live First run: every folder, what was recorded in it, and one primary action that loads it', () => {
  const calls = []
  const data = runData({ running: false, project: null, finished_at: null })
  const html = text({ data, actions: { load: (id) => calls.push(id) } })
  assert.ok(html.includes('Folders where an agent has worked · 2'))
  assert.ok(html.includes('shop') && html.includes('old'))
  assert.ok(html.includes('9 sessions · 12 prompts · 30 edits · 40 commands · 5 test runs · 7 claims · 6 threads'))
  assert.ok(html.includes('Load every session'))
  const v = new M.Logic({ data, actions: { load: (id) => calls.push(id) } }).renderVals()
  v.v.primary.act()
  assert.deepEqual(calls, ['p1'])
})

test('live First run teaches the proof chain and speaker wording with labelled sample facts', () => {
  const data = runData(null)
  const first = text({ data }, { step: 1 })
  assert.ok(first.includes('What you’ll see after the first session'))
  assert.ok(first.includes('Example only · from a sample session, not this project.'))
  assert.ok(first.includes('Tests pass.'))
  assert.ok(first.includes('step 52'))
  const second = text({ data }, { step: 2 })
  assert.ok(second.includes('example · wording illustrative'))
  assert.ok(second.includes('claimed, and no proof found in the record'))
})

test('live First run: the load’s progress, quietly, under the folder', () => {
  const data = runData({ running: true, project: 'p1', files_total: 540, files_done: 312, events_added: 401220, errors: [], finished_at: null })
  const html = text({ data, actions: { load: () => {} } })
  assert.ok(html.includes('Reading 540 recordings — 312 done · 401,220 rows'))
  assert.match(html, /<button[^>]*disabled=""[^>]*>Reading every session…/)
})

test('live First run: Install waits for this folder’s install plan and says so — never "Not available in this version"; hooks are not called absent', () => {
  // C30 R-ROUTES "Install": the route exists. Until its plan for the folder is read, the button is off and says why.
  const data = runData(null)
  const step2 = text({ data, actions: { load: () => {} } }, { step: 2 })
  assert.match(step2, /<button[^>]*disabled=""[^>]*title="Reading what is already installed for this folder…"/)
  assert.ok(!step2.includes('Not available in this version'))
  const inst = new M.Logic({ data, actions: { load: () => {} } })
  inst.renderVals()
  inst.state = Object.assign({}, inst.state, { step: 3 })
  const v = inst.renderVals()
  assert.match(v.v.h3.title, /can’t tell from here whether shop is set up/)
  assert.doesNotMatch(v.v.h3.title, /Nothing is installed/)
})

test('live First run: a quick folder shows what is counted and says the rest is not yet counted', async () => {
  const { LIST_QUICK } = await import('../../data/project-fixture.js')
  const data = firstRunData({ projects: LIST_QUICK, load: null, storage: null, now: NOW })
  const html = text({ data, actions: { load: () => {} } }, { folder: 'p9' })
  assert.ok(html.includes('4 sessions · the rest not yet counted'))
  assert.doesNotMatch(html, /\b0 (prompts|edits|commands|test runs|claims|threads)\b/)
})
