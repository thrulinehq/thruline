/* The one Change model.
 *
 *   changeSnapshot(threadView, tracked, now, options) → snapshot
 *
 * PURE: no fetch, no window, no clock but `now`. It turns one served ThreadView
 * (`GET /api/hq/projects/<p>/threads/<t>`, docs/hq-next/API.md) — and, when a
 * person has tracked this work as a Change, that Change's own derived model —
 * into one snapshot in exactly the shape of
 * `screens/Change-Brief/demo.js`'s DEMO (minus `states`). Every Change screen
 * reads its facts from this snapshot through its own `from-change.js`; no screen
 * derives a fact this file holds, so one question cannot get two answers (F2).
 *
 * The snapshot's contract is `screens/Change-Brief/DATA.md`. Fields are ADDED
 * here, never renamed; each addition is marked "added W2-B" there.
 *
 * Rules this file keeps (CLAUDE.md, WAVE2-CARD.md):
 * - Every number is a typed field of the served document, or a position in it
 *   (a step is a row's place in the thread's record). A count the record does
 *   not hold is `null`, never 0.
 * - The proof chain follows DESIGN-SPEC.md §4.0: ① said → ② rests on → ③ ran on
 *   → ④ since → ⑤ re-checked → ⑥ so. Nothing here is a colour; the Brief's
 *   logic draws green only at ⑥ and ◭ only at ④.
 * - Criteria, coverage, readiness and decisions come from `tracked` (the
 *   existing `deriveChangeModel`), never re-derived. Without it, what counts as
 *   done is not accepted, and the snapshot says so.
 */

import { displayPath, homeless, homePath, isExclusionMarker, repoTops } from './paths.js'
import { fmtCount } from './count.js'
import { notesOf } from './notes.js'
import { secondsOf } from './seconds.js'
import { objectionsOf } from './objections.js'
import { toldOf } from './told.js'

const pad = (n) => String(n).padStart(2, '0')

/** HH:MM in this machine's local time, or null. */
export function hhmm(ts) {
  if (!ts) return null
  const d = new Date(ts)
  if (Number.isNaN(d.getTime())) return null
  return pad(d.getHours()) + ':' + pad(d.getMinutes())
}

/** YYYY-MM-DD in this machine's local time. */
function ymd(d) {
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate())
}

const msOf = (ts) => {
  const t = ts ? Date.parse(ts) : NaN
  return Number.isNaN(t) ? null : t
}

/** The short form a person reads for a version: the 7-character commit when
 *  there is one, else `wt-` and 7 characters of the worktree id. */
export function shortVersion(v) {
  if (!v) return null
  if (v.short) return String(v.short).slice(0, 7)
  const id = String(v.id || '')
  if (/^[0-9a-f]{7,}$/i.test(id)) return id.slice(0, 7)
  if (id.startsWith('wt-')) return id.slice(0, 10)
  return id.slice(0, 10)
}

const AGENT = { claude_code: 'Claude Code', codex: 'Codex' }
const FEED = { claude_code: 'Claude Code transcripts', codex: 'Codex transcripts' }

/**
 * Where a path sits: relative to the project root, or to one of the project's
 * member roots (another worktree of the same repository), with that
 * worktree's name beside it; a `.claude/worktrees/<name>/` path is the same.
 * A path under none of them is shown through the one display formatter
 * (paths.js `displayPath`): another checkout's `<checkout>/<rel>`, else
 * `~/…` under the home folder, never the home folder itself. Added W2-B.
 */
export function placePath(path, root, members, tops) {
  const p = String(path || '')
  const trim = (r) => String(r || '').replace(/\/$/, '')
  const base = (r) => trim(r).slice(trim(r).lastIndexOf('/') + 1)
  const roots = [trim(root)].concat((members || []).map(trim)).filter(Boolean)
  // the longest root that holds the path
  let hit = null
  for (const r of roots) if (p.startsWith(r + '/') && (!hit || r.length > hit.length)) hit = r
  if (!hit) return { file: displayPath(p, { root, members, tops }), worktree: null }
  let rest = p.slice(hit.length + 1)
  let worktree = hit === trim(root) ? null : base(hit)
  const m = rest.match(/^\.claude\/worktrees\/([^/]+)\/(.*)$/)
  if (m) { worktree = m[1]; rest = m[2] }
  return { file: rest, worktree }
}

/** Recorded words for display: the `<pasted_content …>` and
 *  `</pasted_content>` wrapper tags a prompt can carry are dropped and the
 *  words they wrap are kept. Nothing else changes. */
/** What a thread is called when its prompts hold no sentence a title can be
 *  taken from (an image pasted alone, say): said in words, never a blank row.
 *  The same words The whole story already uses. */
export const UNTITLED = 'Untitled work'

export function unwrapPasted(text) {
  if (text == null) return text
  return String(text).replace(/<\/?pasted_content\b[^>]*>\s*/g, '').trim()
}

/** A prompt's served segments (`[{kind: typed|pasted, text}]`, wrapper
 *  tags already removed by the engine), or null from an older server. */
export function segmentsOf(prompt) {
  const seg = prompt && Array.isArray(prompt.segments) ? prompt.segments.filter((x) => x && typeof x.text === 'string') : null
  return seg && seg.length ? seg.map((x) => ({ kind: x.kind === 'pasted' ? 'pasted' : 'typed', text: x.text })) : null
}

/** A prompt's words for display: its segments joined when served, else its
 *  text with the `<pasted_content>` wrapper tags dropped. */
export function promptText(prompt) {
  if (!prompt) return null
  const seg = segmentsOf(prompt)
  if (seg) return seg.map((x) => x.text).join('\n\n')
  return unwrapPasted(prompt.text)
}

/** A claim's words for display inside ❝ ❞: markdown emphasis and code
 *  markers (`**`, `__`, backticks) stripped, a markdown link
 *  `[text](url)` read as its text, and a heading's `#` or a list item's
 *  marker dropped; a directive's markup — a container `:::name` line, a
 *  `::name{…}` line, a leaf `:name{…}` or `:name[words]{…}` — dropped,
 *  keeping any words it carries; the words are unchanged. The verbatim
 *  words stay in `text`. */
export function plainText(text) {
  if (isExclusionMarker(text)) return text
  return stripDirectives(String(text == null ? '' : text))
    .replace(/!?\[([^\]\n]*)\]\([^)\n]*\)/g, '$1')
    .replace(/^[ \t]{0,3}#{1,6}[ \t]+/gm, '')
    .replace(/^[ \t]*(?:[-*+•]|\d{1,3}[.)])[ \t]+/gm, '')
    .replace(/\*\*|__|`/g, '')
}

/** Directive markup (the generic-directive form Codex writes, e.g.
 *  `:codex-annotation{index="1"}`, `::git-stage{cwd="…"}`, `:::note`)
 *  removed; the words in a `[label]` are kept. */
export function stripDirectives(text) {
  return String(text == null ? '' : text)
    // a whole line that is only a container fence or a text/leaf directive
    .replace(/^[ \t]*:{2,}[A-Za-z][\w-]*(?:\[[^\]\n]*\])?(?:\{[^}\n]*\})?[ \t]*(?:\n|$)/gm, '')
    .replace(/^[ \t]*:{3,}[ \t]*(?:\n|$)/gm, '')
    // inline: `:name[words]{attrs}` keeps its words; `:name{attrs}` goes
    .replace(/(^|[^\w:/])::?([A-Za-z][\w-]*)\[([^\]\n]*)\](?:\{[^}\n]*\})?/g, (_m, lead, _n, words) => lead + words)
    .replace(/(^|[^\w:/])::?[A-Za-z][\w-]*\{[^}\n]*\}[ \t]*/g, '$1')
    .replace(/^[ \t]+/, '')
}

/** The paths a test command names, when it names any: tokens after the
 *  runner that look like a path or a file. Null when it names none. */
export function scopeOf(command) {
  const segs = String(command || '').split(/&&|\|\||;|\|/)
  for (const seg of segs) {
    const words = seg.trim().split(/\s+/).filter(Boolean)
    const at = words.findIndex((w) => /^(pytest|cargo|npm|npx|node|vitest|jest|go|pnpm|yarn|bun|deno|python3?|uv|make|rspec|phpunit|mix|ctest|dotnet|swift|mvn|gradle|\.\/gradlew)$/.test(w))
    if (at < 0) continue
    const paths = words.slice(at + 1).filter((w) => !w.startsWith('-') && !/[<>$'"*{}()=]/.test(w) && (w.includes('/') || /\.[a-z0-9]{1,5}$/i.test(w)))
    if (paths.length) return paths
  }
  return null
}

/* A test run passes when it succeeded and its tally names no failure; it
 * fails when it failed or its tally names one (API.md, "The verdict rule").
 * The runs here are the snapshot's own rows, which carry the tally as
 * `passed`/`failed` — a run that exited 0 while printing "1 failed" fails. */
const passing = (r) => !!r && r.outcome === 'succeeded' && !(typeof r.failed === 'number' && r.failed > 0)
const failing = (r) => !!r && (r.outcome === 'failed' || (typeof r.failed === 'number' && r.failed > 0))

const VERDICTS = new Set(['proven', 'not_proven', 'contradicted', 'recorded'])

/**
 * The engine is the one rule (crates/core/src/hq/statements.rs): a claim the
 * server served with a `verdict` keeps that verdict, and the chain this file
 * derived is made to agree with it. The rule in `proofOf` is only the
 * fallback for an older server that serves no verdict.
 */
function withServedVerdict(proof, claim) {
  const served = claim && VERDICTS.has(claim.verdict) ? claim.verdict : null
  if (!served) return proof
  const basis = claim.verdict_basis || null
  if (served === proof.verdict) return Object.assign({}, proof, { basis })
  let stopsAt = proof.stopsAt
  if (served === 'proven' || served === 'contradicted') stopsAt = null
  else if (served === 'not_proven') stopsAt = basis === 'edited_since_run' || basis === 'run_on_older_version' ? 5 : (proof.stopsAt || 2)
  return Object.assign({}, proof, { verdict: served, stopsAt, basis })
}

/**
 * @param {object} view   a ThreadView
 * @param {object|null} tracked  `{change, hq, proposals}` — the tracked Change's
 *   served document and its `deriveChangeModel` result — or null
 * @param {Date|number|string} now
 * @param {{readVersion?: string, loadedAt?: string, storageBytes?: number|null}} [options]
 */
export function changeSnapshot(view, tracked, now, options = {}) {
  const v = view || {}
  const at = now instanceof Date ? now : new Date(now == null ? Date.now() : now)
  const root = v.project && v.project.root
  const members = options.members || []
  const thread = v.thread || {}
  /* the project's top folders, from every path the record names inside it:
   * how another checkout of the same repository is recognised (paths.js) */
  const tops = repoTops([].concat((v.edits || []).map((e) => e.file), (thread.proof && thread.proof.edited_after_last_pass) || [], (v.files || []).map((f) => f && (f.path || f.file))), root)
  const where = (p) => placePath(p, root, members, tops)
  /* The server serves an edit's file relative to the project's recorded root
   * (API.md §Edit); an absolute path is what the OS editor link needs. A
   * relative file is joined to the recorded root and then goes through every
   * check an absolute one does — a `..` that would climb out of the root is
   * refused there, and a `~` or exclusion marker is never made into a path.
   * The joined path stays in JS: the Brief builds the link at click time. */
  const projectRoot = typeof root === 'string' && root.startsWith('/') && !root.startsWith('//') ? root.replace(/\/+$/, '') : null
  const recordedLocalPath = (file) => {
    let p = file
    if (typeof p === 'string' && p && !p.startsWith('/') && !p.startsWith('~') && !isExclusionMarker(p)) p = projectRoot ? projectRoot + '/' + p : null
    if (typeof p !== 'string' || !p.startsWith('/') || p.startsWith('//') || /[\\\0\r\n]/.test(p)
      || p.split('/').some((part) => part === '.' || part === '..') || isExclusionMarker(p)) return null
    const roots = [root, ...members].filter((r) => typeof r === 'string' && r.startsWith('/'))
    return roots.some((r) => p.startsWith(r.replace(/\/+$/, '') + '/')) ? p : null
  }

  /* ---- versions: served order, short ids a person reads ---- */
  const vmap = new Map()
  const versions = []
  const addVersion = (src) => {
    if (!src || vmap.has(src.id)) return
    const short = shortVersion(src)
    const committedHash = Boolean(src.short) || /^[0-9a-f]{7,}$/i.test(String(src.id || ''))
    const row = {
      id: short,
      fullId: src.id,
      madeByStep: null,
      recordedAtStep: null,
      commit: committedHash ? short : null,
      unsavedChanges: src.committed === false ? true : (src.committed === true ? false : null),
      message: homeless(src.message) || null,
      sessionId: src.session_id || null,
    }
    vmap.set(src.id, row)
    versions.push(row)
  }
  for (const x of v.versions || []) addVersion(x)
  const verOf = (id) => {
    if (!id) return null
    if (!vmap.has(id)) addVersion({ id })
    return vmap.get(id).id
  }
  const currentVersion = verOf(v.current_version_id || thread.current_version_id)
  // the current version is drawn last, as the canvas draws "now"
  const curIdx = versions.findIndex((x) => x.id === currentVersion)
  if (curIdx >= 0 && curIdx !== versions.length - 1) versions.push(versions.splice(curIdx, 1)[0])

  /* ---- one ordered record: a step is a row's place in it ---- */
  const ORDER = { prompt: 0, edit: 1, command: 2, claim: 3, message: 3 }
  const rows = []
  ;(v.prompts || []).forEach((o, i) => rows.push({ t: 'prompt', id: o.event_id, ts: o.ts, i, o }))
  ;(v.edits || []).forEach((o, i) => rows.push({ t: 'edit', id: o.id, ts: o.ts, i, o }))
  ;(v.commands || []).forEach((o, i) => rows.push({ t: 'command', id: o.id, ts: o.ts, i, o }))
  ;(v.claims || []).forEach((o, i) => rows.push({ t: 'claim', id: o.id, ts: o.ts, i, o }))
  const claimedEvents = new Set((v.claims || []).map((c) => c.event_id))
  ;(v.messages || []).forEach((o, i) => {
    if (!claimedEvents.has(o.event_id)) rows.push({ t: 'message', id: o.event_id, ts: o.ts, i, o })
  })
  rows.sort((a, b) => (msOf(a.ts) ?? 0) - (msOf(b.ts) ?? 0) || ORDER[a.t] - ORDER[b.t] || a.i - b.i)
  const stepOf = new Map()
  rows.forEach((r, k) => stepOf.set(r.t + ':' + r.id, k + 1))

  /* ---- sessions, request, edits, runs ---- */
  const sessions = (v.sessions || []).map((s) => ({
    id: s.id,
    agent: AGENT[s.source] || s.source || null,
    source: null, // the capture version is not served
    startedAt: hhmm(s.started_at),
    step: null,
    origin: s.origin || null,
  }))
  const agentOf = (sid) => { const s = sessions.find((x) => x.id === sid); return s ? s.agent : null }

  const firstWork = (v.prompts || []).find((p) => p.work) || (v.prompts || [])[0] || null
  const request = firstWork
    ? { id: firstWork.event_id, step: stepOf.get('prompt:' + firstWork.event_id), time: hhmm(firstWork.ts), text: homeless(promptText(firstWork)), segments: (segmentsOf(firstWork) || null) && segmentsOf(firstWork).map((x) => ({ kind: x.kind, text: homeless(x.text) })), verbatim: firstWork.text, sessionId: firstWork.session_id, source: { file: homePath(firstWork.raw_file), line: firstWork.raw_line } }
    : null

  const edits = (v.edits || []).map((e) => ({
    id: e.id,
    step: stepOf.get('edit:' + e.id),
    time: hhmm(e.ts),
    ts: e.ts,
    file: where(e.file).file, // the display path (paths.js): never the home folder
    editorPath: recordedLocalPath(e.file), // hidden from text: a recorded absolute path for the OS editor link
    worktree: where(e.file).worktree, // added W2-B
    hunks: Array.isArray(e.hunks) ? e.hunks : [], // added W2-B: as served (API.md §Edit)
    source: e.source || null, // added W2-B: structured_patch | apply_patch | request
    op: e.op || null, // added W2-B
    receipt: e.raw_file ? { file: homePath(e.raw_file), line: e.raw_line } : null, // the receipt, with the home folder as ~ // added W2-B
    makesVersion: verOf(e.version_id),
    linesAdded: e.lines_added ?? null,
    linesRemoved: e.lines_removed ?? null,
    ok: e.ok ?? null,
    sessionId: e.session_id,
  }))

  const runs = (v.commands || []).map((c) => ({
    id: c.id,
    step: stepOf.get('command:' + c.id),
    time: hhmm(c.ts),
    ts: c.ts,
    kind: c.is_test ? 'test' : 'command',
    command: homeless(c.command), // the home folder written as ~ (paths.js); still runs when pasted
    verbatim: c.command, // added polish: as recorded
    version: verOf(c.version_id),
    status: c.outcome === 'unknown' ? 'no_result' : 'finished',
    outcome: c.outcome,
    exitCode: c.exit_code ?? null,
    passed: c.tally ? c.tally.passed : null,
    failed: c.tally ? c.tally.failed : null,
    durationMs: null,
    capturedBy: 'transcript',
    scope: c.is_test ? (scopeOf(c.command) || null) && scopeOf(c.command).map((p) => (p.startsWith('/') ? where(p).file : p)) : null, // the paths the command names; null when it names none
    sessionId: c.session_id,
  }))
  const runById = new Map(runs.map((r) => [r.id, r]))
  const tests = runs.filter((r) => r.kind === 'test')

  /* ---- what counts as done: only from the tracked Change ---- */
  const criteria = criteriaOf(tracked)

  /* ---- statements and their proof chains ---- */
  const byStep = (a, b) => a.step - b.step
  edits.sort(byStep)
  runs.sort(byStep)
  const lastEdit = edits.length ? edits[edits.length - 1] : null

  const statements = (v.claims || []).map((c) => {
    const step = stepOf.get('claim:' + c.id)
    const st = {
      id: c.id,
      messageId: c.event_id,
      text: c.text,
      plain: homeless(plainText(c.text)), // added W2-B: for display inside ❝ ❞ (no markup, no directive, the home folder as ~); `text` stays verbatim
      claimKind: c.kind,
      verifier: Boolean(c.has_verifier),
      step,
      time: hhmm(c.ts),
      ts: c.ts || null, // added C30 R-RUN: when it was said, to order a check ThruLine ran (no step) against it
      sessionId: c.session_id,
      agent: agentOf(c.session_id),
      versionBefore: verOf(c.version_id),
      source: c.raw_file ? { file: homePath(c.raw_file), line: c.raw_line } : null, // the receipt, with the home folder as ~
    }
    st.proof = withServedVerdict(proofOf(st, c, { runById, tests, edits, lastEdit, currentVersion, criteria }), c)
    /* added C30 R-LEDGER: what people said was wrong with this sentence, beside the verdict above and never in
       place of it. Set only when someone objected, so a statement nobody objected to is the object it was. */
    const objections = objectionsOf(c.objections)
    if (objections.length) st.objections = objections
    return st
  }).sort(byStep)

  /* ---- messages: the full served agent text, even when no claim rule fired.
   * Older thread servers have no `messages`: the claim snippets remain a
   * fallback. An unclassified sentence stays under Also said, not a claim. */
  const messages = (v.messages || []).map((m) => {
    const claimsHere = statements.filter((st) => st.messageId === m.event_id)
    return {
      id: m.event_id, step: claimsHere.length ? claimsHere[0].step : stepOf.get('message:' + m.event_id),
      time: hhmm(m.ts), sessionId: m.session_id, agent: agentOf(m.session_id),
      text: m.text, plain: homeless(plainText(m.text)),
    }
  })
  for (const st of statements) {
    let m = messages.find((x) => x.id === st.messageId)
    if (!m) { m = { id: st.messageId, step: st.step, time: st.time, sessionId: st.sessionId, agent: st.agent, text: '', plain: '' }; messages.push(m) }
    if (!(v.messages || []).some((raw) => raw.event_id === st.messageId)) {
      m.text = m.text ? m.text + ' ' + st.text : st.text
      m.plain = m.plain ? m.plain + ' ' + st.plain : st.plain // fallback for an older thread server
    }
  }
  messages.sort((a, b) => (a.step ?? 0) - (b.step ?? 0))

  /* The statement the brief is about: the latest checkable one that is not
   * proven, else the latest checkable one, else the latest. */
  const verif = statements.filter((s) => s.verifier)
  const open = verif.filter((s) => s.proof.verdict !== 'proven')
  const brief = { statementId: (open[open.length - 1] || verif[verif.length - 1] || statements[statements.length - 1] || { id: null }).id }
  const B = statements.find((s) => s.id === brief.statementId) || null

  /* ---- the Change ---- */
  const readVersion = options.readVersion || currentVersion
  const hq = tracked && tracked.hq
  /* NOT-APPROVAL-WORDS: the engine's `readiness.ready` only picks the state
   * word (Ready for approval), the six stages' marks and which act is primary
   * when the door is open; this file writes no sentence about why approval is
   * off — the screens that say it read `readiness.blocker` (RUN-LINK-TRUTH-R5). */
  const ready = Boolean(hq && hq.readiness && hq.readiness.ready)
  let state
  if (readVersion && currentVersion && readVersion !== currentVersion) state = 'needs_recheck'
  else if (ready) state = 'ready'
  else if (!B) state = 'recorded'
  else state = B.proof.verdict === 'recorded' ? 'recorded' : B.proof.verdict
  const decided = Boolean(hq && hq.decision && hq.decision.approvedFull)
  const stages = [
    ['define', criteria.accepted ? 'done' : 'not_started'],
    ['prepare', 'done'],
    ['build', ready ? 'done' : 'current'],
    ['verify', ready ? 'done' : 'not_started'],
    ['review', ready ? (decided ? 'done' : 'current') : 'not_reached'],
    ['approve', decided ? 'done' : (ready ? 'not_started' : 'not_reached')],
  ].map(([id, status]) => ({ id, status }))

  const loadedAt = msOf(options.loadedAt)
  /* A delayed load can deliver an edit with a capture timestamp earlier than
   * the first painted frame. The version boundary is the stronger fact: edits
   * belonging to versions after that frame changed what the person saw. */
  const readIndex = versions.findIndex((v) => v.id === readVersion)
  const changedSinceLoad = state === 'needs_recheck'
    ? Array.from(new Set(edits.filter((e) =>
      (readIndex >= 0 && versions.findIndex((v) => v.id === e.makesVersion) > readIndex)
      || (loadedAt != null && (msOf(e.ts) ?? 0) > loadedAt)).map((e) => e.file)))
    : []

  /* ---- findings, typed ---- */
  const findings = []
  if (B && B.proof.since) {
    const e = edits.find((x) => x.id === B.proof.since.editId)
    if (e) findings.push({ kind: 'edit_after_run', editStep: e.step })
  }
  const after = ((thread.proof && thread.proof.edited_after_last_pass) || []).map((p) => where(p).file)
  for (const f of after.slice(0, 2)) findings.push({ kind: 'no_check', file: f })
  const lastFailed = [...tests].reverse().find(failing)
  const lastPassed = [...tests].reverse().find(passing)
  if (lastFailed || lastPassed) findings.push({ kind: 'run_outcomes', failedSteps: lastFailed ? [lastFailed.step] : [], passedSteps: lastPassed ? [lastPassed.step] : [] })
  const lastOpen = [...tests].reverse().find((r) => r.status === 'no_result')
  if (lastOpen) findings.push({ kind: 'no_result', step: lastOpen.step })

  /* ---- the graph: every record on its lane; the Brief chooses what to draw ---- */
  const nodes = []
  const ses = sessions[0]
  if (ses) nodes.push({ id: ses.id, lane: 'request', kind: 'session', step: null, version: null, label: ses.agent + (sessions.length > 1 ? ' · ' + fmtCount(sessions.length) + ' sessions' : ''), time: ses.startedAt, ref: ses.id })
  if (request) nodes.push({ id: request.id, lane: 'request', kind: 'request', step: request.step, version: null, label: request.text, ref: request.id })
  for (const e of edits) nodes.push({ id: e.id, lane: 'work', kind: 'edit', step: e.step, version: e.makesVersion, label: e.file, ref: e.id })
  /* The execution lane holds test runs: what a statement can rest on. Every
   * command stays in `runs`. */
  for (const r of runs) if (r.kind === 'test') nodes.push({ id: r.id, lane: 'execution', kind: r.status === 'no_result' ? 'command' : 'run', step: r.step, version: r.version, label: r.command, ref: r.id })
  nodes.push({ id: 'criteria', lane: 'claims', kind: 'criteria', step: null, version: null, label: null, ref: 'criteria' })
  for (const s of statements) nodes.push({ id: s.id, lane: 'claims', kind: 'statement', step: s.step, version: null, label: s.plain, ref: s.id })
  const edges = []
  for (const s of statements) {
    if (s.proof.restsOn && s.proof.restsOn.runId) edges.push({ from: s.id, to: s.proof.restsOn.runId, kind: 'inferred' })
  }

  const B_run = B && B.proof.restsOn && B.proof.restsOn.runId ? runById.get(B.proof.restsOn.runId) : null
  const recheck = {
    runner: B_run ? B_run.command : null,
    scope: [],
    on: currentVersion,
    permission: null,
  }

  let primaryAction
  if (state === 'needs_recheck') primaryAction = 'check_what_changed'
  else if (state === 'ready') primaryAction = 'approve'
  else if (runs.some((r) => r.status === 'running' && r.version === currentVersion)) primaryAction = 'stop_check'
  else primaryAction = 'check_again'

  /* ---- steps: every row of the record, numbered as every screen numbers it
   * (added C30 R-ROUTES "Step view"). One entry per row — an ask, an edit, a
   * command, a claim — at the position `stepOf` gave it, so `#/changes/<id>/step/<n>`
   * and every chip that names a step agree. The detail is the served row's own
   * (hunks and output lines are the served arrays, not copies); a screen
   * writes the home folder as ~ when it draws them (paths.js). */
  const editBy = new Map(edits.map((e) => [e.id, e]))
  const stmtBy = new Map(statements.map((x) => [x.id, x]))
  const shownVersion = (id) => (id ? (vmap.has(id) ? vmap.get(id).id : shortVersion({ id })) : null) // never adds a version to the list
  const steps = rows.map((r, k) => {
    const o = r.o
    const base = {
      step: k + 1,
      kind: r.t,
      id: r.id,
      eventId: o.event_id || String(r.id).split('#')[0],
      ts: r.ts || null,
      time: hhmm(r.ts),
      sessionId: o.session_id || null,
      agent: agentOf(o.session_id),
      version: shownVersion(o.version_id),
      receipt: o.raw_file ? { file: homePath(o.raw_file), line: o.raw_line ?? null } : null,
      prompt: null,
      edit: null,
      command: null,
      claim: null,
      message: null,
    }
    if (r.t === 'prompt') {
      const seg = segmentsOf(o)
      base.prompt = { text: homeless(promptText(o)), segments: seg ? seg.map((x) => ({ kind: x.kind, text: homeless(x.text) })) : null, work: o.work !== false }
    } else if (r.t === 'message') {
      base.message = { text: homeless(plainText(o.text)) }
    } else if (r.t === 'edit') {
      const e = editBy.get(r.id)
      base.edit = { file: e.file, op: e.op, action: o.action || null, source: e.source, linesAdded: e.linesAdded, linesRemoved: e.linesRemoved, ok: e.ok, makesVersion: e.makesVersion, hunks: e.hunks }
    } else if (r.t === 'command') {
      base.command = {
        command: homeless(o.command),
        isTest: Boolean(o.is_test),
        isCommit: Boolean(o.is_commit),
        outcome: o.outcome || 'unknown',
        exitCode: o.exit_code ?? null,
        tally: o.tally ? { passed: o.tally.passed ?? null, failed: o.tally.failed ?? null, skipped: o.tally.skipped ?? null, unit: o.tally.unit || 'tests', runner: o.tally.runner || null } : null,
        tallyLine: o.tally_line ? homeless(o.tally_line) : null,
        output: Array.isArray(o.output_excerpt) ? o.output_excerpt : [],
        outputTotal: o.output_lines_total ?? null,
      }
    } else {
      const x = stmtBy.get(r.id)
      base.claim = { text: x.plain, claimKind: x.claimKind, verifier: x.verifier, statementId: x.id }
    }
    return base
  })

  const sources = Array.from(new Set((v.sessions || []).map((s) => s.source)))
  const snapshot = {
    project: { id: v.project && v.project.id, name: v.project && v.project.name, worktree: root ? homePath(root) : null }, // the home folder as ~ (DEMO: "~/code/checkout-web")
    machine: { recordsBytes: options.storageBytes ?? null, networkCalls: false },
    record: { chainHead: null, tree: null, feeds: sources.map((s) => FEED[s] || s) },
    sessions,
    request,
    messages,
    brief,
    change: {
      id: tracked && tracked.change ? tracked.change.id : thread.id,
      threadId: thread.id || null,
      title: homeless(unwrapPasted((tracked && tracked.hq && tracked.hq.title) || thread.title || null)) || UNTITLED,
      state,
      stages,
      recordedFrom: null,
      stepCount: rows.length,
      /* The best signal served: the thread's last row is within 10 minutes of
       * now (`whose_move: agent`), so capture is live. Otherwise not known. */
      recording: thread.whose_move === 'agent' ? true : null,
      whoseMove: thread.whose_move || null,
      /* added C30 R-LEDGER: the last time a person copied a message for the agent here, or said they
       * pasted it (`told_agent` of the project view, data/told.js), or null. Nothing was sent. */
      told: toldOf(options.told, at),
    },
    view: { readAt: hhmm(at.toISOString()), readDate: ymd(at), readVersion, previousLookAt: null, sinceLook: null, changedSinceLoad },
    versions,
    currentVersion,
    edits,
    runs,
    statements,
    steps, // added C30 R-ROUTES: every row of the record, by step (above)
    criteria,
    recheck,
    findings,
    graph: { nodes, edges },
    primaryAction,
    masking: v.masking || null,
    leftOutByExclusion: Number.isInteger(v.left_out_by_exclusion) && v.left_out_by_exclusion > 0 ? v.left_out_by_exclusion : null,
    /* added C30 R-RUN: the tracked Change's served run sheet (GET …/run), or null */
    runSheet: null,
    /* added C30 R-LEDGER: the words people kept on the tracked Change, with who and when, as served */
    notes: notesOf(tracked && tracked.change),
    seconds: secondsOf(tracked && tracked.change),
  }
  return withRunSheet(snapshot, tracked && tracked.runSheet, at)
}

/* ---------------------------------------------------------------------------
 * C30 R-RUN — the Change's own check, as its run sheet serves it.
 *
 * A check a person starts from HQ (`thruline check --for-change`, through the
 * run route) is recorded in ThruLine's ledger, not in the agent's transcript,
 * so the thread view never holds it. `withRunSheet` folds the run sheet's
 * typed fields (`job`, `checks[0].last`: outcome, tests_passed, tests_failed,
 * exit_code, failure_line, started_at, recorded_at) into the snapshot, so
 * every screen draws the same result (Brief, Every statement, What changed,
 * Check it again).
 *
 * - A run is drawn as on the current version only when it started at or after
 *   the last edit the record holds; an earlier end is left out (it says
 *   nothing about the files now).
 * - A check that is starting or running is a `running` run; a `passed` or
 *   `failed` end is a finished test run with the server's own counts. Every
 *   other end (checked nothing, tool not found, stopped, interrupted) settles
 *   nothing and adds no run: the check sheet says it in words.
 * - A run here has no step (`step: null`): steps are places in the agent's
 *   record, and this run is not in it. No step is invented.
 * - Proof: the newest test evidence wins, as in `proofOf`. A pass newer than
 *   every transcript test run, on the current version, proves a checkable
 *   statement; a failure newer than them and after the statement contradicts
 *   it. The statement the brief is about does not move.
 *
 * Applying it again to its own result starts from the snapshot without a
 * sheet, so a fresher sheet (followed while the check runs) replaces the old
 * one and never stacks on it.
 * ------------------------------------------------------------------------- */
const SANS_SHEET = Symbol('snapshot without the run sheet')
export const SHEET_RUN_PREFIX = 'check:'

const secondsBetween = (a, b) => {
  const x = msOf(a)
  const y = msOf(b)
  return x == null || y == null || y < x ? null : Math.floor((y - x) / 1000)
}

/** The run row a served run sheet adds to the snapshot, or null. */
export function sheetRunOf(served, { currentVersion, edits, now }) {
  if (!served || typeof served !== 'object') return null
  const check = Array.isArray(served.checks) ? served.checks[0] || null : null
  const job = served.job && served.job.for_this_change === true ? served.job : null
  const last = check && check.last ? check.last : null
  const lastEditMs = (edits || []).reduce((m, e) => { const t = msOf(e && e.ts); return t != null && (m == null || t > m) ? t : m }, null)
  const nowIso = (now instanceof Date ? now : new Date(now == null ? Date.now() : now)).toISOString()
  const command = check && check.command ? homeless(check.command) : null
  const common = { kind: 'test', command, verbatim: check ? check.command || null : null, step: null, capturedBy: 'thruline', scope: null, sessionId: null }
  if (job || (last && last.outcome === 'running')) {
    const started = (job && job.started_at) || (last && last.started_at) || null
    const run = (job && job.run) || (last && last.run) || null
    return {
      ...common,
      id: SHEET_RUN_PREFIX + run,
      runKey: run,
      time: hhmm(started),
      ts: started,
      version: currentVersion,
      status: 'running',
      starting: Boolean(job && job.phase === 'starting'),
      outcome: null,
      exitCode: null,
      passed: null,
      failed: null,
      durationMs: null,
      elapsedS: secondsBetween(started, nowIso),
      output: [],
      failures: [],
    }
  }
  if (!last || (last.outcome !== 'passed' && last.outcome !== 'failed')) return null
  const started = msOf(last.started_at)
  if (started == null || (lastEditMs != null && started < lastEditMs)) return null
  const at = last.recorded_at || last.started_at
  const num = (n) => (Number.isInteger(n) ? n : null)
  const line = typeof last.failure_line === 'string' && last.failure_line ? last.failure_line : null
  return {
    ...common,
    id: SHEET_RUN_PREFIX + last.run,
    runKey: last.run || null,
    time: hhmm(at),
    ts: at,
    version: currentVersion,
    status: 'finished',
    outcome: last.outcome === 'passed' ? 'succeeded' : 'failed',
    exitCode: num(last.exit_code),
    passed: num(last.tests_passed),
    failed: num(last.tests_failed),
    durationMs: msOf(last.recorded_at) != null ? msOf(last.recorded_at) - started : null,
    elapsedS: null,
    output: line ? [line] : [],
    failures: line ? [{ line: 0 }] : [],
  }
}

/** The proof of one checkable statement, with the sheet's run folded in. */
function sheetProof(st, row, transcriptTests) {
  const p = st.proof
  if (!st.verifier || !p || p.verdict === 'recorded') return p
  if (row.status === 'running') {
    return p.verdict === 'proven' ? p : Object.assign({}, p, { recheck: { status: 'running', runId: row.id } })
  }
  const rowMs = msOf(row.ts)
  const newest = rowMs != null && transcriptTests.every((r) => msOf(r.ts) == null || msOf(r.ts) < rowMs)
  if (!newest) return p
  const looked = (p.lookedAt || []).concat([row.id])
  const passes = row.outcome === 'succeeded' && row.failed === 0 && typeof row.passed === 'number' && row.passed > 0
  if (passes) {
    const cited = p.restsOn && p.restsOn.runId ? p.restsOn.runId : null
    return Object.assign({}, p, {
      verdict: 'proven', stopsAt: null, basis: null,
      restsOn: { kind: 'run', runId: row.id, basis: 'later_run', match: null, checkedAgainst: null },
      ranOn: { version: row.version },
      since: null,
      recheck: { status: 'not_needed', runId: null },
      lookedAt: looked,
      thenAndNow: cited && p.ranOn && p.ranOn.version ? { from: p.ranOn.version, to: row.version, noLongerCounts: [], kept: [], added: [{ runId: row.id, newScope: [] }] } : p.thenAndNow || null,
      followUp: Object.assign({}, p.followUp || {}, { checkedAgain: { runId: row.id } }),
    })
  }
  const fails = row.outcome === 'failed' || (typeof row.failed === 'number' && row.failed > 0)
  const after = msOf(st.ts) == null || rowMs > msOf(st.ts)
  if (fails && after) {
    return Object.assign({}, p, { verdict: 'contradicted', stopsAt: null, basis: null, recheck: { status: 'failed', runId: row.id }, lookedAt: looked })
  }
  return p
}

/**
 * The snapshot with one served run sheet folded in (see above). `served` null
 * gives the snapshot without any sheet.
 */
export function withRunSheet(snapshot, served, now) {
  if (!snapshot) return snapshot
  const base = snapshot[SANS_SHEET] || snapshot
  if (!served) return base
  const row = sheetRunOf(served, { currentVersion: base.currentVersion, edits: base.edits, now })
  const transcriptTests = (base.runs || []).filter((r) => r.kind === 'test' && r.status === 'finished')
  const statements = row ? base.statements.map((st) => {
    const proof = sheetProof(st, row, transcriptTests)
    return proof === st.proof ? st : Object.assign({}, st, { proof })
  }) : base.statements
  const B = statements.find((s) => s.id === (base.brief && base.brief.statementId)) || null
  const kept = base.change.state === 'needs_recheck' || base.change.state === 'ready' || !B
  const state = kept ? base.change.state : (B.proof.verdict === 'recorded' ? 'recorded' : B.proof.verdict)
  const runs = row ? base.runs.concat([row]) : base.runs
  let primaryAction
  if (runs.some((r) => r.status === 'running' && r.version === base.currentVersion)) primaryAction = 'stop_check'
  else if (state === 'needs_recheck') primaryAction = 'check_what_changed'
  else if (state === 'ready') primaryAction = 'approve'
  else primaryAction = 'check_again'
  const out = Object.assign({}, base, {
    runSheet: served,
    runs,
    statements,
    change: state === base.change.state ? base.change : Object.assign({}, base.change, { state }),
    primaryAction,
  })
  Object.defineProperty(out, SANS_SHEET, { value: base, enumerable: false })
  return out
}

/** What counts as done, from the tracked Change's own derived model only. */
function criteriaOf(tracked) {
  const hq = tracked && tracked.hq
  const change = tracked && tracked.change
  const scope = change && change.accepted_scope
  const proposals = tracked && tracked.proposals
  const proposed = proposals && Array.isArray(proposals.criteria) ? proposals.criteria.length
    : (proposals && Array.isArray(proposals.items) ? proposals.items.length : null)
  if (!scope) {
    return { accepted: false, acceptedAt: null, acceptedOnVersion: null, acceptedAfterStep: null, version: null, lineCount: null, lines: [], proposedCount: proposed, proposedFromStep: null, provenOn: null }
  }
  const acc = scope.acceptance || {}
  const lineCount = hq && Array.isArray(hq.criteria) ? hq.criteria.length : (Array.isArray(scope.criteria) ? scope.criteria.length : null)
  /* The accepted words and the engine's line state have the same numbered
   * identity. Keep the actual line in the snapshot, rather than a count alone:
   * Every statement, its proof strip, and the Done screen then read the same
   * accepted list. An absent derived state remains not proven. */
  const lines = (Array.isArray(scope.criteria) ? scope.criteria : []).map((text, i) => {
    const model = hq && Array.isArray(hq.criteria) ? hq.criteria.find((c) => c && c.n === i + 1) : null
    const state = model && ['proven', 'needs_recheck', 'in_progress', 'not_proven', 'recorded'].includes(model.state) ? model.state : 'not_proven'
    return {
      id: 'line-' + (i + 1), num: i + 1, gate: false, link: null,
      text: model && typeof model.text === 'string' ? model.text : text,
      proof: { verdict: state, stopsAt: state === 'proven' ? null : 2, restsOn: null, lookedAt: [], misses: [] },
      recordedLine: model && model.line ? model.line : null,
    }
  })
  return {
    accepted: true,
    acceptedAt: hhmm(acc.accepted_at),
    acceptedTs: acc.accepted_at || null,
    acceptedBy: acc.actor || null,
    acceptedOnVersion: null,
    acceptedAfterStep: null,
    version: scope.version != null ? 'v' + scope.version : null,
    lineCount,
    lines,
    proposedCount: proposed,
    proposedFromStep: null,
    provenOn: hq && hq.counts ? { version: hq.version ? hq.version.short : null, count: hq.counts.proven, runId: null } : null,
  }
}

/**
 * One statement's ①–⑥ chain (DESIGN-SPEC §4.0), as typed links.
 * - no verifier: rests on what counts as done; stops at ② until it is accepted.
 * - verifier: ② the run `rests_on_command_id` names (the latest test run before
 *   the claim — the engine's link, not the agent's citation); ③ that run's
 *   version; ④ the first edit after it (the run states no scope, so any edit
 *   counts, and the snapshot says so); ⑤ a passing test run on the current
 *   version after the last edit; ⑥ the verdict.
 */
function proofOf(st, claim, ctx) {
  const { runById, tests, edits, lastEdit, currentVersion, criteria } = ctx
  if (!st.verifier) {
    return { verdict: 'recorded', version: currentVersion, scope: [], stopsAt: criteria.accepted ? null : 2, rule: null, restsOn: { kind: 'criteria' } }
  }
  const base = {
    version: currentVersion, scope: [], rule: null, earlier: null, lookedAt: [],
    followUp: { toldAgent: false, laterWork: null, laterWorkSince: null, checkedAgain: null },
  }
  const cited = claim.rests_on_command_id ? runById.get(claim.rests_on_command_id) : null
  const afterClaim = tests.filter((r) => r.step > st.step && r.status === 'finished')
  const lastAfter = afterClaim[afterClaim.length - 1] || null
  // ⑤ a passing test run on the current version after the last edit
  const settle = [...tests].reverse().find((r) => r.version === currentVersion && passing(r) && (!lastEdit || r.step > lastEdit.step)) || null
  const looked = (r) => [r && r.id].filter(Boolean)

  if (lastAfter && failing(lastAfter)) {
    return {
      ...base, verdict: 'contradicted', stopsAt: null,
      restsOn: cited ? { kind: 'run', runId: cited.id, basis: 'latest_before', checkedAgainst: null } : { kind: 'run', runId: null, basis: 'latest_before' },
      ranOn: cited ? { version: cited.version } : null,
      since: null,
      recheck: { status: 'failed', runId: lastAfter.id },
      lookedAt: looked(cited).concat(looked(lastAfter)),
      thenAndNow: null,
    }
  }
  if (settle && (!cited || settle.id !== cited.id)) {
    // a later passing run on this version settles it; the chain rests on that run
    const sameCommand = cited && cited.command === settle.command
    return {
      ...base, verdict: 'proven', stopsAt: null,
      restsOn: { kind: 'run', runId: settle.id, basis: sameCommand ? 'matched' : 'later_run', match: sameCommand ? { sameCommandAs: cited.step, widerScope: false } : null, checkedAgainst: null },
      ranOn: { version: settle.version },
      since: null,
      recheck: { status: 'not_needed', runId: null },
      lookedAt: looked(settle),
      thenAndNow: cited && cited.id !== settle.id ? { from: cited.version, to: settle.version, noLongerCounts: [], kept: [], added: [{ runId: settle.id, newScope: [] }] } : null,
      followUp: { ...base.followUp, checkedAgain: { runId: settle.id } },
    }
  }
  if (!cited) {
    return { ...base, verdict: 'not_proven', stopsAt: 2, restsOn: { kind: 'run', runId: null, basis: 'latest_before' }, ranOn: null, since: null, recheck: { status: 'none', runId: null }, thenAndNow: null }
  }
  if (!passing(cited)) {
    return { ...base, verdict: failing(cited) ? 'contradicted' : 'not_proven', stopsAt: failing(cited) ? null : 2, restsOn: { kind: 'run', runId: cited.id, basis: 'latest_before', checkedAgainst: null }, ranOn: { version: cited.version }, since: null, recheck: { status: failing(cited) ? 'failed' : 'none', runId: failing(cited) ? cited.id : null }, lookedAt: looked(cited), thenAndNow: null }
  }
  const since = edits.find((e) => e.step > cited.step) || null
  const holds = !since && cited.version === currentVersion
  return {
    ...base,
    verdict: holds ? 'proven' : 'not_proven',
    stopsAt: holds ? null : 5,
    restsOn: { kind: 'run', runId: cited.id, basis: 'latest_before', checkedAgainst: null },
    ranOn: { version: cited.version },
    since: since ? { editId: since.id, dependsOnWholeChange: true } : null,
    recheck: { status: holds ? 'not_needed' : 'none', runId: null },
    lookedAt: looked(cited),
    thenAndNow: since ? { from: cited.version, to: currentVersion, noLongerCounts: [{ runId: cited.id, editId: since.id }], kept: [], added: [] } : null,
  }
}
