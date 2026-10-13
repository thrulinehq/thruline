import React from 'react'

export function SchemathesisExtensions({ value }) {
  const selected = value.selection || {}
  const registration = value.registration?.registered || {}
  const requested = selected.required_extensions || {}
  const missing = value.missing || []
  return <section aria-label="Native GraphQL and API extensions">
    <h5>Selected API contract and extensions</h5>
    <p>Specification: {selected.specification || 'unavailable'}. Native Schemathesis: 4.26.1.</p>
    <p>Owned service: {selected.service_file}. Accepted schema: {selected.schema_file}. Extension module: {selected.extension_file}.</p>
    <p>Parsed operations: {value.parsed_operations?.join(', ') || 'none'}. Expected: {selected.operations?.join(', ') || 'none'}.</p>
    <p>Selected phase: fuzzing; positive inputs; seed {selected.seed}; at most {selected.max_examples} configured examples per operation. Other phases are outside this selection. Registering a scalar strategy does not establish exhaustive value coverage.</p>
    <p>Native process: {value.process?.state || 'unavailable'}; exit {value.process?.exit_code ?? 'unavailable'}.</p>
    <table><thead><tr><th>Extension family</th><th>Requested</th><th>Native registrations</th></tr></thead><tbody>{['scalars', 'checks', 'hooks', 'serializers'].map(kind => <tr key={kind}><th>{kind}</th><td>{requested[kind]?.join(', ') || 'none'}</td><td>{registration[kind]?.join(', ') || 'none'}</td></tr>)}</tbody></table>
    {missing.length ? <p role="alert">Required native extensions are missing: {missing.map(item => `${item.category}: ${item.name}`).join('; ')}. This check is incomplete.</p> : null}
    {value.capture_complete !== true ? <p role="alert">Native artifact capture is incomplete: {value.capture_gaps?.join('; ') || 'original report unavailable'}.</p> : null}
    <p>The native HTTP report preserves actual request bodies and responses, including selected serialization. Custom checks cover their named assertions. The separately captured independent behavior checks remain distinct evidence.</p>
    <details><summary>Accepted extension events and native phases</summary><pre>{JSON.stringify({ phases: value.native_phases, events: value.extension_events }, null, 2)}</pre></details>
    <details><summary>Exact selected native command and source identities</summary><pre>{JSON.stringify({ argv: value.native_argv, source: value.source_files, scope: value.scope }, null, 2)}</pre></details>
  </section>
}
