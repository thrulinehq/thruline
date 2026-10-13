/* R-2026-09-23-14 — `ui/package.json`'s `test` script names a glob instead of
 * 77 paths, because four lanes conflicted on that one line and four merges
 * remain. A glob is only as good as its coverage, and a guard that lives
 * *inside* the glob cannot notice that the glob stopped selecting it: narrow
 * the pattern and the guard simply never runs. So the coverage guard lives
 * here, out of tree, named explicitly in the same `test` script, where no
 * change to the pattern can silence it.
 *
 * Run: node --test scripts/testing/ui-test-script.test.mjs
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { globSync } from 'node:fs'
import path from 'node:path'

const UI = path.resolve(import.meta.dirname, '../../ui')

/** Every `.test.js` under `ui/src`, found without reference to the pattern. */
async function walk(dir) {
  const found = []
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) found.push(...(await walk(full)))
    else if (entry.name.endsWith('.test.js')) found.push(path.relative(UI, full))
  }
  return found
}

test('the test script is a glob plus explicit out-of-tree entries, not a file list', async () => {
  const pkg = JSON.parse(await fs.readFile(path.join(UI, 'package.json'), 'utf8'))
  const script = pkg.scripts.test
  const args = script.replace(/^node --test /, '').trim().split(/\s+/)
  assert.ok(script.startsWith('node --test '), 'the test script no longer starts with `node --test`')
  const quoted = args.filter((a) => a.startsWith('"') && a.endsWith('"')).map((a) => a.slice(1, -1))
  assert.equal(quoted.length, 1, 'expected exactly one quoted glob pattern in the test script')
  const outOfTree = args.filter((a) => a.startsWith('../'))
  assert.ok(outOfTree.length >= 1, 'the out-of-tree suites are no longer named in the test script')
  for (const entry of outOfTree) {
    await fs.access(path.resolve(UI, entry))
  }
  assert.ok(
    outOfTree.includes('../scripts/testing/ui-test-script.test.mjs'),
    'this file is no longer named in the test script — the coverage guard would go silent',
  )
})

test('the glob selects every .test.js under ui/src, by count and by name', async () => {
  const pkg = JSON.parse(await fs.readFile(path.join(UI, 'package.json'), 'utf8'))
  const pattern = pkg.scripts.test.match(/"([^"]+)"/)[1]
  const selected = globSync(pattern, { cwd: UI })
    .map((entry) => entry.split(path.sep).join('/'))
    .sort()
  const onDisk = (await walk(path.join(UI, 'src')))
    .map((entry) => entry.split(path.sep).join('/'))
    .sort()
  assert.deepEqual(selected, onDisk, 'the test-script glob and the files on disk disagree')
  assert.equal(selected.length, onDisk.length)
  assert.ok(onDisk.length >= 77, `expected at least the 77 suites of R-2026-09-23-14, found ${onDisk.length}`)
})
