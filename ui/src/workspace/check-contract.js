/* Presentation admission for the supplied catalog. This does not run scanners,
 * establish collector trust, or accept a requirement on behalf of the user.
 * Frozen evidence in -> the same scoped state out. Raw observations survive. */
import catalog from './check-catalog.json' with { type: 'json' }

export { catalog }
export const RESULT_STATES = Object.freeze({
  reported: ['Reported', 'Someone said it worked. The evidence has not established that yet.'],
  queued: ['Queued', 'The check was accepted but has not started.'],
  running: ['Running', 'The check started. Its final result has not arrived.'],
  pass: ['Passed', 'Current evidence meets this accepted requirement for the stated scope.'],
  fail: ['Failed', 'The evidence shows a specific requirement was not met.'],
  unknown: ['Not established', 'There is not enough usable evidence to decide.'],
  stale: ['Needs a fresh check', 'Something relevant changed after this result.'],
  blocked: ['Could not run', 'A named problem prevented this check from finishing.'],
  not_applicable: ['Does not apply', 'An explicit rule excludes this check from the stated scope.'],
  waived: ['Exception accepted', 'An authorized person accepted an exception. This is not a pass.'],
})

export const BINDINGS = Object.freeze([
  ['artifact', 'Code and files', 'Includes relevant unsaved, dirty, and untracked inputs.'],
  ['dependencies', 'Dependencies', 'The resolved dependency versions used by the check.'],
  ['environment', 'Environment', 'The relevant runtime, configuration, and platform.'],
  ['requirement', 'Accepted requirement', 'The accepted requirement and observable success criterion.'],
  ['oracle', 'Verifier rules', 'An identified verifier with the authority required by this check.'],
  ['adapter', 'Evidence reader', 'The adapter and parser version that interpreted the result.'],
  ['dataset', 'Test data', 'The dataset or explicit recorded reason it is not applicable.'],
  ['policy', 'Project policy', 'The accepted applicability and evidence rules.'],
])

const present = v => typeof v === 'string' && v.trim().length > 0
export function assessResult(run, context = {}) {
  if (!run) return { status: 'unknown', reasons: ['No result has been recorded for this check.'], invalidations: [], observation: null }
  const reasons = [], invalidations = []
  const answer = status => ({ status, reasons, invalidations, observation: run })
  if (!RESULT_STATES[run.status]) { reasons.push('The result uses a state this build cannot interpret.'); return answer('unknown') }
  if (run.status === 'reported') { reasons.push(RESULT_STATES.reported[1]); return answer('reported') }
  if (run.status === 'blocked') { reasons.push(run.reason || 'The verifier did not supply a reason for the blockage.'); return answer(present(run.reason) ? 'blocked' : 'unknown') }
  if (['queued', 'running'].includes(run.status)) {
    const observed = run.status === 'queued' ? run.schedulerAcknowledgment : run.startedAt
    if (!present(observed)) { reasons.push('No event establishes the displayed execution state.'); return answer('unknown') }
    reasons.push(RESULT_STATES[run.status][1]); return answer(run.status)
  }
  if (run.status === 'not_applicable') {
    const valid = present(run.applicabilityRule) && present(run.reason) && run.policyAccepted === true
    reasons.push(valid ? run.reason : 'An accepted applicability rule and its reason are required.'); return answer(valid ? 'not_applicable' : 'unknown')
  }
  if (run.status === 'waived') {
    const valid = run.waiver?.authorized === true && ['actor', 'reason', 'scope', 'at', 'expiresAt'].every(k => present(run.waiver[k]))
    if (!valid) { reasons.push('The exception needs an authorized actor, scope, reason, date, and expiry.'); return answer('unknown') }
    if (!present(context.at) || !Number.isFinite(Date.parse(context.at)) || !Number.isFinite(Date.parse(run.waiver.expiresAt))) {
      reasons.push('The exception expiry cannot be checked against this reading.'); return answer('unknown')
    }
    if (Date.parse(run.waiver.expiresAt) <= Date.parse(context.at)) { reasons.push('The accepted exception has expired.'); return answer('stale') }
    reasons.push('This is an accepted exception, not a passing check.'); return answer('waived')
  }
  if (run.status === 'stale') { reasons.push(run.reason || 'A relevant input was invalidated.'); return answer('stale') }

  if (run.acceptance?.accepted !== true || !['requirementId', 'revision', 'criterionId', 'criterion', 'actor', 'event'].every(k => present(run.acceptance?.[k]))) reasons.push('No attributable acceptance establishes the requirement and its success criterion.')
  if (run.applicable !== true) reasons.push('Applicability has not been established by the project policy.')
  const spec = catalog.checks.find(c => c.id === run.checkId)
  if (!spec) reasons.push('The result does not identify a check in this catalog.')
  if (!present(run.instanceId) || !present(run.acceptance?.policyDigest) || !present(run.recordedAt) || !present(run.integrityChainReference) || !present(run.reasonRuleId)) reasons.push('The check instance, accepted policy, or history provenance is incomplete.')
  if (!Array.isArray(run.invalidationDependencies) || !run.invalidationDependencies.length || !Array.isArray(run.limitations) || !run.typedFacts || typeof run.typedFacts !== 'object' || !run.resourceMetrics || typeof run.resourceMetrics !== 'object') reasons.push('Typed facts, limitations, resource observations, or invalidation dependencies are missing.')
  if (run.workingInputs?.dirtyIncluded !== true || run.workingInputs?.untrackedIncluded !== true || !present(run.workingInputs?.manifest)) reasons.push('Relevant dirty and untracked files are not explicitly included in the tested inputs.')
  if (!present(run.id) || !present(run.checkId) || !present(run.version) || !present(run.projectId) || !present(run.workspaceIdentity)) reasons.push('The check, result, project, or workspace identity is incomplete.')
  if (context.projectId !== run.projectId || context.workspaceIdentity !== run.workspaceIdentity) reasons.push('The result is not bound to this project and workspace.')
  if (run.trust?.admissible !== true || !present(run.trust?.collector) || !present(run.trust?.boundary)) reasons.push('The collector identity and required trust boundary have not been established.')
  if (run.oracle?.authorized !== true || !present(run.oracle?.identity) || !present(run.oracle?.authority)) reasons.push('The verifier and its authority have not been established.')
  if (!present(run.oracle?.changeAuthorization)) reasons.push('The verifier rules need an attributable authorization.')
  if ((run.oracle?.humanReview === true || /human adjudicated/i.test(spec?.method || '')) && !['actor', 'decision', 'scope', 'at'].every(k => present(run.review?.[k]))) reasons.push('This advisory check needs an identified reviewer and a scoped decision.')
  if (run.coverage?.complete !== true || run.coverage?.truncated !== false || run.parserFidelity !== 'complete' || !Array.isArray(run.evidence) || !run.evidence.length || run.evidence.some(e => !present(e.digest) || !present(e.range))) reasons.push('The required evidence, exact source ranges, or interpretation are incomplete.')
  if (!present(run.completedAt) || (run.execution?.kind === 'process' ? !Number.isInteger(run.execution.exitCode) || !present(run.execution.executable) || !Array.isArray(run.execution.argv) || !present(run.execution.cwd) : !present(run.execution?.notApplicableReason))) reasons.push('A completed execution or an explicit non-process explanation is missing.')
  if (run.execution?.kind === 'process' && (!present(run.startedAt) || !present(run.execution.executableIdentity) || !present(run.execution.environmentFingerprint))) reasons.push('Execution timing, executable identity, or the effective environment fingerprint is missing.')
  if (run.inputsComplete !== true || context.inputsComplete !== true) reasons.push('The relevant inputs are not completely identified; freshness cannot be established.')
  for (const [key, label] of BINDINGS) {
    const before = run.bindings?.[key], now = context.bindings?.[key]
    if (!present(before) || !present(now)) reasons.push(`${label}: the tested and current inputs are not both identified.`)
    else if (before !== now) invalidations.push({ key, label, before, after: now, reason: `${label} changed after this result.` })
  }
  if (invalidations.length) { reasons.push(...invalidations.map(i => i.reason)); return answer('stale') }
  if (reasons.length) return answer('unknown')
  if (run.status === 'fail' && (run.counterexample?.admissible !== true || !present(run.counterexample?.description) || !run.evidence.some(e => e.digest === run.counterexample?.evidenceDigest))) { reasons.push('A failure needs a concrete admissible counterexample linked to the captured evidence. Missing evidence does not establish a bug.'); return answer('unknown') }
  if (run.status === 'pass' && run.criterionSatisfied !== true) { reasons.push('The evidence does not establish the accepted success criterion.'); return answer('unknown') }
  reasons.push(RESULT_STATES[run.status][1]); return answer(run.status)
}

export function catalogRows(session) {
  const runs = Array.isArray(session.checkResults) ? session.checkResults : []
  const context = session.verificationContext || {}
  return catalog.checks.map(spec => {
    const history = runs.filter(r => r.checkId === spec.id)
    // The producer must identify supersession. Arrival order is not authority.
    const superseded = new Set(history.flatMap(r => r.supersedes || []))
    const current = history.filter(r => !superseded.has(r.id))
    const assessment = current.length > 1
      ? { status: 'unknown', reasons: ['Multiple results exist without an explicit supersession relationship.'], invalidations: [] }
      : assessResult(current[0], context)
    return { ...spec, history, result: current.length === 1 ? current[0] : null, assessment, availability: history.length ? 'Result recorded' : 'Specification · no result' }
  })
}
