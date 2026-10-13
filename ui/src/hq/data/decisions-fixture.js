/* Decisions, as `GET /api/hq/projects/<id>/decisions` serves them
 * (docs/decisions/API.md, schema_version 1, with the C35 additions), for tests
 * and the design gallery only. Every key and nullability follows API.md's
 * shared shapes exactly (`decisions.test.js` holds the key sets); no real
 * record.
 *
 * `DECISIONS_LIST`: one project at version wt-3c3ca28a1b2c3d4e with five
 * decisions, one of each state the screens draw: broken (blocks), holds
 * (warns), unverifiable (blocks), stale (blocks), and a proposal with no
 * verdict. The approval gate is active and the Mac's native confirmation is
 * available. Each decision and verdict carries the engine's own
 * `blocks_approval`, as the owner computes it for the exact version.
 *
 * `DECISIONS_CASES`: the gate-rule cases, one decision each (a warning that is
 * broken, a valid waiver, an unverifiable result under `on_unverifiable:
 * warn`, a stale result whose current check is missing, an accepted decision
 * with no result at all).
 *
 * `DECISIONS_DORMANT`: the main decisions under a gate that is not active. */

const VERSION = Object.freeze({
  version_id: 'wt-3c3ca28a1b2c3d4e',
  kind: 'commit',
  tree_hash: '9f1c0d7e5b3a28461c0de5f00dfeedbeadcafe01',
  commit_oid: '3c3ca28a1b2c3d4e5f60718293a4b5c6d7e8f901',
  captured_at: '2026-10-07T09:12:00Z',
})

const OLDER = Object.freeze({
  version_id: 'wt-0aa1bb2cc3dd4ee5',
  kind: 'commit',
  tree_hash: '1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d',
  commit_oid: '0aa1bb2cc3dd4ee5ff60718293a4b5c6d7e8f902',
  captured_at: '2026-10-06T18:40:00Z',
})

const source = (id, line, span, at) => ({
  source_kind: 'work_prompt',
  event_id: id,
  file_anchor: null,
  raw_file: '~/.claude/projects/-work-app/cccccccc-0000-4000-8000-00000000d001.jsonl',
  raw_line: line,
  span_start: 0,
  span_end: span.length,
  span,
  actor: 'you',
  recorded_at: at,
})

const surface = (s, state, places, found, reason = null) => ({ surface: s, state, checked_places: places, finding_count: found, reason })

const finding = (over) => Object.assign({
  surface: 'ui_copy',
  span: null,
  message: 'a home-folder path appears in user-facing copy',
  edit_id: null,
  session_id: 's1',
  thread_id: null, // null until HQ supplies a proved thread link (API.md Finding)
  introduced_at: null,
  present_since_version: 'wt-3c3ca28a1b2c3d4e',
  screen: null,
  difference_pct: null,
  diff_image_path: null,
  reference_hash: null,
}, over)

const verdict = (over) => Object.assign({
  revision: 1,
  version: VERSION,
  state: 'holds',
  basis_state: 'holds',
  stale_reason: null,
  reason: null,
  checkable_if: null,
  record_projection: null,
  findings: [],
  surfaces: [],
  visual: [],
  not_looked_at: [],
  finding_count: 0,
  checked_places: 0,
  check_impl_version: 'decisions-1',
  tool_versions: { git: '2.50.1' },
  computed_at: '2026-10-07T09:15:00Z',
  last_held_version: null,
  waived: null,
  blocks_approval: false,
}, over)

const decision = (over) => Object.assign({
  project_id: 'p1',
  revision: 1,
  lifecycle: 'accepted',
  params_schema_version: 1,
  params_sha256: 'c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00',
  severity: 'blocks',
  accepted_by: 'you',
  accepted_at: '2026-10-05T10:00:00Z',
  acceptance_reason: 'agreed when we set up the launch',
  applies_from_version: 'wt-0aa1bb2cc3dd4ee5',
  current_verdict: null,
  blocks_approval: false,
}, over)

const NO_HOME = 'Never show a home-folder path on any screen'
const STRIPE = 'Payments are named Stripe everywhere'
const TESTS_FIRST = 'Write the failing test before you change the code'
const STATUS_LINE = 'The status bar always says what ThruLine keeps on this machine'
const UTC = 'Every exported time is UTC, written with a Z'

/** The gate, as `project_approval_gate` serves it (crates/cli/src/uiserve/decisions.rs). */
export const GATE_ACTIVE = Object.freeze({
  active: true,
  reason: null,
  members: [{ project_id: 'p1', active: true, reason: null }],
})
export const GATE_DORMANT = Object.freeze({
  active: false,
  reason: 'Decisions approval is dormant in this record.',
  members: [{ project_id: 'p1', active: false, reason: 'No blocking decision has been confirmed in a person’s own window yet.' }],
})

/** The confirmation transport, as `confirmation_transport` serves it. */
export const TRANSPORT_NATIVE = Object.freeze({
  available: 'native_click',
  browser_available: true,
  reason: null,
  native: { schema_version: 1, available: true, reason: null },
  preparation_is_person_act: false,
  requires_presence: true,
})
const WHY_NOT = 'The native review needs ThruLine to run in the foreground of its own terminal window.'
export const TRANSPORT_TERMINAL = Object.freeze({
  available: 'terminal',
  browser_available: false,
  reason: WHY_NOT,
  native: { schema_version: 1, available: false, reason: WHY_NOT },
  preparation_is_person_act: false,
  requires_presence: true,
})
export const TRANSPORT_UNAVAILABLE = Object.freeze({
  available: 'unavailable',
  browser_available: false,
  reason: 'The native review is only available on macOS.',
  native: { schema_version: 1, available: false, reason: 'The native review is only available on macOS.' },
  preparation_is_person_act: false,
  requires_presence: true,
})

/** A list answer around `decisions`, every top-level key as API.md names it. */
export const listWith = ({ decisions, gate = GATE_ACTIVE, transport = TRANSPORT_NATIVE, current = VERSION }) => ({
  schema_version: 1,
  project_id: 'p1',
  current_version: current,
  decisions,
  approval_gate: gate,
  confirmation_transport: transport,
})

const MAIN = [
  decision({
    id: 'dec-01',
    span: NO_HOME,
    sources: [source('ev-dec-01', 41, NO_HOME, '2026-10-05T09:58:00Z')],
    kind: 'ui_copy_absent',
    kind_code: 'A5',
    params: { terms: ['/Users/'], allowlist: [] },
    blocks_approval: true,
    current_verdict: verdict({
      id: 'ver-01',
      decision_id: 'dec-01',
      state: 'broken',
      basis_state: 'broken',
      findings: [
        finding({ id: 'f-01', path: 'ui/src/hq/screens/Approve-Sheet/logic.js', line: 212, span: 'Saved to /Users/', edit_id: 'ed-77', introduced_at: '2026-10-07T08:51:00Z' }),
        finding({ id: 'f-02', path: 'ui/src/hq/screens/Settings/logic.js', line: 88, span: 'in /Users/', edit_id: 'ed-78', introduced_at: '2026-10-07T08:53:00Z' }),
      ],
      surfaces: [surface('ui_copy', 'broken', 214, 2)],
      finding_count: 2,
      checked_places: 214,
      last_held_version: 'wt-0aa1bb2cc3dd4ee5',
      blocks_approval: true,
    }),
  }),
  decision({
    id: 'dec-02',
    span: STRIPE,
    severity: 'warns',
    sources: [source('ev-dec-02', 57, STRIPE, '2026-10-05T10:02:00Z')],
    kind: 'entity_name',
    kind_code: 'G4',
    params: { canonical: 'Stripe', surfaces: ['ui_copy', 'docs'] },
    current_verdict: verdict({
      id: 'ver-02',
      decision_id: 'dec-02',
      surfaces: [surface('ui_copy', 'holds', 214, 0), surface('docs', 'holds', 96, 0)],
      checked_places: 310,
      last_held_version: 'wt-3c3ca28a1b2c3d4e',
    }),
  }),
  decision({
    id: 'dec-03',
    span: TESTS_FIRST,
    sources: [source('ev-dec-03', 63, TESTS_FIRST, '2026-10-05T10:05:00Z')],
    kind: 'tests_before_code',
    kind_code: 'F2',
    params: { change_id: 'chg-1', test_paths: ['tests/'], source_paths: ['src/'] },
    blocks_approval: true, // an unverifiable result that the policy counts as blocking
    current_verdict: verdict({
      id: 'ver-03',
      decision_id: 'dec-03',
      state: 'unverifiable',
      basis_state: 'unverifiable',
      reason: 'the record has no failing run before the first source edit',
      checkable_if: 'a test run is recorded before the source edit',
      blocks_approval: true,
    }),
  }),
  decision({
    id: 'dec-04',
    span: STATUS_LINE,
    sources: [source('ev-dec-04', 70, STATUS_LINE, '2026-10-05T10:09:00Z')],
    kind: 'spec_sentence_present',
    kind_code: 'C5',
    params: { sentence: 'ThruLine is keeping', paths: ['ui/src/hq/data/project.js'] },
    current_verdict: verdict({
      id: 'ver-04',
      decision_id: 'dec-04',
      version: OLDER,
      state: 'stale',
      basis_state: 'holds',
      stale_reason: 'later_commit',
      checked_places: 1,
      last_held_version: 'wt-0aa1bb2cc3dd4ee5',
      blocks_approval: false, // a stale projection never blocks by itself
    }),
  }),
  decision({
    id: 'dec-05',
    span: UTC,
    lifecycle: 'proposed',
    severity: null, // null before an acceptance exists (API.md Decision)
    accepted_by: null,
    accepted_at: null,
    acceptance_reason: null,
    applies_from_version: null,
    sources: [source('ev-dec-05', 81, UTC, '2026-10-06T14:20:00Z')],
    kind: 'timezone_policy',
    kind_code: 'G3',
    params: { format: 'utc_z', surfaces: ['docs'] },
    current_verdict: null,
  }),
]

export const DECISIONS_LIST = Object.freeze(listWith({ decisions: MAIN }))

/** A project that has no decisions yet. */
export const DECISIONS_EMPTY = Object.freeze(listWith({ decisions: [] }))

/* ---- the same decisions under a gate that is not active ---- */

const dormant = (d) => Object.assign({}, d, {
  blocks_approval: false,
  current_verdict: d.current_verdict ? Object.assign({}, d.current_verdict, { blocks_approval: false }) : null,
})

/** The main decisions, the member dormant: nothing blocks, whatever the state words say. */
export const DECISIONS_DORMANT = Object.freeze(listWith({ decisions: MAIN.map(dormant), gate: GATE_DORMANT }))

/* ---- the rules, one decision each (gate active) ---- */

const WAIVER = Object.freeze({ id: 'w-1', actor: 'you', because: 'shipping a fix next', at: '2026-10-07T10:00:00Z', version_id: 'wt-3c3ca28a1b2c3d4e', expires_version: null, expires_at: null })

const CASES = [
  // broken and blocking: the verdict says so on this exact version
  Object.assign({}, MAIN[0], { id: 'cs-broken-blocks', current_verdict: Object.assign({}, MAIN[0].current_verdict, { id: 'ver-c1', decision_id: 'cs-broken-blocks' }) }),
  // broken, but the person set it to warn: the owner says it does not block
  decision({
    id: 'cs-broken-warns',
    span: 'Warn me about home-folder paths',
    severity: 'warns',
    sources: [source('ev-c2', 90, 'Warn me about home-folder paths', '2026-10-05T10:20:00Z')],
    kind: 'ui_copy_absent',
    kind_code: 'A5',
    params: { terms: ['/Users/'], allowlist: [] },
    current_verdict: verdict({
      id: 'ver-c2',
      decision_id: 'cs-broken-warns',
      state: 'broken',
      basis_state: 'broken',
      findings: [finding({ id: 'f-c2', path: 'docs/guide.md', line: 4, surface: 'docs', span: '/Users/', edit_id: 'ed-80' })],
      finding_count: 1,
      checked_places: 12,
      surfaces: [surface('docs', 'broken', 12, 1)],
    }),
  }),
  // broken, with a valid waiver on this exact version
  decision({
    id: 'cs-broken-waived',
    span: 'Dates in the changelog are UTC',
    sources: [source('ev-c3', 91, 'Dates in the changelog are UTC', '2026-10-05T10:21:00Z')],
    kind: 'timezone_policy',
    kind_code: 'G3',
    params: { format: 'utc_z', surfaces: ['docs'] },
    current_verdict: verdict({
      id: 'ver-c3',
      decision_id: 'cs-broken-waived',
      state: 'broken',
      basis_state: 'broken',
      findings: [finding({ id: 'f-c3', path: 'CHANGELOG.md', line: 9, surface: 'docs', span: 'PST', edit_id: 'ed-81' })],
      finding_count: 1,
      checked_places: 4,
      waived: WAIVER,
    }),
  }),
  // no proof, and the policy says no proof only warns (`on_unverifiable: warn`)
  decision({
    id: 'cs-unverifiable-warn',
    span: 'Every migration has a rollback note',
    sources: [source('ev-c4', 92, 'Every migration has a rollback note', '2026-10-05T10:22:00Z')],
    kind: 'spec_sentence_present',
    kind_code: 'C5',
    params: { sentence: 'Rollback:', paths: ['docs/migrations.md'] },
    current_verdict: verdict({
      id: 'ver-c4',
      decision_id: 'cs-unverifiable-warn',
      state: 'unverifiable',
      basis_state: 'unverifiable',
      reason: 'the record does not name the migration file',
      checkable_if: 'the migration file is recorded',
    }),
  }),
  // the latest result is on an older version; the current version has no check and policy requires one
  decision({
    id: 'cs-stale-no-check',
    span: 'Release notes are written before tagging',
    sources: [source('ev-c5', 93, 'Release notes are written before tagging', '2026-10-05T10:23:00Z')],
    kind: 'spec_sentence_present',
    kind_code: 'C5',
    params: { sentence: 'Release notes', paths: ['docs/release.md'] },
    blocks_approval: true,
    current_verdict: verdict({
      id: 'ver-c5',
      decision_id: 'cs-stale-no-check',
      version: OLDER,
      state: 'stale',
      basis_state: 'holds',
      stale_reason: 'later_commit',
      checked_places: 1,
      last_held_version: 'wt-0aa1bb2cc3dd4ee5',
    }),
  }),
  // accepted, blocking, and no result at all: the missing check itself blocks
  decision({
    id: 'cs-no-verdict',
    span: 'The privacy sentence is on every settings screen',
    sources: [source('ev-c6', 94, 'The privacy sentence is on every settings screen', '2026-10-05T10:24:00Z')],
    kind: 'spec_sentence_present',
    kind_code: 'C5',
    params: { sentence: 'Nothing leaves this machine', paths: ['ui/src/hq/screens/Settings'] },
    blocks_approval: true,
    current_verdict: null,
  }),
  // holds, blocking severity: nothing to refuse
  decision({
    id: 'cs-holds-blocks',
    span: 'Counts are written with thousands separators',
    sources: [source('ev-c7', 95, 'Counts are written with thousands separators', '2026-10-05T10:25:00Z')],
    kind: 'entity_name',
    kind_code: 'G4',
    params: { canonical: 'fmtCount', surfaces: ['ui_copy'] },
    current_verdict: verdict({ id: 'ver-c7', decision_id: 'cs-holds-blocks', checked_places: 40, last_held_version: 'wt-3c3ca28a1b2c3d4e' }),
  }),
]

export const DECISIONS_CASES = Object.freeze(listWith({ decisions: CASES }))
