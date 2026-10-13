/* Whole-Story: the canvas's demo content, as the screen's data contract.
 *
 * Every fact the page shows arrives through one object of this shape —
 * `this.props.data` on a live address, this DEMO in the design gallery.
 * Sentences, glyphs, colours and joins are made from these facts in logic.js.
 * DATA.md says what each field means and where the real fact comes from.
 *
 * Times are local clock times ('HH:MM') on `story.date`, at `story.utcOffset`.
 * Steps are record step numbers. Versions are 7-character version ids.
 */
export const DEMO = Object.freeze({
  project: { name: 'checkout-web', root: '~/code/checkout-web' },
  change: { id: 'fix-double-applied-discount-on-checkout-total', title: 'Fix double-applied discount on checkout total' },
  version: { id: 'e71c5b0', commit: '3f9a1c2', current: true, unsavedChanges: false },
  recording: { following: true },
  readAt: '16:10',
  storage: { bytes: 1400000000 },

  story: { date: '2026-09-26', utcOffset: '+00:00', from: '16:01', to: '16:10', asOf: '16:10' },
  sessions: [{ agent: 'Claude Code' }],
  recordedSteps: 19,

  approval: { version: 'e71c5b0', by: 'you', at: '16:10', criteriaVersion: 'v1', decisionFile: '.thruline/decisions/e71c5b0.json' },
  criteriaAccepted: { version: 'v1', by: 'you', at: '16:06', afterWork: true, lastAgentEditAt: '16:04' },

  // How it went, in order: one band per stretch of the record. kind:
  // 'unrecorded' (before capture) · 'session' (before the first version) · 'version'.
  // Each event: kind 'gap' | 'prompt' | 'run' | 'statement' | 'edit' | 'accept' | 'approve';
  // `restsOn` marks the events the approval rests on (the lit thread).
  timeline: [
    { kind: 'unrecorded', events: [{ kind: 'gap', before: '16:01' }] },
    { kind: 'session', events: [{ kind: 'prompt', by: 'you', step: 6 }] },
    { kind: 'version', version: '4b19a0c', events: [{ kind: 'run', step: 26, passed: 3, failed: 1 }] },
    { kind: 'version', version: '9f2ad3e', events: [
      { kind: 'run', step: 46, passed: 4, failed: 0 },
      { kind: 'statement', step: 52 },
    ] },
    { kind: 'version', version: 'e71c5b0', current: true, events: [
      { kind: 'edit', step: 56 },
      { kind: 'run', step: 60, passed: null, failed: null },
      { kind: 'accept', by: 'you', criteriaVersion: 'v1', at: '16:06', restsOn: true },
      { kind: 'run', step: 62, passed: 5, failed: 0, restsOn: true },
      { kind: 'approve', at: '16:10' },
    ] },
  ],

  // What counts as done (the accepted criteria), each with its proof chain.
  // linkBasis: 'person' | 'run_names_subject' | 'accepted_checks'.
  criteria: [
    { n: 1, gate: false, text: 'Applying a discount code once reduces the total by exactly that discount.', linkBasis: 'person' },
    { n: 2, gate: false, text: 'Applying the same discount code twice does not reduce the total twice.', linkBasis: 'person' },
    { n: 3, gate: false, text: 'Checkout totals with no discount code are unchanged.', linkBasis: 'run_names_subject' },
    { n: 4, gate: true, text: 'tests/test_totals.py passes on the final version.', linkBasis: 'accepted_checks' },
  ].map((c) => ({
    ...c,
    proof: {
      state: 'proven',
      restsOn: { kind: 'run', step: 62 },
      ranOn: 'e71c5b0',
      changedSince: false,
      scope: ['cart/', 'checkout/', 'tests/'],
    },
  })),

  // Every run in the Change, in the order shown. passed/failed null = no result.
  runs: [
    { step: 26, at: '16:02', command: 'pytest -q tests/test_totals.py', destructive: false, passed: 3, failed: 1, version: '4b19a0c', capturedBy: 'thruline' },
    { step: 46, at: '16:03', command: 'pytest -q tests/test_totals.py', destructive: false, passed: 4, failed: 0, version: '9f2ad3e', capturedBy: 'thruline' },
    { step: 62, at: '16:09', command: 'pytest -q tests/test_totals.py checkout/', destructive: false, passed: 5, failed: 0, version: 'e71c5b0', capturedBy: 'thruline', scope: ['tests/test_totals.py', 'checkout/'] },
    { step: 60, at: null, command: 'rm -rf .pytest_cache && pytest -q', destructive: true, destroys: 'removes a directory', passed: null, failed: null, version: null, permissionAskedAt: '16:04' },
  ],

  // What the agent said. `checks` is how the record read the statement over time.
  statements: [
    {
      agent: 'Claude Code', step: 52, at: '16:03', quote: 'Tests pass.', claim: 'tests pass', kind: 'checkable',
      versionBefore: '9f2ad3e',
      checks: [
        { at: '16:04', state: 'not_proven', version: 'e71c5b0', restedOn: { step: 46, version: '9f2ad3e' }, changed: { file: 'checkout/api.py', step: 56 }, stoppedAt: 5 },
        { at: '16:09', state: 'proven', version: 'e71c5b0', restsOn: { step: 62, matchedBy: 'thruline', sameCommandAs: 46, scopeAdded: 'checkout/' } },
      ],
    },
    { agent: 'Claude Code', step: 52, at: '16:03', quote: 'Checkout total fixed.', kind: 'done_claim', word: 'fixed' },
    { agent: 'Claude Code', step: 32, at: null, quote: 'The discount is being applied twice. Fixing the second application.', kind: 'unchecked' },
  ],

  // Findings, grouped by how they were settled. settlement.kind: 'settled' | 'nothing' | 'open'.
  findings: {
    foundAt: '16:04',
    groups: [
      {
        findings: [
          { n: 1, text: 'checkout/api.py changed after the run the statement cited.', where: 'step 56 · after step 46' },
          { n: 2, text: 'No run had checkout/api.py in scope.', where: 'looked at steps 26, 46' },
          { n: 3, text: 'The statement’s result came from the agent’s own words; the run was captured separately.', where: 'step 52 · step 46' },
        ],
        settlement: { kind: 'settled', at: '16:09', text: 'step 62 re-ran on e71c5b0 with checkout/ in scope, captured by ThruLine.', ref: 'step 62' },
      },
      {
        findings: [{ n: 4, text: 'A run failed, then a later one passed.', where: 'step 26 on 4b19a0c → step 46 on 9f2ad3e' }],
        settlement: { kind: 'nothing', text: 'Step 26 speaks only for 4b19a0c, the version it ran on.' },
      },
      {
        findings: [{ n: 5, text: 'A destructive command asked for permission and has no result.', where: 'step 60', destroys: 'removes a directory' }],
        settlement: { kind: 'open', text: 'If it runs and changes a file, the version moves.' },
      },
    ],
  },

  // Decision history, version-bound, in order.
  decisions: [
    { kind: 'accept', at: '16:06', criteriaVersion: 'v1', statements: 4, afterWork: true, stateAfter: { state: 'needs_recheck', version: 'e71c5b0' } },
    { kind: 'run', at: '16:09', step: 62, version: 'e71c5b0', scopeAdded: 'checkout/', stateAfter: { state: 'ready', version: 'e71c5b0' } },
    { kind: 'approve', at: '16:10', version: 'e71c5b0', by: 'you', criteriaVersion: 'v1', file: '~/code/checkout-web/.thruline/decisions/e71c5b0.json' },
  ],

  // What the record does not hold. kind: 'before_capture' | 'no_result' | 'code_at_statement' | 'edit_line_counts'.
  gaps: [
    { kind: 'before_capture', at: '16:01' },
    { kind: 'no_result', step: 60, askedAt: '16:04' },
    { kind: 'code_at_statement', step: 52, lastVersion: '9f2ad3e' },
    { kind: 'edit_line_counts' },
  ],

  integrity: { ok: true, checkedAt: '16:10', steps: 19, chainHead: 'afb960f9…2861' },
  masking: { detector: 'v2', masked: 0 },

  // What a redacted copy could leave out, by class.
  redaction: { paths: 6, agentWords: 3, failureOutput: 0 },

  // Whether each record the story rests on can be read now. kind:
  // 'decision_file' | 'run' | 'transcript'.
  readable: {
    checkedAt: '16:10',
    records: [
      { kind: 'decision_file', path: '.thruline/decisions/e71c5b0.json', readable: true },
      { kind: 'run', step: 62, capturedBy: 'thruline', readable: true },
      { kind: 'run', step: 46, capturedBy: 'thruline', readable: true },
      { kind: 'run', step: 26, capturedBy: 'thruline', readable: true },
      { kind: 'transcript', agent: 'Claude Code', files: 1, readable: true },
    ],
  },
})
