/* Record-Over-Time on the real record (W2-E).
 *
 * What this page counts — statements per day, per agent, per project, and how
 * far each one's proof reaches — needs every claim of every thread with its
 * time, its agent and its proof. The only place those are served today is the
 * thread document (`GET /api/hq/projects/<p>/threads/<t>`), one thread at a
 * time. Measured on the scratch store (2026-10-07, 144 threads): reading every
 * thread took 175 s and 167 MB; the threads of the last 7 days alone are 30
 * threads and 33 MB. A page cannot pay that, so the live page reads only the
 * project documents and says, in plain words, that the counts are not
 * available in this version. `tallyKey` and `dailySeries` below are the
 * derivations the counts will use once a per-project statement list is
 * served; they are tested on their own.
 *
 * PURE: no fetch, no clock but `now`.
 */
import { currentProjectId, inboxBadge, projectName } from '../../data/project.js'

const pad = (n) => String(n).padStart(2, '0')
const list = (x) => (Array.isArray(x) ? x : [])

export const hhmmLocal = (ms) => {
  const d = new Date(ms)
  return pad(d.getHours()) + ':' + pad(d.getMinutes())
}
const ymdLocal = (ms) => {
  const d = new Date(ms)
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate())
}

/**
 * @param {{projects: object[]|null, storage: object|null, now: number}} p
 *   `projects` are the ProjectSummary rows of `GET /api/hq/projects`.
 */
export function recordFromProjects({ projects, storage, now, current, reading = null }) {
  const ps = list(projects)
  const sum = (k) => ps.reduce((n, p) => n + (typeof p[k] === 'number' ? p[k] : 0), 0)
  return {
    live: true,
    today: ymdLocal(now),
    readAt: hhmmLocal(now),
    // the sidebar badge: the ONE count every screen shows (`yourMove` of the current project, C46 §1.1)
    inbox: { yourMove: inboxBadge(ps, current, reading) },
    currentProject: currentProjectId(ps, current) || null, // the sidebar lists it first, marked current
    storage: { bytes: storage && typeof storage.bytes === 'number' ? storage.bytes : null },
    windows: [7, 30, 90],
    sources: [...new Set(ps.flatMap((p) => list(p.sources)))],
    projects: ps.map((p) => ({
      id: p.id,
      name: projectName(p),
      threads: typeof p.threads === 'number' ? p.threads : null,
      claims: typeof p.claims === 'number' ? p.claims : null,
      lastActivity: p.last_activity || null,
      sources: list(p.sources),
    })),
    totals: { claims: sum('claims'), threads: sum('threads') },
    // Not served: see the header of this file.
    tallies: null,
    medianMinutesToProof: null,
    daily: null,
  }
}

/** A statement's place in the tally, from its proof as the Change model states it. */
export function tallyKey(st) {
  const pf = st && st.proof
  if (!pf) return null
  if (pf.verdict === 'proven') return 'proven'
  if (pf.verdict === 'contradicted') return 'rerunFailed'
  if (pf.verdict === 'recorded') return pf.stopsAt === 3 ? 'recordedStopsAtRanOn' : 'recordedStopsAtSo'
  if (pf.verdict === 'not_proven' || pf.verdict === 'needs_recheck') {
    if (pf.recheck && pf.recheck.status === 'failed') return 'rerunFailed'
    if (pf.stopsAt === 2) return 'noProofFound'
    if (pf.stopsAt === 3) return 'recordedStopsAtRanOn'
    return 'needsRecheck'
  }
  return null
}

/**
 * Which days each agent was recorded on, index 0 = (days-1) days ago, last =
 * today: a day is recorded when a session of that agent spans it.
 *
 * @param {Array<{agent: 'cc'|'cx', from: string, to: string}>} sessions
 */
export function recordedDays(sessions, now, days = 90) {
  const today = new Date(now); today.setHours(0, 0, 0, 0)
  const dayIndex = (iso) => {
    const d = new Date(iso); d.setHours(0, 0, 0, 0)
    const i = days - 1 - Math.round((today - d) / 86400000)
    return i >= 0 && i < days ? i : null
  }
  const out = {}
  for (const a of ['cc', 'cx']) {
    const recorded = new Array(days).fill(false)
    for (const s of list(sessions).filter((x) => x.agent === a)) {
      const from = dayIndex(s.from), to = dayIndex(s.to)
      const lo = from === null ? 0 : from, hi = to === null ? days - 1 : to
      if (from === null && to === null) continue
      for (let i = lo; i <= hi; i++) recorded[i] = true
    }
    out[a] = recorded
  }
  return out
}

/**
 * Statements per day for the longest window, per agent: index 0 = (days-1)
 * days ago, last = today. A day on which the agent has no recorded session is
 * `null` — not recorded, never a zero.
 *
 * `recorded`, when given, is the agents' recorded days (`recordedDays`) to use
 * instead of `sessions`' own: the page passes the machine's, so that a project
 * nobody worked in on a day the agent was recording counts 0 that day, not
 * "not recorded".
 *
 * @param {Array<{ts: string, agent: 'cc'|'cx', proven: boolean}>} statements
 * @param {Array<{agent: 'cc'|'cx', from: string, to: string}>} sessions
 * @param {number} now
 * @param {number} days
 * @param {{cc: boolean[], cx: boolean[]}} [recorded]
 */
export function dailySeries(statements, sessions, now, days = 90, recorded = null) {
  const today = new Date(now); today.setHours(0, 0, 0, 0)
  const dayIndex = (iso) => {
    const d = new Date(iso); d.setHours(0, 0, 0, 0)
    const i = days - 1 - Math.round((today - d) / 86400000)
    return i >= 0 && i < days ? i : null
  }
  const rec = recorded || recordedDays(sessions, now, days)
  const out = {}
  for (const a of ['cc', 'cx']) {
    const st = rec[a].map((r) => (r ? 0 : null))
    const pr = rec[a].map((r) => (r ? 0 : null))
    for (const x of list(statements).filter((y) => y.agent === a)) {
      const i = dayIndex(x.ts)
      if (i === null) continue
      if (st[i] === null) { st[i] = 0; pr[i] = 0 }
      st[i] += 1
      if (x.proven) pr[i] += 1
    }
    out[a] = { statements: st, proven: pr }
  }
  return out
}

/* ---- the counts, from `GET /api/hq/projects/<id>/statements` (polish) ----
 * One row per claim, no text, with the engine's own verdict (the one rule:
 * crates/core/src/hq/statements.rs), and every session's span. The page's
 * tallies, daily series and project lifetimes are counted from those rows
 * only. What the route serves beside the verdict — when the run that proves a
 * claim was recorded, whether a run came before it, whether its proof was moved
 * by a later edit — is read from the same rows by metrics.js. What it does not
 * serve (re-runs a person started) is left `null` and the page says it is not
 * counted, never 0. */

const AGENT_KEY = { claude_code: 'cc', codex: 'cx' }

/** The engine's verdict and basis, as the proof shape `tallyKey` reads. */
export function proofOfServed(row) {
  const v = row && row.verdict
  if (v === 'proven') return { verdict: 'proven', stopsAt: null }
  if (v === 'contradicted') return { verdict: 'contradicted', stopsAt: null, recheck: { status: 'failed' } }
  if (v === 'recorded') return { verdict: 'recorded', stopsAt: 6 }
  if (v === 'not_proven') {
    const b = row.verdict_basis
    return { verdict: 'not_proven', stopsAt: b === 'edited_since_run' || b === 'run_on_older_version' ? 5 : 2 }
  }
  return null
}

const TALLY_ZERO = () => ({
  statements: 0, proven: 0, needsRecheck: 0, noProofFound: 0, rerunFailed: 0,
  recordedStopsAtRanOn: 0, recordedStopsAtSo: 0,
  // not served by the statements route: absent, never 0
  reopened: null, reproven: null,
  reruns: { passed: null, failed: null, stopped: null },
  beforeAnyRun: { total: null, laterProven: null, stillNothing: null },
})

/**
 * The page's data with its counts: `recordFromProjects` plus `tallies`,
 * `daily` and each project's `lifetime`, from the statements documents.
 *
 * @param {{projects: object[], statements: Object<string, object>, storage: object|null, now: number}} p
 *   `statements[projectId]` is that project's `GET …/statements` document.
 */
export function recordFromStatements({ projects, statements, storage, now, current, reading = null }) {
  const base = recordFromProjects({ projects, storage, now, current, reading })
  const today = new Date(now); today.setHours(0, 0, 0, 0)
  const daysAgo = (iso) => {
    const d = new Date(iso)
    if (Number.isNaN(d.getTime())) return null
    d.setHours(0, 0, 0, 0)
    return Math.round((today - d) / 86400000)
  }
  const DAYS = Math.max(...base.windows)
  const tallies = {}
  for (const w of base.windows) tallies[w] = {}
  const daily = {}
  /* The rows every count on the page is made from: a claim of a known agent,
   * with a verdict the tally places and a time that parses, made today or
   * before. The tallies and the daily series are both counted from these rows
   * and no others, so the series sums to the tally in every window. */
  const rowsOf = {}
  const spans = []
  for (const p of base.projects) {
    const doc = statements && statements[p.id]
    rowsOf[p.id] = []
    for (const c of list(doc && doc.claims)) {
      const a = AGENT_KEY[c.agent]
      const key = a ? tallyKey({ proof: proofOfServed(c) }) : null
      const ago = daysAgo(c.ts)
      if (!key || ago === null || ago < 0) continue
      rowsOf[p.id].push({ pid: p.id, c, a, key, ago })
      spans.push({ agent: a, from: c.ts, to: c.ts })
    }
    for (const s of list(doc && doc.sessions)) {
      if (AGENT_KEY[s.source]) spans.push({ agent: AGENT_KEY[s.source], from: s.started_at || s.last_at, to: s.last_at || s.started_at })
    }
  }
  /* Whether an agent was recorded on a day is a fact about the machine, not a
   * project: the agent's sessions in every project. A project nobody worked in
   * that day counts 0 for it; only a day the agent recorded nothing anywhere is
   * null. (Per-project spans made every day "not recorded" for All projects,
   * since on every day some project had no session: R-O-T "too few days" with
   * 5,475 statements counted.) */
  const machineDays = recordedDays(spans, now, DAYS)
  const outProjects = base.projects.map((p) => {
    const doc = statements && statements[p.id]
    const claims = list(doc && doc.claims)
    const sessions = list(doc && doc.sessions)
    const byAgent = { cc: 0, cx: 0 }
    let last = null
    for (const c of claims) {
      const a = AGENT_KEY[c.agent]
      if (!a) continue
      byAgent[a] += 1
      if (!last || String(c.ts) > String(last)) last = c.ts
    }
    for (const { a, key, ago } of rowsOf[p.id]) {
      for (const w of base.windows) {
        if (ago >= w) continue
        const t = ((tallies[w][p.id] = tallies[w][p.id] || {})[a] = tallies[w][p.id][a] || TALLY_ZERO())
        t.statements += 1
        t[key] += 1
      }
    }
    const series = dailySeries(
      rowsOf[p.id].map(({ c, a }) => ({ ts: c.ts, agent: a, proven: c.verdict === 'proven' })),
      [], now, DAYS, machineDays)
    const own = recordedDays(sessions.filter((s) => AGENT_KEY[s.source]).map((s) => ({ agent: AGENT_KEY[s.source], from: s.started_at || s.last_at, to: s.last_at || s.started_at })), now, DAYS)
    for (const a of ['cc', 'cx']) {
      // an agent this project never recorded in the window is absent, not a row of nulls or zeros
      const touched = own[a].some(Boolean) || rowsOf[p.id].some((r) => r.a === a && r.ago < DAYS)
      if (!touched || series[a].statements.every((x) => x === null)) continue
      ;(daily[p.id] = daily[p.id] || {})[a] = series[a]
    }
    const lastLocal = last ? ymdLocal(Date.parse(last)) : null
    return Object.assign({}, p, {
      recording: false, // per-project capture state is not served; the frame's pill says what is recording
      changes: p.threads,
      dismissedChanges: null,
      lastRecordedOn: p.lastActivity ? ymdLocal(Date.parse(p.lastActivity)) : null,
      lifetime: { statements: claims.length, byAgent, lastStatementOn: lastLocal },
    })
  })
  return Object.assign(base, {
    projects: outProjects,
    tallies,
    daily,
    medianMinutesToProof: {}, // computed per selection from `rows` (metrics.js), not stored
    /* The rows every count above is made of, kept whole so that the counts that
     * need more than a verdict (metrics.js) and the CSV read the same ones. */
    rows: base.projects.flatMap((p) => rowsOf[p.id]),
    unserved: true,
  })
}
