/* contentLeaks(source, demo, opts) — the demo record's facts that are written
 * literally into a screen's source, outside its design-only parts.
 *
 * Design-only parts are what the template gates on `v.isDemo`: the
 * `{v?.isDemo ? (<> … </>) : null}` blocks and the demo branch of
 * `{v?.isDemo ? "…" : "…"}`. A leaf of `demo` (every string, recursively, but
 * not under `opts.ignoreKeys`, nor the value of an enum key such as `kind` or
 * `state`) shorter than 5 characters or in `opts.skip` is not checked: such
 * words are vocabulary, not facts of one record.
 * Pure; no file access. Kept as one copy per screen folder on purpose: each
 * screen agent owns only its own folder. */
export function stripDesignOnly(src) {
  let out = ''
  let i = 0
  const OPEN = '{v?.isDemo ? (<>'
  for (;;) {
    const g = src.indexOf(OPEN, i)
    if (g < 0) { out += src.slice(i); break }
    out += src.slice(i, g)
    let depth = 1
    let j = g + OPEN.length
    while (depth > 0) {
      const o = src.indexOf('<>', j)
      const c = src.indexOf('</>', j)
      if (c < 0) throw new Error('unbalanced isDemo gate at ' + g)
      if (o >= 0 && o < c) { depth += 1; j = o + 2 } else { depth -= 1; j = c + 3 }
    }
    const tail = ') : null}'
    if (!src.startsWith(tail, j)) throw new Error('isDemo gate not closed by ") : null}" at ' + j)
    i = j + tail.length
  }
  return out.replace(/\{v\?\.isDemo \? "(?:[^"\\]|\\.)*" :/g, '{')
}

function stripComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:'"])\/\/[^\n]*/g, '$1')
}

// enum-valued fields: their values are the product's vocabulary, which the logic must name
const ENUM_KEYS = ['kind', 'key', 'id', 'state', 'action', 'mark', 'swatch', 'status', 'evidence', 'nearProofStop']

function leaves(x, ignoreKeys, out = []) {
  if (typeof x === 'string') out.push(...x.split('\n').map((l) => l.trim()))
  else if (Array.isArray(x)) x.forEach((y) => leaves(y, ignoreKeys, out))
  else if (x && typeof x === 'object') Object.keys(x).forEach((k) => { if (!ignoreKeys.includes(k) && !ENUM_KEYS.includes(k)) leaves(x[k], ignoreKeys, out) })
  return out
}

export function contentLeaks(source, demo, { skip = new Set(), ignoreKeys = [], source: kind = 'jsx' } = {}) {
  const text = kind === 'js' ? stripComments(source) : stripComments(stripDesignOnly(source))
  const found = new Set()
  for (const leaf of leaves(demo, ignoreKeys)) {
    if (leaf.length < 5 || skip.has(leaf)) continue
    const lit = JSON.stringify(leaf).slice(1, -1)
    if (text.includes(leaf) || text.includes(lit)) found.add(leaf)
  }
  return [...found].sort()
}
