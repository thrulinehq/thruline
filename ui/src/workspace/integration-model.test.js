import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { integrationView, toolStatus, groupPacks, quoteArgument, executionProblem } from './integration-model.js'
test('tool readings are scoped to the selected session and exact contract', () => {
  const value = { schema: 'thruline.integration-workspace.v1', session: 'a', catalog: { projects: [] }, results: { contract: 'thruline.integration-observation.v1', runs: [] } }
  assert.equal(integrationView(value, 'a'), value)
  assert.throws(() => integrationView(value, 'b'))
  assert.throws(() => integrationView({ ...value, results: {} }, 'a'))
})
test('a missing assessment or invalid state is never presented as successful', () => {
  assert.equal(toolStatus({ record: { decode_error: 'missing report' } }).state, 'unknown')
  assert.equal(toolStatus({ assessment: { state: 'proven' } }).state, 'unknown')
  assert.match(toolStatus({ assessment: { state: 'passed' } }).meaning, /not a whole-product verdict/)
})
test('execution problems explain interruption without turning it into a finding', () => {
  const record={plan:{pack:'osv-scanner',timeout_seconds:30,memory_mb:256},admission:{run_state:'completed'},stderr_excerpt:'could not load db for crates.io'}
  assert.match(executionProblem(record).why,/database could not be loaded/)
  assert.match(executionProblem({...record,admission:{run_state:'cancelled'}}).why,/cancelled/)
  assert.match(executionProblem({...record,isolation:{state:{OOMKilled:true}}}).why,/256 MiB/)
  assert.equal(executionProblem({plan:{pack:'playwright'},admission:{run_state:'completed'}}),null)
})
test('33 upstream sources form nine groups without default-enabled scans', () => {
  const c = JSON.parse(readFileSync(new URL('../../../spec/integrations/catalog.json', import.meta.url)))
  assert.equal(c.projects.length, 33); assert.equal(groupPacks(c.projects).length, 9)
  assert.ok(c.projects.every(p => p.default_enabled === false))
})
test('captured native advice is distinct from an imported report and verified behavior', () => {
  const assessment = { state: 'reported' }
  assert.match(toolStatus({ assessment }).label, /Imported report/)
  const native = toolStatus({ assessment, record: { admission: { captured: true } } })
  assert.match(native.label, /Captured advisory observation/)
  assert.match(native.meaning, /independent behavior checks/)
  assert.equal(native.state, 'reported')
})
test('copied commands keep paths as literal shell arguments', () => {
  assert.equal(quoteArgument("/tmp/it's a project"), "'/tmp/it'\\''s a project'")
  assert.equal(quoteArgument('$(touch /tmp/unsafe)'), "'$(touch /tmp/unsafe)'")
})

test('catalog mapping accounts for 134 distinct checks without graduating candidate tools', async () => {
  const { readFile } = await import('node:fs/promises')
  const coverage = JSON.parse(await readFile(new URL('../../../spec/integrations/coverage.json', import.meta.url)))
  assert.equal(coverage.checks.length, 134)
  assert.equal(new Set(coverage.checks.map(c => c.id)).size, 134)
  for (const row of coverage.checks) {
    assert.equal(row.qualified, false)
    assert.equal(row.catalog_status, 'proposed')
    assert.match(row.requirement_verification, /not established/)
    for (const id of row.capabilities) assert.ok(coverage.capabilities.some(c => c.id === id && c.check_ids.includes(row.id)))
  }
  assert.deepEqual(coverage.checks.find(c => c.id === 'SEC-08').capabilities, ['dated-advisory-inventory'], 'only the exercised dated OSV capability maps here')
  assert.ok(!coverage.capabilities.find(c => c.id === 'declared-dependency-policy').check_ids.includes('SEC-08'), 'bans/sources does not imply vulnerability coverage')
})
