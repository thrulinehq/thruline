/* The one display formatter for paths (sign-off polish) and the plain words
 * a quote is drawn from. Synthetic paths only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { displayPath, homePath, homeless, middle, repoTops } from './paths.js'
import { plainText, stripDirectives, placePath } from './change.js'

const ROOT = '/Users/someone/shop'

test('inside the project root a path reads relative to it', () => {
  assert.equal(displayPath(ROOT + '/src/api.py', { root: ROOT }), 'src/api.py')
  assert.equal(displayPath('/Users/someone/shop-wt/src/a.py', { root: ROOT, members: ['/Users/someone/shop-wt'] }), 'shop-wt/src/a.py')
})

test('inside another checkout of the same repository it reads <checkout>/<rel>', () => {
  const tops = repoTops([ROOT + '/scripts/x.mjs', ROOT + '/qual/notes.md'], ROOT)
  assert.equal(displayPath('/Users/someone/Developer/orbit-dev-20260927/wt/scripts/q/t.mjs', { root: ROOT, tops }), 'orbit-dev-20260927/wt/scripts/q/t.mjs')
  assert.equal(displayPath('/Users/someone/shop-cloud-a/crates/core/src/x.rs', { root: ROOT }), 'shop-cloud-a/crates/core/src/x.rs')
  // a folder of the home folder that only looks like a repository top is not a checkout
  assert.equal(displayPath('/Users/someone/src/a.py', { root: ROOT }), '~/src/a.py')
  assert.equal(displayPath('/Users/someone/.cargo/bin/x', { root: ROOT }), '~/.cargo/bin/x')
})

test('anywhere else under the home folder it reads ~/…; other paths as recorded', () => {
  assert.equal(displayPath('/Users/someone/Downloads/notes.txt', { root: ROOT }), '~/Downloads/notes.txt')
  assert.equal(displayPath('/home/someone/x/y.txt', {}), '~/x/y.txt')
  assert.equal(displayPath('/private/tmp/a.txt', { root: ROOT }), '/private/tmp/a.txt')
  assert.equal(homePath('/Users/someone'), '~')
  assert.equal(homePath('/opt/x'), '/opt/x')
})

test('words: every home-folder prefix becomes ~, a quoted one still runs in a shell, nothing else moves', () => {
  assert.equal(homeless('ran in /Users/someone/shop, then /home/me/x.'), 'ran in ~/shop, then ~/x.')
  assert.equal(homeless("cd '/Users/someone/a b' && ls"), "cd ~/'a b' && ls")
  assert.equal(homeless('cd "/Users/someone/x"'), 'cd ~/"x"')
  assert.equal(homeless('qual/runs/r1/home/.bench/x'), 'qual/runs/r1/home/.bench/x', 'a "home" folder mid-path is not the home folder')
  assert.equal(homeless('see file:///Users/someone/a.md'), 'see file://~/a.md')
  assert.equal(homeless(null), null)
})

test('middle keeps the file name and the first part, and only shortens what is long', () => {
  assert.equal(middle('src/api.py', 30), 'src/api.py')
  assert.equal(middle('orbit-dev-20260927/wt/scripts/qualification/test/rulings.test.mjs', 48), 'orbit-dev-20260927/…/test/rulings.test.mjs')
  assert.equal(middle('orbit-dev-20260927/wt/scripts/qualification/test/rulings.test.mjs', 30), '…/test/rulings.test.mjs')
  for (const n of [20, 30, 40, 48]) assert.ok(middle('a/' + 'b'.repeat(80) + '/c/d/file-name.js', n).length <= n, String(n))
  const marker = 'left out by your exclusion (`src/private-file.rs`)'
  assert.equal(middle(marker, 20), marker, 'an exclusion marker is never shortened as a path')
})

test('the Change model shows a path outside every root through the one formatter', () => {
  assert.equal(placePath('/Users/someone/Developer/x/qual/STATUS.md', ROOT, []).file, '~/Developer/x/qual/STATUS.md')
  assert.equal(placePath(ROOT + '/.claude/worktrees/w1/src/a.py', ROOT, []).worktree, 'w1')
})

test('a Codex directive never reaches the plain words; its words do', () => {
  assert.equal(plainText(':codex-annotation{index="1"} I carried out the work.'), 'I carried out the work.')
  assert.equal(plainText('::git-stage{cwd="/x"}\nTests pass.'), 'Tests pass.')
  assert.equal(plainText(':::note\nTests pass.\n:::'), 'Tests pass.\n')
  assert.equal(plainText('See :cite[the run]{id=4} for it.'), 'See the run for it.')
  assert.equal(plainText('Done.:codex-annotation{index="2"}'), 'Done.')
  // ordinary colons are words
  assert.equal(stripDirectives('Result: 4 passed, at 12:30 {ok}.'), 'Result: 4 passed, at 12:30 {ok}.')
  assert.equal(stripDirectives('see http://x.test/a:b{c}'), 'see http://x.test/a:b{c}')
})
