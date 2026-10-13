/* Record-Over-Time: the canvas's demo content, as the screen's data contract.
 *
 * Every fact the screen shows arrives through one object of this shape —
 * `this.props.data` on a live address, this DEMO in the design gallery.
 * Presentation (sentences, plural words, glyphs, colours, chart geometry)
 * stays in logic.js. DATA.md says what each field means, which aggregate the
 * store must compute, over which window, and whether it has it today.
 *
 * The canvas's demo record is illustrative and staged so that every count
 * agrees with every other.
 */

// A tally: the statements agents made in one window, one project, one agent,
// and where each one's proof stands now.
const tally = (n, p, r, fa, fx, r3, r6, re, reP, pass, fail, stop, b, bP, bN) => ({
  statements: n,
  proven: p,
  needsRecheck: r,
  noProofFound: fa,
  rerunFailed: fx,
  recordedStopsAtRanOn: r3,
  recordedStopsAtSo: r6,
  reopened: re,
  reproven: reP,
  reruns: { passed: pass, failed: fail, stopped: stop },
  beforeAnyRun: { total: b, laterProven: bP, stillNothing: bN },
})

const DEMO_PROJECT_STATEMENT = tally(1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1)
const DEMO_DAY = 68 // 21 days before today
const oneOn = (day) => Array.from({ length: 90 }, (_, i) => (i === day ? 1 : 0))

export const DEMO = Object.freeze({
  today: '2026-09-26',
  readAt: '16:10',
  inbox: { yourMove: 2 },
  storage: { bytes: 1400000000 },

  // The windows the page offers, in days, ending today.
  windows: [7, 30, 90],

  // Every project on this machine that ThruLine records or has recorded.
  projects: [
    { id: 'checkout', name: 'checkout-web', recording: true, changes: 3, dismissedChanges: 0, lastRecordedOn: '2026-09-26',
      lifetime: { statements: 171, byAgent: { cc: 120, cx: 51 }, lastStatementOn: '2026-09-26' } },
    { id: 'demo', name: 'demo', recording: false, changes: 0, dismissedChanges: 1, lastRecordedOn: '2026-09-05',
      lifetime: { statements: 1, byAgent: { cc: 1, cx: 0 }, lastStatementOn: '2026-09-05' } },
    { id: 'notes', name: 'notes-cli', recording: true, changes: 0, dismissedChanges: 0, lastRecordedOn: null,
      lifetime: { statements: 0, byAgent: { cc: 0, cx: 0 }, lastStatementOn: null } },
  ],

  // How each agent is captured on this machine.
  capture: {
    claudeCode: { hookVersion: 'v0.4' },
    runCapture: true,
    codex: { answering: false },
  },

  // tallies[window][projectId][agentId]; an absent entry means nothing was recorded.
  tallies: {
    7: {
      checkout: {
        cc: tally(12, 8, 2, 1, 0, 0, 1, 4, 2, 2, 0, 0, 1, 0, 1),
        cx: tally(4, 2, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0),
      },
    },
    30: {
      checkout: {
        cc: tally(45, 31, 7, 2, 1, 2, 2, 12, 4, 4, 1, 1, 3, 2, 1),
        cx: tally(18, 10, 3, 2, 0, 1, 2, 5, 2, 2, 0, 1, 3, 1, 2),
      },
      demo: { cc: DEMO_PROJECT_STATEMENT },
    },
    90: {
      checkout: {
        cc: tally(120, 81, 18, 7, 3, 6, 5, 34, 13, 13, 3, 4, 11, 5, 6),
        cx: tally(51, 28, 9, 5, 1, 4, 4, 15, 5, 5, 1, 2, 8, 3, 5),
      },
      demo: { cc: DEMO_PROJECT_STATEMENT },
    },
  },

  // Median minutes from a statement to its proof, among the statements proven
  // in the window: medianMinutesToProof[window][scope][agent], where scope is
  // 'all' or a project id and agent is 'all', 'cc' or 'cx'. Absent = none proven.
  medianMinutesToProof: {
    7: { all: { all: 5, cc: 4, cx: 9 }, checkout: { all: 5, cc: 4, cx: 9 } },
    30: { all: { all: 7, cc: 6, cx: 12 }, checkout: { all: 7, cc: 6, cx: 12 } },
    90: { all: { all: 8, cc: 7, cx: 13 }, checkout: { all: 8, cc: 7, cx: 13 } },
  },

  // Statements per day for the longest window: daily[projectId][agentId] holds
  // two arrays of 90, index 0 = 89 days ago, 89 = today. `statements` = made
  // that day; `proven` = of them, proven on their version. null = that agent was
  // not recorded that day (never a zero). Absent = nothing recorded.
  daily: {
    checkout: {
      cc: {
        statements: [1,3,1,1,1,0,1,3,2,3,2,1,1,1,0,2,1,0,2,1,0,0,3,2,2,1,1,1,1,1,1,0,1,0,0,2,2,0,2,0,0,1,1,3,3,2,3,1,0,0,2,1,3,2,0,0,1,1,2,2,3,2,0,5,2,0,1,1,0,1,2,1,2,2,1,0,0,2,4,2,1,0,1,3,2,2,1,1,1,2],
        proven: [1,1,0,1,0,0,0,0,2,0,2,1,1,1,0,2,1,0,2,1,0,0,2,1,1,1,1,1,1,1,1,0,1,0,0,2,2,0,2,0,0,1,1,1,0,1,1,1,0,0,2,1,1,2,0,0,1,0,2,2,2,2,0,0,1,0,1,1,0,1,1,1,2,2,1,0,0,1,4,1,1,0,1,1,1,2,1,1,1,1],
      },
      cx: {
        statements: [1,0,0,1,0,0,1,0,0,1,0,1,0,0,0,1,1,0,0,0,0,0,0,2,0,0,0,0,3,1,1,2,0,0,0,2,1,1,0,1,1,0,0,1,0,1,2,0,0,0,0,1,1,0,0,1,1,1,2,0,1,0,0,2,0,2,0,0,0,0,1,1,0,1,1,2,0,1,0,1,0,1,0,0,0,0,2,0,2,null],
        proven: [1,0,0,0,0,0,1,0,0,0,0,0,0,0,0,1,1,0,0,0,0,0,0,1,0,0,0,0,1,0,0,0,0,0,0,2,0,0,0,1,0,0,0,1,0,1,1,0,0,0,0,1,0,0,0,1,1,1,2,0,1,0,0,1,0,0,0,0,0,0,0,1,0,0,0,2,0,1,0,1,0,1,0,0,0,0,2,0,0,null],
      },
    },
    demo: {
      cc: { statements: oneOn(DEMO_DAY), proven: oneOn(-1) },
    },
  },
})
