/* Done-Criteria: the canvas's demo record, as the screen's data contract.
 *
 * Every piece of content the screen shows arrives through this shape; the
 * presentation (sentences, glyphs, colours, joins) is built from it in
 * logic.js renderVals(). DATA.md says where each field must come from in
 * the store. Values are exactly what the canvas shows.
 *
 * `states` holds the demo switcher's variants (Fresh, Reviewing, One dropped,
 * v1 recorded): each sets every line's decision, which candidate links are
 * made, and whether the version is recorded. The base record is Reviewing.
 * Live data carries those facts on `criteria` itself. */

export const DEMO = {
  project: { name: 'checkout-web' },
  change: { title: 'Fix double-applied discount on checkout total' },
  version: { id: 'e71c5b0', commit: '3f9a1c2', clean: true },
  /* steps recorded in this Change */
  steps: 19,
  readAt: '16:04',
  /* when ThruLine began recording this Change */
  recordingSince: '16:01',

  request: { text: 'the checkout total is wrong when a discount code is applied — fix it', by: 'you', at: '16:01', step: 6 },
  agent: { name: 'Claude Code', lastEdit: { at: '16:04', step: 56 } },

  /* the agent's claim that the screen asks a bar for */
  claim: { text: 'Checkout total fixed.', word: 'fixed', at: '16:03', step: 52 },

  /* the part of the request no command can observe; kept as a note */
  notCheckable: { quote: 'fix it', shownBy: [1, 2, 3] },

  /* what counts as done */
  criteria: {
    version: 1,
    recorded: false,
    acceptedAt: null,
    acceptedBy: 'you',
    lines: [
      {
        n: 1, text: 'Applying a discount code once reduces the total by exactly that discount.', kind: 'behaviour',
        source: { kind: 'words', quote: '…wrong when a discount code is applied…' },
        check: 'pytest -q tests/test_totals.py checkout/', namedTest: 'test_single_discount',
        evidence: 'candidate', state: 'accepted', linked: true,
      },
      {
        n: 2, text: 'Applying the same discount code twice does not reduce the total twice.', kind: 'behaviour',
        source: { kind: 'name', quote: 'Fix double-applied discount…' },
        check: 'pytest -q tests/test_totals.py checkout/', namedTest: 'test_double_discount',
        evidence: 'candidate', state: 'proposed', linked: false,
      },
      {
        n: 3, text: 'Checkout totals with no discount code are unchanged.', kind: 'behaviour',
        source: { kind: 'words', quote: 'the checkout total is wrong…' },
        check: 'pytest -q tests/test_totals.py checkout/', namedTest: 'checkout/tests/test_api.py::test_total_plain',
        evidence: 'looked_at', state: 'proposed', linked: false,
      },
      {
        n: 4, text: 'tests/test_totals.py passes on the final version.', kind: 'gate',
        source: { kind: 'check', steps: [22, 42] },
        check: 'pytest -q tests/test_totals.py', ranAtSteps: [22, 42],
        evidence: 'check', state: 'accepted', linked: false,
      },
    ],
  },

  /* the one command that would re-check every line */
  recheck: { command: 'pytest -q tests/test_totals.py checkout/' },

  /* the runs the record holds */
  lastRun: {
    startStep: 42, step: 46, command: 'pytest -q tests/test_totals.py', file: 'tests/test_totals.py',
    version: '9f2ad3e', exit: 0, passed: 4, failed: 0, capturedBy: 'the exec recorder',
  },
  earlierRun: { startStep: 22, step: 26, version: '4b19a0c', exit: 1, passed: 3, failed: 1 },
  pendingRun: { step: 60 },
  /* the edit after the last run, which reopens anything it covered */
  movedEdit: { file: 'checkout/api.py', step: 56 },
  /* the part of the check's scope no finished run has included */
  uncovered: 'checkout/',

  /* what the Change covers, suggested from the recorded edits */
  covers: [
    { path: 'cart/', editSteps: [16, 36], included: true },
    { path: 'checkout/', editSteps: [56], included: true },
    { path: 'tests/', editSteps: [], holdsCheck: true, included: true },
  ],

  /* templates for what counts as done, from Settings › Rules */
  templates: [
    { name: 'Bug fix: reproduces, fixed, no regression', from: 'Rules' },
    { name: 'Tests for every touched file pass on the final version', from: 'Rules' },
    { name: 'No file outside the declared folders changed', from: 'Rules' },
  ],

  storage: { bytes: 1400000000 },

  /* the demo's clock for an acceptance pressed in the gallery (live takes the record's time) */
  demoTimes: { accept: '16:06' },

  states: {
    fresh: { lines: ['proposed', 'proposed', 'proposed', 'proposed'], links: [false, false, false, false], recorded: false },
    reviewing: { lines: ['accepted', 'proposed', 'proposed', 'accepted'], links: [true, false, false, false], recorded: false },
    dropped: { lines: ['accepted', 'accepted', 'dropped', 'accepted'], links: [true, true, false, false], recorded: false },
    recorded: { lines: ['accepted', 'accepted', 'accepted', 'accepted'], links: [true, true, false, false], recorded: true },
  },
}

export default DEMO
