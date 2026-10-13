/* Inbox: the canvas's own demo content, as one object — the screen's data
 * contract (DATA.md). Every fact here is exactly what the artboard shows;
 * renderVals() formats it, template.jsx draws it. Wave 2 replaces DEMO with
 * the same shape read from the store; nothing here is a real record.
 *
 * Times are the record's local clock as the canvas prints them ("16:04").
 * Versions are the short (7-character) version id. */

export const DEMO = Object.freeze({
  /** When this screen last read the record ("now" for anything the person does). */
  readAt: '16:10',

  /** Every project on this machine, in sidebar order. */
  projects: [
    { id: 'checkout-web', name: 'checkout-web', recording: true, lastRecordedOn: null, changeCount: 4, dismissedCount: 0 },
    { id: 'demo', name: 'demo', recording: false, lastRecordedOn: '5 Sep', changeCount: 2, dismissedCount: 1 },
    { id: 'notes-cli', name: 'notes-cli', recording: true, lastRecordedOn: null, changeCount: 0, dismissedCount: 0 },
  ],

  /** What is being recorded on this machine, and what is not. */
  capture: {
    claudeCode: { on: true, hookVersion: 'v0.4' },
    runCapture: { on: true },
    codex: { answering: false },
  },

  /** What ThruLine keeps on this machine (bytes, as /api/storage measures it). */
  storage: { bytes: 1_400_000_000 },

  /** The Change whose rows explain the "counted by Change" headline. */
  countedBy: { change: 'disc', needs: ['check', 'prompt'] },

  /** Every Change in view, by id. `move` is whose move it is. */
  changes: {
    post: {
      id: 'post',
      title: 'Add postcode validation to the address form',
      project: 'checkout-web',
      state: 'ready',
      move: 'you',
      version: '7c01d9e',
      since: '15:48',
      sessionEndedAt: '15:48',
      criteria: {
        version: null,
        lines: [
          { n: 1, state: 'proven', gate: false },
          { n: 2, state: 'proven', gate: false },
          { n: 3, state: 'proven', gate: false },
        ],
      },
      chain: { holds: true, links: 6 },
    },
    disc: {
      id: 'disc',
      title: 'Fix double-applied discount on checkout total',
      shortName: 'the discount fix',
      project: 'checkout-web',
      state: 'recheck',
      move: 'you',
      version: 'e71c5b0',
      since: '16:04',
      sessions: 1,
      agent: 'Claude Code',
      needEvidence: 4,
      changedFile: 'checkout/api.py',
      claim: { agent: 'Claude Code', quotes: ['Checkout total fixed.', 'Tests pass.'], at: '16:03', step: 52 },
      restsOn: { lines: 3, step: 46, version: '9f2ad3e' },
      changedAtStep: 56,
      uncovered: { scope: 'checkout/', line: 3 },
      criteria: {
        version: 'v1',
        lines: [
          { n: 1, state: 'recheck', gate: false },
          { n: 2, state: 'recheck', gate: false },
          { n: 3, state: 'progress', gate: false },
          { n: 4, state: 'recheck', gate: true },
        ],
      },
      stops: [
        { link: 5, name: 'RE-CHECKED', lines: 3 },
        { link: 2, name: 'RESTS ON', lines: 1 },
      ],
      thread: [
        { version: '4b19a0c', now: false, events: [{ step: 26, kind: 'run', result: 'failed' }] },
        { version: '9f2ad3e', now: false, events: [{ step: 46, kind: 'run', result: 'passed' }, { step: 52, kind: 'claim' }] },
        { version: 'e71c5b0', now: true, ran: false, events: [{ step: 56, kind: 'edit', file: 'checkout/api.py' }, { step: 60, kind: 'command', result: null }] },
      ],
      check: { command: 'pytest -q tests/test_totals.py checkout/', reachesLines: 4, asks: 'once for this project' },
    },
    stripe: {
      id: 'stripe',
      title: 'Upgrade Stripe SDK to v14',
      project: 'checkout-web',
      state: 'notProven',
      move: 'agent',
      version: '3e8b1d4',
      since: '16:02',
      agent: 'Claude Code',
      noteCopiedAt: '16:02',
      laterWork: false,
      stops: [{ link: 2, name: 'RESTS ON', lines: 1 }],
    },
  },

  /** A command the agent asked permission for and the person has not answered. */
  prompts: [
    {
      id: 'prompt',
      change: 'disc',
      agent: 'Claude Code',
      command: 'rm -rf .pytest_cache && pytest -q',
      effect: 'removes-directory',
      step: 60,
      requestedAt: '16:04',
      waitingMinutes: 6,
      scope: 'checkout/',
    },
  ],

  /** Sessions recording now. */
  sessions: [
    {
      id: 'live',
      agent: 'Claude Code',
      project: 'checkout-web',
      change: 'disc',
      startedAt: '16:01',
      hookVersion: 'v0.4',
      hookInstalledAt: '16:01',
      steps: 19,
      lastStepAt: '16:04',
      waitingAtStep: 60,
    },
  ],

  /** Things that need nothing from anyone. */
  nothingNeeded: {
    items: [
      { kind: 'approved', project: 'checkout-web', title: 'Round line-item prices to cents', version: '5a91c3e', by: 'you', on: 'yesterday' },
      { kind: 'approved', project: 'demo', title: 'Seed the demo store with sample carts', version: '0e4b7d2', by: 'you', on: '5 Sep' },
      { kind: 'dismissed', project: 'demo', state: 'notProven', notProvenStatements: 1 },
    ],
    destructive: [{ command: 'rm -rf .venv', change: 'Upgrade Stripe SDK to v14', at: '15:58' }],
  },
})
