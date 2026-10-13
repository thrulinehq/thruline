/* PATH-UI-2-R2, BROKEN 1 — the Inbox never files a Change that still needs a
 * check under "nothing is waiting".
 *
 * Found by the round-1 adversary at f4b81633 (launch gates/adversary-CLOUD-
 * PATH-UI-2/TRANSCRIPT-f4b81633.md). Construction: a Change with two accepted
 * lines; line 1 set aside by a named person under the release policy; line 2's
 * passing run linked to it by a person; the code committed after the run.
 * The served readiness is the same at the base (cf8ca110) and at f4b81633:
 * not ready, `criterion:2` missing because whether it still applies was never
 * assessed, `criterion:1` waived. The Change screen says "can't be approved
 * yet". At the base the Inbox said "1 Change needs your attention"; at
 * f4b81633 it said "Nothing needs your attention right now", because
 * TELL-DRAFT-WAIVED credited line 1 as covered, which took the only reason
 * left standing (`missing_evidence`) to zero, and `awaiting_approval` does not
 * fire on an answer that is not ready.
 *
 * The rule these tests hold: the Inbox reads the SERVED readiness answer, the
 * field the Change screen reads (`hq.readiness`). A Change whose answer is not
 * ready and names what is missing is "Needs a check", however many of its
 * lines the coverage calls proven; a set-aside line never stands in for
 * another line; ready with exceptions stays "Waiting on you".
 *
 * THE 2x2 (code x documents), measured with the adversary's own script
 * (r2/inbox-repro.mjs: attentionOf over deriveChangeModel) on the four
 * documents the head server and the base server each served for that Change:
 *
 *                       head server's documents          base server's documents
 *   cf8ca110 (base)     needsPerson=true  items=1        needsPerson=true  items=1
 *                       [missing_evidence=1]             [missing_evidence=1]
 *   f4b81633 (head)     needsPerson=false items=0        needsPerson=false items=0
 *                       []                               []
 *
 * The rows are the code, the columns the record: the same silence appears in
 * both columns of the f4b81633 row, so it is the code that moved, not the
 * record. The first two tests hold the current code to the base row on both
 * columns; they are red at f4b81633 and green at the fix.
 *
 * `inbox-waived-unassessed.fixture.json` holds both document sets, trimmed to
 * the fields the model reads and scrubbed of this machine's paths. Every other
 * fixture below is synthetic.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-inbox-readiness-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import { assembleInbox } from './inbox.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { attentionOf } from './attention.js';
  import { inboxItem, inboxSections, sectionOf } from './inbox-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { assembleInbox, deriveChangeModel, attentionOf, inboxItem, inboxSections, sectionOf, HQ_COPY };
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'inbox-readiness.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const FIXTURE = JSON.parse(readFileSync(join(here, 'inbox-waived-unassessed.fixture.json'), 'utf8'))
const clone = (v) => JSON.parse(JSON.stringify(v))

/** What the base measured for the adversary's Change, on either server's documents. */
const BASE_ANSWER = Object.freeze({ needsPerson: true, items: 1, reasons: ['missing_evidence=1'] })
/* The top sentence for that Change. BRIEF-COUNT-AGREES moved it (D-050): it was
 * "1 of 2 things that count as done have evidence. 1 still needs it.", which
 * counts the SET-ASIDE line as the one that still needs evidence and the
 * NEVER-ASSESSED line as the one that has it — the same number, about the
 * opposite lines. The served readiness names one line as missing, and the top
 * now says that: one thing, claiming nothing about which lines have evidence. */
const TOP_WORDS = 'Accepted: 2 things count as done. 1 thing still needs evidence.'

/** The documents of one server, as `readChangeDocuments` hands them to the Inbox. */
function docsOf(set) {
  const cov = set.coverage
  return {
    change: set.change.change,
    coverage: { ...cov.coverage, readiness: cov.readiness, review_findings: cov.review_findings || null, run_checks: null },
    applicability: set.applicability,
    proposals: set.proposals.proposals,
    work: [],
  }
}

/** The Inbox as the app assembles it from those documents (`inbox.jsx` `assembleInbox`). */
const inboxOfDocs = (docs) => M.assembleInbox([docs.change], new Map([[docs.change.id, docs]]))
const answerOf = (hq, docs) => {
  const a = M.attentionOf({ change: docs.change, work: [], hq, applicability: [] })
  return { needsPerson: a.needsPerson, items: a.items, reasons: a.reasons.map((r) => r.id + '=' + r.count) }
}
const sectionCount = (inbox, id) => M.inboxSections(inbox.items, inbox.settled).find((s) => s.id === id).count

/* ── the 2x2: the record is not what moved ─────────────────────────────── */

for (const server of ['head', 'base']) {
  test(`the adversary's Change, as the ${server} server served it: the served answer is what it was (not ready, line 2 never assessed, line 1 waived)`, () => {
    const docs = docsOf(FIXTURE[server])
    const hq = M.deriveChangeModel(docs.change, docs.coverage, docs.applicability, docs.proposals, docs.work)
    assert.equal(hq.readiness.ready, false)
    assert.deepEqual(hq.readiness.missing.map((m) => m.id + ':' + m.slug), ['criterion:2:readiness_never_assessed'])
    assert.deepEqual(hq.readiness.waivedLines.map((w) => w.id), ['criterion:1'])
    assert.equal(hq.inboxRow.reason, TOP_WORDS, 'and the top sentence says what the readiness answer says: one thing still needs evidence')
  })

  test(`the adversary's Change, as the ${server} server served it: the Inbox says what it said at the base, under "Needs a check"`, () => {
    const docs = docsOf(FIXTURE[server])
    const hq = M.deriveChangeModel(docs.change, docs.coverage, docs.applicability, docs.proposals, docs.work)
    assert.deepEqual(answerOf(hq, docs), BASE_ANSWER, 'the attention the base measured, not "nothing is waiting"')
    /* And the Inbox as the app builds it from the same documents. */
    const inbox = inboxOfDocs(docs)
    assert.equal(inbox.items.length, 1, 'the Change is listed')
    assert.equal(inbox.settled, 0, 'it is not counted among the Changes that need nothing')
    const item = inbox.items[0]
    assert.equal(item.reason.id, 'missing_evidence')
    assert.equal(M.sectionOf(item), 'check')
    assert.equal(sectionCount(inbox, 'check'), 1)
    assert.equal(sectionCount(inbox, 'you'), 0, 'it is not waiting on an approver: nothing is ready to approve')
    assert.equal(item.attention.items, 1)
    assert.equal(item.reasonText, TOP_WORDS, 'in the top sentence the Change screen prints')
    assert.equal(item.line.action, M.HQ_COPY.change.inbox.reasons.missing_evidence.action)
    /* The Change screen's own count is the row's: one derivation. */
    assert.deepEqual(hq.attention.reasons.map((r) => r.id + '=' + r.count), BASE_ANSWER.reasons)
    assert.equal(hq.attention.items, item.attention.items)
  })
}

/* ── what the rule is, on synthetic Changes ────────────────────────────── */

const RUN_NAME = 'cargo test'
const LINE_ONE = 'When the cart is empty, the total shows zero.'
const LINE_TWO = 'When the cart has two items, the total shows their sum.'
const LINE_THREE = 'A cart with a coupon takes it off the total.'
const CHANGE_ID = 'cccccccc-0000-4000-8000-00000000c0a1'

const change = (criteria) => ({
  id: CHANGE_ID,
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1, criteria, allowed_scope: ['src/'], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: null,
})

const IDENTITY = {
  id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: null, linked_by: null, waiver: null, waivable: false,
}
const UNCONFIRMED = {
  ...IDENTITY, outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'identity_unconfirmed', unsatisfied_slug: 'readiness_identity_unconfirmed',
}
const satisfied = (n, text) => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: RUN_NAME, linked_by: 'walker', waiver: null, waivable: true,
})
const setAside = (n, text, outcome = 'waived') => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome, outcome_slug: outcome, unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked',
  evidence_name: null, linked_by: null, waivable: true,
  waiver: { actor: 'sam', authority: 'the release policy', rationale: 'The sandbox is down.' },
})
/** A line the record shows proven and the engine still lists: whether it still applies was never assessed. */
const unassessed = (n, text, slug = 'never_assessed') => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: slug, unsatisfied_slug: 'readiness_' + slug,
  evidence_name: null, linked_by: null, waiver: null, waivable: true,
})
const nothing = (n, text) => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked',
  evidence_name: null, linked_by: null, waiver: null, waivable: true,
})

const provenItem = (n) => ({
  link_id: 'l' + n, evidence_id: 'eb_test', session_id: 's1', basis: 'manual', basis_slug: 'link_basis_manual',
  actor: 'walker', asserted_at: '2026-09-22T11:00:00Z', kind: 'check_run', name: RUN_NAME,
  observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies',
  fresh: true, fresh_slug: 'applicability_still_applies', execution: 'completed', execution_slug: 'execution_completed',
})

/**
 * `proven` names the lines the COVERAGE shows proven (a person linked a passing
 * run to them); `obligations` is what the engine's readiness says about each.
 * The two are separate on purpose: the whole defect is that they can disagree.
 */
function coverage(lines, proven, obligations, { ready = false, withExceptions = false, slug = null, identity = IDENTITY } = {}) {
  return {
    scope_version: 1,
    link_rows: proven.length,
    execution_sessions: ['s1'],
    criteria: lines.map((text, i) => (proven.includes(i + 1)
      ? { index: i + 1, text, state: 'evidence', removed: [], items: [provenItem(i + 1)] }
      : { index: i + 1, text, state: 'none', items: [], removed: [] })),
    candidates: [{ kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: RUN_NAME, observed: 'proven', bears: true, execution: 'completed', execution_slug: 'execution_completed' }],
    executions: [],
    readiness: {
      schema_version: 1, change_id: CHANGE_ID, ready, with_exceptions: withExceptions, scope_version: 1,
      target_version: 'e71c5b0aaaa',
      headline_slug: slug || (ready ? (withExceptions ? 'readiness_ready_with_exceptions' : 'readiness_ready') : 'readiness_needs_evidence_one'),
      obligations: [identity, ...obligations],
      rests_on: obligations.filter((o) => o.outcome === 'satisfied').map((o) => ({ obligation_id: o.id, text: o.text, evidence_name: o.evidence_name, linked_by: o.linked_by })),
      missing: [identity, ...obligations].filter((o) => o.outcome === 'needs_evidence').map((o) => o.id),
      waived: obligations.filter((o) => o.outcome === 'waived').map((o) => o.id),
      not_applicable: obligations.filter((o) => o.outcome === 'not_applicable').map((o) => o.id),
    },
  }
}

const lines2 = [LINE_ONE, LINE_TWO]
const lines3 = [LINE_ONE, LINE_TWO, LINE_THREE]
const inboxOf = (doc, criteria) => {
  const c = change(criteria)
  return M.inboxItem({ change: c, proposals: null, work: [], hq: M.deriveChangeModel(c, doc, null, null, []) })
}
const hqOf = (doc, criteria) => M.deriveChangeModel(change(criteria), doc, null, null, [])

test('a set-aside line beside a line that is proven but never assessed: "Needs a check", one thing (synthetic form of the adversary\'s Change)', () => {
  const doc = coverage(lines2, [2], [setAside(1, LINE_ONE), unassessed(2, LINE_TWO)])
  const item = inboxOf(doc, lines2)
  assert.ok(item, 'listed')
  assert.equal(item.reason.id, 'missing_evidence')
  assert.equal(M.sectionOf(item), 'check')
  assert.equal(item.attention.items, 1)
  assert.equal(item.reasonText, item.hq.inboxRow.reason, 'the words are the Change screen\'s own')
})

test('a set-aside line never counts as evidence for another line: waived, never assessed, nothing yet = two things, not one', () => {
  const doc = coverage(lines3, [2], [setAside(1, LINE_ONE), unassessed(2, LINE_TWO), nothing(3, LINE_THREE)])
  const hq = hqOf(doc, lines3)
  assert.deepEqual(hq.readiness.missing.map((m) => m.n), [2, 3], 'the served answer names both')
  const item = inboxOf(doc, lines3)
  assert.equal(item.reason.id, 'missing_evidence')
  assert.equal(item.reason.count, 2, 'the count is the answer\'s: the waived line offsets neither')
  assert.equal(item.attention.items, 2)
  assert.equal(hq.attention.items, 2, 'and the Change screen counts the same two')
})

test('a Change whose served answer is not ready and names what is missing is never "nothing is waiting", whatever the coverage calls proven', () => {
  for (const slug of ['never_assessed', 'could_not_assess', 'stale', 'other_version', 'unlinked_run']) {
    const doc = coverage([LINE_ONE], [1], [unassessed(1, LINE_ONE, slug)])
    const item = inboxOf(doc, [LINE_ONE])
    assert.ok(item, `${slug}: listed`)
    assert.equal(item.reason.id, 'missing_evidence', slug)
    assert.equal(M.sectionOf(item), 'check', slug)
    assert.equal(item.attention.items, 1, slug)
  }
})

test('a not-ready answer that names something other than a line (the work is not confirmed) is a thing to check too', () => {
  const doc = coverage(lines2, [1, 2], [satisfied(1, LINE_ONE), satisfied(2, LINE_TWO)], { identity: UNCONFIRMED })
  const item = inboxOf(doc, lines2)
  assert.ok(item, 'listed')
  assert.equal(item.reason.id, 'missing_evidence')
  assert.equal(M.sectionOf(item), 'check')
  assert.equal(item.attention.items, 1, 'one thing: the engine names one')
})

test('ready with exceptions stays "Waiting on you": TELL-DRAFT-WAIVED holds', () => {
  const doc = coverage(lines2, [2], [setAside(1, LINE_ONE), satisfied(2, LINE_TWO)], { ready: true, withExceptions: true })
  const item = inboxOf(doc, lines2)
  assert.ok(item)
  assert.equal(item.reason.id, 'awaiting_approval')
  assert.equal(M.sectionOf(item), 'you')
  assert.equal(item.attention.items, 1)
  assert.deepEqual(item.also, [], 'no second reason on the row')
  assert.equal(item.reasonText, item.hq.readiness.available.reason, 'in the served readiness\'s own words')
})

test('a Change whose answer is ready and whose every line is proven is unchanged: waiting only on its approver', () => {
  const doc = coverage([LINE_ONE], [1], [satisfied(1, LINE_ONE)], { ready: true })
  const item = inboxOf(doc, [LINE_ONE])
  assert.equal(item.reason.id, 'awaiting_approval')
  assert.equal(M.sectionOf(item), 'you')
})

test('a Change that really needs a check keeps its count and its words: a line with nothing behind it, and a waiver beside it', () => {
  const doc = coverage(lines3, [1], [satisfied(1, LINE_ONE), setAside(2, LINE_TWO), nothing(3, LINE_THREE)])
  const item = inboxOf(doc, lines3)
  assert.equal(item.reason.id, 'missing_evidence')
  assert.equal(item.reason.count, 1, 'only line 3 is without evidence')
  assert.equal(item.reasonText, item.hq.inboxRow.reason)
  assert.equal(item.line.template, M.HQ_COPY.change.inbox.reasons.missing_evidence.line_one)
})

test('no readiness answer served: the Inbox counts as it always did, and invents nothing from an answer it was not given', () => {
  const c = change(lines2)
  const doc = coverage(lines2, [1], [satisfied(1, LINE_ONE), nothing(2, LINE_TWO)])
  delete doc.readiness
  const hq = M.deriveChangeModel(c, doc, null, null, [])
  assert.equal(hq.readiness.absent, true)
  const item = M.inboxItem({ change: c, proposals: null, work: [], hq })
  assert.equal(item.reason.id, 'missing_evidence')
  assert.equal(item.reason.count, 1, 'line 2 has nothing behind it, as the coverage says')
  /* And with every line proven and no answer, nothing invents a reason. */
  const proven = coverage(lines2, [1, 2], [satisfied(1, LINE_ONE), satisfied(2, LINE_TWO)])
  delete proven.readiness
  assert.equal(M.inboxItem({ change: c, proposals: null, work: [], hq: M.deriveChangeModel(c, proven, null, null, []) }), null)
})

test('an answer that is not ready but names nothing missing (no version written down) invents no reason', () => {
  const c = change([LINE_ONE])
  c.implementation = null
  const doc = coverage([LINE_ONE], [1], [satisfied(1, LINE_ONE)], { ready: false })
  doc.readiness.target_version = null
  const hq = M.deriveChangeModel(c, doc, null, null, [])
  assert.equal(hq.readiness.ready, false)
  assert.equal(hq.readiness.missing.length, 0)
  assert.equal(M.inboxItem({ change: c, proposals: null, work: [], hq }), null, 'nothing named, nothing counted: as at the base')
})

test('a Change approved on the version it stands at, with a ready answer, still needs nothing', () => {
  const c = change([LINE_ONE])
  c.decisions = {
    target_revision: 'e71c5b0aaaa', scope_version: 1,
    standing: { id: 'cd_1', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T12:00:00Z', scope_version: 1, exceptions: [] },
    history: [{ id: 'cd_1', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T12:00:00Z', scope_version: 1, exceptions: [] }],
  }
  const doc = coverage([LINE_ONE], [1], [satisfied(1, LINE_ONE)], { ready: true })
  assert.equal(M.inboxItem({ change: c, proposals: null, work: [], hq: M.deriveChangeModel(c, doc, null, null, []) }), null)
})

test('the document sets are not edited by the reading: the same documents twice give the same Inbox', () => {
  const a = docsOf(FIXTURE.head)
  const before = JSON.stringify(a)
  const one = inboxOfDocs(a)
  const two = inboxOfDocs(clone(a))
  assert.equal(JSON.stringify(a), before, 'nothing was written into the served documents')
  assert.deepEqual(one.items.map((i) => [i.id, i.reason.id, i.reason.count]), two.items.map((i) => [i.id, i.reason.id, i.reason.count]))
})

/* ── self-attack: hostile and odd answers ─────────────────────────────── */

const hqWith = (doc, criteria, edit) => {
  const hq = M.deriveChangeModel(change(criteria), doc, null, null, [])
  if (edit) edit(hq)
  return hq
}
const attentionWith = (hq, criteria) => M.attentionOf({ change: change(criteria), work: [], hq, applicability: [] })

test('the same line named twice, and the same other thing named twice, are each one thing', () => {
  const doc = coverage(lines2, [1, 2], [satisfied(1, LINE_ONE), unassessed(2, LINE_TWO)], { identity: UNCONFIRMED })
  const hq = hqWith(doc, lines2, (h) => { h.readiness.missing = [...h.readiness.missing, ...h.readiness.missing] })
  assert.equal(hq.readiness.missing.length, 4)
  const a = attentionWith(hq, lines2)
  assert.equal(a.items, 2, 'line 2 and the unconfirmed work: two things, not four')
})

/* BRIEF-COUNT-AGREES-R2 changed how this hostile answer is built (D-050), not what it expects. It used to edit `readiness.missing` on the
 * derived model after the fact; the count of what still needs evidence is `readiness.tally` now, derived once from the served obligations,
 * so the hostile obligation is served — in the document, where the engine would put it — and the count is still one more thing. */
test('an obligation naming a line the Change does not have is one more thing named, never a hole in the count', () => {
  const doc = coverage([LINE_ONE], [1], [unassessed(1, LINE_ONE), nothing(9, 'A line this Change does not have.')])
  const hq = hqWith(doc, [LINE_ONE])
  assert.equal(attentionWith(hq, [LINE_ONE]).items, 2)
})

test('odd entries in the answer\'s missing list are counted, never thrown at', () => {
  const doc = coverage([LINE_ONE], [1], [unassessed(1, LINE_ONE)])
  for (const odd of [null, undefined, 3, 'criterion:1', {}, { n: 'x' }, { n: 0 }, { n: -1 }, { n: 1.5 }]) {
    const hq = hqWith(doc, [LINE_ONE], (h) => { h.readiness.missing = [odd] })
    const a = attentionWith(hq, [LINE_ONE])
    assert.ok(a.items >= 0 && a.needsPerson === (a.reasons.length > 0), JSON.stringify(odd))
  }
})

test('an answer that says ready is never read for what it lists as missing: ready is ready', () => {
  const doc = coverage([LINE_ONE], [1], [satisfied(1, LINE_ONE)], { ready: true })
  const hq = hqWith(doc, [LINE_ONE], (h) => { h.readiness.missing = [{ id: 'criterion:1', n: 1 }] })
  assert.deepEqual(attentionWith(hq, [LINE_ONE]).reasons.map((r) => r.id), ['awaiting_approval'])
})

test('an answer that is absent is not read at all: a list of missing things beside no answer counts nothing', () => {
  const doc = coverage([LINE_ONE], [1], [satisfied(1, LINE_ONE)])
  delete doc.readiness
  const hq = hqWith(doc, [LINE_ONE], (h) => { h.readiness = { ...h.readiness, absent: true, missing: [{ id: 'criterion:1', n: 1 }] } })
  assert.equal(attentionWith(hq, [LINE_ONE]).reasons.length, 0)
})

test('with no derived model at all, the Inbox counts as it did before there was one', () => {
  const a = M.attentionOf({ change: change(lines2), work: [], applicability: [], evidence: [{ criterion: 1, state: 'proven' }] })
  assert.deepEqual(a.reasons.map((r) => r.id + '=' + r.count), ['missing_evidence=1'])
})

test('a Change approved on a version and then not ready (the record moved) is a thing to check, from the answer, as it was from the coverage', () => {
  const c = change([LINE_ONE])
  c.decisions = {
    target_revision: 'e71c5b0aaaa', scope_version: 1,
    standing: { id: 'cd_1', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T12:00:00Z', scope_version: 1, exceptions: [] },
    history: [{ id: 'cd_1', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T12:00:00Z', scope_version: 1, exceptions: [] }],
  }
  const doc = coverage([LINE_ONE], [1], [unassessed(1, LINE_ONE, 'stale')])
  const item = M.inboxItem({ change: c, proposals: null, work: [], hq: M.deriveChangeModel(c, doc, null, null, []) })
  assert.ok(item)
  assert.equal(item.reason.id, 'missing_evidence')
})

test('the Inbox as a whole: the adversary\'s Change is counted under "Needs a check" beside a Change that waits on its approver and one that needs nothing', () => {
  const a = docsOf(FIXTURE.head)
  const second = { ...docsOf(FIXTURE.base), change: { ...docsOf(FIXTURE.base).change, id: 'cccccccc-0000-4000-8000-00000000c0a2', opened_at: '2026-09-23T08:00:00Z' } }
  const ready = change([LINE_ONE])
  ready.id = 'cccccccc-0000-4000-8000-00000000c0a3'
  ready.opened_at = '2026-09-24T08:00:00Z'
  const readyDoc = coverage([LINE_ONE], [1], [satisfied(1, LINE_ONE)], { ready: true })
  const approved = change([LINE_ONE])
  approved.id = 'cccccccc-0000-4000-8000-00000000c0a4'
  approved.opened_at = '2026-09-25T08:00:00Z'
  approved.decisions = {
    target_revision: 'e71c5b0aaaa', scope_version: 1,
    standing: { id: 'cd_1', act: 'approved', target_revision: 'e71c5b0aaaa', actor: 'sam', decided_at: '2026-09-22T12:00:00Z', scope_version: 1, exceptions: [] },
    history: [],
  }
  const docs = new Map([
    [a.change.id, a],
    [second.change.id, second],
    [ready.id, { change: ready, coverage: readyDoc, applicability: null, proposals: null, work: [] }],
    [approved.id, { change: approved, coverage: readyDoc, applicability: null, proposals: null, work: [] }],
  ])
  const inbox = M.assembleInbox([a.change, second.change, ready, approved], docs)
  const sections = M.inboxSections(inbox.items, inbox.settled)
  assert.equal(sections.find((s) => s.id === 'check').count, 2, 'both copies of the adversary\'s Change')
  assert.equal(sections.find((s) => s.id === 'you').count, 1, 'the ready one')
  assert.equal(sections.find((s) => s.id === 'nothing').count, 1, 'the approved one')
  assert.equal(inbox.items.length, 3)
})
