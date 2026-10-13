/* Ask — a scoped command bar, never a chat (P6-4; design record §3.5).
 *
 * PURE, and deliberately narrow. Every function here takes the ONE derived
 * view-model `deriveChangeModel` returned and returns typed fields. It reads
 * no document, touches no DOM, reaches no address, and — this is the whole
 * point — passes through NO MODEL. An answer is a record query: a fixed set of
 * five questions, each a function over fields the model already derived, with
 * the evidence it used named beside it and a door to where that evidence is on
 * the screen.
 *
 * WHAT IT MAY SAY. Two sources and no third: `HQ_COPY.ask` for every word Ask
 * itself contributes, and the model's own typed fields for every sentence
 * about this particular record (a criterion's accepted text, its state line,
 * the readiness sentence, what the record holds beside what was said). Those
 * fields are themselves either the record's own words or `HQ_COPY` filled by
 * `change-model.js`. `ask-hq.test.js` drives that as a rule over the rendered
 * answers rather than as a list of strings: every string in an answer is
 * either a copy string or a value present in the model.
 *
 * WHAT IT REFUSES TO SAY. Nothing it cannot ground. Where the record cannot
 * answer one of the five, the answer is `answered: false` and the same three
 * fields every time — what Ask looked for, what is missing, and what would
 * answer it — because an unanswerable question with a confident-looking blank
 * beside it is the failure this file exists to prevent. Free text that matches
 * none of the five lands on the same shape, with the five offered. There is no
 * other path: Ask cannot produce a sentence nobody wrote.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { setAsideSaid } from './change-view.js'

const A = HQ_COPY.ask
const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/* ── where an answer's evidence lives on the Change screen ─────────────── */

/**
 * The anchors Ask's doors open. Each is a stage of the six-stage strip and the
 * id of an element THAT STAGE ALREADY RENDERS — Ask adds no surface of its own
 * to point at, so a door can never lead somewhere that is not on the screen.
 */
export const EVIDENCE_ANCHORS = Object.freeze({
  criterion: (n) => 'ws-cov-line-' + n,
  readiness: 'ws-change-readiness',
  decision: 'ws-change-decision',
  findings: 'ws-change-cards',
})

const criterionDoor = (n) => ({ stage: 'verify', anchor: EVIDENCE_ANCHORS.criterion(n) })
const readinessDoor = { stage: 'approve', anchor: EVIDENCE_ANCHORS.readiness }
const decisionDoor = { stage: 'verify', anchor: EVIDENCE_ANCHORS.decision }
const findingsDoor = { stage: 'verify', anchor: EVIDENCE_ANCHORS.findings }

/**
 * Take a person to one piece of evidence: the stage that holds it, then the
 * line itself.
 *
 * A DOOR, NOT AN ACT. Nothing is written, nothing is read, and with no
 * document (the static-markup tests, and the server) it does nothing at all
 * rather than throwing. The scroll happens after the stage has rendered,
 * because the element does not exist until then.
 *
 * @param {{stage: string, anchor: string}|null} door
 * @param {(stage: string) => void} onGo the Change screen's stage move
 * @param {Document|null} [doc]
 * @param {(fn: Function) => void} [later] how to wait for the render
 * @returns {boolean} whether there was a door to open
 */
export function openDoor(door, onGo, doc, later) {
  if (!door || !door.anchor) return false
  if (typeof onGo === 'function' && door.stage) onGo(door.stage)
  const d = doc || (typeof document === 'undefined' ? null : document)
  if (!d || typeof d.getElementById !== 'function') return true
  const land = () => {
    const el = d.getElementById(door.anchor)
    if (!el) return
    if (typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'start' })
    /* Focus follows the selection, the same rule the stage strip keeps: a
     * keyboard reader lands on the thing the answer pointed at, not on the
     * page. A line is not focusable by itself, so it is made a stop only for
     * script and stays out of the Tab order. */
    if (typeof el.setAttribute === 'function' && !el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1')
    if (typeof el.focus === 'function') el.focus()
  }
  const wait = typeof later === 'function'
    ? later
    : (fn) => (typeof setTimeout === 'function' ? setTimeout(fn, 0) : fn())
  wait(land)
  return true
}

/* ── the five questions ────────────────────────────────────────────────── */

/**
 * The five, in the order they are offered. `id` is what everything else keys
 * on; `words` are what a typed question is matched against, and they are
 * matching material only — nothing here is ever rendered.
 */
export const QUESTIONS = Object.freeze([
  Object.freeze({ id: 'needs_evidence', words: Object.freeze(['evidence', 'needs', 'still', 'missing', 'proof', 'proven', 'waiting', 'left']) }),
  Object.freeze({ id: 'changed_since', words: Object.freeze(['changed', 'change', 'since', 'moved', 'last', 'check', 'recheck', 're-check', 'stale', 'applies']) }),
  Object.freeze({ id: 'why_not_approve', words: Object.freeze(['approve', 'approval', 'ready', 'accept', 'sign', 'blocked', 'why']) }),
  Object.freeze({ id: 'said_vs_record', words: Object.freeze(['agent', 'claim', 'claimed', 'said', 'say', 'told', 'back', 'backed', 'ai']) }),
  Object.freeze({ id: 'not_recorded', words: Object.freeze(['recorded', 'record', 'not', 'gap', 'gaps', 'written', 'down', 'nothing', 'unrecorded']) }),
])

/**
 * Whether this version stands approved and its ready answer is not offered
 * beside the approval (`change-model.js`, `hiddenBecause`). Then "Why can't I
 * approve?" is the wrong question: it is offered, and answered, in words that
 * fit.
 */
const approvedHere = (hq) => Boolean(hq && hq.readiness && hq.readiness.hiddenBecause === 'approved' && hq.decision && hq.decision.approved)

/** The question one of the five is, for this record. */
const questionFor = (id, hq) => (id === 'why_not_approve' && approvedHere(hq) ? A.questions.why_not_approve.question_approved : A.questions[id].question)

/** The five as a person meets them: the id and the question, from the copy. */
export function suggestions(hq) {
  return QUESTIONS.map((q) => ({ id: q.id, question: questionFor(q.id, hq) }))
}

/* Words that carry no question. Without these, "What is the weather in
 * Lisbon" matches "What changed since the last check?" on `what` and `the`,
 * and Ask answers a question nobody asked — which is worse than saying the
 * record doesn't say. */
const STOP = new Set(['a', 'an', 'the', 'is', 'are', 'was', 'were', 'be', 'been', 'do', 'does', 'did', 'to', 'of', 'in', 'on', 'at', 'for', 'it', 'its', 'this', 'that', 'these', 'those', 'and', 'or', 'can', 'could', 'should', 'would', 'i', 'me', 'my', 'we', 'you', 'your', 'has', 'have', 'had', 'about', 'any', 'anything', 'something', 'what', 'there', 'here', 'been'])

/** The words of one question, lowercased, punctuation and stop words dropped. */
const tokens = (text) => String(text || '').toLowerCase().split(/[^a-z0-9'-]+/).filter((w) => w && !STOP.has(w))

/**
 * Which of the five a typed question is, or null.
 *
 * Keyword overlap, highest first, ties going to the earlier question in the
 * list. NULL IS A REAL ANSWER: nothing is guessed at, and a question that
 * matches nothing lands on the same "doesn't say" card as a question the
 * record cannot answer, with the five offered.
 *
 * @param {string} text
 * @returns {string|null} a question id
 */
export function matchQuestion(text) {
  const asked = tokens(text)
  if (!asked.length) return null
  let best = null
  let score = 0
  for (const q of QUESTIONS) {
    /* The whole question counts too, so typing one of the five back word for
     * word always finds itself. */
    const material = new Set([...q.words, ...tokens(A.questions[q.id].question), ...tokens(A.questions[q.id].question_approved)])
    let n = 0
    for (const word of asked) if (material.has(word)) n += 1
    if (n > score) { score = n; best = q.id }
  }
  return score > 0 ? best : null
}

/* ── the answers ───────────────────────────────────────────────────────── */

const evidence = (kind, id, label, door) => ({ kind, kindName: A.kinds[kind], id, label, door })

/** The "doesn't say" card, built the same way every time. */
const cannot = (id, question, q) => ({
  id,
  question,
  answered: false,
  sentences: [],
  evidence: [],
  action: null,
  lookedFor: q.looked_for,
  missing: q.missing,
  wouldAnswer: q.would_answer,
})

const answered = (id, question, sentences, ev, action) => ({
  id,
  question,
  answered: true,
  sentences: list(sentences).filter(Boolean),
  evidence: list(ev),
  action: action || null,
  lookedFor: null,
  missing: null,
  wouldAnswer: null,
})

const plural = (q, n) => (n === 1 ? q.lead_one : fill(q.lead_many, { n }))

/**
 * What still needs evidence.
 *
 * BRIEF-COUNT-AGREES-R2 — with the readiness answer served, the lines it names
 * (`hq.readiness.tally`, the one count the top, the Change screen's list, the
 * whole-story page and the Inbox's items read): a line whose applicability was
 * never assessed still needs evidence however it reads on the coverage, and a
 * line a named person set aside is not waiting for any. What is missing that is
 * not a line (the identity, a review) is said after them, on its own. Without an
 * answer, every accepted statement that is not Proven, as before: absence of an
 * answer is not an answer.
 */
function needsEvidence(hq) {
  const Q = A.questions.needs_evidence
  const question = Q.question
  const all = list(hq.criteria)
  if (!all.length) return cannot('needs_evidence', question, Q)
  const tally = hq.readiness && !hq.readiness.absent ? hq.readiness.tally : null
  if (tally) {
    const beyond = list(tally.beyond).map((b) => b.words).filter(Boolean)
    const asideAt = new Map(list(tally.setAside).map((d) => [d.n, d]))
    if (tally.needing.length) {
      const ev = tally.needing.map((m) => evidence('criterion', m.n ?? m.id, m.text, m.n ? criterionDoor(m.n) : readinessDoor))
      const why = tally.needing.map((m) => m.reason || (all.find((c) => c.n === m.n) || {}).line)
      return answered('needs_evidence', question, [plural(Q, tally.needing.length), ...why, ...beyond], ev, { label: Q.action, door: ev[0].door })
    }
    /* BRIEF-COUNT-AGREES-R3 — what was set aside is said as the top says it (`setAsideSaid`): the lines' count, and a
     * review a person set aside named apart, so this answer never says "1" over a list that holds a review too. */
    const aside = setAsideSaid(tally)
    const lead = aside ? fill(Q.lead_none_set_aside, { aside }) : Q.lead_none
    const ev = all.map((c) => evidence('criterion', c.n, c.text, criterionDoor(c.n)))
    const lines = all.map((c) => (asideAt.has(c.n) ? [asideAt.get(c.n).text, asideAt.get(c.n).waiver].filter(Boolean).join(' — ') : c.line))
    return answered('needs_evidence', question, [lead, ...lines, ...beyond], ev, { label: Q.action, door: ev[0].door })
  }
  const waiting = all.filter((c) => c.state !== 'proven')
  const ev = (waiting.length ? waiting : all).map((c) => evidence('criterion', c.n, c.text, criterionDoor(c.n)))
  const sentences = waiting.length
    ? [plural(Q, waiting.length), ...waiting.map((c) => c.line)]
    : [Q.lead_none, ...all.map((c) => c.line)]
  return answered('needs_evidence', question, sentences, ev, { label: Q.action, door: ev[0].door })
}

/** What changed since the last check: the model's own freshness answer. */
function changedSince(hq) {
  const Q = A.questions.changed_since
  const question = Q.question
  /* STEP-7-GIT-LINE (R-2026-09-28-42): when git says the code moved since the
   * version was recorded, the answer opens with that sentence — the same one
   * the Change screen prints first — and the rest follows it. */
  const moved = hq.codeMoved ? hq.codeMoved.sentence : null
  const limit = hq.codeLimit ? list(hq.codeLimit.lines).filter((l) => l !== moved) : []
  const assessed = list(hq.criteria).filter((c) => c.freshness)
  if (!assessed.length) {
    /* A line nobody assessed cannot be said to have moved or held; the code
     * itself moving is still a fact the record can state. */
    return moved ? answered('changed_since', question, [moved, ...limit], [], null) : cannot('changed_since', question, Q)
  }
  const stale = assessed.filter((c) => c.freshness.fresh === false)
  const ev = (stale.length ? stale : assessed).map((c) => evidence('criterion', c.n, c.text, criterionDoor(c.n)))
  /* STEP-7-LIMITS (PW-F12): with nothing stale, the answer used to open with
   * `changed_since_none` — "Nothing the record watches has changed since
   * these were checked." — read as "the code has not moved", when nothing
   * committed after the last recording has been looked at. The record's limit
   * is said instead, from the field the Change screen prints; and where a
   * line did move it is said after what moved, because what the record
   * watched is not everything that may have. */
  const sentences = stale.length
    ? [moved, hq.changedSince.text, ...stale.map((c) => c.freshness.sentence), ...limit]
    : [moved, ...limit]
  return answered('changed_since', question, sentences, ev, { label: Q.action, door: ev[0].door })
}

/** Why approval is not available: the one blocker, then what else it lists. */
function whyNotApprove(hq) {
  const Q = A.questions.why_not_approve
  const question = Q.question
  const r = hq.readiness
  if (approvedHere(hq)) return approvedAnswer(hq)
  if (!r || r.absent || !r.sentence) return cannot('why_not_approve', question, Q)
  const ev = []
  if (r.ready) {
    const sentences = [r.sentence]
    for (const rest of list(r.restsOn)) sentences.push(rest.line)
    /* RUN-LINK-TRUTH-R6 — Ask says what the screen says on a ready answer:
     * what was set aside (T-5-3, as served), then that approval is available
     * in the served `available` pair, never a blocked line. */
    for (const w of list(r.waivedLines)) sentences.push([w.text, w.waiver].filter(Boolean).join(' — '))
    if (r.available) sentences.push(r.available.reason + ' ' + r.available.enabler)
    ev.push(evidence('readiness', 'readiness', r.heading, readinessDoor))
    if (hq.decision && hq.decision.headline) sentences.push(hq.decision.headline)
    return answered('why_not_approve', question, sentences, ev, { label: Q.action_ready, door: decisionDoor })
  }
  /* RUN-LINK-TRUTH-R5 — not ready: the one blocker first, word for word as
   * Brief, the Approve stage, the Inspector and What we checked say it, then
   * each other missing obligation with its own pair, in the engine's order.
   * Ask builds no reason of its own; `offLines` is `blockerOf`'s. */
  const sentences = [...list(r.offLines)]
  for (const m of list(r.missing)) {
    ev.push(evidence('readiness', m.id || m.text, m.text, m.n ? criterionDoor(m.n) : readinessDoor))
  }
  for (const w of list(r.waivedLines)) sentences.push([w.text, w.waiver].filter(Boolean).join(' — '))
  if (!ev.length) ev.push(evidence('readiness', 'readiness', r.heading, readinessDoor))
  return answered('why_not_approve', question, sentences, ev, { label: Q.action, door: readinessDoor })
}

/**
 * The same question, on a version that stands approved: the decision in the
 * words the screen shows, that nothing is left to approve, who approved it
 * against which list, what it rests on, what it was given under, what an
 * approval does not do, and how to take it back. Every sentence is a field
 * the model derived or a copy string.
 */
function approvedAnswer(hq) {
  const Q = A.questions.why_not_approve
  const d = hq.decision
  const r = hq.readiness
  const sentences = [d.headline, Q.approved_already, d.by]
  const rests = list(r.restsOn).map((x) => x.line).filter(Boolean)
  if (rests.length) sentences.push(fill(Q.approved_rests_on, { version: d.version || '' }), ...rests)
  const exceptions = list(d.exceptions).map((e) => e.line).filter(Boolean)
  if (exceptions.length) sentences.push(d.exceptionsHeading, ...exceptions)
  sentences.push(d.sentence, Q.approved_withdraw)
  const ev = [evidence('decision', 'decision', d.headline, decisionDoor)]
  return answered('why_not_approve', Q.question_approved, sentences, ev, { label: Q.action_ready, door: decisionDoor })
}

/** What was said about this work, and what the record holds beside it. */
function saidVsRecord(hq) {
  const Q = A.questions.said_vs_record
  const question = Q.question
  const pairs = list(hq.packet && hq.packet.said)
  if (!pairs.length) return cannot('said_vs_record', question, Q)
  const sentences = [plural(Q, pairs.length)]
  const ev = []
  pairs.forEach((p, i) => {
    if (p.said) sentences.push(Q.said_heading + ': ' + p.said)
    if (p.record) sentences.push(Q.record_heading + ': ' + p.record)
    ev.push(evidence('finding', i + 1, str(p.said) || str(p.record), findingsDoor))
  })
  return answered('said_vs_record', question, sentences, ev, { label: Q.action, door: findingsDoor })
}

/** What the record does not hold, said as a list rather than as a blank. */
function notRecorded(hq) {
  const Q = A.questions.not_recorded
  const question = Q.question
  const all = list(hq.criteria)
  if (!all.length) return cannot('not_recorded', question, Q)
  const gaps = list(hq.packet && hq.packet.notRecorded)
  const sentences = gaps.length ? [plural(Q, gaps.length), ...gaps] : [Q.lead_none]
  const ev = gaps.length
    ? gaps.map((g, i) => evidence('gap', i + 1, g, criterionDoor(all[0].n)))
    : all.map((c) => evidence('criterion', c.n, c.text, criterionDoor(c.n)))
  return answered('not_recorded', question, sentences, ev, { label: Q.action, door: ev[0].door })
}

const ANSWERS = Object.freeze({
  needs_evidence: needsEvidence,
  changed_since: changedSince,
  why_not_approve: whyNotApprove,
  said_vs_record: saidVsRecord,
  not_recorded: notRecorded,
})

/**
 * One of the five, answered from the one derived view-model.
 *
 * @param {string} id one of the five question ids
 * @param {object|null} hq what `deriveChangeModel` returned
 * @returns {object|null} the answer, or null for an id that is not one of the five
 */
export function answer(id, hq) {
  const fn = ANSWERS[id]
  if (!fn) return null
  if (!hq) return null
  return fn(hq)
}

/**
 * What a person typed, answered — or the "doesn't say" card with the five
 * offered, which is the only other thing that can happen.
 *
 * @param {string} text
 * @param {object|null} hq
 * @returns {object}
 */
export function ask(text, hq) {
  const id = matchQuestion(text)
  const found = id ? answer(id, hq) : null
  if (found) return { ...found, asked: str(text) }
  return {
    id: null,
    question: str(text),
    asked: str(text),
    answered: false,
    sentences: [],
    evidence: [],
    action: null,
    lookedFor: A.unmatched.looked_for,
    missing: A.unmatched.missing,
    wouldAnswer: A.unmatched.would_answer,
  }
}

/** The scope chip: this Change, by its own recorded title. */
export function scopeChip(hq) {
  const title = hq ? str(hq.title) : null
  return { text: title ? fill(A.scope.chip, { title }) : A.scope.chip_untitled, title }
}

export default answer
