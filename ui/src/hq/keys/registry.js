/* How a screen hands the dispatcher its own keys (C30 R-KEYS).
 *
 * A screen supplies handlers by name — the names in the key table's `act`
 * (table.js): `next`, `prev`, `open`, `primary`, `approve`, `copy`, `expand`,
 * … A handler acts and returns nothing, returns an address for the dispatcher
 * to go to, or returns `false` when there is nothing to act on (the key then
 * passes to the page untouched). The dispatcher calls a handler only on a
 * screen the table says the key is wired on.
 *
 * Ported screens use `withKeys(Logic)`: the logic's `renderVals()` returns its
 * handlers as `keys`, and the latest render is what a key reaches. Anything
 * else may call `provideKeys(owner, handlers)` and `releaseKeys(owner)`. The
 * design gallery never wraps a screen, so nothing there registers.
 */
let owner = null
let handlers = {}

/** Make `h` the keys of the screen in front; `who` is whatever releases them. */
export function provideKeys(who, h) {
  owner = who
  handlers = h && typeof h === 'object' ? h : {}
}

/** Drop `who`'s keys, if they are still the ones in front. */
export function releaseKeys(who) {
  if (owner !== who) return
  owner = null
  handlers = {}
}

/** The handlers of the screen in front (an empty object when none). */
export function currentKeys() {
  return handlers
}

/** A ported screen's logic, its `keys` handed to the dispatcher on every render. */
export function withKeys(Logic) {
  return class Keyed extends Logic {
    renderVals() {
      const v = super.renderVals()
      provideKeys(this, v && v.keys)
      return v
    }
    componentWillUnmount() {
      releaseKeys(this)
      if (super.componentWillUnmount) super.componentWillUnmount()
    }
  }
}
