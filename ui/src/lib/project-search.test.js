import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { projectSearchResult } from './project-search.js'

const PROJECTS = [
  { fixture: 'a', name: 'Alpha workspace' },
  { fixture: 'b', name: 'Beta tools' },
  { fixture: 'c', name: 'Gamma API' },
]

test('project search is case-insensitive, deterministic, and reports exact coverage', () => {
  const result = projectSearchResult(PROJECTS, ' TOOLS ')
  assert.deepEqual(result.matching.map((project) => project.fixture), ['b'])
  assert.equal(result.matchCount, 1)
  assert.equal(result.totalCount, 3)
  assert.equal(result.query, 'tools')
})

test('a truthful zero result keeps the current project visible exactly once', () => {
  const result = projectSearchResult(PROJECTS, 'not present', 'b')
  assert.equal(result.matchCount, 0)
  assert.equal(result.currentPinned, true)
  assert.deepEqual(result.visible.map((project) => project.fixture), ['b'])
})

test('a matching current project is not duplicated and clearing reaches all projects', () => {
  const matching = projectSearchResult(PROJECTS, 'beta', 'b')
  assert.deepEqual(matching.visible.map((project) => project.fixture), ['b'])
  assert.equal(matching.currentPinned, false)
  assert.deepEqual(projectSearchResult(PROJECTS).visible, PROJECTS)
})

test('both product shells use the same bounded derivation without truncating project reachability', () => {
  const mac = readFileSync(fileURLToPath(new URL('../mac/shell.jsx', import.meta.url)), 'utf8')
  const os = readFileSync(fileURLToPath(new URL('../os/shell.jsx', import.meta.url)), 'utf8')
  for (const [name, source] of [['mac/shell.jsx', mac], ['os/shell.jsx', os]]) {
    assert.match(source, /projectSearchResult\(projects, projectSearch,/)
    assert.match(source, /No project names match/)
    assert.ok(!/projects\.slice\(0,\s*14\)/.test(source), `${name} hides projects behind a fixed slice`)
  }
  assert.equal([...mac.matchAll(/const \[projectSearch, setProjectSearch\]/g)].length, 1)
  assert.equal([...os.matchAll(/const \[projectSearch, setProjectSearch\]/g)].length, 1)
})
