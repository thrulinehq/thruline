import React from 'react'

// A historical receipt supplies its own declaration; current labels must not rewrite it.
export function IntegrationSupport({ support }) {
  if (!support) return <p className="ws-hq-muted">This receipt has no versioned support declaration. Inspect its exact command, inputs and recorded limitations.</p>
  return <details className="ws-support-scope"><summary>Supported scope and limits</summary>
    <dl><dt>Reviewed tool version</dt><dd>{support.tool_version}</dd><dt>Language and check scope</dt><dd>{support.language_scope}</dd>{support.graph ? <><dt>Graph</dt><dd>{support.graph}</dd></> : null}<dt>Execution platform</dt><dd>{support.execution_platform}</dd><dt>Qualification</dt><dd>{support.qualification}</dd></dl>
    <p>Required inputs: {support.required_inputs.join('; ')}.</p>
    {support.evidence_limits.map(limit => <p key={limit}>{limit}</p>)}
    <small>Support declaration version {support.version}. The receipt records the actual executable and input identities.</small>
  </details>
}
