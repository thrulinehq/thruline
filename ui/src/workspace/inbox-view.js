/* NOT-APPROVAL-WORDS: renders the Inbox reasons' copy; a row's reason is the top sentence about evidence, never why approval is off (RUN-LINK-TRUTH-R5 census). */
/* The Inbox — the deterministic model behind the app's landing (T-2-6).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness, no model. It
 * takes the documents the Change routes already serve (`/api/changes` for the
 * list, and per Change the same pair `change-workspace.js` reads) plus the
 * confirmed work the existing adapter builds, and decides what one line of the
 * Inbox says. `inbox.jsx` only lays out what this decided, and every word it
 * lays out comes from `HQ_COPY.change.inbox`.
 *
 * WHAT THIS FILE IS FOR. Step 2 of the Sam walkthrough: "is there one obvious
 * thing to do, and can Sam say in their own words why it's there?" One item
 * per Change that needs something, the REASON in Sam's words, and ONE next
 * action that opens the Change at the stage which would settle that reason.
 *
 * THE RULES IT HOLDS:
 *
 *  - SEVEN reasons, in one priority order (`REASONS`), highest first. A Change
 *    with several shows the highest as its line and the rest under it — one
 *    item per Change, never one per reason (`B8.4`, `B8.5`).
 *  - Every number in every sentence comes from a TYPED field of the record:
 *    a count of findings of a named class, a count of accepted criteria with
 *    nothing behind them, a count of required checks the record holds no
 *    result for. Nothing here counts events, sessions, days or anything a
 *    person did not ask about (`B8.6`).
 *  - Missing evidence is missing. A criterion with nothing behind it is a
 *    reason to look, never a pass and never a failure (`B7.2`).
 *  - A suggestion is a suggestion (`B8.8`, `B3.12`): only CONFIRMED work
 *    contributes a reason. Suggested work is a secondary line, counted apart,
 *    and can never make an item appear or change its reason.
 *  - No state word, no percentage, no colour carrying meaning. A reason is a
 *    sentence about the record, not a verdict about the work.
 *
 * WHAT IT DELIBERATELY DOES NOT DO. Nothing here links one criterion to one
 * piece of evidence, because the record holds no such link yet (the boundary
 * `change-view.js` and `packet-view.js` both state). `evidence` arrives as a
 * TYPED input and today's routes hand it in empty, so every accepted criterion
 * counts as having nothing behind it. `invalidated_after_edit` and
 * `awaiting_approval` are the same shape: they read typed inputs
 * (`applicability`, `decision`) that Phase 3 and Phase 5 will write, they are
 * implemented and tested against typed records, and they are unreachable from
 * the served routes until those records exist. Matching a criterion to a
 * finding by its words, or reading a session's verdict as this Change's
 * verdict, would be the collapse `B7.2` and `B3.13` forbid.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { scopeOfWork, targetVersion, projectNameOf, STAGES } from './change-view.js'
import { REASONS, findingCounts, reasonsFor, attentionOf } from './attention.js'

/* A3-F2 — the seven reasons, the finding counts and whether a Change needs a
 * person at all are decided in ONE place, `attention.js`, and re-exported
 * here so nothing that already imports them from the Inbox's model moves. */
export { REASONS, findingCounts, reasonsFor, attentionOf }

const C = HQ_COPY.change.inbox

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/**
 * The stage of the Change that would settle each reason. The one next action
 * opens the Change there, so the click lands on the thing that resolves the
 * sentence the person just read rather than on the Change's front door.
 */
export const REASON_STAGE = Object.freeze({
  observed_failure: 'verify',
  applicable_finding: 'verify',
  interrupted_capture: 'verify',
  missing_evidence: 'verify',
  invalidated_after_edit: 'build',
  approved_code_moved: 'build',
  awaiting_definition: 'define',
  awaiting_approval: 'approve',
})

/* Every stage named above is one of the six. A typo here would send a person
 * to the Define view with no sign anything was wrong. */
for (const stage of Object.values(REASON_STAGE)) {
  if (STAGES.indexOf(stage) < 0) throw new Error('inbox: unknown stage ' + stage)
}

/**
 * A run that was cut short, in ITS OWN words (T-3-4 verifier finding 2).
 *
 * When the reason a Change is in the Inbox is an interrupted capture and the
 * model holds a stopped run, the row prints that run's own execution sentence
 * — "This check was interrupted when ThruLine restarted. Run it again." — and
 * not the generic one about the recording having gaps. `null` in every other
 * case, so the caller falls through to the ordinary sentence.
 */
export function interruptedText(input, head) {
  const i = input || {}
  if (!head || head.id !== 'interrupted_capture') return null
  const stopped = list(i.hq && i.hq.stoppedRuns)
  return stopped.length ? str(stopped[0].executionLine) : null
}

/**
 * A Change ready with exceptions, in the served readiness's own words
 * (TELL-DRAFT-WAIVED). When the reason it is listed for is that it waits for
 * its approver and the engine says it is ready WITH exceptions, the row says
 * what the Approve control says: that everything has evidence behind it except
 * what a person set aside. The top sentence of the Change screen counts a
 * set-aside line as one that "still needs" evidence, which the row must not
 * repeat beside a Change that waits only on a decision. `null` in every other
 * case, so the caller falls through to the ordinary sentence.
 */
export function withExceptionsText(input, head) {
  const r = input && input.hq && input.hq.readiness
  if (!head || head.id !== 'awaiting_approval' || !r || !r.ready || !r.withExceptions || !r.available) return null
  return str(r.available.reason)
}

/**
 * The sentence for one reason, and the copy key it came from.
 *
 * `fill` is left to the view: this returns the template and the values, so a
 * test can assert which line was chosen without matching prose.
 *
 * @param {{id: string, values: object, count: number, variant: ?string}} reason
 * @returns {{template: string, values: object, action: string, stage: string}}
 */
export function reasonLine(reason) {
  const r = reason || {}
  const entry = C.reasons[r.id]
  if (!entry) throw new Error('inbox: no copy for reason ' + r.id)
  const items = typeof r.count === 'number' ? r.count : 0
  /* BRIEF-COUNT-AGREES-R2 — a reason that adds up to some number of items may say another: "N things that count as
   * done have no evidence yet" is a count of THOSE, and what is missing that is not one of them (the identity, a
   * review) is an item that needs you and is said in its own words, never among them. */
  const n = typeof r.shown === 'number' ? r.shown : items
  let template
  if (r.variant === 'checks') template = n === 1 ? entry.checks_one : entry.checks
  else if (r.shown === 0 && typeof r.words === 'string' && r.words) template = r.words
  else template = entry.line_one && n === 1 ? entry.line_one : entry.line
  if (typeof template !== 'string') throw new Error('inbox: no line for reason ' + r.id)
  return {
    template,
    values: template.includes('{n}') ? { n } : {},
    action: entry.action,
    stage: REASON_STAGE[r.id],
  }
}

/**
 * Where the work is, in the words the Change screen uses for it.
 *
 * The version comes from `targetVersion`, the same function the Change screen
 * and the packet page use, so no two surfaces can name a version differently.
 *
 * @param {object} change
 * @returns {{repository: string|null, folder: string|null, project: string|null, version: object}}
 */
export function scopeLine(change) {
  const b = (change && change.implementation) || null
  const folder = b ? str(b.workspace_identity) : null
  /* BUILD-14 — `project` is what the card shows; `repository` and `folder`
   * are the record's own strings, for the technical layer only. */
  return { repository: b ? str(b.repository_identity) : null, folder, project: projectNameOf(folder), version: targetVersion(change) }
}

/** The address of one Change, opened at the stage that settles its reason. */
export function changeHrefAt(id, stage) {
  const at = STAGES.indexOf(stage) >= 0 ? stage : 'define'
  return '#/changes/' + encodeURIComponent(String(id)) + '/' + at
}

/**
 * One Inbox item, or null when this Change needs nothing.
 *
 * An item is built only for a Change with at least one reason. A Change with
 * none is not listed at all: the Inbox is what needs attention, and padding it
 * with rows that need nothing is the decorative statistic `B8.6` forbids in a
 * different costume.
 *
 * @param {object} input the same input `reasonsFor` takes
 * @returns {object|null}
 */
export function inboxItem(input) {
  const i = input || {}
  const change = i.change || {}
  const id = str(change.id)
  if (!id) return null
  /* A3-F2 — whether this Change is listed is `attentionOf`'s decision and
   * no one else's, so every surface that counts Changes needing a person
   * counts the same ones. */
  const attention = attentionOf(i)
  if (!attention.needsPerson) return null
  const reasons = attention.reasons
  const head = reasons[0]
  const line = reasonLine(head)
  return {
    id,
    title: str(change.request && change.request.title) || '',
    openedAt: str(change.opened_at) || '',
    reason: head,
    line,
    also: reasons.slice(1).map((r) => ({ reason: r, line: reasonLine(r) })),
    /* P6-INBOX-REASON — the row's reason line is the model's, so the row and
     * the Change screen state the same thing about the same Change. */
    /* P6-INBOX-REASON / T-3-4 verifier finding 2 — the one sentence the row
     * prints. The model's, when there is one, so the row and the Change screen
     * say the same thing about the same Change; the reason's own template
     * otherwise, which is the case before anything has been accepted and the
     * model has no headline to give. A run that was cut short is named in the
     * reason's own words rather than left as a generic capture gap. */
    reasonText: (head.id === 'approved_code_moved' ? fill(line.template, line.values) : null)
      || interruptedText(i, head) || withExceptionsText(i, head) || (i.hq && i.hq.inboxRow && str(i.hq.inboxRow.reason))
      || fill(line.template, line.values),
    scope: scopeLine(change),
    suggested: scopeOfWork(change).suggested.length,
    /* A3-F2 — how many things on this Change need you, and how many notes
     * sit beside them, from the one derivation. */
    attention: { items: attention.items, notes: attention.notes },
    href: changeHrefAt(id, line.stage),
    /* T-S-3 — the one derived view-model for this Change, carried through
     * untouched. The row renders its state word and its version from here; this
     * file derives neither, so the Inbox cannot disagree with the Change screen
     * or the packet about one line. */
    hq: i.hq || null,
  }
}

/**
 * The Inbox, ordered.
 *
 * By reason first — the order of `REASONS` — and within one reason by the
 * recorded opening time, newest first. Both halves are deterministic over the
 * record, so the same store always draws the same screen and nothing moves
 * under a person between two reads.
 *
 * @param {Array<object>} inputs one per Change, each as `inboxItem` takes
 * @returns {{items: object[], settled: number}} `settled` is how many Changes
 *   were read and needed nothing — stated, never hidden (Rule 12).
 */
export function inboxItems(inputs) {
  const items = []
  let settled = 0
  for (const input of list(inputs)) {
    const item = inboxItem(input)
    if (item) items.push(item)
    else if (input && input.change && str(input.change.id)) settled += 1
  }
  items.sort((a, b) => {
    const byReason = REASONS.indexOf(a.reason.id) - REASONS.indexOf(b.reason.id)
    if (byReason !== 0) return byReason
    const byTime = String(b.openedAt).localeCompare(String(a.openedAt))
    return byTime !== 0 ? byTime : String(a.id).localeCompare(String(b.id))
  })
  return { items, settled }
}

/* ── whose turn it is (A3-F14; report line 1181) ─────────────────────── */

/**
 * The Inbox's sections, in the report's fixed order: whose move each Change
 * is waiting on. Waiting on you · Needs a check · Waiting on the agent ·
 * Recording now · Nothing needed.
 */
export const WHOSE_TURN = Object.freeze(['you', 'check', 'agent', 'recording', 'nothing'])

/**
 * The sections no field of the record can fill yet. The record keeps no
 * hand-off to the agent, and whether a session is still recording is not a
 * typed fact. Each ships disabled, with its reason and what would turn it on,
 * and never holds an item or states a number — nothing is inferred to fill
 * it, and nothing is read from browser storage.
 */
export const NEEDS_ENGINE_SECTIONS = Object.freeze(['agent', 'recording'])

/**
 * Which section a Change goes in, by the reason it is in the Inbox at all
 * (its highest). A pure table over typed reason ids: a decision waiting on a
 * person is "Waiting on you"; everything the record has not settled is
 * "Needs a check". Missing evidence never satisfies a check, so a line with
 * nothing behind it is never "Nothing needed".
 */
export const SECTION_OF_REASON = Object.freeze({
  observed_failure: 'check',
  applicable_finding: 'check',
  interrupted_capture: 'check',
  missing_evidence: 'check',
  invalidated_after_edit: 'check',
  approved_code_moved: 'you',
  awaiting_definition: 'you',
  awaiting_approval: 'you',
})

for (const reason of REASONS) {
  if (!SECTION_OF_REASON[reason]) throw new Error('inbox: no section for reason ' + reason)
}

/** The one section an item belongs to. An unknown reason is an error, never a guess. */
export function sectionOf(item) {
  const id = item && item.reason && item.reason.id
  const section = id ? SECTION_OF_REASON[id] : null
  if (!section) throw new Error('inbox: no section for reason ' + id)
  return section
}

/**
 * The Inbox's items, sectioned.
 *
 * `count` is the number the section's heading states: the items in it, or,
 * for "Nothing needed", the Changes that were read and need nothing. A
 * disabled section's count is `null` — it states no number at all.
 * `attention` marks the sections whose counts add up to the Inbox heading's.
 *
 * @param {object[]} items what `inboxItems` returned, already ordered
 * @param {number} settled the Changes read that need nothing
 */
export function inboxSections(items, settled) {
  return WHOSE_TURN.map((id) => {
    if (id === 'nothing') return { id, attention: false, disabled: false, items: [], count: settled || 0 }
    if (NEEDS_ENGINE_SECTIONS.includes(id)) return { id, attention: true, disabled: true, items: [], count: null }
    const mine = list(items).filter((item) => sectionOf(item) === id)
    return { id, attention: true, disabled: false, items: mine, count: mine.length }
  })
}

/* ── what this machine is keeping (`L1.1`) ─────────────────────────────── */

/** The units, smallest first. Base ten, because that is what a Mac's own disk readouts use. */
const UNITS = Object.freeze([
  ['B', 1],
  ['KB', 1e3],
  ['MB', 1e6],
  ['GB', 1e9],
  ['TB', 1e12],
])

/**
 * A byte count as a person reads it: one decimal, never a made-up number.
 *
 * The input is the byte count the server measured. Anything that is not a
 * finite, non-negative number returns null, and the sentence is then the one
 * that says the size could not be measured — never a zero, which would read
 * as "this product is keeping nothing".
 *
 * @param {number} bytes
 * @returns {string|null}
 */
export function formatSize(bytes) {
  if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes < 0) return null
  let unit = UNITS[0]
  for (const u of UNITS) if (bytes >= u[1]) unit = u
  const value = bytes / unit[1]
  if (unit[0] === 'B') return Math.round(value) + ' B'
  return (Math.round(value * 10) / 10).toFixed(1) + ' ' + unit[0]
}

/**
 * The storage sentence, from the served field and nothing else.
 *
 * `parts` is the breakdown for the technical layer (T-2-10, finding 1): the
 * server serves a KEY and a byte count per component and this names it from
 * the copy file, largest first, so the word a person reads lives where every
 * other word a person reads lives. A key the copy file does not know is named
 * "everything else" rather than shown as an internal word or dropped — the
 * number would stop adding up to the total either way.
 *
 * @param {{bytes: ?number, path: ?string, parts: ?object[]}|null} storage the
 *   `/api/storage` body
 * @returns {{known: boolean, size: string|null, path: string|null, also: string|null,
 *   bytes: number|null, parts: Array<{key: string, noun: string, size: string, bytes: number}>}}
 */
export function storageReading(storage) {
  const s = storage || {}
  const bytes = typeof s.bytes === 'number' && Number.isFinite(s.bytes) && s.bytes >= 0 ? s.bytes : null
  const size = formatSize(bytes)
  const nouns = C.storage.parts
  const parts = list(s.parts)
    .map((p) => {
      const n = p && typeof p.bytes === 'number' && Number.isFinite(p.bytes) && p.bytes >= 0 ? p.bytes : null
      const key = p && str(p.key)
      if (n == null || !key) return null
      return { key, noun: nouns[key] || nouns.other, size: formatSize(n), bytes: n }
    })
    .filter((p) => p && p.bytes > 0)
    .sort((a, b) => b.bytes - a.bytes || a.key.localeCompare(b.key))
  /* `also`: the second folder the server counted (THRULINE_HOME kept apart
   * from HOME), or null — the number above already includes it.
   * `alsoCommandLog`: the command log's own file, counted on its own because
   * the folder it is kept in holds the records folder (THRULINE_HOME = HOME),
   * or null. Never both. */
  return { known: size != null, size, path: str(s.path), also: str(s.also), alsoCommandLog: str(s.also_command_log), bytes, parts }
}

export default inboxItems
