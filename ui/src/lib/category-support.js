/* Session-scoped deterministic support for the four interpretation branches.
 *
 * Input is the accepted Layer-2 `workMap` only. This module never reads model
 * output, candidate interpretation prose, transcript bytes, or the filesystem.
 * It emits bounded navigation records, not conclusions. */

export const RECORDED_ARTIFACT_LIMIT = 4

const list = (value) => (Array.isArray(value) ? value : [])

function domToken(value) {
  return Array.from(String(value || ''), (character) => (
    /[A-Za-z0-9_-]/.test(character)
      ? character
      : `_${character.codePointAt(0).toString(16)}_`
  )).join('') || 'unknown'
}

export function recordedDetailId(sessionId) {
  return `wm-recorded-${domToken(sessionId)}`
}

export function artifactDetailId(artifactKey) {
  return `wm-${domToken(artifactKey)}`
}

export function recordedArtifactRegionId(sessionId) {
  return `wm-unresolved-artifacts-${domToken(sessionId)}`
}

function exactPathRelation(item) {
  return list(item?.exactPath?.clusterMatches).length + list(item?.exactPath?.evidenceMatches).length > 0
}

function artifactNavigation(item) {
  return {
    key: item.key,
    detailId: artifactDetailId(item.key),
    href: `#${artifactDetailId(item.key)}`,
    filePresentation: item.filePresentation,
    exactPathRelation: exactPathRelation(item),
  }
}

export function sessionCategorySupport(map, sessionId) {
  const matchingGroups = list(map?.sessions).filter((group) => group?.session?.id === sessionId)
  if (map?.status !== 'ready' || typeof sessionId !== 'string' || !sessionId || matchingGroups.length !== 1) {
    return {
      status: 'unavailable',
      sessionId: typeof sessionId === 'string' ? sessionId : '',
      happened: { clusters: 0, boundaries: 0, detailId: '', detailHref: '' },
      missing: { tails: 0, artifacts: [], detailId: '', detailHref: '' },
      lagging: { represented: false },
      contradictory: { represented: false },
    }
  }

  const group = matchingGroups[0]
  const detailId = recordedDetailId(sessionId)
  const unresolvedArtifacts = list(map?.artifacts?.unresolved)
    .filter((item) => item?.fate === 'unresolved' && list(item.sessions).includes(sessionId))
    .map(artifactNavigation)

  return {
    status: 'ready',
    sessionId,
    happened: {
      clusters: list(group.clusters).length,
      boundaries: list(group.boundaries).length,
      detailId,
      detailHref: `#${detailId}`,
    },
    missing: {
      tails: list(group.tails).length,
      artifacts: unresolvedArtifacts,
      detailId,
      detailHref: `#${detailId}`,
    },
    lagging: { represented: false },
    contradictory: { represented: false },
  }
}

export function unresolvedArtifactWindow(artifacts, expanded = false) {
  const all = list(artifacts)
  const hasDisclosure = all.length > RECORDED_ARTIFACT_LIMIT
  const isExpanded = hasDisclosure && expanded === true
  const items = isExpanded ? all : all.slice(0, RECORDED_ARTIFACT_LIMIT)
  return {
    items,
    total: all.length,
    shown: items.length,
    expanded: isExpanded,
    hasDisclosure,
    controlLabel: !hasDisclosure
      ? ''
      : isExpanded
        ? 'Show fewer unresolved artifacts'
        : `Show all ${all.length} unresolved artifacts`,
  }
}
