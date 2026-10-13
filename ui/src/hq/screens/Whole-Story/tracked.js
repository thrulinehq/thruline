/* A tracked Change's documents and derived model — the same reads
 * use-change.js makes (`readChangeDocuments` / `deriveChangeModel`), never
 * re-derived. Its own module so the screen's test can stand in for it. */
export async function readTracked(id) {
  const [{ readChangeDocuments }, { deriveChangeModel }] = await Promise.all([
    import('../../../workspace/change-workspace.jsx'),
    import('../../../workspace/change-model.js'),
  ])
  const docs = await readChangeDocuments(id, { runs: true })
  if (!docs || docs.status !== 'ready') return null
  const hq = deriveChangeModel(docs.change, docs.coverage, docs.applicability, docs.proposals, docs.work)
  return { change: docs.change, hq, proposals: docs.proposals }
}
