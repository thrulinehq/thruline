/* Sign-off polish (HQ Next): no screen shows the home folder.
 *
 * A census over every live screen: each one is rendered on its live path
 * from a synthetic record whose every path sits under a made-up home folder
 * (/Users/someone/…), and none of them may draw that prefix — in text, in
 * title=, anywhere in the markup. Paths are shown through the one display
 * formatter (data/paths.js): relative inside the project, `<checkout>/<rel>`
 * inside another checkout of the same repository, `~/…` under the home
 * folder. The words a person copies (Tell the agent, the PR note, the
 * whole-story Markdown) write the home folder as `~`.
 *
 * It also holds the quotes to plain words: a Codex directive such as
 * `:codex-annotation{index="1"}` never reaches a screen.
 * Nothing here is the founder's record. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const pairs = ['Inbox', 'Onboarding', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask', 'Whole-Story', 'PR-Face', 'Record-Over-Time', 'Team-View', 'Settings', 'Changes', 'Projects', 'Step-View']
  const imports = pairs.map((n, i) => `import T${i} from './${n}/template.jsx'\nimport L${i} from './${n}/logic.js'`).join('\n')
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { withFrame, inboxData, firstRunData } from '../data/project.js'
import { changeSnapshot } from '../data/change.js'
import { threadFixture } from './What-Changed/thread-fixture.js'
import { everyStatementFromChange } from './Every-Statement/from-change.js'
import { whatChangedFromChange } from './What-Changed/from-change.js'
import { wholeStoryFrom } from './Whole-Story/from-change.js'
import { selectDoneCriteria } from './Done-Criteria/from-change.js'
import { selectApproveSheet } from './Approve-Sheet/from-change.js'
import { selectRepairLoop } from './Repair-Loop/from-change.js'
import { commandFromChange } from './Command-Ask/from-change.js'
import { prFaceFrom } from './PR-Face/from-change.js'
import { markdownOf } from './PR-Face/markdown.js'
import { tracked as prTracked, prDoc } from './PR-Face/pr-fixture.js'
import { settingsFromServer } from './Settings/from-server.js'
import { recordFromStatements } from './Record-Over-Time/from-projects.js'
import { teamFromStore } from './Team-View/from-store.js'
import { changesData } from './Changes/from-projects.js'
import { projectsData } from './Projects/from-projects.js'
import { stepViewFromChange } from './Step-View/from-change.js'
import { Page as StatementsPage } from './Record-Statements/index.jsx'
import { statementList } from './Record-Statements/model.js'
${imports}
const S = { ${pairs.map((n, i) => `'${n}': [T${i}, L${i}]`).join(', ')} }
export { stepViewFromChange, changeSnapshot, threadFixture, everyStatementFromChange, whatChangedFromChange, wholeStoryFrom, selectDoneCriteria, selectApproveSheet, selectRepairLoop, commandFromChange, prFaceFrom, markdownOf, prTracked, prDoc, settingsFromServer, recordFromStatements, teamFromStore, inboxData, firstRunData, changesData, projectsData }
export const renderStatements = (view) => renderToStaticMarkup(React.createElement(StatementsPage, { view, at: { project: 'p1' } }))
export { statementList }
export function render(name, props, state) {
  const [Template, Logic] = S[name]
  const L = name === 'PR-Face' ? Logic : withFrame(Logic)
  const inst = new L(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export function vals(name, props, state) {
  const [, Logic] = S[name]
  const inst = new (withFrame(Logic))(props || {})
  inst.props = props || {}
  if (state) inst.state = { ...inst.state, ...state }
  return inst.renderVals()
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-home-')), 'screens.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const HOME = '/Users/someone'
const ROOT = HOME + '/shop'
const OTHER = HOME + '/Developer/shop-dev-20260927/wt' // another checkout of the same repository
const NOW = new Date('2026-10-07T10:30:00.000Z')
const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

/** The fixture thread, moved under a home folder: one edit inside the
 *  project, the rest in another checkout; commands and words that name
 *  absolute paths; a claim that opens with a Codex directive. */
function homeThread() {
  const v = M.threadFixture()
  v.project.root = ROOT
  v.edits[0].file = ROOT + '/src/totals.py'
  v.edits[1].file = OTHER + '/src/api.py'
  v.edits[2].file = HOME + '/Developer/shop-dev-20260927/qual/STATUS.md'
  v.thread.proof.edited_after_last_pass = [OTHER + '/src/api.py', HOME + '/Developer/shop-dev-20260927/qual/STATUS.md']
  v.commands[0].command = `cd ${OTHER} && pytest -q ${OTHER}/tests/test_totals.py`
  v.commands[1].command = `ls ${ROOT}/src`
  v.claims[0].text = `:codex-annotation{index="1"} Tests pass in ${OTHER}/tests/test_totals.py.`
  v.claims[1].text = '::git-stage{cwd="' + OTHER + '"}\nCheckout total fixed.'
  v.prompts[0].text = `Fix the checkout total in ${ROOT}/src/totals.py`
  for (const r of [...v.prompts, ...v.edits, ...v.commands, ...v.claims]) r.raw_file = HOME + '/.claude/projects/-Users-someone-shop/s1.jsonl'
  v.sessions[0].file = HOME + '/.claude/projects/-Users-someone-shop/s1.jsonl'
  return v
}
const SNAP = () => M.changeSnapshot(homeThread(), null, NOW, { storageBytes: 5.4e9 })

const PROJECTS = {
  complete: true,
  projects: [
    { id: 'p1', name: 'shop', root: ROOT, complete: true, sessions: 3, threads: 1, claims: 2, sources: ['claude_code', 'codex'], last_activity: '2026-10-07T10:06:00.000Z', whose_move: { you: 1, agent: 0, done: 0 } },
    { id: 'p2', name: 'someone', root: HOME, complete: false, sessions: 1, threads: null, sources: ['claude_code'], last_activity: '2026-09-01T10:00:00.000Z', whose_move: null },
    { id: 'p3', name: 'shop', root: OTHER, complete: true, sessions: 1, threads: 1, sources: ['codex'], last_activity: '2026-10-01T10:00:00.000Z', whose_move: { you: 7, agent: 0, done: 0 } },
  ],
}
const PROJECT_VIEW = {
  project: Object.assign({}, PROJECTS.projects[0]),
  threads: [{ id: 't1', title: 'Fix the checkout total', whose_move: 'you', whose_move_basis: 'claim_without_passing_test', current_version_id: 'wt-2222222222222222', last_activity_at: '2026-10-07T10:06:00.000Z', started_at: '2026-10-07T10:00:00.000Z', sessions: 1,
    proof: { last_passing_test_at: '2026-10-07T10:02:00.000Z', edited_after_last_pass: [OTHER + '/src/api.py'] },
    top_files: [{ path: OTHER + '/src/api.py', edits: 2 }, { path: ROOT + '/src/totals.py', edits: 1 }] }],
}
const STORAGE = { bytes: 5.4e9, path: HOME + '/.thruline', parts: [{ path: HOME + '/.thruline/ledger.db', bytes: 5e9 }] }

/** Every live screen, rendered from the home-folder record. */
function pages() {
  const s = SNAP()
  const frame = {}
  const out = {
    Inbox: M.render('Inbox', { data: M.inboxData({ projects: PROJECTS, view: PROJECT_VIEW, storage: STORAGE, now: NOW }), frame }),
    Onboarding: M.render('Onboarding', { data: M.firstRunData({ projects: PROJECTS, storage: STORAGE, now: NOW }), frame }),
    'Change-Brief': M.render('Change-Brief', { data: s, frame }),
    'Every-Statement': M.render('Every-Statement', { data: M.everyStatementFromChange(s), frame }),
    'What-Changed': M.render('What-Changed', { data: M.whatChangedFromChange(s), frame }),
    'Done-Criteria': M.render('Done-Criteria', { data: M.selectDoneCriteria(s, null), frame }),
    'Repair-Loop': M.render('Repair-Loop', { data: M.selectRepairLoop(s, null, { now: NOW }), frame }),
    'Approve-Sheet': M.render('Approve-Sheet', { data: M.selectApproveSheet(s, null, {}), frame }),
    'Command-Ask': M.render('Command-Ask', { data: M.commandFromChange(s, PROJECT_VIEW, { readAt: '10:30' }), frame, overlay: true }),
    'Whole-Story': M.render('Whole-Story', { data: M.wholeStoryFrom(s, null, { now: NOW.getTime() }), frame }),
    /* C30 R-PR: the note is drawn for a tracked Change only; its lines come from the tracked model */
    'PR-Face': M.render('PR-Face', { data: M.prFaceFrom(s, { now: NOW.getTime(), tracked: M.prTracked(), pr: { doc: M.prDoc() } }) }),
    'Record-Over-Time': M.render('Record-Over-Time', { data: M.recordFromStatements({ projects: PROJECTS.projects, statements: { p1: { claims: [], sessions: [] } }, storage: STORAGE, now: NOW.getTime() }), frame }),
    'Team-View': M.render('Team-View', { data: M.teamFromStore({ project: PROJECTS.projects[0], projects: PROJECTS.projects, changes: [], storage: STORAGE, licence: null, now: NOW.getTime() }), frame }),
    Settings: M.render('Settings', { data: M.settingsFromServer({ storage: STORAGE, lanes: null, projects: PROJECTS, now: NOW }), frame }),
    // C30 R-NAV: the two home screens with no artboard
    Changes: M.render('Changes', { data: M.changesData({ projects: PROJECTS, views: { p1: PROJECT_VIEW, p3: { error: 'x' } }, storage: STORAGE, now: NOW }), frame }),
    Projects: M.render('Projects', { data: M.projectsData({ projects: PROJECTS, storage: STORAGE, now: NOW }), frame }),
    /* C30 R-ROUTES: one step of the record, with the session's own lines around it */
    'Step-View': M.render('Step-View', { data: stepData(s, s.steps.find((x) => x.kind === 'edit').step), frame }),
    /* C54 #7: the statements a Record count opens, from projects whose roots sit under the home folder */
    'Record-Statements': M.renderStatements(M.statementList({
      projects: PROJECTS.projects,
      documents: { p1: { claims: [{ id: 'c1', thread_id: 't1', ts: '2026-10-06T10:00:00.000Z', agent: 'claude_code', verdict: 'proven', verdict_basis: 'run_holds', proof_moved: false }] }, p2: { claims: [] }, p3: { claims: [] } },
      at: { range: '30', agent: 'all', scope: 'all', subset: 'all' }, now: NOW.getTime(),
    })),
  }
  return out
}

/** The step page's facts for step `n`, with a session timeline whose lines name the home folder. */
function stepData(s, n) {
  const row = s.steps.find((x) => x.step === n)
  const item = (event_id, seq, kind, text) => ({ event_id, seq, ts: row.ts, kind, text, tool: null, sidechain: false })
  const timeline = { items: [
    item('before', 1, 'assistant', 'Looking at ' + OTHER + '/src/api.py'),
    item(row.eventId, 2, 'assistant', 'Working in ' + ROOT + '/src/totals.py'),
    item('after', 3, 'tool', null),
  ] }
  timeline.items[2].tool = { name: 'Bash', summary: ['cd ' + OTHER + ' && ls'], outcome: 'succeeded', exit_code: 0 }
  return M.stepViewFromChange(s, { change: 'chg_x', step: n, project: 'p1' }, { timeline })
}

/** Every string a screen's state can reach, for the screens whose sheets
 *  open on a state (⌘K's Find rows, Every-Statement's inspector). */
function findRows(s) {
  const d = M.commandFromChange(s, PROJECT_VIEW, { readAt: '10:30' })
  return JSON.stringify(d.findable)
}

test('census: every live screen is rendered here, and none draws the home folder', () => {
  const dirs = readdirSync(here, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name).sort()
  const P = pages()
  assert.deepEqual(Object.keys(P).sort(), dirs, 'a screen folder is not in this census')
  const leaks = []
  for (const [n, html] of Object.entries(P)) {
    const t = unescape(html)
    if (t.includes(HOME)) leaks.push(n + ': ' + t.slice(Math.max(0, t.indexOf(HOME) - 80), t.indexOf(HOME) + 80))
  }
  assert.deepEqual(leaks, [])
})

/** The same screens in the states that show more: Technical, each
 *  statement in the inspector, the sheets, Settings' every section. */
function morePages() {
  const s = SNAP()
  const frame = {}
  const out = []
  const cb = { data: s, frame }
  out.push(['Change-Brief tech', M.render('Change-Brief', cb, { tech: true, open: { why: true, then: true, kind: true, after: true, tech: true }, more: true, showHeld: true })])
  for (const st of s.statements) {
    out.push(['Change-Brief pinned ' + st.step, M.render('Change-Brief', cb, { pinned: String(st.step), tech: true })])
    out.push(['Every-Statement ' + st.id, M.render('Every-Statement', { data: M.everyStatementFromChange(s), frame }, { sel: st.id, sheet: 'check' })])
    out.push(['Every-Statement tell ' + st.id, M.render('Every-Statement', { data: M.everyStatementFromChange(s), frame }, { sel: st.id, sheet: 'tell' })])
  }
  for (const x of s.steps) out.push(['Step-View step ' + x.step, M.render('Step-View', { data: stepData(s, x.step), frame })])
  out.push(['Step-View no such step', M.render('Step-View', { data: M.stepViewFromChange(s, { change: 'chg_x', step: 9999, project: 'p1' }, {}), frame })])
  out.push(['PR-Face untracked', M.render('PR-Face', { data: M.prFaceFrom(s, { now: NOW.getTime() }) })])
  out.push(['Repair-Loop copied', M.render('Repair-Loop', { data: M.selectRepairLoop(s, null, { now: NOW }), frame }, { copied: true, pasted: true })])
  for (const fmt of ['md', 'html', 'json']) out.push(['Whole-Story ' + fmt, M.render('Whole-Story', { data: M.wholeStoryFrom(s, null, { now: NOW.getTime() }), frame }, { fmt, paths: true })])
  for (const query of ['api', 'pytest', 'Tests']) out.push(['Command-Ask ' + query, M.render('Command-Ask', { data: M.commandFromChange(s, PROJECT_VIEW, { readAt: '10:30' }), frame, overlay: true }, { query })])
  for (const nav of ['agents', 'checks', 'rules', 'privacy', 'notifications', 'appearance', 'keyboard']) out.push(['Settings ' + nav, M.render('Settings', { data: M.settingsFromServer({ storage: STORAGE, lanes: null, projects: PROJECTS, now: NOW }), frame }, { nav })])
  return out
}

test('census: in the states that show more, no screen draws the home folder', () => {
  const leaks = []
  for (const [n, html] of morePages()) {
    const t = unescape(html)
    if (t.includes(HOME)) leaks.push(n + ': ' + t.slice(Math.max(0, t.indexOf(HOME) - 80), t.indexOf(HOME) + 80))
  }
  assert.deepEqual(leaks, [])
})

test('census: ⌘K’s Find rows name files without the home folder', () => {
  const rows = findRows(SNAP())
  assert.ok(rows.includes('src/api.py'))
  assert.ok(!rows.includes(HOME), rows.slice(rows.indexOf(HOME) - 80, rows.indexOf(HOME) + 80))
})

test('census: no quote, on any Change screen, carries a Codex directive', () => {
  const P = pages()
  for (const [n, html] of Object.entries(P)) {
    const t = unescape(html)
    assert.doesNotMatch(t, /:codex-annotation|::git-stage|\{index=/, n)
  }
  const s = SNAP()
  assert.equal(s.statements[0].plain, `Tests pass in ${OTHER}/tests/test_totals.py.`.replace(HOME, '~'))
  assert.ok(s.statements[0].text.startsWith(':codex-annotation{index="1"}'), 'the verbatim words stay in text')
})

test('no field of the Settings data holds the home folder, drawn or not', () => {
  const d = M.settingsFromServer({ storage: STORAGE, lanes: { claude_code: { live: 0, root: HOME + '/.claude/projects' }, codex: { live: 0, root: HOME + '/.codex/sessions' }, window_minutes: 5 }, projects: PROJECTS, now: NOW })
  const j = JSON.stringify(d)
  assert.ok(!j.includes(HOME), j.slice(Math.max(0, j.indexOf(HOME) - 80), j.indexOf(HOME) + 80))
})

test('the copied words write the home folder as ~ and keep the path the agent needs', () => {
  const s = SNAP()
  const rl = M.selectRepairLoop(s, null, { now: NOW })
  const msg = JSON.stringify(rl)
  assert.ok(!msg.includes(HOME), msg.slice(msg.indexOf(HOME) - 80, msg.indexOf(HOME) + 80))
  const md = M.prFaceFrom(s, { now: NOW.getTime(), tracked: M.prTracked(), pr: { doc: M.prDoc() } }).copy
  assert.ok(!String(md).includes(HOME))
})
