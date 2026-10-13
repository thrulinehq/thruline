import React from 'react'

export function RewriteModules({ availability, properties }) {
  if (!availability) return null
  return <section aria-label="OpenRewrite module and license scope">
    <h5>Selected OpenRewrite module</h5>
    <p>Selected module: {availability.selected_module || 'not supplied'}. Native runtime: {availability.runtime_version}. Acquired source: {availability.source_version} at {availability.source_pin}.</p>
    <p>Each module has its own license. Installed jars indicate availability only; this result covers the selected parser and recipe. Acquired MSAL modules are not enabled by this adapter.</p>
    {properties ? <section aria-label="Native Properties migration">
      <p>Native Properties recipe: {properties.recipe}. Exact key: {properties.options?.property_key}; accepted old value: {properties.options?.old_value ?? 'any value'}; target value: {properties.options?.new_value}.</p>
      <p>Selected Properties files: {properties.selected?.join(', ') || 'none'}. Discovered Properties files: {properties.discovered?.join(', ') || 'none'}.</p>
      <p>Native key matches: {properties.matching_key_count ?? 'unavailable'}; values outside the accepted migration: {properties.unexpected_value_count ?? 'unavailable'}; changes at native fixed point: {properties.fixed_point_changes ?? 'unavailable'}.</p>
      <p>Properties parsing does not perform Java compilation or type attribution. The proposed patch and fixed point share the same native engine. Independent protected consumer checks remain required before apply.</p>
      {properties.parsed_entries?.map((entry, index) => <p key={index}>{entry.path} · {entry.key} = {entry.value}</p>)}
    </section> : null}
    <table><thead><tr><th>Acquired module</th><th>Own license</th><th>Runtime</th><th>Adapter selection</th></tr></thead>
      <tbody>{availability.modules?.map(module => <tr key={module.module}>
        <th scope="row">{module.module}</th><td>{availability.licenses?.[module.license_sha256]?.name || 'unavailable'}</td>
        <td>{module.runtime_present ? `Present: ${module.runtime_jar?.name}` : 'Not present'}</td>
        <td>{module.restricted ? 'Not enabled (MSAL)' : module.selected ? `Selected: ${module.adapter || 'no implemented recipe'}` : module.adapter ? `Unselected: ${module.adapter}` : 'No implemented recipe'}</td>
      </tr>)}</tbody>
    </table>
    <details><summary>Acquired module license texts and hashes</summary>{Object.entries(availability.licenses || {}).map(([hash, license]) => <article key={hash}><h6>{license.name}</h6><p>{license.source_path} · SHA256 {hash}</p><pre>{license.text}</pre></article>)}</details>
    <p>{availability.selected_scope}</p>
  </section>
}
