/* The one key dispatcher (C30 R-KEYS): what a keydown means, decided from
 * the key table (table.js) and nothing else.
 *
 * PURE apart from reading the element a key was pressed in: `decide` returns
 * what to do and does nothing itself, so a node test can drive every key, in
 * every mode, on every screen, with a fake clock (use-keys.jsx does it).
 *
 * The rules, in order:
 *  - ⌘K / Ctrl+K toggles Find, ask, act from anywhere, as it always did.
 *  - Esc closes Find, ask, act, then this map; otherwise it is "back" where a
 *    screen draws it.
 *  - While Find, ask, act or the map is open, nothing else acts (both are
 *    modal; the palette's own field takes its keys).
 *  - A key a screen already handled (`defaultPrevented`) is left alone.
 *  - No key acts while typing in a field (input, textarea, select, editable);
 *    ⌘K and Esc are the only exceptions above.
 *  - A single character acts only as Settings › Keyboard says: On (no ⌥),
 *    Only with ⌥ (⌥ held), Off (never). ↵, Esc and ⌘ chords are not single
 *    characters (WCAG 2.1.4) and act in every mode.
 *  - `g` waits SEQUENCE_MS for its second key; the second key may be pressed
 *    with or without ⌥.
 *  - A held key repeats only j and k. A held ? is not a second ? that closes the
 *    map the first opened, and a held g is the same wait, not a second key that
 *    would end it.
 *  - ↵ and ⌘↵ belong to a focused link, button or field (the browser follows
 *    it), except a list row's own select button. */
import { KEY_TABLE, SEQUENCE_MS, actOf, tokenOf, wiredOn } from './table.js'
import { href } from '../routes.js'

/** Whether an element takes typing: a text input, a textarea, a select, or anything editable. */
export function isTextField(el) {
  if (!el || el.nodeType !== 1) return false
  if (el.isContentEditable) return true
  if (el.closest && el.closest('[contenteditable]:not([contenteditable="false"])')) return true
  const tag = el.tagName
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (tag !== 'INPUT') return false
  const type = (el.getAttribute('type') || 'text').toLowerCase()
  return !['button', 'submit', 'reset', 'checkbox', 'radio', 'range', 'color', 'file', 'image'].includes(type)
}

/** Whether ↵ belongs to the focused element: a link, a button or a field — not a list row's own select button. */
export function ownsEnter(el) {
  if (!el || el.nodeType !== 1) return false
  const tag = el.tagName
  // a row a click selects (Inbox, Changes): ↵ opens it; a row that is a link (Projects) keeps its ↵
  if (tag === 'BUTTON' && el.classList.contains('tl-row')) return false
  if (tag === 'A') return el.hasAttribute('href')
  if (['BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'SUMMARY'].includes(tag)) return true
  const role = el.getAttribute('role')
  return ['button', 'link', 'option', 'menuitem', 'tab', 'checkbox', 'radio', 'switch'].includes(role)
}

/* With ⌥ held, a Mac types another character (⌥j is ∆), so the key is read from its place. */
function charOfCode(code, shift) {
  const c = typeof code === 'string' ? code : ''
  let m = /^Key([A-Z])$/.exec(c)
  if (m) return shift ? m[1] : m[1].toLowerCase()
  m = /^Digit([0-9])$/.exec(c)
  if (m) return shift ? null : m[1]
  if (c === 'Period') return shift ? null : '.'
  if (c === 'Slash') return shift ? '?' : '/'
  if (c === 'Backquote') return shift ? '~' : '`'
  return null
}

const NAMED = new Set(['Enter', 'Escape', 'ArrowUp', 'ArrowDown'])

/** A keydown as the table spells it: `{token, kind, alt}`, or null for a key the table has no use for. */
export function keyOf(e) {
  if (!e || e.isComposing) return null
  const key = typeof e.key === 'string' ? e.key : ''
  if (e.metaKey || e.ctrlKey) {
    if (key === 'Enter') return { token: 'Mod+Enter', kind: 'chord', alt: !!e.altKey }
    if (key.toLowerCase() === 'k') return { token: 'Mod+k', kind: 'chord', alt: !!e.altKey }
    return null
  }
  if (NAMED.has(key)) return { token: key, kind: 'named', alt: !!e.altKey }
  const ch = e.altKey ? charOfCode(e.code, e.shiftKey) : key.length === 1 ? key : null
  return ch ? { token: ch, kind: 'single', alt: !!e.altKey } : null
}

/** Whether a single character may act in a mode: On without ⌥, Only with ⌥ with it, Off never. */
export function singleAllowed(k, mode) {
  if (mode === 'on') return !k.alt
  if (mode === 'mod') return k.alt
  return false
}

/* token → [{entry, i}], for one key; second key → [{entry, i}], for `g` sequences */
const ONE = new Map()
const SECOND = new Map()
for (const entry of KEY_TABLE) {
  if (entry.by) continue
  entry.keys.forEach((drawn, i) => {
    const t = tokenOf(drawn)
    const into = t.seq ? SECOND : ONE
    const at = t.seq ? t.seq[1] : t.token
    if (!into.has(at)) into.set(at, [])
    into.get(at).push({ entry, i })
  })
}

const NONE = Object.freeze({ do: 'none' })
const WAIT = Object.freeze({ do: 'wait' })
const FIND = KEY_TABLE.find((x) => x.id === 'find')

/**
 * @param {{now?: () => number, within?: number}} opts
 * @returns {{decide: (e: object, ctx: object) => object, pending: () => boolean}}
 *
 * ctx: { mode: 'on'|'mod'|'off', screen: artboard name, at: route, palette: bool,
 *        overlay: bool, target: element the key was pressed in, handlers: {name: fn} }
 * The answer's `do`: 'none' · 'wait' (a `g` is waiting for its second key) ·
 * 'palette' (toggle) · 'find' (open) · 'close-palette' · 'map' · 'close-map' ·
 * 'go' (`href`) · 'face' (switch Plain / Technical) · 'call' (`act`, the screen's handler).
 */
export function createDispatcher({ now = () => Date.now(), within = SEQUENCE_MS } = {}) {
  let gAt = null

  function resolve(hits, e, ctx) {
    const screen = ctx.screen
    const at = ctx.at || {}
    // a key drawn on this screen means what the screen says before what it means everywhere;
    // Esc's "close" was settled above, so here it is never the answer
    const ordered = hits.filter((h) => !h.entry.global).concat(hits.filter((h) => h.entry.global && h.entry.id !== 'close'))
    for (const { entry, i } of ordered) {
      if (!entry.global && !(entry.screens && entry.screens.includes(screen))) continue
      if (!wiredOn(entry, screen)) return Object.assign({ entry }, NONE)
      const act = actOf(entry, i)
      const moves = act === 'next' || act === 'prev'
      if (e.repeat && !moves) return NONE
      if (entry.go) {
        if (entry.change && !at.change) return Object.assign({ entry }, NONE)
        return { do: 'go', entry, href: href(entry.go, { change: at.change, project: at.project }) }
      }
      if (entry.id === 'help') return { do: 'map', entry }
      if (entry.id === 'slash') return { do: 'find', entry }
      // t: the face is the person's stored choice, not any one screen's (data/face.js)
      if (entry.id === 'face') return { do: 'face', entry }
      const handlers = ctx.handlers || {}
      if (typeof handlers[act] !== 'function') return Object.assign({ entry }, NONE)
      return { do: 'call', entry, act }
    }
    return NONE
  }

  function decide(e, ctx) {
    const k = keyOf(e)
    if (!k) return NONE
    if (k.token === 'Mod+k') {
      gAt = null
      return { do: 'palette', entry: FIND }
    }
    if (k.token === 'Escape') {
      gAt = null
      if (ctx.palette) return { do: 'close-palette' }
      if (ctx.overlay) return { do: 'close-map' }
    }
    if (ctx.palette) return NONE
    if (ctx.overlay) {
      // the map is modal: `?` closes it as it opened it, and nothing else acts
      // a held ? repeats its keydown: the repeat is not a second press that closes what the first opened
      if (k.token === '?' && singleAllowed(k, ctx.mode) && !e.repeat) return { do: 'close-map' }
      return NONE
    }
    // Escape closes a sheet even when its note or name field has focus.
    if (e.defaultPrevented || (isTextField(ctx.target) && k.token !== 'Escape')) return NONE
    if (k.kind !== 'single') {
      gAt = null
      if ((k.token === 'Enter' || k.token === 'Mod+Enter') && ownsEnter(ctx.target)) return NONE
      const hits = ONE.get(k.token)
      return hits ? resolve(hits, e, ctx) : NONE
    }
    const t = now()
    if (gAt != null && t - gAt <= within) {
      // a held g repeats its keydown: that is the same wait, neither a second key (which would
      // end it) nor a fresh g (which would start the 1000 ms again)
      if (e.repeat && k.token === 'g') return WAIT
      // the second key of a `g` sequence: the `g` already passed the mode
      gAt = null
      const hits = SECOND.get(k.token)
      return hits ? resolve(hits, e, ctx) : NONE
    }
    gAt = null
    if (!singleAllowed(k, ctx.mode)) return NONE
    if (k.token === 'g') {
      if (e.repeat) return NONE
      gAt = t
      return WAIT
    }
    const hits = ONE.get(k.token)
    return hits ? resolve(hits, e, ctx) : NONE
  }

  return { decide, pending: () => gAt != null && now() - gAt <= within }
}
