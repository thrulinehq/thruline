/* ONE derived view-model for the Change (T-S-3; design record §3.4, §3.8,
 * §5, §10, binding by R-2026-09-22-3).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness. One function
 * over the four documents the server already serves — `/api/changes/<id>`,
 * `/api/changes/<id>/coverage`, `/api/changes/<id>/applicability`,
 * `/api/changes/<id>/proposals` — and nothing else.
 *
 * WHY THIS FILE EXISTS. The design record's §10 caution, in its own words:
 * every one-truth-per-screen defect in four rounds came from strings written
 * per screen. The Change screen said one thing, the Inbox row said what was
 * true an hour ago, and the packet said a third thing, because each of them
 * derived its own state word, its own version string and its own count. This
 * is the single derivation. The Change screen, the Inbox row and the packet
 * page render `deriveChangeModel`'s output and do not compute a state word, a
 * version string or a count of their own — `change-model.test.js` greps the
 * three screens for the five state words and fails when one reappears.
 *
 * WHAT IT MAY SAY. Every word is `HQ_COPY.change.model` or a sentence the
 * ENGINE authored and served (the freshness lines, mirrored word for word into
 * `HQ_COPY.change.coverage.freshness` and fenced by a Rust test). Two things
 * are the RECORD speaking and are passed through: the Change's own title and
 * the evidence's own name.
 *
 * THE RULES IT HOLDS.
 *
 *  - The five state words of §5, and no sixth. `STATE_WORDS` is the closed set.
 *  - "Proven" NEVER appears without its scope and its version (§3.8). The
 *    builder of that line refuses to produce it when either is missing, and
 *    falls back to the bare word with nothing claimed around it.
 *  - Freshness outranks everything. A criterion whose evidence was proven on a
 *    version the record has since moved off is `Needs a re-check` on ALL THREE
 *    surfaces at once, because all three read this field — until a run on the
 *    version it moved to holds, which is the re-check (`criterionState`).
 *  - Missing evidence is missing (`B7.2`, `B7.15`) — never a pass, never a
 *    failure. It is `In progress`.
 *  - Nothing is inferred from words. The criterion→evidence link is the
 *    engine's documented rule, read out of the coverage document; the
 *    freshness is the engine's dependency rule, read out of the applicability
 *    document. This file matches nothing by its prose.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill, compose, SlotError } from './hq-narrate.js'
import { buildChangeView, nothingWrongLine, readinessConclusion } from './change-view.js'
import { buildCoverageView, evidenceEntries, failureOf, servedUnlinkedLines, unlinkedCompletedRun } from './coverage-view.js'
import { attentionOf } from './attention.js'

const M = HQ_COPY.change.model
const F = HQ_COPY.change.coverage.freshness
/* T9 — the Technical line of the one applicability rule. */
const CARRIED = HQ_COPY.change.coverage.carried

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/** The five state words of design §5, keyed by the model's own state id. */
export const STATE_WORDS = Object.freeze({ ...M.states })

/** The five words themselves, as a closed set, for the fence in the tests. */
export const STATE_WORD_SET = Object.freeze(Object.values(M.states))

/* ── freshness: what the engine said about whether it still applies ────── */

/**
 * The freshness of one criterion, from `GET /api/changes/<id>/applicability`.
 *
 * The served items are per EVIDENCE link. A criterion is stale when any piece
 * of evidence behind it is stale — one piece of evidence that no longer
 * applies is enough to stop the line being settled, and saying otherwise would
 * be the product averaging away the fact it exists to carry.
 *
 * NO RECORD IS NOT A DENIAL. A criterion whose evidence has never been
 * assessed gets `null`, and the caller shows nothing rather than "still
 * applies" guessed in the absence of a fact.
 *
 * @param {object} row the coverage row (carries `n`, the criterion index)
 * @param {object|null} doc the applicability document
 * @param {Map<string,string>|null} [names] the one short-form-per-version map
 *   for every version this document names (T-5-2's `shortVersions`), computed
 *   once by the caller so two criteria cannot name two trees the same way
 * @param {{version: string|null}|null} [moved] APPLIES-VS-MOVED: given when the
 *   served Change says the code moved since the version was recorded. The
 *   engine's "still applies" compares RECORDED versions; the git line reads the
 *   live folder; "unchanged" beside it reads as "the code has not changed". So
 *   a line that still applies names its frame instead — the version as
 *   recorded, and that the code has changed since. Nothing else moves: a line
 *   that needs a re-check never said "unchanged", and with no `moved` every
 *   field is what it was.
 * @returns {{fresh: boolean, sentence: string, technical: string,
 *   reasonClass: string|null, reason: string|null, paths: string[],
 *   checkedPaths: string[]}|null}
 */
export function freshnessOf(row, doc, names = null, moved = null) {
  const n = row && typeof row.n === 'number' ? row.n : null
  const assessed = list(row && row.items).filter((i) => i && i.fresh !== null && i.fresh !== undefined)
  if (!assessed.length) return null
  const stale = assessed.find((i) => i.fresh === false) || null
  const chosen = stale || assessed[0]
  /* The reason class and the paths are the applicability document's, matched
   * to this link by the id the two documents share. No row there is no reason
   * — the line still says it needs a re-check, and the drawer says nothing
   * rather than inventing which class of thing moved. */
  const detail = list(doc && doc.items).find(
    (i) => i && (String(i.link_id || '') === String(chosen.id) || Number(i.criterion_index) === Number(n)),
  ) || {}
  const slug = str(chosen.freshSlug) || (stale ? 'applicability_needs_a_recheck' : 'applicability_still_applies')
  const paths = list(detail.reason_paths).map(String).filter(Boolean)
  /* T-F-1-F1 — the subject of the "still applies" sentence.
   *
   * `reason_paths` is EMPTY on an `Applies` record by design: there is
   * nothing invalidating to name. What that sentence is about is the thing
   * the rule COMPARED — the check profile's declared paths, or the accepted
   * scope when it declares none — which the engine serves as `checked_paths`.
   * And the version is the one the evidence's own snapshot recorded
   * (`old_snapshot`), named the way every other version on this screen is
   * named. Before this, both were read from a field the served item does not
   * carry, and the line rendered "Still applies — this path unchanged since
   * ." with two holes in it.
   *
   * The short form is T-5-2's `shortVersions`, decided over EVERY version the
   * applicability document names, not a fixed seven characters: two snapshots
   * taken seconds apart share a ULID's leading ten, and a label that can name
   * the wrong tree is not a name. `names` is that map, computed once per
   * document by the caller; a call without one falls back to the same rule
   * over this item's own versions alone. */
  const checked = list(detail.checked_paths).map(String).filter(Boolean)
  const raw = str(detail.version) || str(detail.proven_version) || str(detail.old_snapshot)
  const forms = names || shortVersions(versionsNamedBy(doc))
  const version = raw ? forms.get(raw) || raw : null
  /* Two different `{path}`s, and they are not interchangeable. The HEADLINE's
   * path is the one the rule CHECKED; the REASON's path is the one that
   * MOVED. Filling the headline from the reason would have it say a path was
   * unchanged when that path is exactly the one that changed. */
  const values = { path: checked[0] || paths[0] || null, version }
  const reasonValues = { path: paths[0] || checked[0] || null, version }
  const reasonSlug = str(detail.reason_slug)
  const reasonLine = reasonSlug && F[reasonSlug] ? F[reasonSlug] : null
  const headline = namedLine(slug, values)
  /* T9 — what the rule compared, for the Technical drawer of a line that
   * still applies: the path it checked and the two snapshots it compared,
   * old → new, in the same short forms as every other version here. Typed
   * fields only; a stale line carries nothing forward. */
  const short = (v) => (str(v) ? forms.get(str(v)) || str(v) : null)
  const carried = stale ? null : { path: checked[0] || null, from: short(detail.old_snapshot), to: short(detail.new_snapshot) }
  const carriedTechnical = carried
    ? say(CARRIED.applicability_carried_forward, { path: carried.path, version: carried.from, now: carried.to }) || CARRIED.applicability_carried_forward_unnamed
    : null
  const framed = moved && !stale
  return {
    fresh: !stale,
    sentence: framed
      ? (moved.version ? fill(M.applies_as_recorded.named, { version: moved.version }) : M.applies_as_recorded.unnamed)
      : headline.plain,
    technical: headline.technical,
    reasonClass: reasonSlug,
    /* The reason lives ONE LEVEL DOWN (the card): the line says the evidence
     * needs a re-check, and the drawer says which class of thing moved. */
    reason: reasonLine ? say(reasonLine.plain, reasonValues) : null,
    reasonTechnical: reasonLine ? say(reasonLine.technical, reasonValues) : null,
    paths,
    checkedPaths: checked,
    carried,
    carriedTechnical: framed && carriedTechnical ? carriedTechnical + ' ' + M.applies_as_recorded.carried_tail : carriedTechnical,
    rule: CARRIED.applicability_rule,
  }
}

/** Every version an applicability document names, for the one short-form set. */
function versionsNamedBy(doc) {
  const out = []
  for (const i of list(doc && doc.items)) {
    if (!i) continue
    out.push(str(i.version), str(i.proven_version), str(i.old_snapshot), str(i.new_snapshot))
  }
  return out.filter(Boolean)
}

/**
 * One sentence, or null when the record cannot fill a slot in it (T-F-1-F1).
 *
 * `compose` refuses an empty or missing value rather than rendering the stray
 * space and the orphaned stop — " since ." — so a sentence whose subject the
 * record does not carry comes back as nothing at all, and the caller says
 * nothing rather than showing a hole. Any other error is a bug and is thrown.
 */
function say(template, values) {
  try {
    return compose(template, values)
  } catch (e) {
    if (e instanceof SlotError) return null
    throw e
  }
}

/**
 * The freshness headline for one slug, in both registers, with every slot
 * filled by a value the record actually carries (T-F-1-F1).
 *
 * When either register has a slot the record cannot fill, BOTH fall back to
 * the `_unnamed` variant of the same conclusion — the sentence that states
 * the conclusion and says the subject is not named, which is true — and never
 * to a half-filled line. The fallback carries no slots, so it always
 * composes; if a slug somehow has neither, the honest re-check line (also
 * slotless) is the floor, because a freshness line that renders nothing would
 * read as "nothing to say here", which is a stronger claim than the record
 * makes.
 */
export function namedLine(slug, values) {
  const candidates = [F[slug], F[`${slug}_unnamed`], F.applicability_needs_a_recheck]
  for (const line of candidates) {
    if (!line) continue
    const plain = say(line.plain, values)
    const technical = say(line.technical, values)
    if (plain !== null && technical !== null) return { plain, technical }
  }
  return { plain: F.applicability_needs_a_recheck.plain, technical: F.applicability_needs_a_recheck.technical }
}

/* T-3-6-F3 verifier finding 1: `failureOf` used to live here and be applied
 * to a COPY of the coverage items, so the packet carried what broke and the
 * Verify stage — the screen "Look at what broke" opens — rendered the raw
 * items and carried neither it nor the seeded label. It now lives on the one
 * item derivation in `coverage-view.js`, where every consumer goes through it.
 * Re-exported so the name stays where callers already found it. */
export { failureOf }

/** The reason classes named across a Change's criteria, in Sam's words, once each. */
export function changedSince(criteria) {
  const seen = []
  for (const c of list(criteria)) {
    const r = c && c.freshness && c.freshness.reason
    if (r && !seen.includes(r)) seen.push(r)
  }
  if (!seen.length) return { any: false, reasons: [], text: M.changed_since_none }
  const template = seen.length === 1 ? M.changed_since_one : M.changed_since_many
  return { any: true, reasons: seen, text: fill(template, { reasons: seen.join(' ') }) }
}

/**
 * STEP-7-LIMITS (PW-F12) — what the record cannot say about the code, as the
 * lines every surface prints: the top of Brief and Explore, beside Approve,
 * the page's "What changed" section, the exported file and Ask's
 * changed-since answer.
 *
 * ThruLine records the code when a Change is made and when a check runs. The
 * limit line says so, and never that the code moved or that it did not.
 *
 * STEP-7-GIT-LINE (R-2026-09-28-42) — when the served Change carries
 * `code_moved` (the commit recorded with the version against the folder's
 * commit when the page was read, from git), `codeMovedOf` makes ONE sentence
 * of it and it goes FIRST in `lines`, beside the limit and never instead of
 * it, so every surface carries it without being touched. With no field, a
 * `null` field or equal commits there is no moved line at all: silence, which
 * is exactly what the base rendered, and never "unchanged".
 *
 * @param {?string} shortVersion the version the screen names, or null
 * @param {?object} served the served Change's `code_moved`, or null
 * @returns {{lines: string[], text: string, moved: ?object}}
 */
export function codeLimitOf(shortVersion, served = null) {
  const L = M.code_limit
  const moved = codeMovedOf(served, shortVersion)
  const lines = [...(moved ? [moved.sentence] : []), shortVersion ? fill(L.named, { version: shortVersion }) : L.unnamed]
  return { lines, text: lines.join(' '), moved }
}

/** How many paths the sentence names before it says how many more. */
export const CODE_MOVED_NAMED = 3

/**
 * STEP-7-GIT-LINE — the served `code_moved` as the words a person reads, and
 * its receipt one level down. It claims only that the code moved: anything it
 * cannot read as two different commits is `null`.
 *
 * @param {?object} served `{recorded, now, changed_paths, commits, uncommitted_compared}`
 * @param {?string} shortVersion
 * @returns {?{sentence: string, clause: string, paths: string[], commits: number, recorded: string, now: string, heading: string, receipt: string, uncommitted: string, uncommittedCompared: boolean}}
 */
export function codeMovedOf(served, shortVersion) {
  if (!served || typeof served !== 'object') return null
  const recorded = str(served.recorded)
  const now = str(served.now)
  if (!recorded || !now || recorded === now) return null
  const C = M.code_moved
  const paths = list(served.changed_paths).map(String).filter(Boolean)
  const commits = typeof served.commits === 'number' && served.commits > 0 ? served.commits : 0
  const files = paths.length === 1 ? C.files_one : fill(C.files_many, { n: paths.length })
  const commitWords = commits === 1 ? C.commits_one : fill(C.commits_many, { n: commits })
  const what = paths.length && commits
    ? fill(C.files_in_commits, { files, commits: commitWords })
    : paths.length ? files : commitWords
  const named = paths.slice(0, CODE_MOVED_NAMED).join(', ')
    + (paths.length > CODE_MOVED_NAMED ? ' ' + fill(C.more, { n: paths.length - CODE_MOVED_NAMED }) : '')
  const lead = shortVersion ? fill(C.lead_named, { version: shortVersion }) : C.lead_unnamed
  const sentence = lead + ': ' + what + (named ? ' — ' + named : '') + '. ' + C.tail
  return {
    sentence,
    /* STEP-7-FIRST-READ — the sentence's own lead, alone, for the one place
     * that has room for a clause and not a receipt: the Inbox card. It names
     * the version and claims only that the code moved. */
    clause: lead + '.',
    paths,
    commits,
    recorded,
    now,
    heading: C.receipt_heading,
    receipt: fill(C.receipt, { recorded, now }),
    uncommitted: C.uncommitted,
    uncommittedCompared: served.uncommitted_compared === true,
  }
}

/* ── the state of one criterion ────────────────────────────────────────── */

/**
 * The one state id for one accepted line, from typed fields only.
 *
 * The order is the order of what a person needs to know first:
 *
 *   evidence that no longer applies  → needs_recheck
 *   the record holds the opposite    → not_proven
 *   the record backs it              → proven
 *   a run that settled nothing       → recorded
 *   nothing behind it at all         → in_progress
 *
 * A3 g2 r1 — THE RE-CHECK IS THE RE-CHECK. Stale evidence stops the line
 * being settled, except in one case, where the engine has already said the
 * opposite: every stale item is about ANOTHER version (`applies === false`),
 * and a run on THIS version (`applies === true`) that the record does not
 * call stale holds, with nothing on this version showing the opposite. That
 * is `change_readiness`'s own rule: one admissible item (bears, Proven,
 * applies to this version, not stale) satisfies the line. The same screen
 * then says "Ready for approval … It rests on" this line, and until this
 * case existed the chip beside it said "Needs a re-check" about a re-check
 * that had happened and passed. A stale item on this version, or a failing
 * run on this version, still leaves the line short of Proven.
 *
 * @param {{state: string, items: object[]}} row a coverage row
 * @param {object|null} freshness what `freshnessOf` returned for it
 * @returns {string} one of the keys of `STATE_WORDS`
 */
/** A3-F12 — the Inspector draws this many cards, and this many pieces of evidence, in full. */
export const INSPECTOR_FOLD_AFTER = 1

/** A failure with its own words is never folded (the never-fold rule, as in `lod.js`). */
const neverFolds = (x) => Boolean(x && x.state === 'not_proven' && ((x.failure && x.failure.line) || x.failure_line))

/**
 * Split a list into what the Inspector draws in full and what one disclosure
 * holds: the first `foldAfter`, plus every failure with its own words wherever
 * it sits, in the order given. Nothing is dropped; `folded.length` is the
 * count the disclosure says.
 *
 * @template T
 * @param {T[]} items
 * @param {number} foldAfter
 * @returns {{shown: T[], folded: T[]}}
 */
export function foldList(items, foldAfter) {
  const shown = []
  const folded = []
  list(items).forEach((x, n) => ((n < foldAfter || neverFolds(x)) ? shown : folded).push(x))
  return { shown, folded }
}

export function criterionState(row, freshness) {
  const items = list(row && row.items)
  const bearing = items.filter((i) => i && i.bears === true && i.applies !== false)
  if (freshness && freshness.fresh === false && bearing.length && !recheckedHere(items, bearing)) return 'needs_recheck'
  if (bearing.some((i) => i.state === 'not_proven')) return 'not_proven'
  if (bearing.some((i) => i.state === 'proven')) return 'proven'
  if (items.length) return 'recorded'
  return 'in_progress'
}

/**
 * Whether a run on this version has already answered the re-check that a
 * stale item from another version asked for: every item the record calls
 * stale is about another version, and a bearing run on this version, not
 * stale, holds while none shows the opposite. Typed fields only (`fresh`,
 * `applies`, `bears`, the observed state); see `criterionState`.
 */
function recheckedHere(items, bearing) {
  const stale = items.filter((i) => i && i.fresh === false)
  if (!stale.length || !stale.every((i) => i.applies === false)) return false
  const current = bearing.filter((i) => i.fresh !== false)
  return current.some((i) => i.state === 'proven' && i.applies === true) && !current.some((i) => i.state === 'not_proven')
}

/**
 * The line a person reads for one criterion, on every surface.
 *
 * §3.8 is mechanical here: `Proven` is only ever produced by `proven_line`,
 * which needs BOTH a version and a scope. With either missing the state is
 * still Proven in the record and the line says so WITHOUT the word standing
 * alone as a claim — it falls back to the state line with its version, and
 * when there is no version either, to the bare word. The word can therefore
 * never appear on a screen unaccompanied by what it was proven about.
 *
 * @returns {string}
 */
export function stateLine(state, version, scope, what) {
  const word = STATE_WORDS[state] || STATE_WORDS.in_progress
  if (state === 'proven' && version && list(scope).length) {
    return fill(M.proven_line, { what: what || word, version, scope: list(scope).join(', ') })
  }
  if (version) return fill(M.state_line, { word, version })
  return word
}

/** What would settle one line, one sentence, by state (Inspector block 3). */
export function settleSentence(state) {
  const I = M.inspector
  return I['settle_' + state] || I.settle_in_progress
}

/* ── execution: what became of the run, beside what it observed ────────── */

/**
 * T-3-4 (`B7.1`, `B7.6`, `Q12`) — one card per run, with the EXECUTION state
 * and the OBSERVED result as **two fields** (design record §3.8: never one
 * badge).
 *
 * The rule this holds, and the reason the card exists at all: a run that was
 * cancelled, interrupted or blocked **never carries a state word**. Not
 * "Proven", not "Not proven" — a check that did not reach the end observed
 * nothing to be right or wrong about, and putting any of the five words on it
 * is the product answering a question the record cannot answer. `word` is null
 * for every one of those, and `change-model.test.js` fences it.
 *
 * The one primary action on such a card is *Run it again*, and it is the only
 * action: a re-run is a new examination with a new examined-inputs record
 * (T-3-2), so there is nothing to resume and nothing to repair.
 *
 * @param {object|null} cov what `buildCoverageView` returned
 * @returns {Array<object>}
 */
export function executionsOf(cov) {
  const X = M.execution
  return list(cov && cov.executions).map((e) => {
    const stopped = e.state === 'cancelled' || e.state === 'interrupted' || e.state === 'blocked'
    /* The reason sentence is the specific one when the record names a reason,
     * and the generic state sentence otherwise. "Interrupted when ThruLine
     * restarted" is a different fact from "interrupted", and the record knows
     * which, so the screen says which. */
    const reason = e.reasonSlug ? X.reasons[e.reasonSlug] || null : null
    return {
      runKey: e.runKey,
      bindingId: e.bindingId,
      name: e.name,
      state: e.state,
      /* THE EXECUTION HALF. */
      executionLine: reason || X.states[e.stateSlug] || X.states.execution_not_run,
      executionState: X.states[e.stateSlug] || X.states.execution_not_run,
      reasonLine: reason,
      /* THE OBSERVED HALF, said separately and never merged into the one
       * above. A stopped run settles nothing, and that is a sentence rather
       * than a silence. */
      settles: e.settles === true,
      observedLine: e.settles ? null : X.observed_nothing,
      /* `B12.6`: verified, never assumed, and "nothing looked" is its own
       * answer rather than a clean bill. */
      cleanupLine: e.cleanupVerified === true
        ? X.cleanup.verified
        : e.cleanupVerified === false
          ? X.cleanup.survivors
          : X.cleanup.unknown,
      survivors: e.survivors,
      stopped,
      running: e.state === 'running',
      /* NEVER a state word on a run that did not finish. */
      word: null,
      /* ONE primary action, chosen by the execution state and by nothing else:
       * a run still going can be stopped; a run that was stopped can be run
       * again; a run that finished needs neither. There is never a second
       * control on one of these cards — "resume" is not a thing the record can
       * offer, because a re-run is a new examination (T-3-2). */
      action: e.state === 'running' ? X.cancel : stopped ? X.run_it_again : null,
      /* What pressing it does, for the running case, so "Stop this check" is
       * never read as "undo this check". */
      actionNote: e.state === 'running' ? X.cancel_running : null,
      /* The act the surface performs. `null` on a finished run, so a card
       * cannot offer a control the model did not choose. */
      act: e.state === 'running' ? 'cancel' : stopped ? 'rerun' : null,
      startedAt: e.startedAt,
      recordedAt: e.recordedAt,
      actor: e.actor,
    }
  })
}

/* ── repair history: what happened after it was raised (T6) ─────────────── */

/**
 * T6 — one sentence per accepted line about what has happened since it was
 * raised, of exactly three kinds, chosen from typed fields and nothing else:
 *
 *   (a) no run on the current version settles it  → "Still stands: nothing
 *       has run on {version} …" (or: what ran there settled nothing)
 *   (b) a run on the current version shows the opposite → "Still stands: the
 *       run on {version} shows {failed} failed, {passed} passed."
 *   (c) a run on the current version holds → the model's own full Proven
 *       line (`stateLine`), never "cleared" or "resolved"
 *
 * "On the current version" is the item's own `applies` field, read the way
 * `criterionState` reads it. A run speaks for "now" only when it bears and the
 * record does not call it stale (`fresh`) — the engine's own test for a run
 * that counts. A Change that holds NO run at all gets no rows — then, and
 * only then, "nothing has been checked a second time" is true, and the
 * section's own empty sentence says it. With no version named, a run cannot
 * be placed on "this" version, and the row says so rather than guessing.
 *
 * TYPED FIELDS DECIDE (b) — A3 g2 r1. The verifier's real-product walk: after
 * a passing Check-it-again run, a code change and a failing one, the server
 * serves the earlier pass `applies: false, fresh: false` beside the failing
 * run `applies: true`. The model leads that line with "Needs a re-check" (a
 * stale proof outranks a run, as the engine's readiness reason does), and
 * this function used to let that word pick (a) "what ran on {version} no
 * longer applies" — false about the failing run, which does apply. A run on
 * this version that shows the opposite is now (b) whatever word the line
 * leads with; "Still stands" and "Needs a re-check" do not contradict each
 * other. (c) still needs the model's own word to be Proven, so a Proven line
 * never stands beside a different word; "no longer applies" is said only when
 * a run on this version is the stale one.
 *
 * No step is served on coverage items, so no sentence names one; an absence
 * gets no receipt.
 *
 * @param {Array<object>} criteria the model's criteria (items via `itemOf`)
 * @param {string|null} shortVersion the version the screen names
 * @param {string[]} scopePaths the accepted scope, for the Proven line
 * @param {Array<object>} executions the coverage view's execution records
 * @param {Map<number, {who: string, name: string}>} [linkers] RUN-LINK-TRUTH —
 *   by criterion index, the lines the served readiness says count through a
 *   person's link (`linkedByLines`), so a Proven row on one names that person
 * @returns {Array<{n: number, text: string, kind: string, sentence: string,
 *   check: string|null, failed: number|null, passed: number|null}>}
 */
export function repairHistoryOf(criteria, shortVersion, scopePaths, executions, linkers) {
  const R = HQ_COPY.change.repairs
  const runsOf = (c) => list(c && c.items).filter((i) => i && i.kind === 'check_run')
  const anyRun = list(criteria).some((c) => runsOf(c).length) || list(executions).length > 0
  if (!anyRun) return []
  return list(criteria).map((c) => {
    const base = { n: c.n, text: c.text, check: null, failed: null, passed: null }
    if (!shortVersion) return { ...base, kind: 'unbound', sentence: R.unbound }
    const runs = runsOf(c)
    const onThis = runs.filter((i) => i.applies !== false)
    const settled = onThis.filter((i) => i.bears === true && i.fresh !== false)
    const opposite = settled.filter((i) => i.state === 'not_proven')
    const holds = settled.filter((i) => i.state === 'proven')
    if (opposite.length) {
      const run = latestLinked(opposite)
      const counted = Number.isInteger(run.testsFailed) && Number.isInteger(run.testsPassed)
      return {
        ...base,
        kind: 'b',
        check: str(run.name),
        failed: counted ? run.testsFailed : null,
        passed: counted ? run.testsPassed : null,
        sentence: counted
          ? fill(R.still_opposite, { version: shortVersion, failed: run.testsFailed, passed: run.testsPassed })
          : fill(R.still_opposite_no_count, { version: shortVersion }),
      }
    }
    if (c.state === 'proven' && holds.length) {
      const run = latestLinked(holds)
      /* RUN-LINK-TRUTH: when the engine says this line counts through a
       * person's link to THIS run, the person is named on the same line. */
      const linked = linkers instanceof Map ? linkers.get(c.n) : null
      const what = linked && (!linked.name || linked.name === str(run.name)) ? provenWhat('proven', linked, str(run.name)) : str(run.name)
      return { ...base, kind: 'c', check: str(run.name), sentence: stateLine('proven', shortVersion, scopePaths, what) }
    }
    /* What ran on this version no longer applies — said when a run on this
     * version is the stale one, or when the model holds the line stale and no
     * item says which went stale. */
    if (c.state === 'needs_recheck' && (onThis.some((i) => i.fresh === false) || !runs.some((i) => i.fresh === false))) {
      return { ...base, kind: 'a', sentence: fill(R.still_moved, { version: shortVersion }) }
    }
    return {
      ...base,
      kind: 'a',
      sentence: fill(onThis.length ? R.still_settled_nothing : R.still_none, { version: shortVersion }),
    }
  })
}

/**
 * The most recent of some items, by the one time an item carries — when it
 * was linked (`at`) — and by served order on a tie. Served order alone is not
 * a time: the coverage document lists a criterion's items by link id.
 */
function latestLinked(items) {
  return items.reduce((out, i) => (out === null || String(i.at || '') >= String(out.at || '') ? i : out), null)
}

/* ── readiness: the strongest thing the tool says (T-3-5) ──────────────── */

/**
 * The accepted line an obligation is about, by the engine's own identity —
 * its `criterion_index`, else the index in its recorded id `criterion:<n>` —
 * or null for one that is not a line (the candidate identity, a review).
 */
function lineOf(o) {
  if (typeof o.criterion_index === 'number') return o.criterion_index
  const recorded = /^criterion:([1-9][0-9]*)$/.exec(str(o.id) || '')
  return recorded ? Number(recorded[1]) : null
}

/**
 * Which of the two obligations that are not a line this is — `identity` (which
 * work will be looked at) or `review` — or null for an obligation that is a
 * line. By the engine's own id and kind (`change_readiness.rs`).
 */
function apartOf(o) {
  const at = (v) => (v === 'candidate_identity' ? 'identity' : v === 'review' ? 'review' : null)
  return at(str(o.id)) || at(str(o.kind))
}

/**
 * What the ENGINE computed about whether this version is ready for approval.
 *
 * NOTHING IS DECIDED HERE. `ready`, the obligations, their outcomes and the
 * list of what a ready answer rests on are all typed fields of
 * `change_readiness::compute`, served in the coverage document. This function
 * looks each slug up in the copy file and fills its placeholders, and that is
 * the whole of its job — a second readiness rule living on the client is
 * exactly the drift `change-model.js` exists to prevent.
 *
 * THE RULES IT CARRIES THROUGH:
 *
 *  - No readiness at all (an unreadable document) is NOT ready, and says the
 *    empty-set sentence rather than guessing.
 *  - `ready` is only ever true with a version named; the sentence carries it.
 *  - A waived or not-applicable line is listed with the person, the authority
 *    and the reason, and is ABSENT from `rests_on` — the engine builds that
 *    list from satisfied obligations only, and this file never adds to it.
 *  - There is no all-clear: the ready sentence is followed by the list, and
 *    `restsOn` being empty while `ready` is true is impossible by construction
 *    upstream and asserted downstream.
 *
 * @param {object|null} coverage the `coverage` object, carrying `readiness`
 * @param {string|null} shortVersion the version the rest of the screen names
 */
export function readinessOf(coverage, shortVersion, { checkHere = false } = {}) {
  const R = M.readiness
  /* NEVER-ASSESSED-ON-SCREEN — where the screen offers Check it again for this
   * Change (`checkHere`), a reason whose remedy the terminal words name as a
   * command says the on-screen act instead (`reasons_here`, `blocked_by_here`).
   * Without it, and for every other reason, the words are the mirrored ones. */
  const reasonSentence = (slug) => (checkHere && R.reasons_here[slug]) || R.reasons[slug] || null
  const doc = (coverage && coverage.readiness) || null
  const reviewUnavailable = reviewUnavailableOf(coverage)
  const obligations = list(doc && doc.obligations).map((o) => {
    const d = o && o.waiver ? o.waiver : null
    const values = {
      who: (d && str(d.actor)) || '',
      authority: (d && str(d.authority)) || '',
      why: (d && str(d.rationale)) || '',
    }
    const waiverLine = d && o.outcome === 'waived'
      ? fill(R.waived, values)
      : d && o.outcome === 'not_applicable'
        ? fill(R.not_applicable, values)
        : null
    return {
      id: str(o.id),
      n: typeof o.criterion_index === 'number' ? o.criterion_index : null,
      /* BRIEF-COUNT-AGREES-R2 — which accepted line this is about, by the
       * engine's own identity (`criterion_index`, else the index in its id
       * `criterion:<n>`), or null for an obligation that is not a line: the
       * candidate identity, a review. The count of what still needs evidence
       * counts lines, so this is what says which obligations are one. */
      line: lineOf(o),
      apart: apartOf(o),
      text: str(o.text),
      outcome: str(o.outcome),
      needsEvidence: o.outcome === 'needs_evidence',
      /* A waiver is NEVER evidence: this stays false for both kinds. */
      isEvidence: o.outcome === 'satisfied',
      reason: withReviewUnavailable(o, o.unsatisfied_slug ? reasonSentence(o.unsatisfied_slug) : null, reviewUnavailable),
      /* The engine's reason slug, carried so the Approve door is chosen from it and not from a second reading. */
      slug: str(o.unsatisfied_slug),
      waiver: waiverLine,
      evidenceName: str(o.evidence_name),
      linkedBy: str(o.linked_by),
      waivable: o.waivable !== false,
      notWaivable: o.waivable === false ? R.not_waivable : null,
    }
  })
  const missing = obligations.filter((o) => o.needsEvidence)
  const restsOn = list(doc && doc.rests_on).map((r) => ({
    id: str(r.obligation_id),
    text: str(r.text),
    evidenceName: str(r.evidence_name),
    linkedBy: str(r.linked_by),
    /* One line per thing it rests on: the accepted words, then the name of
     * what was looked at. Never a count, never a proportion. RUN-LINK-TRUTH:
     * when it counts through a person's link, that person is named on the
     * same line — the engine serves who, and this only looks the words up. */
    line: [str(r.text), str(r.evidence_name), str(r.linked_by) ? fill(R.linked_by, { who: str(r.linked_by) }) : '']
      .filter(Boolean).join(' — '),
  }))
  /* RUN-LINK-TRUTH — the lines the ENGINE names as having a passing run
   * nobody linked, in accepted order. Read by the run sheet and the top
   * sentence; decided nowhere but `change_readiness::why_not`. */
  const unlinked = missing
    .filter((o) => o.slug === UNLINKED_RUN_SLUG && typeof o.n === 'number')
    .map((o) => ({ n: o.n, text: o.text }))
  /* BRIEF-COUNT-AGREES-R2 — THE ONE COUNT. What still needs evidence is the
   * accepted lines the served answer says need it, and no other thing: a
   * set-aside line (waived, or does not apply) is never one, a line whose
   * applicability was never assessed always is, and the identity or a review
   * that is missing is not a line at all, so it is kept apart (`beyond`) and
   * said on its own, in words that never read as one of "the things that count
   * as done". An obligation naming a line the Change does not have is such a
   * thing too, and the same obligation served twice is one. Every surface that
   * states the count reads THIS — the top, the list under it, the page's own
   * list, Ask, the Inbox's items, the files — and none of them computes it
   * again. Derived from the obligations above and from nothing else. */
  const accepted = list(coverage && coverage.criteria).length
  const isLine = (o) => !o.apart && (o.line === null || !accepted || (o.line >= 1 && o.line <= accepted))
  const distinct = obligations.filter((o, i) => !o.id || obligations.findIndex((x) => x.id === o.id) === i)
  const said = (o) => ({ id: o.id, n: o.line, text: o.text, slug: o.slug, reason: o.reason, waiver: o.waiver })
  const beyondWords = { identity: HQ_COPY.change.conclusion.beyond_identity, review: HQ_COPY.change.conclusion.beyond_review, other: HQ_COPY.change.conclusion.beyond_other }
  /* BRIEF-COUNT-AGREES-R3 — WHAT WAS SET ASIDE is counted the way it is listed. The set-aside list every surface reads
   * (`waivedLines`) and the count the top and Ask state are one thing: an obligation a named person's waiver is on —
   * waived, or does not apply — once, whether it is one of the accepted lines or not. The lines are `setAside` (they are
   * "the things that count as done", and the top's "N of M" is a count of them); what is set aside and is not a line —
   * a review, which the accepted profile may ask for and a person may set aside — is `setAsideApart`, named apart. A
   * bare count of "set aside" had counted the lines only, over a list that also held the review: "1 was set aside" over
   * two entries. It also gave the review's entry no words, so nothing on it said what had been waived; the entry is
   * named here (`change.model.readiness.set_aside_name`), once, and every list reads the name from this list. An
   * outcome of waived with no person's waiver on it is not "set aside by a person" and is in neither. */
  const isAside = (o) => Boolean(o.waiver)
  const asideWords = { identity: HQ_COPY.change.conclusion.set_aside_identity, review: HQ_COPY.change.conclusion.set_aside_review, other: HQ_COPY.change.conclusion.set_aside_other }
  const asideApart = (o) => ({ ...said(o), text: o.text || R.set_aside_name[o.apart || 'other'], kind: o.apart || 'other', words: asideWords[o.apart || 'other'] })
  const waivedLines = distinct.filter(isAside).map((o) => (o.text || isLine(o) ? o : { ...o, text: R.set_aside_name[o.apart || 'other'] }))
  const tally = {
    total: distinct.filter(isLine).length,
    met: distinct.filter((o) => isLine(o) && o.isEvidence).map(said),
    needing: distinct.filter((o) => isLine(o) && o.needsEvidence).map(said),
    setAside: distinct.filter((o) => isLine(o) && isAside(o)).map(said),
    setAsideApart: distinct.filter((o) => !isLine(o) && isAside(o)).map(asideApart),
    beyond: distinct.filter((o) => !isLine(o) && o.needsEvidence).map((o) => ({ ...said(o), kind: o.apart || 'other', words: beyondWords[o.apart || 'other'] })),
  }
  const version = str(doc && doc.target_version) ? shortVersion || short(doc.target_version) : shortVersion
  const ready = Boolean(doc && doc.ready === true && version)
  /* NO DOCUMENT IS NOT AN ANSWER. A read that failed, or a caller with no
   * readiness block, gets `absent` and NO SENTENCE: the surfaces then say
   * nothing about readiness rather than printing "nothing counts as done yet"
   * over a Change whose accepted lines are sitting right there. Not ready
   * either way — `ready` is false and the Approve door stays shut. */
  if (!doc) {
    return {
      ready: false,
      absent: true,
      version,
      heading: R.heading,
      sentence: null,
      slug: null,
      restsOn: [],
      restsOnHeading: R.rests_on_heading,
      withExceptions: false,
      setAsideHeading: R.set_aside_heading,
      obligations: [],
      missing: [],
      missingCount: 0,
      tally: null,
      also: [],
      waivedLines: [],
      reviewUnavailable: null,
      unlinked: [],
      blocker: { reason: R.blocked_reason, enabler: R.blocked_enabler },
      /* RUN-LINK-TRUTH-R6 — no answer served, so nothing is "available". */
      available: null,
      blockers: [],
      offLines: [],
    }
  }
  /* T-5-3. The headline slug is the ENGINE'S on both branches now: it is the
   * engine that knows whether a ready answer rests in part on something
   * nobody checked, and `readiness_ready_with_exceptions` is its word for
   * that. Forcing `readiness_ready` here would have printed the ordinary
   * sentence over an answer with a waiver in it. */
  const slug = ready
    ? (str(doc.headline_slug) === 'readiness_ready_with_exceptions' ? 'readiness_ready_with_exceptions' : 'readiness_ready')
    : str(doc.headline_slug) || 'readiness_needs_evidence'
  const template = R.states[slug] || R.states.readiness_needs_evidence
  /* Why the Approve door is off, and the next act. The pair is chosen from
   * the readiness computation's own reason for the FIRST line still
   * missing, so the door and the list above it can never give two reasons
   * (RUN-LINK-TRUTH, PW-F14). */
  /* RUN-LINK-TRUTH-R6 — on a READY answer there is no blocker at all: a
   * served pair about a shut door, beside an answer that says the door is
   * open, is what round 5's adversary found the Approve stage printing (by
   * way of `approveBlocker`) under a waiver. What a ready answer says about
   * approval is `available`, chosen by the engine's own `with_exceptions`. */
  /* BRIEF-COUNT-AGREES-R2 — the sentence over the list of what still needs
   * evidence counts THE LINES that do (`tally.needing`), because that is what
   * the list under it holds. What is missing that is not a line is said after
   * it, on its own (`also`), in words of its own (`conclusion.beyond_*`, which
   * are deliberately not a line's reason: those say why approval is off, and the
   * one answer to that is the blocker); with no line waiting, it is the whole of
   * what is said. `missing` stays what the engine
   * names (the door and the blockers read it, identity first); `missingCount` is
   * the count the sentence states, of the lines. */
  const alsoWords = tally.beyond.map((b) => b.words)
  let sentence = fill(template, { version: version || '', n: missing.length })
  let also = []
  if (!ready && /^readiness_needs_evidence/.test(slug)) {
    if (tally.needing.length) {
      sentence = fill(R.states[tally.needing.length === 1 ? 'readiness_needs_evidence_one' : 'readiness_needs_evidence'], { version: version || '', n: tally.needing.length })
      also = alsoWords
    } else if (alsoWords.length) {
      sentence = alsoWords.join(' ')
    }
  }
  const blocker = ready ? null : blockerOf(missing[0] || null, coverage, slug, checkHere)
  const available = ready ? pairOf(doc.with_exceptions ? R.available_with_exceptions : R.available) : null
  const blockers = missing.map((m) => ({ id: m.id, n: m.n, text: m.text, ...pairFor(m, coverage, checkHere) }))
  return {
    ready,
    absent: false,
    version,
    heading: R.heading,
    /* The one sentence. On the ready branch it ENDS in the colon the list
     * follows, so the claim and what it rests on cannot be separated. */
    sentence,
    slug,
    restsOn,
    restsOnHeading: R.rests_on_heading,
    /* T-5-3 (`B7.22`): the engine's own field, carried, never re-derived. */
    withExceptions: Boolean(doc.with_exceptions),
    setAsideHeading: R.set_aside_heading,
    obligations,
    missing,
    missingCount: tally.needing.length,
    tally,
    also,
    waivedLines,
    /* T-4-1a-FIX: why no review findings can be had, when the accepted policy
     * expects a review. Read by the Review stage; never counted as a review. */
    reviewUnavailable,
    unlinked,
    /* Why the Approve door is off, and the next act (see `blocker` above).
     * Null when ready (RUN-LINK-TRUTH-R6). */
    blocker,
    /* RUN-LINK-TRUTH-R6 — when ready: that approval is available, and where.
     * Null when not ready. Every surface that explains approval on a ready
     * answer says this pair, or shows the live decision. */
    available,
    /* RUN-LINK-TRUTH-R5 — one pair per missing obligation, in the engine's
     * served order, each keyed by that obligation's own slug: the same pair
     * `blockerOf` gives when it is first. */
    blockers,
    /* RUN-LINK-TRUTH-R5 — "why is approval off", as the lines every surface
     * that answers it reads (Ask, the packet page, the three files): the one
     * blocker first, word for word, then each other missing obligation with
     * its own pair. Empty when ready. */
    offLines: ready ? [] : [pairLine(blocker), ...blockers.slice(1).map((b) => [b.text, pairLine(b)].filter(Boolean).join(' — '))],
  }
}

/** The engine's reason slug for a line whose passing run nobody linked. */
export const UNLINKED_RUN_SLUG = 'readiness_unlinked_run'

/**
 * RUN-LINK-TRUTH-R4 — THE one answer to "why is approval off, and what is the
 * next act?", served as `readiness.blocker` and read by every surface that
 * says it: Brief's off Approve, the Approve stage, the Inspector's off
 * primary and "What we checked" (through `actionBlocker`). It is the pair for
 * the FIRST missing obligation in the engine's served order
 * (`readiness.missing`, in `obligations` order: identity, then the accepted
 * lines, then a review), keyed by that obligation's own served slug. Nothing
 * here looks at which line "leads"; round 3's adversary found the Inspector
 * doing that and saying "link it" beside Brief's "run the checks".
 *
 * Every reason slug `change_readiness::Unsatisfied` can serve has its own pair
 * in `readiness.blocked_by`; the unlinked-run reason keeps `blocked_unlinked`,
 * which names the run. The engine answers the empty set before anything else
 * (`readiness_nothing_accepted`), so this does too. Not ready with nothing
 * missing and something accepted is a Change with no version written down.
 * The generic pair is left only for a slug this build does not know.
 *
 * The run is NAMED from the coverage document's own candidate — the same
 * typed fields `unlinkedCompletedRun` reads — only for words: which lines it
 * concerns is the engine's answer, never this function's.
 */
function blockerOf(first, coverage, headline, checkHere = false) {
  const R = M.readiness
  if (headline === 'readiness_nothing_accepted') return pairOf(R.blocked_by.readiness_nothing_accepted)
  if (!first) return pairOf(R.blocked_by.readiness_no_version)
  return pairFor(first, coverage, checkHere)
}

/** One missing obligation's own pair, keyed by its served slug. */
function pairFor(o, coverage, checkHere = false) {
  const R = M.readiness
  if (o.slug === UNLINKED_RUN_SLUG) {
    const run = unlinkedCompletedRun(coverage || null)
    const U = R.blocked_unlinked
    return {
      reason: run && run.name ? fill(U.reason, { name: run.name }) : U.reason_unnamed,
      enabler: U.enabler,
    }
  }
  const own = (checkHere && R.blocked_by_here[o.slug]) || R.blocked_by[o.slug]
  return own ? pairOf(own) : { reason: R.blocked_reason, enabler: R.blocked_enabler }
}

const pairOf = (p) => ({ reason: p.reason, enabler: p.enabler })
/** A pair as the one sentence a surface prints: the reason, then the act. */
const pairLine = (p) => p.reason + ' ' + p.enabler

/* ── the whole model ───────────────────────────────────────────────────── */

/**
 * T-4-1a-FIX (R-2026-09-28-5 (a)) — where the accepted policy expects a review
 * (a `review` obligation is in the readiness answer) and the server says no
 * review can be had on this machine, the plain sentence and the technical
 * detail under it. The sentence is the copy file's, chosen by the served
 * reason; an unrecognised reason still gets a sentence, never silence.
 * `null` otherwise. It changes no obligation's outcome.
 *
 * @param {object|null} coverage the coverage object, carrying `readiness` and
 *   `review_findings`
 * @returns {{sentence: string, technical: string}|null}
 */
export function reviewUnavailableOf(coverage) {
  const said = coverage && coverage.review_findings
  const doc = coverage && coverage.readiness
  if (!said || said.available !== false || !doc) return null
  if (!list(doc.obligations).some((o) => o && o.id === 'review')) return null
  const U = HQ_COPY.change.review.unavailable_now
  const why = str(said.why)
  return {
    sentence: why === 'tool_not_set_up' || why === 'model_not_answering' ? U[why] : U.unknown,
    technical: str(said.technical),
  }
}

/* The review obligation's reason, followed by why no review can be had. */
function withReviewUnavailable(o, reason, unavailable) {
  if (!unavailable || !o || o.id !== 'review' || !reason) return reason
  return `${reason} ${unavailable.sentence}`
}

/**
 * ONE derived view-model for one Change.
 *
 * @param {object} change the `change` object of `/api/changes/<id>`
 * @param {object|null} coverage the `coverage` object of the coverage document
 * @param {object|null} applicability the applicability document
 * @param {object|null} proposals the `proposals` object of the proposals document
 * @param {Array<object>} [work] the confirmed sessions, already adapted
 * @returns {object}
 */
export function deriveChangeModel(change, coverage, applicability, proposals, work) {
  const cov = buildCoverageView(coverage || null)
  const view = buildChangeView({
    change: change || {},
    proposals: proposals || null,
    work: list(work),
    evidence: evidenceEntries(coverage || null),
    unlinkedRun: unlinkedCompletedRun(coverage || null),
    unlinkedLines: servedUnlinkedLines(coverage || null).length,
  })

  const version = view.version
  const versionText = versionSentence(version)
  const shortVersion = version.named ? version.revision : null
  const scopePaths = list(view.covers && view.covers.paths)

  /* T-F-1-F1 with T-5-2: one short-form set over every version the
   * applicability document names, decided once, so the freshness lines of two
   * criteria cannot name two different trees with the same characters. */
  const freshNames = shortVersions(versionsNamedBy(applicability || null))
  /* STEP-7-LIMITS / STEP-7-GIT-LINE — what the record cannot say about the
   * code, derived ONCE, before the criteria: APPLIES-VS-MOVED needs to know,
   * line by line, whether the served Change says the code moved. */
  const codeLimit = codeLimitOf(shortVersion, change && change.code_moved)
  const movedFrame = codeLimit.moved ? { version: shortVersion } : null
  /* R-2026-09-28-25 — the engine's answer, read once, tied to lines by identity. */
  const unassessed = neverAssessedLines(coverage, cov.rows)
  /* RUN-LINK-TRUTH — the lines the engine says count through a person's
   * link, and who; so no "Proven:" line reads without that person named. */
  const linkers = linkedByLines(coverage)

  const criteria = cov.rows.map((row) => {
    const assessed = freshnessOf(row, applicability || null, freshNames, movedFrame)
    const state = criterionState(row, assessed)
    /* A3 g2 r1 — Proven with a stale assessment is exactly the case
     * `criterionState` calls answered: every stale item is about another
     * version and a run on this version holds. The line rests on that run now,
     * so its freshness is read over the items on this version, and the
     * Inspector never prints "needs a re-check" beside the Proven line of the
     * re-check that happened. Every other case keeps the whole assessment. */
    const freshness = state === 'proven' && assessed && assessed.fresh === false
      ? freshnessOf({ ...row, items: row.items.filter((i) => i.applies !== false) }, applicability || null, freshNames, movedFrame)
      : assessed
    const proven = state === 'proven' && shortVersion && scopePaths.length
      ? { version: shortVersion, scope: scopePaths }
      : null
    return {
      n: row.n,
      text: row.text,
      state,
      word: STATE_WORDS[state],
      proven,
      line: stateLine(state, shortVersion, scopePaths, provenWhat(state, linkers.get(row.n), row.items.length ? row.items[0].name : null)),
      sentence: row.sentence,
      /* RUN-LINK-INSPECTOR — the coverage row's own state, carried: this line
       * says "a test ran … link it". The Inspector reads it (with the engine's
       * answer, below) and derives nothing about runs of its own. */
      unlinked: row.state === 'unlinked_run',
      items: row.items,
      freshness,
      neverAssessed: unassessed.has(row.n),
      settle: settleSentence(state),
    }
  })

  /* The Change's leading state: the first criterion, in accepted order, whose
   * state is the one a person needs to act on. Worst first, and `proven` last,
   * so a Change with one line needing a re-check never reads as settled. */
  const ORDER = ['not_proven', 'needs_recheck', 'recorded', 'in_progress', 'proven']
  let leading = null
  for (const s of ORDER) {
    const hit = criteria.find((c) => c.state === s)
    if (hit) { leading = hit; break }
  }

  const counts = {
    total: criteria.length,
    proven: criteria.filter((c) => c.state === 'proven').length,
    notProven: criteria.filter((c) => c.state === 'not_proven').length,
    needsRecheck: criteria.filter((c) => c.state === 'needs_recheck').length,
    recorded: criteria.filter((c) => c.state === 'recorded').length,
    inProgress: criteria.filter((c) => c.state === 'in_progress').length,
    withEvidence: view.conclusion.withEvidence,
    withoutEvidence: view.conclusion.withoutEvidence,
    problems: view.conclusion.problems,
  }

  const decision = decisionOf(change, shortVersion)

  /* T-3-5 — the readiness answer, derived ONCE here like every other field,
   * so the Change screen's headline, the Approve door, the Inbox row and the
   * packet cannot disagree about whether this version is ready.
   * NEVER-ASSESSED-ON-SCREEN — with `checkHere` where this screen offers Check
   * it again for the Change (it has an id, and the run door is drawn for an
   * id), and it is derived BEFORE the headline: the top sentence reads it. */
  const readinessAnswer = readinessOf(coverage, shortVersion, { checkHere: Boolean(view.id) })
  /* A standing approval already answers the question for its exact version.
   * Keep the engine's readiness and its evidence, but do not offer approval
   * readiness beside that decision. Compare the served FULL versions: an
   * approval on an older version must not hide this one's ready answer. */
  const alreadyApproved = readinessAnswer.ready && decision.approvedFull
    && decision.approvedFull === str(coverage && coverage.readiness && coverage.readiness.target_version)
  /* READY-BESIDE-APPROVED repair: `hiddenBecause` says WHY the sentence is
   * absent, so a surface that answers about readiness (Ask, the JSON file)
   * can tell an approved version from an answer that could not be read. */
  const readiness = alreadyApproved
    ? { ...readinessAnswer, sentence: null, available: null, hiddenBecause: 'approved' }
    : readinessAnswer
  /* The top sentence never says "0 still need it" over a readiness answer that
   * is not ready: `readinessConclusion` returns the conclusion untouched in
   * every other case, and leaves the coverage counts above exactly as they are.
   * Every renderer of the top sentence reads THIS object (`hq.conclusion`). */
  const conclusion = readinessConclusion(view.conclusion, readiness)

  const headline = {
    key: conclusion.key,
    values: conclusion.values,
    text: fill(HQ_COPY.change.conclusion[conclusion.key], conclusion.values),
    /* The state word the top of the screen is about, which is the leading
     * criterion's and nothing else's. The headline SENTENCE stays free of
     * state words (`change-view.js`'s standing rule, swept by hq-copy.test.js);
     * this field is what the Inbox row and the packet agree with. */
    state: leading ? leading.state : null,
    word: leading ? leading.word : null,
    version: shortVersion,
  }

  /* RUN-LINK-INSPECTOR — the leading line has a passing run nobody linked, and
   * that is what the engine's answer says is missing FIRST. Both must say it:
   * the line's own row (the sentence a person reads under it) and
   * `readiness.missing[0]` (the one answer to "why is approval off", which
   * RUN-LINK-TRUTH-R4 made every surface read). With an unconfirmed identity
   * ahead of it, or with the engine and the row parting, the Inspector is the
   * Inspector it was — never "link it" beside a blocker that says confirm. */
  const firstMissing = readiness.missing[0] || null
  const unlinkedLead = leading && leading.unlinked && firstMissing
    && firstMissing.slug === UNLINKED_RUN_SLUG && firstMissing.n === leading.n
    ? leading
    : null

  /* NEVER-ASSESSED-ON-SCREEN — the leading line has evidence and the engine's
   * answer says its applicability was never assessed, first among what is
   * missing: "Nothing — the record already backs this, on the version named
   * above" would say it holds on a version nobody assessed it for. The line's
   * own state word is not touched; only the sentence about what would settle
   * it, and only where the screen offers the act. */
  const neverAssessedLead = leading && leading.neverAssessed && firstMissing && Boolean(view.id)
    && firstMissing.slug === 'readiness_never_assessed' && firstMissing.n === leading.n
    ? leading
    : null

  const inspector = {
    what: { heading: M.inspector.what, quote: leading ? leading.text : null, summary: headline.text },
    shows: {
      heading: M.inspector.shows,
      state: headline.state,
      word: headline.word,
      line: leading ? leading.line : null,
      items: leading ? leading.items : [],
      empty: M.inspector.shows_empty,
      /* RUN-LINK-INSPECTOR — the line's own unlinked-run sentence, the one the
       * criterion row prints, in place of "Nothing in the record bears on this
       * line yet." Null everywhere else. */
      unlinked: unlinkedLead ? { n: unlinkedLead.n, sentence: unlinkedLead.sentence } : null,
      freshness: leading ? leading.freshness : null,
      /* A3-F12 — how many finding cards, and how many of the leading line's
       * pieces of evidence, the Inspector draws in full before one disclosure
       * holds the rest. Decided here, once, so every surface folds alike. */
      foldAfter: INSPECTOR_FOLD_AFTER,
    },
    settle: {
      heading: M.inspector.settle,
      sentence: unlinkedLead
        ? M.inspector.settle_unlinked_run
        : neverAssessedLead ? M.inspector.settle_never_assessed : leading ? leading.settle : settleSentence('in_progress'),
    },
    /* A3-F4: one named derivation, so every state's primary can be held to
     * "on and does what it says, or off with both halves" by one test. */
    action: inspectorAction(headline.state, leading ? leading.n : null, view.blocker, readiness, unlinkedLead),
    technical: M.inspector.technical,
  }

  /* T6 — what happened after each line was raised, derived ONCE here, so the
   * Change screen, the packet page and the exported file say the same kind
   * for the same fields. */
  const repairs = repairHistoryOf(criteria, shortVersion, scopePaths, cov.executions, linkers)

  /* T-3-4 — every run, with its execution state beside its observed result.
   * `stopped` is the subset a person has to act on: the cards that carry one
   * primary "Run it again" and no state word at all. */
  const executions = executionsOf(cov)
  const stoppedRuns = executions.filter((r) => r.stopped)
  /* The runs a person can act on at all: the ones still going (stop it) and
   * the ones that were cut short (run it again). A finished run needs no card
   * above the Inspector, because it is already answered by its criterion. */
  const openRuns = executions.filter((r) => r.running)
  const actionableRuns = executions.filter((r) => r.action)

  /* UNRECORDED-WORK-NOT-SILENT — which linked pieces of work ThruLine holds
   * no steps for, derived ONCE here, so the top of the Change, the Verify
   * view, the packet page and the exported file say one thing from one field. */
  const unrecorded = unrecordedWorkOf(work)

  const model = {
    id: view.id,
    title: str(change && change.request && change.request.title),
    decision,
    unrecorded,
    readiness,
    /* AI-REVIEW-OUT-OF-LAUNCH: whether this build offers AI review at all,
     * as the server says it. Only an explicit `true` lets the Review stage
     * offer one; the launch build serves `false`. */
    aiReview: Boolean(coverage && coverage.ai_review === true),
    headline,
    /* NEVER-ASSESSED-ON-SCREEN — the top sentence's key and values, once. The
     * Change screen's Explore view and the whole-story page render the
     * sentence from this and not from their own reading of the counts. */
    conclusion,
    state: headline.state,
    word: headline.word,
    version: { ...version, text: versionText, short: shortVersion },
    counts,
    criteria,
    changedSince: changedSince(criteria),
    /* STEP-7-LIMITS — the record's limit, derived once for every surface;
     * STEP-7-GIT-LINE — with the served move first in it, and the move alone
     * as `codeMoved` for the surfaces that print its receipt. */
    codeLimit,
    codeMoved: codeLimit.moved,
    /* T-3-4: the execution dimension, derived ONCE here like every other
     * field, so the Change screen, the Inbox row and the packet cannot
     * disagree about whether a run finished. */
    executions,
    stoppedRuns,
    openRuns,
    actionableRuns,
    executionHeading: M.execution.heading,
    executionNone: M.execution.none,
    repairs,
    inspector,
    inboxRow: {
      title: str(change && change.request && change.request.title),
      word: headline.word,
      line: leading ? leading.line : null,
      version: versionText,
      reason: headline.text,
      /* T-3-4 — how many runs on this Change were cut short. The Inbox's
       * `interrupted capture` reason (T-2-6) fires from THIS, which is the
       * record, rather than from a list the loader hard-coded empty. */
      stoppedRuns: stoppedRuns.length,
      stoppedRunLine: stoppedRuns.length ? stoppedRuns[0].executionLine : null,
      /* T-S-4: the Inbox row says the same thing the Change screen says about
       * whether this has been approved, because it reads the same field. */
      decision: decision.headline,
      /* T-3-5: and the same thing about whether it is ready. The Inbox's
       * `awaiting approval` reason fires from THIS, which is the record. */
      ready: readiness.ready,
      readinessLine: readiness.sentence,
    },
    /* The taken-away record, as TYPED FIELDS (T-S-5). `packetMarkdown` below
     * builds the Markdown from exactly these and from `HQ_COPY` headings, and
     * from nothing else — no agent prose reaches it, and no number on it is
     * written anywhere but here. The §3.6 sections, in order: headline with
     * scope and version · each statement with its proof · what the agent said
     * against what the record shows · every run with its version · findings
     * with their resolution · decision history · what was not recorded. */
    packet: packetOf({ view, change, headline, versionText, shortVersion, scopePaths, decision, criteria, cov, readiness, repairs, codeLimit, unrecorded }),
    view,
    coverage: cov,
  }
  /* A3-F2 — how many things on this Change need you, from the ONE attention
   * derivation, over the same input the Inbox hands it: the Change, its
   * proposals, its confirmed work and this model. The Inbox row and the
   * Change's own model cannot state two numbers for one record. */
  model.attention = attentionOf({ change: change || {}, proposals: proposals || null, work: list(work), hq: model })
  return model
}

/**
 * The taken-away record: ONE array of §3.6 sections, rendered by the packet
 * page and walked by `packetMarkdown` (T-S-5 verifier finding 1).
 *
 * Before this, the page rendered `packet-view.js`'s twelve sections and the
 * Markdown built its own seven, so "every run with its version" and "what was
 * not recorded" existed only in the copied text and a person reading the page
 * top to bottom never met them. The two cannot diverge again: there is one
 * `sections` array, `packet.jsx` maps over it, and the builder below walks the
 * same rows in the same order.
 */
export function packetOf({ view, change, headline, versionText, shortVersion, scopePaths, decision, criteria, cov, readiness, repairs, codeLimit, unrecorded = null }) {
  const MD = HQ_COPY.change.packet.markdown
  const packet = {
    id: view.id,
    title: str(change && change.request && change.request.title),
    headline: headline.text,
    version: versionText,
    versionShort: shortVersion,
    scope: scopePaths,
    /* The scope, as the one sentence both surfaces print. */
    coversLine: list(scopePaths).length ? fill(MD.covers, { scope: list(scopePaths).join(', ') }) : MD.covers_none,
    /* STEP-7-GIT-LINE — the same lines the Change screen prints about the
     * code (the served move first, then the limit), and the move's receipt as
     * typed fields for the file's technical part. */
    codeLines: codeLimit ? list(codeLimit.lines) : [],
    codeMoved: codeLimit && codeLimit.moved ? codeLimit.moved : null,
    /* How many lines have been accepted at all (P6-DESIGN-FIXES-13; design
     * record §8 item 3). Nothing accepted and "no evidence yet" are two
     * different facts, and the packet used to state both at once. */
    acceptedCount: list(view.accepted && view.accepted.criteria).length,
    /* QUAL-F2-R3 (`Q7`) — whether the list of what counts as done was accepted
     * after the work it covers, as TYPED FIELDS and the one sentence made from
     * them, or `null` when the record does not show it. The fact is read ONCE,
     * by `change-view.js` (`retrospectiveOf`, from the engine's
     * `change.retrospective`), and reaches here as `view.retrospective`; this
     * composes the sentence ONCE from the copy file, and `sectionsOf` below
     * turns it into the one section every surface walks — the page's accepted
     * section, Markdown, HTML, JSON, Copy as Markdown and the redacted copy. No
     * surface derives it, or words it, again. It is a caution about order, not
     * evidence: nothing here says the work meets the list. And, like the
     * Change screen and the page, it is about a list that has something in it:
     * with no criterion accepted there is no "this list" to say it of. */
    acceptedAfterWork: view.retrospective && view.retrospective.acceptedOn && list(view.accepted && view.accepted.criteria).length
      ? {
        acceptedOn: view.retrospective.acceptedOn,
        scopeVersion: view.retrospective.scopeVersion,
        sentence: fill(HQ_COPY.change.packet.accepted.after_work, { date: view.retrospective.acceptedOn }),
      }
      : null,
    decision: decision.headline,
    /* T-3-5 — the packet says what the screen says about readiness, from the
     * same field, and lists what a ready answer rests on. It sits in the
     * HEADER, beside the headline and the version, rather than as a seventh
     * §3.6 section: the six sections are the record, and this is the answer
     * about the record. A packet that said "ready" without the list would be
     * the all-clear lens rule 5 forbids, so the two are one block.
     *
     * `readinessRests` is what it rests on when ready, and what still needs
     * evidence when it is not — one list, always present, never a tick. */
    readiness: readiness || null,
    readinessLine: readiness && !readiness.absent ? readiness.sentence : null,
    /* RUN-LINK-TRUTH-R5 — why this version can't be approved, when it can't:
     * the one blocker every surface on the Change screen says, word for word,
     * printed BEFORE the readiness sentence and its list so the taken-away
     * record leads with the same answer. The list below keeps each line's own
     * reason. Null when ready or when no readiness was served. */
    readinessBlocker: readiness && !readiness.absent && !readiness.ready ? list(readiness.offLines)[0] || null : null,
    readinessBlockerPair: readiness && !readiness.absent && !readiness.ready
      ? { reason: readiness.blocker.reason, enabler: readiness.blocker.enabler }
      : null,
    readinessRests: readiness && readiness.sentence
      ? (readiness.ready
        /* P6B-HTT-4 — the rests-on line of a criterion whose applicability
         * was never assessed says so on that line, in the words the "What was
         * not recorded" section uses for it. The selection is the engine's
         * and is untouched: nothing is added, dropped or reordered. */
        ? list(readiness.restsOn).map((r) => {
          /* P6B-HTT-4-R2 — tied by identity; a line whose identity is not
           * served and whose words more than one accepted line carries is
           * never guessed at: it says it cannot be told which it is. */
          const c = criterionRestedOn(r, readiness, criteria)
          if (c === UNIDENTIFIED) {
            return list(criteria).some((x) => x.text === r.text && neverAssessed(x))
              ? r.line + ' — ' + MD.rests_unidentified
              : r.line
          }
          return neverAssessed(c) ? r.line + ' — ' + MD.rests_never_assessed : r.line
        })
        /* BRIEF-COUNT-AGREES-R2 — not ready: the lines that still need
         * evidence, and only those, so the list under "N things still need
         * evidence:" has N entries. What is missing that is not a line
         * (`readinessAlso`) and the lines set aside (`readinessSetAside`) are
         * said after it, each on its own. */
        : list(readiness.tally && readiness.tally.needing).map((m) => [m.text, m.reason].filter(Boolean).join(' — ')))
        .concat(readiness.ready ? list(readiness.waivedLines).map((d) => [d.text, d.waiver].filter(Boolean).join(' — ')) : [])
      : [],
    readinessAlso: readiness && !readiness.absent && !readiness.ready ? list(readiness.also) : [],
    readinessSetAsideHeading: readiness && !readiness.absent && !readiness.ready && list(readiness.waivedLines).length ? readiness.setAsideHeading : null,
    readinessSetAside: readiness && !readiness.absent && !readiness.ready
      ? list(readiness.waivedLines).map((d) => [d.text, d.waiver].filter(Boolean).join(' — '))
      : [],
    decisionBy: decision.by || null,
    /* T7 — the same sentence the Decision and the Brief headline print. */
    decisionListMoved: decision.listMovedLine || null,
    /* T-5-3 (`B10.9`) — the packet carries what the standing approval was
     * given under, in the same words the screen shows, from the same rows. */
    decisionExceptions: list(decision.exceptions).map((e) => e.line),
    /* READY-BESIDE-APPROVED repair (verifier finding 2): the same rows as
     * typed fields, for the machine-readable file. */
    decisionExceptionRows: list(decision.exceptions).map((e) => ({
      line: e.line, obligationId: e.obligationId, kind: e.kind, actor: e.actor, authority: e.authority, rationale: e.rationale,
    })),
    decisionExceptionsHeading: decision.exceptionsHeading || null,
    decisionNone: decision.none,
    decisionHistory: decision.history,
    footer: MD.footer,
    criteria: criteria.map((c) => ({
        n: c.n,
        text: c.text,
        word: c.word,
        line: c.line,
        freshness: c.freshness,
        repair: (list(repairs).find((r) => r.n === c.n) || {}).sentence || null,
        /* What was looked at for this line, by the evidence's OWN name as the
         * record holds it, with the state word this file derived. A line with
         * nothing behind it carries an empty list and says so in words. */
        proof: list(c.items).map((i) => ({
          name: str(i.name),
          word: STATE_WORDS[i.state] || null,
          applies: str(i.appliesText),
          /* T-3-6-F3: the packet carries what the Change screen carries. The
           * words the check printed when it broke, and where the finding came
           * from, are on both or on neither — a taken-away record that omits
           * the seeded label would be `B16.10` failing by export. */
          failure: i.failure,
        })),
      })),
      /* UNRECORDED-WORK-NOT-SILENT-R3 — every captured claim, once: the
       * findings as before, then the claims that raised no card. */
      said: saidAgainstRecord(view.findings, unflaggedOf(view.work)),
      /* UNRECORDED-WORK-NOT-SILENT — the one row the said section adds for
       * linked work ThruLine holds no steps for; null when there is none. */
      saidUnrecorded: unrecordedSaidRow(unrecorded),
      runs: runsOf(criteria, cov, shortVersion),
      /* T6 — the same rows the Change screen shows, from the same derivation. */
      repairs: list(repairs),
      findings: list(view.findings).map((f) => ({
        record: str(f.record && f.record.text),
        /* The record does not write down how a finding was answered. Saying so
         * is the whole point of the section (Rule 12); inventing one would be
         * the packet claiming a resolution nobody recorded. */
        resolution: str(f.resolution),
      })).filter((f) => f.record),
    notRecorded: notRecordedOf(criteria, scopePaths, shortVersion),
  }
  packet.sections = sectionsOf(packet)
  return packet
}

/**
 * The section that carries the "accepted after the work was done" caution
 * (QUAL-F2-R3). It is a member of `hq.packet.sections` — the array the page
 * renders and every exported file walks — and only when the record shows the
 * order; an accepted-first Change has six sections and no trace of it.
 */
export const ACCEPTED_AFTER_WORK_SECTION = 'accepted_after_work'

/**
 * The body sections of §3.6, in its order, each as typed rows.
 *
 * A row is `{text, lines}` — one line a person reads, and what sits under it.
 * A section with no rows carries `none`, the sentence that says what the
 * record's silence means; it is never a blank and never a tick.
 *
 * ONE section is conditional, and comes first (QUAL-F2-R3): when the record
 * shows the list of what counts as done was accepted after the work, the
 * caution says so before the proof that is measured against that list. It has
 * exactly one row, so it never needs a `none`, and the page's accepted section
 * is its renderer (`packet.jsx`), not a second author of the sentence.
 */
export function sectionsOf(p) {
  const MD = HQ_COPY.change.packet.markdown
  const row = (text, lines) => ({ text, lines: list(lines).filter(Boolean) })
  const after = p && p.acceptedAfterWork && str(p.acceptedAfterWork.sentence) ? p.acceptedAfterWork : null
  return [
    ...(after
      ? [{
        id: ACCEPTED_AFTER_WORK_SECTION,
        title: MD.accepted_after_work_heading,
        none: null,
        rows: [row(after.sentence, [])],
      }]
      : []),
    {
      id: 'proof',
      title: MD.proof_heading,
      /* ONE of the two, never both (P6-DESIGN-FIXES-13; design §8 item 3).
       * With nothing accepted there is nothing for evidence to be missing
       * FROM, and "No test ran for this." read as a verdict on statements the
       * record does not hold. The sentence below is the one the packet's own
       * "What counts as done" section prints in the same situation, reused
       * rather than re-worded so the page says one thing twice and not two
       * things once. */
      none: p.acceptedCount ? MD.proof_none : HQ_COPY.change.packet.sections.accepted.empty,
      rows: list(p.criteria).map((c) => row(c.text + ' — ' + c.line, [
        c.freshness && c.freshness.sentence,
        /* T6 — what happened after this line was raised, the same sentence
         * the Change screen and the page's repair section show. It sits under
         * its statement rather than in a seventh section (§3.6 has six), and
         * a Proven line that is already the row's own line is not said twice. */
        c.repair && c.repair !== c.line ? c.repair : null,
        ...(list(c.proof).length
          ? list(c.proof).flatMap((e) => [
            [e.name, e.word, e.applies].filter(Boolean).join(' — '),
            /* Only on a line the record holds a problem for: a passing run
             * has nothing that broke, and printing "the record holds no line
             * from the failure" under it would invent a failure to be silent
             * about. */
            e.word === STATE_WORDS.not_proven
              ? e.failure.heading + ': ' + (e.failure.line || e.failure.none)
              : null,
            e.word === STATE_WORDS.not_proven
              ? e.failure.classLabel + ': ' + e.failure.classText
              : null,
          ].filter(Boolean))
          : [MD.proof_none]),
      ])),
    },
    {
      id: 'said',
      title: MD.said_heading,
      /* Reached only when every linked piece of work has recorded steps and
       * none holds a claim: unrecorded work is a ROW below, never silence.
       * The JSON export carries `none` beside the rows, so with unrecorded
       * work it is that row's sentence, and `said_none` appears nowhere. */
      /* R3: and never while a row holds what the agent said — a captured
       * claim is a row, so said_none is not beside it, in any format. */
      none: p.saidUnrecorded ? p.saidUnrecorded.text : list(p.said).length ? null : MD.said_none,
      rows: [
        ...list(p.said).map((s) => (s.said
          ? row(withWord(fill(MD.said, { said: s.said }), s.word), [s.record ? fill(MD.shows, { record: s.record }) : null])
          : row(withWord(fill(MD.shows, { record: s.record }), s.word), []))),
        ...(p.saidUnrecorded ? [{ ...row(p.saidUnrecorded.text, p.saidUnrecorded.lines), receipts: list(p.saidUnrecorded.receipts) }] : []),
      ],
    },
    {
      id: 'runs',
      title: MD.runs_heading,
      none: MD.runs_none,
      rows: list(p.runs).map((r) => ({
        ...row(
          [r.name, r.word].filter(Boolean).join(' — ')
          + ' — ' + (r.version ? fill(MD.run_version, { version: r.version }) : r.where || MD.run_version_none),
          r.lines,
        ),
        /* Whether this run counts for this Change: the page's examination
         * section lists the linked rows and the rest apart, verbatim. */
        linked: r.linked !== false,
      })),
    },
    {
      id: 'findings',
      title: MD.findings_heading,
      none: MD.findings_none,
      rows: list(p.findings).map((f) => row(f.record, [f.resolution || MD.resolution_none])),
    },
    {
      id: 'decisions',
      title: MD.decisions_heading,
      /* The same sentence the page's own decision section prints when the
       * record holds no act, so the page and the copied text say one thing. */
      none: HQ_COPY.change.packet.sections.decisions.empty,
      rows: list(p.decisionHistory).map((h) => row(h.line, [])),
    },
    {
      id: 'not_recorded',
      title: MD.not_recorded_heading,
      none: MD.not_recorded_none,
      rows: list(p.notRecorded).map((t) => row(t, [])),
    },
  ]
}

/* ── the taken-away record (T-S-5) ─────────────────────────────────────── */

/**
 * What the agent said, beside what the record shows, one pair per finding.
 *
 * Both halves are the narrated card's own text — the words `verdict-model.js`
 * composed from the record and its copy families. A finding whose `said` half
 * is absent is still shown, because "the record shows X and nobody claimed
 * otherwise" is a fact, and dropping it would be the packet choosing what a
 * reader meets.
 */
export function saidAgainstRecord(findings, unflagged = []) {
  const wordOf = (state) => (state && Object.prototype.hasOwnProperty.call(HQ_COPY.states, state) ? HQ_COPY.states[state].word : null)
  const pair = (f) => ({ said: str(f && f.said && f.said.text), record: str(f && f.record && f.record.text), word: wordOf(f && f.state) })
  /* A claim a card already quotes (its own card, or a tests-weakened card
   * about it) is said once, by that card, in the same conversation. */
  const quoted = new Set(list(findings)
    .filter((f) => f && f.said && f.said.text)
    .flatMap((f) => [f.proofNode, ...list(f.refs)].filter(Boolean).map((r) => String(f.sessionId) + ' ' + r)))
  return [
    ...list(findings).map(pair),
    ...list(unflagged).filter((u) => u && !quoted.has(String(u.sessionId) + ' ' + u.proofNode)).map(pair),
  ].filter((p) => p.said || p.record)
}

/**
 * UNRECORDED-WORK-NOT-SILENT-R3 — the captured claims of the confirmed work
 * that raised no card (`buildVerdict`'s `unflagged`), each tagged with its
 * conversation, in work order. The verdict model computed their states; this
 * only gathers them.
 */
export function unflaggedOf(work) {
  return list(work).flatMap((w) => list(w && w.verdict && w.verdict.unflagged).map((u) => ({ ...u, sessionId: String(w.id) })))
}

/** A said-section line with the state word the verdict model computed for it, when there is one. */
function withWord(text, word) {
  return word ? text + ' — ' + word : text
}

/**
 * UNRECORDED-WORK-NOT-SILENT-R2 — whether the served record says a claim WOULD
 * have been captured for one linked conversation, read from the two served
 * documents the loader already holds. Pure; the loader calls it once per
 * conversation and puts the answer on the work entry.
 *
 * A claim is extracted only from a recorded `stop` event's final message
 * (crates/cli/src/claims_cmd.rs:209; explain_cmd.rs:123-147), so a
 * conversation with no recorded end has `claims: []` whatever the agent said.
 * Two served fields say whether the end was recorded, and both must:
 *  - explain's `why_it_stopped.kind` (explain_cmd.rs:244), which is
 *    `no_stop_recorded` when the record holds no `stop` event at all
 *    (crates/explain/src/stop.rs:70-78) and a named reason when it does;
 *  - claims' `capture_degraded` (claims_cmd.rs:418), which the claims view
 *    sets when no `stop`/`session_end` is recorded at or after the last step
 *    (`lifecycle_gap`, claims_cmd.rs:533-557, applied at :648) — a stop the
 *    work carried on past — and for every other reason capture is incomplete.
 * `ended` is true only when the first names a recorded stop AND the second is
 * a literal `false`; false when either positively says the end is missing;
 * null when either could not be read. `claims` is the served claim count, or
 * null when the claims document was not read.
 *
 * @param {object|null} explainDoc  `/api/explain`, or null
 * @param {object|null} claimsDoc   `/api/claims`, or null when refused
 * @returns {{ended: ?boolean, claims: ?number}}
 */
export function claimCaptureOf(explainDoc, claimsDoc) {
  const claims = claimsDoc && Array.isArray(claimsDoc.claims) ? claimsDoc.claims.length : null
  const why = explainDoc && explainDoc.why_it_stopped && typeof explainDoc.why_it_stopped === 'object' ? explainDoc.why_it_stopped : null
  const kind = why && typeof why.kind === 'string' ? why.kind : null
  const degraded = claimsDoc && typeof claimsDoc.capture_degraded === 'boolean' ? claimsDoc.capture_degraded : null
  let ended = null
  if (kind === 'no_stop_recorded' || kind === 'not_observable' || degraded === true) ended = false
  else if (kind !== null && claims !== null && degraded === false) ended = true
  return { ended, claims }
}

/**
 * UNRECORDED-WORK-NOT-SILENT (R2) — the linked pieces of work for which the
 * record does not positively hold what the agent said, and the three
 * sentences that say so.
 *
 * `work` is the loader's list (`readChangeDocuments`), which already leaves
 * out executions: a run `thruline check` wrote is not a conversation. Each
 * entry carries `steps` (the reading's own count of events, or null), and
 * since R2 `ended` and `claims` from `claimCaptureOf` above. A piece of work
 * is RECORDED when either
 *  - the claims record holds at least one claim for it (the claim shows), or
 *  - it has steps, its end was recorded (`ended === true`, so a claim would
 *    have been captured), and it holds none.
 * Every other shape counts here: 0 steps; steps with no recorded end or stop;
 * claims unknown. Round 1 decided from `steps` alone, and a conversation with
 * events but no recorded `stop` — the agent's words in it, `claims: []` — was
 * served "the record holds nothing the agent said".
 *
 * An entry with no `steps` field at all was not built by the loader (a
 * fixture that predates the field) and says nothing either way, so it is not
 * counted. An entry with `steps` and no `ended` is claims-unknown and counts.
 *
 * Its sentences say only that what the agent said was not recorded, never
 * why. Beside them, `held` and `logs` say where the agent's own log holds it
 * (UNRECORDED-WORK-NO-QUOTE, below). Nothing here reaches a state word, a
 * count, readiness or approval: it only replaces sentences that claimed more.
 *
 * @param {Array<{id: string, steps?: ?number, ended?: ?boolean, claims?: ?number, log?: ?{file: string, line: number}}>} work
 * @returns {{count: number, ids: string[], top: ?string, verify: ?string, said: ?string, held: ?string, logs: object[]}}
 */
export function unrecordedWorkOf(work) {
  const recorded = (w) => (typeof w.claims === 'number' && w.claims > 0)
    || (typeof w.steps === 'number' && w.steps > 0 && w.ended === true && w.claims === 0)
  const ids = list(work)
    .filter((w) => w && Object.prototype.hasOwnProperty.call(w, 'steps') && !recorded(w))
    .map((w) => String(w.id))
  const n = ids.length
  const pick = (one, many) => (n === 0 ? null : n === 1 ? one : fill(many, { n }))
  const MD = HQ_COPY.change.packet.markdown
  const T = HQ_COPY.change.unrecorded
  /* UNRECORDED-WORK-NO-QUOTE — for the work counted above and ONLY that
   * work, where the agent's own log holds what it said, when the server
   * placed it (`GET /api/changes/<id>` → `agent_log`, derived in
   * `crates/core/src/agent_log.rs` only from lines whose own `message.role`
   * names the agent). A file and a line: the words are never served, so none
   * can be drawn. Work whose words the record holds never reaches this list.
   * Nothing here is a finding or reaches a count, a state word, readiness or
   * approval. `held` is the one sentence for the whole list: ThruLine wasn't
   * recording, the log holds what was said, and this release shows no words. */
  const counted = new Set(ids)
  const logs = list(work)
    .filter((w) => w && counted.has(String(w.id)) && placeOf(w.log))
    .map((w) => ({
      id: String(w.id),
      file: w.log.file,
      line: w.log.line,
      receipt: fill(T.receipt, { path: w.log.file, line: w.log.line }),
    }))
  return {
    count: n,
    ids,
    top: pick(T.top_one, T.top_many),
    verify: pick(HQ_COPY.change.verify.unrecorded_one, HQ_COPY.change.verify.unrecorded_many),
    said: pick(MD.said_unrecorded_one, MD.said_unrecorded_many),
    held: logs.length === 0 ? null : logs.length === 1 ? T.log_one : fill(T.log_many, { n: logs.length }),
    logs,
  }
}

/**
 * The Verify view's and the page's one sentence about the linked work.
 *
 * UNRECORDED-WORK-NOT-SILENT — work whose words the record does not positively
 * hold has not been read against anything, so "nothing … contradicts" is not
 * said of it: the unrecorded sentence takes its place. Otherwise it is
 * `change-view.js` `nothingWrongLine`, which counts the pieces of work actually
 * read (PW-F13) and carries its scope (T-2-8-F5). The model's own field when
 * there is one; the same derivation otherwise. Shared by the Change screen and
 * the whole-story page so one Change reads the same way on both.
 */
export function linkedWorkLine(model) {
  const unrecorded = model && model.hq ? model.hq.unrecorded : unrecordedWorkOf(model && model.work)
  if (unrecorded && unrecorded.count) return unrecorded.verify
  return nothingWrongLine(model)
}

/** A served place with the two fields its receipt needs, or null. */
function placeOf(place) {
  return place && typeof place.file === 'string' && place.file && Number.isInteger(place.line) ? place : null
}

/**
 * The said section's row for unrecorded work: `{text, lines, receipts, files}`,
 * or null.
 *
 * `lines` holds the one sentence that says ThruLine wasn't recording, the
 * agent's own log holds what it said, and this release does not show its words
 * (`held`), and `receipts` the log file and line of each such log, one level
 * down — the page folds them away and the exported file puts them in its
 * technical part. Both are empty when no log could be placed: the sentence
 * then stands alone. `files` is the typed value behind the receipts, for the
 * redacted copy of the exported file, where paths are removed by value.
 */
export function unrecordedSaidRow(unrecorded) {
  if (!unrecorded || !unrecorded.count || !unrecorded.said) return null
  const logs = list(unrecorded.logs)
  return {
    text: unrecorded.said,
    lines: unrecorded.held ? [unrecorded.held] : [],
    receipts: logs.map((l) => l.receipt),
    files: logs.map((l) => l.file),
  }
}

/**
 * Every run written down for this Change, with the version it ran on.
 *
 * Linked evidence first, in criterion order, then the runs the record holds
 * that nobody linked to a line — a run that settles nothing is still a run,
 * and a packet that showed only the linked ones would be quietly selective.
 * The version is named only when the record says the run was about the code
 * this Change is set up against. PACKET-CHECKED-TRUE (verifier try 2): when the
 * record says it was about OTHER code, the row says so in Proof's own sentence
 * (`where`, the item's `appliesText`) and keeps its result word, with a line
 * saying that result is about that other code; only when the record says
 * neither does the caller print that it does not say. What an unsettled run
 * showed is carried in the mapping's words (`lines`), and a run nobody linked
 * says so on its own row (`linked: false`).
 *
 * A RUN IS A `check_run`, AND NOTHING ELSE (T-S-5 verifier finding 2). The
 * coverage document's candidate list also carries `observation` rows, whose
 * names are the engine's own constants — `COMMAND_STARTED`, `EVIDENCE_BOUND`.
 * On live data those reached the copied Markdown as bare all-caps lines beside
 * the one real check. They are not runs, they have no plain-English name, and
 * this is the surface a person hands to somebody else.
 */
export function runsOf(criteria, cov, shortVersion) {
  const out = []
  const seen = new Set()
  const P = HQ_COPY.change.packet
  const push = (i, linked) => {
    const id = str(i.evidenceId) || str(i.id)
    if (!id || seen.has(id)) return
    if (str(i.kind) !== 'check_run') return
    seen.add(id)
    const shown = HQ_COPY.states[i.state] || null
    const other = linked && i.applies === false
    out.push({
      name: str(i.name) || id,
      /* The mapping's word for an unsettled run ("Recorded") too, never none. */
      word: STATE_WORDS[i.state] || (shown && shown.word) || null,
      version: linked && i.applies === true ? shortVersion : null,
      where: other ? str(i.appliesText) : null,
      linked,
      lines: [
        str(i.note),
        shown && i.state !== 'proven' && i.state !== 'not_proven' ? shown.subtitle : null,
        other ? P.checks.not_this_version : null,
        linked ? null : P.markdown.run_unlinked,
      ].filter(Boolean),
    })
  }
  for (const c of list(criteria)) for (const i of list(c.items)) push(i, true)
  for (const i of list(cov && cov.candidates)) push(i, false)
  return out
}

/**
 * RUN-LINK-TRUTH — by criterion index, the satisfied lines the served
 * readiness says count through a person's link: who linked it, and the name
 * of what they linked. The engine decides; this only reads.
 */
function linkedByLines(coverage) {
  const doc = coverage && coverage.readiness
  const out = new Map()
  for (const o of list(doc && doc.obligations)) {
    if (!o || o.outcome !== 'satisfied' || !str(o.linked_by) || typeof o.criterion_index !== 'number') continue
    out.set(o.criterion_index, { who: str(o.linked_by), name: str(o.evidence_name) })
  }
  return out
}

/** What a "Proven:" line names: the evidence, and — when it counts through a
 * person's link — that person, on the same line. */
function provenWhat(state, linked, fallback) {
  if (state !== 'proven' || !linked) return fallback
  return [linked.name || fallback, fill(M.readiness.linked_by, { who: linked.who })].filter(Boolean).join(' — ')
}

/** The engine's reason slug for a line whose applicability was never assessed. */
export const NEVER_ASSESSED_SLUG = 'readiness_never_assessed'

/**
 * The engine's reason slug for a never-assessed line that no `change prepare`
 * can assess (NEVER-ASSESSED-NOT-READY-R2). Still never assessed — it is
 * listed wherever that absence is — but its served sentence says why, and
 * names the act that would settle it instead of promising a prepare.
 */
export const COULD_NOT_ASSESS_SLUG = 'readiness_could_not_assess'

/**
 * Whether this criterion is one the ENGINE names as never assessed
 * (R-2026-09-28-25). The ONE rule both "What was not recorded" and the
 * rests-on list read (P6B-HTT-4), so the two cannot disagree about a line.
 *
 * NOTHING IS DECIDED HERE. It used to be worked out from the items (something
 * linked, and no freshness), which is a second rule on the client; it is now
 * `change_readiness::never_assessed`, served as the obligation's reason and
 * attached to the criterion by identity in `deriveChangeModel`. No readiness
 * served, or no identity to tie the answer to one line, and nothing is
 * marked: absence of an answer is not an answer.
 */
export function neverAssessed(c) {
  return Boolean(c && c.neverAssessed === true)
}

/**
 * The accepted lines the served readiness names as never assessed, by
 * criterion index — by the obligation's criterion_index, else its recorded id
 * (`criterion:<n>`), else its words when exactly one accepted line carries
 * them (the P6B-HTT-4-R2 identity rule; shared words are never guessed at).
 */
function neverAssessedLines(coverage, rows) {
  const doc = coverage && coverage.readiness
  const out = new Set()
  for (const o of list(doc && doc.obligations)) {
    const slug = o ? str(o.unsatisfied_slug) : ''
    if (slug !== NEVER_ASSESSED_SLUG && slug !== COULD_NOT_ASSESS_SLUG) continue
    if (typeof o.criterion_index === 'number') { out.add(o.criterion_index); continue }
    const recorded = /^criterion:([1-9][0-9]*)$/.exec(str(o.id))
    if (recorded) { out.add(Number(recorded[1])); continue }
    const same = list(rows).filter((r) => r.text === str(o.text))
    if (same.length === 1) out.add(same[0].n)
  }
  return out
}

/** A rests-on entry the record does not let the packet tie to one criterion. */
export const UNIDENTIFIED = Object.freeze({ unidentified: true })

/**
 * The criterion a rests-on entry names, by its IDENTITY (P6B-HTT-4-R2): the
 * obligation's criterion_index when the readiness document carries it, else
 * the index in the recorded obligation id (`criterion:<n>`, the engine's own
 * form in change_readiness.rs). Only when neither is served do the accepted
 * words decide, and only when exactly one accepted line carries them; words
 * shared by several lines return UNIDENTIFIED, never the first of them.
 */
export function criterionRestedOn(r, readiness, criteria) {
  const all = list(criteria)
  const o = list(readiness && readiness.obligations).find((x) => x.id && x.id === r.id)
  if (o && typeof o.n === 'number') return all.find((c) => c.n === o.n) || null
  const recorded = /^criterion:([1-9][0-9]*)$/.exec(str(r.id))
  if (recorded) return all.find((c) => c.n === Number(recorded[1])) || null
  const same = all.filter((c) => c.text === r.text)
  if (same.length > 1) return UNIDENTIFIED
  return same[0] || null
}

/**
 * What the record does not hold, said as a list rather than as a blank.
 *
 * Only absences this model can state as facts about typed fields: a line with
 * nothing looked at for it, a line whose evidence was never assessed for
 * whether it still applies, no folders, no version. Nothing here is a
 * judgement about the work.
 */
export function notRecordedOf(criteria, scopePaths, shortVersion) {
  const MD = HQ_COPY.change.packet.markdown
  const out = []
  for (const c of list(criteria)) {
    if (!list(c.items).length) out.push(fill(MD.not_recorded_no_test, { what: c.text }))
    else if (neverAssessed(c)) out.push(fill(MD.not_recorded_no_freshness, { what: c.text }))
  }
  if (!list(scopePaths).length) out.push(MD.not_recorded_no_scope)
  if (!shortVersion) out.push(MD.not_recorded_no_version)
  return out
}

/**
 * The packet as Markdown, from `model.hq` and from nothing else (T-S-5).
 *
 * PURE, and deliberately narrow: its only argument is the one derived
 * view-model, and every number, state word, version and decision act it emits
 * is a field of `hq.packet` copied through. It never reads a document, never
 * touches the DOM, and never receives an agent's sentence except the two
 * halves of a finding, which are the narrated record's own text and are
 * labelled as such.
 *
 * `change-model.test.js` asserts that every digit sequence and every state
 * word in the output appears in the model's typed fields, so a number typed
 * into a heading here fails the suite rather than reaching a person.
 *
 * @param {object} hq what `deriveChangeModel` returned
 * @returns {string}
 */
export function packetMarkdown(hq) {
  const p = (hq && hq.packet) || null
  if (!p) return ''
  const out = []
  const line = (s) => out.push(s)

  line('# ' + (p.title || ''))
  line('')
  if (p.headline) line(p.headline)
  if (p.version) line(p.version)
  if (p.coversLine) line(p.coversLine)
  /* T-3-5 — the answer and what it rests on, together, in the order a person
   * reads them. The list follows the sentence with nothing between them. */
  if (p.readinessBlocker) line(p.readinessBlocker)
  if (p.readinessLine) line(p.readinessLine)
  for (const rest of list(p.readinessRests)) line('- ' + rest)
  /* BRIEF-COUNT-AGREES-R2 — the same two blocks the exported files carry after that list. */
  for (const also of list(p.readinessAlso)) line(also)
  if (p.readinessSetAsideHeading && list(p.readinessSetAside).length) {
    line(p.readinessSetAsideHeading)
    for (const aside of list(p.readinessSetAside)) line('- ' + aside)
  }
  if (p.decision) line(p.decision)
  if (p.decisionListMoved) line(p.decisionListMoved)
  if (p.decisionBy) line(p.decisionBy)
  if (list(p.decisionExceptions).length) {
    if (p.decisionExceptionsHeading) line(p.decisionExceptionsHeading)
    for (const e of list(p.decisionExceptions)) line('- ' + e)
  }

  /* The SAME array the page maps over, in the same order, row for row. */
  for (const section of list(p.sections)) {
    line('')
    line('## ' + section.title)
    if (!list(section.rows).length) { if (section.none) line(section.none); continue }
    for (const row of list(section.rows)) {
      line('- ' + row.text)
      for (const sub of list(row.lines)) line('  ' + sub)
    }
  }

  line('')
  line('---')
  line(p.footer || '')
  return out.join('\n') + '\n'
}

/* ── the decision ──────────────────────────────────────────────────────── */

/**
 * What has been decided about this Change, derived ONCE (T-S-4).
 *
 * The served `decisions` document is rows: every approval and every withdrawal,
 * oldest first, each naming its own version, actor, time and accepted-criteria
 * version, plus the version the record stands at now. Nothing here infers a
 * decision from a state word, and nothing here moves an approval onto a version
 * it was not given for — `movedSince` is the comparison stated, never resolved.
 *
 * `version` is the SHORT form the rest of the screen uses, so the headline and
 * the version line name the same seven characters.
 *
 * @param {object|null} change the `change` object of `/api/changes/<id>`
 * @param {string|null} shortVersion the version the screen is about
 */
export function decisionOf(change, shortVersion) {
  const D = M.decision
  const doc = (change && change.decisions) || null
  /* The FULL revision the record stands at. The screen reads the short form
   * everywhere; the act has to carry the whole one, because the server
   * compares it against the record and a truncated comparison would refuse
   * every approval as a moved tree. */
  const impl = (change && change.implementation) || null
  /* T-3-6-F5: the act used to be allowed to carry an empty string when the
   * served document held no version, which reaches the server as "I saw
   * nothing" and is refused as a moved tree for the wrong reason. Every source
   * here is the SAME order the engine's `standing_target` uses, so the version
   * the screen sends is the version the engine compares against; when all of
   * them are absent, `seen` is null and the button that would send it is off,
   * because `canApprove` needs a version. */
  const seen = str(doc && doc.target_revision)
    || (impl ? (str(impl.snapshot_id) || str(impl.base_revision)) : null)
    || null
  /* T-5-2 fix round. ONE short form per version, decided over every version
   * this document names — the standing approval's, every act in the history,
   * the one the record stands at now, and the bound implementation's — so the
   * headline, the history, the moved sentence, the packet and the Markdown
   * cannot name two different trees with the same seven characters. */
  const forms = shortVersions([
    str(doc && doc.target_revision),
    impl && str(impl.snapshot_id),
    impl && str(impl.base_revision),
    doc && doc.standing ? str(doc.standing.target_revision) : null,
    ...list(doc && doc.history).map((row) => str(row.target_revision)),
  ])
  const nameOf = (v) => forms.get(str(v)) || short(v)
  /* T-5-3 (`B7.20`, `B10.9`). What one act was given UNDER, off the act's own
   * rows — never off the live waivers, so a line in the history can
   * never acquire an exception written after it. */
  const exceptionsOf = (row) => list(row && row.exceptions).map((e) => ({
    obligationId: str(e.obligation_id),
    kind: str(e.kind),
    actor: str(e.actor),
    authority: str(e.authority),
    rationale: str(e.rationale),
    line: fill(D.exception_line, {
      what: str(e.obligation_id) || '',
      why: str(e.rationale) || '',
      who: str(e.actor) || '',
      authority: str(e.authority) || '',
    }),
  }))
  /* T-5-2-NO-REVIVAL (R-2026-09-28-10). The engine names, on each approval a
   * later approval replaced, the one that replaced it; the line says so. */
  const byId = new Map(list(doc && doc.history).map((row) => [str(row.id), row]))
  const replacedBy = (row) => {
    const later = str(row && row.superseded_by) ? byId.get(str(row.superseded_by)) : null
    return later ? ' ' + fill(D.superseded_by_version, { version: nameOf(later.target_revision) || '' }) : ''
  }
  const history = list(doc && doc.history).map((row) => ({
    act: str(row.act),
    exceptions: exceptionsOf(row),
    version: nameOf(row.target_revision),
    actor: str(row.actor),
    when: day(row.decided_at),
    scopeVersion: typeof row.scope_version === 'number' ? row.scope_version : null,
    /* T-5-2 (`B10.9`). Every line names the VERSION its act was about. A
     * history of "Approved by Sam on …" then "Approval withdrawn by Sam on …"
     * reads as one decision that followed the code; these two acts were about
     * two different versions and the list has to say which. */
    line: str(row.act) === 'approved'
      ? fill(list(row.exceptions).length ? D.approved_with_exceptions_by_version : D.approved_by_version, { version: nameOf(row.target_revision) || '', who: str(row.actor) || '', when: day(row.decided_at) || '', scope: String(row.scope_version) }) + replacedBy(row)
      : fill(D.withdrawn_by_version, { version: nameOf(row.target_revision) || '', who: str(row.actor) || '', when: day(row.decided_at) || '' }),
  }))
  const standing = doc && doc.standing ? doc.standing : null
  if (!standing) {
    return {
      approved: false,
      version: null,
      seen,
      approvedFull: null,
      movedSince: false,
      listMovedSince: false,
      headline: null,
      movedLine: null,
      movedWhileOpenLine: null,
      listMovedLine: null,
      nowVersion: str(doc && doc.target_revision) ? nameOf(doc.target_revision) : shortVersion,
      sentence: D.not_an_action,
      none: D.none,
      history,
      withExceptions: false,
      exceptions: [],
      exceptionsHeading: D.exceptions_heading,
      canApprove: Boolean(shortVersion),
      canWithdraw: false,
      approveLabel: D.approve,
      withdrawLabel: D.withdraw,
      heading: D.heading,
    }
  }
  const approvedVersion = nameOf(standing.target_revision)
  /* The comparison the whole task turns on: the version the approval names
   * against the version the record stands at NOW. It is stated on the screen
   * and never resolved by moving the approval.
   *
   * T-5-2 fix round: compared on the WHOLE revisions when the document holds
   * both, never on the labels. Two versions can share a label — that is
   * finding 1 — so a comparison of labels could call a moved tree unmoved. */
  const nowFull = str(doc && doc.target_revision)
  const approvedFull = str(standing.target_revision)
  const nowVersion = nowFull ? nameOf(nowFull) : shortVersion
  const movedSince = nowFull && approvedFull
    ? nowFull !== approvedFull
    : Boolean(shortVersion) && approvedVersion !== shortVersion
  /* T-5-3 (`B7.22`). Whether THIS approval was given with something set
   * aside, read off its own row. The headline below is chosen from it, so the
   * words "Approved on {version}" can never appear over an approval that was
   * not one. */
  const exceptions = exceptionsOf(standing)
  const withExceptions = exceptions.length > 0
  /* T7 (report F5). The list of what counts as done the approval was given
   * against, beside the list the record stands at now — both typed fields of
   * the served decisions document. Derived, never stored, the same rule
   * `code_changed_since` states; stated, never resolved by moving the
   * approval onto the new list. */
  const thenList = typeof standing.scope_version === 'number' ? standing.scope_version : null
  const nowList = doc && typeof doc.scope_version === 'number' ? doc.scope_version : null
  const listMovedSince = thenList !== null && nowList !== null && thenList !== nowList
  return {
    approved: true,
    version: approvedVersion,
    seen,
    approvedFull,
    movedSince,
    withExceptions,
    exceptions,
    exceptionsHeading: D.exceptions_heading,
    headline: fill(
      withExceptions
        ? (movedSince ? D.approved_with_exceptions_moved : D.approved_with_exceptions)
        : (movedSince ? D.approved_moved : D.approved),
      { version: approvedVersion || '' },
    ),
    /* T-5-2 (`Q13`). The second sentence, for the surfaces that show the
     * headline: which version the approval names, and which one the record
     * stands at now. Null when they are the same, so nothing ever says a
     * Change moved that did not.
     *
     * MOVED-WHILE-OPEN-TRUTH. This is what a screen may say of a move it did
     * not see: the two facts, and nothing about the reader's own session. A
     * fresh derivation cannot know whether the screen was open when the record
     * moved, so it never says so. */
    movedLine: movedSince
      ? fill(D.moved_since_approval, { version: approvedVersion || '', now: nowVersion || '' })
      : null,
    /* The same two facts, said as the screen's own experience: it was open and
     * saw the served version change. Read by the one caller that watches the
     * record across its own reads (`change-workspace.jsx`, `movedWhileOpen`),
     * which puts it in `movedLine`'s place; nothing else reads it. */
    movedWhileOpenLine: movedSince
      ? fill(D.moved_while_reading, { version: approvedVersion || '', now: nowVersion || '' })
      : null,
    /* T7: which list the approval was given against, and which one is
     * accepted now. Null when they are the same, so nothing says a list moved
     * that did not. One field: the Decision, the Brief headline, the packet
     * page and the exported file all print this string. */
    listMovedSince,
    listMovedLine: listMovedSince ? fill(D.list_moved, { then: thenList, now: nowList }) : null,
    /* The version the record stands at now, named by the same rule, so the
     * two halves of that sentence can never be the same string. */
    nowVersion,
    sentence: D.not_an_action,
    by: fill(D.approved_by, {
      who: str(standing.actor) || '',
      when: day(standing.decided_at) || '',
      scope: String(standing.scope_version),
    }),
    none: D.none,
    history,
    canApprove: false,
    canWithdraw: true,
    approveLabel: D.approve,
    withdrawLabel: D.withdraw,
    heading: D.heading,
  }
}

/** What the record said moved, in Sam's words, from a refused act's answer. */
export function movedSentences(answer) {
  const D = M.decision
  const files = list(answer && answer.changed).map(String).filter(Boolean)
  const out = [D.moved_refused]
  if (files.length) out.push(files.length === 1 ? D.moved_files_one : fill(D.moved_files_many, { n: files.length }))
  else out.push(D.moved_unnamed)
  return { lines: out, files, enabler: D.moved_enabler }
}

const short = (v) => {
  const s = str(v)
  if (!s) return null
  return s.length > 7 ? s.slice(0, 7) : s
}

/** The shortest prefix a version may ever be shown as. */
export const SHORTEST_VERSION = 7

/**
 * Short forms for a set of versions that are UNIQUE AMONG THEMSELVES (T-5-2
 * fix round, verifier finding 1).
 *
 * A fixed seven characters is safe for a git revision and unsafe for a ULID:
 * the leading ten characters of a ULID are the millisecond it was minted, so
 * two snapshots taken seconds apart share them. That is not hypothetical —
 * one Change in this task's own acceptance walk held two real approvals,
 * `01M37KZMP5NB…` and `01M37KZP1AZX…`, and both rendered as "Approved on
 * 01M37KZ". A label that can name the wrong thing is not a name, and "is it
 * one version, named?" is the question this whole screen exists to answer.
 *
 * The rule is git's own: the shortest prefix, never below seven, that tells
 * every version the document names apart. When two versions cannot be told
 * apart by any prefix — one is a prefix of the other — every one of them is
 * named in full rather than named ambiguously.
 *
 * It takes the WHOLE set at once, because uniqueness is a property of the
 * set and not of one string; a per-string helper could not have this
 * property, which is why the old one did not.
 *
 * @param {Array<string|null|undefined>} versions every version the surface names
 * @returns {Map<string, string>} full version → the form to show
 */
export function shortVersions(versions) {
  const all = [...new Set(list(versions).map((v) => str(v)).filter(Boolean))]
  const longest = all.reduce((n, v) => Math.max(n, v.length), SHORTEST_VERSION)
  const at = (v, n) => (v.length > n ? v.slice(0, n) : v)
  let width = SHORTEST_VERSION
  for (; width < longest; width += 1) {
    if (new Set(all.map((v) => at(v, width))).size === all.length) break
  }
  /* At `longest` the prefixes ARE the ids, so this ends; if two still collide
   * there, one id is a prefix of another and both are shown whole. */
  return new Map(all.map((v) => [v, at(v, width)]))
}

const day = (v) => {
  const s = str(v)
  const m = s ? /^(\d{4}-\d{2}-\d{2})T\d{2}:\d{2}:\d{2}/.exec(s) : null
  return m ? m[1] : s
}

/**
 * May a surface offer the decision (approve this version, or take an approval
 * back)? THE one rule, read by every surface that draws a decide control: the
 * Inspector's Decision, the "Review and decide" door in "What we checked", and
 * Brief's off Approve (which shows exactly when this says no and the engine is
 * not ready). RUN-LINK-TRUTH-R3: "What we checked" used to answer with a second
 * rule — every line proven and `canApprove` — that never read readiness, and
 * offered a live decision beside a record the engine said was not ready.
 *
 * Approving needs the ENGINE's `readiness.ready`; a line's own proven state is
 * not enough (an unconfirmed identity is tied to no line). Withdrawing is never
 * gated on readiness — taking an approval back is always reachable.
 *
 * @param {object|null} hq the derived Change model
 * @returns {boolean}
 */
export const showsDecision = (hq) => Boolean(hq && hq.decision && (hq.decision.canWithdraw || (hq.decision.canApprove && hq.readiness && hq.readiness.ready)))

/**
 * The Inspector's one primary (block 4), for a state.
 *
 * Exactly one primary button, chosen by state, and off in every state this
 * build cannot act on — with the reason and the enabler for THAT BUTTON'S OWN
 * ACT.
 *
 * T-S-3 verifier finding 1: this took `view.blocker` unconditionally, which is
 * the Approve blocker, so a button reading "Run the check again" carried an
 * explanation about there being nothing to approve. The Approve state still
 * takes `view.blocker` — that is its own act and the Change screen already
 * answers it. Every other state takes the pair keyed by its own state.
 *
 * T-3-6-F3 (`Q2`, rule 7). Reading what broke is an act this build CAN do, on
 * this screen, so the one primary on a Change with a failure is a door to the
 * line that broke rather than a dead button asking for a run that already
 * happened. `door` is the criterion it opens; the view scrolls to it. It is
 * on only when there IS a line to open (A3-F4): otherwise the state's own
 * pair, never an empty one.
 *
 * RUN-LINK-INSPECTOR. After a passing run no line is linked to, the
 * Inspector's one primary is the run sheet's own door to that line's link
 * control — the same words (`run.outcomes.passed.unlinked_door`) and the same
 * target — rather than "Run the check" for a run that already happened. It is
 * carried as `link`, BESIDE the fields below, which stay what they were: the
 * Verify stage's summary reads `enabled`/`door`/`reason`/`enabler` and keeps
 * saying why approval is off. `unlinkedLine` is the leading line when the
 * engine and its row both say so (see `deriveChangeModel`); with none, the
 * object has exactly the fields it had.
 *
 * @param {string|null} state the leading state
 * @param {number|null} leadingN the leading criterion's number
 * @param {{reason: string, enabler: string}} approveBlocker
 * @param {object|null} readiness
 * @param {{n: number, text: string}|null} [unlinkedLine]
 */
export function inspectorAction(state, leadingN, approveBlocker, readiness, unlinkedLine = null) {
  const open = state === 'not_proven' && leadingN != null
  const label = primaryActionLabel(state)
  const approval = !open && (approvalIsOff(state, readiness) || Boolean(readiness && readiness.ready))
  const link = !open && unlinkedLine && typeof unlinkedLine.n === 'number'
    ? { n: unlinkedLine.n, label: fill(HQ_COPY.run.outcomes.passed.unlinked_door, { line: String(unlinkedLine.text == null ? '' : unlinkedLine.text) }) }
    : null
  return {
    heading: M.inspector.action,
    label,
    /* RUN-LINK-TRUTH-R4 — the label an OFF control carries. When its pair is
     * the one answer to "why is approval off", the control that carries it is
     * the Approve control, so the noun on the button is the noun in the
     * sentence (T-S-3 finding 1). A live run door keeps `label`. */
    offLabel: approval ? M.actions.approve : label,
    enabled: open,
    door: open ? leadingN : null,
    ...(open ? { reason: '', enabler: '' } : actionBlocker(state, approveBlocker, readiness)),
    ...(link ? { link } : {}),
  }
}

/* Is the off primary's pair the readiness blocker? For the Approve state
 * whenever readiness is not ready (as before); for every other state when a
 * readiness document was served and says not ready. */
const approvalIsOff = (state, readiness) => Boolean(readiness && !readiness.ready && (state === 'proven' || !readiness.absent))

/**
 * Why the Inspector's one primary button is off, and what would turn it on —
 * about the act on THAT button. When the pair is the readiness blocker, the
 * control drawn off is the Approve control (`offLabel`), so the act is approving.
 *
 * RUN-LINK-TRUTH-R4 — ONE answer. Whenever the engine served a readiness
 * document that says not ready, the pair is `readiness.blocker`, the same
 * pair Brief and the Approve stage print, chosen once from the first missing
 * obligation in the engine's order. Round 2 had this look up the LEADING
 * line's own reason instead; round 3's adversary served an unconfirmed
 * identity first and an unlinked-run line, and "What we checked" said "link
 * it" while Brief said "run the checks". No leading-line lookup is left: a
 * line's own reason is the Verify stage's to print, and it still does.
 *
 * The state-keyed pair is the fallback only when no readiness was served.
 * The Approve state keeps its old rule: with no readiness at all it delegates
 * to `approveBlocker` in `change-view.js`.
 *
 * @param {string|null} state
 * @param {{reason: string, enabler: string}} approveBlocker
 * @param {object|null} readiness `readinessOf`'s answer
 * @returns {{reason: string, enabler: string}}
 */
export function actionBlocker(state, approveBlocker, readiness) {
  /* RUN-LINK-TRUTH-R6 — a READY answer is explained from itself, never from
   * `approveBlocker` or a state's pair: round 5's adversary found a line set
   * aside (T-5-3) counted as "no evidence yet" by the second derivation. */
  if (readiness && readiness.ready && readiness.available) {
    return { reason: readiness.available.reason, enabler: readiness.available.enabler }
  }
  if (approvalIsOff(state, readiness)) {
    return { reason: readiness.blocker.reason, enabler: readiness.blocker.enabler }
  }
  if (state === 'proven') return { reason: approveBlocker.reason, enabler: approveBlocker.enabler }
  const pair = M.blocked[state] || M.blocked.read
  return { reason: pair.reason, enabler: pair.enabler }
}

/** The one primary action for a state (Inspector block 4). Exactly one, always. */
export function primaryActionLabel(state) {
  const A = M.actions
  if (state === 'needs_recheck') return A.recheck
  /* T-3-6-F3 (`Q2`): a line the record holds the OPPOSITE of does not need
   * another run; it needs reading. `recorded` keeps "Run the check" — a run
   * that checked nothing has nothing to read. */
  if (state === 'not_proven') return A.look
  if (state === 'recorded') return A.run
  if (state === 'in_progress') return A.run
  if (state === 'proven') return A.approve
  return A.read
}

/**
 * The one version sentence, wherever a version is named.
 *
 * It was `versionLine` in `change-workspace.jsx`, which meant the Change
 * screen owned it and the Inbox and packet each re-derived it. It is here now
 * and the three call it.
 */
export function versionSentence(version) {
  const V = HQ_COPY.change.version
  const v = version || {}
  if (v.key === 'placeholder') return V.placeholder
  if (!v.bound) return V.none
  return v.date
    ? fill(V.bound, v.values)
    : fill(V.bound, { ...v.values, date: '' }).replace(/\s*·\s*$/, '')
}

export default deriveChangeModel
