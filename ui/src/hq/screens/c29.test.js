/* C29 (CTO ruling, 2026-10-07), on the screens themselves:
 *  1. every count of 1,000 or more is drawn with thousands separators —
 *     Record over time's headline and cards, First run's rail;
 *  2. First run's "keeping the record of N sessions" and Settings' "Records of
 *     your sessions" state the same number, from one function, with its basis;
 *  R1. the Inbox draws its frame at once and only the list area says it is
 *     reading, naming the project — never "nothing needs you", never a zero;
 *  R2. while a load runs, the status bar's line states the rows added so far,
 *     with the same separators.
 * Synthetic fixtures only; nothing here reads a real store. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as P from '../data/project.js'
import { recordFromStatements } from './Record-Over-Time/from-projects.js'
import { settingsFromServer } from './Settings/from-server.js'
import IT from './Inbox/template.jsx'
import IL from './Inbox/logic.js'
import OT from './Onboarding/template.jsx'
import OL from './Onboarding/logic.js'
import RT from './Record-Over-Time/template.jsx'
import RL from './Record-Over-Time/logic.js'
import ST from './Settings/template.jsx'
import SL from './Settings/logic.js'
const S = { Inbox: [IT, IL], Onboarding: [OT, OL], 'Record-Over-Time': [RT, RL], Settings: [ST, SL] }
export { P, recordFromStatements, settingsFromServer }
export function render(name, props, state) {
  const [Template, Logic] = S[name]
  const inst = new (P.withFrame(Logic))(props || {})
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
  const file = join(await mkdtemp(join(tmpdir(), 'hq-c29-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const words = (html) => unescape(html.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ')
const NOW = new Date('2026-10-07T10:30:00.000Z')
/** What a person sees: the text of every element not hidden by display:none. */
const shown = (html) => {
  const doc = new JSDOM('<body>' + html + '</body>').window.document
  for (const el of [...doc.querySelectorAll('[style]')]) if (/display:\s*none/.test(el.getAttribute('style'))) el.remove()
  return doc.body.textContent.replace(/\s+/g, ' ')
}

/* A machine like the founder's, in miniature: 72 projects, 76,746 sessions, one of them 55,904. */
const projectRow = (i, sessions, extra) => Object.assign({
  id: 'p' + String(i).padStart(2, '0'), name: i === 0 ? 'shop' : i === 1 ? 'thruline' : 'proj' + i, root: '/u/code/proj' + i,
  sources: ['claude_code'], sessions, complete: true, threads: 3, claims: 4,
  last_activity: new Date(NOW.getTime() - (i + 1) * 3600e3).toISOString(), whose_move: { you: 1, agent: 0, done: 2 },
}, extra || {})
const SESSIONS = [55904, 20000].concat(new Array(70).fill(12)).concat([]) // 55,904 + 20,000 + 70×12 = 76,744
SESSIONS[71] = 14 // → 76,746
const PROJECTS = { schema_version: 1, complete: true, projects: SESSIONS.map((n, i) => projectRow(i, n)) }

test('1 · Record over time draws its headline and cards with thousands separators', () => {
  // 5,646 statements in thruline: 68 proven, 351 need a re-check, 897 recorded, 4,330 with no proof found
  const mk = (n, verdict, basis) => Array.from({ length: n }, (_, k) => ({ id: verdict + k, agent: 'claude_code', verdict, verdict_basis: basis, ts: new Date(NOW.getTime() - (1 + (k % 5)) * 86400e3).toISOString() }))
  const claims = [].concat(mk(68, 'proven'), mk(351, 'not_proven', 'edited_since_run'), mk(897, 'recorded'), mk(4330, 'not_proven', 'no_match'))
  const sessions = [1, 2, 3, 4, 5, 6].map((d) => ({ source: 'claude_code', started_at: new Date(NOW.getTime() - d * 86400e3).toISOString(), last_at: new Date(NOW.getTime() - d * 86400e3 + 3600e3).toISOString() }))
  const projects = [projectRow(1, 578, { threads: 1203, claims: 5646 })]
  const data = M.recordFromStatements({ projects, statements: { p01: { claims, sessions } }, storage: { bytes: 26e9 }, now: NOW.getTime() })
  const html = words(M.render('Record-Over-Time', { data }))
  assert.ok(html.includes('68 of 5,646 statements are proven on the version they were about.'), 'the headline')
  for (const s of ['351 need a re-check', '4,330 have no proof in the record', '897 are recorded and settle nothing', '5,578 of 5,646 stop short', '1,203 Changes']) {
    assert.ok(html.includes(s), 'missing: ' + s)
  }
  for (const bare of ['5646', '4330', '5578', '1203']) assert.ok(!html.includes(bare), 'a count drawn without its separator: ' + bare)
})

test('1 · First run’s rail draws its counts with thousands separators', () => {
  const projects = { schema_version: 1, complete: true, projects: [projectRow(1, 1679, { prompts: 3067, edits: 76746, commands: { total: 4330 }, tests: { total: 1204 }, claims: 5646, threads: 1001 })] }
  const data = M.P.firstRunData({ projects, load: null, storage: { path: '/u/.thruline' }, now: NOW })
  const html = words(M.render('Onboarding', { data, actions: { load: () => {}, open: () => {} } }))
  assert.ok(html.includes('ThruLine is keeping the record of 1,679 sessions in 1 project on this machine.'), 'the rail')
  assert.ok(html.includes('1,679 sessions · 3,067 prompts · 76,746 edits · 4,330 commands · 1,204 test runs · 5,646 claims · 1,001 threads'), 'what was found')
  for (const bare of ['1679', '3067', '76746', '4330', '5646']) assert.ok(!html.includes(bare), 'a count drawn without its separator: ' + bare)
})

test('2 · First run and Settings state the same sessions count, from one function, with its basis', () => {
  const kept = M.P.sessionsKept(PROJECTS)
  assert.deepEqual(kept, { sessions: 76746, projects: 72, uncounted: 0, reading: 0 })
  const phrase = M.P.sessionsKeptPhrase(kept)
  assert.equal(phrase, '76,746 sessions in 72 projects')

  const first = words(M.render('Onboarding', { data: M.P.firstRunData({ projects: PROJECTS, load: null, storage: null, now: NOW }), actions: { load: () => {}, open: () => {} } }))
  assert.ok(first.includes('ThruLine is keeping the record of 76,746 sessions in 72 projects on this machine. Each sub-agent’s session counts as one.'), first.slice(0, 400))

  const storage = { schema_version: 4, bytes: 26.3e9, path: '/u/.thruline', parts: [{ key: 'records', bytes: 11.7e9 }, { key: 'preserved_transcripts', bytes: 14.6e9 }] }
  const settings = words(M.render('Settings', { data: M.settingsFromServer({ storage, projects: PROJECTS, now: NOW }) }, { nav: 'privacy' }))
  assert.ok(settings.includes('Records of your sessions'), 'the part')
  assert.ok(settings.includes('76,746 sessions in 72 projects · every recorded step'), 'the part names the same count')
  assert.ok(settings.includes('Records of your sessions: 76,746 sessions in 72 projects, the sum over every project ThruLine lists. Each sub-agent’s session counts as one.'), 'and its basis')
  // the same phrase, on both screens, from the one derivation
  assert.ok(first.includes(phrase) && settings.includes(phrase))
})

test('2 · rows still being read are said, never folded silently into a whole-looking total', () => {
  const rows = PROJECTS.projects.map((p, i) => (i < 3 ? Object.assign({}, p, { complete: false, threads: null, whose_move: null }) : p))
  const quick = { schema_version: 1, complete: false, projects_complete: 69, projects: rows }
  const k = M.P.sessionsKept(quick)
  assert.equal(k.reading, 3)
  assert.equal(M.P.sessionsKeptSentence(k), 'ThruLine is keeping the record of 76,746 sessions in 72 projects on this machine. Each sub-agent’s session counts as one. Still counting 3 of the 72 projects.')
  // a row that serves no session count is not a 0: the total is "at least"
  const holes = { projects: rows.map((p, i) => (i === 5 ? Object.assign({}, p, { sessions: null }) : p)) }
  assert.match(M.P.sessionsKeptSentence(M.P.sessionsKept(holes)), /^ThruLine is keeping the record of at least 76,734 sessions in 72 projects/)
  // nothing served: nothing said; nothing kept: said plainly
  assert.equal(M.P.sessionsKept(null), null)
  assert.equal(M.P.sessionsKeptSentence(M.P.sessionsKept({ projects: [] })), 'ThruLine is keeping nothing on this machine yet.')
  // Settings says the same "still counting" beside its number
  const s = M.settingsFromServer({ storage: { bytes: 1e9, parts: [{ key: 'records', bytes: 1e9 }] }, projects: quick, now: NOW })
  assert.equal(s.storage.kept.reading, 'Still counting 3 of the 72 projects.')
})

test('R1 · the Inbox draws its frame while the project is read, and only the list says so', () => {
  const data = M.P.inboxReading({ projects: PROJECTS, projectId: 'p01', storage: { bytes: 26.3e9 }, now: NOW })
  const html = shown(M.render('Inbox', { data, frame: { load: null } }))
  assert.ok(html.includes('Reading the record of thruline…'), 'the list area names the project')
  // the frame: top bar, sidebar project list, status bar
  for (const s of ['Inbox', '72 projects · counted by Change', 'shop', 'proj70', 'Nothing leaves this machine', 'ThruLine is keeping 26.3 GB of records on this machine']) {
    assert.ok(html.includes(s), 'the frame lacks: ' + s)
  }
  // never "nothing needs you", no group, no zero count
  for (const never of ['Nothing is your move', 'your move ·', 'Nothing needed', 'Waiting on you', '0 sessions', 'Counted by Change:']) {
    assert.ok(!html.includes(never), 'while reading the Inbox says: ' + never)
  }
  // before even the list answers: the frame, and "Reading the record…"
  const bare = words(M.render('Inbox', { data: M.P.inboxReading({ projects: null, projectId: undefined, storage: null, now: NOW }), frame: { load: null } }))
  assert.ok(bare.includes('Reading the record…') && bare.includes('Nothing leaves this machine'))
  // the same screen with its view is the ordinary Inbox
  const view = { project: projectRow(1, 578, { threads: 0, whose_move: { you: 0, agent: 0, done: 0 } }), threads: [] }
  const live = words(M.render('Inbox', { data: M.P.inboxData({ projects: PROJECTS, view, storage: null, now: NOW }), frame: { load: null } }))
  assert.ok(live.includes('Nothing is your move.') && !live.includes('Reading the record'))
})

test('R1 · while reading, a badge is drawn only for a count the list already has, never a 0', () => {
  // C46 §1.1: the count is the one count, from the project's readiness reading a screen kept
  const read = { project: 'p01', ready: {}, moves: {}, failed: 0, listFailed: false }
  const none = { projects: PROJECTS.projects.map((p) => Object.assign({}, p, { whose_move: { you: 0, agent: 0, done: 1 } })) }
  assert.equal(M.P.inboxReading({ projects: none, projectId: 'p01', now: NOW, reading: read }).inboxBadge, null)
  assert.equal(M.P.inboxReading({ projects: PROJECTS, projectId: 'p01', now: NOW, reading: read }).inboxBadge, 1)
  assert.equal(M.P.inboxReading({ projects: PROJECTS, projectId: 'p01', now: NOW }).inboxBadge, null, 'no reading kept yet: no badge')
})

test('R2 · while a load runs, the status bar states the rows added so far, with separators', () => {
  const load = { running: true, project: 'p01', files_total: 1204, files_done: 312, events_added: 76746, finished_at: null }
  assert.equal(M.P.loadLine(load), 'Reading new recordings · 312 of 1,204 · 76,746 rows added')
  assert.equal(M.P.loadLine(Object.assign({}, load, { events_added: 1 })), 'Reading new recordings · 312 of 1,204 · 1 row added')
  // drawn in the status bar, on the reading Inbox as on any screen
  const data = M.P.inboxReading({ projects: PROJECTS, projectId: 'p01', storage: null, now: NOW })
  const html = words(M.render('Inbox', { data, frame: { load } }))
  assert.ok(html.includes('Reading new recordings · 312 of 1,204 · 76,746 rows added'))
  assert.ok(!html.includes('76746'))
})
