/* What-Changed: the canvas's demo content, as the screen's data contract.
 *
 * Every fact the screen shows arrives through this one object (or through
 * `props.data` of the same shape, in the live product). These are typed facts
 * named for what they are; how a fact reads on screen (counts, sentences,
 * hunk headers, glyphs, colours) is decided in logic.js `renderVals()`.
 * DATA.md says, for every field, what it means and which recorded fact it must
 * come from.
 *
 * Steps are the record's step numbers within their own session. Times are the
 * local clock time of the record, HH:MM. Versions are short commit-like ids.
 * The diff lines are the canvas's illustrative lines; the record does not
 * hold context lines yet (see DATA.md, `edits[].hunks[].lines`).
 */
const ctx = (text) => ({ op: 'context', text })
const add = (text) => ({ op: 'add', text })
const del = (text) => ({ op: 'del', text })

export const DEMO = {
  project: { id: 'checkout-web', name: 'checkout-web', worktree: '~/code/checkout-web' },
  machine: { recordsBytes: 1400000000 },
  view: { readAt: '16:04' },
  change: {
    id: 'change-checkout-discount',
    title: 'Fix double-applied discount on checkout total',
    recording: true,
    followingLive: true,
    /* The six stages, in order. status: done | current | not_started | not_reached. */
    stages: [
      { id: 'define', status: 'not_started' },
      { id: 'prepare', status: 'done' },
      { id: 'build', status: 'current' },
      { id: 'verify', status: 'not_started' },
      { id: 'review', status: 'not_reached' },
      { id: 'approve', status: 'not_reached' },
    ],
  },
  /* The version the screen speaks for: the newest recorded version. */
  version: { id: 'e71c5b0', commit: '3f9a1c2', unsavedChanges: false },

  /* Recorded versions of the code, oldest first; the last is the current one.
   * madeByStep: the edit that made it. When the first version was made by an
   * edit, the code before that edit was never recorded. */
  versions: [
    { id: '4b19a0c', madeByStep: 16, recordedAtStep: null },
    { id: '9f2ad3e', madeByStep: 36, recordedAtStep: 38 },
    { id: 'e71c5b0', madeByStep: 56, recordedAtStep: 58 },
  ],

  /* Every file this Change edited or ran. kind: code | tests | config | generated. */
  files: [
    { path: 'cart/totals.py', kind: 'code' },
    { path: 'checkout/api.py', kind: 'code' },
    { path: 'tests/test_totals.py', kind: 'tests' },
  ],
  selectedFile: 'checkout/api.py',

  /* Every edit, line by line, in step order. `makesVersion` is the version the
   * code became after it. A hunk's `oldStart` is numbered against the code
   * just before this edit; `newStart` against the code just after it. */
  edits: [
    { id: 'edit-16', step: 16, sessionId: 'session-1', time: null, file: 'cart/totals.py', op: 'edit', makesVersion: '4b19a0c',
      hunks: [
        { oldStart: 12, newStart: 12, context: 'def apply_discount(subtotal, code):', lines: [
          ctx('    discount = lookup_discount(code)'),
          ctx('    if discount is None:'),
          ctx('        return subtotal'),
          add('    discount = min(discount, subtotal)'),
          ctx('    return subtotal - discount'),
        ] },
      ] },
    { id: 'edit-36', step: 36, sessionId: 'session-1', time: null, file: 'cart/totals.py', op: 'edit', makesVersion: '9f2ad3e',
      hunks: [
        { oldStart: 25, newStart: 25, context: 'def compute_total(cart, code):', lines: [
          ctx('    subtotal = sum(i.price * i.qty for i in cart.items)'),
          ctx('    total = apply_discount(subtotal, code)'),
          del('    if code:'),
          del('        total = apply_discount(total, code)'),
          ctx('    return round_money(total)'),
        ] },
      ] },
    { id: 'edit-56', step: 56, sessionId: 'session-1', time: '16:04', file: 'checkout/api.py', op: 'edit', makesVersion: 'e71c5b0',
      hunks: [
        { oldStart: 41, newStart: 41, context: 'def checkout_total(request):', lines: [
          ctx('    cart = load_cart(request.session_id)'),
          ctx('    code = request.json.get("discount_code")'),
          del('    total = compute_total(cart, code) - lookup_discount(code)'),
          add('    # compute_total already applies the code once'),
          add('    total = compute_total(cart, code)'),
          ctx('    tax = tax_for(cart.region, total)'),
          ctx('    return jsonify('),
          ctx('        total=round_money(total + tax),'),
          ctx('    )'),
        ] },
        { oldStart: 59, newStart: 60, context: 'def order_summary(request):', lines: [
          ctx('    cart = load_cart(request.session_id)'),
          ctx('    code = request.json.get("discount_code")'),
          ctx('    summary = build_summary(cart)'),
          ctx('    summary["discount"] = lookup_discount(code)'),
          add('    summary["total"] = compute_total(cart, code)'),
          ctx('    return jsonify(summary)'),
        ] },
      ] },
  ],

  /* Every command run in this Change, in step order. status: finished |
   * no_result | running. A finished run held when it exited 0 (or, with no
   * exit code recorded, when its tally shows no failure). */
  runs: [
    { id: 'run-26', step: 26, sessionId: 'session-1', time: null, version: '4b19a0c', command: 'pytest -q tests/test_totals.py', scope: ['tests/test_totals.py'],
      status: 'finished', exitCode: 1, passed: 3, failed: 1, durationMs: null, capturedBy: 'thruline', declaresPaths: false },
    { id: 'run-46', step: 46, sessionId: 'session-1', time: '16:03', version: '9f2ad3e', command: 'pytest -q tests/test_totals.py', scope: ['tests/test_totals.py'],
      status: 'finished', exitCode: 0, passed: 4, failed: 0, durationMs: null, capturedBy: 'thruline', declaresPaths: false },
    { id: 'run-60', step: 60, sessionId: 'session-1', time: null, version: 'e71c5b0', command: 'rm -rf .pytest_cache && pytest -q', scope: [],
      status: 'no_result', exitCode: null, passed: null, failed: null, durationMs: null, capturedBy: 'thruline', declaresPaths: false },
  ],

  /* The statements the agents made in this Change; `statementId` is the one
   * this screen answers for. versionBefore: the last version recorded before
   * it was said. */
  statements: [
    { id: 'stmt-52-tests', messageId: 'said-52', text: 'Tests pass.', claimKind: 'tests_pass', verifier: true,
      step: 52, time: '16:03', sessionId: 'session-1', agent: 'Claude Code', versionBefore: '9f2ad3e',
      proof: { verdict: 'not_proven', restsOn: { kind: 'run', runId: 'run-46', basis: 'cited_by_agent' } } },
  ],
  statementId: 'stmt-52-tests',

  /* The one check that would cover the changed file on this version. */
  recheck: {
    runner: 'pytest -q',
    scope: ['tests/test_totals.py', 'checkout/'],
    on: 'e71c5b0',
    permission: 'ask_once_per_project',
  },

  /* Demo only: what the canvas shows after "Check it again" finishes. The
   * live product never fabricates this; a real run arrives in `runs`. */
  afterCheck: {
    run: { id: 'run-62', step: 62, sessionId: 'session-1', time: '16:09', version: 'e71c5b0', command: 'pytest -q tests/test_totals.py checkout/',
      scope: ['tests/test_totals.py', 'checkout/'], status: 'finished', exitCode: null, passed: 5, failed: 0, durationMs: 420,
      capturedBy: 'thruline', declaresPaths: false, matchedBy: 'thruline', illustrative: true },
    statementVerdict: 'proven',
    stages: [
      { id: 'define', status: 'done' },
      { id: 'prepare', status: 'done' },
      { id: 'build', status: 'done' },
      { id: 'verify', status: 'done' },
      { id: 'review', status: 'current' },
      { id: 'approve', status: 'not_started' },
    ],
  },
}
