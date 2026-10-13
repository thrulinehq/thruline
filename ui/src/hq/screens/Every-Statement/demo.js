/* Every-Statement: the canvas's demo content, as the screen's data contract.
 *
 * Every fact the screen shows arrives through this one object (or through
 * `props.data` of the same shape, in the live product). These are typed facts
 * named for what they are; how a fact reads on screen (joining, glyphs,
 * colours, sentences) is decided in logic.js `renderVals()`. DATA.md says, for
 * every field, what it means and which recorded fact it must come from.
 *
 * Field names follow Change-Brief's demo.js wherever a field means the same
 * thing there (project, machine, view, change, sessions, messages, versions,
 * runs, edits, statements[].proof, recheck).
 *
 * Steps are the record's step numbers within their own session. Times are the
 * local clock time of the record, HH:MM. Versions are short commit-like ids.
 */
export const DEMO = {
  project: { id: 'checkout-web', name: 'checkout-web', worktree: '~/code/checkout-web' },
  machine: { recordsBytes: 1400000000 },
  view: { readAt: '16:06', readDate: '2026-09-26', utcOffset: '+00:00' },
  change: {
    id: 'change-checkout-discount',
    title: 'Fix double-applied discount on checkout total',
    state: 'needs_recheck',
    recording: true,
  },
  /* The version the screen speaks for: the newest recorded version. */
  version: { id: 'e71c5b0', commit: '3f9a1c2' },

  sessions: [
    { id: 'session-1', agent: 'Claude Code' },
    { id: 'session-2', agent: 'Codex' },
  ],

  /* Recorded versions of the code, oldest first; the last is the current one. */
  versions: [
    { id: '4b19a0c', madeByStep: 16, recordedAtStep: null },
    { id: '9f2ad3e', madeByStep: 36, recordedAtStep: 38 },
    { id: 'e71c5b0', madeByStep: 56, recordedAtStep: 58 },
  ],

  /* Every command run this Change holds, in step order. status: finished |
   * no_result | running. A finished run held when it exited 0. */
  runs: [
    { id: 'run-26', step: 26, sessionId: 'session-1', version: '4b19a0c', command: 'pytest -q tests/test_totals.py', scope: ['tests/test_totals.py'],
      status: 'finished', exitCode: 1, passed: 3, failed: 1, durationMs: null, capturedBy: 'thruline', declaresPaths: false },
    { id: 'run-46', step: 46, sessionId: 'session-1', version: '9f2ad3e', command: 'pytest -q tests/test_totals.py', scope: ['tests/test_totals.py'],
      status: 'finished', exitCode: 0, passed: 4, failed: 0, durationMs: null, capturedBy: 'thruline', declaresPaths: false },
    { id: 'run-60', step: 60, sessionId: 'session-1', version: 'e71c5b0', command: 'rm -rf .pytest_cache && pytest -q', scope: [],
      status: 'no_result', exitCode: null, passed: null, failed: null, durationMs: null, capturedBy: 'thruline', declaresPaths: false },
  ],

  /* Edits a proof chain's ④ Since points at. Every line of every edit is on What-Changed. */
  edits: [
    { id: 'edit-56', step: 56, sessionId: 'session-1', time: '16:04', file: 'checkout/api.py', makesVersion: 'e71c5b0' },
  ],
  lastEditAt: '16:04',

  /* What counts as done: the accepted criteria, in accepted order.
   * link: who tied evidence to the line (person | accepted_checks | null). */
  criteria: {
    accepted: true,
    version: 'v1',
    acceptedAt: '16:06',
    acceptedBy: 'you',
    lines: [
      { id: 'l1', num: 1, gate: false, link: 'person',
        text: 'Applying a discount code once reduces the total by exactly that discount.',
        proof: { verdict: 'needs_recheck', stopsAt: 5, restsOn: { kind: 'run', runId: 'run-46', basis: 'linked_by_person' }, since: { editId: 'edit-56' } } },
      { id: 'l2', num: 2, gate: false, link: 'person',
        text: 'Applying the same discount code twice does not reduce the total twice.',
        proof: { verdict: 'needs_recheck', stopsAt: 5, restsOn: { kind: 'run', runId: 'run-46', basis: 'linked_by_person' }, since: { editId: 'edit-56' } } },
      { id: 'l3', num: 3, gate: false, link: null,
        text: 'Checkout totals with no discount code are unchanged.',
        proof: { verdict: 'in_progress', stopsAt: 2, restsOn: null, lookedAt: ['run-46'], misses: ['checkout/'] } },
      { id: 'l4', num: 4, gate: true, link: 'accepted_checks',
        text: 'tests/test_totals.py passes on the final version.',
        proof: { verdict: 'needs_recheck', stopsAt: 5, restsOn: { kind: 'run', runId: 'run-46', basis: 'accepted_checks' }, since: { editId: 'edit-56' } } },
    ],
  },

  /* The agents' messages, as recorded. A message can hold several statements. */
  messages: [
    { id: 'said-14', step: 14, time: '16:02', sessionId: 'session-2', agent: 'Codex', text: 'Refactored the discount rules; lint is clean.' },
    { id: 'said-32', step: 32, time: null, sessionId: 'session-1', agent: 'Claude Code', text: 'The discount is being applied twice. Fixing the second application.' },
    { id: 'said-52', step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', text: 'Checkout total fixed. Tests pass.' },
  ],

  /* Every checkable statement in those messages, in the order served.
   * claimKind: tests_pass | lint_clean | task_complete. verifier: whether a
   * run can check the claim by itself. versionBefore: the last version
   * recorded before it was said. */
  statements: [
    { id: 'stmt-52-tests', messageId: 'said-52', text: 'Tests pass.', claimKind: 'tests_pass', verifier: true,
      step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', versionBefore: '9f2ad3e',
      proof: { verdict: 'not_proven', stopsAt: 5, restsOn: { kind: 'run', runId: 'run-46', basis: 'cited_by_agent' }, since: { editId: 'edit-56' } } },
    { id: 'stmt-14-lint', messageId: 'said-14', text: 'lint is clean.', claimKind: 'lint_clean', verifier: true,
      step: 14, time: '16:02', sessionId: 'session-2', agent: 'Codex', versionBefore: '4b19a0c',
      proof: { verdict: 'not_proven', stopsAt: 2, restsOn: null, lookedAtSessions: ['session-1', 'session-2'] } },
    { id: 'stmt-52-fixed', messageId: 'said-52', text: 'Checkout total fixed.', claimKind: 'task_complete', verifier: false,
      step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', versionBefore: '9f2ad3e',
      proof: { verdict: 'recorded', stopsAt: 2, restsOn: { kind: 'criteria' }, verb: 'fixed' } },
    { id: 'stmt-14-refactored', messageId: 'said-14', text: 'Refactored the discount rules', claimKind: 'task_complete', verifier: false,
      step: 14, time: '16:02', sessionId: 'session-2', agent: 'Codex', versionBefore: '4b19a0c',
      proof: { verdict: 'recorded', stopsAt: 2, restsOn: { kind: 'criteria' }, verb: 'refactored' } },
  ],

  /* The one check that would supply the missing links on this version. */
  recheck: {
    runner: 'pytest -q',
    scope: ['tests/test_totals.py', 'checkout/'],
    on: 'e71c5b0',
    permission: 'ask_once_per_project',
    reaches: { lines: [1, 2, 3, 4], statements: ['stmt-52-tests'] },
    elapsedS: 3,
  },

  /* The statement open in the inspector when the screen first draws. */
  selected: 'stmt-52-tests',
}
