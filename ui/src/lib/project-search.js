/** Deterministic project-name search with the current project retained once. */
export function projectSearchResult(projects, query = '', currentFixture = null) {
  const all = Array.isArray(projects) ? projects : []
  const normalized = String(query).trim().toLowerCase()
  const matching = normalized
    ? all.filter((project) => String(project.name || '').toLowerCase().includes(normalized))
    : all
  const current = all.find((project) => project.fixture === currentFixture) || null
  const currentPinned = !!current && !matching.some((project) => project.fixture === current.fixture)
  return {
    matching,
    visible: currentPinned ? [current, ...matching] : matching,
    matchCount: matching.length,
    totalCount: all.length,
    currentPinned,
    query: normalized,
  }
}
