/* L2 derivation — one claim, and everything the record can say about it.
 *
 * Provenance: the receipt content model is carried from the pre-R2 shell's
 * inspector (ui/src/App.jsx, the `rc` block and its Claim / Evidence / Verdict /
 * Capture proof / Ledger sections) together with lib/design.js's composers and
 * lib/capture.js's receipt view. Nothing here is new engine detail — R2.3
 * *wraps* the existing receipt rather than reinventing it, which is why the
 * engine's own wording survives byte-for-byte behind the fold.
 *
 * The six parts are the DESIGN-HANDOFF §4.3 anatomy, in order. Each dimension
 * is labeled and none is conflated with another: strength is not currency, and
 * currency is not truth. M5.R2 separated them at cost. */

import { captureReceiptView } from './capture.js'
import { fmt } from './design.js'
import { presentationState } from './fixture.js'
import { normalizeRecordText } from './record-text.js'
import { LEDGER_STATE, claimStory, countPhrase, sentence, word } from './voice.js'

/**
 * The ledger line, in the A80 shape.
 *
 * Not `Ledger ✓ 777,404 events`: that string was three leaks in one — a
 * provider, a bare model tag, and a check mark standing in for a word. The
 * count carries its noun, and a chain that is NOT intact says so rather than
 * dropping the clause (Rule 12).
 */
export const ledgerLine = (l) => {
  if (l == null) return ''
  const events = countPhrase(fmt(l.total), 'event on the ledger', 'events on the ledger')
  /* A150 (P1): the two facts the old boolean conflated, spoken separately by
     the authority. `chain_intact` may be absent (an export older than the
     split) or null (never measured) — absence of a check is not a state of
     the chain, so both fall to "unchecked" rather than to an accusation.
     "chain broken" is dead: an intact chain with dead receipts must never
     wear it again. */
  const chain =
    l.chain_intact === true
      ? LEDGER_STATE.chain_checked.word
      : l.chain_intact === false
        ? LEDGER_STATE.chain_damaged.word
        : l.verified
          ? LEDGER_STATE.chain_checked.word
          : LEDGER_STATE.never_checked.word
  const parts = [events, `chain ${chain}`]
  if (l.raw_files_missing > 0) {
    parts.push(
      `${countPhrase(fmt(l.raw_files_missing), 'source file no longer on disk', 'source files no longer on disk')}`,
    )
  }
  return parts.join(' · ')
}

/** Engine detail, kept intact and kept behind the fold. */
export const judgedDetail = (verdict) =>
  verdict?.score == null ? verdict?.text || '' : `${verdict.text} · ${verdict.score.toFixed(2)}`

const EVIDENCE_ACTION = {
  command: 'Command run',
  edit: 'File edited',
  read: 'File read',
  write: 'File written',
  test: 'Test run',
}

const slashPath = (value) => {
  const normalized = String(value || '').replace(/\\/g, '/')
  if (normalized === '/') return '/'
  return normalized.replace(/\/+$/, '')
}

/** A readable first-level path without discarding the original raw value. */
export function evidencePathPresentation(path, cwd) {
  if (!path) return null
  const rawPath = String(path)
  const normalizedPath = slashPath(path)
  const normalizedCwd = slashPath(cwd)
  const relativeInput = !normalizedPath.startsWith('/') && !/^[A-Za-z]:\//.test(normalizedPath)
  if (relativeInput) {
    return { display: normalizedPath, label: 'Workspace path', outside: false, rawPath }
  }
  const underCwd = normalizedCwd === '/'
    ? normalizedPath.startsWith('/')
    : normalizedCwd && (normalizedPath === normalizedCwd || normalizedPath.startsWith(`${normalizedCwd}/`))
  if (underCwd) {
    const relative = normalizedPath === normalizedCwd
      ? '.'
      : normalizedCwd === '/'
        ? normalizedPath.slice(1)
        : normalizedPath.slice(normalizedCwd.length + 1)
    return { display: relative, label: 'Workspace path', outside: false, rawPath }
  }
  return boundedOutsidePathPresentation(rawPath)
}

/** A bounded display for a path whose workspace relationship was not proven. */
export function boundedOutsidePathPresentation(path, label = 'Outside workspace') {
  if (!path) return null
  const rawPath = String(path)
  const normalizedPath = slashPath(path)
  const parts = normalizedPath.split('/').filter(Boolean)
  const homeOffset = /^(?:[A-Za-z]:)?$/i.test(parts[0] || '') && /^Users$/i.test(parts[1] || '')
    ? 3
    : /^(?:Users|home)$/i.test(parts[0] || '')
      ? 2
      : 0
  const safeParts = homeOffset > 0 ? parts.slice(homeOffset) : parts
  const tail = safeParts.slice(-3).join('/') || 'path not shown'
  const abbreviated = normalizedPath.startsWith('/') || /^[A-Za-z]:[\\/]/.test(normalizedPath) || safeParts.length > 3
  return { display: abbreviated ? `…/${tail}` : tail, label, outside: true, rawPath }
}

const ABSOLUTE_PATH = /(?:^|[\s("'`=])(?:\/(?!\/)[^\s"'`]+|[A-Za-z]:[\\/][^\s"'`]+)/
const COMMAND_SHAPE = /^(?:bash|zsh|sh|fish|powershell|cmd|command|git|cargo|npm|pnpm|yarn|node|python\d*)\b|^[>$]\s|(?:\s(?:&&|\|\||\|)\s)|(?:→|⇒|=>)|\b(?:stdout|stderr|output|result)\s*:/i
const ABSOLUTE_PATH_SPAN = /(^|[\s("'`=:\[])((?:\/(?!\/)|[A-Za-z]:[\\/])[^\s"'`<>|→⇒]+)/gm
const RAW_CLAIM_FALLBACK = 'Exported record; exact text is available in Raw receipt details.'

const trimPathPunctuation = (value) => {
  let path = value
  let suffix = ''
  while (/[),.;:!?\]}]$/.test(path)) {
    suffix = `${path.slice(-1)}${suffix}`
    path = path.slice(0, -1)
  }
  return { path, suffix }
}

function inlineClaimPath(path, evidence) {
  const containing = new Map()
  for (const item of evidence) {
    if (item?.path !== path || !item.scope?.cwd) continue
    const presentation = evidencePathPresentation(path, item.scope.cwd)
    if (!presentation || presentation.outside) continue
    containing.set(slashPath(item.scope.cwd), presentation)
  }
  if (containing.size === 1) {
    return `[Project-relative path: ${[...containing.values()][0].display}]`
  }
  const ambiguous = containing.size > 1
  const bounded = boundedOutsidePathPresentation(path, ambiguous ? 'Workspace relation ambiguous' : 'Workspace relation unproven')
  return `[${bounded.label}: ${bounded.display}]`
}

/**
 * Display-only claim copy. Raw claim bytes remain in `rawReceiptDetails`.
 * Absolute path spans are rewritten only from exact evidence, or bounded when
 * the workspace relation cannot be proved. Command/result dumps are withheld.
 */
export function claimTextPresentation(claimText, evidence = []) {
  const raw = String(claimText || '')
  const normalized = normalizeRecordText(raw)
  const commandIdentities = evidence.map((item) => item?.scope?.command_identity).filter(Boolean)
  const containsScopedCommand = commandIdentities.some((value) => {
    const exact = String(value).trim()
    return exact.length >= 4 && raw.toLowerCase().includes(exact.toLowerCase())
  })
  if (!normalized || COMMAND_SHAPE.test(raw.trim()) || containsScopedCommand) {
    const action = evidence.length > 0 ? (EVIDENCE_ACTION[evidence[0]?.kind] || 'Evidence recorded') : 'Exported record'
    return { text: `${action}; exact text is available in Raw receipt details.`, reduced: true }
  }

  const text = normalized.replace(ABSOLUTE_PATH_SPAN, (match, prefix, candidate) => {
    const { path, suffix } = trimPathPunctuation(candidate)
    return `${prefix}${inlineClaimPath(path, evidence)}${suffix}`
  })
  if (ABSOLUTE_PATH.test(text)) return { text: RAW_CLAIM_FALLBACK, reduced: true }
  return { text, reduced: text !== normalized }
}

/** Keep only summaries that are already readable prose, never command/output dumps. */
export function safeEvidenceSummary(evidence) {
  const raw = String(evidence?.summary || '')
  if (!raw.trim()) return ''
  const scope = evidence?.scope || {}
  if (raw.includes('\n') || raw.includes('\r')) return ''
  if (ABSOLUTE_PATH.test(raw) || COMMAND_SHAPE.test(raw)) return ''
  for (const privateValue of [scope.cwd, scope.command_identity]) {
    const value = String(privateValue || '').trim()
    if (value.length >= 4 && raw.toLowerCase().includes(value.toLowerCase())) return ''
  }
  return normalizeRecordText(raw)
}

/** Human-readable evidence presentation; all raw fields remain beside it. */
export function evidencePresentation(evidence) {
  const scope = evidence?.scope || {}
  return {
    action: EVIDENCE_ACTION[evidence?.kind] || 'Evidence recorded',
    summary: safeEvidenceSummary(evidence),
    line: scope.seq == null ? '' : String(scope.seq),
    path: evidencePathPresentation(evidence?.path, scope.cwd),
  }
}

/** Where a receipt line points, down to file and line. */
function whereFrom(scope, evidence) {
  const parts = []
  if (evidence?.path) parts.push({ label: 'File', value: evidence.path })
  if (scope?.cwd) parts.push({ label: 'Working directory', value: scope.cwd })
  if (scope?.git_branch) parts.push({ label: 'Branch', value: scope.git_branch })
  if (scope?.command_identity) parts.push({ label: 'Command', value: scope.command_identity })
  if (scope?.session) parts.push({ label: 'Run', value: scope.session })
  if (scope?.seq != null) parts.push({ label: 'Line', value: String(scope.seq) })
  return parts
}

/**
 * The claim at `nodeId`, as the six parts of L2.
 *
 * @param {object} raw     the project fixture, exactly as exported
 * @param {string} nodeId  which claim
 */
export function claimView(raw, nodeId) {
  const node = (raw.nodes || []).find((n) => n.id === nodeId)
  if (!node) return null
  const contractVersion = Number(raw.contract_version || 1)
  const state = presentationState(node, contractVersion)
  const r = raw.receipts?.[nodeId] || {}
  const session = (raw.sessions || []).find((s) => s.id === node.session_id)

  const evidence = (r.evidence || []).map((ev, i) => ({
    key: i,
    kind: ev.kind,
    /* The project's own words. Never rewritten to fit our table — a quotation
       is the project speaking, not us (doctrine, `why` level 2). */
    summary: ev.summary,
    where: whereFrom(ev.scope, ev),
    presentation: evidencePresentation(ev),
    raw: ev,
  }))

  /* Candidate proof history is deliberately separate from the deciding verdict
     line: a stale nearby run is context, not automatically the evidence that
     governed this claim. Carried from App.jsx's `freshness_notes`. */
  const freshness = [
    ...new Set(
      (r.claim?.claims || [])
        .flatMap((c) => c.evidence_scopes || [])
        .filter((s) => s.freshness === 'stale')
        .map((s) => s.freshness_note)
        .filter(Boolean),
    ),
  ]

  const capture = captureReceiptView(raw.capture_receipts?.[node.anchor_event_id])
  const rawClaimText = r.claim?.text || node.label || ''

  /* Currency is its own dimension, and `stale` is the only positive thing
     Forensic Mode can prove about freshness (§M4.4). DUSK appears exactly
     here, and only here — nothing summons it at zero. */
  const currency = node.dimensions?.currency || null
  const stale = capture?.currentness === 'stale' || freshness.length > 0 || state === 'stale'

  return {
    id: nodeId,
    state,
    word: word(state),

    /* 1 — the sentence */
    sentence: sentence(state),

    /* 2 — what happened */
    story: claimStory(state),
    /* Receipt counts follow §4: "2 receipts", never "2". A claim with nothing
       to trace to drops the clause rather than saying "0 receipts". */
    receiptCount: evidence.length,
    receiptPhrase: evidence.length > 0 ? countPhrase(evidence.length, 'receipt', 'receipts') : '',

    /* 3 — the receipt, to file and line */
    claimText: rawClaimText,
    claimPresentation: claimTextPresentation(rawClaimText, r.evidence || []),
    run: session?.label || r.claim?.session || node.session_id,
    runDate: session?.date || '',
    messageNo: r.claim?.message_no ?? null,
    ts: r.claim?.ts || '',
    /* No evidence means no receipt section at all (§4): a zero-count element
       does not render, and part 2's story has already said why the record is
       quiet here. The old shell's two "nothing here" lines were prose composed
       in a view, and they do not come across. */
    evidence,

    /* 4 — how this was judged (engine detail, one disclosure behind) */
    judged: judgedDetail(r.verdict),
    model: r.verdict?.model || '',

    /* 5 — history & currency */
    currency,
    stale,
    freshness,
    capture,
    ledger: r.ledger || null,
    ledgerLine: ledgerLine(r.ledger),

    /* 6 — the door to L3 */
    anchor: node.anchor_event_id,
    sessionId: node.session_id,
    rawReceiptDetails: {
      node_id: node.id,
      event_id: node.anchor_event_id,
      session_id: node.session_id,
      recorded_at: r.claim?.ts || '',
      claim: r.claim || { text: node.label || '' },
      lineage: (raw.edges || []).filter((edge) => edge.from === node.id || edge.to === node.id),
      evidence: r.evidence || [],
      ledger: r.ledger || null,
    },
  }
}
