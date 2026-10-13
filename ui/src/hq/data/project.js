/* The one project model (HQ Next, W2-A).
 *
 * PURE: no fetch, no window, no clock of its own — `now` is passed in. It turns
 * the engine's documents (docs/hq-next/API.md) into what three surfaces draw:
 *
 *   inboxData   GET /api/hq/projects + /api/hq/projects/<id> + /api/storage
 *               → the Inbox's data contract (screens/Inbox/DATA.md, the shape
 *               of screens/Inbox/demo.js): EVERY thread of the project is one
 *               row, in the group of its `whose_move`.
 *   sidebarProjects                 → the Inbox sidebar's project list.
 *   firstRunData  + GET /api/hq/load → First run's data contract
 *               (screens/Onboarding/DATA.md).
 *
 * Every number is a typed field of a served document. A fact the engine does
 * not serve is left absent (never 0, never invented); the screens draw absence
 * in the design's own words.
 *
 * Whose move → Inbox group (the engine decides `whose_move`, this file only
 * files it; see API.md "Whose move"):
 *   you,   basis agent_asked                      → Waiting on you (a question only you can answer)
 *   you,   basis claim_without_passing_test /
 *          last_test_failed                       → Needs a check: "Needs a re-check" when a file
 *                                                  changed after the last passing test run,
 *                                                  else "Not proven"
 *   agent  (its last row is within 10 minutes)    → Recording now
 *   done                                          → Nothing needed (collapsed)
 *   you or done, and a tracked Change of the thread is ready for a decision
 *   (`ready`, data/inbox-ready.js; `filesAsApprove`) → Waiting on you, as the approve row
 * "Your move" — the headline, the filter, the footer and the sidebar badge on
 * EVERY screen — is ONE count, `yourMove` (C46 §1.1): the engine's count of the
 * person's threads, plus each finished thread a ready Change moves into "Waiting
 * on you" (one more of the person's move, one fewer finished). "The agent's" is
 * the engine's `agent` count; "Nothing needed" what is left finished. Every
 * row-count equals the rows it shows, and every screen's badge is the same number.
 *
 * NOT-APPROVAL-WORDS: `filesAsApprove`, `readingOf` and `yourMove` file and count a tracked
 * Change that IS ready for a decision (inbox-ready.js's reading); nothing here says why approval
 * is off or draws a Change that is not ready.
 */
import { href } from '../routes.js'
import { UNTITLED, hhmm, unwrapPasted } from './change.js'
import { homePath } from './paths.js'
import { loadWatch } from './load-watch.js'
import { fmtCount } from './count.js'
import { toldOf } from './told.js'
import { faceBar } from './face.js'
import { hideRows } from './inbox-hide.js'

/* ---- the frame's shared sentences: one derivation, every screen ---- */

/** Where the recording signal is read: recordings written in the last few
 *  minutes, per agent, counted from file times (GET, read only). */
export const LANES = '/api/lanes'

/** The one privacy sentence the status bar says on every screen. */
export const PRIVACY = 'Nothing leaves this machine'

const BLUE = '#52A8FF'

/**
 * The top bar's recording pill, on every screen. "Recording" (the design's
 * pulsing blue dot) only when the record shows activity in the last few
 * minutes. On a Change screen (a `change` is in hand) the pill describes THAT
 * Change, as the design's `change.recording` does (§2.1b, §4 model): only its
 * own last step being that recent makes it say Recording, so another agent
 * session elsewhere on the machine cannot make a Change that is the person's
 * move look recorded, and the pill agrees with the status bar's "· recording".
 * Anywhere else (the home screens, or a Change not read yet) it follows
 * `/api/lanes`: an agent recording written within its window. Otherwise a quiet
 * "Not recording now" with a still, hollow dot. It never counts projects and
 * never says "not known".
 *
 * @param {{lanes?: object|null, change?: {recording?: boolean|null}|null}} signal
 * @returns {{on: boolean, text: string, title: string, color: string, dot: string, anim: string, ring: string}}
 */
export function recordingPill({ lanes = null, change = null } = {}) {
  const live = (k) => (lanes && lanes[k] && typeof lanes[k].live === 'number' && lanes[k].live > 0 ? lanes[k].live : 0)
  const ofChange = !!change && typeof change === 'object'
  const n = ofChange ? 0 : live('claude_code') + live('codex')
  const win = lanes && typeof lanes.window_minutes === 'number' ? lanes.window_minutes : null
  const mine = ofChange && change.recording === true
  const on = n > 0 || mine
  const within = win != null ? `the last ${win} minutes` : 'the last few minutes'
  let title
  if (n > 0) title = `${fmtCount(n)} agent ${n === 1 ? 'session was' : 'sessions were'} written to on this machine in ${within}.`
  else if (mine) title = 'The last step of this work was recorded in the last few minutes.'
  else if (ofChange) title = 'Nothing in the record shows an agent working on this Change in the last few minutes.'
  else if (lanes) title = `No agent session on this machine was written to in ${within}.`
  else title = 'Nothing in the record shows an agent working in the last few minutes.'
  return on
    ? { on, text: 'Recording', title, color: BLUE, dot: BLUE, anim: 'tl-pulse 1.6s ease-in-out infinite', ring: 'none' }
    : { on, text: 'Not recording now', title, color: '#A1A1AA', dot: 'transparent', anim: 'none', ring: 'inset 0 0 0 1px #82828C' }
}

/**
 * An artboard's logic on a live address, with the frame's shared values
 * added to what it renders: `rec` (the recording pill, `recordingPill`) and
 * `privacy` (the one privacy sentence). The signal arrives as the screen's
 * `frame` prop: `{lanes, change}`. The design gallery runs the bare logic, so
 * the canvas renders exactly as drawn there.
 */
/* Keys that move focus or leave a field: they never open the palette. */
const PASS_KEYS = new Set(['Tab', 'Shift', 'Escape', 'Alt', 'Control', 'Meta', 'CapsLock'])

/**
 * Find, ask, act from the frame (C30 R-LINKS): every "Find, ask, act" link
 * and every search field opens the palette over the screen, through the
 * app's `openCommand`, instead of following its href. Absent (undefined)
 * when the screen was not given `openCommand` — the design gallery — so the
 * canvas's markup is unchanged there.
 */
export function findHandlers(props) {
  const open = props && typeof props.openCommand === 'function' ? props.openCommand : null
  if (!open) return { openCommand: undefined, findKey: undefined }
  return {
    openCommand: (e) => {
      if (e && e.preventDefault) e.preventDefault()
      open()
    },
    findKey: (e) => {
      if (!e || PASS_KEYS.has(e.key) || ((e.metaKey || e.ctrlKey) && e.key && e.key.toLowerCase() === 'k')) return
      if (e.preventDefault) e.preventDefault()
      open()
    },
  }
}

/**
 * The home nav's addresses (C30 R-NAV), on every live screen: Changes (g c),
 * Projects (g p) and Team (g t), each keeping the project the person is in so
 * the frame (badge, current project) stays where it was. The design gallery
 * does not run withFrame, so its nav stays as the canvas drew it.
 */
export function hqNav(at) {
  const project = (at && at.project) || undefined
  return { changes: href('changes', { project }), projects: href('projects', { project }), team: href('team', { project }) }
}

export function withFrame(Logic) {
  return class LiveFrame extends Logic {
    componentDidMount() {
      if (super.componentDidMount) super.componentDidMount()
      // the one load watcher (C27): the status bar says when new recordings are being read
      this.__unload = loadWatch.subscribe((load) => {
        const line = loadLine(load)
        if (line !== this.__loadLine) { this.__loadLine = line; if (this.__redraw) this.__redraw() }
      })
    }
    componentWillUnmount() {
      if (this.__unload) this.__unload()
      if (super.componentWillUnmount) super.componentWillUnmount()
    }
    renderVals() {
      const v = super.renderVals()
      const f = (this.props && this.props.frame) || {}
      const line = f.load !== undefined ? loadLine(f.load) : (this.__loadLine || null)
      return Object.assign({}, v, { rec: recordingPill({ lanes: f.lanes || null, change: f.change || null }), privacy: PRIVACY, loadLine: line, hqNav: hqNav(this.props && this.props.at), faceBar: faceBar(this.props) }, findHandlers(this.props))
    }
  }
}

/** A served count, its thousands separated (count.js); null when not served. */
const n0 = (x) => (typeof x === 'number' && Number.isFinite(x) ? fmtCount(x) : null)

/**
 * The status bar's one progress line while ThruLine reads new recordings
 * (C27), from `GET /api/hq/load`: "Reading new recordings · 12 of 569 ·
 * 1,204 rows added". A part the load does not state is left out, never 0.
 * Null when no load is running — the status bar then says nothing about it.
 */
export function loadLine(load) {
  if (!load || !load.running) return null
  const done = n0(load.files_done)
  const total = n0(load.files_total)
  const added = n0(load.events_added)
  return ['Reading new recordings', done && total ? done + ' of ' + total : null, added ? added + ' ' + (load.events_added === 1 ? 'row' : 'rows') + ' added' : null].filter(Boolean).join(' · ')
}

/** How often (ms) the open project's view is re-read: every `ms` while a load
 *  of that project runs, so new work appears as it lands; never otherwise. */
export function viewRereadEvery(load, projectId, ms = 3000) {
  return load && load.running && projectId && (!load.project || load.project === projectId) ? ms : undefined
}

/** How often (ms) the Inbox re-reads its documents while something is being recorded. */
export const RECORDING_REREAD_MS = 5000

/** Whether a project's view shows an agent working right now: a thread whose move is the
 *  agent's, or the project's own count of them. The engine decides `whose_move`; this only reads it. */
export function isRecording(view) {
  if (!view || typeof view !== 'object') return false
  const wm = view.project && view.project.whose_move
  if (wm && typeof wm.agent === 'number' && wm.agent > 0) return true
  return Array.isArray(view.threads) && view.threads.some((t) => t && t.whose_move === 'agent')
}

/** How often (ms) to re-read the Inbox's project view: while a load runs (`viewRereadEvery`), else
 *  every `ms` while something is being recorded — the project's view shows it, or `machine` says an
 *  agent session on this machine was just written to (the lanes signal, which is how a recording
 *  that starts after the page opened is noticed) — and never otherwise. */
export function inboxRereadEvery(load, projectId, view, machine = false, ms = RECORDING_REREAD_MS) {
  const loading = viewRereadEvery(load, projectId)
  if (loading !== undefined) return loading
  return isRecording(view) || machine === true ? ms : undefined
}

/** The status bar's one storage sentence (DESIGN-SPEC §2.4, L1.1), up to
 *  the "see what" link. */
export function storageSentence(bytes) {
  const size = fmtSize(bytes)
  return size ? 'ThruLine is keeping ' + size + ' of records on this machine · ' : 'How much ThruLine keeps on this machine could not be read · '
}

/** A size a person reads in the status bar: decimal GB (MB under 1 GB). */
export function fmtSize(bytes) {
  if (typeof bytes !== 'number' || !Number.isFinite(bytes)) return null
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB'
  if (bytes >= 1e6) return Math.round(bytes / 1e6) + ' MB'
  return Math.max(1, Math.round(bytes / 1e3)) + ' KB'
}

const pad = (n) => String(n).padStart(2, '0')
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
const has = (x) => x !== undefined && x !== null
const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : undefined)

/** A recorded instant in the machine's local time, in the design's forms:
 *  "16:04" today, "yesterday", "5 Sep" this year, "5 Sep 2025" before. */
export function fmtWhen(iso, now) {
  if (typeof iso !== 'string') return undefined
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return undefined
  const n = now instanceof Date ? now : new Date(now)
  const day = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const diffDays = Math.round((day(n) - day(d)) / 86400000)
  if (diffDays === 0) return `${pad(d.getHours())}:${pad(d.getMinutes())}`
  if (diffDays === 1) return 'yesterday'
  const dm = `${d.getDate()} ${MONTHS[d.getMonth()]}`
  return d.getFullYear() === n.getFullYear() ? dm : `${dm} ${d.getFullYear()}`
}

/** "16:10": the local time of a read. */
export function fmtClock(now) {
  const n = now instanceof Date ? now : new Date(now)
  return `${pad(n.getHours())}:${pad(n.getMinutes())}`
}

/** The version chip: a commit's 7-character hash, or the engine's own
 *  working-tree id ("wt-…", work after the last commit) shortened, never
 *  dressed up as a commit. */
export function shortVersion(id) {
  if (typeof id !== 'string' || !id) return undefined
  if (/^[0-9a-f]{7,40}$/.test(id)) return id.slice(0, 7)
  if (id.startsWith('wt-')) return id.slice(0, 10)
  return id
}

/** A path as a person reads it: relative to the project root when inside it. */
export function shortPath(path, root) {
  if (typeof path !== 'string') return undefined
  if (typeof root === 'string' && root && path.startsWith(root.endsWith('/') ? root : `${root}/`)) return path.slice(root.length).replace(/^\/+/, '')
  // outside the root (another worktree, a notes folder): its last two parts, as a row has room for
  const parts = path.split('/').filter(Boolean)
  return parts.length > 2 ? `…/${parts.slice(-2).join('/')}` : homeShort(path)
}

/** "/Users/<me>/code/x" → "~/code/x" (display only; the one formatter, paths.js). */
export function homeShort(path) {
  if (typeof path !== 'string') return undefined
  return homePath(path)
}

/** What a folder's path reads as while nothing has stated where it is: the
 *  server serves the vendor's folder name as stored, which is not a path. */
export const PATH_UNKNOWN = 'path not known until the first recording'

/** Whether a project's `root` is a path some record states. A row from a
 *  server that does not say (or a fixture) is read as known. */
export const pathKnown = (p) => !!p && p.path_known !== false

/** A project's path as a person reads it: the home folder as `~`, or the one
 *  honest sentence when no record states it. Never a guess. */
export function projectPath(p) {
  if (!p) return undefined
  return pathKnown(p) ? homeShort(p.root) : PATH_UNKNOWN
}

/** A project's name as a person reads it. A project whose root is the home
 *  folder itself is named after the account; it reads "Home folder". */
export function projectName(p) {
  if (!p) return undefined
  return isHomeRoot(p) ? 'Home folder' : p.name
}

/** Whether a project's root is the home folder itself. */
export function isHomeRoot(p) {
  return !!p && typeof p.root === 'string' && homePath(p.root.replace(/\/+$/, '')) === '~'
}

const SOURCE_NAME = { claude_code: 'Claude Code', codex: 'Codex' }
const sourcesName = (sources) => {
  const names = (sources || []).map((s) => SOURCE_NAME[s] || s)
  return names.length ? names.join(' and ') : undefined
}

/** Whether a project has records: sessions recorded for it (a row that does
 *  not say counts as having them only if it names a last activity). */
export const hasRecords = (p) => !!p && (typeof p.sessions === 'number' ? p.sessions > 0 : !!p.last_activity)

/** Which project is current: `?project=` when it names one, else the most
 *  recently active project that has records (C27); none when none has. */
export function currentProjectId(projects, wanted) {
  const list = Array.isArray(projects) ? projects : []
  if (wanted && list.some((p) => p.id === wanted)) return wanted
  let best = null
  for (const p of list) if (hasRecords(p) && (!best || String(p.last_activity || '') > String(best.last_activity || ''))) best = p
  return best ? best.id : undefined
}

/** Whether a project row is whole. `GET /api/hq/projects` serves a quick row
 *  (`complete: false`) before a project's index is built: its index-only
 *  counts are null — not yet counted, never 0. An older engine sends no
 *  `complete`, and its rows are whole. */
export const isWhole = (p) => !!p && p.complete !== false

/** How often (ms) to re-read `GET /api/hq/projects`: every `ms` while any row
 *  is a quick row, and never once the document says it is complete. */
export function projectsRereadEvery(doc, ms = 2000) {
  if (!doc || typeof doc !== 'object') return undefined
  if (doc.complete === false) return ms
  if (doc.complete === undefined && Array.isArray(doc.projects) && doc.projects.some((p) => !isWhole(p))) return ms
  return undefined
}

/**
 * The Inbox badge on every screen's sidebar — ONE derivation (F2, C46 §1.1):
 * `yourMove` of the current project, the number the Inbox headline counts.
 * The current project is `currentProjectId`'s: `?project=` when it names one,
 * else the most recently active. `reading` is that project's readiness reading
 * (`readingOf`; data/your-move.js shares it between screens). Null — no badge,
 * never 0 and never a number that may be short — when the count is not served,
 * not yet counted, or the readiness is not read.
 */
export function inboxBadge(projects, wanted, reading = null) {
  const list = Array.isArray(projects) ? projects : []
  const id = currentProjectId(list, wanted)
  const p = list.find((x) => x && x.id === id)
  return p ? yourMove(p, reading) : null
}

/**
 * Whether a tracked Change that is ready for a decision (`ready`, inbox-ready.js) files a thread as
 * the Inbox's approve row: when the engine calls the thread finished, or the person's move on
 * anything but a question only they can answer. The agent's work in progress keeps its row. The one
 * rule: the Inbox's filing (fileThread) and the one count (yourMove) both ask it.
 */
export function filesAsApprove(t, ready) {
  if (!t || !ready || !ready.change || !ready.version) return false
  return t.whose_move === 'done' || (t.whose_move === 'you' && t.whose_move_basis !== 'agent_asked')
}

/**
 * A reading of one project's readiness, as `yourMove` counts from it (C46 §1.1): which tracked
 * Change is ready on which thread (`ready`, inbox-ready.js `readReadyChanges`), the engine's move of
 * each of those threads (`moves`, from the project's view), and how many looks could not finish.
 * Null when there is no project or nothing was read.
 */
export function readingOf(view, read, hides = null) {
  const project = view && view.project
  if (!project || project.id == null || !read) return null
  const ready = read.ready || {}
  const moves = {}
  const hidden = hideRows(hides, project.id)
  let hiddenYourMove = 0
  let dismissedCount = 0
  for (const t of (view && view.threads) || []) {
    if (t && ready[t.id]) moves[t.id] = { whose_move: t.whose_move, whose_move_basis: t.whose_move_basis }
    if (t && hidden && hidden.get(String(t.id))?.hidden) {
      dismissedCount += 1
      if (t.whose_move === 'you' || filesAsApprove(t, ready[t.id])) hiddenYourMove += 1
    }
  }
  return { project: String(project.id), ready, moves, hiddenYourMove, dismissedCount, failed: read.failed || 0, listFailed: !!read.listFailed }
}

/**
 * "Your move" — THE count, the one function every screen says it from (C46 §1.1; C30 P0-5): the
 * sidebar badge on every screen, and what the Inbox's and Changes' headline, chips and footer count
 * row by row. It is the engine's count of the threads that are the person's move, plus each thread
 * the engine calls finished that a ready Change files as its approve row (`filesAsApprove`).
 *
 * Null — no badge, never a number that may be short — when the row is not yet counted (a quick
 * row), when this project's readiness has not been read (`reading`, `readingOf`'s shape, null), or
 * when a look could not finish (the list of Changes, or one Change's readiness).
 *
 * @param {object} row      the project's row (`GET /api/hq/projects`, or its view's `project`)
 * @param {object|null} reading
 * @returns {number|null}
 */
export function yourMove(row, reading) {
  const engine = row && isWhole(row) && row.whose_move ? row.whose_move.you : null
  if (typeof engine !== 'number' || !Number.isFinite(engine) || engine < 0) return null
  if (!reading || reading.project !== String(row.id) || reading.listFailed || reading.failed > 0 || !reading.ready) return null
  let more = 0
  for (const [id, r] of Object.entries(reading.ready)) {
    const m = reading.moves && reading.moves[id]
    if (m && m.whose_move === 'done' && filesAsApprove(m, r)) more += 1
  }
  return Math.max(0, engine + more - (reading.hiddenYourMove || 0))
}

/** The badge's words for a screen reader, or null with no badge. */
export function inboxBadgeAria(n) {
  return typeof n === 'number' ? `${fmtCount(n)} ${n === 1 ? 'Change is' : 'Changes are'} your move` : null
}

/** By last activity, newest first; a row without one last. */
const byRecent = (a, b) => String(b.last_activity || '').localeCompare(String(a.last_activity || ''))

/** The sidebar's project list: the current project first, marked current;
 *  then the others by last activity, newest first. */
export function sidebarProjects(projects, now, current) {
  const list = (Array.isArray(projects) ? projects : []).filter(Boolean)
  const cur = currentProjectId(list, current)
  const ordered = list.filter((p) => p.id === cur).concat(list.filter((p) => p.id !== cur).sort(byRecent))
  return ordered.map((p) => ({
    id: p.id,
    name: projectName(p),
    current: p.id === cur,
    complete: isWhole(p),
    // a quick row's whose_move is null: whether it is recording is not yet counted
    recording: isWhole(p) ? !!(p.whose_move && p.whose_move.agent > 0) : null,
    lastRecordedOn: fmtWhen(p.last_activity, now),
    changeCount: num(p.threads),
    href: href('inbox', { project: p.id }),
  }))
}

/** One thread, filed into the Inbox's data contract. Mutates `out`.
 *  `ready` is the tracked Change that is ready for a decision on this thread, if one is
 *  (data/inbox-ready.js): `{change, version, criteria}`. */
function fileThread(t, project, now, out, ready, tracked) {
  const pid = project.id
  const nowMs = (now instanceof Date ? now : new Date(now)).getTime()
  const last = t.last_activity_at
  const base = {
    id: t.id,
    title: unwrapPasted(t.title) || UNTITLED,
    project: projectName(project),
    version: shortVersion(t.current_version_id),
    lastActive: fmtWhen(last, now),
    sessions: num(t.sessions),
    href: href('brief', { change: t.id, project: pid }),
    /* The row is a thread, and every address it carries names the thread (with the project, so nothing
     * is searched for): ↵, Check it again, Tell the agent and Copy it again. Which tracked Change the
     * sheet reads is the sheet's to say (data/use-change.js `findTracked`); the Inbox learns a thread's
     * tracked Change from a separate read that lands after the row is drawn, so an address that used
     * it would change under a person's finger. The one address that names a Change is Approve, which
     * must be that Change (`approveHref`, below). */
    checkHref: href('check', { change: t.id, project: pid }),
  }
  /* A tracked Change that is ready for a decision is the person's move: its row is the approve
   * row, in place of the thread's own (a not-proven or waiting row, or a finished item). Two
   * threads keep what they are: one the agent is working on right now, and a question only the
   * person can answer (`filesAsApprove`, the rule `yourMove` counts by). */
  if (filesAsApprove(t, ready)) {
    out.changes[t.id] = Object.assign(base, {
      move: 'you',
      state: 'ready',
      version: ready.version,
      sessionEndedAt: base.lastActive,
      criteria: ready.criteria || undefined,
      // the tracked Change's own address: the Approve sheet reads that Change, not whichever one the thread resolves to
      href: href('brief', { change: ready.change, project: pid }),
      approveHref: href('approve', { change: ready.change, project: pid }),
    })
    return
  }
  if (t.whose_move === 'you') {
    if (t.whose_move_basis === 'agent_asked') {
      out.changes[t.id] = Object.assign(base, { move: 'you', state: 'asked' })
      const lastMs = Date.parse(last)
      const command = t.waiting_command || null
      out.prompts.push({
        id: `ask-${t.id}`,
        change: t.id,
        asked: true,
        requestedAt: fmtWhen(command && command.at || last, now),
        waitingMinutes: Number.isNaN(lastMs) ? undefined : Math.max(0, (nowMs - lastMs) / 60000),
        command: command && command.command || undefined,
        step: command && command.step || undefined,
        effect: command && /^\s*rm\s+-[^\n]*r/.test(command.command || '') ? 'removes-directory' : undefined,
        href: base.href,
      })
      return
    }
    const proof = t.proof || {}
    const edited = Array.isArray(proof.edited_after_last_pass) ? proof.edited_after_last_pass : []
    const recheck = edited.length > 0 && has(proof.last_passing_test_at)
    // C30 R-LEDGER: the person copied a note for the agent (or said they pasted it) and no work
    // has been recorded on the thread since → the Inbox FILES it under "Waiting on the agent".
    // Only the filing moves: `move` stays the engine's `whose_move` ('you'), so every count
    // — the headline, the filters, the sidebar badge — is still the engine's. Nothing was sent.
    const told = toldOf(t.told_agent, now)
    out.changes[t.id] = Object.assign(base, {
      move: 'you',
      state: recheck ? 'recheck' : 'notProven',
      changedFile: recheck ? shortPath(edited[0], project.root) : undefined,
      changedFiles: recheck ? edited.length : undefined,
      lastTestFailed: t.whose_move_basis === 'last_test_failed',
      // where the proof stops on the ①–⑥ chain (DESIGN-SPEC §4.0): a file changed after
      // the passing run → ⑤ RE-CHECKED; nothing passing behind the claim → ② RESTS ON
      stops: [recheck ? { link: 5, name: 'RE-CHECKED' } : { link: 2, name: 'RESTS ON' }],
      ...(t.latest_claim ? { claim: { agent: t.latest_claim.agent, quotes: [t.latest_claim.text], /* when it was said: the clock time, as the Brief says it ("at 16:03 · step 52"), not the row's day word */ at: hhmm(t.latest_claim.at), step: t.latest_claim.step } } : {}),
      ...(Array.isArray(t.version_thread) && t.version_thread.length
        ? { thread: t.version_thread.map((v) => ({ version: shortVersion(v.version), now: !!v.now, ran: v.ran, events: Array.isArray(v.events) ? v.events : [] })) } : {}),
      ...(tracked && typeof tracked === 'object' && tracked.criteria ? { criteria: tracked.criteria, needEvidence: tracked.criteria.lines.filter((line) => line.state !== 'proven').length } : {}),
      ...((tracked && tracked.checkCommand || proof.last_test && proof.last_test.command)
        ? { check: { command: tracked && tracked.checkCommand || proof.last_test.command, reachesLines: tracked && tracked.criteria && tracked.criteria.lines.length, asks: 'once for this project' } } : {}),
    }, told && told.waiting && told.when ? { noteCopiedAt: told.when, toldAct: told.act } : {})
    return
  }
  if (t.whose_move === 'agent') {
    out.changes[t.id] = Object.assign(base, { move: 'agent', state: 'working' })
    out.sessions.push({
      id: `live-${t.id}`,
      change: t.id,
      project: projectName(project),
      startedAt: fmtWhen(t.started_at, now),
      lastStepAt: fmtWhen(last, now),
      href: base.href,
    })
    return
  }
  /* Approved: the person's approval stands for the version its Change is on now (`current`, from the
   * Change's own readiness: data/inbox-ready.js). Where that was not read, the approval must name this
   * thread's own version id, as it did before. */
  const stands = (a) => (a.current === undefined ? a.version === t.current_version_id : a.current === true)
  if (tracked && typeof tracked === 'object' && tracked.approved && stands(tracked.approved)) {
    out.nothingNeeded.items.push({ kind: 'approved', project: projectName(project), title: base.title, version: shortVersion(tracked.approved.version), by: tracked.approved.actor, on: fmtWhen(tracked.approved.at, now), href: base.href })
  } else {
    out.nothingNeeded.items.push({ kind: 'done', project: projectName(project), title: base.title, version: base.version, on: base.lastActive, href: base.href })
  }
}

/**
 * The Inbox's data, for one project.
 * @param {{projects?: object, view: object, storage?: object, ready?: object, readyFailed?: number, now: Date|number}} docs
 *   projects: GET /api/hq/projects · view: GET /api/hq/projects/<id> · storage: GET /api/storage
 *   ready: by thread id, the tracked Change that is ready for a decision on it (data/inbox-ready.js);
 *     null or undefined while the readiness has not been read
 *   readyFailed: how many of those looks could not finish (the list of Changes counts as one)
 */
export function inboxData({ projects, view, storage, ready, tracked = {}, readyFailed = 0, hides = null, capture = {}, now }) {
  const project = (view && view.project) || {}
  const filed = { changes: {}, prompts: [], sessions: [], nothingNeeded: { items: [], destructive: [] } }
  const hidden = hideRows(hides, project.id)
  for (const t of (view && view.threads) || []) {
    for (const command of Array.isArray(t.file_removals) ? t.file_removals : []) {
      if (command && command.command) filed.nothingNeeded.destructive.push({ command: command.command, change: unwrapPasted(t.title) || UNTITLED, at: fmtWhen(command.at, now) })
    }
    const state = hidden && hidden.get(String(t.id))
    if (state && state.hidden) {
      filed.nothingNeeded.items.push({ kind: 'dismissed', project: projectName(project), title: unwrapPasted(t.title) || UNTITLED, by: 'you', on: fmtWhen(state.recorded_at, now), row: state })
    } else fileThread(t, project, now, filed, ready && ready[t.id], tracked && tracked[t.id])
  }
  const served = projects && Array.isArray(projects.projects) ? projects.projects : [project]
  // the project in front of the person is read whole: its own document is the fuller row
  const list = served.some((p) => p && p.id === project.id) ? served.map((p) => (p && p.id === project.id ? Object.assign({}, p, project) : p)) : served
  /* The badge is the one count (`yourMove`) on this project's readiness, read with the rows above:
   * the number of rows "Your move" draws, and the number every other screen's badge says. */
  const badge = inboxBadge(list, project.id, ready == null ? null : readingOf(view, { ready, failed: readyFailed }, hides))
  const side = sidebarProjects(list, now, project.id)
  if (hidden) {
    const current = side.find((p) => p.id === project.id)
    if (current) current.dismissedCount = filed.nothingNeeded.items.filter((i) => i.kind === 'dismissed').length
  }
  return {
    readAt: fmtClock(now),
    projectId: project.id,
    projectName: projectName(project),
    projects: side,
    inboxBadge: badge,
    inboxBadgeAria: inboxBadgeAria(badge),
    hideRows: hidden ? Object.fromEntries(hidden) : {},
    threadCount: num(project.threads),
    whoseMove: project.whose_move,
    capture,
    storage: num(storage && storage.bytes) !== undefined ? { bytes: storage.bytes } : undefined,
    countedBy: null,
    ...filed,
  }
}

/**
 * What ThruLine keeps on this machine, counted ONE way for every screen that
 * says it (C29): First run's rail and Settings' "Records of your sessions"
 * both state this, from this function and nothing else.
 *
 * The basis: the sum of `sessions` over every row of `GET /api/hq/projects`,
 * and how many rows (projects) that is. `sessions` is the engine's count of
 * recordings, both agents, each sub-agent's session counted as one
 * (docs/hq-next/API.md, ProjectSummary) — a quick row (`complete: false`)
 * carries it too, read from the ledger's own session rows. A row that serves
 * no number is not counted as 0: it makes the total "at least" (`uncounted`).
 * Rows still being read are counted in `reading`, and the screens say so.
 *
 * @param {object|object[]|null} projects the document, or its `projects` array
 * @returns {{sessions: number, projects: number, uncounted: number, reading: number}|null}
 *   null when no project list is served (the screens then say nothing about it)
 */
export function sessionsKept(projects) {
  const list = Array.isArray(projects) ? projects : projects && Array.isArray(projects.projects) ? projects.projects : null
  if (!list) return null
  const rows = list.filter(Boolean)
  let sessions = 0
  let uncounted = 0
  let reading = 0
  for (const p of rows) {
    if (num(p.sessions) !== undefined) sessions += p.sessions
    else uncounted += 1
    if (!isWhole(p)) reading += 1
  }
  return { sessions, projects: rows.length, uncounted, reading }
}

/** "76,746 sessions in 72 projects" (or "at least …" when a row served no count). */
export function sessionsKeptPhrase(k) {
  if (!k) return null
  return (k.uncounted ? 'at least ' : '') + countWord(k.sessions, 'session', 'sessions') + ' in ' + countWord(k.projects, 'project', 'projects')
}

/** The basis, said beside the number on both screens. */
export const SESSIONS_BASIS = 'Each sub-agent’s session counts as one.'

/** What is still being read, said beside the number; '' when nothing is. */
export function sessionsKeptReading(k) {
  return k && k.reading ? 'Still counting ' + fmtCount(k.reading) + ' of the ' + fmtCount(k.projects) + ' projects.' : ''
}

/** First run's rail: "ThruLine is keeping the record of 76,746 sessions in 72 projects on this machine. …" */
export function sessionsKeptSentence(k) {
  if (!k || (k.sessions === 0 && !k.uncounted)) return 'ThruLine is keeping nothing on this machine yet.'
  return ['ThruLine is keeping the record of ' + sessionsKeptPhrase(k) + ' on this machine.', SESSIONS_BASIS, sessionsKeptReading(k)].filter(Boolean).join(' ')
}

const countWord = (n, one, many) => fmtCount(n) + ' ' + (n === 1 ? one : many)

/**
 * The Inbox while its project's view is still being read (C29 R1): the frame
 * at once — top bar, the sidebar's project list from `GET /api/hq/projects`,
 * which answers in milliseconds, and the status bar with the load's progress —
 * and only the list area says it is reading, naming the project. No row, no
 * count: nothing here says "nothing needs you", and no zero is drawn. The
 * sidebar badge is drawn only when the list already counts something there.
 *
 * @param {{projects?: object|null, projectId?: string, storage?: object|null, now: Date|number, reading?: object|null}} docs
 *   reading: the project's kept readiness reading (data/your-move.js), when a screen has read one
 */
export function inboxReading({ projects, projectId, storage, now, reading = null }) {
  const list = projects && Array.isArray(projects.projects) ? projects.projects : []
  const project = list.find((p) => p && p.id === projectId) || null
  const badge = inboxBadge(list, projectId, reading)
  const shown = typeof badge === 'number' && badge > 0 ? badge : null
  return {
    reading: true,
    readAt: fmtClock(now),
    projectId: projectId || undefined,
    projectName: project ? projectName(project) : undefined,
    projects: sidebarProjects(list, now, projectId),
    inboxBadge: shown,
    inboxBadgeAria: inboxBadgeAria(shown),
    threadCount: undefined,
    whoseMove: undefined,
    capture: {},
    storage: num(storage && storage.bytes) !== undefined ? { bytes: storage.bytes } : undefined,
    countedBy: null,
    changes: {},
    prompts: [],
    sessions: [],
    nothingNeeded: { items: [], destructive: [] },
  }
}

/** The list area's one line while reading: "Reading the record of thruline…". */
export function readingLine(name) {
  return name ? 'Reading the record of ' + name + '…' : 'Reading the record…'
}

/** What a load found, for one project: the founder's "every prompt, every line, every run". */
export function foundIn(p) {
  if (!p) return undefined
  return {
    sessions: num(p.sessions),
    prompts: num(p.prompts),
    edits: num(p.edits),
    commands: num(p.commands && p.commands.total),
    tests: num(p.tests && p.tests.total),
    claims: num(p.claims),
    threads: num(p.threads),
  }
}

/** The load's progress, as First run draws it. `load` is GET /api/hq/load. */
export function loadProgress(load, projectId) {
  if (!load || typeof load !== 'object') return undefined
  const mine = !load.project || load.project === projectId
  return {
    running: !!load.running && mine,
    finished: !load.running && mine && has(load.finished_at),
    other: !!load.running && !mine,
    filesTotal: num(load.files_total),
    filesDone: num(load.files_done),
    eventsAdded: num(load.events_added),
    errors: Array.isArray(load.errors) ? load.errors.length : undefined,
  }
}

/**
 * First run's data. Every folder is an HQ project: a repository root where an
 * agent has worked (the engine only makes a project of a git repository root,
 * so `git` is a served fact). Whether ThruLine's hooks are installed for a
 * folder is not served over HTTP, so `capture` is absent.
 */
export function firstRunData({ projects, load, storage, now }) {
  const list = projects && Array.isArray(projects.projects) ? projects.projects : []
  const kept = sessionsKept(projects)
  return {
    home: storage && typeof storage.path === 'string' ? homeShort(storage.path) : undefined,
    /* C26: by last activity, newest first; the home folder last, and never the
     * default — the default is the most recently active other folder, and with
     * only the home folder nothing is chosen for the person. */
    folders: list.slice().sort((a, b) => Number(isHomeRoot(a)) - Number(isHomeRoot(b)) || byRecent(a, b)).map((p) => ({
      id: p.id,
      home: isHomeRoot(p),
      name: projectName(p),
      path: projectPath(p),
      pathKnown: pathKnown(p),
      agent: sourcesName(p.sources),
      // C30 P0-7: nothing served says whether the folder is a Git repository (the engine joins a
      // folder to the outermost repository above it when there is one, and keeps its own root when
      // there is none), so this is unknown until the capture-health route (C30 §R-ROUTES) says
      git: null,
      recorded: (num(p.sessions) || 0) > 0,
      complete: isWhole(p),
      capture: null,
      firstSession: null,
      found: foundIn(p),
      lastActive: fmtWhen(p.last_activity, now),
    })),
    defaultFolder: (list.filter((p) => !isHomeRoot(p)).sort(byRecent)[0] || {}).id || null,
    // C29: the one derivation Settings states too (sessionsKept)
    storage: kept && kept.projects ? { sessions: kept.sessions, projects: kept.projects, line: sessionsKeptSentence(kept) } : { sessions: undefined },
    load: load ? Object.assign({ project: load.project || null }, loadProgress(load, load.project)) : undefined,
  }
}
