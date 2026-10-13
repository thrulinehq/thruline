/* Onboarding (First run): the canvas's own demo content, as one object — the
 * screen's data contract (DATA.md). Every fact here is exactly what the
 * artboard shows; renderVals() formats it, template.jsx draws it. Wave 2
 * replaces DEMO with the same shape read from the store; nothing here is a
 * real record.
 *
 * `states` holds the variants the canvas's DEMO STATE switcher selects
 * (Not installed · Waiting · Recording · Hooks can't run). In the product the
 * chosen folder's `capture` says which one is true; the switcher is not drawn. */

export const DEMO = Object.freeze({
  /** Where ThruLine keeps what it records on this machine. */
  home: '~/.thruline',

  /** Every folder on this machine where an agent has worked, in list order. */
  folders: [
    {
      id: 'checkout',
      name: 'checkout-web',
      path: '~/code/checkout-web',
      agent: 'Claude Code',
      git: true,
      recorded: false,
      capture: { installed: false, health: 'none' },
      firstSession: {
        startedAt: '16:01',
        agent: 'Claude Code',
        hookVersion: 'v0.4',
        firstAsk: {
          step: 6,
          text: 'the checkout total is wrong when a discount code is applied — fix it',
          proposedChange: 'Fix double-applied discount on checkout total',
        },
      },
    },
    {
      id: 'notes',
      name: 'notes-cli',
      path: '~/code/notes-cli',
      agent: 'Claude Code',
      git: false,
      recorded: false,
      capture: { installed: false, health: 'none' },
      firstSession: { startedAt: '16:01', agent: 'Claude Code', hookVersion: 'v0.4', firstAsk: null },
    },
  ],

  /** What ThruLine keeps on this machine now. */
  storage: { sessions: 0 },

  /** The design's DEMO STATE variants: what changes when the chosen folder's capture is in that state. */
  states: {
    none: { storage: { sessions: 0 } },
    waiting: { storage: { sessions: 0 } },
    recording: { storage: { sessions: 1 } },
    problem: { storage: { sessions: 0 } },
  },

  /** Lines a speaker hook would add when a session starts (the step 2 example). */
  speakerExample: [
    { text: '“Tests pass.”: claimed, and no proof found in the record on e71c5b0.', step: 52 },
    { text: 'checkout/api.py changed after the last run that passed.', step: 56 },
    { text: 'The command at step 60 has no result yet.', step: 60 },
  ],

  /** One statement's proof chain, shown in step 1 as what a first session yields. */
  example: {
    said: { agent: 'Claude Code', quote: 'Tests pass.', step: 52, at: '16:03' },
    restsOn: { step: 46, command: 'pytest -q tests/test_totals.py', passed: 4, failed: 0, exit: 0 },
    ranOn: { step: 38, version: '9f2ad3e', versionsOlder: 1 },
    since: { step: 56, file: 'checkout/api.py', at: '16:04' },
    stops: { version: 'e71c5b0', file: 'checkout/api.py' },
    so: { state: 'notProven', version: 'e71c5b0' },
  },
})
