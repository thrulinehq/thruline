/* ⌘K's record search (C30 R-ROUTES "Record search", census §3.08 row 4):
 * the one reader of `GET /api/hq/projects/<id>/search` (docs/hq-next/API.md).
 *
 * The route finds, across every Change of one project, the kinds the design
 * draws for a Find row — Request, Said, File, Read, Run, Command, Version —
 * each masked on the server, each with the thread (Change) it belongs to.
 * Given the Change open in the palette (`thread`), it leaves that Change's
 * own requests, edits, commands and statements to the palette, which finds
 * them in the Change's own view with the steps every Change screen names;
 * its reads and versions come from here. So each kind has one source.
 *
 * READS ONLY. The one address here is a GET; nothing is written.
 *
 * Every answer is said, never swallowed (Rule 12):
 *   {status: 'idle'}                       nothing typed, or no project
 *   {status: 'reading', q}                 asked, not answered yet
 *   {status: 'served', q, doc}             the route's document
 *   {status: 'failed', q, message}         a sentence a person can read
 * `q` is the typed text the answer is for, so an answer to older text is
 * never drawn as the answer to newer text.
 */
import { useEffect, useRef, useState } from 'react'

/** How many hits one search asks for: the palette draws four, and says how many more. */
export const SEARCH_LIMIT = 20
/** The pause after the last key before the record is asked, in ms. */
export const SEARCH_DELAY_MS = 150
/** The longest typed text the route takes (crates/core/src/hq/search.rs MAX_QUERY_CHARS). */
export const SEARCH_MAX_CHARS = 200

/** The one address: `GET /api/hq/projects/<id>/search?q=…[&limit=…][&thread=…]`. */
export const SEARCH_ADDRESS = (project, q, { limit = SEARCH_LIMIT, thread } = {}) =>
  `/api/hq/projects/${encodeURIComponent(project)}/search?q=${encodeURIComponent(q)}` +
  (limit ? `&limit=${encodeURIComponent(limit)}` : '') +
  (thread ? `&thread=${encodeURIComponent(thread)}` : '')

const KINDS = new Set(['Request', 'Said', 'File', 'Read', 'Run', 'Command', 'Version'])

/** The served document, if it is one this reader understands; else null. */
function documentOf(body) {
  if (!body || body.schema_version !== 1 || !Array.isArray(body.results) || typeof body.total !== 'number') return null
  if (!body.results.every((h) => h && KINDS.has(h.kind) && typeof h.thread_id === 'string')) return null
  return body
}

/**
 * Ask the record once. Never throws.
 * @param {string} project  HQ project id
 * @param {string} q        the typed text, as typed
 * @param {{thread?: string|null, limit?: number}} [opts]
 * @param {typeof fetch} [fetchImpl]
 */
export async function readSearch(project, q, opts = {}, fetchImpl = typeof fetch === 'function' ? fetch : null) {
  const text = String(q == null ? '' : q)
  if (!project || !text.trim()) return { status: 'idle' }
  if (text.length > SEARCH_MAX_CHARS) return { status: 'failed', q: text, message: 'That is longer than the record can be searched for. Type fewer words.' }
  if (!fetchImpl) return { status: 'failed', q: text, message: 'ThruLine’s local server did not answer, so the rest of the record was not searched.' }
  let res
  try {
    res = await fetchImpl(SEARCH_ADDRESS(project, text, opts), { signal: opts.signal, headers: { accept: 'application/json' } })
  } catch (e) {
    if (e && e.name === 'AbortError') throw e
    return { status: 'failed', q: text, message: 'ThruLine’s local server did not answer, so the rest of the record was not searched.' }
  }
  let body = null
  try {
    body = JSON.parse(await res.text())
  } catch (_e) {
    body = null
  }
  const doc = res.ok ? documentOf(body) : null
  if (doc) return { status: 'served', q: text, doc }
  if (res.status === 404 && body && body.error === 'no_such_route') {
    return { status: 'failed', q: text, message: 'This ThruLine does not search the rest of the record yet.' }
  }
  if (res.status === 503) return { status: 'failed', q: text, message: 'The record needs an upgrade before it can be searched. Run thruline upgrade in a terminal.' }
  return { status: 'failed', q: text, message: 'ThruLine could not search the rest of the record just now. Type again to retry.' }
}

/**
 * The search for what is typed now: asked SEARCH_DELAY_MS after the last
 * key, the earlier question cancelled. `project` null reads nothing.
 */
export function useRecordSearch(project, query, { thread = null, delayMs = SEARCH_DELAY_MS, fetchImpl } = {}) {
  const [state, setState] = useState({ status: 'idle' })
  const live = useRef(true)
  useEffect(() => () => { live.current = false }, [])
  useEffect(() => {
    const text = String(query == null ? '' : query)
    if (!project || !text.trim()) {
      setState({ status: 'idle' })
      return undefined
    }
    setState({ status: 'reading', q: text })
    const ctl = typeof AbortController === 'function' ? new AbortController() : null
    const t = setTimeout(() => {
      readSearch(project, text, { thread, signal: ctl ? ctl.signal : undefined }, fetchImpl).then(
        (r) => { if (live.current) setState(r) },
        (e) => { if (live.current && !(e && e.name === 'AbortError')) setState({ status: 'failed', q: text, message: 'ThruLine could not search the rest of the record just now. Type again to retry.' }) },
      )
    }, delayMs)
    return () => {
      clearTimeout(t)
      if (ctl) ctl.abort()
    }
  }, [project, query, thread, delayMs, fetchImpl])
  return state
}
