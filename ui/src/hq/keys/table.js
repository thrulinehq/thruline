/* The one key table (C30 R-KEYS). DESIGN-SPEC §2.9, plus every key a screen
 * draws (FEATURE-CENSUS "keys" rows), in one list. The dispatcher (dispatch.js),
 * the `?` overlay (overlay.jsx) and Settings › Keyboard (rows.js) are all read
 * from it; nothing else names a key.
 *
 * PURE: no window, no React.
 *
 * An entry:
 *   keys    what is drawn, one per action (`act`): '⌘K', 'g i', 'j', '↵', 'Esc'
 *   label   what it does, in the words Settings › Keyboard prints
 *   kind    'single' — one character (WCAG 2.1.4: Settings › Keyboard governs it);
 *           'chord'  — held with ⌘ or Ctrl;  'named' — ↵, Esc, ↑, ↓
 *   global  true: on every live screen, acted on by the dispatcher itself
 *   screens where the key is drawn (or, for Changes and Projects, which have no
 *           artboard, where the list keys belong)
 *   wired   the screens where it acts today; a screen not listed draws the key and
 *           the key does nothing there yet (`waiting` says what it waits on)
 *   go      a route key (routes.js) the dispatcher navigates to
 *   change  true: it needs a Change open, and does nothing on a home screen
 *   by      'palette': Find, ask, act's own field acts on it, never the dispatcher
 *   palette screens where the palette handles this shared key while open
 *   act     the handler name a screen registers for each key (registry.js);
 *           defaults to the entry's id
 *
 * Two keys the canvas's Settings drew are not here, and §2.9's `n`/`N` are not
 * either: docs/hq-next/DECISIONS.md, "C30 R-KEYS". */

/** How long after `g` its second key is waited for (ms). */
export const SEQUENCE_MS = 1000

const HOME = ['Inbox', 'Changes', 'Projects', 'Record-Over-Time', 'Team-View', 'Settings', 'Onboarding']
const CHANGE = ['Change-Brief', 'Every-Statement', 'What-Changed', 'Done-Criteria', 'Repair-Loop', 'Approve-Sheet', 'Whole-Story', 'PR-Face']

/** Every live screen, by its artboard name (routes.js SCREENS). */
export const LIVE_SCREENS = Object.freeze([...HOME, ...CHANGE])
export const CHANGE_SCREENS = Object.freeze(CHANGE)

const WAIT_CHANGE = 'the Change screens’ own handlers (R-KEYS, second card)'

export const KEY_TABLE = Object.freeze([
  // ---- everywhere ----
  { id: 'find', keys: ['⌘K'], label: 'Find, ask, act', kind: 'chord', global: true },
  { id: 'help', keys: ['?'], label: 'This map', kind: 'single', global: true },
  { id: 'slash', keys: ['/'], label: 'Find', kind: 'single', global: true },
  { id: 'face', keys: ['t'], label: 'Plain / Technical', kind: 'single', global: true },
  { id: 'go-inbox', keys: ['g i'], label: 'Inbox', kind: 'single', global: true, go: 'inbox' },
  { id: 'go-changes', keys: ['g c'], label: 'Changes', kind: 'single', global: true, go: 'changes' },
  { id: 'go-projects', keys: ['g p'], label: 'Projects', kind: 'single', global: true, go: 'projects' },
  { id: 'go-record', keys: ['g r'], label: 'Record over time', kind: 'single', global: true, go: 'record' },
  { id: 'go-settings', keys: ['g s'], label: 'Settings', kind: 'single', global: true, go: 'settings' },
  { id: 'go-team', keys: ['g t'], label: 'Team', kind: 'single', global: true, go: 'team' },
  { id: 'go-done', keys: ['g d'], label: 'What counts as done', kind: 'single', global: true, go: 'done', change: true },
  { id: 'go-story', keys: ['g w'], label: 'The whole story', kind: 'single', global: true, go: 'story', change: true },
  { id: 'go-list', keys: ['g l'], label: 'Every statement', kind: 'single', global: true, go: 'statements', change: true },
  { id: 'alt-brief', keys: ['1'], label: 'Brief', kind: 'single', global: true, go: 'brief', change: true },
  { id: 'alt-explore', keys: ['2'], label: 'Explore', kind: 'single', global: true, go: 'statements', change: true },
  { id: 'alt-workbench', keys: ['3'], label: 'Workbench', kind: 'single', global: true, go: 'changed', change: true },
  { id: 'close', keys: ['Esc'], label: 'Close what is open', kind: 'named', global: true },

  // ---- on one screen ----
  { id: 'back', keys: ['Esc'], label: 'Back to the Change', kind: 'named', screens: ['Repair-Loop', 'Whole-Story'], wired: ['Repair-Loop', 'Whole-Story'], go: 'brief', change: true },
  { id: 'cancel', keys: ['Esc'], label: 'Close the sheet', kind: 'named', screens: ['Change-Brief', 'Approve-Sheet'], wired: ['Change-Brief', 'Approve-Sheet'] },

  // ---- lists ----
  { id: 'move', keys: ['j', 'k'], act: ['next', 'prev'], label: 'Next / previous in a list', kind: 'single', screens: ['Inbox', 'Changes', 'Projects', 'Change-Brief', 'Every-Statement'], wired: ['Inbox', 'Changes', 'Projects', 'Change-Brief', 'Every-Statement'], waiting: WAIT_CHANGE },
  { id: 'open', keys: ['↵'], label: 'Open', kind: 'named', screens: ['Inbox', 'Changes', 'Projects', 'Every-Statement'], wired: ['Inbox', 'Changes', 'Projects', 'Every-Statement'], waiting: WAIT_CHANGE },
  { id: 'primary', keys: ['.'], label: 'Check it again', kind: 'single', screens: ['Inbox', 'Changes', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Repair-Loop', 'Command-Ask'], wired: ['Inbox', 'Changes', 'Repair-Loop', 'What-Changed', 'Change-Brief', 'Command-Ask', 'Every-Statement'], palette: ['Command-Ask'], waiting: WAIT_CHANGE },
  { id: 'approve', keys: ['⌘↵'], label: 'Approve', kind: 'chord', screens: ['Inbox', 'Changes', 'Change-Brief', 'What-Changed', 'Repair-Loop', 'Approve-Sheet', 'Command-Ask'], wired: ['Inbox', 'Changes', 'Repair-Loop', 'What-Changed', 'Change-Brief'], waiting: WAIT_CHANGE },
  { id: 'copy', keys: ['y'], label: 'Tell the agent', kind: 'single', screens: ['Inbox', 'Changes', 'Change-Brief', 'Every-Statement', 'What-Changed', 'Repair-Loop', 'Command-Ask'], wired: ['Inbox', 'Changes', 'Repair-Loop', 'What-Changed', 'Change-Brief', 'Command-Ask', 'Every-Statement'], palette: ['Command-Ask'], waiting: WAIT_CHANGE },
  { id: 'hide', keys: ['h'], label: 'Hide until it changes', kind: 'single', screens: ['Inbox', 'Changes'], wired: ['Inbox', 'Changes'] },
  { id: 'expand', keys: ['e'], label: 'Expand / collapse', kind: 'single', screens: ['Inbox', 'Changes', 'Change-Brief'], wired: ['Inbox', 'Changes', 'Change-Brief'], waiting: WAIT_CHANGE },
  { id: 'then', keys: ['d'], label: 'Proof, then and now', kind: 'single', screens: ['Change-Brief'], wired: ['Change-Brief'] },
  { id: 'tech', keys: ['~'], label: 'Technical drawer', kind: 'single', screens: ['Change-Brief', 'Command-Ask'], wired: ['Change-Brief'], waiting: WAIT_CHANGE },
  { id: 'walk', keys: ['w'], label: 'Walk me through it', kind: 'single', screens: ['Change-Brief'], wired: ['Change-Brief'] },
  { id: 'check-half', keys: ['c'], label: 'Go to Check it again', kind: 'single', screens: ['Repair-Loop'], wired: ['Repair-Loop'] },

  // ---- in Find, ask, act (its field acts on them) ----
  { id: 'palette-move', keys: ['↑', '↓'], label: 'Move in Find, ask, act', kind: 'named', screens: ['Command-Ask'], wired: ['Command-Ask'], by: 'palette' },
  { id: 'palette-ask', keys: ['↵'], label: 'Ask the record', kind: 'named', screens: ['Command-Ask'], wired: ['Command-Ask'], by: 'palette' },
  { id: 'palette-row', keys: ['↵'], label: 'Follow a result', kind: 'named', screens: ['Command-Ask'], wired: ['Command-Ask'], by: 'palette' },
  { id: 'palette-esc', keys: ['Esc'], label: 'Clear, then close', kind: 'named', screens: ['Command-Ask'], wired: ['Command-Ask'], by: 'palette' },
])

/** The handler name for an entry's `i`th key. */
export const actOf = (entry, i = 0) => (entry.act ? entry.act[i] : entry.id)

/** Whether an entry acts on `screen` today. */
export function wiredOn(entry, screen) {
  if (entry.global) return !entry.waiting
  return !!(entry.wired && entry.wired.includes(screen))
}

/** The entries a screen shows: its own (drawn there), then the global ones. */
export function entriesFor(screen) {
  return {
    here: KEY_TABLE.filter((e) => !e.global && e.screens && e.screens.includes(screen)),
    everywhere: KEY_TABLE.filter((e) => e.global),
  }
}

/** Every handler name a screen registers when every key drawn on it is wired there. */
export function handlersWired(screen) {
  const out = []
  for (const e of KEY_TABLE) {
    if (e.global || e.by || e.go || e.palette?.includes(screen) || !wiredOn(e, screen)) continue
    e.keys.forEach((_, i) => out.push(actOf(e, i)))
  }
  return out.sort()
}

/* ---------------------------------------------------------------- tokens */

/**
 * A drawn key, as the dispatcher matches it: `{seq: ['g', 'i']}` for a
 * sequence, else `{token}`: a character, 'Enter', 'Escape', 'ArrowUp',
 * 'ArrowDown', 'Mod+k' or 'Mod+Enter'.
 */
export function tokenOf(drawn) {
  if (drawn === '⌘K') return { token: 'Mod+k' }
  if (drawn === '⌘↵') return { token: 'Mod+Enter' }
  if (drawn === '↵') return { token: 'Enter' }
  if (drawn === 'Esc') return { token: 'Escape' }
  if (drawn === '↑') return { token: 'ArrowUp' }
  if (drawn === '↓') return { token: 'ArrowDown' }
  const parts = drawn.split(' ')
  return parts.length === 2 ? { seq: parts } : { token: drawn }
}
