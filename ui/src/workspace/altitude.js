/* Three altitudes, one shell (P6-1; design record §3.1, §6).
 *
 * PURE. No React, no DOM of its own: every function that touches storage or a
 * window takes it as a parameter, so a test can hand it a fake and watch.
 *
 * WHAT THIS HOLDS.
 *
 *  - The three altitudes — `brief`, `explore`, `workbench` — and the default,
 *    which is `brief`. A newcomer never sees Workbench unless they ask for it
 *    (§3.1). The third was called `pro` until A3-WORKBENCH (ruling #5,
 *    R-2026-09-26-3): "Pro" is a plan's name, so the view mode was renamed,
 *    and a remembered or addressed `pro` still reads as `workbench`.
 *  - Where the choice is remembered: one namespaced `localStorage` key, read
 *    and written inside try/catch. No storage, a blocked storage, or a value
 *    this build does not know all read as `brief`.
 *  - The rule that the remembered value is only ever what a person CHOSE.
 *    `rememberAltitude` writes nothing unless it is told the act was a choice,
 *    and an address carrying `?altitude=` is a view of one page, never a
 *    memory — so nobody is remembered as `workbench` who never pressed it.
 *  - The §6 keyboard map, as data, and one key handler that reads it. The
 *    handler ignores anything typed into a field and anything pressed with a
 *    modifier held, and it is installed once per window however many screens
 *    ask for it.
 */

/** The three altitudes, lowest first. */
export const ALTITUDES = Object.freeze(['brief', 'explore', 'workbench'])

/**
 * Ids an earlier build wrote or put in an address, and the altitude each now
 * is (A3-WORKBENCH). Read only: `rememberAltitude` writes the new id, so the
 * old one leaves storage the next time a person chooses.
 */
export const LEGACY_ALTITUDE = Object.freeze({ pro: 'workbench' })

/** An altitude id, with an earlier build's id read as today's; else null. */
function currentAltitude(v) {
  if (isAltitude(v)) return v
  return typeof v === 'string' && Object.prototype.hasOwnProperty.call(LEGACY_ALTITUDE, v) ? LEGACY_ALTITUDE[v] : null
}

/** What a person meets before they have chosen anything (§3.1). */
export const DEFAULT_ALTITUDE = 'brief'

/** The one key the choice is remembered under, on this machine only. */
export const ALTITUDE_KEY = 'thruline.shell.altitude.v1'

/** @param {unknown} v @returns {boolean} */
export const isAltitude = (v) => typeof v === 'string' && ALTITUDES.includes(v)

/**
 * The remembered altitude, or `brief`. A remembered `pro` is Workbench.
 *
 * @param {Storage|null|undefined} storage
 * @returns {'brief'|'explore'|'workbench'}
 */
export function readAltitude(storage) {
  try {
    const v = storage && typeof storage.getItem === 'function' ? storage.getItem(ALTITUDE_KEY) : null
    return currentAltitude(v) || DEFAULT_ALTITUDE
  } catch (_blocked) {
    return DEFAULT_ALTITUDE
  }
}

/**
 * Remember an altitude — only when a person chose it.
 *
 * @param {Storage|null|undefined} storage
 * @param {string} altitude
 * @param {{chosen?: boolean}} [how] `chosen: true` for a click or a key press;
 *   anything else (an address, a default, a render) writes nothing
 * @returns {boolean} whether it was written
 */
export function rememberAltitude(storage, altitude, how) {
  if (!how || how.chosen !== true || !isAltitude(altitude)) return false
  try {
    if (!storage || typeof storage.setItem !== 'function') return false
    storage.setItem(ALTITUDE_KEY, altitude)
    return true
  } catch (_blocked) {
    return false
  }
}

/**
 * The altitude an address asks for with `?altitude=`, or null. A view of one
 * page for the screen walk; it is never written to storage. An old address
 * carrying `?altitude=pro` opens Workbench.
 *
 * @param {string} search
 * @returns {'brief'|'explore'|'workbench'|null}
 */
export function altitudeFromSearch(search) {
  try {
    const v = new URLSearchParams(search || '').get('altitude')
    return currentAltitude(v)
  } catch (_bad) {
    return null
  }
}

/**
 * The altitude a screen opens at: the address's, then the remembered one,
 * then `brief`.
 *
 * @param {{search?: string, storage?: Storage|null}} [at]
 */
export function initialAltitude(at) {
  const a = at || {}
  return altitudeFromSearch(a.search || '') || readAltitude(a.storage)
}

/* ── the keyboard (§6) ─────────────────────────────────────────────────── */

/**
 * The §6 keyboard map, as data. A `wired: false` row is DECLARED and not yet
 * acting: the help sheet says it arrives later, and pressing it says so on
 * screen rather than doing nothing in silence. Since P6-3 every row acts: J
 * and K walk the thread and F focuses it wherever the Trace is drawn, and on a
 * screen with no Trace they say that there is no picture there.
 *
 * `copy` names the row's line in `HQ_COPY.shell.keys`.
 */
export const KEYMAP = Object.freeze([
  Object.freeze({ keys: Object.freeze(['j', 'k']), action: 'walk', wired: true, copy: 'walk' }),
  Object.freeze({ keys: Object.freeze(['/']), action: 'ask', wired: true, copy: 'ask' }),
  /* P6-4: the second door to Ask, the one a person who has used a command
   * bar before reaches for. `combo: true` says it is a chord rather than a
   * key: `keyAction` matches it by hand, before the rule that a held Ctrl,
   * Alt or Meta means the press was not for us, and the plain-key loop
   * below skips it so its labels never match a bare keystroke. */
  Object.freeze({ keys: Object.freeze(['\u2318K', 'Ctrl+K']), action: 'ask', wired: true, copy: 'ask_command', combo: true }),
  Object.freeze({ keys: Object.freeze(['.']), action: 'primary', wired: true, copy: 'primary' }),
  Object.freeze({ keys: Object.freeze(['t']), action: 'register', wired: true, copy: 'register' }),
  Object.freeze({ keys: Object.freeze(['f']), action: 'thread', wired: true, copy: 'thread' }),
  /* A3-WORKBENCH (ruling #5): 1 / 2 / 3 choose the altitudes, in place of
   * B, E and P — P was the old name's letter, and a digit names no word. */
  Object.freeze({ keys: Object.freeze(['1']), action: 'brief', wired: true, copy: 'brief' }),
  Object.freeze({ keys: Object.freeze(['2']), action: 'explore', wired: true, copy: 'explore' }),
  Object.freeze({ keys: Object.freeze(['3']), action: 'workbench', wired: true, copy: 'workbench' }),
  Object.freeze({ keys: Object.freeze(['Escape']), action: 'close', wired: true, copy: 'close' }),
  Object.freeze({ keys: Object.freeze(['?']), action: 'help', wired: true, copy: 'help' }),
])

/* ── the single-key switch (WCAG 2.2 SC 2.1.4, N13-KEYS-214) ─────────── */

/** The one key the single-key switch is remembered under, on this machine only. */
export const SINGLE_KEYS_KEY = 'thruline.shell.single-keys.v1'

/** Where this page holds the switch, so it works when nothing can be remembered. */
const SINGLE_KEYS_HELD = '__thrulineSingleKeys'

/**
 * Whether single-character shortcuts are on, as remembered. On unless a
 * person switched them off: no storage, a blocked storage, or a value this
 * build does not know all read as on.
 *
 * @param {Storage|null|undefined} storage
 * @returns {boolean}
 */
export function readSingleKeys(storage) {
  try {
    const v = storage && typeof storage.getItem === 'function' ? storage.getItem(SINGLE_KEYS_KEY) : null
    return v !== 'off'
  } catch (_blocked) {
    return true
  }
}

/**
 * Remember the switch. A blocked storage writes nothing and does not throw.
 *
 * @param {Storage|null|undefined} storage
 * @param {boolean} on
 * @returns {boolean} whether it was written
 */
export function rememberSingleKeys(storage, on) {
  try {
    if (!storage || typeof storage.setItem !== 'function') return false
    storage.setItem(SINGLE_KEYS_KEY, on ? 'on' : 'off')
    return true
  } catch (_blocked) {
    return false
  }
}

/** A window's storage, or null when asking for it throws. */
function storageOf(win) {
  try {
    return win ? win.localStorage : null
  } catch (_blocked) {
    return null
  }
}

/**
 * Whether single-character shortcuts act on this window: what this page
 * set, else what storage remembers, else on.
 *
 * @param {Window|any} win
 * @returns {boolean}
 */
export function singleKeysOn(win) {
  if (!win) return true
  if (typeof win[SINGLE_KEYS_HELD] === 'boolean') return win[SINGLE_KEYS_HELD]
  return readSingleKeys(storageOf(win))
}

/**
 * Switch single-character shortcuts on or off for this window, and remember it.
 *
 * @param {Window|any} win
 * @param {boolean} on
 * @returns {boolean} whether it was remembered
 */
export function setSingleKeys(win, on) {
  if (!win) return false
  win[SINGLE_KEYS_HELD] = !!on
  return rememberSingleKeys(storageOf(win), !!on)
}

/** Whether a key press landed somewhere a person is typing. */
export function isTyping(target) {
  if (!target) return false
  const tag = String(target.tagName || '').toUpperCase()
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (target.isContentEditable === true) return true
  if (typeof target.closest === 'function') {
    try {
      if (target.closest('[contenteditable=""], [contenteditable="true"], [contenteditable="plaintext-only"]')) return true
    } catch (_selector) {
      /* an element that cannot be asked is not a field */
    }
  }
  return false
}

/**
 * The map's action for one key press, or null.
 *
 * Null inside a field, null with Ctrl, Alt or Meta held, and null with Shift
 * held — except for `?`, which is Shift on most keyboards, and for a digit,
 * which is Shift on some (AZERTY): Shift is the key itself there and not a
 * modifier on it. Where Shift is not how a digit is typed, Shift+3 arrives
 * as `#` and matches nothing.
 *
 * With `singleKeys: false` (WCAG 2.2 SC 2.1.4) every printable single-key
 * row answers nothing. The ⌘K/Ctrl+K chord and Escape are not character
 * keys and still act, and the top bar's Keys button opens the list that
 * turns the letters back on.
 *
 * @param {{key: string, target?: any, ctrlKey?: boolean, altKey?: boolean,
 *   metaKey?: boolean, shiftKey?: boolean, isComposing?: boolean}} e
 * @param {{singleKeys?: boolean}} [opts] absent means on
 * @returns {string|null}
 */
export function keyAction(e, opts) {
  if (!e || typeof e.key !== 'string' || e.isComposing) return null
  if (isTyping(e.target)) return null
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key
  /* The one chord this app answers (P6-4). Exactly Cmd or Ctrl and the K —
   * with Alt or Shift also held it is somebody else's shortcut, not ours. */
  if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && key === 'k') return 'ask'
  if (e.ctrlKey || e.altKey || e.metaKey) return null
  if (e.shiftKey && key !== '?' && !/^[0-9]$/.test(key)) return null
  if (opts && opts.singleKeys === false && key !== 'Escape') return null
  for (const row of KEYMAP) if (!row.combo && row.keys.includes(key)) return row.action
  return null
}

/** The map's row for an action. */
export const keyRow = (action) => KEYMAP.find((r) => r.action === action) || null

const INSTALLED = '__thrulineShellKeys'

/**
 * Install THE key handler on a window, once.
 *
 * A second call while one is installed installs nothing and returns a cleanup
 * that does nothing, so two screens — or StrictMode's double effect — can
 * never put two handlers on the same keys. The first call's cleanup removes it
 * and frees the slot.
 *
 * @param {Window|any} win
 * @param {(action: string, e: KeyboardEvent) => void} onAction
 * @returns {() => void}
 */
export function installKeys(win, onAction) {
  if (!win || typeof win.addEventListener !== 'function' || typeof onAction !== 'function') return () => {}
  if (win[INSTALLED]) return () => {}
  const handler = (e) => {
    const action = keyAction(e, { singleKeys: singleKeysOn(win) })
    if (!action) return
    if (typeof e.preventDefault === 'function' && action !== 'close') e.preventDefault()
    onAction(action, e)
  }
  win[INSTALLED] = handler
  win.addEventListener('keydown', handler)
  return () => {
    if (win[INSTALLED] !== handler) return
    win.removeEventListener('keydown', handler)
    win[INSTALLED] = null
  }
}

/** The classes this app gives its one filled primary, and nothing else. A
 *  box that holds none of them has no main action; `.` never guesses one. */
export const PRIMARY_SELECTOR = '.ws-change-primary, .ws-verdict-primary, .ws-checks-primary, [data-primary]'

/**
 * The one primary action on the page, pressed — or, when it is off, why.
 *
 * The screen marks the container of its one primary with `data-primary-action`.
 * An enabled control inside it is clicked. An off control is not pressed; its
 * own description (the reason and what would enable it, `aria-describedby`) is
 * returned so the shell can say it, which is `.` answering rather than doing
 * nothing.
 *
 * @param {Document|any} doc
 * @returns {{pressed: boolean, reason: string|null}}
 */
export function pressPrimary(doc) {
  if (!doc || typeof doc.querySelector !== 'function') return { pressed: false, reason: null }
  const box = doc.querySelector('[data-primary-action]')
  if (!box) return { pressed: false, reason: null }
  /* A3-F8a: a live control inside `[data-no-key-primary]` is never pressed by
   * a key — the decision says so on its own box, so one `.` can never record
   * an approval or take one back. It is passed over for the next primary in
   * the box; when it is the only one, `.` presses nothing and hands back the
   * box's own sentence, which says how that act IS done. An OFF one is still
   * answered as before: focus on it or its reason, which records nothing. */
  const onByHand = (c) => !(c.disabled === true || c.getAttribute('aria-disabled') === 'true') &&
    typeof c.closest === 'function' && c.closest('[data-no-key-primary]')
  let byHand = null
  const candidates = box.matches && box.matches('button, a[href]')
    ? [box]
    : typeof box.querySelectorAll === 'function'
      ? [...box.querySelectorAll(PRIMARY_SELECTOR)]
      : [box.querySelector(PRIMARY_SELECTOR)].filter(Boolean)
  const control = candidates.find((c) => {
    const held = onByHand(c)
    if (held && !byHand) byHand = held
    return !held
  })
  if (!control) return { pressed: false, reason: byHand ? (byHand.getAttribute('data-no-key-primary') || null) : null }
  const off = control.disabled === true || control.getAttribute('aria-disabled') === 'true'
  if (!off) {
    if (typeof control.click === 'function') control.click()
    return { pressed: true, reason: null }
  }
  /* T-6-4: an off primary takes focus, so a keyboard reader lands on the
   * control that is off and its own reason is what a screen reader reads
   * next. A primary that is pressed does not: like any shortcut, `.` acts
   * without moving focus, and where the act moves the selection, focus
   * follows the selection (the workspace's own rule). */
  if (typeof control.focus === 'function') control.focus()
  /* N13-FOCUS-2411: never left under a sticky header (its box is padded). */
  if (typeof control.scrollIntoView === 'function') control.scrollIntoView({ block: 'nearest' })
  const ids = String(control.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean)
  const said = ids.map((id) => doc.getElementById(id)).filter(Boolean)
  /* A `disabled` button cannot take focus; its reason can, where it allows it. */
  if (doc.activeElement !== control && said[0] && typeof said[0].getAttribute === 'function' && said[0].getAttribute('tabindex') === '-1' && typeof said[0].focus === 'function') said[0].focus()
  const text = said.map((el) => el.textContent.trim()).join(' ')
  return { pressed: false, reason: text || null }
}

export default ALTITUDES
