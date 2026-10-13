import React from 'react'

const recorded = value => value === undefined ? 'not recorded' : typeof value === 'object' ? JSON.stringify(value) : String(value)

export function DependencyResolutionScope({ graph }) {
  const options = graph.summary?.optionsUsed || {}
  const environment = graph.summary?.environment || {}
  const edges = (graph.modules || []).flatMap(module => (module.dependencies || []).map(edge => ({ from: module.source, ...edge })))
  const mode = options.tsPreCompilationDeps
  const modes = {
    false: 'Compile-erased dependencies are omitted by the native graph mode.',
    true: 'Source dependencies are included before compilation; edges are not separately classified as compilation-only.',
    specify: 'Source dependencies are included and native preCompilationOnly values distinguish compilation-only edges.',
  }
  return <section aria-label="Native resolver and TypeScript scope">
    <h5>Resolver and TypeScript scope</h5>
    <p>Precompilation mode: {recorded(mode)}. {modes[String(mode)] || 'The producer did not record a recognized precompilation mode.'}</p>
    <p>These are native dependency classifications. They do not establish type checking, successful compilation or application behavior. An available parser does not establish support for every syntax form.</p>
    <dl>
      <dt>TypeScript configuration</dt><dd>{recorded(options.tsConfig)}</dd>
      <dt>Resolver strategy</dt><dd>{recorded(options.externalModuleResolutionStrategy)}</dd>
      <dt>Resolver options</dt><dd>{recorded(options.enhancedResolveOptions)}</dd>
      <dt>Module systems</dt><dd>{recorded(options.moduleSystems)}</dd>
      <dt>Preserve symlinks</dt><dd>{recorded(options.preserveSymlinks)}</dd>
      <dt>Configured exclusions</dt><dd>{recorded(options.exclude)}</dd>
    </dl>
    {(environment.issues || []).map((issue, index) => <p key={index} style={{ whiteSpace: 'pre-wrap' }}>{issue.severity}: {issue.name} — {issue.description}</p>)}
    {environment.transpilersFound ? <table><caption>Native parser availability</caption><thead><tr><th>Parser</th><th>Available</th><th>Recorded version</th></tr></thead><tbody>{environment.transpilersFound.map(parser => <tr key={parser.name}><td>{parser.name}</td><td>{recorded(parser.available)}</td><td>{recorded(parser.currentVersion)}</td></tr>)}</tbody></table> : <p>Native parser availability was not recorded.</p>}
    {environment.extensionsFound ? <p>Native extension availability: {environment.extensionsFound.map(extension => `${extension.extension}: ${extension.available ? 'available' : 'unavailable'}`).join(', ')}.</p> : null}
    <p>Showing {Math.min(edges.length, 100)} of {edges.length} native dependency classifications. All recorded edges and environment details remain in the export.</p>
    <table><caption>Native dependency classifications</caption><thead><tr><th>Source</th><th>Import specifier</th><th>Resolved destination</th><th>Module system</th><th>Dependency types</th><th>Compilation-only</th></tr></thead><tbody>{edges.slice(0, 100).map((edge, index) => <tr key={index}><td>{edge.from}</td><td>{recorded(edge.module)}</td><td>{edge.resolved}</td><td>{recorded(edge.moduleSystem)}</td><td>{(edge.dependencyTypes || []).join(', ') || 'not recorded'}</td><td>{edge.preCompilationOnly === undefined ? 'not classified' : String(edge.preCompilationOnly)}</td></tr>)}</tbody></table>
  </section>
}
