/* Deterministic Layer-2 work map.
 *
 * This derives presentation structure only from the ordinary surface and the
 * candidate fields the export already carries. It never uses an unproven
 * `interpretation` value, ranks no objective, creates no edge, and links a
 * receipt/path only on exact equality. */

import { surfaceProjection } from './fixture.js'
import { isExportExcerpt, recordTextPreview } from './record-text.js'
import { boundedOutsidePathPresentation, evidencePathPresentation } from './receipt.js'

export const INTERPRETATION_EMPTY = 'Unlabeled — awaiting interpretation'

const list = (value) => (Array.isArray(value) ? value : [])

function anchorIndex(data) {
  const byAnchor = new Map()
  for (const node of data.nodes || []) {
    if (!node.anchor_event_id) continue
    if (!byAnchor.has(node.anchor_event_id)) byAnchor.set(node.anchor_event_id, [])
    byAnchor.get(node.anchor_event_id).push(node)
  }
  return byAnchor
}

/** A receipt target exists only for one exact node-anchor match with a receipt. */
export function exactAnchorReceipt(eventId, index, receipts) {
  if (!eventId) return { eventId: '', nodeId: null, matchCount: 0 }
  const matches = index.get(eventId) || []
  const withReceipt = matches.filter((node) => receipts[node.id])
  return {
    eventId,
    nodeId: matches.length === 1 && withReceipt.length === 1 ? matches[0].id : null,
    matchCount: matches.length,
  }
}

function presentationAnchor(eventId, index, receipts) {
  return exactAnchorReceipt(eventId, index, receipts)
}

function evidencePathIndex(nodes, receipts) {
  const byPath = new Map()
  for (const node of nodes) {
    const receipt = receipts[node.id]
    for (const [evidenceIndex, evidence] of list(receipt?.evidence).entries()) {
      if (typeof evidence.path !== 'string' || evidence.path.length === 0) continue
      if (!byPath.has(evidence.path)) byPath.set(evidence.path, [])
      byPath.get(evidence.path).push({ nodeId: node.id, evidenceIndex, evidence })
    }
  }
  return byPath
}

const normalizedCwd = (value) => {
  const normalized = String(value || '').replace(/\\/g, '/')
  if (normalized === '/') return '/'
  return normalized.replace(/\/+$/, '')
}

/**
 * Present a candidate file relatively only when exact receipt evidence for
 * that same full path proves one unambiguous containing working directory.
 */
export function candidateFilePresentation(file, exactEvidence = []) {
  const raw = typeof file === 'string' ? file : ''
  const fallback = boundedOutsidePathPresentation(raw, 'Workspace relation unproven') || {
    display: 'No file path exported', label: 'Workspace relation unproven', outside: true,
  }
  if (!raw || (!raw.startsWith('/') && !/^[A-Za-z]:[\\/]/.test(raw))) {
    return { display: fallback.display, label: fallback.label, outside: true, relation: 'unproven' }
  }

  const containing = new Map()
  for (const evidence of exactEvidence) {
    if (evidence?.path !== raw || !evidence.scope?.cwd) continue
    const presentation = evidencePathPresentation(raw, evidence.scope.cwd)
    if (!presentation || presentation.outside) continue
    containing.set(normalizedCwd(evidence.scope.cwd), presentation)
  }
  if (containing.size === 1) {
    const presentation = [...containing.values()][0]
    return { display: presentation.display, label: 'Project-relative path', outside: false, relation: 'exact evidence path + cwd' }
  }
  const ambiguous = containing.size > 1
  return {
    display: fallback.display,
    label: ambiguous ? 'Workspace relation ambiguous' : fallback.label,
    outside: true,
    relation: ambiguous ? 'ambiguous exact containing cwd' : 'unproven',
  }
}

function exactPathMatches(file, clusters, evidenceByPath) {
  if (typeof file !== 'string' || file.length === 0) return { clusterMatches: [], evidenceMatches: [] }
  const clusterMatches = clusters
    .filter((cluster) => cluster.file === file)
    .map((cluster) => ({ key: cluster.key, sessionId: cluster.sessionId }))
  const evidenceMatches = list(evidenceByPath.get(file)).map(({ nodeId, evidenceIndex }) => ({ nodeId, evidenceIndex }))
  return { clusterMatches, evidenceMatches }
}

function sessionGroup(session, listed = true) {
  return {
    session,
    listed,
    openingAsks: [],
    clusters: [],
    boundaries: [],
    tails: [],
  }
}

/** Build project → session structures → project artifacts in export order. */
export function workMap(raw) {
  if (!raw || typeof raw !== 'object') {
    return {
      status: 'unavailable', project: null, eventCount: 0, sessions: [], artifacts: { exists: [], unresolved: [], other: [] },
      counts: { openingAsks: 0, clusters: 0, boundaries: 0, tails: 0, artifacts: 0 },
    }
  }
  const data = surfaceProjection(raw)
  const candidates = data.candidates
  if (!candidates || typeof candidates !== 'object') {
    return {
      status: 'unavailable', project: data.project || null, eventCount: (data.nodes || []).length, sessions: [], artifacts: { exists: [], unresolved: [], other: [] },
      counts: { openingAsks: 0, clusters: 0, boundaries: 0, tails: 0, artifacts: 0 },
    }
  }

  const receipts = data.receipts || {}
  const anchors = anchorIndex(data)
  const evidenceByPath = evidencePathIndex(data.nodes || [], receipts)
  const groups = list(data.sessions).map((session) => sessionGroup(session))
  const bySession = new Map(groups.map((group) => [group.session.id, group]))
  const groupFor = (sessionId) => {
    if (bySession.has(sessionId)) return bySession.get(sessionId)
    const group = sessionGroup({ id: sessionId, label: 'Run not listed in the session index', date: '', msgs: null }, false)
    bySession.set(sessionId, group)
    groups.push(group)
    return group
  }

  const asks = list(candidates.opening_asks)
  for (const [index, ask] of asks.entries()) {
    const preview = recordTextPreview(ask.text || '', 560)
    groupFor(ask.session_id).openingAsks.push({
      key: `ask:${index}`,
      sessionId: ask.session_id,
      text: preview,
      excerpt: isExportExcerpt(ask.text),
      continuation: ask.continuation === true,
      anchor: presentationAnchor(ask.event_id, anchors, receipts),
    })
  }

  const clusterList = list(candidates.clusters)
  const clusters = clusterList.map((cluster, index) => ({
    key: `cluster:${index}`,
    sessionId: cluster.session_id,
    file: cluster.file,
    filePresentation: candidateFilePresentation(cluster.file, list(evidenceByPath.get(cluster.file)).map((match) => match.evidence)),
    edits: cluster.edits,
    first: presentationAnchor(cluster.first_event_id, anchors, receipts),
    last: presentationAnchor(cluster.last_event_id, anchors, receipts),
    interpretationLabel: INTERPRETATION_EMPTY,
    artifactMatches: [],
  }))
  for (const cluster of clusters) groupFor(cluster.sessionId).clusters.push(cluster)

  const boundaries = list(candidates.boundaries)
  for (const [index, boundary] of boundaries.entries()) {
    groupFor(boundary.session_id).boundaries.push({
      key: `boundary:${index}`,
      sessionId: boundary.session_id,
      message: recordTextPreview(boundary.message || '', 420),
      anchor: presentationAnchor(boundary.event_id, anchors, receipts),
    })
  }

  const tails = list(candidates.tails)
  for (const [index, tail] of tails.entries()) {
    const explicitEvent = tail.event_id || tail.last_event_id || tail.first_event_id || ''
    groupFor(tail.session_id).tails.push({
      key: `tail:${index}`,
      sessionId: tail.session_id,
      edits: tail.edits_after_last_boundary,
      files: list(tail.files),
      filePresentations: list(tail.files).map((file) => candidateFilePresentation(file, list(evidenceByPath.get(file)).map((match) => match.evidence))),
      anchor: presentationAnchor(explicitEvent, anchors, receipts),
      interpretationLabel: INTERPRETATION_EMPTY,
    })
  }

  const artifactGroups = { exists: [], unresolved: [], other: [] }
  const artifacts = list(candidates.artifacts).map((artifact, index) => {
    const group = artifact.fate === 'exists' ? 'exists' : artifact.fate === 'unresolved' ? 'unresolved' : 'other'
    const item = {
      key: `artifact:${index}`,
      file: artifact.file,
      filePresentation: candidateFilePresentation(artifact.file, list(evidenceByPath.get(artifact.file)).map((match) => match.evidence)),
      edits: artifact.edits,
      sessions: list(artifact.sessions),
      fate: artifact.fate,
      group,
      exactPath: exactPathMatches(artifact.file, clusters, evidenceByPath),
    }
    artifactGroups[group].push(item)
    return item
  })
  for (const cluster of clusters) {
    cluster.artifactMatches = artifacts.filter((artifact) => artifact.file === cluster.file).map((artifact) => artifact.key)
  }

  return {
    status: 'ready',
    project: data.project || null,
    eventCount: (data.nodes || []).length,
    sessions: groups,
    artifacts: artifactGroups,
    counts: {
      openingAsks: asks.length,
      clusters: clusterList.length,
      boundaries: boundaries.length,
      tails: tails.length,
      artifacts: artifacts.length,
    },
  }
}
