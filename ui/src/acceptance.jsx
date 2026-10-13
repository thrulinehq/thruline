import React, { useEffect, useState } from 'react'
import './acceptance.css'

const labels = { verified: 'Proven', failed: 'Not proven', unverified: "Recorded" }
const quote = (value) => `'${String(value).replaceAll("'", "'\\''")}'`

function Command({ children }) {
  const [copied, setCopied] = useState(false)
  return <div className="accept-command"><code>{children}</code><button onClick={async () => {
    try { await navigator.clipboard.writeText(children); setCopied(true) } catch { setCopied(false) }
  }}>{copied ? 'Copied' : 'Copy'}</button></div>
}

export default function Acceptance() {
  const [reading, setReading] = useState(null)
  const [error, setError] = useState(null)
  const [busy, setBusy] = useState(false)
  const [readAt, setReadAt] = useState(null)
  async function refresh() {
    setBusy(true); setReading(null); setError(null)
    const abort = new AbortController()
    const timeout = setTimeout(() => abort.abort(), 32000)
    try {
      const response = await fetch('/api/acceptance', { cache: 'no-store', signal: abort.signal })
      if (!response.ok) throw new Error(`The local reading failed (HTTP ${response.status}). Run acceptance status in your terminal to inspect the cause.`)
      const value = await response.json()
      if (value.schema_version !== 1 || value.shape !== 'thruline.acceptance.status' || !Object.hasOwn(labels, value.state)) throw new Error('The local result has an unsupported format.')
      if (value.accepted && (!value.evaluation || !Array.isArray(value.evaluation.checks) || !Array.isArray(value.contract?.checks))) throw new Error('The local result is incomplete.')
      setReading(value); setReadAt(new Date().toLocaleTimeString())
    } catch (cause) { setError(cause.name === 'AbortError' ? 'The reading exceeded 32 seconds. Inspect acceptance status before trying again.' : cause.message) }
    finally { clearTimeout(timeout); setBusy(false) }
  }
  useEffect(() => { refresh() }, [])
  const location = reading?.location
  const where = location ? ` --repo ${quote(location.repo)} --exec-db ${quote(location.exec_db)}` : ''
  const checks = reading?.evaluation?.checks || []
  const criteria = reading?.evaluation?.criteria || []
  const proven = checks.filter((check) => check.state === 'verified').length
  const title = busy ? 'Reading your accepted requirements…' : error ? "ThruLine could not read your requirements." : !reading?.accepted ? 'No requirements have been accepted yet.' : reading.state === 'verified' ? 'Your declared checks are proven for this local source.' : reading.state === 'failed' ? 'A required check did not pass.' : 'Your requirements still have open work.'
  return <main className="accept-page">
    <header className="accept-top"><a href="/acceptance" aria-label="ThruLine acceptance home">ThruLine <span>/ Accepted work</span></a><button disabled={busy} onClick={refresh}>{busy ? 'Reading…' : 'Refresh evidence'}</button></header>
    <section className="accept-hero" aria-live="polite">
      <p className="accept-eyebrow">LOCAL ADVISORY · {busy ? 'READING' : labels[error ? 'unverified' : reading?.state] || "Recorded"}</p>
      <h1>{title}</h1>
      {error ? <p role="alert">{error} No previous result is being shown.</p> : reading?.accepted ? <><p>{reading.contract.objective}</p><div className="accept-counts"><strong>{proven} of {checks.length} checks proven</strong><span>{criteria.length} accepted {criteria.length === 1 ? 'criterion' : 'criteria'}</span><span>Read at {readAt}</span></div></> : !busy && <p>Review a concrete set of checks before asking ThruLine whether the work is complete.</p>}
    </section>
    {reading && <>
      <section className="accept-next"><h2>Next step</h2><p>{reading.next_action}</p>{!reading.accepted && <Command>{`thruline acceptance init${where}`}</Command>}</section>
      {reading.pending_draft_change && <aside className="accept-warning">The draft changed or was removed. Previously accepted requirements still apply; review a replacement explicitly.</aside>}
      {reading.accepted && !reading.protected_inputs_verified && <aside className="accept-warning">Protected inputs could not be confirmed. {reading.protected_input_error || `Changed: ${(reading.protected_changes || []).join(', ')}`}</aside>}
      {reading.accepted && <>
        <section className="accept-section"><h2>Agreed requirements</h2>{criteria.map((criterion) => {
          const definition = reading.contract.criteria.find((item) => item.id === criterion.criterion_id)
          return <article className="accept-criterion" key={criterion.criterion_id}><div><h3>{definition?.description || criterion.criterion_id}</h3><p>{definition?.human_acceptance_required ? 'Human acceptance remains required.' : `${definition?.check_ids.length || 0} required checks`}</p></div><span className={`accept-state ${criterion.state}`}>{labels[criterion.state]}</span></article>
        })}</section>
        <section className="accept-section"><h2>Execution evidence</h2>{checks.map((check) => {
          const definition = reading.contract.checks.find((item) => item.id === check.check_id)
          const run = reading.runs.find((item) => item.run_id === check.run_id)
          return <article className="accept-check" key={check.check_id}>
            <div className="accept-row"><h3>{check.check_id}</h3><span className={`accept-state ${check.state}`}>{labels[check.state]}</span></div>
            <p>{check.reason}</p>
            {run?.counts && <p className="accept-tally">{run.counts.passed} passed · {run.counts.failed} failed · {run.counts.skipped} skipped · {run.counts.xfail} expected failures · {run.counts.total} total</p>}
            <p>{run ? run.current ? 'Applies to the inputs observed at this reading.' : 'Historical run; current applicability is unverified.' : 'No applicable captured run.'}</p>
            <details><summary>Scope, evidence and how to check</summary><dl><dt>Command</dt><dd><code>{definition.argv.map(quote).join(' ')}</code></dd><dt>Working directory</dt><dd>{definition.cwd}</dd><dt>Time limit</dt><dd>{definition.timeout_seconds} seconds · one explicit attempt</dd><dt>Evidence trust</dt><dd>{run?.trust || 'No producing evidence'}</dd>{run && <><dt>Environment boundary</dt><dd>{run.environment_boundary}</dd><dt>Runner identity</dt><dd>{run.runner_version}</dd><dt>Run</dt><dd><code>{run.run_id}</code></dd><dt>Input tree</dt><dd><code>{run.input_tree}</code></dd></>}</dl>
              {run?.reasons?.map((reason, index) => <p key={index}>{reason}</p>)}
              <Command>{`thruline acceptance run${where} --check ${quote(check.check_id)} --digest ${quote(reading.contract_digest)}`}</Command><p>Copying does not run a command. Review the scope before running it in your terminal.</p>
            </details>
          </article>
        })}</section>
        <section className="accept-section"><h2>Carry this work forward</h2><p>A continuity packet keeps the accepted work visible. Exporting it does not confirm that another agent resumed it.</p><Command>{`thruline acceptance handoff${where} --json`}</Command><details><summary>Create a portable signed summary</summary><p>The receipt carries check scope, counts, evidence digests and limits. A valid signature alone does not establish independent trust.</p><Command>{`thruline acceptance receipt${where} --json`}</Command></details></section>
        <footer className="accept-footer"><p>This is a local advisory reading. External services and installed dependencies are not a closed environment. Refresh after changes; this page does not watch your files.</p><details><summary>Accepted identity</summary><p>Revision {reading.contract.revision} · accepted {reading.accepted_at}</p><code>{reading.contract_digest}</code><p>Current source tree: <code>{reading.evaluation.subject}</code></p></details></footer>
      </>}
    </>}
  </main>
}
