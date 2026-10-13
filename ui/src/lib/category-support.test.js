import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  RECORDED_ARTIFACT_LIMIT,
  artifactDetailId,
  recordedArtifactRegionId,
  recordedDetailId,
  sessionCategorySupport,
  unresolvedArtifactWindow,
} from './category-support.js'
import { workMap } from './work-map.js'

const representativePath = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

function artifact(key, fate, sessions, display = `safe/${key}.js`, exact = false) {
  return {
    key,
    fate,
    sessions,
    filePresentation: { display, label: 'Project-relative path', outside: false, relation: 'exact evidence path + cwd' },
    exactPath: {
      clusterMatches: exact ? [{ key: 'cluster:0' }] : [],
      evidenceMatches: [],
    },
  }
}

test('representative support is exact, session-scoped, path-safe, and maps every unresolved artifact once', () => {
  const map = workMap(JSON.parse(readFileSync(representativePath, 'utf8')))
  const sessionId = map.sessions[0].session.id
  const support = sessionCategorySupport(map, sessionId)

  assert.equal(support.status, 'ready')
  assert.deepEqual(
    { clusters: support.happened.clusters, boundaries: support.happened.boundaries },
    { clusters: 26, boundaries: 17 },
  )
  assert.equal(support.missing.tails, 0)
  assert.equal(support.missing.artifacts.length, 18)
  assert.equal(support.lagging.represented, false)
  assert.equal(support.contradictory.represented, false)

  const expected = map.artifacts.unresolved
    .filter((item) => item.fate === 'unresolved' && item.sessions.includes(sessionId))
    .map((item) => item.key)
  assert.deepEqual(support.missing.artifacts.map((item) => item.key), expected)
  assert.equal(new Set(expected).size, 18)
  assert.equal(Object.values(map.artifacts).flat().filter((item) => expected.includes(item.key)).length, 18)
  for (const item of support.missing.artifacts) {
    assert.equal(item.detailId, artifactDetailId(item.key))
    assert.equal(item.href, `#${item.detailId}`)
    assert.doesNotMatch(JSON.stringify(item), /\/Users\/|[A-Za-z]:[\\/]/)
  }
  assert.doesNotMatch(JSON.stringify(support), /\/Users\//)
})

test('multi-session support uses exact membership and never promotes unknown fate or empty sessions', () => {
  const forbidden = 'candidate interpretation must never enter support'
  const map = {
    status: 'ready',
    sessions: [
      { session: { id: 's:one' }, clusters: [{ interpretation: forbidden }], boundaries: [{}], tails: [{ interpretation: forbidden }] },
      { session: { id: 's-two' }, clusters: [{}, {}], boundaries: [], tails: [] },
    ],
    artifacts: {
      unresolved: [
        artifact('artifact:0', 'unresolved', ['s:one'], 'safe/one.js', true),
        artifact('artifact:1', 'unresolved', ['s-two']),
        artifact('artifact:2', 'unresolved', ['s:one', 's-two']),
        artifact('artifact:3', 'unresolved', []),
        artifact('artifact:4', 'unknown', ['s:one']),
      ],
      exists: [artifact('artifact:5', 'exists', ['s:one'])],
      other: [artifact('artifact:6', 'future', ['s:one'])],
    },
  }

  const one = sessionCategorySupport(map, 's:one')
  const two = sessionCategorySupport(map, 's-two')
  assert.deepEqual(one.missing.artifacts.map((item) => item.key), ['artifact:0', 'artifact:2'])
  assert.deepEqual(two.missing.artifacts.map((item) => item.key), ['artifact:1', 'artifact:2'])
  assert.deepEqual([one.happened.clusters, one.happened.boundaries, one.missing.tails], [1, 1, 1])
  assert.deepEqual([two.happened.clusters, two.happened.boundaries, two.missing.tails], [2, 0, 0])
  assert.equal(one.missing.artifacts[0].exactPathRelation, true)
  assert.equal(one.missing.artifacts[1].exactPathRelation, false)
  assert.doesNotMatch(JSON.stringify(one) + JSON.stringify(two), new RegExp(forbidden))
  assert.equal(sessionCategorySupport(map, 'missing').status, 'unavailable')
})

test('unresolved artifact disclosure is bounded, exact, ordered, and reversible', () => {
  assert.equal(RECORDED_ARTIFACT_LIMIT, 4)
  for (const total of [0, 4]) {
    const input = Array.from({ length: total }, (_, index) => artifactNavigation(index))
    const window = unresolvedArtifactWindow(input)
    assert.equal(window.shown, total)
    assert.equal(window.hasDisclosure, false)
    assert.equal(window.controlLabel, '')
  }

  for (const total of [5, 18]) {
    const input = Array.from({ length: total }, (_, index) => artifactNavigation(index))
    const closed = unresolvedArtifactWindow(input)
    const open = unresolvedArtifactWindow(input, true)
    const collapsed = unresolvedArtifactWindow(input, false)
    assert.deepEqual(closed.items.map((item) => item.key), input.slice(0, 4).map((item) => item.key))
    assert.equal(closed.controlLabel, `Show all ${total} unresolved artifacts`)
    assert.equal(open.shown, total)
    assert.equal(open.items.at(-1), input.at(-1))
    assert.equal(open.controlLabel, 'Show fewer unresolved artifacts')
    assert.deepEqual(collapsed.items, closed.items)
  }
})

test('session and artifact navigation ids are stable and fragment-safe', () => {
  assert.equal(recordedDetailId('s:one'), 'wm-recorded-s_3a_one')
  assert.equal(recordedArtifactRegionId('s:one'), 'wm-unresolved-artifacts-s_3a_one')
  assert.equal(artifactDetailId('artifact:17'), 'wm-artifact_3a_17')
})

test('Work Map keeps recorded support separate from model proposals and every compact artifact link targets its one detail card', () => {
  const component = readFileSync(fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)), 'utf8')
  const css = readFileSync(fileURLToPath(new URL('../macos.css', import.meta.url)), 'utf8')

  assert.match(component, /Recorded structure at export/)
  assert.match(component, /Recorded unresolved signals at export/)
  assert.match(component, /These are recorded unresolved signals, not proof of missing work\./)
  assert.match(component, /represents no deterministic \{relation\} relation for this run\. This is not proof of absence\./)
  assert.match(component, /<h5>Model-proposed interpretation<\/h5>/)
  assert.match(component, /<RelationLabel kind="recorded" detail="Deterministic support" \/>/)
  assert.match(component, /<RelationLabel kind="model_proposed" detail="Optional shadow" \/>/)
  assert.doesNotMatch(component, /kind="model_proposed" detail="Shadow branch"/)
  assert.match(component, /<RecordedCategorySupport[\s\S]*<section className="wm-category-model"/)
  assert.match(component, /Not analyzed locally\. No local-model row is shown for this category\./)
  assert.match(component, /sessionCategorySupport\(map, group\.session\.id\)/)
  assert.match(component, /<a href=\{item\.href\}>[\s\S]*<FileDisplay presentation=\{item\.filePresentation\}/)
  assert.match(component, /<article id=\{artifactDetailId\(item\.key\)\} className="wm-item wm-artifact">/)
  assert.match(component, /<details id=\{recordedDetailId\(group\.session\.id\)\}/)
  assert.match(component, /aria-expanded=\{window\.expanded\}/)
  assert.match(component, /aria-controls=\{regionId\}/)
  assert.match(component, /event\.currentTarget\.focus\(\)/)
  assert.doesNotMatch(component, /support\.artifacts[^\n]*\.file\b|item\.file\}/)
  assert.match(css, /\.wm-category-source[^}]*min-width:\s*0/)
  assert.match(css, /\.wm-artifact-support-list[^}]*grid-template-columns:\s*repeat\(2, minmax\(0, 1fr\)\)/)
  assert.match(css, /@media \(max-width: 600px\)[\s\S]*\.wm-artifact-support-list[^}]*grid-template-columns:\s*1fr/)
})

function artifactNavigation(index) {
  const key = `artifact:${index}`
  return { key, detailId: artifactDetailId(key), href: `#${artifactDetailId(key)}` }
}
