import React from 'react'

export function JoernFrontends({ inventory, scope }) {
  if (!inventory) return null
  const native = scope?.native
  return <section aria-label="Joern frontend availability and native scope">
    <h5>Selected native frontend</h5>
    <p>Selected frontend: {inventory.selected_frontend || 'not supplied'}. Runtime {inventory.runtime_version}; acquired source {inventory.source_pin}.</p>
    <p>{scope?.assessment_scope}</p>
    <p>Frontend launchers and jars show installed availability. Only the selected native parser and query execute; this table does not establish all-language coverage.</p>
    <p>Selected inputs: {scope?.selected?.join(', ') || 'none'}. Accepted method: {scope?.method || 'no method query selected'}.</p>
    {native ? <>
      <p>Native graph language: {native.languages?.join(', ')}. Concrete selected methods: {native.selected_method_count}; unknown syntax nodes: {native.unknown_nodes}.</p>
      {native.selected_methods?.map((method, index) => <article key={index}><p>{method.name} · {method.file}</p><pre>{method.code}</pre></article>)}
      <p>{scope?.binary_scope ? 'The following native locations describe binary instructions, not source line numbers. Binary graph availability does not establish defect absence or source equivalence.' : 'The structural call-name query does not establish builtin binding or runtime types. Independent consumer behavior remains separate.'}</p>
      {native.selected_calls?.map((call, index) => <p key={index}>{call.name} · {call.code} · {scope?.binary_scope ? 'native instruction location' : 'source line'} {call.line ?? 'unavailable'}</p>)}
      <details><summary>Native method inventory and external records</summary>{native.method_inventory?.map((method, index) => <p key={index}>{method.name} · {method.is_external ? 'external' : 'concrete'} · {method.file}</p>)}<p>External and unresolved methods limit semantic analysis. Native inventory truncated: {String(native.method_inventory_truncated)}.</p></details>
    </> : <p>No native frontend/query report was produced for this selection.</p>}
    {scope?.parse_problems?.length ? <div role="alert"><p>Native parse problems leave this source scope incomplete.</p>{scope.parse_problems.map((message, index) => <pre key={index}>{message}</pre>)}</div> : null}
    {scope?.errors?.length ? <div role="alert"><p>This selected frontend remains incomplete.</p>{scope.errors.map((message, index) => <p key={index}>{message}</p>)}</div> : null}
    {scope?.artifact_retention?.complete === false ? <div role="alert"><p>Native artifact retention is incomplete. {scope.artifact_retention.omitted?.length || 0} child files exceed the bounded capture. Export must retain an explicit missing-artifact gap.</p></div> : null}
    <table><thead><tr><th>Frontend</th><th>Language/input</th><th>Installed runtime</th><th>Implemented selection</th></tr></thead><tbody>{inventory.frontends?.map(frontend => <tr key={frontend.frontend}><th scope="row">{frontend.frontend}</th><td>{frontend.language}</td><td>{frontend.runtime_present ? 'Launcher and pinned jar present' : 'Launcher or pinned jar unavailable'}</td><td>{frontend.implemented_selection || 'No query selection implemented here'}{frontend.selected ? ' (selected)' : ''}</td></tr>)}</tbody></table>
    <p>Core license: {inventory.core_license?.name}. Frontend dependencies retain separate terms; embedded notices are not a blanket license for every bundled component.</p>
    <details><summary>Pinned core license and bundled component notices</summary><pre>{inventory.core_license?.text}</pre><p>{inventory.bundled_component_notices?.jar} · SHA256 {inventory.bundled_component_notices?.sha256}</p>{inventory.bundled_component_notices?.entries?.map(entry => <article key={entry.entry}><p>{entry.entry} · SHA256 {entry.sha256}</p><pre>{entry.text}</pre></article>)}<p>{inventory.bundled_component_notices?.scope}</p></details>
  </section>
}
