import React from 'react'

export function RuleEngineErrors({ errors }) {
  if (!Array.isArray(errors) || errors.length === 0) return null
  return <section aria-label="Native rule engine errors"><h5>Native scanner errors</h5><p>The affected rule or source scope is incomplete.</p>{errors.map((error, index) => <p key={index}>{typeof error?.type === 'string' ? error.type : 'Native error'}: {typeof error?.long_msg === 'string' ? error.long_msg : typeof error?.message === 'string' ? error.message : typeof error?.short_msg === 'string' ? error.short_msg : JSON.stringify(error)}</p>)}</section>
}

export function OpengrepScope({ scope }) {
  return <section aria-label="Opengrep native flow and grammar scope">
    <h5>Opengrep native flow and grammar scope</h5>
    <p>{scope.engine} {scope.version} · {scope.mode}. Scanned extensions: {scope.scanned_file_extensions?.join(', ') || 'none'}.</p>
    <p>{scope.findings?.length || 0} native findings. These are observations from one engine.</p>
    {scope.findings?.map((finding, index) => <article key={`${finding.native_pointer}-${index}`}>
      <p><strong>{finding.rule}</strong> · {finding.path}:{finding.start?.line}:{finding.start?.col}–{finding.end?.line}:{finding.end?.col}</p>
      <p>{finding.message}</p>
      <details><summary>Native metavariables and fingerprint</summary><pre>{JSON.stringify({ metavariables: finding.metavariables, fingerprint: finding.fingerprint, native_pointer: finding.native_pointer }, null, 2)}</pre></details>
      {finding.trace ? <details open><summary>Native source, intermediate values and sink</summary><pre>{JSON.stringify(finding.trace, null, 2)}</pre></details> : <p>No taint trace was emitted for this finding.</p>}
      {typeof finding.fix === 'string' ? <details open><summary>Native rule-defined replacement</summary><pre>{finding.fix}</pre><p>This suggestion requires a current staged preview and named independent checks before application.</p></details> : null}
    </article>)}
    {scope.limitations?.map((limitation, index) => <p key={index}>{limitation}</p>)}
    {scope.skipped_rules?.length || scope.skipped_paths?.length ? <p role="alert">The native scanner skipped selected work. Inspect the retained report before continuing.</p> : null}
  </section>
}
