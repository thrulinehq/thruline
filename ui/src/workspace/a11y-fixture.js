/* The accessibility drive's synthetic Change (T-6-4) — test and walk-aid data only.
 *
 * NOT a data path: nothing in the app imports this file. `a11y-keys.test.js`
 * and `docs/loop/p6b/a11y-drive.mjs` answer the Change screen's reads from it,
 * so the Change screen they drive is the real `ChangeWorkspace` reading
 * documents of the shape the local server serves. Every value is synthetic;
 * no string here came off a real machine. The same Change `shell-chrome.test.js`
 * renders: one line of what counts as done, and one run that proved it.
 */

export const A11Y_CHANGE_ID = 'ch1'

export const A11Y_CHANGE = Object.freeze({
  id: A11Y_CHANGE_ID,
  project_id: 'p1',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  examination: 'none_yet',
})

export const A11Y_COVERAGE = Object.freeze({
  scope_version: 1,
  link_rows: 1,
  criteria: [{ index: 1, text: 'The totals add up', state: 'evidence', items: [{
    link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass',
    observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies', fresh: true, fresh_slug: 'applicability_still_applies',
  }] }],
})

/** What each Change read answers, by address. Anything else is a 404. */
export const A11Y_CHANGE_ROUTES = Object.freeze({
  ['/api/changes/' + A11Y_CHANGE_ID]: { change: A11Y_CHANGE },
  ['/api/changes/' + A11Y_CHANGE_ID + '/proposals']: { proposals: null },
  ['/api/changes/' + A11Y_CHANGE_ID + '/coverage']: { coverage: A11Y_COVERAGE },
  ['/api/changes/' + A11Y_CHANGE_ID + '/inputs']: {},
  ['/api/changes/' + A11Y_CHANGE_ID + '/applicability']: { items: [] },
})

/**
 * A `fetch` that answers from `A11Y_CHANGE_ROUTES` and says 404 to anything
 * else — so the workspace, finding no served session, opens its bundled demo
 * record under its permanent banner, exactly as it does with no server.
 */
export async function a11yFetch(url) {
  const path = String(url).replace(/^https?:\/\/[^/]+/, '').split('?')[0]
  const doc = Object.prototype.hasOwnProperty.call(A11Y_CHANGE_ROUTES, path) ? A11Y_CHANGE_ROUTES[path] : null
  return doc
    ? { ok: true, status: 200, statusText: 'OK', headers: { get: () => null }, text: async () => JSON.stringify(doc), json: async () => doc }
    : { ok: false, status: 404, statusText: 'Not Found', headers: { get: () => null }, text: async () => '{}', json: async () => ({}) }
}

export default A11Y_CHANGE_ROUTES
