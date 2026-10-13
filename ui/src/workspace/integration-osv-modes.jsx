import React from 'react'

const count = inventory => (inventory || []).reduce((n, item) => n + (item.vulnerability_count || 0), 0)
export function OsvModeFacts({ value }) {
  const raw = count(value.unfiltered_inventory)
  const selected = count(value.selected_inventory)
  return <section aria-label="Native OSV mode and policy">
    <h5>Selected dependency analysis</h5>
    <p>Native mode: {value.mode}. Recognized package records: {value.unfiltered_inventory?.length ?? 'unavailable'}.</p>
    <p>Before policy: {raw} advisory records. Under selected policy: {selected}. Suppressed advisories remain in the unfiltered report; suppression does not repair a dependency.</p>
    <p>Coverage is limited to the selected files and prepared package versions. Static symbol analysis helps prioritize known advisories; it does not establish runtime exploitability.</p>
    {value.mode === 'go' ? <section aria-label="Native Go symbol analysis">
      <p>Native Go symbol analysis uses the pinned toolchain. Cgo is disabled and module downloads are disabled. The selected go.mod language version and the observed compiler version are separate inputs.</p>
      <p>Known uncalled symbols retain their package advisory. Missing symbol metadata or a compiler failure means unknown call analysis.</p>
      <details><summary>Go compiler identity and analysis scope</summary><pre>{JSON.stringify(value.analysis_configuration, null, 2)}</pre></details>
    </section> : null}
    {value.uncovered_packages?.length ? <p role="alert">Some package versions have no prepared advisory coverage. This analysis is incomplete.</p> : null}
    {value.errors?.map((error, index) => <p role="alert" key={index}>{error}</p>)}
    {value.selected_inventory?.map((entry, index) => <article key={index}>
      <p><strong>{entry.package.name} {entry.package.version}</strong> · {entry.package.ecosystem} · {entry.source?.path}</p>
      {entry.licenses ? <p>Native declared licenses: {entry.licenses.join(', ')}. Outside selected allowlist: {entry.license_violations?.join(', ') || 'none reported'}.</p> : null}
      {entry.package.deprecated === true ? <p>Deprecated in the prepared deps.dev metadata.</p> : null}
      {entry.package.image_origin_details ? <p>Native image layer index: {entry.package.image_origin_details.index}.</p> : null}
      {entry.groups?.map((group, groupIndex) => <div key={groupIndex}>
        {value.mode === 'go' ? (group.experimental_analysis && Object.keys(group.experimental_analysis).length ? Object.entries(group.experimental_analysis).map(([id, analysis]) => <p key={id}>{id}: {analysis.called === true ? 'Native known called symbol' : analysis.called === false ? 'Native known uncalled symbol; advisory retained' : 'Unknown native call analysis'}.</p>) : <p>Unknown native call analysis for {group.ids.join(', ')}.</p>) : null}
        <details><summary>Advisory aliases and native symbol analysis · {group.ids.join(', ')}</summary><pre>{JSON.stringify(group, null, 2)}</pre></details>
      </div>)}
    </article>)}
    <p>License and deprecation facts are dated upstream registry metadata. Missing metadata remains incomplete. SPDX exports contain inventory; native JSON supplies advisory findings.</p>
    <details><summary>Unfiltered dependency inventory</summary><pre>{JSON.stringify(value.unfiltered_inventory, null, 2)}</pre></details>
    <details><summary>Exact policy and prepared dataset scope</summary><pre>{JSON.stringify({ configuration: value.configuration, policy: value.policy, dataset_scope: value.dataset_scope, dataset_files: value.dataset_files }, null, 2)}</pre></details>
    {value.native_image_metadata ? <details><summary>Native image layers and commands</summary><pre>{JSON.stringify(value.native_image_metadata, null, 2)}</pre></details> : null}
    <details><summary>Native stages and retained output identities</summary><pre>{JSON.stringify(value.commands, null, 2)}</pre></details>
  </section>
}
