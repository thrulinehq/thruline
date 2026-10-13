/* The first-use path — the deterministic model behind the first screen (T-2-7).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness. It takes the
 * two documents the first-use routes serve (`GET /api/first-use` and
 * `GET /api/first-use/proposal`) and decides what the screen says; `first-use.jsx`
 * lays out what this decided, and every word it lays out comes from
 * `HQ_COPY.first_use`.
 *
 * WHAT THIS FILE IS FOR. `03-USER-LENS.md` rule 8 and §4 step 1: install → pick
 * a project → the tool proposes a Change from what it can already see → a true
 * statement about Sam's own work, in under five minutes, with no documentation.
 * The baseline's H2T-1 is the moment it removes: the documented quickstart
 * dead-ended at "0 transcript file(s)" and the first true statement anybody
 * reached was about demo data.
 *
 * THE RULES IT HOLDS:
 *
 *  - EVERY NUMBER COMES FROM A TYPED FIELD of the served document. A count that
 *    is not a number is `null` and the row says the count is unknown — never 0,
 *    because "nothing recorded here" and "we could not count it" are different
 *    facts and only one of them is about the person's work (`B8.6`, Rule 12).
 *  - A MISSING PROPOSAL IS MISSING (`B7.2`, `B11.7`): a project with nothing
 *    recorded gets the sentence that says so and the one action that would
 *    change it, never a "setup required" and never an empty screen.
 *  - AN UNAVAILABLE TOOL IS NOT A DEFECT (`B11.6`). A check whose program is
 *    not on this machine is carried with `not available here: <tool>`, stays on
 *    the list, and is counted apart from anything about the work itself.
 *  - NOTHING IS WRITTEN BY READING. This model decides what a person is
 *    offered; the two writes are a click each, and accepting what counts as
 *    done is a second, separate act (`B5.7`).
 *  - ONE PRIMARY ACTION PER STEP (rule 7), and a disabled one says why and what
 *    would turn it on (rule 6) — `pickAction` is that answer, and it names WHICH
 *    action, because a project that is already picked and already tracked has a
 *    different truthful next step (the Inbox) from one this store has never
 *    seen. A second visit to this screen never writes the same work twice.
 */

import { HQ_COPY } from './hq-copy.js'
import { dayOf } from './change-view.js'

const C = HQ_COPY.first_use

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)
/** A count the record states, or `null`. Never a fallback to zero. */
const count = (v) => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? Math.floor(v) : null)

/* ── what this machine already has ─────────────────────────────────────── */

/**
 * One row per repository the server found, in the order the server found them.
 *
 * The order is the server's (most recently recorded first) and is not re-sorted
 * here. The screen preserves that order within its recorded and other groups.
 *
 * @param {object|null} doc the body of `GET /api/first-use`
 * @returns {{rows: object[], notes: string[], registered: number|null, storeFolder: string|null, storeBytes: number|null}}
 */
export function candidateRows(doc) {
  const d = doc || {}
  const rows = []
  for (const c of list(d.candidates)) {
    const key = str(c && c.key)
    const label = str(c && c.label)
    if (!key || !label) continue
    rows.push({
      key,
      label,
      source: str(c.source) || 'unknown',
      here: c.source === 'here',
      registered: c.registered === true,
      project: str(c.project),
      /* How many Changes this project already holds, as the server counted
         them. `null` where there was nothing to count — a project this store
         has never seen — and never a 0, which would read as the different
         fact "picked, and holding nothing". */
      changes: count(c.changes),
      recorded: count(c.recordings),
      notRead: count(c.not_read) || 0,
      newest: dayOf(c.last_recorded),
      oldest: dayOf(c.first_recorded),
    })
  }
  return {
    rows,
    notes: list(d.notes).map(String),
    registered: count(d.registered),
    storeFolder: str(d.store && d.store.folder),
    storeBytes: count(d.store && d.store.bytes),
  }
}

/**
 * What one row says about itself, as copy keys and values — never as a
 * sentence built here.
 *
 * @param {object} row one of `candidateRows().rows`
 * @returns {{key: string, values: object}[]}
 */
export function rowFacts(row) {
  const out = []
  if (row.recorded == null) out.push({ key: 'recorded_unknown', values: {} })
  else if (row.recorded === 0) out.push({ key: 'recorded_none', values: {} })
  else if (row.recorded === 1) out.push({ key: 'recorded_one', values: {} })
  else out.push({ key: 'recorded_many', values: { n: row.recorded } })
  if (row.recorded && row.newest) out.push({ key: 'newest', values: { date: row.newest } })
  if (row.here) out.push({ key: 'here', values: {} })
  /* Picked, and whether anything is already tracked in it — two different
     facts, and the second one is why coming back to this screen does not write
     the same work down a second time. */
  if (row.registered && row.changes === 1) out.push({ key: 'tracked_one', values: {} })
  else if (row.registered && row.changes > 1) out.push({ key: 'tracked_many', values: { n: row.changes } })
  else if (row.registered) out.push({ key: 'already', values: {} })
  return out
}

/**
 * The one primary action of the pick step, and — when it is off — why, and
 * what would turn it on (rule 6).
 *
 * THREE ANSWERS, NOT A BOOLEAN.
 *
 *  - nothing picked: off, with both halves of the reason;
 *  - a project this store has never seen: `use`, which registers it;
 *  - a project already picked and already carrying a Change: `inbox`, which
 *    goes to where that Change already is.
 *
 * The third exists because this screen is reachable a second time by perfectly
 * ordinary means — a bookmark, a typed address, a second tab, the back button —
 * and it used to offer "Use this project" every time, with no note that the
 * project was already picked, which walked a person into a second identical
 * item in their Inbox and no explanation of why there were two. That is the
 * *confused* failure mode `03-USER-LENS.md` §2 names, caused by this product
 * and not by the record. Still ONE primary action (rule 7) — a different one,
 * because the truthful next step is different.
 *
 * @param {string|null} selected the key of the picked row, if any
 * @param {object[]} rows
 */
export function pickAction(selected, rows) {
  const row = list(rows).find((r) => r.key === selected) || null
  if (!row) {
    return { kind: 'blocked', enabled: false, label: C.pick.action, reason: C.pick.blocked.reason, enabler: C.pick.blocked.enabler }
  }
  /* Registered AND holding a Change. Registered with nothing tracked in it yet
     keeps the ordinary action: there is nothing to duplicate, and sending a
     person to an Inbox that would have nothing of theirs in it is not help. */
  if (row.registered && row.changes > 0) {
    return { kind: 'inbox', enabled: true, label: C.pick.already_action, note: C.pick.already_note }
  }
  return { kind: 'use', enabled: true, label: C.pick.action }
}

/* ── the Change the record already holds ───────────────────────────────── */

/**
 * The proposed Change, or the honest absence of one.
 *
 * `have: false` is not an error and not an empty screen: it is the branch that
 * carries the sentence about nothing being captured yet and the one action
 * that would change that (`B11.7`).
 *
 * @param {object|null} doc the body of `GET /api/first-use/proposal`
 */
export function proposalView(doc) {
  const p = doc && doc.proposed ? doc.proposed : null
  if (!p) return { have: false, project: str(doc && doc.project) }
  const profile = p.profile || {}
  const checks = []
  for (const c of list(profile.checks)) {
    const id = str(c && c.id)
    if (!id) continue
    const slug = str(c.availability_slug) || 'check_ready'
    checks.push({
      id,
      tool: str(c.tool) || id,
      mandatory: c.mandatory === true,
      available: slug === 'check_ready',
      slug,
      /* The program or settings file the engine named. The sentence about it
       * is the copy file's; the name is the machine's. */
      name: str(c.availability_name),
    })
  }
  return {
    have: true,
    project: str(doc && doc.project),
    session: str(p.session),
    title: str(p.title),
    request: str(p.request),
    recordedAt: dayOf(p.recorded_at),
    touched: count(p.files_touched),
    checks,
    /* Counted apart, because they are different facts. */
    absent: checks.filter((c) => c.slug === 'check_not_available_here').length,
    needsSettings: checks.filter((c) => c.slug === 'check_needs_a_recipe').length,
  }
}

/**
 * What one check's availability line says, as a copy key and its values.
 *
 * `B11.6`: a tool this machine does not have is named as absent, never mixed
 * with anything the record says about the work — and never confused with the
 * OTHER reason a check cannot run. The engine holds two different facts here
 * (`check_not_available_here`, `check_needs_a_recipe`) and they say opposite
 * things about the machine: one means the program is missing, the other means
 * the program is installed and has no settings file yet. Collapsing them made
 * the screen say "not available here: cargo" about a machine with cargo on it,
 * which is this product telling a small lie about somebody's own laptop.
 */
export function checkAvailability(check) {
  if (check.available) return null
  if (check.slug === 'check_needs_a_recipe') {
    return { key: 'needs_settings', values: { name: check.name || check.tool } }
  }
  return { key: 'unavailable', values: { tool: check.name || check.tool } }
}

/**
 * The criteria the Change screen would show, from `/api/changes/<id>/proposals`.
 *
 * Read only after the Change exists, because criteria belong to a Change. The
 * absence of any is a sentence, not an empty list (`B5.9`).
 */
export function criteriaLines(doc) {
  const proposals = doc && doc.proposals ? doc.proposals : null
  const lines = []
  for (const c of list(proposals && proposals.criteria)) {
    const text = str(c && c.text)
    if (text) lines.push({ number: count(c.number), text })
  }
  return lines
}

/**
 * The folders the Change would cover, from `/api/changes/<id>/proposals`
 * (T-3-2b).
 *
 * Read off the served document's typed fields only. An empty list is an
 * answer, and the caller renders the sentence for it rather than a blank.
 */
export function scopeLines(doc) {
  const scope = doc && doc.proposals && doc.proposals.scope ? doc.proposals.scope : null
  return list(scope && scope.paths).map(String).filter(Boolean)
}

export default candidateRows
