/* Projects (C30 R-NAV): what the page draws, from `GET /api/hq/projects` and
 * `GET /api/storage`. PURE: no fetch, no window; `now` is passed in.
 *
 * Every project is one of First run's folder rows (data/project.js
 * `firstRunData`, the same order and the same words), made a link to that
 * project's Inbox. The project the person is in carries First run's chosen
 * look. Every count goes through count.js, every path through paths.js.
 */
import { href } from '../../routes.js'
import { fmtCount } from '../../data/count.js'
import { currentProjectId, firstRunData, fmtClock, fmtWhen, inboxBadge, inboxBadgeAria, isWhole, sessionsKept, sessionsKeptPhrase, sidebarProjects } from '../../data/project.js'

const plural = (n, one, many) => (n === 1 ? one : many)
const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : undefined)

/** A folder row's last column: what the record holds for it, in plain words. */
export function projectSummary(p, now) {
  if (!p) return ''
  const when = fmtWhen(p.last_activity, now)
  if (!((num(p.sessions) || 0) > 0)) return 'Not recorded yet'
  const last = when ? 'last active ' + when : ''
  if (!isWhole(p) || num(p.threads) === undefined) return [last, 'not yet counted'].filter(Boolean).join(' · ')
  return [fmtCount(p.threads) + ' ' + plural(p.threads, 'Change', 'Changes'), last].filter(Boolean).join(' · ')
}

/**
 * @param {{projects?: object|null, storage?: object|null, now: Date|number, current?: string, reading?: object|null}} docs
 *   reading: the current project's readiness reading (data/your-move.js), for the one "your move" count
 */
export function projectsData({ projects, storage, now, current, reading = null }) {
  const list = projects && Array.isArray(projects.projects) ? projects.projects.filter(Boolean) : []
  const cur = currentProjectId(list, current)
  const byId = Object.fromEntries(list.map((p) => [p.id, p]))
  const first = firstRunData({ projects, storage, now })
  const badge = inboxBadge(list, current, reading)
  const kept = sessionsKept(projects)
  return {
    readAt: fmtClock(now),
    currentProject: cur || null,
    projects: sidebarProjects(list, now, current),
    inboxBadge: badge,
    inboxBadgeAria: inboxBadgeAria(badge),
    storage: num(storage && storage.bytes) !== undefined ? { bytes: storage.bytes } : undefined,
    kept: kept && kept.projects ? sessionsKeptPhrase(kept) : null,
    folders: first.folders.map((f) => Object.assign({}, f, {
      href: href('inbox', { project: f.id }),
      current: f.id === cur,
      summary: projectSummary(byId[f.id], now),
    })),
  }
}
