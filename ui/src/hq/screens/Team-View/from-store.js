/* Team-View on the real record: what this machine's store holds, and nothing
 * else (W2-E).
 *
 * ThruLine has no team model. There are no seats, no groups, no per-path
 * approval rules and no shared templates. What the store does hold is:
 *   - every approval and withdrawal (`change_decisions`, served on each
 *     Change as `decisions.history`);
 *   - every second person's name beside an approval that stands
 *     (`change_decision_seconds`, served as `seconds[]`, each with a `counting`
 *     the server derived; C30 R-LEDGER);
 *   - every line set aside under the accepted profile's authority, as the
 *     approval that was given under it records it (`decisions.history[].exceptions`);
 *   - how much this machine keeps (`/api/storage`);
 *   - the licence (`/api/licence`): Free or Pro, and nothing else (CTO C21-b),
 *     decided by the one plan rule, `data/plan.js`.
 * This file turns those served documents into the screen's live data.
 * It is pure: no fetch, no clock but `now`.
 */
import { inboxBadge } from '../../data/project.js'
import { licencePlan } from '../../data/plan.js'
import { secondsOf } from '../../data/seconds.js'

const pad = (n) => String(n).padStart(2, '0')
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** The machine's local time, in the design's `HH:MM` form, with the day when it is not today. */
export function localWhen(iso, now) {
  const t = new Date(iso)
  if (Number.isNaN(t.getTime())) return null
  const hm = pad(t.getHours()) + ':' + pad(t.getMinutes())
  const n = new Date(now)
  const sameDay = t.getFullYear() === n.getFullYear() && t.getMonth() === n.getMonth() && t.getDate() === n.getDate()
  return sameDay ? 'today ' + hm : t.getDate() + ' ' + MONTHS[t.getMonth()] + ' ' + hm
}
export const localHM = (iso) => {
  const t = new Date(iso)
  return Number.isNaN(t.getTime()) ? null : pad(t.getHours()) + ':' + pad(t.getMinutes())
}

const list = (x) => (Array.isArray(x) ? x : [])
const short = (rev) => (typeof rev === 'string' && /^[0-9a-f]{8,}$/i.test(rev) ? rev.slice(0, 7) : rev)

/** Free or Pro, by the ONE plan rule (data/plan.js, C30 P0-6); null when the licence was not read.
 * `licensee` is a served name when there is one; /api/licence serves none today, so it is null. */
export function planOf(licence) {
  const { plan, word } = licencePlan(licence)
  if (!plan) return null
  return { name: word, licensee: plan === 'pro' && typeof licence.licensee === 'string' ? licence.licensee : null }
}

/**
 * @param {object} p
 * @param {object} p.project   the HQ ProjectSummary in front of the person
 * @param {object[]} p.projects every HQ ProjectSummary (for the counts in the frame)
 * @param {object[]} p.changes  ChangeView[] from `GET /api/changes`
 * @param {?object} p.storage   `GET /api/storage`
 * @param {?object} p.licence   `GET /api/licence`
 * @param {number|string|Date} p.now
 */
export function teamFromStore({ project, projects, changes, storage, licence, now, reading = null }) {
  const members = new Set(list(project && project.members).map((m) => m.ledger_project_id))
  const mine = list(changes).filter((c) => c && members.has(c.project_id))

  const decisions = []
  const setAside = []
  for (const c of mine) {
    const title = (c.request && c.request.title) || null
    const criteria = list(c.accepted_scope && c.accepted_scope.criteria)
    const d = c.decisions || {}
    const history = list(d.history)
    const withdrawn = new Set(history.filter((r) => r.act === 'withdrawn' && r.withdraws).map((r) => r.withdraws))
    // C30 R-LEDGER: the second people's names that still count, as the server derived them
    // (`counting`); a Change served without `seconds[]` simply has none.
    const counting = secondsOf(c)
    for (const r of history) {
      if (r.act !== 'approved') continue
      const stands = !!(d.standing && d.standing.id === r.id) && !withdrawn.has(r.id)
      decisions.push({
        changeId: c.id,
        title,
        version: short(r.target_revision),
        full: r.target_revision,
        actor: r.actor,
        at: localHM(r.decided_at),
        decidedAt: r.decided_at,
        recordPath: r.record_path || null,
        withdrawn: withdrawn.has(r.id),
        /* the approval that stands now; only it can be seconded */
        standing: stands,
        /* a later approval took its place (derived by the server, absent otherwise) */
        replaced: !withdrawn.has(r.id) && !!r.superseded_by,
        seconds: stands
          ? counting.filter((s) => s.decisionId === r.id).map((s) => ({ id: s.id, who: s.who, at: localHM(s.at), decidedAt: s.at }))
          : [],
        codeMovedSince: !!(d.standing && d.standing.id === r.id && d.target_revision && d.target_revision !== r.target_revision),
      })
      for (const e of list(r.exceptions)) {
        const m = /^criterion:0*(\d+)$/.exec(String(e.obligation_id || ''))
        const line = m && criteria[Number(m[1]) - 1] ? criteria[Number(m[1]) - 1] : null
        setAside.push({
          changeId: c.id,
          kind: e.kind === 'not_applicable' ? 'not_applicable' : 'waived',
          line,
          version: short(r.target_revision),
          change: title,
          by: e.actor,
          authority: e.authority,
          at: localWhen(r.decided_at, now),
          why: e.rationale,
        })
      }
    }
  }
  decisions.sort((a, b) => String(b.decidedAt).localeCompare(String(a.decidedAt)))

  return {
    live: true,
    project: { name: project ? project.name : null },
    projects: list(projects).length,
    readAt: localHM(new Date(now).toISOString()),
    // the sidebar badge: the ONE count every screen shows (`yourMove` of the current project, C46 §1.1)
    inbox: { yourMove: inboxBadge(list(projects), project && project.id, reading) },
    plan: planOf(licence),
    storage: { bytes: storage && typeof storage.bytes === 'number' ? storage.bytes : null },
    decisions,
    setAside,
  }
}
