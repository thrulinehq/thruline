/* Team-View: the canvas's demo content, as the screen's data contract.
 *
 * Every fact the screen shows arrives through one object of this shape —
 * `this.props.data` on a live address, this DEMO in the design gallery.
 * Presentation (joining, glyphs, colours, plural words) stays in logic.js.
 * DATA.md says what each field means and where the real fact comes from.
 */
export const DEMO = Object.freeze({
  project: { name: 'checkout-web', baseBranch: 'main' },
  recording: { projects: 2 },
  readAt: '16:12',
  inbox: { yourMove: 2 },
  plan: { name: 'Team', seats: 5, licensee: 'Acme Ltd' },
  storage: { bytes: 1400000000 },

  // Everyone the team knows of. `seated` is who holds an approving seat on main.
  people: [
    { id: 'sam', name: 'Sam', isYou: true, email: 's••@acme.dev', key: 'ed25519 7f3a…c21e', seated: true },
    { id: 'priya', name: 'Priya', isYou: false, email: 'p••••@acme.dev', key: 'ed25519 91be…04d7', seated: true },
    { id: 'jonas', name: 'Jonas', isYou: false, email: 'j••••@acme.dev', key: 'ed25519 2c4f…e8a0', seated: true },
    { id: 'mateo', name: 'Mateo', isYou: false, email: 'm••••@acme.dev', key: null, seated: false },
    { id: 'ines', name: 'Ines', isYou: false, email: 'i••••@acme.dev', key: null, seated: false },
  ],

  // Groups named in rules; `members` are people ids.
  groups: [{ name: 'payments-owners', members: ['sam', 'priya'] }],

  // Approval rules on the base branch, most specific first; the last has
  // `paths: null` and stands for everything else. `group: null` = anyone with a seat.
  rules: [
    { id: 'checkout', paths: 'checkout/**', group: 'payments-owners', twoApprovals: true },
    { id: 'else', paths: null, group: null, twoApprovals: false },
  ],

  // Decisions recorded with one approval that wait on a second, in the order shown.
  waiting: [
    {
      change: { id: 'round-tax', title: 'Round tax per line, not per order' },
      version: '2c8e4d1',
      commit: '8a0f3b9',
      cleanTree: true,
      decisionFile: 'decisions/2c8e4d1.json',
      rulesMatched: ['checkout', 'else'],
      approvals: [{ by: 'priya', at: '11:05' }],
      next: 'sam',
    },
    {
      change: { id: 'fix-discount', title: 'Fix double-applied discount on checkout total' },
      version: 'e71c5b0',
      commit: '3f9a1c2',
      cleanTree: true,
      decisionFile: 'decisions/e71c5b0.json',
      rulesMatched: ['checkout', 'else'],
      approvals: [{ by: 'sam', at: '16:10' }],
      next: 'priya',
    },
  ],
  changedAfterApproval: [],

  // Shared "What counts as done" templates adopted for this repository.
  templates: [
    { name: 'Bug fix', version: 3, lines: 4, adoptedBy: 'priya', adoptedOn: '12 Sep' },
    { name: 'Payments change', version: 2, lines: 5, adoptedBy: 'sam', adoptedOn: '18 Sep' },
    { name: 'Dependency upgrade', version: 1, lines: 3, adoptedBy: 'jonas', adoptedOn: '22 Sep' },
  ],

  // Lines set aside under team authority. kind: 'waived' | 'not_applicable'.
  setAside: [
    { kind: 'waived', line: 'Stripe webhook signatures verify on SDK v14', version: '3e8b1d4', change: 'Upgrade Stripe SDK to v14', by: 'priya', authority: 'payments-owners', at: 'today 15:20', why: 'Only Stripe’s test mode can reach it.' },
    { kind: 'not_applicable', line: 'Totals round correctly in currencies without cents', version: '2c8e4d1', change: 'Round tax per line, not per order', by: 'sam', authority: 'payments-owners', at: 'today 10:52', why: 'checkout-web sells only in EUR and GBP.' },
    { kind: 'waived', line: 'The coupon banner reads well on a 320 px screen', version: '6a0b2f4', change: 'Reword the coupon banner', by: 'jonas', authority: 'anyone with a seat', at: '23 Sep 14:32', why: 'No visual test exists in this repository yet.' },
  ],

  activity: { monthDeciders: 3 },
})
