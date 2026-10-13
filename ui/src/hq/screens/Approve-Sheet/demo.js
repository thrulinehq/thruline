/* Approve-Sheet: the canvas's demo record, as the screen's data contract.
 *
 * Every piece of content the screen shows arrives through this shape; the
 * presentation (sentences, glyphs, colours, joins) is built from it in
 * logic.js renderVals(). DATA.md says where each field must come from in
 * the store. Values are exactly what the canvas shows.
 *
 * `states` holds the demo switcher's variants, laid over the base in the
 * design gallery only: `free` (no licence: approving needs Pro) and `moves` (the code
 * moved while the sheet was open). Live data carries its own `licence` and,
 * when the code has moved, its own `moved`. */

export const DEMO = {
  project: { name: 'checkout-web', worktree: '~/code/checkout-web' },
  change: { title: 'Fix double-applied discount on checkout total', sessions: 1 },
  version: { id: 'e71c5b0', commit: '3f9a1c2', clean: true },
  session: { day: 'today', start: '16:01', end: '16:09' },
  agent: { name: 'Claude Code', lastEditAt: '16:04' },
  readAt: '16:09',
  /* when the Change became Ready for approval (the re-check finished) */
  readyAt: '16:09',

  request: { text: 'the checkout total is wrong when a discount code is applied — fix it', step: 6 },

  /* what the agent said; `proof` says which kind of row each one is */
  statements: [
    { text: 'Checkout total fixed.', step: 52, at: '16:03', proof: 'recorded' },
    { text: 'Tests pass.', step: 52, at: '16:03', proof: 'proven' },
  ],

  /* what counts as done, as accepted */
  criteria: {
    version: 'v1',
    acceptedAt: '16:06',
    acceptedBy: 'you',
    setAside: 0,
    lines: [
      { n: 1, text: 'Applying a discount code once reduces the total by exactly that discount.', kind: 'behaviour', link: 'person', state: 'proven' },
      { n: 2, text: 'Applying the same discount code twice does not reduce the total twice.', kind: 'behaviour', link: 'person', state: 'proven' },
      { n: 3, text: 'Checkout totals with no discount code are unchanged.', kind: 'behaviour', link: 'person', state: 'proven' },
      { n: 4, text: 'tests/test_totals.py passes on the final version.', kind: 'gate', link: 'accepted_check', state: 'proven' },
    ],
  },

  /* the run every line rests on */
  run: {
    step: 62,
    command: 'pytest -q tests/test_totals.py checkout/',
    scope: ['tests/test_totals.py', 'checkout/'],
    version: 'e71c5b0',
    exit: 0,
    passed: 5,
    failed: 0,
    durationS: 0.42,
    at: '16:09',
    cwd: '~/code/checkout-web',
    capturedBy: 'ThruLine',
  },

  /* the trace map behind the sheet */
  trace: {
    lanes: { request: 1, work: 3, execution: 4, claims: 4 },
    versions: ['4b19a0c', '9f2ad3e'],
    edits: [
      { step: 16, file: 'cart/totals.py' },
      { step: 36, file: 'cart/totals.py' },
      { step: 56, file: 'checkout/api.py' },
    ],
    runs: [
      { step: 26, passed: 3, failed: 1, exit: 1 },
      { step: 46, passed: 4, failed: 0, exit: 0 },
      { step: 62, passed: 5, failed: 0, exit: 0 },
    ],
  },

  /* a command with no result yet, which does not hold the approval up */
  pending: { step: 60, command: 'rm -rf .pytest_cache && pytest -q' },

  /* who is approving: the name the box starts with */
  reviewer: { name: 'Sam' },

  /* decisions already recorded on this version, oldest first */
  decisions: [],

  /* the demo's clock for acts pressed in the gallery (live acts take the record's time) */
  demoTimes: { approve: '16:10', withdraw: '16:14' },

  /* Free or Pro, nothing else (CTO C21-b): approving a version needs Pro */
  licence: { plan: 'pro' },
  storage: { bytes: 1400000000 },

  moved: null,

  states: {
    free: { licence: { plan: 'free' } },
    moves: { moved: { version: 'a1b2c3d', files: ['checkout/api.py'], step: 64, at: '16:11' } },
  },
}

export default DEMO
