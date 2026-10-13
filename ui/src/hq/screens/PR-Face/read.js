/* PR-Face's one read of its own: `GET /api/changes/<id>/pr` (C30 R-PR,
 * `crates/cli/src/uiserve/pr_preview.rs`, docs/hq-next/API.md) — the tracked
 * Change's folder: its branch and head from git, the receipts the ThruLine
 * Action would verify there, and the CLI's own check-run name and badge
 * words. A GET; it writes nothing. Everything else the page draws comes
 * through `Repair-Loop/live.js` `useLive` (the one Change snapshot and the
 * tracked Change's documents and model).
 */
import { useResource } from '../../data/api.js'

/** The address, declared once. */
export const PR_PREVIEW = (id) => '/api/changes/' + encodeURIComponent(id) + '/pr'

/** How often the page reads the folder again while it is open. */
export const PR_REREAD_MS = 20000

/** A deliberate local check. The existing read address accepts `?check=1`
 * only with this launch's token; its verifier uses disposable Git storage. */
export async function checkReceipt(id, f = fetch) {
  if (!id) throw new Error('No Change is open to check.')
  const tokenResponse = await f('/api/run/token', {
    cache: 'no-store', credentials: 'same-origin', headers: { Accept: 'application/json' },
  })
  const token = await tokenResponse.json().catch(() => null)
  if (!tokenResponse.ok || typeof token?.token !== 'string') throw new Error('ThruLine could not confirm this check came from its own page.')
  const response = await f(PR_PREVIEW(id) + '?check=1', {
    method: 'GET', cache: 'no-store', credentials: 'same-origin',
    headers: { Accept: 'application/json', 'X-ThruLine-Run-Token': token.token },
  })
  const doc = await response.json().catch(() => null)
  if (!response.ok || !doc?.checked) throw new Error('ThruLine could not check this receipt. The preview has not assigned it a conclusion.')
  return doc
}

/** `{doc, error, loading}` for a tracked Change, or null for work nobody tracked. */
export function usePrPreview(trackedId) {
  const r = useResource(trackedId ? PR_PREVIEW(trackedId) : null, { every: PR_REREAD_MS })
  if (!trackedId) return null
  const doc = r.data && r.data.schema_version === 1 ? r.data : null
  return { doc, error: r.error ? String(r.error.message || r.error) : (r.data && !doc ? 'unknown document' : null), loading: r.loading }
}
