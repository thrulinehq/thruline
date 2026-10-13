import { test } from 'node:test'
import assert from 'node:assert/strict'
import { editorHref } from './editor.js'

test('the Brief forms a VS Code file link from only an absolute recorded path and line', () => {
  assert.equal(editorHref('/Users/me/a repo/src/cart #1.py', 7), 'vscode://file/Users/me/a%20repo/src/cart%20%231.py:7:1')
  assert.equal(editorHref('/Users/me/repo/src/cart.py', null), 'vscode://file/Users/me/repo/src/cart.py')
})

test('the Brief refuses paths that cannot safely name a local file', () => {
  for (const path of ['src/cart.py', '~/repo/src/cart.py', '/repo/../private/key', '/repo/./cart.py', '/repo/a\\b', '/repo/a\nb', 'left out by your exclusion (`env`)']) {
    assert.equal(editorHref(path, 7), null, path)
  }
  assert.equal(editorHref('/repo/cart.py', 0), null)
})
