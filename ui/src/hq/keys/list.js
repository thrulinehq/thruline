/* The list keys (C30 R-KEYS: j k ↵ . ⌘↵ y e), as handlers a list screen
 * registers (registry.js). Each key does what a click on the same row does,
 * through the same handler or the same link; a row with nothing for a key
 * answers `false`, and the key does nothing.
 *
 * `rowKeys` serves the Inbox and Changes, whose rows are selected by a click.
 * `focusKeys` serves Projects, whose rows are links: there the keyboard's
 * place is focus, and ↵ on a focused row is the link's own. */

/**
 * @param {{rows: Array<{id: string, kind: string, t: object, select: Function, copy?: Function, hide?: Function}>,
 *          selId: string|null, expand?: Function}} list
 *   rows    the rows on screen, in the order they are drawn
 *   selId   the row selected now (it may be one the filter hides)
 *   expand  what `e` toggles ("Nothing needed")
 */
export function rowKeys({ rows, selId, expand }) {
  const at = rows.findIndex((r) => r.id === selId)
  const sel = at >= 0 ? rows[at] : null
  const step = (d) => () => {
    if (!rows.length) return false
    const i = at < 0 ? 0 : Math.max(0, Math.min(rows.length - 1, at + d))
    if (i === at) return false
    rows[i].select()
    return undefined
  }
  const t = (sel && sel.t) || {}
  return {
    next: step(1),
    prev: step(-1),
    // ↵: the row's own "Open the Change →"
    open: () => t.openHref || false,
    // .: "▶ Check it again", on a row that draws it
    primary: () => (sel && sel.kind === 'check' && t.checkHref) || false,
    // ⌘↵: "◇ Approve", on a row that draws it
    approve: () => (sel && sel.kind === 'approve' && t.approveHref) || false,
    // y: "Tell the agent" on a check row (the same address as its Check it again); "Copy it again" on the agent's row
    copy: () => {
      if (sel && sel.kind === 'check') return t.checkHref || false
      if (sel && sel.kind === 'agent' && typeof sel.copy === 'function') { sel.copy(); return undefined }
      return false
    },
    hide: () => {
      if (!sel || typeof sel.hide !== 'function') return false
      return sel.hide()
    },
    expand: () => {
      if (typeof expand !== 'function') return false
      expand()
      return undefined
    },
  }
}

/**
 * @param {{list: string, doc?: Document}} where
 *   list  a selector for the list whose rows are links (each `[role="listitem"]`)
 */
export function focusKeys({ list, doc = typeof document !== 'undefined' ? document : null }) {
  const rows = () => (doc ? [...doc.querySelectorAll(list + ' > [role="listitem"]')] : [])
  const here = (all) => all.indexOf(doc && doc.activeElement)
  const step = (d) => () => {
    const all = rows()
    if (!all.length) return false
    // from the focused row, else from the row drawn as the one you are in, else the first row
    const i = here(all)
    const from = i >= 0 ? i : all.findIndex((a) => a.getAttribute('aria-current') === 'true')
    const to = from < 0 ? 0 : Math.max(0, Math.min(all.length - 1, from + d))
    all[to].focus()
    return undefined
  }
  return {
    next: step(1),
    prev: step(-1),
    // ↵ with no row focused: the row drawn as the one you are in
    open: () => {
      const cur = rows().find((a) => a.getAttribute('aria-current') === 'true')
      return (cur && cur.getAttribute('href')) || false
    },
  }
}
