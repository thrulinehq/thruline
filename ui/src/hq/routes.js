/* HQ Next routing — which ported screen an address names, and where each of
 * the canvas's prototype links goes in the live product.
 *
 * PURE: no window, no fetch. `parse` reads a hash; `href` writes one; `link`
 * turns a canvas prototype link (`Change-Brief.dc.html`) into the live address
 * for the Change in front of the person.
 *
 *   #/                         Inbox (the project the person last opened)
 *   #/inbox[?project=<id>]     Inbox
 *   #/changes[?project=<id>]   Changes: every Change and thread of work, across projects
 *   #/projects[?project=<id>]  Projects: every project on this machine
 *   #/changes/<id>             Change · Brief
 *   #/changes/<id>?statement=<s>  Change · Brief, with statement <s> in the inspector
 *   #/changes/<id>/statements  Every statement (Explore)
 *   #/changes/<id>/changed     What changed (Workbench)
 *   #/changes/<id>/done        What counts as done
 *   #/changes/<id>/check       Check it again · Tell the agent
 *   #/changes/<id>/approve     Approve sheet
 *   #/changes/<id>/story       The whole story
 *   #/changes/<id>/pr          On the pull request (a preview of the note; nothing is posted)
 *   #/changes/<id>/step/<n>    One step of the record: what was said, run, edited or asked,
 *                              where it came from, and the lines around it (C30 R-ROUTES "Step view")
 *   #/record                   Record over time
 *   #/record/statements        Record over time: statements across Changes, filtered by its KPI slice
 *   #/settings[?pane=<pane>]   Settings, on one pane (storage, keyboard, plan, …)
 *   #/team                     Team
 *   #/start                    First run
 *
 * Any address may carry `find=1`: the screen it names, with Find, ask, act
 * (the ⌘K palette) open over it.
 *
 * C30 R-LINKS: only a hash that starts `#/` is an address. Anything else
 * (`#ws-done`, the whole story's contents) is a place on the page, and the
 * router never reads it (`isAddress`, `nextRoute`). A canvas link the table
 * below does not know, or a link to a Change screen with no Change open,
 * resolves to NO link (`undefined`) — never to the Inbox.
 */

export const SCREENS = Object.freeze({
  inbox: 'Inbox',
  brief: 'Change-Brief',
  statements: 'Every-Statement',
  changed: 'What-Changed',
  done: 'Done-Criteria',
  check: 'Repair-Loop',
  approve: 'Approve-Sheet',
  story: 'Whole-Story',
  pr: 'PR-Face',
  step: 'Step-View',
  record: 'Record-Over-Time',
  recordStatements: 'Record-Statements',
  settings: 'Settings',
  team: 'Team-View',
  start: 'Onboarding',
  // C30 R-NAV: the two home screens the canvas drew as nav items with no page of their own
  changes: 'Changes',
  projects: 'Projects',
})

const CHANGE_SUBS = new Set(['statements', 'changed', 'done', 'check', 'approve', 'story', 'pr', 'step'])
const CHANGE_SCREENS = new Set([...CHANGE_SUBS, 'brief'])
const RECORD_RANGES = new Set(['7', '30', '90'])
const RECORD_AGENTS = new Set(['all', 'cc', 'cx'])
const RECORD_SUBSETS = new Set(['all', 'proven', 'moved', 'open', 'recorded'])

/**
 * Settings' panes, by the name an address uses → the pane Settings draws.
 * "Settings › Storage" is the Privacy & storage pane.
 */
export const SETTINGS_PANES = Object.freeze({
  agents: 'agents',
  checks: 'checks',
  rules: 'rules',
  storage: 'privacy',
  privacy: 'privacy',
  notifications: 'notifications',
  appearance: 'appearance',
  keyboard: 'keyboard',
  plan: 'plan',
})

/** True when a hash is an address this router reads: empty, `#`, or `#/…`. */
export function isAddress(hash) {
  return hash == null || hash === '' || hash === '#' || (typeof hash === 'string' && hash.startsWith('#/'))
}

/** The route after the hash changed: a hash that is not an address leaves it where it was. */
export function nextRoute(prev, hash) {
  return isAddress(hash) ? parse(hash) : prev
}

/** A step number as the record numbers it: a whole number from 1. */
const STEP_NUMBER = /^[1-9][0-9]{0,8}$/

/** @returns {{screen: string, change?: string, project?: string, pane?: string, find?: true, statement?: string, step?: number|null, asked?: string}} */
export function parse(hash) {
  const h = typeof hash === 'string' ? hash : ''
  const [path, query = ''] = h.replace(/^#/, '').split('?')
  const params = new URLSearchParams(query)
  const project = params.get('project') || undefined
  const extra = params.get('find') ? { find: true } : {}
  const segs = path.split('/').filter(Boolean)
  if (segs.length === 0 || segs[0] === 'inbox') return { screen: 'inbox', project, ...extra }
  if (segs[0] === 'changes' && segs[1]) {
    const id = decodeURIComponent(segs[1])
    const sub = segs[2] || 'brief'
    const screen = CHANGE_SUBS.has(sub) ? sub : 'brief'
    /* the Brief opened at one statement (Every statement's "Open this in the full statement view") */
    const statement = screen === 'brief' ? params.get('statement') || undefined : undefined
    /* one step of the record: `step` is its number, or null when the address
     * names none (the screen then says there is no such step — it never
     * redirects); `asked` is what the address carried in its place */
    let stepPart = {}
    if (screen === 'step') {
      const raw = segs[3] ? safeDecode(segs[3]) : ''
      stepPart = STEP_NUMBER.test(raw) ? { step: Number(raw) } : { step: null, ...(raw ? { asked: raw } : {}) }
    }
    return { screen, change: id, project, ...extra, ...(statement ? { statement } : {}), ...stepPart }
  }
  if (segs[0] === 'record' && segs[1] === 'statements' && segs.length === 2) {
    const range = params.get('range')
    const agent = params.get('agent')
    const subset = params.get('subset')
    return {
      screen: 'recordStatements', project,
      range: RECORD_RANGES.has(range) ? range : '30',
      agent: RECORD_AGENTS.has(agent) ? agent : 'all',
      scope: params.get('scope') || 'all',
      subset: RECORD_SUBSETS.has(subset) ? subset : 'all',
      ...extra,
    }
  }
  if (segs[0] === 'settings') {
    const pane = params.get('pane')
    return { screen: 'settings', project, ...(pane && SETTINGS_PANES[pane] ? { pane } : {}), ...extra }
  }
  if (['record', 'team', 'start', 'changes', 'projects'].includes(segs[0])) return { screen: segs[0], project, ...extra }
  return { screen: 'inbox', project, ...extra }
}

const safeDecode = (x) => {
  try { return decodeURIComponent(x) } catch (_e) { return x }
}

/** The address of a screen. A step's address carries its number (`step`). */
export function href(screen, { change, project, pane, find, statement, step, range, agent, scope, subset } = {}) {
  const q = new URLSearchParams()
  if (project) q.set('project', project)
  if (screen === 'brief' && statement) q.set('statement', statement)
  if (screen === 'settings' && pane && SETTINGS_PANES[pane]) q.set('pane', pane)
  if (find) q.set('find', '1')
  const qs = q.toString() ? '?' + q.toString() : ''
  if (screen === 'inbox') return `#/inbox${qs}`
  if (screen === 'brief') return `#/changes/${encodeURIComponent(change)}${qs}`
  if (screen === 'step') return `#/changes/${encodeURIComponent(change)}/step${Number.isInteger(step) && step > 0 ? '/' + step : ''}${qs}`
  if (screen === 'recordStatements') {
    q.set('range', RECORD_RANGES.has(String(range)) ? String(range) : '30')
    q.set('agent', RECORD_AGENTS.has(agent) ? agent : 'all')
    q.set('scope', scope || 'all')
    q.set('subset', RECORD_SUBSETS.has(subset) ? subset : 'all')
    return `#/record/statements?${q.toString()}`
  }
  if (CHANGE_SUBS.has(screen)) return `#/changes/${encodeURIComponent(change)}/${screen}${qs}`
  return `#/${screen}${qs}`
}

/* The step view is not a canvas page: a step is named by its number, which no
 * canvas link carries (`href('step', …)` is how a screen reaches one). */
const BY_ARTBOARD = Object.fromEntries(Object.entries(SCREENS).filter(([k]) => k !== 'step' && k !== 'recordStatements').map(([k, v]) => [v, k]))

/**
 * Every canvas page a link may name, and what it becomes in the app:
 *  - a screen key (SCREENS);
 *  - `find`: Find, ask, act, opened over the screen in front of the person
 *    (the screen's own click handler opens it without leaving the page);
 *  - `plan`: Settings › Plan & licence (C30 R-PLAN; there is no separate
 *    Plan-Licence screen).
 * A page not in this table is not a link.
 */
export const LINK_TARGETS = Object.freeze({ ...BY_ARTBOARD, 'Command-Ask': 'find', 'Plan-Licence': 'plan' })

const DC = /^([A-Za-z0-9_-]+)\.dc\.html(?:#(.*))?$/

/**
 * One canvas link, resolved for the person's place `at`: the live address,
 * or `undefined` when there is nothing real to go to.
 */
export function resolveLink(target, at = {}) {
  if (typeof target !== 'string') return undefined
  const m = target.match(DC)
  if (!m) return target
  const kind = LINK_TARGETS[m[1]]
  const frag = m[2]
  if (!kind) return undefined
  const project = at.project
  if (kind === 'settings') {
    if (frag != null && !SETTINGS_PANES[frag]) return undefined
    return href('settings', { project, pane: frag || undefined })
  }
  if (frag != null) return undefined
  if (kind === 'plan') return href('settings', { project, pane: 'plan' })
  if (kind === 'find') {
    if (!at.screen || !SCREENS[at.screen]) return undefined
    if (CHANGE_SCREENS.has(at.screen) && !at.change) return undefined
    return href(at.screen, { change: at.change, project, pane: at.pane, find: true, step: at.step })
  }
  if (CHANGE_SCREENS.has(kind)) return at.change ? href(kind, { change: at.change, project }) : undefined
  return href(kind, { project })
}

/**
 * A canvas prototype link, made live. A link to a Change screen with no
 * Change in front of the person, or to a page the app does not have, is no
 * link at all (`undefined`): never the Inbox in its place.
 */
export function linker(at) {
  const where = at || {}
  return (target) => resolveLink(target, where)
}
