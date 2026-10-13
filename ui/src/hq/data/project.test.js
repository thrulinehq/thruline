/* W2-A: the one project model. Pure tests over a synthetic ProjectView in the
 * engine's wire shape (docs/hq-next/API.md); no real record is committed. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { PATH_UNKNOWN, currentProjectId, firstRunData, fmtWhen, inboxData, loadProgress, shortPath, shortVersion, sidebarProjects } from './project.js'
import { inboxText } from '../screens/Inbox/text.js'

import { LIST, NOW, PROJECT, VIEW, at } from './project-fixture.js'

test('every thread is one row, in the group of its whose_move, and the counts equal the project’s', () => {
  const D = inboxData({ projects: LIST, view: VIEW, storage: { bytes: 5_400_000_000 }, now: NOW })
  const t = inboxText(D)
  const ids = [...t.groups.you, ...t.groups.check, ...t.groups.agent, ...t.groups.live].map((r) => r.id)
  assert.equal(t.groups.check.length + t.groups.you.length, PROJECT.whose_move.you)
  assert.equal(Number(t.n.yourMove), PROJECT.whose_move.you)
  assert.equal(Number(t.n.agentMove), PROJECT.whose_move.agent)
  assert.equal(Number(t.n.nothing), PROJECT.whose_move.done)
  assert.equal(t.groups.live.length, 1)
  // every thread appears once: as a row, or in Nothing needed
  assert.equal(ids.length + D.nothingNeeded.items.length, VIEW.threads.length)
  assert.deepEqual(t.groups.check.map((r) => r.id), ['re1', 'np1'])
  assert.deepEqual(t.groups.you.map((r) => r.kind), ['prompt'])
})

test('row text is the thread’s own: verbatim title, short version, state from its proof', () => {
  const D = inboxData({ projects: LIST, view: VIEW, now: NOW })
  const t = inboxText(D)
  const re = t.byId.re1.t
  assert.equal(re.title, 'Fix the “total” — verbatim, masked')
  assert.equal(re.version, '3f9a1c2')
  assert.equal(re.recheck, true)
  assert.equal(re.file, 'checkout/api.py')
  assert.equal(re.moreFiles, ' and 1 more')
  assert.equal(re.rowStop, '⑤ RE-CHECKED')
  const np = t.byId.np1.t
  assert.equal(np.recheck, false)
  assert.equal(np.version, 'wt-00bb958')
  assert.equal(np.rowStop, '② RESTS ON')
  assert.equal(np.lineTail, 'No passing test run backs what the agent said.')
  // nothing the record does not hold: no evidence count, no claim quote
  assert.equal(np.needEvidence, '')
  assert.equal(np.claim, false)
})

test('rows open the thread as a Change, in this project', () => {
  const D = inboxData({ projects: LIST, view: VIEW, now: NOW })
  assert.equal(D.changes.re1.href, '#/changes/re1?project=p1')
  assert.equal(D.changes.re1.checkHref, '#/changes/re1/check?project=p1')
  assert.equal(D.prompts[0].href, '#/changes/ask1?project=p1')
  assert.equal(D.nothingNeeded.items[0].href, '#/changes/done1?project=p1')
})

test('a fact the engine does not serve stays absent, never 0', () => {
  const D = inboxData({ projects: LIST, view: VIEW, now: NOW })
  assert.equal(D.storage, undefined)
  assert.deepEqual(D.capture, {})
  const t = inboxText(D)
  assert.equal(t.storage, '')
  assert.equal(t.capClaudeShow, false)
  assert.equal(t.captureShow, false)
  assert.equal(t.nn.dismissed, '')
  assert.equal(t.nn.destructive, '')
})

test('the headline, filter and footer count threads by whose move', () => {
  const t = inboxText(inboxData({ projects: LIST, view: VIEW, storage: { bytes: 5_400_000_000 }, now: NOW }))
  assert.equal(t.headline, 'Three things are your move. One is the agent’s.')
  assert.equal(t.footer, '3 Changes are your move · 1 is the agent’s · 1 session recording · read 16:10')
  assert.equal(t.countNote, 'Counted by Change: every thread of work in shop, 6 in all.')
  assert.equal(t.hdrProjects, '2 projects · counted by Change')
  assert.equal(t.hdrRecording, 'Recording · 1 project')
  assert.equal(t.storage, 'ThruLine is keeping 5.4 GB of records on this machine · ')
})

test('which project is current: ?project= when it names one, else the most recently active', () => {
  assert.equal(currentProjectId(LIST.projects, 'p0'), 'p0')
  assert.equal(currentProjectId(LIST.projects, 'nope'), 'p1')
  assert.equal(currentProjectId(LIST.projects, undefined), 'p1')
  assert.equal(currentProjectId([], undefined), undefined)
})

test('the sidebar lists every project with its own line', () => {
  const side = sidebarProjects(LIST.projects, NOW)
  assert.deepEqual(side.map((p) => [p.name, p.recording, p.changeCount, p.lastRecordedOn]), [['shop', true, 6, '16:05'], ['old', false, 1, '1 Oct']])
  assert.equal(side[1].href, '#/inbox?project=p0')
})

test('times are local, in the design’s forms', () => {
  assert.equal(fmtWhen(at(9, 5), NOW), '09:05')
  assert.equal(fmtWhen(at(9, 5, 6), NOW), 'yesterday')
  assert.equal(fmtWhen(at(9, 5, 1), NOW), '1 Oct')
  assert.equal(fmtWhen(new Date(2025, 8, 5).toISOString(), NOW), '5 Sep 2025')
  assert.equal(fmtWhen(null, NOW), undefined)
})

test('versions and paths as a person reads them', () => {
  assert.equal(shortVersion('3f9a1c2abcdef'), '3f9a1c2')
  assert.equal(shortVersion('wt-00bb958a3c8b2ae7'), 'wt-00bb958')
  assert.equal(shortVersion(null), undefined)
  assert.equal(shortPath('/r/x/a.py', '/r/x'), 'a.py')
  assert.equal(shortPath('/Users/me/elsewhere/deep/b.md', '/r/x'), '…/deep/b.md')
})

test('First run: every folder, what was recorded in it, and the load’s progress', () => {
  const load = { schema_version: 1, running: true, project: 'p1', files_total: 540, files_done: 312, events_added: 401220, errors: [], started_at: 'x', finished_at: null }
  const F = firstRunData({ projects: LIST, load, storage: { path: '/Users/me/.thruline', bytes: 1 }, now: NOW })
  assert.equal(F.folders.length, 2)
  assert.deepEqual(F.folders[0].found, { sessions: 9, prompts: 12, edits: 30, commands: 40, tests: 5, claims: 7, threads: 6 })
  assert.equal(F.folders[0].agent, 'Claude Code and Codex')
  assert.equal(F.folders[0].path, '~/code/shop')
  assert.equal(F.folders[0].capture, null) // hooks are not served: unknown, not "not installed"
  assert.equal(F.home, '~/.thruline')
  assert.equal(F.storage.sessions, 18)
  assert.deepEqual([F.load.running, F.load.filesTotal, F.load.filesDone, F.load.eventsAdded], [true, 540, 312, 401220])
  assert.equal(loadProgress({ running: false, project: 'p1', finished_at: 'z' }, 'p1').finished, true)
  assert.equal(loadProgress({ running: true, project: 'p0' }, 'p1').other, true)
})

/* ---- polish: one recording derivation, everywhere ---- */
import { recordingPill, withFrame, PRIVACY, fmtSize } from './project.js'

test('polish: the pill says Recording only when the record shows activity in the last few minutes', () => {
  const lanes = (cc, cx) => ({ claude_code: { live: cc }, codex: { live: cx }, window_minutes: 5 })
  assert.equal(recordingPill({ lanes: lanes(1, 0) }).text, 'Recording')
  assert.equal(recordingPill({ lanes: lanes(0, 2) }).on, true)
  assert.equal(recordingPill({ lanes: lanes(0, 0), change: { recording: true } }).text, 'Recording')
  for (const quiet of [recordingPill({ lanes: lanes(0, 0) }), recordingPill({}), recordingPill({ lanes: null, change: { recording: null } })]) {
    assert.equal(quiet.text, 'Not recording now')
    assert.equal(quiet.anim, 'none', 'a quiet pill does not pulse')
  }
  assert.match(recordingPill({ lanes: lanes(1, 0) }).anim, /tl-pulse/)
  // never a project count, never "not known", never "From the record"
  for (const p of [recordingPill({ lanes: lanes(3, 1) }), recordingPill({})]) {
    assert.doesNotMatch(p.text + ' ' + p.title, /project|not known|From the record|following live/i)
  }
})

test('polish (frame#F5): on a Change screen the pill describes that Change, not the machine', () => {
  const lanes = (cc, cx) => ({ claude_code: { live: cc }, codex: { live: cx }, window_minutes: 5 })
  // Four live sessions elsewhere on the machine, and this Change is the person's move: not recording.
  for (const change of [{ recording: null }, { recording: false }, {}]) {
    const p = recordingPill({ lanes: lanes(4, 0), change })
    assert.equal(p.text, 'Not recording now', 'another live session must not make this Change look recorded')
    assert.equal(p.on, false)
    assert.equal(p.anim, 'none')
    assert.ok(p.title.length > 10, 'the tooltip explains its source')
    assert.match(p.title, /on this Change/, 'the tooltip says it is about this Change')
    assert.doesNotMatch(p.title + ' ' + p.text, /project|not known|From the record|following live|\d+ agent session/i)
  }
  // The Change's own last step is the agent's: Recording, whatever the machine says.
  assert.equal(recordingPill({ lanes: lanes(0, 0), change: { recording: true } }).text, 'Recording')
  assert.equal(recordingPill({ lanes: lanes(4, 0), change: { recording: true } }).text, 'Recording')
  assert.doesNotMatch(recordingPill({ lanes: lanes(4, 0), change: { recording: true } }).title, /\d+ agent session/, 'a Change pill does not count the machine')
  // No Change in hand (home screens, a Change still being read): the machine's lanes decide, as before.
  assert.equal(recordingPill({ lanes: lanes(4, 0) }).text, 'Recording')
  assert.equal(recordingPill({ lanes: lanes(4, 0), change: null }).text, 'Recording')
  assert.equal(recordingPill({ lanes: lanes(0, 0), change: null }).text, 'Not recording now')
})

test('polish: every live screen gets the pill and the privacy sentence from withFrame; the gallery logic is untouched', () => {
  class L { constructor(props) { this.props = props } renderVals() { return { a: 1 } } }
  const Live = withFrame(L)
  const v = new Live({ frame: { lanes: { claude_code: { live: 0 }, codex: { live: 0 } } } }).renderVals()
  assert.equal(v.a, 1)
  assert.equal(v.rec.text, 'Not recording now')
  assert.equal(v.privacy, PRIVACY)
  assert.equal(new L({}).renderVals().rec, undefined, 'the design gallery runs the bare logic')
})

test('polish: one size formatter: decimal GB', () => {
  assert.equal(fmtSize(5_400_000_000), '5.4 GB')
  assert.equal(fmtSize(1_400_000_000), '1.4 GB')
  assert.equal(fmtSize(217_100_000), '217 MB')
  assert.equal(fmtSize(null), null)
})

test('polish: a thread title’s <pasted_content> wrapper tags are dropped for display', () => {
  const view = JSON.parse(JSON.stringify(VIEW))
  view.threads[0].title = '<pasted_content id="7fa1"> Lane brief'
  const D = inboxData({ projects: LIST, view, now: NOW })
  const all = JSON.stringify(D)
  assert.doesNotMatch(all, /pasted_content/)
})

/* ---- quick rows: counted later, never 0 ---- */
import { QUICK, LIST_QUICK } from './project-fixture.js'
import { isWhole, projectsRereadEvery } from './project.js'
import { projectLine } from '../screens/Inbox/text.js'

test('quick row: the sidebar says "not yet counted", never 0 or "nothing yet"', () => {
  const side = sidebarProjects(LIST_QUICK.projects, NOW)
  const q = side.find((p) => p.id === 'p9')
  assert.equal(q.complete, false)
  assert.equal(q.recording, null)
  assert.equal(q.changeCount, undefined)
  assert.equal(projectLine(q), 'recorded 1 Oct · not yet counted')
  const t = inboxText(inboxData({ projects: LIST_QUICK, view: VIEW, now: NOW }))
  const line = t.projects.find((p) => p.name === 'notes').line
  assert.doesNotMatch(line, /\b0\b|nothing yet/)
  // the Inbox's own project comes from /api/hq/projects/<id>, which is always whole
  assert.equal(t.n.yourMove, String(PROJECT.whose_move.you))
})

test('First run: a folder with no stated path shows its folder name and the one phrase; a recorded hyphenated folder shows its recorded path', () => {
  const unknown = { ...PROJECT, id: 'u1', name: '-Users-me-my-app', root: '-Users-me-my-app', path_known: false, sessions: 0, last_activity: null }
  const recorded = { ...PROJECT, id: 'r1', name: 'my-app', root: '/Users/me/my-app', path_known: true }
  const F = firstRunData({ projects: { schema_version: 1, projects: [unknown, recorded] }, now: NOW })
  const u = F.folders.find((x) => x.id === 'u1')
  const r = F.folders.find((x) => x.id === 'r1')
  assert.equal(PATH_UNKNOWN, 'path not known until the first recording')
  assert.equal(u.path, 'path not known until the first recording')
  assert.equal(u.pathKnown, false)
  assert.equal(u.name, '-Users-me-my-app', 'the folder name as stored')
  assert.equal(r.path, '~/my-app')
  assert.equal(r.pathKnown, true)
  // a server that does not say (an older engine) is read as known
  const older = firstRunData({ projects: LIST, now: NOW })
  assert.ok(older.folders.every((f) => f.pathKnown === true))
})

test('quick row: First run shows what is counted and says the rest is not yet counted', () => {
  const F = firstRunData({ projects: LIST_QUICK, now: NOW })
  const f = F.folders.find((x) => x.id === 'p9')
  assert.equal(f.complete, false)
  assert.deepEqual(f.found, { sessions: 4, prompts: undefined, edits: undefined, commands: undefined, tests: undefined, claims: undefined, threads: undefined })
  assert.equal(F.storage.sessions, PROJECT.sessions + 4)
})

test('projects are re-read every 2 s until the list is complete, then not at all', () => {
  assert.equal(projectsRereadEvery(LIST_QUICK), 2000)
  assert.equal(projectsRereadEvery({ ...LIST_QUICK, complete: true }), undefined)
  assert.equal(projectsRereadEvery(LIST), undefined) // an older engine: no `complete`, whole rows
  assert.equal(projectsRereadEvery({ projects: [QUICK] }), 2000)
  assert.equal(projectsRereadEvery(null), undefined)
  assert.equal(isWhole(QUICK), false)
  assert.equal(isWhole(PROJECT), true)
})

test('a thread the record gives no words to title reads "Untitled work", never a blank row', () => {
  const view = { ...VIEW, threads: VIEW.threads.map((t) => (t.id === 'np1' || t.id === 'done1' ? { ...t, title: '', title_basis: 'none' } : t)) }
  const D = inboxData({ projects: LIST, view, now: NOW })
  const rows = [...Object.values(D.changes), ...D.nothingNeeded.items]
  const titles = rows.map((r) => r.title)
  assert.ok(titles.every((t) => typeof t === 'string' && t.length > 0), JSON.stringify(titles))
  assert.equal(titles.filter((t) => t === 'Untitled work').length, 2)
})
