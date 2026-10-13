import React from 'react'
import { IntegrationSupport } from './integration-support.jsx'
import { AI_REVIEW_TITLES } from './ai-review-ui.js'
const TITLES = { megalinter: 'Check Python and YAML together', sonarqube: 'Import the completed local analysis', clippy: 'Check selected Rust features', joern: 'Inspect a selected native graph query', openrewrite: 'Preview a selected Java or Properties recipe', infer: 'Trace captured Java defects', aider: 'Propose a repair with Aider', 'openhands-sdk': 'Inspect an OpenHands repair session', 'mini-swe-agent': 'Attempt a bounded local repair', opengrep: 'Trace intrafile source-to-sink flows', ruff: 'Check Python imports and variables', 'ast-grep': 'Check the source rule', 'cargo-deny': 'Check dependency policy', 'dependency-cruiser': 'Check module boundaries', knip: 'Review unused code', betterleaks: 'Look for secret candidates', 'cargo-mutants': 'Check whether tests catch mistakes', playwright: 'Check the user workflow', schemathesis: 'Check the API contract', semgrep: 'Check reviewed security rules', 'osv-scanner': 'Check known dependency advisories' }

export function GuidedChecks({ sessionId, workflow, onRefresh }) {
  const [busy, setBusy] = React.useState('')
  const [review, setReview] = React.useState(null)
  const [message, setMessage] = React.useState('')
  const [error, setError] = React.useState('')
  const mounted = React.useRef(true)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  async function act(action, recipe, digest = '') {
    setError(''); setMessage(''); if (action !== 'cancel') setBusy(`${action}:${recipe}`)
    try {
      const response = await fetch('/api/integrations/action', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ session: sessionId, action, recipe, digest }) })
      const data = await response.json()
      if (!response.ok) throw new Error(data.error || 'The check request did not complete. Refresh its receipts before retrying.')
      if (!mounted.current) return
      if (action === 'preview') setReview({ recipe, ...data })
      else if (action === 'cancel') setMessage(data.next)
      else { setReview(null); setMessage(action === 'accept' ? data.next : 'The run ended. Its receipt is saved below; earlier receipts remain available.'); onRefresh() }
    } catch (e) { if (mounted.current) setError(e.message) }
    finally { if (mounted.current && action !== 'cancel') setBusy('') }
  }
  if (!workflow) return null
  return <section className="ws-guided-checks" aria-label="Requirement to receipt">
    <h3>What should this work satisfy?</h3>
    <p>Choose a specific requirement. Review its check, accept it, then run it. HQ keeps the result tied to that requirement and the code it examined.</p>
    {workflow.error ? <p role="alert">{workflow.error}</p> : null}
    {workflow.recipes?.map(recipe => <details key={recipe.id}><summary>{TITLES[recipe.id] || AI_REVIEW_TITLES[recipe.id] || recipe.id}<small>{recipe.ready ? recipe.accepted ? 'Requirement accepted' : 'Ready to review' : 'Setup needed'}</small></summary>
      <IntegrationSupport support={recipe.support} />
      {recipe.ready ? <><strong>{recipe.requirement}</strong><p>{recipe.why}</p>{recipe.database ? <p>{recipe.database}</p> : null}<div className="ws-hq-inline-actions"><button disabled={!!busy} onClick={() => act('preview', recipe.id)}>Review requirement and check</button>{recipe.accepted ? <button disabled={!!busy} onClick={() => act('run', recipe.id, recipe.acceptance_hash)}>Run accepted check</button> : null}</div></> : <><p>{recipe.setup}</p><p className="ws-hq-muted">{recipe.next}</p></>}
    </details>)}
    {review ? <article className="ws-integration-detail" aria-label="Review this requirement"><h4>Review this requirement</h4><strong>{review.plan.criterion.requirement}</strong><p>Success means: {review.plan.criterion.success}</p><p>{review.scope_note}</p><IntegrationSupport support={review.plan.support} />{review.database ? <p>{review.database}</p> : null}<p>Budget: {review.plan.timeout_seconds} seconds · {review.plan.memory_mb} MiB worker memory · one check at a time. {review.plan.pack === 'sonarqube' ? 'The registered local analysis service is accessed on its private container network.' : review.plan.local_model_port ? 'Only the registered local model endpoint is allowed; external network is denied.' : 'Network access is denied.'}</p><details><summary>Exact command, inputs and requirement revision</summary><pre>{JSON.stringify({ plan: review.plan, requirement: review.contract, previous: review.previous }, null, 2)}</pre></details><p>Accepting records your local approval of this check. It does not run it or accept the finished product.</p><button disabled={!!busy} onClick={() => act('accept', review.recipe, review.digest)}>Accept this requirement and check</button> <button disabled={!!busy} onClick={() => setReview(null)}>Close review</button></article> : null}
    {busy ? <p role="status">{busy.startsWith('run:') ? 'The check is running within its declared budget. You can continue inspecting the graph.' : 'Preparing the requirement and its check…'} {busy.startsWith('run:') ? <button onClick={() => act('cancel', busy.slice(4))}>Cancel this check</button> : null}</p> : null}
    {message ? <p role="status">{message}</p> : null}{error ? <p className="ws-integration-error" role="alert">{error}</p> : null}
    <p className="ws-hq-muted">{workflow.trust}</p>
  </section>
}
