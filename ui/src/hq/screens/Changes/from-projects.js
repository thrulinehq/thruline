/* Changes (C30 R-NAV, `g c`): every Change and thread of work, across every
 * project on this machine, from `GET /api/hq/projects` and each project's
 * `GET /api/hq/projects/<id>`. PURE: no fetch, no window; `now` is passed in.
 *
 * Each project is one block, filed exactly as its Inbox files it
 * (data/project.js `inboxData`, with that project's readiness reading, so a
 * ready Change is its approve row here too), so a row here is the Inbox's row
 * for the same thread, word for word. A project whose view or readiness has
 * not arrived is `reading`; one whose view could not be read is `failed`;
 * neither is ever drawn as "nothing needed". The sidebar badge is the one
 * count (`yourMove`, C46 §1.1) of the current project.
 *
 * NOT-APPROVAL-WORDS: each project's readiness reading only reaches `inboxData`, which files a
 * Change that IS ready as its approve row; nothing here says why approval is off.
 */
import { href } from '../../routes.js'
import { currentProjectId, fmtClock, hasRecords, inboxBadge, inboxBadgeAria, inboxData, sidebarProjects } from '../../data/project.js'

const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : undefined)
const byRecent = (a, b) => String(b.last_activity || '').localeCompare(String(a.last_activity || ''))

/** The projects whose views the page reads: every project that has records, newest first. */
export function projectsToRead(projects) {
  const list = projects && Array.isArray(projects.projects) ? projects.projects.filter(Boolean) : []
  return list.filter(hasRecords).sort(byRecent)
}

/**
 * @param {{projects?: object|null, views?: Record<string, object>, readings?: Record<string, object>, storage?: object|null, now: Date|number, current?: string}} docs
 *   views: project id → its view document, or `{error}` when it could not be read; absent = not read yet
 *   readings: project id → its readiness reading (data/project.js `readingOf`); absent = not read yet
 */
export function changesData({ projects, views = {}, readings = {}, hides = {}, storage, now, current }) {
  const list = projects && Array.isArray(projects.projects) ? projects.projects.filter(Boolean) : []
  const cur = currentProjectId(list, current)
  const side = sidebarProjects(list, now, cur)
  for (const row of side) {
    const n = readings[row.id] && readings[row.id].dismissedCount
    if (typeof n === 'number') row.dismissedCount = n
  }
  const sideById = Object.fromEntries(side.map((p) => [p.id, p]))
  const blocks = projectsToRead(projects).map((p) => {
    const v = views[p.id]
    const r = readings[p.id]
    const base = { id: p.id, side: sideById[p.id] || null, href: href('inbox', { project: p.id }) }
    if (v && v.error) return Object.assign(base, { failed: true })
    if (!v || !r) return Object.assign(base, { reading: true })
    const data = inboxData({ projects, view: v, storage, ready: r.ready, readyFailed: r.failed + (r.listFailed ? 1 : 0), hides: hides[p.id], now })
    return Object.assign(base, { data, threads: num(v.project && v.project.threads) })
  })
  const badge = inboxBadge(list, cur, readings[cur] || null)
  return {
    readAt: fmtClock(now),
    currentProject: cur || null,
    projects: side,
    inboxBadge: badge,
    inboxBadgeAria: inboxBadgeAria(badge),
    storage: num(storage && storage.bytes) !== undefined ? { bytes: storage.bytes } : undefined,
    blocks,
  }
}
