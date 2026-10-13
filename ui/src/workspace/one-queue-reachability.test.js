import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/* A3-COUNT-LEGACY-QUEUES — the prototype's picker / sidebar / status-bar
 * counts are not in the shipped product, and cannot come back unannounced.
 *
 * Report `#ui-counts` / `#ui-picker`; fix-list rows F2 ("A property test:
 * every surface's count is equal") and F14 ("Picker count = Inbox count").
 * The half of those rows that does NOT reproduce: the per-statement queue
 * "N things worth a look" (`lib/inbox.js` `inboxQueue`), rendered by
 * `os/shell.jsx`, and the per-project "Needs attention" picker in
 * `mac/shell.jsx`, are reachable only from the mock-tooling gallery
 * (`main.jsx`: `VITE_MOCK_TOOLING === '1'` and `?gallery`). `thruline ui`
 * never mounts them, so their counts never meet the Inbox's.
 *
 * Predeclared in the card: a test asserts that no module reachable from a
 * product entry point — `main.jsx`'s boot paths other than the gallery —
 * imports `os/shell.jsx`, `mac/shell.jsx`, or `lib/inbox.js`'s
 * `inboxQueue`, so a second per-statement attention queue cannot return to
 * the product unannounced.
 *
 * The walk is static: every `import … from '…'`, `export … from '…'` and
 * `import('…')` with a relative specifier, from `main.jsx`, skipping exactly
 * one edge — the gated gallery import — and asserting that edge is still
 * gated.
 */

const here = dirname(fileURLToPath(import.meta.url))
const SRC = resolve(here, '..')
const MAIN = join(SRC, 'main.jsx')
const GALLERY = join(SRC, 'gallery.jsx')
const FORBIDDEN = [join(SRC, 'os/shell.jsx'), join(SRC, 'mac/shell.jsx')]

const stripComments = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:\\'"`])\/\/.*$/gm, '$1')

function resolveSpec(from, spec) {
  const base = resolve(dirname(from), spec)
  for (const candidate of [base, base + '.js', base + '.jsx', base + '.mjs', join(base, 'index.js'), join(base, 'index.jsx')]) {
    if (existsSync(candidate) && statSync(candidate).isFile()) return candidate
  }
  return null
}

/** Every relative module `file` imports, with the names it imports when they are named. */
function importsOf(file) {
  const src = stripComments(readFileSync(file, 'utf8'))
  const out = []
  for (const m of src.matchAll(/(?:^|\n)\s*(?:import|export)\s+([\s\S]*?)\s+from\s+['"](\.{1,2}\/[^'"]+)['"]/g)) out.push({ spec: m[2], names: m[1] })
  for (const m of src.matchAll(/(?:^|\n)\s*import\s+['"](\.{1,2}\/[^'"]+)['"]/g)) out.push({ spec: m[1], names: '' })
  for (const m of src.matchAll(/import\(\s*['"](\.{1,2}\/[^'"]+)['"]\s*\)/g)) out.push({ spec: m[1], names: '', dynamic: true })
  return out
}

/** The modules a product boot can reach from main.jsx, and every edge taken. */
function reachable() {
  const seen = new Set()
  const edges = []
  const queue = [MAIN]
  while (queue.length) {
    const file = queue.shift()
    if (seen.has(file)) continue
    seen.add(file)
    if (!/\.(jsx?|mjs)$/.test(file)) continue
    for (const imp of importsOf(file)) {
      const to = resolveSpec(file, imp.spec)
      if (!to) continue
      /* The one skipped edge: main.jsx's gated gallery import. */
      if (file === MAIN && to === GALLERY) continue
      edges.push({ from: file, to, names: imp.names })
      if (!seen.has(to)) queue.push(to)
    }
  }
  return { seen, edges }
}

test('LEGACY · the gallery is still the only door to the prototype shells, and it is still gated behind mock tooling', () => {
  const main = readFileSync(MAIN, 'utf8')
  assert.match(main, /if \(import\.meta\.env\.VITE_MOCK_TOOLING === '1' && new URLSearchParams\(location\.search\)\.has\('gallery'\)\) \{\s*\n\s*const \{ default: Gallery \} = await import\('\.\/gallery\.jsx'\)/, 'the gallery import is no longer behind the mock-tooling gate')
  assert.equal((stripComments(main).match(/gallery\.jsx/g) || []).length, 1, 'main.jsx reaches the gallery by a second path')
})

test('LEGACY · no module a product boot reaches imports os/shell.jsx, mac/shell.jsx or inboxQueue', () => {
  const { seen, edges } = reachable()
  const rel = (f) => relative(SRC, f)
  assert.ok(seen.size >= 40, `the walk reached only ${seen.size} modules — it is not reading the product`)
  for (const expected of ['workspace/shell.jsx', 'workspace/inbox.jsx', 'workspace/change-workspace.jsx', 'workspace/first-use.jsx', 'workspace/change-way-in.jsx', 'workspace/attention.js']) {
    assert.ok(seen.has(join(SRC, expected)), `the walk did not reach ${expected}, which the product mounts`)
  }
  const hits = FORBIDDEN.filter((f) => seen.has(f)).map((f) => {
    const via = edges.filter((e) => e.to === f).map((e) => rel(e.from))
    return `${rel(f)} (imported by ${via.join(', ')})`
  })
  assert.deepEqual(hits, [], 'a prototype shell with its own attention count is reachable from the product')
  const queue = edges.filter((e) => e.to === join(SRC, 'lib/inbox.js') && /\binboxQueue\b/.test(e.names)).map((e) => rel(e.from))
  assert.deepEqual(queue, [], 'a product module imports the per-statement attention queue')
})

test('LEGACY · the guard is live: the same walk sees the prototype shells and the queue where they are imported', () => {
  const walk = (start) => {
    const seen = new Set()
    const edges = []
    const queue = [start]
    while (queue.length) {
      const file = queue.shift()
      if (seen.has(file)) continue
      seen.add(file)
      if (!/\.(jsx?|mjs)$/.test(file)) continue
      for (const imp of importsOf(file)) {
        const to = resolveSpec(file, imp.spec)
        if (!to) continue
        edges.push({ from: file, to, names: imp.names })
        queue.push(to)
      }
    }
    return { seen, edges }
  }
  /* The gallery reaches the per-project picker; os/shell.jsx (which nothing
   * outside the gallery's own tests mounts) reaches the per-statement queue.
   * If the walk could not see these edges, its silence about the product
   * would mean nothing. */
  assert.ok(walk(GALLERY).seen.has(join(SRC, 'mac/shell.jsx')), 'the walk cannot see mac/shell.jsx even from the gallery')
  const os = walk(join(SRC, 'os/shell.jsx'))
  assert.ok(os.seen.has(join(SRC, 'mac/shell.jsx')), 'the walk cannot see os/shell.jsx importing mac/shell.jsx')
  assert.ok(os.edges.some((e) => e.to === join(SRC, 'lib/inbox.js') && /\binboxQueue\b/.test(e.names)), 'the walk cannot see an inboxQueue import')
})
