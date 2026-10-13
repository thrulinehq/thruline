/* The one way a path reaches a person (HQ Next sign-off polish).
 *
 * PURE. Every screen, and every field of the Change snapshot a screen draws a
 * path from, shows a path through this file, so no screen draws the home
 * folder ("/Users/<name>/…", "/home/<name>/…"):
 *
 *   inside the project root            → relative to it        "src/api.py"
 *   inside another checkout of the same
 *   repository (its trailing structure
 *   matches the project's)             → "<checkout>/<rel>"    "orbit-dev-20260927/wt/scripts/x.mjs"
 *   anywhere else under the home folder → "~/…"
 *   any other path                      → as recorded
 *
 * `middle` shortens a long path in its middle, keeping the file name; the
 * screen that uses it puts the whole display path in title=.
 *
 * Words that carry paths (a quote, a command, a message a person copies)
 * go through `homeless`, which writes the home folder as `~` and changes
 * nothing else. A command keeps working when pasted: a quoted home path
 * becomes `~/'rest'`, which a shell still expands.
 */

/** The home-folder prefix of an absolute path: "/Users/<name>" or "/home/<name>". */
const HOME_AT_START = /^\/(?:Users|home)\/[^/\s'"`]+(?=\/|$)/
/** The same prefix anywhere in words: after a start, a space, a quote, `(`, `=`, `:`, `,`, or `//` (file:///…). */
const HOME_IN_TEXT = /(^|[\s(=:,[{<>|;&]|\/\/)(['"`]?)\/(?:Users|home)\/[^/\s'"`)]+(?=[/\s'"`),.;:]|$)/gm

/** "/Users/<me>/code/x" → "~/code/x"; anything else unchanged. */
export function homePath(path) {
  if (typeof path !== 'string') return path
  return path.replace(HOME_AT_START, '~')
}

/** Free words with every home-folder prefix written as `~`. A quoted path
 *  keeps working in a shell: `'/Users/me/a b'` → `~/'a b'`. */
export function homeless(text) {
  if (typeof text !== 'string') return text
  return text.replace(HOME_IN_TEXT, (_m, lead, q) => lead + (q ? '~' + q : '~')).replace(/~(['"`])\//g, '~/$1')
}

const trim = (r) => String(r || '').replace(/\/+$/, '')

/** The server's fixed stand-in is display text, never a path to shorten. */
export const isExclusionMarker = (value) => typeof value === 'string' && value.startsWith('left out by your exclusion (`') && value.endsWith('`)')

/** Folder names a repository commonly holds at its top: a checkout is
 *  recognised by one of these (or by a top folder seen inside the project)
 *  sitting right under it. */
export const REPO_TOPS = Object.freeze(['src', 'crates', 'ui', 'scripts', 'docs', 'tests', 'test', 'lib', 'packages', 'apps', 'app', 'cmd', 'internal', 'pkg', 'bin', 'tools', 'web', 'server', 'client', '.github', '.thruline'])

/** The folders at the top of the project, as the record shows them: the first
 *  part of every given path inside the root, plus the common ones. */
export function repoTops(paths, root) {
  const r = trim(root)
  const tops = new Set(REPO_TOPS)
  for (const p of paths || []) {
    if (typeof p !== 'string' || !r || !p.startsWith(r + '/')) continue
    const first = p.slice(r.length + 1).split('/')[0]
    if (first && p.slice(r.length + 1).includes('/')) tops.add(first)
  }
  return tops
}

/** A short folder name ("wt", "repo") says little alone: the checkout is
 *  then named with its parent. */
const THIN = /^(wt|worktree|worktrees|checkout|repo|main|code|src)$/

/**
 * A path as a person reads it.
 * @param {string} path
 * @param {{root?: string|null, members?: string[], tops?: Set<string>}} [ctx]
 * @returns {string}
 */
export function displayPath(path, ctx = {}) {
  if (typeof path !== 'string' || !path) return path
  const roots = [trim(ctx.root)].concat((ctx.members || []).map(trim)).filter(Boolean)
  let hit = null
  for (const r of roots) if (path.startsWith(r + '/') && (!hit || r.length > hit.length)) hit = r
  if (hit) {
    const rest = path.slice(hit.length + 1)
    return hit === trim(ctx.root) ? rest : base(hit) + '/' + rest
  }
  if (!HOME_AT_START.test(path)) return path
  // under the home folder: another checkout of this repository?
  const tops = ctx.tops || new Set(REPO_TOPS)
  const parts = path.replace(HOME_AT_START, '').split('/').filter(Boolean)
  for (let j = 1; j < parts.length - 1; j++) {
    if (!tops.has(parts[j]) || parts[j - 1].startsWith('.')) continue
    const from = j >= 2 && THIN.test(parts[j - 1]) && !parts[j - 2].startsWith('.') ? j - 2 : j - 1
    return parts.slice(from).join('/')
  }
  return homePath(path)
}

const base = (p) => trim(p).slice(trim(p).lastIndexOf('/') + 1)

/** A long path shortened in its middle, keeping its first part and its file
 *  name: "orbit-dev-20260927/…/test/rulings.test.mjs". The caller puts the
 *  whole path in title=. */
export function middle(path, max = 48) {
  if (isExclusionMarker(path)) return path
  if (typeof path !== 'string' || path.length <= max) return path
  const parts = path.split('/')
  const file = parts[parts.length - 1]
  if (parts.length < 3 || file.length + 2 > max) return '…' + path.slice(path.length - max + 1)
  // the longest tail of whole folders that fits, with the first part when it fits too
  const tailFrom = (room) => {
    let tail = file
    for (let i = parts.length - 2; i > 0; i--) {
      if (parts[i].length + 1 + tail.length > room) break
      tail = parts[i] + '/' + tail
    }
    return tail
  }
  const head = parts[0]
  if (head.length + 3 + file.length <= max) return head + '/…/' + tailFrom(max - head.length - 3)
  return '…/' + tailFrom(max - 2)
}
