import React from 'react'

export function MegaLinterScope({ facts }) {
  const selection = facts.selection || {}
  const coverage = facts.coverage || {}
  const requests = facts.applicability || []
  const commands = (facts.command_results || []).filter(row => row.kind !== 'native.command.started')
  const unfinished = requests.filter(row => !['completed_exit_zero', 'completed_diagnostics'].includes(row.status))
  return <section aria-label="MegaLinter selected scope">
    <h5>Selected analyzers, commands and exclusions</h5>
    <p>Flavor: {selection.flavor}. {requests.length} requested analyzers; {unfinished.length} did not complete their selected scope.</p>
    <p>Native exit zero can still carry warnings. Missing applicability, timeouts and failed commands are not clean checks.</p>
    <table aria-label="MegaLinter requested analyzer inventory"><thead><tr><th>Analyzer</th><th>Native outcome</th><th>Files</th><th>Exit</th><th>Seconds</th></tr></thead><tbody>{requests.map(row => <tr key={row.id}><td>{row.id}</td><td>{row.status?.replaceAll('_', ' ')}</td><td>{row.native_final?.files_number ?? 'unavailable'}</td><td>{row.native_final?.return_code ?? 'unavailable'}</td><td>{typeof row.native_final?.elapsed_time_s === 'number' ? row.native_final.elapsed_time_s.toFixed(3) : 'unavailable'}</td></tr>)}</tbody></table>
    <p>Parallel workers requested: {selection.parallel?.enabled ? selection.parallel.workers : 'disabled'}. Observed worker processes: {facts.parallel_observations?.worker_pids?.length ?? 0}. Observed overlapping analyzer pairs: {facts.parallel_observations?.overlapping_linter_pairs?.length ?? 0}.</p>
    <p>Native linter timeout: {selection.timeouts?.linter_seconds} seconds. Overall deadline: {selection.timeouts?.overall_seconds} seconds.</p>
    <details><summary>Selected files and excluded scope</summary><pre>{JSON.stringify({ selected: coverage.selected_files_by_linter, excluded: coverage.excluded_applicable_files, ignored: coverage.ignored_source, omissions: coverage.omissions, filters: coverage.filters }, null, 2)}</pre></details>
    <details><summary>Native descriptor and flavor applicability</summary><p>{facts.native_inventory?.length ?? 0} native descriptor records retained. This inventory does not qualify every language or install absent analyzers.</p><pre>{JSON.stringify(requests.map(row => ({ id: row.id, factory: row.native_factory, final: row.native_final })), null, 2)}</pre></details>
    <section aria-label="MegaLinter native command results"><h6>Actual pre/post commands</h6>{commands.length ? commands.map((row, index) => <article key={index}><p>{row.phase} · {row.kind === 'native.command.failed' ? 'failed' : `native exit ${row.result?.status}`}</p><pre>{row.result?.command_info?.command || row.message}</pre>{row.result?.stdout ? <pre>{row.result.stdout}</pre> : null}</article>) : <p>No native pre/post command result was captured.</p>}</section>
    <p>Custom code runs in a disposable Linux namespace with no network, host credentials or writable host mounts. Supplied values can remain visible in configuration and to processes inside that namespace; environment masking only replaces matched child variable entries.</p>
    <p>Original project unchanged: {facts.native_fix?.original_unchanged === true ? 'confirmed by source hashes' : 'not established'}. Captured workspace unchanged: {coverage.captured_workspace_unchanged === true ? 'confirmed' : 'not established'}.</p>
    <details><summary>Native namespace and captured artifact gaps</summary><pre>{JSON.stringify({ image: facts.namespace?.Image, state: facts.namespace?.State, mounts: facts.namespace?.Mounts, closure: facts.artifact_closure }, null, 2)}</pre></details>
  </section>
}
