/* A3-F15 — "The first-time test passes at 390 px", in a real Chrome.
 *
 * Predeclared (cards/A3/A3-F15.md): at 390×844, on the Change route at
 * altitude `brief`, for each of four Changes — proven, needs a re-check, not
 * proven, and nothing accepted —
 *   (a) the title, the one sentence and the state line are visible (a
 *       non-zero box, not `display:none`) inside the first viewport;
 *   (b) the page has no sideways scroll (`scrollWidth <= innerWidth`);
 *   (c) no `<canvas>` and no `.ws-canvas` is drawn;
 *   (d) the Inspector's one primary control is there, and either focusable
 *       or off with a reason a person can read;
 * and the version line is present. That is UX-DOCTRINE's three questions:
 * is anything wrong (a), what first (d), why believe it (the version).
 *
 *   node --test scripts/testing/brief-390.test.mjs
 *
 * A guard, not a fix: the product already keeps the headline at this width
 * and draws no graph on the Change Brief. Headless Chrome through
 * `change-page.mjs`, which launches it only through `chrome-launch.mjs` with a
 * throwaway profile; with no Chrome the test is SKIPPED as "could not run",
 * never passed. Every value below is synthetic.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { chromeAvailable, withChangePage } from './change-page.mjs'

const base = (id, over) => ({
  id, project_id: 'p1', opened_at: '2026-09-22T09:00:00Z',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: { version: 1, criteria: ['The totals add up'], allowed_scope: ['checkout/'], check_profile: [], acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' } },
  examination: 'none_yet',
  ...(over || {}),
})
const link = (extra) => ({
  link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass',
  observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies', fresh: true, fresh_slug: 'applicability_still_applies',
  ...(extra || {}),
})
const coverage = (items) => ({ scope_version: 1, link_rows: items.length, criteria: [{ index: 1, text: 'The totals add up', state: items.length ? 'evidence' : 'none', items }] })

const FIXTURES = [
  ['proven', base('b390p'), coverage([link()])],
  ['needs a re-check', base('b390r'), coverage([link({ fresh: false, fresh_slug: 'applicability_needs_a_recheck' })])],
  ['not proven', base('b390n'), coverage([link({ observed: 'not_proven', failure_line: 'FAILED totals: expected 42, got 41' })])],
  ['nothing accepted', base('b390a', { accepted_scope: null }), null],
]
const ROUTES = {}
for (const [, change, cov] of FIXTURES) {
  const at = '/api/changes/' + change.id
  ROUTES[at] = { change }
  ROUTES[at + '/proposals'] = { proposals: null }
  if (cov) ROUTES[at + '/coverage'] = { coverage: cov }
  ROUTES[at + '/inputs'] = {}
  ROUTES[at + '/applicability'] = { items: [] }
}

/* Everything (a)–(d) asks, read in the page in one pass. */
const READ = `(() => {
  const vh = window.innerHeight
  const seen = (sel) => {
    const el = document.querySelector(sel)
    if (!el) return { sel, present: false }
    const r = el.getBoundingClientRect()
    const cs = getComputedStyle(el)
    return { sel, present: true, text: el.textContent.trim().slice(0, 80), w: r.width, h: r.height, top: r.top, bottom: r.bottom,
      shown: cs.display !== 'none' && cs.visibility !== 'hidden' && r.width > 0 && r.height > 0,
      inFirstViewport: r.top >= 0 && r.top < vh && r.bottom > 0 }
  }
  const action = document.querySelector('[aria-labelledby="ws-inspector-h-action"]')
  const primary = action ? action.querySelector('button') : null
  const why = primary && primary.getAttribute('aria-describedby')
    ? primary.getAttribute('aria-describedby').split(/\\s+/).map((id) => (document.getElementById(id) || {}).textContent || '').join(' ').trim()
    : ''
  return {
    innerWidth: window.innerWidth, scrollWidth: document.documentElement.scrollWidth,
    headline: [seen('.ws-change-title'), seen('.ws-change-conclusion'), seen('.ws-brief-state')],
    version: seen('.ws-change-version'),
    canvases: document.querySelectorAll('canvas, .ws-canvas').length,
    primary: primary ? { label: primary.textContent.trim(), disabled: primary.disabled || primary.getAttribute('aria-disabled') === 'true', tabIndex: primary.tabIndex, why } : null,
  }
})()`

test('A3-F15 · the first-time test at 390 px: headline, state and one primary, no graph, no sideways scroll', { skip: !(await chromeAvailable()) && 'could not run: no Chrome on this machine' }, async () => {
  await withChangePage(ROUTES, async ({ show, evaluate }) => {
    for (const [what, change] of FIXTURES) {
      await show(`/app.html?altitude=brief#/changes/${change.id}`, 390, 844, "document.querySelector('.ws-change-conclusion') && document.querySelector('[aria-labelledby=\"ws-inspector-h-action\"]')")
      const r = await evaluate(READ)
      console.log(`${what}: ${JSON.stringify({ scrollWidth: r.scrollWidth, innerWidth: r.innerWidth, headline: r.headline.map((h) => [h.sel, h.shown, h.inFirstViewport, Math.round(h.top)]), canvases: r.canvases, primary: r.primary && { label: r.primary.label, disabled: r.primary.disabled } })}`)
      for (const h of r.headline) {
        assert.ok(h.present, `${what}: ${h.sel} is not on the page`)
        assert.ok(h.shown, `${what}: ${h.sel} is drawn with no box, or hidden`)
        assert.ok(h.text.length > 0, `${what}: ${h.sel} says nothing`)
        assert.ok(h.inFirstViewport, `${what}: ${h.sel} is not in the first viewport (top ${h.top}, viewport 844)`)
      }
      assert.ok(r.version.present && r.version.text.length > 0, `${what}: the version line is missing`)
      assert.ok(r.scrollWidth <= r.innerWidth, `${what}: the page scrolls sideways (${r.scrollWidth} > ${r.innerWidth})`)
      assert.equal(r.canvases, 0, `${what}: a graph is drawn on the Brief`)
      assert.ok(r.primary, `${what}: the Inspector has no primary control`)
      if (r.primary.disabled) assert.ok(r.primary.why.length > 0, `${what}: "${r.primary.label}" is off and says no reason`)
      else assert.ok(r.primary.tabIndex >= 0, `${what}: "${r.primary.label}" cannot be reached by keyboard`)
    }
  })
})
