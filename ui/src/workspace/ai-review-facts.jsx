import React from 'react'
/* AI-REVIEW-OUT-OF-LAUNCH-R2 — the PR-Agent parts of a captured agent run's
 * facts. `vite build` resolves this file to ai-review-facts.launch.jsx, so the
 * launch bundle carries none of it; see ai-review-ui.js for the mechanism. */
export function AiReviewPatchFacts({ facts }) {
  return facts.pr_agent_options?.mode === 'plain-diff' ? <section aria-label="Supplied patch review"><h6>Native plain-diff review</h6><p>Input patch: {facts.pr_agent_options.diff_file}. Only this patch was supplied; unchanged source and commit history are outside the review.</p><p>Working-tree enrichment: {facts.pr_agent_plain_diff?.working_tree_enrichment === false ? 'disabled' : 'not established'}. Native changed files: {facts.pr_agent_plain_diff?.native_files?.map(file => file.path).join(', ') || 'not established'}.</p><p>{facts.oracle_applicability?.reason}</p><p>Review output is advisory. No source patch is applied by this provider.</p></section> : null
}
export function aiReviewDiffIdentity(facts) { return facts.pr_agent_plain_diff?.input?.sha256 }
