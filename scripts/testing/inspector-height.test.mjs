/* A3-F12 — "Never longer than about two viewports", in a real Chrome.
 *
 * Predeclared (cards/A3/A3-F12.md): on the Change route at altitude `brief`,
 * with a fixture of 12 finding cards and 6 criteria, the Inspector element's
 * `scrollHeight` is ≤ 2 × `innerHeight` at 1512×920 and at 1024×768.
 *
 *   node --test scripts/testing/inspector-height.test.mjs
 *
 * Headless Chrome through `change-page.mjs`, which launches it only through
 * `chrome-launch.mjs` with a throwaway profile. With no Chrome on the
 * machine the test is SKIPPED and says so: that is "could not run", never a
 * pass. Every value below is synthetic.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { chromeAvailable, withChangePage } from './change-page.mjs'

const ID = 'chf12'
const SESSION = 'sf12'
const SIX = ['Uploads retry.', 'Retries back off.', 'Three tries at most.', 'The person is told.', 'Nothing is lost.', 'The log says why.']
const item = (n, observed, extra) => ({
  link_id: 'l' + n, evidence_id: 'e' + n, session_id: SESSION, basis: 'declared', kind: 'check_run', name: 'check ' + n,
  observed, bears: true, applies: true, applies_slug: 'inputs_applies', ...(extra || {}),
})
const CHANGE = {
  id: ID, project_id: 'p1', opened_at: '2026-09-21T09:00:00Z',
  request: { title: 'Retry failed uploads' },
  implementation: { snapshot_id: 'b0c1d2e3f40', bound_at: '2026-09-21T10:00:00Z', repository_identity: 'uploads' },
  accepted_scope: { version: 1, criteria: SIX, allowed_scope: ['uploads/'], check_profile: [], acceptance: { actor: 'sam', accepted_at: '2026-09-21T09:30:00Z' } },
  associations: [{ id: 'a1', session_id: SESSION, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-09-21T10:01:00Z', reason: null }],
  examination: 'none_yet',
}
const CLAIMS = Array.from({ length: 12 }, (_, n) => ({
  id: 'c' + (n + 1), text: `Step ${n + 1} is done and every test passes on it.`, verdict: 'unsupported',
  findings: [{ detector: 'test_run_failed', severity: 'warn', detail: 'A test failed.' }],
}))
const ROUTES = {
  ['/api/changes/' + ID]: { change: CHANGE },
  ['/api/changes/' + ID + '/proposals']: { proposals: null },
  ['/api/changes/' + ID + '/coverage']: {
    coverage: {
      scope_version: 1, link_rows: 5,
      criteria: SIX.map((text, i) => ({
        index: i + 1, text, state: 'evidence',
        items: i === 0 ? [item(1, 'proven'), item(2, 'proven'), item(3, 'not_proven', { failure_line: 'FAILED retry_backoff: expected 3 attempts, saw 1' }), item(4, 'proven'), item(5, 'proven')] : [],
      })),
    },
  },
  ['/api/changes/' + ID + '/inputs']: {},
  ['/api/changes/' + ID + '/applicability']: { items: [] },
  ['/api/explain?session=' + SESSION]: {
    shape: 'thruline.explain', schema_version: 1, session: SESSION,
    what_happened: [{ seq: 1, kind: 'user_prompt', text: 'Make the upload retry.' }, { seq: 2, kind: 'stop', text: 'stopped: Finished' }],
    claims_vs_proof: [], promises: [], risk_flags: [],
  },
  ['/api/claims?session=' + SESSION]: { shape: 'thruline.claims', schema_version: 1, session: SESSION, claims: CLAIMS },
}

const VIEWPORTS = [[1512, 920], [1024, 768]]

test('A3-F12 · at Brief, 12 finding cards and 6 criteria: the Inspector is never longer than two viewports', { skip: !(await chromeAvailable()) && 'could not run: no Chrome on this machine' }, async () => {
  await withChangePage(ROUTES, async ({ show, evaluate }) => {
    for (const [w, h] of VIEWPORTS) {
      await show(`/app.html?altitude=brief#/changes/${ID}`, w, h, "document.querySelectorAll('.ws-change-inspector .ws-verdict-card').length > 0")
      const m = await evaluate(`(() => {
        const el = document.querySelector('.ws-change-inspector')
        return { scrollHeight: el.scrollHeight, innerHeight: window.innerHeight, cards: el.querySelectorAll('.ws-verdict-card').length }
      })()`)
      console.log(`${w}x${h}: Inspector scrollHeight ${m.scrollHeight} px, 2 x innerHeight ${2 * m.innerHeight} px, cards in the Inspector ${m.cards}`)
      assert.equal(m.cards, 12, `${w}×${h}: the fixture's twelve cards did not all reach the Inspector`)
      assert.ok(m.scrollHeight <= 2 * m.innerHeight, `${w}×${h}: the Inspector is ${m.scrollHeight} px, more than two viewports (${2 * m.innerHeight} px)`)
    }
  })
})
