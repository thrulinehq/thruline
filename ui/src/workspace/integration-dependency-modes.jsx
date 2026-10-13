import React from 'react'

const native = (modes, name) => modes.reports?.[name]?.native

export function DependencyReviewModes({ graph }) {
  const modes = graph.review_modes
  if (!modes) return null
  const summary = graph.summary || {}
  const selection = modes.selection || {}
  const baseline = native(modes, 'baseline-filtered')
  const generated = native(modes, 'baseline-generation')
  const affected = native(modes, 'affected')
  const cached = native(modes, 'cache')
  return <section aria-label="Native baseline, affected scope and cache">
    <h5>Baseline, affected scope and cache</h5>
    <p>Selected native modes: {modes.complete ? 'complete' : 'incomplete'}.</p>
    <p>The primary assessment uses the uncached configured full graph. Baseline and affected views do not replace its findings. A native cache hit reuses a prior graph; it does not run fresh analysis.</p>
    <p>Full configured companion: {graph.module_count} modules, {summary.error ?? 'unrecorded'} errors, {summary.warn ?? 'unrecorded'} warnings and {summary.ignore ?? 'unrecorded'} ignored violations. Configured exclusions still limit this scope.</p>
    {(modes.errors || []).map((error, index) => <p key={index}>Native mode incomplete: {error}</p>)}
    <dl>
      <dt>Source root</dt><dd>{modes.source_identity?.root || 'not established'}</dd>
      <dt>Source HEAD</dt><dd>{modes.source_identity?.head || 'not established'}</dd>
      <dt>Source and configuration identity</dt><dd>{modes.source_identity_sha256 || 'not established'}</dd>
      <dt>Prepared runtime identity</dt><dd>{modes.source_identity?.runtime_sha256 || 'not established'}</dd>
      <dt>Known baseline</dt><dd>{selection.baseline || 'not selected'}</dd>
      <dt>Affected base commit</dt><dd>{selection.affected_base || 'not selected'}</dd>
    </dl>
    {baseline ? <p>Native baseline view: {baseline.summary?.error} errors, {baseline.summary?.warn} warnings and {baseline.summary?.ignore} known violations ignored. Ignored findings remain in the native records and the full companion.</p> : null}
    {Array.isArray(generated) ? <details><summary>Native baseline proposal · {generated.length} violations</summary>
      <p>The native baseline reporter consumed the exact uncached full graph. This is a proposal; the source baseline has not been changed. The original native proposal bytes remain in the export.</p>
      <pre>{JSON.stringify(generated, null, 2).slice(0, 32768)}</pre>
      {JSON.stringify(generated).length > 32768 ? <p>This preview is limited to 32768 characters; the export retains the complete proposal.</p> : null}
    </details> : null}
    {affected ? <p>Native affected view: {affected.modules?.length} modules and {affected.summary?.violations?.length} visible violations. It contains changed modules and their transitive dependents; excluded destinations can hide violations that remain in the full companion.</p> : null}
    {modes.cache ? <>
      <p>Native cache execution: {modes.cache.execution_kind === 'native_cached_result_reused' ? 'reused cached graph; no fresh analysis' : 'performed native analysis'}. Strategy: {modes.cache.strategy}. Uncached companion matches: {String(modes.cache.matches_uncached_full_companion)}.</p>
      <p>Cache identity includes the source root, source and configuration bytes, selected baseline and prepared runtime. Both outputs come from the same engine and are not independent confirmation.</p>
      <dl><dt>Cache before SHA256</dt><dd>{modes.cache.before?.sha256 || 'absent'}</dd><dt>Cache after SHA256</dt><dd>{modes.cache.after?.sha256 || 'not captured'}</dd><dt>Cached native modules</dt><dd>{cached?.modules?.length ?? 'not captured'}</dd></dl>
    </> : null}
    <details><summary>Actual native mode commands · {modes.processes?.length || 0}</summary>
      <p>Original stdout, stderr, progress events, inline baseline/cache bytes and their hashes remain in the exported native receipt.</p>
      {(modes.processes || []).map(process => <details key={process.id}><summary>{process.id} · exit {process.exit_code ?? 'unavailable'}</summary><pre>{JSON.stringify(process.argv)}</pre><pre>{process.stderr?.slice(0, 32768)}</pre></details>)}
    </details>
  </section>
}
