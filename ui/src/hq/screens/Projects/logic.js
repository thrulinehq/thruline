/* Projects (C30 R-NAV): renderVals() returns everything template.jsx reads.
 * There is no canvas artboard and no demo for this page: it is drawn only
 * on the live path, from `this.props.data` (from-projects.js). The frame's
 * values are the Inbox's (Inbox/text.js), so the sidebar, the badge and the
 * status bar say exactly what they say on the Inbox. */
import { DCLogic } from '../../dc/runtime.jsx'
import { inboxText } from '../Inbox/text.js'
import { middle } from '../../data/paths.js'
import { fmtCount } from '../../data/count.js'
import { focusKeys } from '../../keys/list.js'

const O = '#FF7A29', HAIR = '#1F1F23', CTRL = '#66666F'

/** First run's folder-row look: the current project as First run draws the chosen folder. */
export function folderUi(on) {
  return { on: on ? 'true' : 'false', bg: on ? '#16161A' : '#111113', bd: on ? O : HAIR, ring: on ? O : CTRL, dot: on ? O : 'transparent' }
}

/** The page's own words: the Inbox's frame sentences for the sidebar and status bar. */
export function projectsText(D) {
  const frame = inboxText({ projects: D.projects, inboxBadge: D.inboxBadge, inboxBadgeAria: D.inboxBadgeAria, readAt: D.readAt, storage: D.storage, changes: {}, prompts: [], sessions: [], nothingNeeded: { items: [] }, capture: {} })
  const n = D.folders.length
  return Object.assign({}, frame, {
    headline: 'Projects',
    countNote: 'Folders where an agent has worked · ' + fmtCount(n) + '. Open one to see its Inbox.',
    footer: [D.kept, 'read ' + D.readAt].filter(Boolean).join(' · '),
  })
}

export default class Projects extends DCLogic {
  renderVals() {
    const D = this.props.data
    const t = projectsText(D)
    const folders = D.folders.map((f) => ({
      id: f.id,
      href: f.href,
      cur: f.current ? 'true' : undefined,
      aria: f.name + (f.current ? ', the project you are in' : '') + '. Opens its Inbox',
      name: f.name,
      fullName: f.name,
      path: middle(f.path, 52),
      full: f.path,
      agent: f.agent,
      gitOk: f.git === true,
      git: f.git === true ? 'Git repository' : f.git === false ? 'No Git repository' : 'Git not checked yet',
      recorded: f.summary,
      ui: folderUi(f.current),
    }))
    return {
      isDemo: false,
      page: {
        name: 'Projects',
        inbox: { cur: undefined, cls: 'tl-ghost', bg: undefined, color: '#A1A1AA', weight: undefined },
      },
      t,
      folders,
      codex: { tried: false, notTried: false, retry: undefined },
      // C30 R-KEYS: j and k move focus along the rows, which are links; ↵ on a focused row is the link's own
      keys: focusKeys({ list: 'div[role="list"][aria-label="Projects"]' }),
    }
  }
}
