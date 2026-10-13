import React from 'react'

export function InferModeFacts({ value }) {
  const current = value.stages?.current
  return <section aria-label="Selected Infer checker">
    <h5>Infer {value.version} · {value.mode}</h5>
    <p>Selected sources: {current?.sources?.join(', ') || 'not captured'}. Java release: {value.configuration?.java_release ?? 'not a Java capture'}.</p>
    {value.deprecated ? <p role="note">This checker is deprecated in Infer 1.3.0. Purity and impurity are experimental.</p> : null}
    {value.experimental ? <p role="note">Experimental language support. Existing Rust ULLBC analysis does not establish the source-to-IR compilation pipeline; Erlang requires its own compiler runtime.</p> : null}
    {value.mode === 'python' ? <p role="alert">Pulse documentation lists Python unsupported. This pinned binary has Python analyzers disabled; no Python analysis result is available.</p> : null}
    {value.mode === 'racerd' ? <p>Native concurrent contexts: {value.concurrency_context?.any_thread ?? 0} AnyThread; {value.concurrency_context?.no_thread ?? 0} NoThread. NoThread is not proof of race freedom.</p> : null}
    {value.costs?.length ? <><p>Costs are abstract instruction polynomials, not measured running time. Top means an unknown bound.</p><table><thead><tr><th>Procedure</th><th>Native cost</th><th>Degree</th></tr></thead><tbody>{value.costs.map((cost, index) => <tr key={index}><td>{cost.native.procedure_name || cost.native.procedure || JSON.stringify(cost.native.loc)}</td><td>{cost.native.exec_cost?.hum?.hum_polynomial || 'unavailable'}</td><td>{cost.known ? `linear ${cost.linear_degree}, logarithmic ${cost.log_degree}` : 'Top / unknown'}</td></tr>)}</tbody></table></> : null}
    {value.differential ? <p>Native cost comparison: {value.differential.introduced?.length ?? 'unknown'} introduced; {value.differential.fixed?.length ?? 'unknown'} fixed. Both reports come from the selected current and baseline source captures.</p> : null}
    {value.pure_functions?.length ? <p>Native pure-function facts: {value.pure_functions.map(x => x.procedure).join(', ')}. These are positive facts, not defects.</p> : null}
    {value.side_effect_facts?.length ? <p>Native side-effect facts: {value.side_effect_facts.map(x => x.procedure).join(', ')}. Purity required by this selection: {value.configuration?.require_pure ? 'yes' : 'no'}.</p> : null}
    {value.taint_model_matches?.length ? <ul aria-label="Captured taint model matches">{value.taint_model_matches.map((model, index) => <li key={index}>{model.class}.{model.method} · {model.role} · {model.matched ? 'captured defined procedure' : 'unmatched model: incomplete'}</li>)}</ul> : null}
    <p>Native summaries may assume caller preconditions. Absence of a manifest diagnostic does not establish valid inputs or a complete call graph. Skipped external-call summaries: {value.pulse_skipped_calls?.length ?? 'unknown'}.</p>
    {value.errors?.length ? <ul aria-label="Native Infer capture gaps">{value.errors.map((error, index) => <li key={index}>{error}</li>)}</ul> : null}
    {current?.native_stderr ? <details open><summary>Actual native compiler and capture diagnostics</summary><pre>{current.native_stderr}</pre></details> : null}
    <details><summary>Native procedure summaries and caller assumptions</summary><pre>{current?.summary_text || 'No native summary produced.'}</pre><pre>{JSON.stringify({ preconditions: value.pulse_preconditions, skipped_calls: value.pulse_skipped_calls }, null, 2)}</pre></details>
    <details><summary>Actual Infer commands and capture inventory</summary><pre>{JSON.stringify({ commands: value.commands, captured: current?.captured_files, errors: value.errors, runtime: value.native_executable, source_hashes: value.source_hashes }, null, 2)}</pre></details>
    <p>Original selected source unchanged: {value.original_unchanged ? 'confirmed by hashes' : 'not established'}. These Infer modes do not provide automatic source fixes. A reviewed correction needs independent behavior checks.</p>
  </section>
}
