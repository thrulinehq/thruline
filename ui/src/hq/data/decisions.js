/* Decisions on HQ screens: the one reader and the one model.
 *
 * A decision is a standing rule the person set for a project, in their own
 * words (`span`), checked by the engine against exact versions
 * (docs/decisions/API.md on decisions/engine, schema_version 1). Every word
 * and number here comes from a typed field of that contract; nothing is
 * counted from clipped findings and no verdict is ever inferred:
 *   - a route this build's server does not have reads "not checked", never
 *     "holds" (API.md: "the UI must show that it cannot check the decision");
 *   - `current_verdict: null` reads "Not checked yet", never "holds";
 *   - a proposal is not in force until the person accepts it and confirms it
 *     themselves, in a terminal or in the separate window that opens on this
 *     Mac (C28/C35: the HTTP routes record a pending act and activate nothing);
 *   - whether a decision refuses an approval is the engine's typed answer
 *     (`approval_gate`, `Decision.blocks_approval`, `Verdict.blocks_approval`,
 *     C35), never derived from the state word and the severity: a dormant
 *     gate member, a warning, a valid waiver, a stale projection and an
 *     unverifiable result under `on_unverifiable: warn` refuse nothing.
 *
 * Words: "the record shows", "no proof found", "broken on <version>". Never
 * an intent word, never a confidence. */

import { shortVersion as hqShortVersion } from './project.js'

export const DECISIONS_API = Object.freeze({
  list: (project) => `/api/hq/projects/${encodeURIComponent(project)}/decisions`,
  detail: (project, id, version) =>
    `/api/hq/projects/${encodeURIComponent(project)}/decisions/${encodeURIComponent(id)}` +
    (version ? `?version=${encodeURIComponent(version)}` : ''),
})

/** Dispatched on `window` when a confirmation is recorded, so every screen's
 *  `useDecisions` re-reads the list now instead of at its next turn. */
export const DECISIONS_CHANGED = 'thruline:decisions-changed'

/** The terminal path that activates a person act (C28). HQ shows it to copy;
 *  it runs nothing. */
export const TERMINAL = Object.freeze({
  waive: (id, version) => `thruline decide prepare waive ${id} --version-id ${version} --actor <your name> --because <why>`,
  retire: (id) => `thruline decide prepare retire ${id} --actor <your name> --because <why>`,
  accept: (id) => `thruline decide prepare accept ${id} --actor <your name> --because <why>`,
  confirm: 'thruline decide confirm <action> <act_row_id>',
})

/**
 * Read one project's decisions. Never throws.
 *   {status: 'served', doc}        the list, schema_version 1
 *   {status: 'not_served'}         this build's server has no Decisions route
 *   {status: 'unreadable', message} the route answered, and not with a list
 */
export async function readDecisions(project, fetchImpl = typeof fetch === 'function' ? fetch : null) {
  if (!project || !fetchImpl) return { status: 'not_served' }
  let res
  try {
    res = await fetchImpl(DECISIONS_API.list(project), { headers: { accept: 'application/json' } })
  } catch (e) {
    return { status: 'unreadable', message: 'ThruLine’s local server did not answer, so no decision was checked.' }
  }
  let body = null
  try {
    body = JSON.parse(await res.text())
  } catch (e) {
    body = null
  }
  const contract = body && body.schema_version === 1
  if (res.ok && contract && Array.isArray(body.decisions)) return { status: 'served', doc: body }
  if (res.status === 404 && !contract) return { status: 'not_served' }
  if (res.status === 503) return { status: 'unreadable', message: 'The record needs an upgrade before its decisions can be read. Run thruline upgrade in a terminal.' }
  const why = contract && body.detail ? String(body.detail) : contract && body.error ? String(body.error) : `it answered ${res.status}`
  return { status: 'unreadable', message: `ThruLine could not read this project’s decisions: ${why}.` }
}

/** A version as a person reads it: the one rule every HQ screen uses
 *  (data/project.js `shortVersion`), so a decision's "Broken on wt-3c3ca28"
 *  names the version exactly as the Brief's crumb and the Inbox rows do. */
export const shortVersion = (id) => hqShortVersion(id) ?? null

const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`

/**
 * The list's `approval_gate` as the screens read it. `known` is false when the
 * server did not serve one (an older server): the screens then say they cannot
 * tell, and refuse nothing. `reason` is the server's own text, shown as given.
 */
export function gateOf(doc) {
  const g = doc && typeof doc === 'object' ? doc.approval_gate : null
  if (!g || typeof g !== 'object' || typeof g.active !== 'boolean') return { known: false, active: null, reason: null }
  return { known: true, active: g.active, reason: typeof g.reason === 'string' && g.reason ? g.reason : null }
}

/**
 * The list's `confirmation_transport`, reduced to what a control needs:
 *   native  true only when the server says `available: 'native_click'` AND its
 *           native preflight says available (C35); anything else is false
 *   mode    the server's `available` word
 *   reason  the server's own text for why not, shown as given
 * null when the server served none. Preparing is never a person act, and
 * nothing here says a confirmation happened.
 */
export function transportOf(doc) {
  const t = doc && typeof doc === 'object' ? doc.confirmation_transport : null
  if (!t || typeof t !== 'object') return null
  const mode = ['native_click', 'terminal', 'unavailable'].includes(t.available) ? t.available : 'unavailable'
  const native = mode === 'native_click' && !!t.native && t.native.available === true
  const reason = [t.reason, t.native && t.native.reason].find((x) => typeof x === 'string' && x) || null
  return { mode, native, reason }
}

/** Where one finding is, in the record's own words. */
function findingLine(f) {
  if (!f) return null
  const where = f.path ? (f.line != null ? `${f.path}:${f.line}` : f.path) : f.screen ? `the ${f.screen} screen` : null
  return { id: f.id, where, message: f.message || null, span: f.span || null, sessionId: f.session_id || null, editId: f.edit_id || null }
}

/**
 * One decision as every screen draws it. `ctx` is what the list says around
 * it: `{project, gate, transport, currentCommit}` (decisionsModel supplies it).
 */
export function decisionRow(d, currentVersion, ctx = {}) {
  const v = d.current_verdict || null
  const lifecycle = d.lifecycle
  const ver = v && v.version ? shortVersion(v.version.version_id) : null
  const now = shortVersion(currentVersion)
  let state
  let sentence
  if (lifecycle === 'proposed') {
    state = 'proposed'
    sentence = 'Proposed. Not in force until you accept it and confirm it yourself.'
  } else if (lifecycle === 'retired' || lifecycle === 'superseded') {
    state = lifecycle
    sentence = lifecycle === 'retired' ? 'Retired.' : 'Replaced by a later revision.'
  } else if (!v) {
    state = 'not_checked'
    sentence = 'Not checked yet.'
  } else if (v.state === 'broken') {
    state = 'broken'
    sentence = `Broken on ${ver}: the record shows it in ${plural(v.finding_count, 'place', 'places')}.`
  } else if (v.state === 'holds') {
    state = 'holds'
    sentence = `Holds on ${ver}, in ${plural(v.checked_places, 'place', 'places')} checked.`
  } else if (v.state === 'unverifiable') {
    state = 'unverifiable'
    sentence = `No proof found on ${ver}${v.reason ? `: ${v.reason}` : ''}.`
  } else if (v.state === 'not_applicable') {
    state = 'not_applicable'
    sentence = `Does not apply to ${ver}.`
  } else if (v.state === 'stale') {
    state = 'stale'
    const moved = v.stale_reason === 'working_tree_dirty' ? 'there are uncommitted edits since' : 'a later commit moved the code since'
    sentence = `Last checked on ${ver}; ${moved}${now ? `, so it is not checked on ${now}` : ''}.`
  } else {
    state = 'not_checked'
    sentence = 'Not checked yet.'
  }
  const waived = v && v.waived ? { actor: v.waived.actor, because: v.waived.because, version: shortVersion(v.waived.version_id), at: v.waived.at } : null
  const first = v && Array.isArray(v.findings) && v.findings.length ? findingLine(v.findings[0]) : null
  const gate = ctx.gate || null
  const severityWords =
    d.severity === 'blocks' ? (gate && gate.active === false ? 'Set to block approval' : 'Blocks approval')
    : d.severity === 'warns' ? 'Warns'
    : lifecycle === 'proposed' ? 'You choose whether it blocks approval or only warns when you accept it'
    : ''
  return {
    id: d.id,
    words: d.span,
    kind: d.kind,
    kindCode: d.kind_code,
    revision: Number.isInteger(d.revision) ? d.revision : null,
    severity: d.severity,
    blocks: d.severity === 'blocks',
    severityWords,
    /* the engine's own answers about approving (C35): this decision on the
       exact version the list was read at, and this result on its own version */
    blocksApproval: d.blocks_approval === true,
    verdictBlocks: !!v && v.blocks_approval === true,
    commitOid: v && v.version && v.version.commit_oid ? v.version.commit_oid : null,
    project: ctx.project || null,
    transport: ctx.transport || null,
    currentCommit: ctx.currentCommit || null,
    lifecycle,
    state,
    sentence,
    checkableIf: v && v.state === 'unverifiable' && v.checkable_if ? `Checkable when ${v.checkable_if}.` : null,
    version: ver,
    versionId: v && v.version ? v.version.version_id : null,
    findingCount: v ? v.finding_count : null,
    firstFinding: first,
    findings: v && Array.isArray(v.findings) ? v.findings.map(findingLine) : [],
    waived,
    waivedSentence: waived ? `Set aside on ${waived.version} by ${waived.actor}: “${waived.because}”` : null,
    lastHeld: v && v.last_held_version ? shortVersion(v.last_held_version) : null,
    acceptedBy: d.accepted_by || null,
    source: Array.isArray(d.sources) && d.sources[0] ? { file: d.sources[0].raw_file, line: d.sources[0].raw_line, at: d.sources[0].recorded_at } : null,
    commands: {
      waive: lifecycle === 'accepted' && v && v.version ? TERMINAL.waive(d.id, v.version.version_id) : null,
      retire: lifecycle === 'accepted' ? TERMINAL.retire(d.id) : null,
      accept: lifecycle === 'proposed' ? TERMINAL.accept(d.id) : null,
      confirm: TERMINAL.confirm,
    },
  }
}

const ORDER = { broken: 0, unverifiable: 1, stale: 2, not_checked: 3, holds: 4, not_applicable: 5, proposed: 6, retired: 7, superseded: 8 }

/**
 * The project's decisions, one model for every screen.
 *   status       'served' | 'not_served' | 'unreadable', or 'reading' before
 *                the first answer (no read handed in): a screen says nothing
 *                then, rather than "not checked" for a route it has not asked
 *   broken       accepted decisions the record shows broken (the Inbox's group)
 *   rows         every decision, broken first, blocking before warning
 *   counts       from the rows the server served, never from clipped findings
 *   gate         the list's `approval_gate` (gateOf): known, active, reason
 *   transport    the list's `confirmation_transport` (transportOf), or null
 *   project      the project id the list was served for
 *   currentVersionId / currentCommit   the list's `current_version`, the exact
 *                version the engine's `blocks_approval` answers are about
 */
export function decisionsModel(read) {
  if (!read || read.status !== 'served') {
    const status = read ? read.status : 'reading'
    return {
      status,
      message: status === 'unreadable' ? read.message : status === 'reading' ? null : 'Decisions are not checked in this version of ThruLine.',
      rows: [],
      broken: [],
      counts: null,
      currentVersion: null,
      currentVersionId: null,
      currentCommit: null,
      project: null,
      gate: { known: false, active: null, reason: null },
      transport: null,
    }
  }
  const cv = read.doc.current_version || null
  const current = cv ? cv.version_id : null
  const ctx = {
    project: typeof read.doc.project_id === 'string' ? read.doc.project_id : null,
    gate: gateOf(read.doc),
    transport: transportOf(read.doc),
    currentCommit: cv && typeof cv.commit_oid === 'string' && cv.commit_oid ? cv.commit_oid : null,
  }
  const rows = read.doc.decisions
    .map((d) => decisionRow(d, current, ctx))
    .sort((a, b) => ORDER[a.state] - ORDER[b.state] || Number(b.blocks) - Number(a.blocks) || String(a.id).localeCompare(String(b.id)))
  const broken = rows.filter((r) => r.state === 'broken' && r.lifecycle === 'accepted' && !r.waived)
  return {
    status: 'served',
    message: null,
    rows,
    broken,
    counts: {
      decisions: rows.length,
      accepted: rows.filter((r) => r.lifecycle === 'accepted').length,
      broken: broken.length,
      proposed: rows.filter((r) => r.lifecycle === 'proposed').length,
    },
    currentVersion: shortVersion(current),
    currentVersionId: current,
    currentCommit: ctx.currentCommit,
    project: ctx.project,
    gate: ctx.gate,
    transport: ctx.transport,
  }
}

/**
 * Why approving `versionId` is refused by a decision, from the engine's typed
 * answers (C35, docs/decisions/API.md "List, detail and proposals") and not
 * from the state word or the severity:
 *
 *   - nothing is refused unless the list's `approval_gate.active` is true: a
 *     dormant member enables no refusal, and a server that served no gate gets
 *     none from HQ (the server stays the gate);
 *   - a result on exactly `versionId` refuses when that verdict's own
 *     `blocks_approval` is true (kind 'broken' or 'no_proof': the result's own
 *     words). A warning decision, a valid waiver, a stale projection and an
 *     unverifiable result under `on_unverifiable: warn` carry `false`, so they
 *     refuse nothing;
 *   - on the version the list was read at (its `current_version`), an
 *     accepted decision whose own `blocks_approval` is true and which has no
 *     result on that version refuses as kind 'no_check': "No check is
 *     recorded on <version> yet" (the missing check itself blocks, under the
 *     active policy). The decision's flag answers for that version only, so a
 *     different version gets no refusal from it.
 *
 * Each item carries the decision's own words. The server is the gate; this
 * says why before the button is pressed.
 */
export function approvalRefusals(model, versionId) {
  if (!model || model.status !== 'served' || !versionId) return []
  if (!model.gate || model.gate.active !== true) return []
  const isCurrent = versionId === model.currentVersionId || (!!model.currentCommit && versionId === model.currentCommit)
  const short = shortVersion(versionId)
  const out = []
  for (const r of model.rows) {
    if (r.lifecycle !== 'accepted') continue
    const resultHere = !!r.versionId && (r.versionId === versionId || (!!r.commitOid && r.commitOid === versionId))
    const byResult = resultHere && r.verdictBlocks
    const byDecision = isCurrent && r.blocksApproval
    if (!byResult && !byDecision) continue
    let kind
    let sentence
    if (resultHere && r.state === 'broken') {
      kind = 'broken'
      sentence = `Broken on ${r.version}: the record shows it in ${plural(r.findingCount, 'place', 'places')}.`
    } else if (resultHere && r.state === 'unverifiable') {
      kind = 'no_proof'
      sentence = r.sentence
    } else if (!resultHere) {
      kind = 'no_check'
      sentence = `No check is recorded on ${short} yet.` + (r.version ? ` The latest result is on ${r.version}.` : '')
    } else {
      kind = 'blocked'
      sentence = `${r.sentence} ThruLine’s approval check still counts it as blocking.`
    }
    out.push({ id: r.id, kind, words: r.words, sentence, firstFinding: resultHere ? r.firstFinding : null })
  }
  return out
}

/**
 * One sentence about the gate, for the screens that say whether decisions
 * block approving: present only when it matters (some accepted decision is set
 * to block) and the gate is not active or was not served. The server's own
 * `reason` is appended as given.
 */
export function gateNote(model) {
  if (!model || model.status !== 'served') return null
  if (!model.rows.some((r) => r.lifecycle === 'accepted' && r.blocks)) return null
  const g = model.gate
  if (!g || !g.known) return 'ThruLine did not say whether decisions block approving, so none is shown as blocking here.'
  if (g.active) return null
  return 'Decisions you set do not block approving right now.' + (g.reason ? ' ' + g.reason : '')
}

/**
 * What "Confirm on this Mac" may offer for one row and one action
 * (`accept` | `retire` | `waive`). PURE. It offers a control only where the
 * terminal path is offered (`row.commands[action]`), only when the list says
 * the native transport is available, and only when the exact fields the
 * server needs are known:
 *
 *   {offer: true, action, fixed}   `fixed` are the action's own fields the
 *                                  person does not type (accept: the revision
 *                                  and the exact commit it applies from;
 *                                  waive: the exact commit of the result)
 *   {offer: false, note}           the sentence to say instead (the server's
 *                                  own reason as given, or what is missing);
 *                                  null when nothing needs saying
 */
export function confirmPlan(row, action) {
  if (!row || !row.commands || !row.commands[action]) return { offer: false, note: null }
  const t = row.transport
  if (!t) return { offer: false, note: null }
  if (!t.native) {
    return { offer: false, note: 'Confirming on this Mac from this page is not available here.' + (t.reason ? ' ' + t.reason : ' Use the terminal path above.') }
  }
  if (!row.project) return { offer: false, note: null }
  if (action === 'accept') {
    if (row.revision == null) return { offer: false, note: 'ThruLine did not say which revision this is, so accepting from this page is not offered. Use the terminal path above.' }
    if (!row.currentCommit) return { offer: false, note: 'ThruLine has no exact commit to apply this from right now (uncommitted edits, or Git could not name one), so accepting from this page is not offered. Use the terminal path above.' }
    return { offer: true, action, fixed: { revision: row.revision, applies_from_version: row.currentCommit } }
  }
  if (action === 'waive') {
    if (!row.commitOid) return { offer: false, note: 'This result is not on a commit, so setting it aside from this page is not offered. Use the terminal path above.' }
    return { offer: true, action, fixed: { version_id: row.commitOid } }
  }
  if (action === 'retire') return { offer: true, action, fixed: {} }
  return { offer: false, note: null }
}

/** The accepted decisions not checked on `versionId` (stale or another version). */
export function notCheckedOn(model, versionId) {
  if (!model || model.status !== 'served' || !versionId) return []
  return model.rows.filter((r) => r.lifecycle === 'accepted' && r.blocks && r.versionId !== versionId && r.state !== 'not_checked')
}

/** ⌘K: each decision, findable by its words, its kind and its state. */
export function decisionSearchItems(model, hrefFor) {
  if (!model || model.status !== 'served') return []
  return model.rows.map((r) => ({
    id: 'd:' + r.id,
    kind: 'Decision',
    text: r.words,
    sub: r.sentence,
    hay: [r.words, r.kind, r.kindCode, r.sentence, r.severity === 'blocks' ? 'blocks approval' : r.severity === 'warns' ? 'warns' : ''].filter(Boolean).join(' ').toLowerCase(),
    href: hrefFor ? hrefFor(r) : null,
  }))
}
