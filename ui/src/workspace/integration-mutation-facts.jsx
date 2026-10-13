import React from 'react'

export function MutationFacts({ facts }) {
  const selection = facts.mutation_scope?.selection
  const outcomes = facts.mutation_outcomes || []
  const mutants = outcomes.filter(value => value.scenario?.Mutant)
  return <section aria-label="Native mutation scope and outcomes">
    <h5>Native Rust mutation outcomes</h5>
    <p>Original baseline: {facts.mutation_baseline?.[0]?.summary || 'missing'}. Each shard requires its own passing build and test baseline.</p>
    {selection ? <>
      <p>Mutation packages: {selection.workspace ? 'entire workspace' : selection.packages?.join(', ') || 'native default members'}. Tests for each mutant: {selection.test_workspace ? 'entire workspace' : selection.test_packages?.join(', ') || 'each mutated package'}.</p>
      <p>Features: {selection.all_features ? 'all features' : selection.features?.join(', ') || 'none explicitly enabled'}. Default features: {selection.no_default_features ? 'disabled' : 'enabled'}.</p>
      <p>Runner: {selection.test_runner}. Parallel mutation jobs: {selection.jobs}. Shard: {selection.shard ? `${selection.shard.index}/${selection.shard.count} (${selection.shard.algorithm})` : 'whole selected set'}. Diff: {selection.in_diff || 'no diff filter'}.</p>
      <p>Selected files: {selection.files?.join(', ') || 'native defaults'}. Excluded files: {selection.exclude_files?.join(', ') || 'none explicitly excluded'}. Name filters: {selection.examine_re?.join(', ') || 'none'}. Excluded names: {selection.exclude_re?.join(', ') || 'none'}.</p>
    </> : <p>Native default selection; one mutation job and Cargo tests.</p>}
    <p>Only the accepted package, feature, diff, file and shard selection is covered. Macro expansions are not mutated. Surviving mutations may be equivalent; unviable and timed-out mutations are not caught tests. A passing shard does not establish all-shard coverage. Nextest excludes doctests.</p>
    {mutants.map((outcome, index) => <details key={`${outcome.scenario.Mutant.name}-${index}`}><summary>{outcome.summary} · {outcome.scenario.Mutant.name}</summary><p>Package: {outcome.scenario.Mutant.package}. Native diff: {outcome.diff_path}. Native log: {outcome.log_path}.</p><pre>{JSON.stringify(outcome.phase_results, null, 2)}</pre></details>)}
    {facts.mutation_scope ? <details><summary>Accepted native mutation command</summary><pre>{JSON.stringify(facts.mutation_scope.accepted_native_argv, null, 2)}</pre></details> : null}
  </section>
}
