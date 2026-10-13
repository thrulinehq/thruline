/* PR-Face's data, selected from the ONE Change snapshot and, for a tracked
 * Change, its own documents and model.
 *
 *   prFaceFrom(snapshot, ctx) → the screen's data (DATA.md, demo.js's shape)
 *   noteFacts(snapshot, ctx)  → the typed facts the note is written from
 *
 * `ctx`:
 *   now      the read time (ms, Date or ISO); the note's "read at"
 *   tracked  `{id, change, model}` from `Repair-Loop/live.js` `useLive`: the
 *            tracked Change's served document and its `deriveChangeModel`
 *            reading, or null for work nobody tracked
 *   pr       `{doc, error, loading}`: `GET /api/changes/<id>/pr` (read.js)
 *   first    `{facts, readAt}`: the facts of the first read on this page, for
 *            the edited-in-place panel
 *   track    `{busy, said}`: where "Track this work as a Change" stands
 *
 * PURE: no fetch, no window, no clock but `now`. Nothing here re-derives a
 * fact the snapshot or the model holds: verdicts, chains, steps, tallies,
 * versions, line states, readiness and the standing approval are read as they
 * state them. A count they do not hold stays null and the note says so in
 * words; it is never 0.
 *
 * WORK NOBODY TRACKED has no note at all: the page says what a tracked
 * Change's note shows, and offers to track it (C30 R-UNTRACKED). It is never
 * a note drawn from statements alone.
 *
 * TIME. The note is a file that can leave the machine, so every time in it is
 * UTC with `Z`, read from a served ISO timestamp — never the snapshot's local
 * `HH:MM`, and never an offset. A row with no timestamp has no time.
 */
import { composeNote, copyOf, markdownOf, setupOf, LINE_MARK, RECEIPTS_DIR } from './markdown.js'
import { countOf } from '../../data/count.js'
import { href as routeHref } from '../../routes.js'

export { MAX_ROWS, MAX_SAID, MAX_RUNS, MAX_VERSIONS } from './markdown.js'

const pad = (n) => String(n).padStart(2, '0')
const list = (x) => (Array.isArray(x) ? x : [])
const str = (x) => (typeof x === 'string' && x.trim() ? x : null)

/** `HH:MMZ` in UTC, from a served ISO timestamp, or null. */
export function utcClock(ts) {
  const d = ts == null ? null : new Date(ts)
  if (!d || Number.isNaN(d.getTime())) return null
  return pad(d.getUTCHours()) + ':' + pad(d.getUTCMinutes()) + 'Z'
}

/** `YYYY-MM-DD HH:MMZ` in UTC, or null. */
export function utcStamp(ts) {
  const d = ts == null ? null : new Date(ts)
  if (!d || Number.isNaN(d.getTime())) return null
  return d.getUTCFullYear() + '-' + pad(d.getUTCMonth() + 1) + '-' + pad(d.getUTCDate()) + ' ' + utcClock(d.toISOString())
}

/** The commit trailer an approval gives, as the Approve sheet writes it
 *  (`Approve-Sheet/logic.js`; pinned equal in `pr-face-r-pr.test.js`). */
export const TRAILER_KEY = 'ThruLine-Approved: '

const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : null)

/** A run as the note states it: only typed fields, time in UTC. */
function runFact(r) {
  if (!r) return null
  return {
    id: r.id,
    step: num(r.step),
    command: r.command == null ? null : String(r.command),
    version: r.version || null,
    status: r.status || null,
    outcome: r.outcome || null,
    exitCode: num(r.exitCode),
    passed: num(r.passed),
    failed: num(r.failed),
    at: utcClock(r.ts),
  }
}

/** One line of what counts as done, as the model reads it. */
function lineFact(c) {
  const fr = c && c.freshness && typeof c.freshness === 'object' ? c.freshness : null
  return {
    n: num(c && c.n),
    text: c && c.text != null ? String(c.text) : '',
    state: c && LINE_MARK[c.state] ? c.state : null,
    word: str(c && c.word),
    line: str(c && c.line),
    sentence: str(c && c.sentence),
    freshness: fr ? { fresh: fr.fresh === true ? true : (fr.fresh === false ? false : null), sentence: str(fr.sentence) } : null,
    evidence: c && Array.isArray(c.items) ? c.items.length : null,
    evidenceName: c && Array.isArray(c.items) && c.items[0] ? str(c.items[0].name) : null,
  }
}

/** The tracked Change ctx holds, or null: a tracked Change is one whose model was read. */
const trackedOf = (ctx) => (ctx && ctx.tracked && ctx.tracked.model ? ctx.tracked : null)

/**
 * The typed facts the note is written from.
 * @param {object} snap a Change snapshot (data/change.js)
 * @param {{now?: number|Date|string, tracked?: object|null}} [ctx]
 */
export function noteFacts(snap, ctx = {}) {
  const s = snap || {}
  const now = ctx.now == null ? Date.now() : ctx.now
  const nowIso = new Date(now instanceof Date ? now.getTime() : now).toISOString()
  const version = s.currentVersion || null
  const runs = list(s.runs)
  const edits = list(s.edits)
  const statements = list(s.statements)
  const runById = new Map(runs.map((r) => [r.id, r]))
  const editById = new Map(edits.map((e) => [e.id, e]))
  const change = s.change || {}

  const checkable = statements.filter((st) => st && st.verifier).map((st) => {
    const p = st.proof || {}
    const rested = p.restsOn && p.restsOn.runId ? runById.get(p.restsOn.runId) : null
    const sinceEdit = p.since && p.since.editId ? editById.get(p.since.editId) : null
    const failedRun = p.verdict === 'contradicted' && p.recheck && p.recheck.runId ? runById.get(p.recheck.runId) : null
    return {
      id: st.id,
      step: num(st.step),
      text: st.plain != null ? String(st.plain) : String(st.text == null ? '' : st.text),
      verdict: p.verdict || 'not_proven',
      stopsAt: num(p.stopsAt),
      restsOn: runFact(rested),
      since: sinceEdit ? { file: sinceEdit.file || null, step: num(sinceEdit.step) } : null,
      failedAfter: failedRun && failedRun.id !== (rested && rested.id) ? runFact(failedRun) : null,
    }
  })
  const unchecked = statements.filter((st) => st && !st.verifier).map((st) => ({
    id: st.id, step: num(st.step), text: st.plain != null ? String(st.plain) : String(st.text == null ? '' : st.text),
  }))

  const tests = runs.filter((r) => r && r.kind === 'test').map(runFact)
  const row = list(s.versions).find((x) => x && x.id === version) || null
  const stamps = [...edits, ...runs].map((x) => x && x.ts).filter(Boolean).map((t) => Date.parse(t)).filter((t) => !Number.isNaN(t))
  const lastRunTs = runs.map((r) => r && r.ts).filter(Boolean).sort().pop() || null

  const sessions = list(s.sessions)
  const agents = Array.from(new Set(sessions.map((x) => x && x.agent).filter(Boolean)))
  const m = s.masking

  /* ---- the tracked Change: its lines, readiness and standing approval ---- */
  const tracked = trackedOf(ctx)
  const model = tracked ? tracked.model : null
  const doc = tracked && tracked.change ? tracked.change : {}
  const scope = doc.accepted_scope || null
  const acc = scope && scope.acceptance ? scope.acceptance : null
  const dec = model && model.decision ? model.decision : null
  const standing = doc.decisions && doc.decisions.standing ? doc.decisions.standing : null
  const R = model && model.readiness ? model.readiness : null

  return {
    tracked: !!tracked,
    changeId: change.id || null,
    title: change.title == null ? null : String(change.title),
    project: s.project && s.project.name ? String(s.project.name) : null,
    projectId: s.project && s.project.id ? String(s.project.id) : null,
    version,
    readVersion: (s.view && s.view.readVersion) || null,
    state: change.state || null,
    accepted: !!scope,
    criteriaVersion: scope && scope.version != null ? 'v' + scope.version : null,
    acceptedAt: acc ? utcClock(acc.accepted_at) : null,
    lineVersion: model && model.version && str(model.version.short) ? model.version.short : null,
    lines: model ? list(model.criteria).map(lineFact) : [],
    leading: model && model.headline && LINE_MARK[model.headline.state] ? { state: model.headline.state, word: str(model.headline.word) } : null,
    /* why approval is off: the one blocker every surface says (`model.readiness.blocker`, built by blockerOf), its reason only */
    readiness: R ? { ready: R.ready === true, blocker: model.readiness.blocker && str(model.readiness.blocker.reason) ? { reason: model.readiness.blocker.reason } : null } : null,
    decision: standing && dec && dec.approved
      ? { approved: true, version: str(dec.version), movedSince: dec.movedSince === true, now: str(dec.nowVersion), actor: str(standing.actor), at: utcStamp(standing.decided_at), id: str(standing.id) }
      : { approved: false },
    checkable,
    unchecked,
    statementCount: statements.length,
    tests,
    lastRunAt: utcClock(lastRunTs),
    versionRow: row ? { id: row.id, commit: row.commit || null, unsaved: row.unsavedChanges === true ? true : (row.unsavedChanges === false ? false : null) } : null,
    versionsSeen: list(s.versions).map((x) => x && x.id).filter(Boolean),
    sessions: { count: sessions.length, agents },
    stepCount: num(change.stepCount),
    editCount: edits.length,
    commandCount: runs.length,
    noResultCount: runs.filter((r) => r && r.status === 'no_result').length,
    firstAt: stamps.length ? utcStamp(new Date(Math.min(...stamps)).toISOString()) : null,
    masking: m && typeof m === 'object' ? { masked: num(m.masked), families: Array.isArray(m.families) ? m.families.map(String) : [], complete: m.complete === true ? true : (m.complete === false ? false : null) } : null,
    readAt: utcStamp(nowIso),
  }
}

/* ------------------------------------------------------------- the host line */

const short = (h) => (typeof h === 'string' && /^[0-9a-f]{7,64}$/i.test(h) ? h.slice(0, 7) : null)

/** The host chrome's second line: the project, and the branch from git. */
function hostMeta(f, pr, tracked) {
  const project = f.project || 'this project'
  if (!tracked) return [project, ...(f.version ? [' · version ', { code: f.version }] : [' · no version recorded'])]
  const doc = pr && pr.doc
  if (!doc) return [project, pr && pr.error ? ' · the branch could not be read' : ' · reading the branch…']
  const g = doc.git || {}
  const head = short(g.head)
  if (str(g.branch)) return [project, ' · branch ', { code: g.branch }, ...(head ? [' · head ', { code: head }] : [' · no commit yet'])]
  if (g.detached === true && head) return [project, ' · no branch, head ', { code: head }]
  return [project, ' · branch not read: ' + (str(g.reason) || 'git did not say')]
}

/* ------------------------------------------------------------ the check run */

/** Section 3: what the ThruLine Action would post for this Change's folder. */
function checkedOf(pr, checked) {
  const doc = pr && pr.doc
  const run = checked && checked.check_run
  return doc && checked && str(checked.fingerprint) &&
    checked.fingerprint === doc.receipts?.newest_fingerprint &&
    checked.receipt === doc.receipts?.newest && checked.head === doc.git?.head &&
    str(run?.output?.title) && str(run?.conclusion) && str(checked.badge?.aria_label) && str(checked.badge?.svg)
    ? checked : null
}

function checkRunOf(pr, checked, check) {
  const doc = pr && pr.doc
  if (!doc) {
    return {
      name: null, tag: null, readAt: null, command: null, conclusions: [],
      title: [pr && pr.error ? 'ThruLine could not read this Change’s folder, so it cannot say what the Action would find there.' : 'Reading this Change’s folder…'],
      lines: pr && pr.error ? [['Reload this page to read it again.']] : [],
    }
  }
  const name = str(doc.check_run && doc.check_run.name)
  const conclusions = list(doc.badge && doc.badge.states).filter((x) => x && str(x.conclusion) && str(x.label))
  const base = { name, readAt: utcClock(doc.read_at), conclusions, command: null }
  const folder = doc.folder || {}
  const r = doc.receipts || {}
  if (!folder.recorded || folder.present === false) {
    const why = str(folder.reason) || 'the folder for this Change could not be found'
    return { ...base, tag: null, title: [why.charAt(0).toUpperCase() + why.slice(1) + ', so this page cannot say what the Action would find there.'], lines: [] }
  }
  if (r.count == null) {
    return { ...base, tag: null, title: ['Whether the folder holds a receipt could not be read: ', { rec: str(r.reason) || 'no reason given' }, '.'], lines: [] }
  }
  if (r.count === 0) {
    return {
      ...base,
      tag: 'no receipt',
      title: ['No check run: this folder holds no receipt.'],
      lines: [['The Action checks the newest receipt in ', { code: RECEIPTS_DIR }, '. This folder has none, so it would stop with “No receipt” and post no check run, no comment and no badge.']],
    }
  }
  const newest = str(r.newest)
  const verified = checkedOf(pr, checked)
  if (verified) return {
    ...base,
    tag: verified.check_run.conclusion,
    title: [verified.check_run.output.title],
    lines: [[verified.check_run.conclusion]],
    receipt: newest,
    canCheck: true,
    checking: !!check?.busy,
    checkError: str(check?.error),
  }
  return {
    ...base,
    tag: 'not checked here',
    title: ['Not checked here: ', newest ? { code: RECEIPTS_DIR + newest } : 'the newest receipt', '.'],
    lines: [
      ['The Action would check ', newest ? { code: newest } : 'the newest receipt', ' on the pull request’s head and post the conclusion that check reaches.' + (r.count > 1 ? ' ' + countOf(r.count, 'receipt is', 'receipts are') + ' here; it takes the newest.' : '')],
      ['Opening this page does not check a receipt. Press Check this receipt to use the CLI verifier with temporary Git storage outside your repository, or check it yourself:'],
    ],
    command: newest && /^[A-Za-z0-9._-]{1,200}$/.test(newest) ? 'thruline verify-receipt ' + RECEIPTS_DIR + newest + ' --offline --format check-run' : null,
    receipt: newest,
    canCheck: !!newest,
    checking: !!check?.busy,
    checkError: str(check?.error),
  }
}

/* ------------------------------------------------------- trailer and badges */

function trailerOf(f) {
  const d = f.decision
  if (!d.approved || !d.version) {
    return { text: null, subject: f.title, note: 'No approval stands for this Change, so there is no trailer yet. Approving a version (Pro) gives you one; ThruLine never commits.' }
  }
  const when = d.at ? ' at ' + d.at : ''
  const who = d.actor ? ' by ' + d.actor : ''
  return {
    text: TRAILER_KEY + d.version,
    subject: f.title,
    note: 'It comes with the approval given in ThruLine' + who + when + ', and names the one version approved.'
      + (d.movedSince ? ' The code has changed since: it names ' + d.version + ', and the record now stands at ' + (d.now || 'a later version') + '.' : '')
      + ' ThruLine never commits.',
  }
}

function badgeOf(pr, checked) {
  const doc = pr && pr.doc
  const verified = checkedOf(pr, checked)
  if (verified) return { image: verified.badge, lines: [['This is the badge the CLI generated when you checked ', { code: RECEIPTS_DIR + verified.receipt }, '.']] }
  const words = list(doc && doc.badge && doc.badge.states).map((x) => str(x && x.label)).filter(Boolean)
  const said = words.length ? words.slice(0, -1).join(', ') + (words.length > 1 ? ' or ' : '') + words[words.length - 1] : null
  const lines = [['No badge is drawn here. The CLI draws it from a checked receipt: ', { code: 'thruline verify-receipt <receipt> --offline --format badge' }, said ? ', and it reads ' + said + '.' : '.']]
  if (doc && doc.receipts && doc.receipts.count === 0) lines.push(['This folder holds no receipt, so there is no badge at all.'])
  return { lines }
}

/* ------------------------------------------------------ the edited-in-place panel */

/** What changed between the first read on this page and now, in words, from
 *  typed fields only. Empty when nothing did. */
export function changesSince(first, now) {
  if (!first || !now) return []
  const out = []
  if (first.version !== now.version) out.push('the version moved from ' + (first.version || 'none') + ' to ' + (now.version || 'none'))
  const runs = now.tests.filter((r) => r.status === 'finished').length - first.tests.filter((r) => r.status === 'finished').length
  if (runs > 0) out.push(countOf(runs, 'test run', 'test runs') + ' added')
  const edits = now.editCount - first.editCount
  if (edits > 0) out.push(countOf(edits, 'edit', 'edits') + ' added')
  const said = now.statementCount - first.statementCount
  if (said > 0) out.push(countOf(said, 'statement', 'statements') + ' added')
  const before = new Map(first.lines.map((l) => [l.n, l]))
  for (const l of now.lines) {
    const b = before.get(l.n)
    if (b && b.state !== l.state) out.push('line ' + l.n + ' now reads ' + (l.word || 'no state') + ' (was ' + (b.word || 'no state') + ')')
  }
  const da = first.decision, db = now.decision
  if (!da.approved && db.approved) out.push('an approval of ' + (db.version || 'a version') + ' was recorded')
  else if (da.approved && !db.approved) out.push('the approval of ' + (da.version || 'a version') + ' was withdrawn')
  else if (da.approved && db.approved && da.id !== db.id) out.push('a new approval of ' + (db.version || 'a version') + ' was recorded')
  return out
}

function editedOf(first, f, note) {
  if (!first || !first.facts) return null
  const since = utcClock(first.readAt) || 'the first read'
  const at = f.readAt ? f.readAt.slice(11) : null
  const changes = changesSince(first.facts, f)
  if (!changes.length) {
    return {
      title: (at ? at + ' · ' : '') + 'nothing to edit yet',
      lines: [],
      delta: 'Nothing in the record has changed since ' + since + ', when this page first read it. When it does, the note above is written again from the record, under the same hidden marker.',
    }
  }
  return {
    title: (at ? at + ' · ' : '') + 'the same note, edited in place',
    lines: note.firstLines,
    delta: 'Since ' + since + ', when this page first read the record: ' + changes.join(' · ') + '.',
  }
}

/* -------------------------------------------------------------- the page */

const UNTRACKED = Object.freeze({
  why: 'This work is not tracked as a Change, so ThruLine has no note for its pull request.',
  what: 'A tracked Change’s note has one row for each line of what counts as done and where it stands on its version, the approval it waits on, and the trailer an approval gives you.',
  next: 'Tracking it records a Change named after your request, with this work linked to it as yours. It approves nothing and sends nothing to the agent.',
  busy: 'Tracking this work as a Change. Nothing is approved and nothing is sent to the agent.',
})
export { UNTRACKED }

/**
 * The screen's data: the board, the host chrome a preview can honestly draw,
 * the note and its Markdown, the setup line, and — for a tracked Change — the
 * approval it waits on, what the Action would post, the trailer and the badge.
 */
export function prFaceFrom(snap, ctx = {}) {
  const tracked = trackedOf(ctx)
  const facts = noteFacts(snap, ctx)
  const pr = ctx.pr || null
  const host = { title: facts.title, number: null, pill: 'Preview', author: null, project: facts.project, version: facts.version, meta: hostMeta(facts, pr, !!tracked) }
  const base = {
    live: true,
    tracked: !!tracked,
    board: { crumbKind: 'preview', crumb: facts.project },
    host,
    comment: { readAt: facts.readAt },
    facts,
  }
  if (!tracked) {
    const t = ctx.track || {}
    return {
      ...base,
      note: null, markdown: null, setup: null, copy: null, approval: null,
      checkRun: null, trailer: null, badge: null, edited: null,
      untracked: { why: UNTRACKED.why, what: UNTRACKED.what, next: t.busy ? UNTRACKED.busy : UNTRACKED.next, busy: !!t.busy, said: str(t.said) },
    }
  }
  const note = composeNote(facts)
  const doc = pr && pr.doc
  const setup = setupOf({ newest: doc && doc.receipts ? doc.receipts.newest : null, checkName: doc && doc.check_run ? doc.check_run.name : null })
  const d = facts.decision
  return {
    ...base,
    note,
    markdown: markdownOf(note),
    setup,
    copy: copyOf(note, setup),
    approval: {
      href: routeHref('approve', { change: tracked.id || facts.changeId, project: facts.projectId || undefined }),
      label: d.approved ? 'Approved on ' + d.version + (d.movedSince ? ' · code changed since' : '') : 'The approval it waits on',
      stands: !!d.approved,
    },
    checkRun: checkRunOf(pr, ctx.checked, ctx.check),
    trailer: trailerOf(facts),
    badge: badgeOf(pr, ctx.checked),
    edited: editedOf(ctx.first, facts, note),
    untracked: null,
  }
}

export default prFaceFrom
