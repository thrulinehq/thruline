import React from 'react'
import { AiReviewPatchFacts, aiReviewDiffIdentity } from './ai-review-facts.jsx'
import { MiniHarnessFacts } from './integration-mini-harness.jsx'
import { InferModeFacts } from './integration-infer-modes.jsx'
import { SchemathesisExtensions } from './integration-schemathesis-extensions.jsx'
import { JoernFrontends } from './integration-joern-frontends.jsx'
import { RewriteModules } from './integration-rewrite-modules.jsx'
import { OsvModeFacts } from './integration-osv-modes.jsx'
import { MutationFacts } from './integration-mutation-facts.jsx'
import { MegaLinterScope } from './integration-megalinter.jsx'
import { DependencyGraph } from './integration-dependency-graph.jsx'
import { DependencyResolutionScope } from './integration-dependency-resolution.jsx'
import { DependencyReviewModes } from './integration-dependency-modes.jsx'
import { SemgrepModeFacts } from './integration-semgrep-mode-facts.jsx'
import { OpengrepScope, RuleEngineErrors } from './integration-opengrep.jsx'

export function NativeFacts({ facts }) {
  const oracle = facts.oracle
  return <>
    {facts.mini_harness ? <MiniHarnessFacts value={facts.mini_harness} /> : null}
    {facts.infer_mode ? <InferModeFacts value={facts.infer_mode} /> : null}
    {facts.schemathesis_extensions ? <SchemathesisExtensions value={facts.schemathesis_extensions} /> : null}
    {facts.frontend_inventory ? <JoernFrontends inventory={facts.frontend_inventory} scope={facts.joern_frontend_scope} /> : null}
    {facts.module_availability ? <RewriteModules availability={facts.module_availability} properties={facts.properties_scope} /> : null}
    {facts.mutation_outcomes ? <MutationFacts facts={facts} /> : null}
    {facts.dependency_graph ? <><DependencyGraph graph={facts.dependency_graph} /><DependencyResolutionScope graph={facts.dependency_graph} /><DependencyReviewModes graph={facts.dependency_graph} /></> : null}
    {facts.osv_mode ? <OsvModeFacts value={facts.osv_mode} /> : null}
    {facts.semgrep_mode ? <SemgrepModeFacts value={facts.semgrep_mode} /> : null}
    <RuleEngineErrors errors={facts.rule_engine_errors} />
    {facts.opengrep_scope ? <OpengrepScope scope={facts.opengrep_scope} /> : null}
    {facts.joern_predefined ? <section aria-label="Native predefined Joern query"><h5>Native predefined C query</h5><p>Selected query: {facts.joern_predefined.query?.name || 'unavailable'}. Native inventory: {facts.joern_predefined.inventory_count} queries; this run executes only the selected C query.</p><p>Nonliteral constant format pointers can produce heuristic false positives. A finding does not prove an exploit. Parser problems and unknown graph syntax remain incomplete.</p>{facts.joern_predefined.query ? <dl><dt>Native author</dt><dd>{facts.joern_predefined.query.author}</dd><dt>Native title</dt><dd>{facts.joern_predefined.query.title}</dd><dt>Native language</dt><dd>{facts.joern_predefined.query.language}</dd><dt>Native tags</dt><dd>{facts.joern_predefined.query.tags?.join(', ')}</dd></dl> : null}{facts.joern_predefined.parse_problems?.length ? <p>Native parse problems: {facts.joern_predefined.parse_problems.join(', ')}</p> : null}<details><summary>Inspect executed predefined traversal</summary><pre>{facts.joern_predefined.query?.traversalAsString || 'No native query was executed.'}</pre></details></section> : null}
    {facts.advisory_only ? <section aria-label="Native agent proposal">
      <h5>Captured local agent work</h5>
      <p>Native process: {facts.process?.state || 'unknown'} · exit {facts.process?.exit_code ?? 'unavailable'}. Agent termination: {facts.agent_termination || 'not supplied'}.</p>
      <p>Context: {facts.source?.selected_files?.join(', ') || 'not identified'}. Editable: {facts.source?.editable_files?.length === 0 ? 'none (read-only)' : (facts.source?.editable_files || facts.source?.selected_files)?.join(', ') || 'not identified'}.</p>
      <AiReviewPatchFacts facts={facts} />
      <p>{facts.source?.capture_basis === 'committed_HEAD' ? `Committed change ${facts.source.base_commit} → ${facts.source.head}. Dirty and untracked work is outside this PR review.` : facts.source?.capture_basis === 'unified_diff_file' ? `Supplied unified diff ${aiReviewDiffIdentity(facts) || 'identity unavailable'}.` : `Working source at HEAD ${facts.source?.head || 'unknown'}.`}</p>
      {facts.native_output?.text ? <details open><summary>Actual native response · {facts.native_output.kind?.replaceAll('_', ' ')}</summary><pre>{facts.native_output.text}</pre>{facts.native_output.truncated ? <p>The displayed excerpt is truncated; the complete captured artifact remains in the export.</p> : null}</details> : null}
      {facts.proposal?.diff ? <details open><summary>Actual proposed source diff</summary><pre>{facts.proposal.diff}</pre></details> : <p>No textual source patch was captured.</p>}
      {oracle?.after ? <p>Frozen behavior oracle: before exit {oracle.before?.exit_code ?? 'unknown'}, after exit {oracle.after.exit_code}. Configuration {oracle.config_sha256}. This covers the named oracle cases on the isolated proposal; it does not apply or accept the whole product.</p> : <p>No independent behavior oracle was performed for this proposal. Agent completion remains advisory.</p>}
      <p>Original project unchanged: {facts.source?.original_unchanged === true ? 'confirmed by source hashes' : 'not established'}.</p>
    </section> : null}
    {facts.knip_scope ? <section aria-label="Knip dependency and export scope"><h5>Dependency and export families</h5>
      <p>Native workspace selection: {facts.knip_scope.selected_workspaces?.join(', ') || 'default selection from the accepted configuration'}. Included workspace directories: {facts.knip_scope.included_workspaces?.join(', ') || 'not supplied'}.</p>
      <p>Configuration: {facts.knip_scope.native_options ? (facts.knip_scope.native_options.configFilePath || 'native defaults') : 'not captured'}. Production scope: {facts.knip_scope.native_options?.isProduction === true ? 'yes' : facts.knip_scope.native_options?.isProduction === false ? 'no' : 'not captured'}.</p>
      {facts.knip_native_issues?.map((issue, index) => <p key={index}><strong>{issue.category}</strong> · {issue.namespace ? `${issue.namespace}.` : ''}{issue.symbols?.map(symbol => `${symbol.name}${symbol.line ? `:${symbol.line}` : ''}`).join(' ↔ ') || issue.name} · {issue.file}{issue.line ? `:${issue.line}` : ''}{issue.workspace ? ` · workspace ${issue.workspace}` : ''}</p>)}
      <p>{facts.knip_scope.compiler_boundary}</p><p>{facts.knip_scope.source_mapping_boundary}</p><p>{facts.knip_scope.catalog_boundary}</p>
      <details><summary>Original issue groups and source locations</summary><pre>{JSON.stringify(facts.knip_issues, null, 2)}</pre></details>
    </section> : null}
    {facts.pagination_complete !== undefined ? <section aria-label="Imported server analysis"><h5>Local server analysis</h5><p>Analysis {facts.analysis?.key || facts.analysis?.id || 'unavailable'} · revision {facts.requested_revision}.</p><p>Issue pagination: {facts.pagination_complete ? 'complete' : 'incomplete'}. {facts.native_issues?.length || 0} open issues; {facts.resolved_successors?.length || 0} resolved issue records retained.</p><p>A server quality gate describes its configured metrics and does not establish the application's behavior.</p></section> : null}
    {facts.graph_scope ? <section><h5>Native graph scope</h5><p>Selected sources: {facts.graph_scope.selected?.join(', ')}.</p><p>{facts.graph_scope.graph_files?.length || 0} graph file records. Paths below are native query observations; missing models remain recorded limitations.</p></section> : null}
    {facts.mode && facts.selection?.schema === 'thruline.ruff-selection.v1' ? <section aria-label="Selected Ruff mode"><h5>Selected Python check</h5><p>Native mode: {facts.mode}. These modes share the Ruff engine.</p><p>{Object.entries(facts.source_classification || {}).map(([path, kind]) => `${path}: ${kind.replaceAll('_', ' ')}`).join('; ')}</p>{facts.notebook_locations?.map((location, index) => <p key={index}>{location.filename} · cell {location.cell_index} ({location.cell_id}) · cell line {location.cell_local_location.row}, column {location.cell_local_location.column}. This is a code-cell location.</p>)}<details><summary>Native configuration and per-file settings</summary><pre>{JSON.stringify({ selection: facts.selection, policy: facts.policy_files, settings: facts.native_settings }, null, 2)}</pre></details></section> : null}
    {facts.msrv_scope ? <section aria-label="Clippy MSRV and suppression"><h5>Captured Rust lint policy</h5><p>Selected MSRV: {facts.msrv_scope.selected_msrv}; suppression: {facts.msrv_scope.suppression}.</p><p>{facts.msrv_scope.limitation}</p>{facts.msrv_scope.conflicts?.length ? <p role="alert">Cargo and Clippy MSRV declarations conflict. This check is incomplete.</p> : null}<details><summary>Source attributes and native MSRV precedence</summary><pre>{JSON.stringify(facts.msrv_scope, null, 2)}</pre></details></section> : null}
    {facts.compiled_targets ? <section><h5>Observed Rust build</h5><p>{facts.compiled_targets.length} compiled target records. Selected features: {facts.configuration?.features?.join(', ') || 'none'}; default features: {String(facts.configuration?.default_features)}.</p><p>Conditional code outside that build is outside this receipt's coverage.</p></section> : null}
    {facts.megalinter_controlled ? <MegaLinterScope facts={facts} /> : null}
    {facts.linters && !facts.megalinter_controlled ? <section><h5>Native analyzer aggregation</h5><p>{facts.linters.length} underlying analyzer results are preserved. Their findings remain attributed to their original engines.</p>{facts.advisor ? <details><summary>Advisor result and model scope</summary><pre>{JSON.stringify(facts.advisor, null, 2)}</pre></details> : null}</section> : null}
    {facts.dependency_inventory && !facts.osv_mode ? <section aria-label="Captured dependency advisories"><h5>Dependencies and advisory ranges</h5><p>{facts.dependency_inventory.length} recognized package versions in this dated database scan. Native alias groups identify related advisory records.</p><p>This Cargo check has no automatic dependency or API migration. Publisher range events identify affected and fixed intervals; an upgrade still needs a build and independent behavior checks.</p>{facts.dependency_inventory.map((entry, index) => <p key={index}>{entry.name} {entry.version} · {entry.ecosystem} · {entry.path}</p>)}{facts.advisory_ranges?.map((entry, index) => <details key={index}><summary>Publisher ranges · {entry.id} · {entry.package} {entry.version}</summary><pre>{JSON.stringify(entry, null, 2)}</pre></details>)}</section> : null}
    {facts.authored_workflows ? <section aria-label="Captured authored workflows"><h5>Actual browser assertions</h5><p>These outcomes cover the named authored tests and their recorded requests. They do not establish untested controls.</p>{facts.authored_workflows.map((workflow, index) => <article key={`${workflow.pointer}-${index}`}><p><strong>{workflow.title}</strong> · {workflow.status}. Expected native status: {workflow.expected_status}. {workflow.file}:{workflow.line}</p>{workflow.results?.map((result, resultIndex) => <div key={resultIndex}><p>Attempt {resultIndex + 1}: {result.status} · retry {result.retry} · {result.duration} ms.</p>{result.attachments?.map((attachment, attachmentIndex) => <NativeAttachment key={attachmentIndex} attachment={attachment} />)}</div>)}</article>)}</section> : null}
  </>
}

function NativeAttachment({ attachment }) {
  let value = null
  if (attachment.contentType === 'application/json' && typeof attachment.body === 'string' && attachment.body.length <= 262144) {
    try { value = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(attachment.body), character => character.charCodeAt(0)))) } catch { /* Preserve unreadable native attachment metadata below. */ }
  }
  return <details><summary>Captured test attachment · {attachment.name}</summary>{value ? <pre>{JSON.stringify(value, null, 2)}</pre> : <p>{attachment.path || 'Inline attachment is not a bounded readable JSON object. Inspect its original captured report.'}</p>}</details>
}

export function FactValue({ name, value }) {
  if (['native_artifacts', 'artifacts'].includes(name) && Array.isArray(value)) return <details><summary>{value.length} retained native artifact records</summary><ul>{value.map((entry, index) => <li key={index}>{entry.relative_path || entry.path || entry.name} · {entry.size ?? entry.bytes ?? 'unknown'} bytes · {entry.sha256 || 'hash unavailable'}</li>)}</ul></details>
  if (typeof value === 'object') return <details><summary>Inspect {name.replaceAll('_', ' ')}</summary><pre>{JSON.stringify(value, null, 2)}</pre></details>
  return String(value)
}
