import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { catalog, assessResult, catalogRows, BINDINGS } from './check-contract.js'
import { adaptSession } from './adapter.js'

function valid() {
  const bindings = Object.fromEntries(BINDINGS.map(([key]) => [key, `${key}:exact-input-v1`]))
  return {
    context: { projectId: 'project-a', workspaceIdentity: 'workspace-a', inputsComplete: true, bindings: { ...bindings } },
    run: { id: 'r1', instanceId: 'instance1', checkId: 'REQ-01', version: 'v1', status: 'pass', projectId: 'project-a', workspaceIdentity: 'workspace-a',
      acceptance: { accepted: true, requirementId: 'req1', revision: 'r1', criterionId: 'c1', criterion: 'Expected output equals observed output', actor: 'owner', event: 'accepted-1', policyDigest: 'policy:exact-input-v1' },
      applicable: true, trust: { admissible: true, collector: 'recorder1', boundary: 'isolated' }, oracle: { authorized: true, identity: 'verifier1', authority: 'project-policy1', changeAuthorization: 'approval1' },
      coverage: { complete: true, truncated: false }, parserFidelity: 'complete', evidence: [{ digest: 'evidence1', range: 'bytes 0-400' }],
      completedAt: '2026-09-09T00:00:00Z', startedAt: '2026-09-08T23:59:00Z', recordedAt: '2026-09-09T00:00:01Z', integrityChainReference: 'chain1', reasonRuleId: 'rule1', invalidationDependencies: ['manifest1'], limitations: [], typedFacts: { output: 10 }, resourceMetrics: { wallTimeMs: 60000 }, workingInputs: { dirtyIncluded: true, untrackedIncluded: true, manifest: 'manifest1' },
      execution: { kind: 'process', executable: '/bin/check', executableIdentity: 'check-v1', environmentFingerprint: 'env1', argv: ['check'], cwd: '/project', exitCode: 0 },
      inputsComplete: true, bindings, criterionSatisfied: true },
  }
}

test('all 134 complete specifications and all 15 families match the supplied research', () => {
  const source = readFileSync(new URL('../../../docs/reference/ThruLine-Check-Catalog.md', import.meta.url))
  assert.equal(createHash('sha256').update(source).digest('hex'), catalog.sourceSha256)
  assert.equal(catalog.checks.length, 134); assert.equal(new Set(catalog.checks.map(c => c.id)).size, 134)
  assert.equal(catalog.families.length, 15)
  for (const family of catalog.families) assert.equal(catalog.checks.filter(c => c.family === family.id).length, family.count)
  for (const check of catalog.checks) for (const key of ['id','family','name','priority','method','cost','gate','trigger','evidence','passCriterion','failCriterion','limits','validation']) assert.ok(check[key]?.trim(), `${check.id}.${key}`)
  assert.match(catalog.contract, /acceptance/i); assert.match(catalog.references, /https:/)
})

test('absence never becomes a pass, a failure, or a completed scanner', () => {
  const rows = catalogRows({})
  assert.equal(rows.length, 134)
  assert.ok(rows.every(r => r.assessment.status === 'unknown' && !r.result && !r.history.length))
  assert.equal(assessResult({ status: 'invented-success' }).status, 'unknown')
})

test('only a versioned result envelope reaches the catalog, preserving its recorded evidence', () => {
  const { run, context } = valid()
  const explain = { shape: 'thruline.explain', schema_version: 1, session: 's', what_happened: [], verification: { contract: 'thruline.check-results.v1', results: [run], context } }
  const session = adaptSession({ key: 's', explain })
  assert.equal(session.checkResults[0], run)
  assert.equal(catalogRows(session)[0].assessment.status, 'pass')
  explain.verification.contract = 'future-format'
  assert.equal(adaptSession({ key: 's', explain }).checkResults, undefined)
})

test('a fully admitted current result supports only its accepted criterion', () => {
  const { run, context } = valid(); const before = JSON.stringify({ run, context })
  assert.equal(assessResult(run, context).status, 'pass')
  assert.equal(JSON.stringify({ run, context }), before, 'admission must not rewrite evidence')
})

for (const [key] of BINDINGS) {
  test(`a changed ${key} invalidates an older pass and preserves its original evidence`, () => {
    const { run, context } = valid(); context.bindings[key] = 'new-exact-input'
    const result = assessResult(run, context)
    assert.equal(result.status, 'stale'); assert.deepEqual(result.invalidations.map(i => i.key), [key]); assert.equal(result.observation.status, 'pass')
  })
  test(`missing ${key} binding prevents a current pass`, () => {
    const { run, context } = valid(); delete context.bindings[key]
    assert.equal(assessResult(run, context).status, 'unknown')
  })
}

for (const [label, change] of [
  ['acceptance', r => r.acceptance.accepted = false], ['accepting actor', r => delete r.acceptance.actor],
  ['criterion', r => delete r.acceptance.criterion], ['collector trust', r => r.trust.admissible = false],
  ['verifier authority', r => r.oracle.authorized = false], ['complete evidence', r => r.coverage.truncated = true],
  ['source range', r => delete r.evidence[0].range], ['parser fidelity', r => r.parserFidelity = 'partial'],
  ['process receipt', r => delete r.execution.exitCode], ['input closure', r => r.inputsComplete = false],
  ['satisfied criterion', r => r.criterionSatisfied = false],
  ['dirty file coverage', r => r.workingInputs.dirtyIncluded = false], ['untracked file coverage', r => r.workingInputs.untrackedIncluded = false],
  ['typed facts', r => delete r.typedFacts], ['integrity provenance', r => delete r.integrityChainReference],
  ['authorized verifier changes', r => delete r.oracle.changeAuthorization], ['executable identity', r => delete r.execution.executableIdentity],
]) test(`no pass without ${label}`, () => { const { run, context } = valid(); change(run); assert.equal(assessResult(run, context).status, 'unknown') })

test('an unrelated workspace result cannot establish a pass', () => {
  const { run, context } = valid(); context.workspaceIdentity = 'another-workspace'
  assert.equal(assessResult(run, context).status, 'unknown')
})

test('failure requires a concrete counterexample linked to captured evidence', () => {
  const { run, context } = valid(); run.status = 'fail'; run.execution.exitCode = 1
  assert.equal(assessResult(run, context).status, 'unknown')
  run.counterexample = { admissible: true, description: 'Expected 10, got 12', evidenceDigest: 'unrelated' }
  assert.equal(assessResult(run, context).status, 'unknown')
  run.counterexample.evidenceDigest = 'evidence1'
  assert.equal(assessResult(run, context).status, 'fail')
})

test('operational states require their own evidence and never promote to pass', () => {
  assert.equal(assessResult({ status: 'reported' }).status, 'reported')
  assert.equal(assessResult({ status: 'blocked' }).status, 'unknown')
  assert.equal(assessResult({ status: 'blocked', reason: 'Runtime unavailable' }).status, 'blocked')
  assert.equal(assessResult({ status: 'queued' }).status, 'unknown')
  assert.equal(assessResult({ status: 'queued', schedulerAcknowledgment: 'job1' }).status, 'queued')
  assert.equal(assessResult({ status: 'running', startedAt: '2026-09-09T00:00:00Z' }).status, 'running')
  assert.equal(assessResult({ status: 'not_applicable', reason: 'No database', applicabilityRule: 'rule1', policyAccepted: true }).status, 'not_applicable')
})

test('waivers remain exceptions and expiry makes them stale', () => {
  const run = { status: 'waived', waiver: { authorized: true, actor: 'owner', reason: 'Accepted tradeoff', scope: 'one criterion', at: '2026-09-01', expiresAt: '2026-09-10' } }
  assert.equal(assessResult(run, { at: '2026-09-09' }).status, 'waived')
  assert.equal(assessResult(run, { at: '2026-09-11' }).status, 'stale')
  assert.equal(assessResult(run).status, 'unknown')
})

test('a later arrival cannot erase a failure without explicit supersession', () => {
  const { run, context } = valid(); run.checkId = catalog.checks[0].id
  const older = { ...run, id: 'old', status: 'fail', counterexample: { admissible: true, description: 'Mismatch', evidenceDigest: 'evidence1' } }
  const session = { checkResults: [older, run], verificationContext: context }
  assert.equal(catalogRows(session)[0].assessment.status, 'unknown')
  run.supersedes = ['old']
  assert.equal(catalogRows(session)[0].assessment.status, 'pass')
  assert.equal(catalogRows(session)[0].history.length, 2)
})
