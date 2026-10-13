/* Change-Brief: the canvas's demo content, as the screen's data contract.
 *
 * Every piece of content the Brief shows arrives through one object of this
 * shape. `renderVals()` reads `this.props.data || DEMO`; in the design gallery
 * (`props.data` absent) it draws `DEMO.states[state.demo]`, one complete
 * snapshot per state the canvas's demo switcher can select. A live Change is
 * one snapshot of the same shape, with no `states`.
 *
 * Facts, not display strings: versions are ids, steps are numbers, tallies are
 * counts, durations are milliseconds, sizes are bytes. Joining with " · ",
 * choosing a glyph or a colour, and every sentence of product language are
 * renderVals' job. The field-by-field contract, with where each fact must come
 * from and whether the store has it today, is DATA.md beside this file.
 *
 * `illustrative: true` marks a record the canvas invented to show a state
 * (steps 62 and 64, version a1b2c3d, the 16:06 acceptance, the live output).
 */

const SESSION = { id: 'session-1', agent: 'Claude Code', source: 'Claude Code hook v0.4', startedAt: '16:01', step: 2 }

const REQUEST = { step: 6, time: '16:01', text: 'the checkout total is wrong when a discount code is applied — fix it' }

/* What the agent said, message by message. A message can hold several
 * statements; the ones that make a checkable claim are in `statements`. */
const SAID_32 = { id: 'said-32', step: 32, sessionId: 'session-1', agent: 'Claude Code', text: 'The discount is being applied twice. Fixing the second application.' }
const SAID_52 = { id: 'said-52', step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', text: 'Checkout total fixed. Tests pass.' }

const V_4B = { id: '4b19a0c', madeByStep: 16, recordedAtStep: null, commit: null, unsavedChanges: null }
const V_9F = { id: '9f2ad3e', madeByStep: 36, recordedAtStep: 38, commit: null, unsavedChanges: null }
const V_E7 = { id: 'e71c5b0', madeByStep: 56, recordedAtStep: 58, commit: '3f9a1c2', unsavedChanges: false }
const V_A1 = { id: 'a1b2c3d', madeByStep: 64, recordedAtStep: null, commit: null, unsavedChanges: true, illustrative: true }

const E_16 = { id: 'edit-16', step: 16, time: null, file: 'cart/totals.py', makesVersion: '4b19a0c' }
const E_36 = { id: 'edit-36', step: 36, time: null, file: 'cart/totals.py', makesVersion: '9f2ad3e' }
const E_56 = { id: 'edit-56', step: 56, time: '16:04', file: 'checkout/api.py', makesVersion: 'e71c5b0' }
const E_64 = { id: 'edit-64', step: 64, time: '16:11', file: 'checkout/api.py', makesVersion: 'a1b2c3d', illustrative: true }

/* Runs: every command execution the record holds for this Change.
 * status: 'finished' (a result is recorded) | 'no_result' | 'running'. */
const RUN_26 = {
  id: 'run-26', step: 26, time: null, kind: 'test', command: 'pytest -q tests/test_totals.py', version: '4b19a0c',
  status: 'finished', exitCode: 1, passed: 3, failed: 1, durationMs: null, capturedBy: 'thruline', scope: ['tests/test_totals.py'],
}
const RUN_46 = {
  id: 'run-46', step: 46, time: null, kind: 'test', command: 'pytest -q tests/test_totals.py', version: '9f2ad3e',
  status: 'finished', exitCode: 0, passed: 4, failed: 0, durationMs: null, capturedBy: 'thruline', scope: ['tests/test_totals.py'],
}
const RUN_60 = {
  id: 'run-60', step: 60, time: null, kind: 'command', command: 'rm -rf .pytest_cache && pytest -q', version: 'e71c5b0',
  status: 'no_result', exitCode: null, passed: null, failed: null, durationMs: null, capturedBy: 'thruline', scope: [],
}
const RUN_62 = {
  id: 'run-62', step: 62, time: '16:09', kind: 'test', command: 'pytest -q tests/test_totals.py checkout/', version: 'e71c5b0',
  status: 'finished', exitCode: null, passed: 5, failed: 0, durationMs: 420, capturedBy: 'thruline', scope: ['tests/test_totals.py', 'checkout/'],
  illustrative: true,
}
const RUN_LIVE = {
  id: 'run-live', step: null, time: null, kind: 'test', command: 'pytest -q tests/test_totals.py checkout/', version: 'e71c5b0',
  status: 'running', elapsedS: 3, exitCode: null, passed: null, failed: null, durationMs: null, capturedBy: 'thruline',
  scope: ['tests/test_totals.py', 'checkout/'],
  output: ['$ pytest -q tests/test_totals.py checkout/', 'tests/test_totals.py ....', 'checkout/tests/test_api.py'],
  illustrative: true,
}

/* The check that would settle the brief's statement on the current version. */
const recheck = (on) => ({
  runner: 'pytest -q', scope: ['tests/test_totals.py', 'checkout/'], on, permission: 'ask_once_per_project',
})

/* The lane graph. Nodes are records placed on a lane and, for work and
 * execution, on the version column they belong to (the version an edit makes,
 * the version a run ran on, the version current when the agent spoke).
 * `ref` names the record a node draws. The two absence markers ("Before
 * 16:01 · Not recorded", "Nothing recorded · A run on … would appear here")
 * are not nodes: renderVals derives them from `change.recordedFrom` and from
 * the current version having no run. Which nodes are lit is not stored either:
 * it is the selected statement's proof chain. */
const N = {
  session: { id: 'session-1', lane: 'request', kind: 'session', step: 2, version: null, label: 'Claude Code hook v0.4', time: '16:01', ref: 'session-1' },
  request: { id: 'request-6', lane: 'request', kind: 'request', step: 6, version: null, label: REQUEST.text, ref: 'request-6' },
  edit16: { id: 'edit-16', lane: 'work', kind: 'edit', step: 16, version: '4b19a0c', label: 'cart/totals.py', ref: 'edit-16' },
  said32: { id: 'said-32', lane: 'work', kind: 'agent_said', step: 32, version: '4b19a0c', label: SAID_32.text, ref: 'said-32' },
  edit36: { id: 'edit-36', lane: 'work', kind: 'edit', step: 36, version: '9f2ad3e', label: 'cart/totals.py', ref: 'edit-36' },
  edit56: { id: 'edit-56', lane: 'work', kind: 'edit', step: 56, version: 'e71c5b0', label: 'checkout/api.py', ref: 'edit-56' },
  edit64: { id: 'edit-64', lane: 'work', kind: 'edit', step: 64, version: 'a1b2c3d', label: 'checkout/api.py', ref: 'edit-64' },
  run26: { id: 'run-26', lane: 'execution', kind: 'run', step: 26, version: '4b19a0c', label: RUN_26.command, ref: 'run-26' },
  run46: { id: 'run-46', lane: 'execution', kind: 'run', step: 46, version: '9f2ad3e', label: RUN_46.command, ref: 'run-46' },
  run62: { id: 'run-62', lane: 'execution', kind: 'run', step: 62, version: 'e71c5b0', label: RUN_62.command, ref: 'run-62' },
  runLive: { id: 'run-live', lane: 'execution', kind: 'run', step: null, version: 'e71c5b0', label: RUN_LIVE.command, ref: 'run-live' },
  cmd60: { id: 'run-60', lane: 'execution', kind: 'command', step: 60, version: 'e71c5b0', label: RUN_60.command, ref: 'run-60' },
  criteria: { id: 'criteria', lane: 'claims', kind: 'criteria', step: null, version: null, label: null, ref: 'criteria' },
  fixed: { id: 'stmt-52-fixed', lane: 'claims', kind: 'statement', step: 52, version: null, label: 'Checkout total fixed.', ref: 'stmt-52-fixed' },
  tests: { id: 'stmt-52-tests', lane: 'claims', kind: 'statement', step: 52, version: null, label: 'Tests pass.', ref: 'stmt-52-tests' },
}

/* kind: 'recorded_order' (the record's own sequence) | 'documented_reference'
 * (one record names another: the agent cites a run) | 'inferred' (ThruLine's
 * reading, not stated anywhere). The canvas draws these six in every state. */
const EDGES = [
  { from: 'request-6', to: 'edit-16', kind: 'recorded_order' },
  { from: 'said-32', to: 'edit-36', kind: 'recorded_order' },
  { from: 'edit-36', to: 'edit-56', kind: 'recorded_order' },
  { from: 'run-46', to: 'edit-36', kind: 'recorded_order' },
  { from: 'run-26', to: 'said-32', kind: 'inferred' },
  { from: 'stmt-52-tests', to: 'run-46', kind: 'documented_reference' },
]

const STATEMENT_FIXED = (proof) => ({
  id: 'stmt-52-fixed', messageId: 'said-52', text: 'Checkout total fixed.', claimKind: 'task_complete', verifier: false,
  step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', versionBefore: '9f2ad3e', source: null, proof,
})
const STATEMENT_TESTS = (proof) => ({
  id: 'stmt-52-tests', messageId: 'said-52', text: 'Tests pass.', claimKind: 'tests_pass', verifier: true,
  step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', versionBefore: '9f2ad3e', source: null, proof,
})

/* The proof chain of "Checkout total fixed.": a completion claim has no
 * verifier, so what counts as done decides it and the chain stops at ②. */
const FIXED_PROOF = { verdict: 'recorded', rule: null, restsOn: { kind: 'criteria' } }

const COMMON = {
  project: { id: 'checkout-web', name: 'checkout-web', worktree: '~/code/checkout-web' },
  machine: { recordsBytes: 1400000000, networkCalls: false },
  record: { chainHead: 'sha256:afb960f9…2861', tree: 'c324df66…', feeds: ['Claude Code hook v0.4', 'the exec recorder'] },
  sessions: [SESSION],
  request: REQUEST,
  messages: [SAID_32, SAID_52],
  brief: { statementId: 'stmt-52-tests' },
}

const change = (state, stages) => ({
  id: 'change-checkout-discount', title: 'Fix double-applied discount on checkout total', state,
  stages, recordedFrom: '16:01', stepCount: 19, recording: true,
})
const st = (define, prepare, build, verify, review, approve) =>
  [['define', define], ['prepare', prepare], ['build', build], ['verify', verify], ['review', review], ['approve', approve]].map(([id, status]) => ({ id, status }))

const NOT_ACCEPTED = { accepted: false, acceptedAt: null, acceptedOnVersion: null, acceptedAfterStep: null, version: null, lineCount: null, proposedCount: 4, proposedFromStep: 6, provenOn: null }
const ACCEPTED = (provenOn) => ({ accepted: true, acceptedAt: '16:06', acceptedOnVersion: 'e71c5b0', acceptedAfterStep: 60, version: 'v1', lineCount: 4, proposedCount: 4, proposedFromStep: 6, provenOn, illustrative: true })

/* S1 · 16:04 — the canvas's default. "Tests pass." rests on step 46, which ran
 * on 9f2ad3e; checkout/api.py changed at step 56; nothing has run on e71c5b0. */
const TESTS_S1 = {
  verdict: 'not_proven', version: 'e71c5b0', scope: [], stopsAt: 5, rule: 'tests_ran_before_final_edit',
  restsOn: { kind: 'run', runId: 'run-46', basis: 'cited_by_agent', checkedAgainst: { covers: ['tests/test_totals.py'], misses: ['checkout/'] } },
  ranOn: { version: '9f2ad3e' },
  since: { editId: 'edit-56', dependsOnWholeChange: false },
  recheck: { status: 'none', runId: null },
  lookedAt: ['run-26', 'run-46', 'run-60'],
  earlier: null,
  thenAndNow: { from: '9f2ad3e', to: 'e71c5b0', noLongerCounts: [{ runId: 'run-46', editId: 'edit-56' }], kept: [], added: [] },
  followUp: { toldAgent: false, laterWork: null, laterWorkSince: null, checkedAgain: null },
}

const S1 = {
  ...COMMON,
  change: change('not_proven', st('not_started', 'done', 'current', 'not_started', 'not_reached', 'not_reached')),
  view: { readAt: '16:04', readDate: '2026-09-26', readVersion: 'e71c5b0', previousLookAt: null, sinceLook: null },
  versions: [V_4B, V_9F, V_E7],
  currentVersion: 'e71c5b0',
  edits: [E_16, E_36, E_56],
  runs: [RUN_26, RUN_46, RUN_60],
  statements: [STATEMENT_TESTS(TESTS_S1), STATEMENT_FIXED(FIXED_PROOF)],
  criteria: NOT_ACCEPTED,
  recheck: recheck('e71c5b0'),
  findings: [
    { kind: 'edit_after_run', editStep: 56 },
    { kind: 'no_check', file: 'checkout/api.py' },
    { kind: 'agent_reported', claimStep: 52, runStep: 46 },
    { kind: 'run_outcomes', failedSteps: [26], passedSteps: [46] },
    { kind: 'no_result', step: 60 },
  ],
  graph: {
    nodes: [N.session, N.request, N.edit16, N.said32, N.edit36, N.edit56, N.run26, N.run46, N.cmd60, N.criteria, N.fixed, N.tests],
    edges: EDGES,
  },
  primaryAction: 'check_again',
}

/* Checking now — the re-check is running on e71c5b0, 3 s in. */
const CHECKING = {
  ...S1,
  view: { ...S1.view, readAt: '16:09' },
  runs: [RUN_26, RUN_46, RUN_60, RUN_LIVE],
  statements: [STATEMENT_TESTS({ ...TESTS_S1, recheck: { status: 'running', runId: 'run-live' } }), STATEMENT_FIXED(FIXED_PROOF)],
  graph: {
    nodes: [N.session, N.request, N.edit16, N.said32, N.edit36, N.edit56, N.run26, N.run46, N.runLive, N.cmd60, N.criteria, N.fixed, N.tests],
    edges: EDGES,
  },
  primaryAction: 'stop_check',
}

/* K-ready · 16:09 — step 62 ran on e71c5b0 with checkout/ in scope; what
 * counts as done was accepted at 16:06 and all 4 lines rest on step 62. */
const TESTS_READY = {
  verdict: 'proven', version: 'e71c5b0', scope: ['tests/test_totals.py', 'checkout/'], stopsAt: null, rule: 'tests_ran_before_final_edit',
  restsOn: { kind: 'run', runId: 'run-62', basis: 'matched', match: { sameCommandAs: 46, widerScope: true }, checkedAgainst: null },
  ranOn: { version: 'e71c5b0' },
  since: null,
  recheck: { status: 'not_needed', runId: null },
  lookedAt: ['run-62', 'run-60'],
  earlier: { at: '16:04', verdict: 'not_proven', version: 'e71c5b0', scope: [] },
  thenAndNow: { from: '9f2ad3e', to: 'e71c5b0', noLongerCounts: [{ runId: 'run-46', editId: 'edit-56' }], kept: [], added: [{ runId: 'run-62', newScope: ['checkout/'] }] },
  followUp: { toldAgent: false, laterWork: null, laterWorkSince: '16:09', checkedAgain: { runId: 'run-62' } },
}

const READY = {
  ...COMMON,
  change: change('ready', st('done', 'done', 'done', 'done', 'current', 'not_started')),
  view: { readAt: '16:09', readDate: '2026-09-26', readVersion: 'e71c5b0', previousLookAt: '16:04', sinceLook: { criteriaAccepted: true, runs: 1, filesChanged: 0 } },
  versions: [V_4B, V_9F, V_E7],
  currentVersion: 'e71c5b0',
  edits: [E_16, E_36, E_56],
  runs: [RUN_26, RUN_46, RUN_60, RUN_62],
  statements: [STATEMENT_TESTS(TESTS_READY), STATEMENT_FIXED(FIXED_PROOF)],
  criteria: ACCEPTED({ version: 'e71c5b0', count: 4, runId: 'run-62' }),
  recheck: recheck('e71c5b0'),
  findings: [
    { kind: 'edit_covered', editStep: 56, runStep: 62, resolved: true },
    { kind: 'in_scope', path: 'checkout/', runStep: 62, resolved: true },
    { kind: 'run_outcomes', failedSteps: [26], passedSteps: [46] },
    { kind: 'no_result', step: 60 },
  ],
  graph: {
    nodes: [N.session, N.request, N.edit16, N.said32, N.edit36, N.edit56, N.run26, N.run46, N.run62, N.cmd60, N.criteria, N.fixed, N.tests],
    edges: EDGES,
  },
  primaryAction: 'approve',
}

/* Code changed · a1b2c3d — checkout/api.py changed again at step 64 (16:11)
 * while the screen was open; step 62 now ran on a version 1 older. */
const TESTS_MOVED = {
  verdict: 'not_proven', version: 'a1b2c3d', scope: [], stopsAt: 5, rule: 'tests_ran_before_final_edit',
  restsOn: { kind: 'run', runId: 'run-62', basis: 'matched', match: { sameCommandAs: 46, widerScope: true }, checkedAgainst: null },
  ranOn: { version: 'e71c5b0' },
  since: { editId: 'edit-64', dependsOnWholeChange: true },
  recheck: { status: 'none', runId: null },
  lookedAt: ['run-62', 'run-60'],
  earlier: { at: '16:09', verdict: 'proven', version: 'e71c5b0', scope: ['tests/test_totals.py', 'checkout/'] },
  thenAndNow: { from: 'e71c5b0', to: 'a1b2c3d', noLongerCounts: [{ runId: 'run-62', editId: 'edit-64' }], kept: [], added: [] },
  followUp: { toldAgent: false, laterWork: { editId: 'edit-64' }, laterWorkSince: null, checkedAgain: { runId: 'run-62' } },
}

const MOVED = {
  ...COMMON,
  change: change('needs_recheck', st('done', 'done', 'current', 'not_started', 'not_reached', 'not_reached')),
  view: { readAt: '16:11', readDate: '2026-09-26', readVersion: 'e71c5b0', previousLookAt: null, sinceLook: null, changedSinceLoad: ['checkout/api.py'] },
  versions: [V_4B, V_9F, V_E7, V_A1],
  currentVersion: 'a1b2c3d',
  edits: [E_16, E_36, E_56, E_64],
  runs: [RUN_26, RUN_46, RUN_60, RUN_62],
  statements: [STATEMENT_TESTS(TESTS_MOVED), STATEMENT_FIXED(FIXED_PROOF)],
  criteria: ACCEPTED({ version: 'a1b2c3d', count: 0, runId: null }),
  recheck: recheck('a1b2c3d'),
  findings: [
    { kind: 'edit_after_run', editStep: 64 },
    { kind: 'no_check', file: 'checkout/api.py' },
    { kind: 'run_outcomes', failedSteps: [26], passedSteps: [46] },
    { kind: 'no_result', step: 60 },
  ],
  graph: {
    nodes: [N.session, N.request, N.edit16, N.said32, N.edit36, N.edit56, N.run26, N.run46, N.run62, N.cmd60, N.criteria, N.fixed, N.tests, N.edit64],
    edges: EDGES,
  },
  primaryAction: 'check_what_changed',
}

/* The default snapshot is S1; `states` holds every variant the demo switcher
 * selects (the canvas's `state.demo` values). */
export const DEMO = { ...S1, states: { s1: S1, checking: CHECKING, ready: READY, moved: MOVED } }

export default DEMO
