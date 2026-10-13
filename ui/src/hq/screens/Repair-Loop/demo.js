/* Repair-Loop: the canvas's demo record, as the screen's data contract.
 *
 * Every piece of content the screen shows arrives through this shape; the
 * presentation (sentences, glyphs, colours, joins) is built from it in
 * logic.js renderVals(). DATA.md says where each field must come from in
 * the store. Values are exactly what the canvas shows.
 *
 * `states` holds the canvas's demo variants. In the design gallery the demo
 * switcher picks one and it is laid over the base; live data carries one
 * `readAt` / `now` / `steps` / `run` of its own at the top level instead. */

const BASE_OUTPUT = [
  'collecting …',
  'collected 5 items',
  'tests/test_totals.py ....                       [ 80%]',
]

export const DEMO = {
  project: { name: 'checkout-web', dir: '~/checkout-web' },
  change: { title: 'Fix double-applied discount on checkout total' },
  version: { id: 'e71c5b0', commit: '3f9a1c2', clean: true },
  session: { day: 'today', start: '16:01', end: '16:09' },
  agent: { name: 'Claude Code' },
  request: { text: 'the checkout total is wrong when a discount code is applied — fix it' },

  /* the statement whose proof stops at ⑤ */
  statement: {
    text: 'Tests pass.',
    said: 'Checkout total fixed. Tests pass.',
    step: 52,
    at: '16:03',
    lastVersionBefore: '9f2ad3e',
  },

  /* the last run that held: what ② rests on until a re-check lands */
  heldRun: {
    step: 46,
    command: 'pytest -q tests/test_totals.py',
    version: '9f2ad3e',
    versionsBehind: 1,
    exit: 0,
    passed: 4,
    failed: 0,
  },

  /* the edit after that run, where ④ moved */
  movedEdit: { verb: 'modify', file: 'checkout/api.py', step: 56 },

  /* what Check it again would run */
  check: {
    scope: ['tests/test_totals.py', 'checkout/'],
    covers: 'checkout/',
    command: 'pytest -q tests/test_totals.py checkout/',
    terminalCommand: 'thruline check --scope tests/test_totals.py checkout/ -- pytest -q',
    environment: '~/checkout-web/.venv',
    consentGranted: true,
  },

  /* the concern Tell the agent is about */
  concern: { raisedAt: '16:04' },

  /* what counts as done: how many accepted lines rest on this run */
  criteria: { version: 'v1', lines: 4 },

  record: { statements: 2, notProven: 1 },
  storage: { bytes: 1400000000 },

  /* Illustrative tallies the Honest outcomes list shows for rows that did not happen. */
  outcomeExamples: { passed: { passed: 5, failed: 0 }, failed: { passed: 4, failed: 1 } },

  states: {
    running: {
      readAt: '16:09',
      now: '16:09',
      steps: 19,
      run: { state: 'running', version: 'e71c5b0', elapsedS: 3, output: BASE_OUTPUT.concat(['checkout/tests/test_api.py ']) },
    },
    passed: {
      readAt: '16:09',
      now: '16:10',
      steps: 20,
      run: {
        state: 'passed', version: 'e71c5b0', step: 62, at: '16:09', exit: 0, passed: 5, failed: 0, durationS: 0.42,
        output: BASE_OUTPUT.concat(['checkout/tests/test_api.py .                    [100%]', '5 passed in 0.42s']),
        failures: [],
      },
    },
    failed: {
      readAt: '16:09',
      now: '16:10',
      steps: 20,
      run: {
        state: 'failed', version: 'e71c5b0', step: 62, at: '16:09', exit: 1, passed: 4, failed: 1, durationS: 0.44,
        output: BASE_OUTPUT.concat([
          'checkout/tests/test_api.py F                    [100%]',
          'FAILED checkout/tests/test_api.py::test_total_with_discount — assert 90 == 95',
          '1 failed, 4 passed in 0.44s',
        ]),
        failures: [{ line: 4, test: 'checkout/tests/test_api.py::test_total_with_discount', assertion: 'assert 90 == 95' }],
      },
    },
    stopped: {
      readAt: '16:09',
      now: '16:10',
      steps: 19,
      run: {
        state: 'stopped', version: 'e71c5b0', at: '16:09', elapsedS: 3, cleanupVerified: true, cancelledRecords: 1,
        output: BASE_OUTPUT.concat(['checkout/tests/test_api.py ']),
      },
    },
    na: {
      readAt: '16:04',
      now: '16:04',
      steps: 19,
      run: { state: 'unavailable' },
    },
  },
}

export default DEMO
