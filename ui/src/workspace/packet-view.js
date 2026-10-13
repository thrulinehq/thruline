/* The whole story on one page — the deterministic model behind it (T-2-5).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness, no model. It
 * takes the same two documents the Change workspace takes
 * (`/api/changes/<id>`, `/api/changes/<id>/proposals`) plus the confirmed
 * sessions the existing adapter builds, and decides what the page says.
 * `packet.jsx` only lays out what this decided, and every word it lays out
 * comes from `HQ_COPY.change.packet`.
 *
 * WHAT THIS FILE IS FOR. Sam's second question — "what still needs evidence,
 * and what's actually wrong?" — answered by reading down ONE page with the
 * technical layer closed (`03-USER-LENS.md` §4 step 4). The page opens with
 * the conclusion sentence and the still-needs-evidence list, and only then
 * does it start on the record, section by section.
 *
 * THE RULES IT HOLDS:
 *
 *  - TWELVE sections, in the order the brief's §10 lists them (`B10.3`),
 *    never reordered, never dropped. `SECTIONS` is that order and
 *    `SECTION_SOURCE` records which sentence of `B10.3` each one is, so the
 *    mapping is inspectable rather than remembered.
 *  - A section with nothing in it is not blank and is not a tick: it renders
 *    its own sentence saying so (`B10.6`). `sectionsOf` decides `filled`; the
 *    view renders `empty` for every `false`, with no exception.
 *  - Every evidence item carries a provenance word AND an address in the
 *    record (`B10.4`, `B10.5`). An item with neither is REFUSED by
 *    `evidenceItem` — it never reaches the page unlabelled, and the page says
 *    how many were refused (Rule 12: fail loud).
 *  - Missing evidence is missing. A criterion with nothing behind it renders
 *    "No test ran for this." — never a pass, never a failure (`B7.2`,
 *    `B7.15`).
 *  - Freshness is UNKNOWN in this build and says so. "Yes" would be exactly
 *    the collapse `B7.2` forbids.
 *
 * WHERE THE MAPPING COMES FROM (T-3-1). `GET /api/changes/<id>/coverage`, and
 * nothing else. `mappingFromCoverage` renders what that document says and adds
 * nothing to it: the state per line, the items under it, the ones a person took
 * off it. Matching a criterion to a finding by its words HERE would be the
 * collapse, not the feature — the engine has one documented rule and this page
 * is a reader of it. `criteriaMapping` stays for the case where no coverage
 * document arrived, and in that case every line reads "No test ran for this.",
 * which is what an unreadable record honestly means.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { buildChangeView, dayOf } from './change-view.js'
import { coverageRows, evidenceEntries, servedUnlinkedLines, unlinkedCompletedRun } from './coverage-view.js'

const P = HQ_COPY.change.packet

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/**
 * The twelve sections, in the brief's §10 order.
 *
 * `B10.3` in one sentence: "The packet must include the Change purpose and
 * original request, accepted criteria and exclusions, exact target and
 * assessment inputs, actual diff summary, criterion-to-evidence mapping,
 * examination results and limitations, applicable review rounds, finding
 * dispositions, repair/rerun history, freshness/applicability, exceptions,
 * and approval history." Twelve items; twelve ids, in that reading order.
 */
export const SECTIONS = Object.freeze([
  'purpose',
  'accepted',
  'target',
  'changed',
  'mapping',
  'examination',
  'review',
  'dispositions',
  'repairs',
  'freshness',
  'exceptions',
  'decisions',
])

/** Which phrase of `B10.3` each section id is. Inspectable, and asserted by test. */
export const SECTION_SOURCE = Object.freeze({
  purpose: 'the Change purpose and original request',
  accepted: 'accepted criteria and exclusions',
  target: 'exact target and assessment inputs',
  changed: 'actual diff summary',
  mapping: 'criterion-to-evidence mapping',
  examination: 'examination results and limitations',
  review: 'applicable review rounds',
  dispositions: 'finding dispositions',
  repairs: 'repair/rerun history',
  freshness: 'freshness/applicability',
  exceptions: 'exceptions',
  decisions: 'approval history',
})

/** The four kinds of evidence the brief keeps apart (`B10.5`). */
export const PROVENANCE = Object.freeze(['captured', 'imported', 'attested', 'generated'])

/* ── one evidence item ─────────────────────────────────────────────────── */

/**
 * One piece of evidence, or a refusal.
 *
 * Two things are required and neither is inferred: a provenance word off
 * `PROVENANCE`, and an address in the record to link to. An item missing
 * either is refused — shown as a refusal count on the page, never rendered
 * unlabelled and never rendered as a link that goes nowhere.
 *
 * @param {{provenance: string, session: string, seq: ?number, state: ?string,
 *   summary: ?string, criterion: ?number}} raw
 * @returns {{ok: true, item: object}|{ok: false, reason: string}}
 */
export function evidenceItem(raw) {
  const r = raw || {}
  const provenance = str(r.provenance)
  if (!provenance || !PROVENANCE.includes(provenance)) return { ok: false, reason: 'no_provenance' }
  const session = str(r.session)
  if (!session) return { ok: false, reason: 'no_record' }
  const seq = typeof r.seq === 'number' && Number.isFinite(r.seq) ? r.seq : null
  return {
    ok: true,
    item: {
      provenance,
      provenanceWord: P.provenance[provenance],
      session,
      seq,
      state: str(r.state),
      summary: str(r.summary),
      criterion: typeof r.criterion === 'number' ? r.criterion : null,
      record: recordAddress(session, seq),
    },
  }
}

/**
 * Where a piece of evidence lives, as an address a reader can open.
 *
 * The address this build can honestly give is the piece of linked work and
 * the step inside it — `lib/receipt.js` calls that pair File and Line, and
 * the file part is not on the routes this page may read. It is a link to the
 * full trace of that work, which is the technical layer, one click down and
 * never hidden (`03-USER-LENS.md` rule 1).
 *
 * @param {string} session
 * @param {?number} seq
 * @returns {{href: string, where: string, link: string}}
 */
export function recordAddress(session, seq) {
  return {
    href: '/sessions?session=' + encodeURIComponent(session) + '#/trace',
    where: seq == null ? P.record.at_work : P.record.at_step.replace('{n}', String(seq)),
    link: P.record.link,
  }
}

/**
 * Every finding of the confirmed work, as evidence items.
 *
 * A finding is a CAPTURED observation: it is what the record on this machine
 * holds, read by the engine. It is labelled so even though it is the only
 * kind this build can produce — an unlabelled item on a page whose whole
 * promise is "you can tell where this came from" is the defect, not the
 * saving.
 *
 * @param {Array<{id: string, findings: object[]}>} work
 * @returns {{items: object[], refused: number}}
 */
export function findingEvidence(work) {
  const items = []
  let refused = 0
  for (const w of list(work)) {
    for (const f of list(w && w.findings)) {
      const made = evidenceItem({
        provenance: 'captured',
        session: str(f.sessionId) || str(w.id),
        seq: typeof f.seq === 'number' ? f.seq : null,
        state: str(f.state),
      })
      if (made.ok) items.push({ ...made.item, card: f })
      else refused += 1
    }
  }
  return { items, refused }
}

/* ── the criterion-to-evidence mapping ─────────────────────────────────── */

/**
 * One line per thing that counts as done, and what was looked at for it.
 *
 * @param {string[]} criteria
 * @param {object[]} evidence typed entries carrying `criterion` (1-based)
 * @returns {{rows: Array<{n: number, text: string, items: object[], nothing: boolean}>, refused: number}}
 */
export function criteriaMapping(criteria, evidence) {
  const rows = list(criteria).map((text, i) => ({ n: i + 1, text: String(text), items: [], nothing: true }))
  let refused = 0
  for (const raw of list(evidence)) {
    const made = evidenceItem(raw)
    if (!made.ok) {
      refused += 1
      continue
    }
    const n = made.item.criterion
    if (n == null || n < 1 || n > rows.length) {
      refused += 1
      continue
    }
    rows[n - 1].items.push(made.item)
    rows[n - 1].nothing = false
  }
  return { rows, refused }
}

/**
 * The mapping, straight off the served coverage document.
 *
 * Every field here is a typed field of that document. `nothing` is kept for the
 * view's sake and means exactly what it meant before: no item under this line.
 *
 * @param {object} coverage the `coverage` object of `/api/changes/<id>/coverage`
 * @returns {{rows: Array<object>, refused: number}}
 */
export function mappingFromCoverage(coverage) {
  const rows = coverageRows(coverage).map((row) => ({
    n: row.n,
    text: row.text,
    state: row.state,
    sentence: row.sentence,
    tool: row.tool,
    items: row.items.map((item) => ({
      provenance: 'captured',
      provenanceWord: P.provenance.captured,
      session: item.session,
      seq: null,
      state: item.state,
      summary: item.name,
      criterion: row.n,
      bears: item.bears,
      why: item.basisText,
      note: item.note,
      /* T-3-2: whether this examination was about the version being approved,
       * and the sentence either way. An item that was not is still printed —
       * the packet hides nothing — and it counts for nothing. */
      applies: item.applies,
      appliesText: item.appliesText,
      record: recordAddress(item.session || '', null),
    })),
    removed: row.removed.map((item) => ({
      who: item.actor || '',
      summary: item.name,
    })),
    nothing: row.items.length === 0,
  }))
  return { rows, refused: 0 }
}

/**
 * What to read first: the things that count as done with nothing behind them,
 * and the things the record holds the opposite of.
 *
 * @param {Array<{n: number, text: string, items: object[], nothing: boolean}>} rows
 * @returns {Array<{n: number, text: string, why: string}>}
 */
export function stillNeedsEvidence(rows) {
  const out = []
  for (const row of list(rows)) {
    /* T-3-1: the served state decides, when there is one. A line whose only
     * evidence settled nothing still needs evidence, and saying "no test ran
     * for this" about it would be false — something ran. */
    if (row.state === 'ran_nothing') {
      out.push({ n: row.n, text: row.text, why: P.mapping.ran_nothing })
      continue
    }
    /* T-3-2: something ran, and not on this version. It still needs evidence,
     * and saying "no test ran for this" about it would be false. */
    if (row.state === 'other_version') {
      out.push({ n: row.n, text: row.text, why: P.mapping.other_version })
      continue
    }
    /* T-3-6-F1 / R-2026-09-23: a finished, counted run is on record and is
     * linked to nothing. "No test ran for this" would be false about it. */
    if (row.state === 'unlinked_run') {
      out.push({ n: row.n, text: row.text, why: P.needs.unlinked_run })
      continue
    }
    if (row.state === 'absent_tool') {
      out.push({ n: row.n, text: row.text, why: fill(P.mapping.absent_tool, { tool: row.tool || '' }) })
      continue
    }
    if (row.nothing) {
      out.push({ n: row.n, text: row.text, why: P.needs.no_test })
      continue
    }
    if (row.items.some((i) => i.state === 'not_proven')) out.push({ n: row.n, text: row.text, why: P.needs.problem })
  }
  return out
}

/**
 * BRIEF-COUNT-AGREES-R2 — what still needs evidence, from the served readiness.
 *
 * `stillNeedsEvidence` above reads the coverage: does any evidence bear on the
 * line. That is not what the top of this page says, or what the Change screen's
 * list says, or Ask — those say the served readiness's, where a line whose
 * applicability was never assessed still needs evidence and a line a named
 * person set aside never does. The walk's own Change read "2 things still need
 * evidence" above a list of one. So where the answer is served, THIS is the
 * list: the lines the answer names, in the answer's order, each with the
 * answer's own reason for it. `stillNeedsEvidence` is the fallback for a page
 * with no answer at all (absence of an answer is not an answer).
 *
 * @param {{needing: Array<{n: number, text: string, slug: ?string, reason: ?string}>}} tally
 *   `hq.readiness.tally`, the one count
 * @returns {Array<{n: number, text: string, why: string, slug: ?string}>}
 */
export function needsFromAnswer(tally) {
  return list(tally && tally.needing).map((m) => ({ n: m.n, text: m.text, why: m.reason || P.needs.no_test, slug: m.slug }))
}

/**
 * What the record holds against what was said, as evidence items.
 *
 * The other half of Sam's second question. The walk found a page that said
 * "two things still need evidence" at the top and put the one thing the record
 * actually contradicts forty lines below, in a section a person reading the
 * top for ten seconds never reaches. Every item here is rendered again, in
 * full, in its own section; this is the pointer, not a second source.
 *
 * `not_proven` is the record holding the opposite. `loose` is something
 * promised with no follow-through. `couldnt` and `fog` are NOT here: a gap in
 * the recording is not a thing that is wrong, and putting it here would be
 * exactly the collapse `B7.2` forbids.
 *
 * @param {object[]} items evidence items from `findingEvidence`
 * @returns {object[]}
 */
export function heldAgainst(items) {
  return list(items).filter((i) => i && (i.state === 'not_proven' || i.state === 'loose'))
}

/* ── suggestions that are not already accepted ─────────────────────────── */

/**
 * One thing-that-counts-as-done, reduced to the sentence a person reads.
 *
 * Case and spacing are not part of what a sentence SAYS, so two lines that
 * differ only in those are the same line to the person reading them. Nothing
 * else is stripped: two different sentences stay two different sentences.
 *
 * @param {string} text
 * @returns {string}
 */
export function sameLine(text) {
  return String(text == null ? '' : text).replace(/\s+/g, ' ').trim().replace(/\.+$/, '').toLowerCase()
}

/**
 * The suggestions that are genuinely NOT accepted.
 *
 * WHY THIS EXISTS. `/api/changes/<id>/proposals` re-derives its suggestions
 * from the request text on every read — it is a live proposal, not a cache —
 * so a Change whose scope was accepted straight off that proposal (the normal
 * path: `change accept --criterion N` accepts exactly what was suggested)
 * comes back with the accepted lines ALSO sitting in the suggestions
 * document. Rendering both blocks unconditionally put the same sentence on
 * the page twice: once as "Accepted at version 1 by sam" and once under
 * "Suggested, and not accepted — nothing counts until somebody accepts it".
 * A person answering "what am I being asked to accept?" met two opposite
 * answers about the same line, four lines apart. Found by the T-2-5 verifier
 * on a real Change through the real server.
 *
 * This is DE-DUPLICATION, not inference: a suggestion is dropped only when
 * its sentence is already accepted word for word. A suggestion that says
 * anything the accepted scope does not say survives and is still labelled as
 * not accepted, which is what `B10.4` asks for.
 *
 * @param {Array<{text: string}>} proposals
 * @param {string[]} accepted the accepted criteria, as rendered
 * @returns {Array<{text: string}>}
 */
export function unacceptedProposals(proposals, accepted) {
  const already = new Set(list(accepted).map(sameLine))
  return list(proposals).filter((p) => p && !already.has(sameLine(p.text)))
}

/* ── which sections hold anything ──────────────────────────────────────── */

/**
 * Whether each section has anything in it, in the brief's order.
 *
 * The four that are always false in this build are false because nothing in
 * this build records them, not because this Change is short of them; each
 * one's `empty` sentence says which.
 *
 * @param {object} m the model under construction
 * @returns {Array<{id: string, heading: string, filled: boolean, empty: string}>}
 */
export function sectionsOf(m) {
  const model = m || {}
  const filled = {
    purpose: Boolean(str(model.request && (model.request.purpose || model.request.title))),
    accepted: Boolean((model.accepted && model.accepted.criteria.length) || list(model.proposals).length),
    target: Boolean(model.prepare && (model.prepare.repository || model.prepare.base || model.prepare.snapshot)),
    changed: Boolean(list(model.build).length),
    mapping: Boolean(model.mapping && model.mapping.rows.length),
    /* PACKET-CHECKED-TRUE — every recorded source this page shows: the
     * linked work AND every run its "Every run" section lists, linked or not
     * (`examinedRuns`) AND any evidence its mapping section shows
     * (`recordedRows`).
     * "Nothing has been checked" only when none of them holds anything. */
    examination: Boolean(list(model.work).length || list(model.checks).length || list(model.unlinkedRuns).length || list(model.recorded).length),
    /* Phase 4 writes these two. Nothing in this build can. */
    review: false,
    dispositions: false,
    /* T6 — filled exactly when the Change holds a run: the rows are the one
     * derivation's (`change-model.js` `repairHistoryOf`), and with no run at
     * all the section's own sentence — "nothing has been checked a second
     * time" — is the true one. */
    repairs: Boolean(model.hq && model.hq.packet && list(model.hq.packet.repairs).length),
    /* Phase 3 writes this one. Until then the answer is UNKNOWN and the
     * section's own sentence says unknown — never "yes". */
    freshness: false,
    /* Phase 5 writes this one. */
    exceptions: false,
    /* T-S-4 writes this one, and T-S-4-F2 wired it: the approval history is
     * real the moment a decision is recorded, and the packet reads the SAME
     * derived field the Change screen and the Inbox row read
     * (`change-model.js`'s `decision.history`) rather than a second reading of
     * the same rows. A Change with no decision still renders the section's own
     * sentence, which is what "nobody has decided" honestly looks like. */
    decisions: Boolean(model.hq && model.hq.decision && model.hq.decision.history.length),
  }
  return SECTIONS.map((id) => ({
    id,
    heading: P.sections[id].heading,
    filled: filled[id] === true,
    empty: P.sections[id].empty,
  }))
}

/* ── what this page is made of (03-USER-LENS.md rule 13) ───────────────── */

/**
 * The footer's two counts, from typed fields and nothing else.
 *
 * `steps` is the count the reading of each piece of linked work already
 * carries (`verdict.checked.events`); it is summed here and never estimated,
 * never derived from elapsed time, never rounded.
 *
 * @param {Array<{steps: ?number}>} work
 * @param {number} [recorded] what else the record holds for this Change (`recordedCount`)
 * @returns {{work: number, steps: number, text: string}}
 */
export function footerOf(work, recorded = 0) {
  const w = list(work)
  let steps = 0
  for (const one of w) if (one && typeof one.steps === 'number' && one.steps > 0) steps += one.steps
  /* PACKET-CHECKED-TRUE — "nothing has been written down" only when nothing
   * has: no linked work AND nothing else in the record for this Change. */
  if (!w.length && recorded > 0) {
    return { work: 0, steps: 0, text: recorded === 1 ? P.footer.recorded_one : fill(P.footer.recorded_many, { n: recorded }) }
  }
  if (!w.length) return { work: 0, steps: 0, text: P.footer.none }
  const workPhrase = w.length === 1 ? P.footer.work_one : P.footer.work_many.replace('{n}', String(w.length))
  const stepPhrase = steps === 1 ? P.footer.steps_one : P.footer.steps_many.replace('{n}', String(steps))
  return { work: w.length, steps, text: P.footer.counts.replace('{work}', workPhrase).replace('{steps}', stepPhrase) }
}

/**
 * PACKET-CHECKED-TRUE (QA P0-6, verifier tries 1 and 2) — the rows of this
 * packet's own "Every run, and the version it ran on" (`hq.packet.sections`,
 * id `runs`), verbatim, split by whether the run counts for this Change. One
 * source: the examination section, Proof's neighbour and every exported file
 * read the same rows, and `runsOf` in `change-model.js` is where a run on
 * other code, an unsettled run and an unlinked run are worded.
 *
 * @param {?object} hq the one derived view-model
 * @returns {{linked: Array<{text: string, lines: string[]}>, unlinked: Array<{text: string, lines: string[]}>}}
 */
export function examinedRuns(hq) {
  const runs = hq && hq.packet ? list(hq.packet.sections).find((x) => x && x.id === 'runs') : null
  const rows = list(runs && runs.rows).filter((r) => r && str(r.text)).map((r) => ({ text: r.text, lines: list(r.lines), linked: r.linked !== false }))
  return { linked: rows.filter((r) => r.linked), unlinked: rows.filter((r) => !r.linked) }
}

/**
 * How many distinct things the record holds for this Change outside linked
 * work: every item under an accepted line, and every run nobody linked. The
 * footer reads this so it never says nothing was written down beside them.
 *
 * @param {?object} hq
 * @returns {number}
 */
export function recordedCount(hq) {
  const cov = hq && hq.coverage
  const ids = new Set()
  for (const row of list(cov && cov.rows)) for (const i of list(row && row.items)) if (i && i.evidenceId) ids.add(i.evidenceId)
  for (const c of list(cov && cov.candidates)) if (c && c.kind === 'check_run' && c.evidenceId) ids.add(c.evidenceId)
  return ids.size
}

/**
 * PACKET-CHECKED-TRUE — the mapping section's lines that hold recorded
 * evidence, in that section's own words: the line, its sentence, and the name
 * of each item under it. An observation is not a run, so "Every run" leaves it
 * out; the mapping still shows it, and the examination section may not then
 * say nothing was looked at.
 *
 * @param {{rows: object[]}} mapping
 * @returns {Array<{n: number, text: string, sentence: ?string, names: string[]}>}
 */
export function recordedRows(mapping) {
  return list(mapping && mapping.rows)
    .filter((row) => row && list(row.items).length)
    .map((row) => ({
      n: row.n,
      text: row.text,
      sentence: str(row.sentence),
      names: list(row.items).map((i) => str(i && i.summary)).filter(Boolean),
    }))
}

/** The derived model's count of what still needs evidence, when the answer was served. */
function servedTally(hq) {
  const r = hq && hq.readiness
  return r && !r.absent && r.tally ? r.tally : null
}

/* ── the whole page, decided in one place ──────────────────────────────── */

/**
 * Everything the page renders, from the documents and nothing else.
 *
 * It is built ON TOP of `buildChangeView`, deliberately: the title, the one
 * conclusion sentence and the one named version must be the same words on
 * both screens, and the only way to guarantee that is for there to be one
 * function that decides them.
 *
 * @param {object} input the same shape `buildChangeView` takes, plus
 *   `work[].steps` for the footer
 * @returns {object}
 */
export function buildPacketView(input) {
  const i = input || {}
  /* One source for both surfaces. The Change screen's top sentence and this
   * page's mapping are built from the same entries, so the two cannot report a
   * different number of things needing evidence for one Change. */
  const base = buildChangeView(
    i.coverage
      ? { ...i, evidence: evidenceEntries(i.coverage), unlinkedRun: unlinkedCompletedRun(i.coverage), unlinkedLines: servedUnlinkedLines(i.coverage).length }
      : i,
  )
  const mapping = i.coverage
    ? mappingFromCoverage(i.coverage)
    : criteriaMapping(base.accepted.criteria, list(i.evidence))
  const found = findingEvidence(base.work)
  const scope = (i.change && i.change.accepted_scope) || null
  const model = {
    ...base,
    /* NEVER-ASSESSED-ON-SCREEN — the top sentence is the derived model's
     * (`hq.conclusion`), so this page never says "0 still need it" over a
     * readiness answer that is not ready. Without `hq`, the view's own. */
    conclusion: i.hq && i.hq.conclusion ? i.hq.conclusion : base.conclusion,
    mapping,
    /* What the suggestions document still suggests that nobody has accepted.
     * The raw `proposals` list stays on the model untouched, because it is
     * what the document said; `unaccepted` is what this page may call
     * unaccepted without contradicting the line above it. */
    unaccepted: unacceptedProposals(base.proposals, base.accepted.criteria),
    /* BRIEF-COUNT-AGREES-R2 — the served answer's lines when it is served; the
     * coverage's only without one. */
    needs: servedTally(i.hq)
      ? needsFromAnswer(i.hq.readiness.tally)
      : stillNeedsEvidence(mapping.rows),
    wrong: heldAgainst(found.items),
    findings: base.findings,
    evidence: found.items,
    refused: mapping.refused + found.refused,
    exclusions: list(scope && scope.exclusions).map(String),
    acceptance: scope && scope.acceptance
      ? {
        actor: str(scope.acceptance.actor),
        date: dayOf(str(scope.acceptance.accepted_at)),
        why: str(scope.acceptance.rationale),
      }
      : null,
    snapshot: base.prepare ? base.prepare.snapshot : null,
  }
  /* T-S-4-F2 — the one derived view-model, before the sections are decided, so
   * "is there an approval history" is answered by the same field the Change
   * screen reads rather than by a second reading of the same rows. */
  model.hq = i.hq || null
  const runs = examinedRuns(model.hq)
  model.checks = runs.linked
  model.unlinkedRuns = runs.unlinked
  model.recorded = recordedRows(model.mapping)
  model.footer = footerOf(base.work, recordedCount(model.hq))
  model.sections = sectionsOf(model)
  return model
}

export default buildPacketView
