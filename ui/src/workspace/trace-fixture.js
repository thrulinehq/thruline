/* The Trace's synthetic session (P6-3) — test and screen-walk data only.
 *
 * NOT a data path: nothing in the app imports this file. `trace.test.js` and
 * `docs/loop/p6/P6-3-render-trace.mjs` adapt it through the real
 * `adapter.adaptSession`, so every card the Trace draws from it is a card the
 * adapter would build from served documents of the same shape. Every value is
 * synthetic; no string here came off a real machine.
 *
 * What it carries, on purpose: every event kind the adapter knows (session
 * start, request, action, result, file change, code version, command, note,
 * hand-back), an ask, a claim with a finding that names a changed file, a
 * promise, two checks — one that ran on an EARLIER version than the last one
 * in the record (the "ran on an earlier version" band) and one whose record
 * carries no exit code (drawn hollow) — and the signed record.
 */

export const TRACE_SESSION_ID = 'synthetic-trace-0001'

const EARLY = '1111111111111111111111111111111111111111'
const FINAL = '2222222222222222222222222222222222222222'
const PREDICATE_TYPE = 'https://thrulinehq.com/attestation/execution-check/v0.1'

export const TRACE_DOCS = Object.freeze({
  key: TRACE_SESSION_ID,
  explain: {
    shape: 'thruline.explain',
    schema_version: 1,
    session: TRACE_SESSION_ID,
    what_happened: [
      { seq: 1, kind: 'session_start', text: 'session started' },
      { seq: 2, kind: 'user_prompt', text: 'Add a handling fee to the shipping rate and make the tests pass.' },
      { seq: 3, kind: 'tool_call', text: 'Read {"file_path":"src/shipping.js"}', tool: 'Read' },
      { seq: 4, kind: 'tool_result', text: 'Read ok' },
      { seq: 5, kind: 'fs_change', text: 'modify src/shipping.js' },
      { seq: 6, kind: 'snapshot', text: 'tree ' + EARLY },
      { seq: 7, kind: 'exec', text: 'npm test', input: 'npm test' },
      { seq: 8, kind: 'tool_result', text: 'Bash FAILED' },
      { seq: 9, kind: 'fs_change', text: 'modify tests/shipping.test.js' },
      { seq: 10, kind: 'snapshot', text: 'tree ' + FINAL },
      { seq: 11, kind: 'note', text: 'Updated the shipping test to match the new fee.' },
      { seq: 12, kind: 'stop', text: 'stopped: Finished' },
    ],
    claims_vs_proof: [],
    promises: [],
    risk_flags: [],
  },
  claims: {
    shape: 'thruline.claims',
    schema_version: 1,
    session: TRACE_SESSION_ID,
    final_tree_hash: FINAL,
    claims: [
      {
        id: 'clm_SYNTH0000000000000000001',
        kind: 'tests_pass',
        text: 'All tests pass.',
        verdict: 'unsupported',
        verifier: 'check_binding',
        scope: 'session',
        extractor: 'completion',
        evidence: ['chk_SYNTH0000000000000000001'],
        findings: [
          {
            detector: 'tests_ran_before_final_edit',
            severity: 'fail',
            detail: 'The tests ran at seq 7, before the change at seq 9.',
            path: 'tests/shipping.test.js',
          },
        ],
      },
    ],
    promises: [
      { id: 'prm_SYNTH0000000000000000001', kind: 'follow_up', status: 'open', text: 'I will add an integration test next.' },
    ],
  },
  asks: {
    asks: [
      { id: 'ask_SYNTH0000000000000000001', kind: 'feature', status: 'partial', text: 'Add a handling fee to the shipping rate.' },
    ],
  },
  receipt: {
    payloadType: 'application/vnd.in-toto+json',
    signatures: [],
    payload: {
      predicateType: PREDICATE_TYPE,
      subject: [{ name: 'synthetic', digest: { gitTree: FINAL } }],
      predicate: {
        session: { id: TRACE_SESSION_ID, event_count: 12 },
        capture: {},
        verifier: { thruline_version: '0.0.0-synthetic', mode: 'local' },
        checks: [
          {
            id: 'chk_SYNTH0000000000000000001',
            category: 'test',
            runner: 'npm',
            exit_code: 1,
            argv_hash: 'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            input_tree_hash: EARLY,
            parsed: { passed: 3, failed: 1, skipped: 0, tests_total: 4 },
          },
          {
            id: 'chk_SYNTH0000000000000000002',
            category: 'lint',
            runner: 'eslint',
            input_tree_hash: FINAL,
          },
        ],
      },
    },
  },
})

export default TRACE_DOCS
