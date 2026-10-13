/* PR-Face: the canvas's demo content, as the screen's data contract.
 *
 * `renderVals()` reads `this.props.data || DEMO`. A live page is one object of
 * this shape built by `from-change.js` from the ONE Change snapshot. The
 * field-by-field contract is DATA.md beside this file.
 *
 * `note` is the comment ThruLine would post, in the note shape markdown.js
 * defines (`composeNote` builds a live one; `markdownOf` writes any one as
 * Markdown). Here it is the canvas's 16:06 note exactly as drawn, so the design
 * gallery renders through the same loops the live page uses. The canvas's
 * other content — the 16:09 panel, the three version↔commit forms, the check
 * run, the trailer and badges, the six contrasts — is drawn as it stands only
 * in the gallery (`isDemo`). A live page draws its own edited-in-place panel,
 * check run, trailer and badge words from the record (C30 R-PR, DATA.md), and
 * never the six contrasts.
 */

const V = { code: 'e71c5b0' }
const V9 = { code: '9f2ad3e' }
const API = { code: 'checkout/api.py' }
const CHECKOUT = { code: 'checkout/' }

const NOTE = {
  marker: '<!-- thruline:note change=<change-id> version=e71c5b0 criteria=v1 schema=note/1 -->',
  title: [{ strong: ['ThruLine · Fix double-applied discount on checkout total'] }, ' · version ', V],
  lines: [
    [{ strong: [{ glyph: '◭' }, ' Needs a re-check — 4 things still need evidence before ', V, ' can be approved.'] }],
    ['3 rest on step 46, which ran on ', V9, ' before ', API, ' changed at step 56. No run has included ', CHECKOUT, ' for line 3.'],
    [{ strong: ['To settle it:'] }, ' ', { code: 'pytest -q tests/test_totals.py checkout/' }, ' on ', V, ', or ', { em: 'Check it again' }, ' in ThruLine.'],
    [{ strong: ['For approval:'] }, ' approval of ', V, ' waits on all 4. Not approved.'],
    ['Where the proof stops: ', { glyph: '⑤' }, ' RE-CHECKED ×3 · ', { glyph: '②' }, ' RESTS ON ×1'],
  ],
  table: {
    head: [[], ['What counts as done'], ['On ', V], ['Proof, link by link'], ['Stops at']],
    rows: [
      [[{ glyph: '◭' }], ['Applying a discount code once reduces the total by exactly that discount.'], ['Needs a re-check'],
        ['accepted 16:06 → step 46, 4 passed, 0 failed, on ', V9, ' → ', API, ' changed (step 56) → nothing on ', V],
        [{ stop: '⑤', label: 'link 5, re-checked' }]],
      [[{ glyph: '◭' }], ['Applying the same discount code twice does not reduce the total twice.'], ['Needs a re-check'],
        ['same run as line 1'],
        [{ stop: '⑤', label: 'link 5, re-checked' }]],
      [[{ glyph: '◐' }], ['Checkout totals with no discount code are unchanged.'], ['In progress'],
        ['accepted 16:06 → no run has included ', CHECKOUT],
        [{ stop: '②', label: 'link 2, rests on' }]],
      [[{ glyph: '◭' }], [{ em: '(gate)' }, ' ', { code: 'tests/test_totals.py' }, ' passes on the final version.'], ['Needs a re-check'],
        ['accepted 16:06 → step 46 (the check you accepted) on ', V9, ' → ', API, ' changed (step 56) → nothing on ', V],
        [{ stop: '⑤', label: 'link 5, re-checked' }]],
    ],
  },
  tableNote: null,
  tableAfter: null,
  layers: [
    {
      open: true,
      summary: ['What the agent said, and what the record shows (2 statements · 1 also said, not checked)'],
      items: [
        [{ glyph: '○' }, ' ❝Tests pass.❞ (step 52, 16:03) — Not proven on ', V, '. It rests on step 46 on ', V9, '; ', API, ' changed after it; nothing has run on ', V, '. The last version ThruLine recorded before this was said is ', V9, '.'],
        [{ glyph: '▪' }, ' ❝Checkout total fixed.❞ (step 52) — Recorded. The record can’t check “fixed” by itself; the table above decides it.'],
      ],
    },
    {
      open: true,
      summary: ['Runs, failures first — 2 finished, none on ', V, ' · 1 with no result yet'],
      items: [
        [{ code: 'pytest -q tests/test_totals.py' }, ' · exit 1 · 1 failed, 3 passed · on ', { code: '4b19a0c' }, ' · step 26 · 16:02'],
        [{ code: 'pytest -q tests/test_totals.py' }, ' · exit 0 · 4 passed, 0 failed · on ', V9, ' · step 46 · 16:03 · captured by ThruLine'],
        [{ glyph: '⋯' }, ' ', { code: 'rm -rf .pytest_cache && pytest -q' }, ' · no result yet · requested at step 60 · 16:04'],
      ],
    },
    { open: false, summary: ['What was looked at · what was not recorded'], items: [] },
    { open: false, summary: ['Prepared for the agent — not sent'], items: [] },
  ],
  version: [V, ' = commit ', { code: '3f9a1c2' }, ' with no unsaved changes.'],
  footer: [
    'Generated locally by ThruLine · nothing was uploaded · Describes ', V,
    ' as of 2026-09-26 16:06 +00:00; all times above are that day · Every line comes from typed fields in a local, hash-chained record; numbers never come from the agent’s prose · Advisory: this never blocks a merge and is not a guarantee that the code is correct · This file is internally consistent and unchanged since export; it does not show who made it · masked: 0 · detector v2 · best-effort · Check it: ',
    { code: 'thruline verify thruline-packet-<change>-e71c5b0.md' },
  ],
}

export const DEMO = {
  live: false,
  board: { crumbKind: 'screen', crumb: 'demo: checkout-web' },
  host: {
    title: 'Fix double-applied discount on checkout total',
    number: 482,
    pill: 'Open',
    author: { initial: 'a', name: 'ashwin' },
    base: 'main',
    head: '3f9a1c2',
    project: null,
    version: null,
  },
  comment: { postedAt: '16:06', readAt: null },
  note: NOTE,
  markdown: null,
}

export default DEMO
