/* Settings' folder-scoped run permissions. A row comes from the record and
 * carries only a server-owned opaque folder id. No browser path is sent. */
import { useCallback, useEffect, useRef, useState } from 'react'

export const PERMISSIONS_READ = (project) => `/api/hq/projects/${encodeURIComponent(project)}/run-permissions`
export const PERMISSION_GRANT = (change) => `/api/changes/${encodeURIComponent(change)}/run/permission`
export const PERMISSION_REVOKE = (change) => `/api/changes/${encodeURIComponent(change)}/run/permission/revoke`
export const HISTORICAL_REVOKE = (project, folder) => `${PERMISSIONS_READ(project)}/${encodeURIComponent(folder)}/revoke`
const RUN_TOKEN = '/api/run/token'
export const FOLDER_ID = /^[0-9a-f]{64}$/

/** Which server-offered action has a complete, internally consistent guard. */
export function safePermissionAction(row) {
  if (!row || typeof row.id !== 'string' || !FOLDER_ID.test(row.id) ||
      typeof row.folder !== 'string' || !row.folder) return null
  const current = typeof row.change_id === 'string' && !!row.change_id && row.historical === false
  const orphan = row.change_id == null && row.historical === true
  const latest = typeof row.latest_row === 'string' && !!row.latest_row
  if (row.action === 'revoke' && latest && (
    (row.state === 'given' && (current || orphan)) ||
    ((row.state === 'file_missing' || row.state === 'not_in_record') && orphan)
  )) return 'revoke'
  if (row.action === 'grant' && current && (
    ((row.state === 'revoked' || row.state === 'file_missing') && latest) ||
    (row.state === 'not_in_record' && (row.latest_row == null || latest))
  )) return 'grant'
  return null
}

async function answer(res) {
  const doc = await res.json().catch(() => null)
  return { ok: res.ok, status: res.status, doc }
}

/** One guarded permission act. The run token and browser Origin protect its POST. */
export async function permissionWrite(project, row, f = fetch) {
  const act = safePermissionAction(row)
  const id = row && row.id
  const change = row && row.change_id
  if (typeof project !== 'string' || !project || !act) {
    return { ok: false, status: 0, doc: { error: 'not_available' } }
  }
  const url = change
    ? act === 'grant' ? PERMISSION_GRANT(change) : PERMISSION_REVOKE(change)
    : HISTORICAL_REVOKE(project, id)
  const body = change
    ? { expected_folder: id, expected_row: row.latest_row ?? null }
    : { expected_row: row.latest_row ?? null }
  let token
  try {
    token = await answer(await f(RUN_TOKEN, { cache: 'no-store', credentials: 'same-origin', headers: { Accept: 'application/json' } }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
  if (!token.ok || !token.doc || typeof token.doc.token !== 'string') {
    return { ok: false, status: token.status, doc: token.doc || { error: 'run_token_required' } }
  }
  try {
    return await answer(await f(url, {
      method: 'POST', cache: 'no-store', credentials: 'same-origin',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json', 'X-ThruLine-Run-Token': token.doc.token },
      body: JSON.stringify(body),
    }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

/** What one answer says. The actual standing always comes from the next GET. */
export function permissionAnswer(res) {
  const doc = res && res.doc && typeof res.doc === 'object' ? res.doc : {}
  if (res && res.ok) return { kind: 'kept' }
  if (typeof doc.plain === 'string' && doc.plain) return { kind: 'refused', detail: doc.plain }
  const words = {
    not_available: 'This folder has no Settings action. Nothing changed.',
    run_token_required: 'ThruLine could not confirm this press came from its own page. Nothing changed. Reload and try again.',
    same_origin_required: 'ThruLine could not confirm this press came from its own page. Nothing changed. Reload and try again.',
    permission_target_changed: 'This permission changed in the record. Read it again.',
    no_permission_to_revoke: 'There is no standing permission to take back here. Nothing changed.',
    unreachable: 'ThruLine’s local server did not answer, so this permission may have changed. Reload to see where it stands.',
  }
  return { kind: 'refused', detail: words[doc.error] || 'ThruLine could not finish this permission change. Check the refreshed row before retrying.' }
}

/** Keep the button waiting through the next read, including after a refusal. */
export async function writeThenRead(project, row, refresh, f = fetch) {
  const answer = await permissionWrite(project, row, f)
  const read = await refresh()
  if (!read || !read.ok) return { kind: 'refused', detail: 'ThruLine could not read this permission again. Reload the page to see where it stands.' }
  return permissionAnswer(answer)
}

/** A project-keyed read; a late answer from a prior project cannot enter this page. */
export function useRunPermissions(project, f = fetch) {
  const [state, setState] = useState({ project: null, doc: null, error: null, loading: false })
  const sequence = useRef(0)
  const selected = useRef(project)
  selected.current = project
  const refresh = useCallback(async () => {
    if (!project || selected.current !== project) return { ok: false, error: 'project_changed' }
    const turn = ++sequence.current
    setState((s) => ({ project, doc: s.project === project ? s.doc : null, error: null, loading: true }))
    try {
      const response = await f(PERMISSIONS_READ(project), { cache: 'no-store', credentials: 'same-origin', headers: { Accept: 'application/json' } })
      const doc = await response.json()
      if (!response.ok || !doc || doc.schema_version !== 1 || doc.project !== project || !Array.isArray(doc.folders)) throw new Error('unreadable')
      if (sequence.current !== turn || selected.current !== project) return { ok: false, error: 'project_changed' }
      setState({ project, doc, error: null, loading: false })
      return { ok: true, doc }
    } catch (_e) {
      if (sequence.current !== turn || selected.current !== project) return { ok: false, error: 'project_changed' }
      setState({ project, doc: null, error: 'unreadable', loading: false })
      return { ok: false, error: 'unreadable' }
    }
  }, [project, f])
  useEffect(() => {
    if (project) refresh()
    return () => { sequence.current++ }
  }, [project, refresh])
  const current = state.project === project
  return {
    doc: current && !state.error ? state.doc : null,
    error: current ? state.error : null,
    loading: !!project && (!current || state.loading),
    refresh,
  }
}
