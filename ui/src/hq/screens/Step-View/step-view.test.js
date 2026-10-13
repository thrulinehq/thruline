/* C30 R-ROUTES "Step view": `#/changes/<id>/step/<n>`.
 *
 *  1. The route: parse and href round-trip; an address that names no step the
 *     record numbers parses to a step page with no number (never to the Brief,
 *     never to the Inbox); no canvas link names the page; Find keeps the step.
 *  2. The one Change model numbers every row: `steps` agrees with the numbers
 *     on edits, runs and statements, includes the asks, and holds the home
 *     folder only as `~`.
 *  3. The page: an edit, a test run, a claim and an ask each show their own
 *     record, the time, the version, the receipt as `~/…` with its line, and
 *     the steps either side as links to `href('step', …)`.
 *  4. A step with no neighbours, and a step the record does not number, are
 *     drawn as themselves — "no such step" links back to the Brief and Every
 *     statement and never to the Inbox.
 *  5. The session's own lines around the step: the window, the step marked;
 *     a read that failed, and a step not among the lines, say so.
 *  6. Copy: plain words, no home folder, no "Not available in this version".
 *
 * Every record is synthetic (step-fixture.js); nothing reads a store or a server. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { thread, timeline, NOW, TID, HOME_FILE } from './step-fixture.js'
import { stepHref as chipHref, stepLinker } from './link.js'

const dom = new JSDOM('<!DOCTYPE html><div id="root"></div>', { url: 'http://127.0.0.1/' })
globalThis.window = dom.window
globalThis.document = dom.window.document

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as R from '../../routes.js'
import { changeSnapshot } from '../../data/change.js'
import * as FC from './from-change.js'
import Logic, * as LG from './logic.js'
import Template from './template.jsx'
import { withFrame } from '../../data/project.js'
import { PortedScreen, projectOf } from '../../screen.jsx'
export { R, changeSnapshot, FC, LG }
export function render(data, at, extra) {
  return renderToStaticMarkup(React.createElement(PortedScreen, { Logic: withFrame(Logic), Template, at, project: data.project.id, data, extra: { frame: {}, ...extra } }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-step-view-')), 'step-view.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()
const { R, FC } = M

const snap = (o) => M.changeSnapshot(thread(o), null, NOW, { storageBytes: 2.1e9 })
const unescape = (x) => x.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const textOf = (html) => unescape(html.replace(/<[^>]*>/g, ' ')).replace(/\s+/g, ' ')
const atOf = (step, extra = {}) => ({ screen: 'step', change: TID, project: 'p1', step, ...extra })
/** The page for step `n` of the record, with the session timeline when given. */
function page(n, { s = snap(), tl, tlError, at } = {}) {
  const a = at || atOf(n)
  const data = FC.stepViewFromChange(s, a, { timeline: tl === undefined ? null : tl, timelineError: tlError || null })
  return unescape(M.render(data, a, {}))
}
const anchors = (html) => {
  const out = []
  const re = /<a\b([^>]*)>([\s\S]*?)<\/a>/g
  let m
  while ((m = re.exec(html))) {
    const attr = (k) => { const x = m[1].match(new RegExp('\\b' + k + '="([^"]*)"')); return x ? x[1] : null }
    out.push({ href: attr('href'), label: m[2].replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim(), current: attr('aria-current') })
  }
  return out
}
const stepHref = (n, project = 'p1') => R.href('step', { change: TID, step: n, project })

test('C54 statements#20: an unclassified agent message has a readable Said step receipt', () => {
  const view = thread()
  view.messages = [{ event_id: 'said-other', session_id: view.sessions[0].id, ts: '2026-10-07T10:02:00Z', text: 'Looking at the cart module first.', raw_file: '/Users/example/.claude/projects/-repo/s1.jsonl', raw_line: 9 }]
  const s = M.changeSnapshot(view, null, NOW)
  const row = s.steps.find((step) => step.kind === 'message' && step.id === 'said-other')
  assert.ok(row)
  const data = FC.stepViewFromChange(s, atOf(row.step))
  assert.equal(data.step.message.text, 'Looking at the cart module first.')
  const h = page(row.step, { s })
  assert.match(textOf(h), /Claude Code said/)
  assert.match(textOf(h), /Looking at the cart module first\./)
  assert.doesNotMatch(textOf(h), /A command/)
})

/* ======================= 1. The route (the address tests are in ../../routes.test.js) ======================= */

test('1 · a chip links only to a step the record numbers: a whole number from 1; a check ThruLine ran has none', () => {
  const at = { change: 'abc', project: 'p1' }
  assert.equal(chipHref(at, 12), '#/changes/abc/step/12?project=p1')
  for (const none of [null, undefined, 0, -1, 12.5, NaN, '3', Infinity]) assert.equal(chipHref(at, none), undefined, String(none))
  assert.equal(chipHref({ project: 'p1' }, 3), undefined, 'no Change open, no link')
  assert.equal(chipHref(undefined, 3), undefined)
  /* the address's project wins; without one the record's own is used */
  assert.equal(stepLinker({ change: 'abc' }, 'p9')(4), '#/changes/abc/step/4?project=p9')
  assert.equal(stepLinker({ change: 'abc', project: 'p1' }, 'p9')(4), '#/changes/abc/step/4?project=p1')
  assert.equal(stepLinker({ project: 'p1' }, 'p9')(4), undefined)
})

test('1 · the app has a screen for the step route (the route never lands on the Inbox’s module)', async () => {
  const { readFile } = await import('node:fs/promises')
  const app = await readFile(join(here, '..', '..', 'app.jsx'), 'utf8')
  assert.match(app, /'Step-View': lazy\(\(\) => import\('\.\/screens\/Step-View\/index\.jsx'\)\)/)
  assert.equal(R.SCREENS[R.parse('#/changes/abc/step/2').screen], 'Step-View')
})

/* ======================= 2. The model numbers every row ======================= */

test('2 · steps are the record: one per row, in the numbers edits, runs and statements carry', () => {
  const s = snap({ moved: true })
  assert.equal(s.steps.length, s.change.stepCount)
  assert.deepEqual(s.steps.map((x) => x.step), [1, 2, 3, 4, 5, 6, 7, 8])
  assert.deepEqual(s.steps.map((x) => x.kind), ['prompt', 'edit', 'command', 'prompt', 'claim', 'claim', 'edit', 'command'])
  for (const e of s.edits) assert.equal(s.steps[e.step - 1].id, e.id)
  for (const r of s.runs) assert.equal(s.steps[r.step - 1].id, r.id)
  for (const t of s.statements) assert.equal(s.steps[t.step - 1].id, t.id)
  assert.equal(s.steps[3].prompt.text.startsWith('Also check the rounding'), true, 'a later ask has a step, though the Change’s request is the first')
  assert.equal(s.request.step, 1)
})

test('2 · a step holds its own time, version, agent, receipt and detail; the home folder is only ever ~', () => {
  const s = snap()
  const run = s.steps[2]
  assert.equal(run.kind, 'command')
  assert.equal(run.time.length, 5)
  assert.equal(run.version, 'wt-1111111')
  assert.equal(run.agent, 'Claude Code')
  assert.deepEqual(run.receipt, { file: '~/.claude/projects/-repo/s1.jsonl', line: 6 })
  assert.equal(run.command.exitCode, 0)
  assert.deepEqual(run.command.tally, { passed: 4, failed: 0, skipped: 0, unit: 'tests', runner: 'pytest' })
  assert.equal(run.command.tallyLine, '4 passed in 0.2s')
  assert.equal(run.eventId, 'c1', 'the event id has its #part cut: it names the session line')
  assert.equal(s.steps[3].prompt.text.includes('/Users/'), false)
  assert.equal(s.steps[3].prompt.text.includes('~/code/shop/src/totals.py'), true)
  assert.equal(JSON.stringify(s.steps).includes('/Users/'), false)
})

test('2 · adding steps adds no version to the list the other screens draw', () => {
  const t = thread()
  t.prompts[1].version_id = 'wt-9999999999999999' // a version no row but this ask names
  const s = M.changeSnapshot(t, null, NOW, {})
  assert.equal(s.versions.some((v) => v.fullId === 'wt-9999999999999999'), false)
  assert.equal(s.steps[3].version, 'wt-9999999')
})

/* ======================= 3. The page ======================= */

test('3 · an edit step: what the agent edited, its lines, the version it made, the receipt as ~/… with its line', () => {
  const html = page(2)
  const text = textOf(html)
  assert.match(text, /Claude Code edited a file/)
  assert.match(text, /src\/totals\.py|\/repo\/src\/totals\.py/)
  assert.match(text, /\+1 −1 lines/)
  assert.match(text, /@@ -10,2 \+10,3 @@ def total\(items\)/)
  assert.match(text, /s = s \+ item\.price/)
  assert.match(text, /~\/\.claude\/projects\/-repo\/s1\.jsonl/)
  assert.match(text, /Line\s+4\b/)
  assert.match(text, /Version\s+wt-1111111/)
  assert.match(text, /Step 2/)
  assert.equal(html.includes('/Users/'), false, 'the home folder is never drawn')
})

test('3 · a test-run step shows the command, the exit, the tally and the output as the record states them', () => {
  const text = textOf(page(3))
  assert.match(text, /Claude Code ran a test/)
  assert.match(text, /pytest -q tests\/test_totals\.py/)
  assert.match(text, /A test run\s+Succeeded\s+exit 0\s+4 tests passed, 0 failed/)
  assert.match(text, /4 passed in 0\.2s/)
  assert.match(text, /The last 3 of 12 lines of output/)
  assert.match(text, /collected 4 items/)
})

test('3 · a failed run with no count says so; a command with no result says it has none', () => {
  const t = thread({ moved: true })
  t.commands[0] = { ...t.commands[0], outcome: 'failed', exit_code: 1, tally: null, tally_line: null }
  const s = M.changeSnapshot(t, null, NOW, {})
  const failed = textOf(page(3, { s }))
  assert.match(failed, /Failed\s+exit 1/)
  assert.match(failed, /The record states no test count for this run\./)
  const open = textOf(page(8, { s }))
  assert.match(open, /Claude Code ran a command/)
  assert.match(open, /No result in the record/)
  assert.doesNotMatch(open, /exit /)
})

test('3 · a claim step shows the sentence, what kind it is, and a way into the full statement view', () => {
  const html = page(5)
  const text = textOf(html)
  assert.match(text, /Claude Code said/)
  assert.match(text, /❝\s*Tests pass\.\s*❞/)
  assert.match(text, /It says the tests pass\. A test run can check this\./)
  const link = anchors(html).find((a) => /full statement view/.test(a.label))
  assert.ok(link, 'the full statement view is offered')
  assert.equal(R.parse(link.href).screen, 'brief')
  assert.equal(R.parse(link.href).statement, snap().steps[4].claim.statementId)
  assert.match(textOf(page(6)), /Nothing in the record can check this on its own\./)
})

test('3 · an ask step shows what was asked, in the person’s words', () => {
  const text = textOf(page(4))
  assert.match(text, /You asked/)
  assert.match(text, /Also check the rounding in ~\/code\/shop\/src\/totals\.py/)
  assert.equal(page(4).includes('/Users/'), false)
})

test('3 · the steps either side are links to their own step addresses, and the aside lists the ones around', () => {
  const html = page(3)
  const links = anchors(html)
  assert.equal(links.find((a) => /Previous step/.test(a.label)).href, stepHref(2))
  assert.equal(links.find((a) => /Next step/.test(a.label)).href, stepHref(4))
  const aside = links.filter((a) => /^Step \d+/.test(a.label))
  assert.deepEqual(aside.map((a) => a.href), [1, 2, 3, 4, 5, 6].map((n) => stepHref(n)))
  assert.equal(aside.find((a) => a.current === 'location').href, stepHref(3), 'the step in front of the person is marked')
  for (const a of aside) assert.equal(R.parse(a.href).step, Number(a.label.match(/^Step (\d+)/)[1]))
})

test('3 · the step page keeps the project and links back to the Change’s Brief, Explore and Workbench', () => {
  const links = anchors(page(3))
  const want = { Brief: 'brief', Explore: 'statements', Workbench: 'changed' }
  for (const [label, screen] of Object.entries(want)) {
    const a = links.find((x) => x.label === label)
    assert.ok(a, label)
    assert.equal(R.parse(a.href).screen, screen)
    assert.equal(R.parse(a.href).change, TID)
    assert.equal(R.parse(a.href).project, 'p1')
  }
  assert.equal(R.parse(links.find((x) => /Back to the Change/.test(x.label)).href).screen, 'brief')
})

/* ======================= 4. No neighbours; no such step ======================= */

test('4 · the first step has no previous, the last has no next: said as text, not as a link', () => {
  const s = snap()
  const first = page(1, { s })
  assert.match(textOf(first), /First step/)
  assert.equal(anchors(first).some((a) => /Previous step/.test(a.label)), false)
  assert.equal(anchors(first).some((a) => /Next step/.test(a.label)), true)
  const last = page(6, { s })
  assert.match(textOf(last), /Last step/)
  assert.equal(anchors(last).some((a) => /Next step/.test(a.label)), false)
})

test('4 · a Change of one step has no neighbours at all and draws nothing for them', () => {
  const s = snap({ single: true })
  assert.equal(s.steps.length, 1)
  const html = page(1, { s })
  const text = textOf(html)
  assert.match(text, /First step/)
  assert.match(text, /Last step/)
  assert.match(text, /of 1 step in this Change/)
  assert.equal(anchors(html).some((a) => /Previous step|Next step/.test(a.label)), false)
  assert.equal(anchors(html).filter((a) => /^Step \d+/.test(a.label)).length, 1, 'the aside holds only this step')
})

for (const [label, step, asked, title] of [
  ['past the last step', 99, undefined, /There is no step 99 in this Change\./],
  ['not a number', null, 'abc', /“abc” is not a step number\./],
  ['no number', null, undefined, /This address names no step\./],
]) {
  test(`4 · ${label}: a visible "no such step" with links back to the Brief and Every statement, and no way to the Inbox in its body`, () => {
    const s = snap()
    const a = atOf(step, asked ? { asked } : {})
    const data = FC.stepViewFromChange(s, a, {})
    const html = unescape(M.render(data, a, {}))
    const text = textOf(html)
    assert.match(text, title)
    assert.match(text, /This Change’s record has 6 steps, numbered 1 to 6\./)
    const body = html.slice(html.indexOf('aria-label="No such step"'))
    const links = anchors(body.slice(0, body.indexOf('</article>')))
    assert.deepEqual(links.map((l) => l.label), ['Back to the Brief', 'Every statement›'])
    assert.equal(R.parse(links[0].href).screen, 'brief')
    assert.equal(R.parse(links[1].href).screen, 'statements')
    for (const l of links) { assert.equal(R.parse(l.href).change, TID); assert.equal(R.parse(l.href).project, 'p1') }
    assert.equal(links.some((l) => /inbox/i.test(l.href || '')), false, 'the body never links to the Inbox')
    assert.equal(/Previous step|Next step/.test(text), false, 'a missing step has no neighbours to offer')
  })
}

/* ======================= 5. The session around the step ======================= */

test('5 · the session’s own lines around the step: three each side, the step marked, the rest counted', () => {
  const html = page(3, { tl: timeline() })
  const text = textOf(html)
  assert.match(text, /The session around this step/)
  const i = html.indexOf('The session around this step')
  const list = html.slice(i, html.indexOf('</ol>', i))
  const items = [...list.matchAll(/<li\b([^>]*)>/g)].map((m) => m[1])
  assert.equal(items.length, 7, 'c1 is the fourth line: three before it, itself and three after')
  assert.equal(items.filter((x) => /aria-current="location"/.test(x)).length, 1)
  assert.match(text, /Bash · succeeded pytest -q tests\/test_totals\.py/)
  assert.match(text, /You asked Also check the rounding in ~\/code\/shop\/src\/totals\.py/, 'the line’s words have the home folder as ~')
  assert.match(text, /4 later lines in this session/)
  assert.doesNotMatch(text, /earlier line/)
})

test('5 · while the session is read it says so; if it cannot be read, it says that, and the rest of the page stands', () => {
  assert.match(textOf(page(3)), /Reading the session…/)
  const failed = page(3, { tlError: new Error('x') })
  assert.match(textOf(failed), /The lines of this session around the step could not be read now\./)
  assert.match(textOf(failed), /Claude Code ran a test/)
})

test('5 · a step that is not among the session’s lines says so instead of showing other lines', () => {
  const tl = timeline()
  tl.items = tl.items.filter((x) => x.event_id !== 'c1')
  const text = textOf(page(3, { tl }))
  assert.match(text, /This step’s line was not found among the session’s lines\./)
  assert.doesNotMatch(text, /A tool answered|earlier line/)
})

test('5 · a long line is cut and says how much was left out', () => {
  const tl = timeline()
  tl.items[1].text = 'x'.repeat(900)
  const text = textOf(page(3, { tl }))
  assert.match(text, /… \(300 more characters\)/)
})

/* ======================= 6. Copy ======================= */

const BANNED = [/\blie[sd]?\b/i, /\bcheat/i, /\bviolat/i, /\bignor/i, /\bhallucinat/i, /\btrial\b/i, /Not available in this version/i, /\d+\s?%/]
const STATES = [
  ['an edit', () => page(2, { tl: timeline() })],
  ['a test run', () => page(3, { tl: timeline() })],
  ['a claim', () => page(5, { tl: timeline() })],
  ['an ask', () => page(4, { tl: timeline() })],
  ['no such step', () => page(99)],
  ['a session that could not be read', () => page(3, { tlError: new Error('x') })],
]
for (const [name, get] of STATES) {
  test(`6 · ${name}: plain words, no home folder, nothing "not available", no confidence figure`, () => {
    const html = get()
    const text = textOf(html)
    for (const re of BANNED) assert.doesNotMatch(text, re, String(re))
    assert.equal(html.includes('/Users/'), false)
    assert.equal(html.includes('/home/'), false)
    assert.equal(html.includes(HOME_FILE), false)
  })
}

test('6 · the page writes nothing and reaches no write address', async () => {
  const { readFile } = await import('node:fs/promises')
  for (const f of ['index.jsx', 'logic.js', 'from-change.js', 'template.jsx', 'chip.jsx', 'link.js']) {
    const src = await readFile(join(here, f), 'utf8')
    assert.doesNotMatch(src, /method:\s*['"]POST|writeAction|postWithToken|fetch\(/, f)
  }
})
