/* PATH-UI-2-R3 — synthetic served documents for one Change with N accepted
 * lines and one passing run that no line is linked to.
 *
 * Shared by the jsdom test (`src/workspace/packet-link-door-r3.test.js`) and
 * the real-browser geometry check (`door-geometry.mjs`), so what the unit test
 * says a door must do and what Chrome measures are about the same documents.
 *
 * Every value is synthetic. The ids use the `cccccccc-` prefix the gate's
 * private-content scan recognises; nothing here reads a machine.
 */

export const CHANGE_ID = 'cccccccc-0000-4000-8000-00000000c0b3'

/** Twelve lines with awkward characters in them (a door label prints the line, so a line is also a hostile string). */
const AWKWARD = [
  'When the cart is empty, the total shows zero.',
  'A price of $5 & a coupon of 100% takes <b>bold</b> off the total.',
  'The total never shows "NaN" or {line} or $& or $1 in it.',
  'Refunds show a negative total: -$3.50 (see \\n and \\\\ handling).',
  'Emoji \u{1F6D2} and RTL עברית show correctly in the cart title.',
  'A cart with one item shows that price only.',
  "Rounding uses the store's rule for half cents.",
  'The `total` element renders <script>alert(1)</script> as text.',
  'Two coupons cannot be stacked on one cart.',
  'Line ten: the total survives a page reload of the cart.',
  'Line eleven: an item removed updates the total.',
  'Line twelve: the total is shown in the shop currency (€ / ¥).',
]

/** N accepted lines. Two is the shape the walk fixture has; more repeat the pattern with numbered, distinct text. */
export function linesOf(n) {
  if (n === 2) return ['When the cart is empty, the total shows zero.', 'The total shows the sum of the prices.']
  return Array.from({ length: n }, (_, i) => (i < AWKWARD.length ? AWKWARD[i] : `Line ${i + 1}: the cart keeps behaving as line ${i + 1} says it should.`))
}

const change = (id, criteria) => ({
  id,
  project_id: 'p1',
  opened_at: '2026-09-22T08:00:00Z',
  request: { title: 'Empty cart total' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1, criteria, allowed_scope: ['src/'], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
  examination: 'none_yet',
  decisions: [],
})

const IDENTITY = {
  id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '',
  outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null,
  evidence_name: null, linked_by: null, waiver: null, waivable: false,
}
const unlinked = (n, text) => ({
  id: 'criterion:' + n, kind: 'criterion', criterion_index: n, text,
  outcome: 'needs_evidence', outcome_slug: 'needs_evidence',
  unsatisfied: 'unlinked_run', unsatisfied_slug: 'readiness_unlinked_run',
  evidence_name: null, linked_by: null, waiver: null, waivable: true,
})
const CANDIDATE = {
  kind: 'check_run', evidence_id: 'eb_test', session_id: 's1', name: 'cargo test',
  observed: 'proven', bears: true, execution: 'completed', execution_slug: 'execution_completed',
}
const coverageOf = (lines) => ({
  scope_version: 1,
  link_rows: 0,
  execution_sessions: ['s1'],
  criteria: lines.map((text, i) => ({ index: i + 1, text, state: 'none', items: [], removed: [] })),
  candidates: [CANDIDATE],
  executions: [],
})
const readinessOf = (id, lines) => ({
  schema_version: 1, change_id: id, ready: false, with_exceptions: false, scope_version: 1,
  target_version: 'e71c5b0aaaa', headline_slug: 'readiness_needs_evidence_one',
  obligations: [IDENTITY, ...lines.map((t, i) => unlinked(i + 1, t))],
  rests_on: [], missing: lines.map((_, i) => 'criterion:' + (i + 1)), waived: [], not_applicable: [],
})

/** One linked check that FAILED (the failing-check Change of `primary-action.test.js`, trimmed): what makes the Inspector's one primary "Look at what broke". */
const BROKEN_ITEM = {
  link_id: 'cce_synthetic_0001', kind: 'check_run', evidence_id: 'evb_synthetic_0001', session_id: 'cli:check:2026-01-02T10:05:00Z',
  basis: 'named', basis_slug: 'basis_named', name: 'cargo test --test=duplicate_retry', observed: 'not_proven',
  bears: true, applies: true, applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z',
  execution: 'completed', execution_slug: 'execution_completed', failure_line: 'left: "job_0002" right: "job_0003"',
  evidence_class: 'seeded', evidence_class_slug: 'evidence_class_seeded',
}
/** The same Change with one line's linked check failed and no readiness answer: the state whose primary is a door to the line that broke. */
const brokenCoverage = (id, lines, broke) => ({
  schema_version: 1, change_id: id, scope_version: 1, link_rows: 1, executions: [],
  criteria: lines.map((text, i) => (i + 1 === broke
    ? { index: i + 1, text, state: 'evidence', state_slug: 'coverage_evidence', items: [BROKEN_ITEM], removed: [] }
    : { index: i + 1, text, state: 'none', state_slug: 'coverage_no_test_ran', items: [], removed: [] })),
})

/**
 * What each read answers, by address. Anything not listed is a 404, which the
 * screens say aloud and go on without.
 *
 * @param {string[]} lines the accepted lines
 * @param {string} [id]
 * @param {{ broke?: number }} [options] `broke`: the one line whose linked check failed (and no readiness answer)
 */
export function routes(lines, id = CHANGE_ID, options = {}) {
  const broke = Number.isInteger(options.broke) ? options.broke : null
  return {
    ['/api/changes/' + id]: { change: change(id, lines) },
    ['/api/changes/' + id + '/proposals']: { proposals: null },
    ['/api/changes/' + id + '/coverage']: broke
      ? { coverage: brokenCoverage(id, lines, broke), readiness: null }
      : { coverage: coverageOf(lines), readiness: readinessOf(id, lines) },
    ['/api/changes/' + id + '/inputs']: {},
    ['/api/changes/' + id + '/applicability']: { items: [] },
  }
}
