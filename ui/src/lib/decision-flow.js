/* A vertical reading of the ordinary §9 surface.
 *
 * This module is presentation only. It groups the export's own nodes by the
 * export's own session order, carries every ordinary-surface node exactly
 * once, and attaches only receipt/edge facts already present in the document.
 * The vertical spine in the component is recorded order, never a derived
 * causal edge; exported lineage stays separate and keeps its exact kind. */

import { surfaceProjection } from './fixture.js'
import { recordTextPreview } from './record-text.js'
import { chipLabel, word } from './voice.js'

export const EVENT_PAGE_SIZE = 40

const nodeText = (node, receipt) => receipt?.claim?.text || node.label || ''

function lineageIndex(edges) {
  const incoming = new Map()
  const outgoing = new Map()
  for (const edge of edges) {
    if (!incoming.has(edge.to)) incoming.set(edge.to, [])
    if (!outgoing.has(edge.from)) outgoing.set(edge.from, [])
    incoming.get(edge.to).push(edge)
    outgoing.get(edge.from).push(edge)
  }
  return { incoming, outgoing }
}

function lineageFor(id, index) {
  const incoming = index.incoming.get(id) || []
  const outgoing = index.outgoing.get(id) || []
  const all = [...incoming, ...outgoing]
  return {
    incoming,
    outgoing,
    count: all.length,
    kinds: [...new Set(all.map((edge) => edge.kind).filter(Boolean))],
  }
}

function presentationNode(node, receipts, lineage) {
  const receipt = receipts[node.id] || null
  const evidenceCount = Number.isSafeInteger(node.evidence_count)
    ? node.evidence_count
    : Array.isArray(receipt?.evidence)
      ? receipt.evidence.length
      : null
  const statusWord = word(node.state)
  const text = nodeText(node, receipt)
  const preview = recordTextPreview(text)
  const messageNo = receipt?.claim?.message_no ?? null
  const recordedAt = receipt?.claim?.ts || ''
  const evidenceSearch = (receipt?.evidence || []).flatMap((evidence) => [
    evidence.summary,
    evidence.path,
    evidence.kind,
    evidence.scope?.cwd,
    evidence.scope?.command_identity,
  ])

  return {
    id: node.id,
    state: node.state,
    statusWord,
    statusLabel: chipLabel(statusWord),
    text,
    preview,
    label: node.label || '',
    sessionId: node.session_id,
    anchorEventId: node.anchor_event_id || '',
    evidenceCount,
    messageNo,
    recordedAt,
    receipt,
    lineage: lineageFor(node.id, lineage),
    record: node,
    searchText: [text, node.label, statusWord, node.anchor_event_id, node.id, messageNo, ...evidenceSearch]
      .filter((value) => value != null && value !== '')
      .join(' ')
      .toLowerCase(),
  }
}

/**
 * Group the ordinary projection into vertical session lanes.
 *
 * Listed sessions retain export order. Nodes retain their order within each
 * session. A node whose session is not listed still renders once in a final
 * explicitly-unlisted lane, in first-seen order; silence is never the fallback.
 */
export function decisionFlow(raw) {
  const data = surfaceProjection(raw)
  const edges = data.edges || []
  const lineage = lineageIndex(edges)
  const receipts = data.receipts || {}
  const openingAsks = data.candidates?.opening_asks || []
  const asksBySession = new Map()
  for (const ask of openingAsks) {
    if (!asksBySession.has(ask.session_id)) asksBySession.set(ask.session_id, [])
    asksBySession.get(ask.session_id).push(ask)
  }

  const groups = (data.sessions || []).map((session) => ({
    session,
    listed: true,
    openingAsks: asksBySession.get(session.id) || [],
    nodes: [],
  }))
  const bySession = new Map(groups.map((group) => [group.session.id, group]))

  for (const node of data.nodes || []) {
    let group = bySession.get(node.session_id)
    if (!group) {
      group = {
        session: {
          id: node.session_id,
          label: 'Run not listed in the session index',
          date: '',
          msgs: null,
        },
        listed: false,
        openingAsks: asksBySession.get(node.session_id) || [],
        nodes: [],
      }
      bySession.set(node.session_id, group)
      groups.push(group)
    }
    group.nodes.push(presentationNode(node, receipts, lineage))
  }

  return {
    groups,
    edges,
    totalCount: (data.nodes || []).length,
    nodeIds: (data.nodes || []).map((node) => node.id),
  }
}

/** Preserve lane/node order while applying the existing OR-state and text filters. */
export function filterDecisionFlow(flow, { filters = [], search = '' } = {}) {
  const states = new Set(filters)
  const query = search.trim().toLowerCase()
  const groups = flow.groups
    .map((group) => ({
      ...group,
      nodes: group.nodes.filter((node) => {
        if (states.size > 0 && !states.has(node.state)) return false
        return query === '' || node.searchText.includes(query)
      }),
    }))
    .filter((group) => group.nodes.length > 0)
  const shownCount = groups.reduce((sum, group) => sum + group.nodes.length, 0)

  return {
    ...flow,
    groups,
    shownCount,
    hiddenCount: flow.totalCount - shownCount,
    query,
    filters: [...filters],
  }
}

/**
 * Mount a bounded prefix of each already-filtered run.
 *
 * Filtering happens before this function. A selected claim expands only its
 * own run far enough to include the exact exported node; no record is moved.
 */
export function windowDecisionFlow(flow, { limits = {}, selected = null, pageSize = EVENT_PAGE_SIZE } = {}) {
  const safePage = Number.isSafeInteger(pageSize) && pageSize > 0 ? pageSize : EVENT_PAGE_SIZE
  const groups = flow.groups.map((group) => {
    const configured = limits[group.session.id]
    let limit = configured === Infinity
      ? group.nodes.length
      : Number.isSafeInteger(configured) && configured > 0
        ? configured
        : safePage
    const selectedIndex = selected ? group.nodes.findIndex((node) => node.id === selected) : -1
    if (selectedIndex >= limit) limit = Math.ceil((selectedIndex + 1) / safePage) * safePage
    const nodes = group.nodes.slice(0, limit)
    return {
      ...group,
      nodes,
      matchingCount: group.nodes.length,
      mountedCount: nodes.length,
      remainingCount: Math.max(0, group.nodes.length - nodes.length),
    }
  })
  const mountedCount = groups.reduce((sum, group) => sum + group.mountedCount, 0)
  return {
    ...flow,
    groups,
    matchingCount: flow.shownCount,
    mountedCount,
    remainingCount: Math.max(0, flow.shownCount - mountedCount),
    pageSize: safePage,
  }
}
