import { AI_REVIEW_IN_UI, isAiReviewPack } from './ai-review-ui.js'
export const TOOL_STATES = Object.freeze({
  passed: ['✓ Condition met', 'The captured run met its specific tool condition. This is not a whole-product verdict.'],
  failed: ['× Condition not met', 'Review the cited result before concluding that the product has a defect.'],
  stale: ['↻ Needs a fresh run', 'An input changed after this evidence was captured.'],
  blocked: ['⊘ Could not finish', 'An execution or acceptance problem prevents a usable answer.'],
  unknown: ['? Not established', 'The evidence does not answer the accepted condition.'],
  reported: ['“ Imported report', 'The report was supplied, but its execution was not captured.'],
})
export function integrationView(value, sessionId) {
  if (value?.schema !== 'thruline.integration-workspace.v1' || value.session !== sessionId || !Array.isArray(value.catalog?.projects)) throw new Error('This tool reading does not match the selected session.')
  if (value.results && (value.results.contract !== 'thruline.integration-observation.v1' || !Array.isArray(value.results.runs))) throw new Error('The tool result contract is not recognized.')
  return value
}
export function toolStatus(row) {
  const status = row?.assessment?.state
  if (status === 'reported' && row?.record?.admission?.captured === true) return { state: status, label: '“ Captured advisory observation', meaning: row.assessment.explanation || 'The native execution was captured. Its advice or proposal still needs independent behavior checks.' }
  return TOOL_STATES[status] ? { state: status, label: TOOL_STATES[status][0], meaning: row.assessment.explanation || TOOL_STATES[status][1] } : { state: 'unknown', label: TOOL_STATES.unknown[0], meaning: row?.record?.decode_error || 'The output could not be interpreted. No successful result is implied.' }
}
export function groupPacks(packs) {
  const groups = new Map()
  for (const pack of packs) {
    if (!groups.has(pack.feature)) groups.set(pack.feature, { id: pack.feature, name: pack.feature_name, packs: [] })
    groups.get(pack.feature).packs.push(pack)
  }
  return [...groups.values()]
}
export function quoteArgument(value) { return "'" + String(value).replaceAll("'", "'\\''") + "'" }
export function executionProblem(record) {
  if (record?.isolation?.state?.OOMKilled) return { why: `The isolated check exceeded its ${record.plan.memory_mb} MiB memory budget.`, next: 'Reduce the test scope or review a larger budget, then run the check again.' }
  const state = record?.admission?.run_state
  if (state === 'timed_out') return { why: `The check exceeded its ${record.plan.timeout_seconds}-second time budget.`, next: 'Check for a hang, reduce the selected scope, or review the budget before rerunning.' }
  if (state === 'cancelled') return { why: 'This check was cancelled before it could give a complete answer.', next: 'Run it again when you are ready. Its partial receipt remains available.' }
  if (state === 'resource_limit') return { why: 'The check reached a resource limit and was stopped.', next: 'Inspect the recorded resource limit, reduce the scope or review its budget, then rerun.' }
  if (state === 'output_limit') return { why: 'The check produced more output than its capture budget could retain.', next: 'Narrow the check or review the output budget, then capture a complete report.' }
  if (record?.plan?.pack === 'osv-scanner' && /no offline version|could not load db/.test(record.stderr_excerpt || '')) return { why: 'The advisory database could not be loaded. This does not mean your dependencies are clear.', next: 'Provision or repair the dated database shown in setup, review it, then rerun the advisory check.' }
  return null
}

/* AI-REVIEW-OUT-OF-LAUNCH (founder ruling R-2026-09-28-20). Whether AI review
 * (the PR-Agent route) is offered is the server's to say, as
 * `workflow.ai_review`; anything short of an explicit `true` is the launch
 * build, where no PR-Agent recipe, source or control reaches the screen —
 * not even a reading from a server that still lists one.
 * AI-REVIEW-OUT-OF-LAUNCH-R2: which packs are AI review, and whether this UI
 * build carries AI review at all, come from ai-review-ui.js, which the launch
 * bundle replaces with a twin that names no such pack. */
export function aiReviewOffered(workflow) { return AI_REVIEW_IN_UI && workflow?.ai_review === true }
export function offeredPacks(packs, workflow) { return aiReviewOffered(workflow) ? packs : packs.filter(p => !isAiReviewPack(p.id)) }
export function offeredWorkflow(workflow) {
  if (!workflow || aiReviewOffered(workflow) || !Array.isArray(workflow.recipes)) return workflow
  return { ...workflow, recipes: workflow.recipes.filter(r => !isAiReviewPack(r.id)) }
}
