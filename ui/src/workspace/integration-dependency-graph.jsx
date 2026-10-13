import React from 'react'

export function DependencyGraph({ graph }) {
  const marker = React.useId().replaceAll(':', '')
  const modules = graph.modules || []
  const visible = modules.slice(0, 36)
  const positions = new Map(visible.map((module, index) => [module.source, {
    x: 15 + (index % 3) * 270, y: 15 + Math.floor(index / 3) * 84,
  }]))
  const edges = visible.flatMap(module => (module.dependencies || []).map(dependency => ({
    from: module.source, to: dependency.resolved, circular: dependency.circular,
  }))).filter(edge => positions.has(edge.to))
  const shownEdges = edges.slice(0, 160)
  const folders = graph.folders || []
  return <section aria-label="Native dependency graph and metrics">
    <h5>Module graph and coupling</h5>
    <p>{graph.module_count} native module records and {graph.dependency_count} dependency records. Arrows follow the imports reported by dependency-cruiser.</p>
    <p>Configured exclusions, unresolved imports and unsupported syntax limit this graph. Coupling and instability describe these dependencies; they do not establish whether the application is correct.</p>
    <details open><summary>View native module graph</summary>
      <p>Showing {visible.length} of {modules.length} module records and {shownEdges.length} of {graph.dependency_count} dependency records. The complete recorded graph and native report formats remain in the export.</p>
      <svg role="img" aria-label="Native module dependency diagram" viewBox={`0 0 810 ${Math.max(90, Math.ceil(visible.length / 3) * 84)}`} style={{ width: '100%', maxHeight: '48rem' }}>
        <defs><marker id={marker} viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto"><path d="M 0 0 L 8 4 L 0 8 z" fill="currentColor" /></marker></defs>
        {shownEdges.map((edge, index) => {
          const from = positions.get(edge.from), to = positions.get(edge.to)
          const middle = (from.y + to.y) / 2 + 55
          return <path key={index} d={`M${from.x + 120},${from.y + 40} C${from.x + 120},${middle} ${to.x + 120},${middle} ${to.x + 120},${to.y}`} fill="none" stroke="currentColor" strokeOpacity="0.45" strokeWidth={edge.circular ? 2 : 1} markerEnd={`url(#${marker})`}><title>{`${edge.from} → ${edge.to}${edge.circular ? ' · circular dependency' : ''}`}</title></path>
        })}
        {visible.map(module => {
          const position = positions.get(module.source)
          const label = module.source.length > 34 ? `…${module.source.slice(-33)}` : module.source
          return <g key={module.source}><rect x={position.x} y={position.y} width="240" height="40" rx="4" fill="var(--ws-raised)" stroke="currentColor" /><text x={position.x + 8} y={position.y + 24} fill="currentColor" fontSize="12">{label}</text><title>{module.source}</title></g>
        })}
      </svg>
    </details>
    {graph.metrics_requested ? <details open><summary>Native folder metrics</summary>
      <p>Incoming and outgoing coupling counts come from the native folder graph. Instability is outgoing coupling divided by incoming plus outgoing coupling. It is a dependency ratio, not a severity rating.</p>
      <table><thead><tr><th>Folder</th><th>Modules</th><th>Incoming</th><th>Outgoing</th><th>Instability</th></tr></thead><tbody>{folders.slice(0, 100).map(folder => <tr key={folder.name}><td>{folder.name || '(root)'}</td><td>{folder.moduleCount === -1 ? 'unavailable (native -1)' : folder.moduleCount}</td><td>{folder.afferentCouplings ?? 'unavailable'}</td><td>{folder.efferentCouplings ?? 'unavailable'}</td><td>{folder.instability ?? 'unavailable'}</td></tr>)}</tbody></table>
      {folders.length > 100 ? <p>Showing the first 100 of {folders.length} native folders; all recorded rows remain in the export.</p> : null}
    </details> : <p>Folder metrics were not requested for this run.</p>}
    {Object.entries(graph.formats || {}).map(([name, report]) => <details key={name}><summary>Native {name} report</summary><p>The native formatter consumed the same captured graph. Original text and command evidence are preserved in the export.</p><pre>{report.text}</pre></details>)}
  </section>
}
