/* The control census (P6-1) — how many controls the workspace draws, and what
 * each is called, so "nothing was deleted from Workbench" is a count and not a claim.
 *
 * PURE over a parsed document. It counts every element a person can operate —
 * `button`, `a[href]`, `input`, `select`, `textarea`, `summary` — and names each
 * the way assistive technology would first reach for: `aria-label`, then its
 * text, then `placeholder`, then `title`. Whitespace is collapsed and nothing
 * else is normalised, so a renamed control is a missing name.
 *
 * The rendering it is applied to is fixed here too — one synthetic session
 * (three steps, one claim with one finding), 1440px wide, the default panels —
 * so the baseline taken at HEAD dc47818 and the count taken after P6-1 are
 * counts of the same screen. Every value below is synthetic.
 */

export const CONTROL_SELECTOR = 'button, a[href], input, select, textarea, summary'

const SESSION_ID = 'synthetic-session-0001'

/** The two documents the synthetic session is adapted from. */
export const SYNTHETIC = Object.freeze({
  id: SESSION_ID,
  explain: {
    shape: 'thruline.explain', schema_version: 1, session: SESSION_ID,
    what_happened: [
      { seq: 1, kind: 'user_prompt', text: 'Make the upload retry.' },
      { seq: 2, kind: 'tool_call', text: 'Edit src/upload.js' },
      { seq: 3, kind: 'stop', text: 'stopped: Finished' },
    ],
    claims_vs_proof: [], promises: [], risk_flags: [],
  },
  claims: {
    shape: 'thruline.claims', schema_version: 1, session: SESSION_ID,
    claims: [{ id: 'c1', text: 'All tests pass.', findings: [{ detector: 'test_run_failed', severity: 'warn', detail: 'A test failed.' }] }],
  },
})

/** The accessible name this census reads for one element. */
export function controlName(el) {
  const raw = el.getAttribute('aria-label') || el.textContent || el.getAttribute('placeholder') || el.getAttribute('title') || ''
  return raw.trim().replace(/\s+/g, ' ')
}

/**
 * Every control in a document, in document order.
 *
 * @param {Document} doc
 * @returns {Array<{tag: string, name: string}>}
 */
export function census(doc) {
  return [...doc.querySelectorAll(CONTROL_SELECTOR)].map((el) => ({ tag: el.tagName.toLowerCase(), name: controlName(el) }))
}

/** A multiset of names: name → how many controls carry it. */
export function nameCounts(rows) {
  const out = {}
  for (const r of rows) out[r.name] = (out[r.name] || 0) + 1
  return out
}

/** Names in `before` that `after` carries fewer of: what a change removed. */
export function missingNames(before, after) {
  const a = nameCounts(after)
  const out = []
  for (const [name, n] of Object.entries(nameCounts(before))) if ((a[name] || 0) < n) out.push({ name, before: n, after: a[name] || 0 })
  return out
}

/**
 * The workspace, rendered to static markup at one altitude, over the synthetic
 * session. `M` is a bundle exporting `Workspace`, `adaptSession` and
 * `renderToStaticMarkup` from one React; a Workspace from before P6-1 simply
 * ignores `altitude`.
 *
 * @param {{Workspace: any, adaptSession: Function, html: Function}} M
 * @param {'brief'|'explore'|'workbench'} altitude
 * @param {object} [over] extra state, e.g. `{ narrow: true }`
 * @returns {string}
 */
export function renderWorkspaceHtml(M, altitude, over) {
  const session = M.adaptSession({ explain: SYNTHETIC.explain, claims: SYNTHETIC.claims, key: SESSION_ID })
  const w = new M.Workspace({ index: { projects: [], totals: {}, kind: 'index', contract_version: 1 } })
  w.state = {
    ...w.state,
    altitude,
    session: SESSION_ID,
    corpus: { order: [SESSION_ID], sessions: { [SESSION_ID]: session } },
    dataStatus: 'ready',
    ...(over || {}),
  }
  return M.html(w.render())
}

/* ── A3-F10 · the inert-control census ─────────────────────────────────
 *
 * "A control census reports 0 inert controls." An INERT control is an enabled
 * `button`, `[role=button]`, `input` or `select` that (i) has no React click
 * or keyboard handler and is not a submit button inside a form, or (ii) has a
 * handler that changes nothing a person can observe — no DOM change, no
 * request, no clipboard write, no navigation, no focus move. `a[href]` and
 * `summary` are live by construction and are not counted here.
 *
 * The current option of a group — `aria-pressed`, `aria-checked` or
 * `aria-selected` true, or `aria-current` set — is live by construction too:
 * choosing what is already chosen changes nothing, and that is the control
 * working, not a dead one.
 */

export const ACTIONABLE_SELECTOR = 'button, [role="button"], input, select'

/** `={() => {}}` as a JSX prop: a control handed an act that does nothing. */
export const NOOP_HANDLER_PROP = /=\{\s*\(\s*\)\s*=>\s*\{\s*\}\s*\}/

const HANDLERS = ['onClick', 'onMouseDown', 'onMouseUp', 'onPointerDown', 'onPointerUp', 'onKeyDown', 'onKeyUp']
const VALUE_HANDLERS = ['onChange', 'onInput']
const isNoop = (fn) => typeof fn === 'function' && /^(?:function\s*\w*)?\s*\(\s*\)\s*(?:=>)?\s*\{\s*\}$/.test(fn.toString().trim())

/** The props React rendered this element with, or `{}`. */
export function reactProps(el) {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'))
  return key ? el[key] || {} : {}
}

const isOff = (el) => el.disabled === true || el.getAttribute('aria-disabled') === 'true' || Boolean(el.closest('fieldset[disabled]'))
const isHidden = (el) => el.tagName === 'INPUT' && el.type === 'hidden'

/** Is this control the current option of a group? */
export function isCurrentOption(el) {
  const on = (a) => el.getAttribute(a) === 'true'
  const current = el.getAttribute('aria-current')
  return on('aria-pressed') || on('aria-checked') || on('aria-selected') || (current !== null && current !== 'false')
}

/** Every enabled control the inert census judges, in document order. */
export function actionableControls(doc) {
  return [...doc.querySelectorAll(ACTIONABLE_SELECTOR)].filter((el) => {
    if (isOff(el) || isHidden(el)) return false
    if (el.tagName === 'A' && el.hasAttribute('href')) return false
    if (el.tagName === 'SUMMARY') return false
    return true
  })
}

/**
 * Part (i): every enabled control with no React act of its own, or whose act
 * is `() => {}`. Pure over a document React rendered.
 *
 * @param {Document} doc
 * @returns {Array<{tag: string, name: string, why: string}>}
 */
export function inertControls(doc) {
  const out = []
  for (const el of actionableControls(doc)) {
    const tag = el.tagName.toLowerCase()
    const p = reactProps(el)
    if (tag === 'button' && el.type === 'submit' && el.closest('form')) continue
    const own = [...HANDLERS, ...((tag === 'input' || tag === 'select') ? VALUE_HANDLERS : [])].map((k) => p[k]).filter((f) => typeof f === 'function')
    if ((tag === 'input' || tag === 'select') && (p.readOnly || el.readOnly)) continue
    if (!own.length) { out.push({ tag, name: controlName(el), why: 'no handler' }); continue }
    if (own.every(isNoop)) out.push({ tag, name: controlName(el), why: 'a handler that does nothing' })
  }
  return out
}

/**
 * Part (ii): click one control and say what a person could observe happen.
 * `effects` is the harness's own list — it pushes every request, clipboard
 * write, download and window it is asked for — and this adds DOM changes,
 * navigation and a focus move. An empty answer is an inert control. A
 * `[role=button]` that is not a `button` is also pressed with Enter, since
 * that is the other way a person operates it.
 *
 * @param {Window} win
 * @param {Element} el
 * @param {{effects: string[], settle: () => Promise<void>}} harness
 * @returns {Promise<string[]>}
 */
export async function observeClick(win, el, { effects, settle }) {
  if (isCurrentOption(el)) return ['current option']
  const doc = win.document
  if (typeof el.focus === 'function') el.focus()
  await settle()
  const act = async (fire) => {
    const start = { href: win.location.href, active: doc.activeElement, n: effects.length }
    const seen = []
    const mo = new win.MutationObserver((records) => { seen.push(...records) })
    mo.observe(doc, { subtree: true, childList: true, attributes: true, characterData: true })
    fire()
    await settle()
    seen.push(...mo.takeRecords())
    mo.disconnect()
    const out = effects.slice(start.n)
    if (seen.length) out.push('dom')
    if (win.location.href !== start.href) out.push('navigation')
    if (doc.activeElement !== start.active) out.push('focus')
    return out
  }
  const clicked = await act(() => el.click())
  if (clicked.length || el.tagName === 'BUTTON' || !el.isConnected) return clicked
  return act(() => el.dispatchEvent(new win.KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })))
}

export default census

/* ── N13-DRAG-257 — every drag has a single-pointer alternative ──────────
 *
 * WCAG 2.2 SC 2.5.7 (Dragging Movements). Every `onMouseDown` /
 * `onPointerDown` in a workspace `.jsx` file is listed here, by its file and
 * its handler's source, either as a DRAG with the single-pointer (click)
 * alternative that does what the drag does, or as a press that is NOT A DRAG.
 * `a11y-drag.test.js` fails on an unlisted press and on a stale row, and
 * operates each alternative with a click. The arrow keys each drag already
 * had stay; a key alternative alone does not satisfy 2.5.7.
 */
export const DRAG_ALTERNATIVES = Object.freeze([
  Object.freeze({
    press: 'canvas.jsx · handlers.canvasDown',
    drag: 'pan the canvas',
    alternative: 'one press on the overview moves the view there (Workbench, Minimap on); Fit and the zoom buttons; selecting a card or a row centres it',
    test: 'a11y-drag.test.js: pan',
  }),
  Object.freeze({
    press: 'canvas.jsx · handlers.onMinimapDown',
    drag: 'pan with the overview',
    alternative: 'the press itself jumps the view (shell.jsx minimapTo, shared by the press and the drag)',
    test: 'a11y-drag.test.js: pan',
  }),
  Object.freeze({
    press: 'canvas.jsx · onDown',
    drag: 'move a card',
    alternative: "the inspector's Move this card buttons (left, right, up, down; the side panel's Trace tab), one arrow-key nudge per click",
    test: 'a11y-drag.test.js: move a card',
  }),
  Object.freeze({
    press: 'docks.jsx · onDown',
    drag: 'resize a dock',
    alternative: "the separator's two step buttons, one arrow-key step per click within the drag's own clamp",
    test: 'a11y-drag.test.js: resize a dock',
  }),
])

export const NOT_A_DRAG = Object.freeze([
  Object.freeze({ press: 'canvas.jsx · (e) => { e.stopPropagation() }', why: "the group card's open/close button stops the card's drag from starting under it; the button acts on click" }),
  Object.freeze({ press: 'change-workspace.jsx · (e) => { if (e.target === e.currentTarget) onClose() }', why: "a press on the backdrop closes the approval's confirming sheet (A3-F8a) and records nothing; nothing moves" }),
  Object.freeze({ press: 'hq.jsx · e => { if (e.target === e.currentTarget) onClose() }', why: 'a press on the backdrop closes the dialog; nothing moves' }),
  Object.freeze({ press: 'run-sheet.jsx · (e) => { if (e.target === e.currentTarget) onClose() }', why: 'a press on the backdrop closes the sheet; nothing moves' }),
  Object.freeze({ press: 'shell-chrome.jsx · (e) => { if (e.target === e.currentTarget) onClose() }', why: 'a press on the backdrop closes the sheet; nothing moves' }),
])
