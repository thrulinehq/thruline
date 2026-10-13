/* Settings, on the record: the served documents → the screen's data contract.
 *
 * PURE. Takes the bodies of the reads Settings makes (each may be null: not
 * read, or the read failed) and returns an object of DEMO's shape plus
 * `live: true` and the facts only the live page has. Nothing here fetches.
 *
 *   storage      GET /api/storage        what ThruLine keeps (L1.1), by part
 *   lanes        GET /api/lanes          recordings written in the last 5 min, per agent
 *   observation  GET /api/observation    HQ's own watch on Codex (and its pause)
 *   mind         GET /api/mind/status    the local model, available or not
 *   licence      GET /api/licence        Free or Pro — nothing else (C21-b), by data/plan.js
 *   projects     GET /api/hq/projects    the projects on this machine
 *   capture      GET /api/hq/projects/<id>/capture, for the project in front of the
 *                person (`current`, as every screen reads it): its `holds`, where its
 *                pause and path exclusions stand in the record (C46 §1.2(1));
 *                `{error}` when that read failed
 *   install      GET /api/hq/projects/<id>/install for the same project: where ThruLine's
 *                hooks and speaker hooks stand (C46 §1.2(2)); `{error}` when that read failed
 *   runPermissions GET /api/hq/projects/<id>/run-permissions: one exact recorded
 *                folder per row, including historical grants; `{error}` when unreadable
 *   templates    GET /api/hq/projects/<id>/done-templates: named lines from the
 *                committed base policy, previewed here without a write
 *   projectView  GET /api/hq/projects/<id> for the same project: only its newest piece of work, which
 *                Rules names in "What counts as done for …" (DESIGN-SPEC §3.10 G2); `{error}` when unread
 *
 * A fact nothing serves is null here, never a made-up default; the logic
 * says so in plain words beside the control it would have fed.
 */
import { storageReading } from '../../../workspace/inbox-view.js'
import { DEMO } from './demo.js'
import { homePath, homeless } from '../../data/paths.js'
import { fmtCount } from '../../data/count.js'
import { SESSIONS_BASIS, currentProjectId, sessionsKept, sessionsKeptPhrase, sessionsKeptReading } from '../../data/project.js'
import { holdsOf } from '../../data/holds.js'
import { captureFacts } from '../../data/capture.js'
import { installFacts } from '../Onboarding/install.js'
import { planOf } from '../../data/plan.js'
import { licenceView } from '../../../workspace/licence-view.js'

/* The facts of this version that are code, not record (docs/hq-next/HANDOFF.md
 * §4 corrects the canvas): `thruline init` installs 7 hook events and 1 dormant
 * one, for every project at once (crates/core/src/sensor.rs PINNED_EVENTS,
 * DORMANT_EVENTS), and run capture wraps 15 tools (crates/capture/src/shim.rs
 * WRAPPED), of which only those present on the machine get a wrapper. */
export const HOOK_EVENTS = Object.freeze(['SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'Stop', 'SessionEnd', 'PreCompact'])
export const DORMANT_EVENTS = Object.freeze(['StopFailure'])
export const SHIM_TOOLS = Object.freeze(['pytest', 'cargo', 'go', 'npm', 'pnpm', 'yarn', 'npx', 'make', 'mvn', 'gradle', 'dotnet', 'uv', 'uvx', 'python', 'python3'])

/* What each part of the store holds, in a person's words. Keys are the ones
 * /api/storage serves (crates/cli/src/uiserve.rs store_component). */
export const PART_WORDS = Object.freeze({
  records: { name: 'Records of your sessions', what: 'every recorded step, added to and never rewritten' },
  preserved_transcripts: { name: 'The work kept word for word', what: 'sealed copies of each transcript, kept even if the agent deletes its own' },
  command_log: { name: 'What was run', what: 'each command a check ran, what it printed and how it ended' },
  digest: { name: 'Summaries', what: 'the short briefs read at the start of a session' },
  export: { name: 'Copies written out', what: 'files you exported' },
  other: { name: 'Everything else', what: 'other files in the same folder' },
})

const SWATCHES = ['project', 'output', 'demo', 'saved_versions']
// The three built-in labels on the Settings artboard. Their standing and notes
// still come from `holds.exclusions`; a typed path keeps its own key as typed.
const BUILT_IN_PATHS = Object.freeze({ env: '.env  .env.*', secrets: 'secrets/', generated: 'generated files' })

/** The same rows for a capture read and for a successful write's answered holds. */
export function exclusionsFromHolds(holds) {
  return holds && Array.isArray(holds.exclusions) ? holds.exclusions.map((e) => ({
    key: e.key, path: e.builtIn ? BUILT_IN_PATHS[e.key] || e.covers : e.key, note: e.note, on: e.on,
    builtIn: e.builtIn, source: e.source, basis: e.basis,
  })) : []
}

const two = (n) => String(n).padStart(2, '0')
/** A time on screen: this machine's local time, HH:MM. */
export const hhmm = (d) => (d instanceof Date && !Number.isNaN(d.getTime()) ? two(d.getHours()) + ':' + two(d.getMinutes()) : null)

/** Free or Pro, and nothing else: the ONE plan rule (data/plan.js, C30 P0-6); null when the licence was not read. */
export { planOf }

const num = (v) => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : null)

/** One project-scoped template read. Never show the prior project's stale answer. */
export function templatesFromRead(read, projectId) {
  if (!projectId) return { status: 'no_project', rows: [], detail: null }
  if (!read || read.loading) return { status: 'loading', rows: [], detail: null }
  if (read.error) return { status: 'failed', rows: [], detail: null }
  const doc = read.data
  if (!doc) return { status: 'loading', rows: [], detail: null }
  if (doc.schema_version !== 1 || doc.project?.id !== projectId || !doc.policy || !Array.isArray(doc.templates)) {
    return { status: 'failed', rows: [], detail: null }
  }
  const state = doc.policy.state
  if (state === 'absent') return { status: 'absent', rows: [], detail: null }
  if (state !== 'read') return { status: 'unreadable', rows: [], detail: typeof doc.policy.detail === 'string' ? doc.policy.detail : null }
  const rows = doc.templates.map((t) => ({
    key: t?.name, text: t?.name, lines: t?.lines,
  }))
  if (rows.some((t) => typeof t.key !== 'string' || !t.key || !Array.isArray(t.lines) || !t.lines.length || t.lines.some((line) => typeof line !== 'string' || !line.trim()))) {
    return { status: 'failed', rows: [], detail: null }
  }
  return { status: rows.length ? 'read' : 'empty', rows, detail: null }
}

export function settingsFromServer({ storage = null, lanes = null, observation = null, mind = null, licence = null, projects = null, projectsError = null, capture = null, install = null, runPermissions = null, templates = null, projectView = null, current = undefined, now = new Date(), hqStopped = false } = {}) {
  const kept = storage ? storageReading(storage) : null
  // C29: the sessions First run states, from the same one derivation; null when the list is not served
  const sk = sessionsKept(projects)
  const sessions = sk && sk.projects ? { ...sk, phrase: sessionsKeptPhrase(sk), basis: SESSIONS_BASIS, reading: sessionsKeptReading(sk) } : null
  const parts = kept ? kept.parts.map((p, i) => {
    const w = PART_WORDS[p.key] || PART_WORDS.other
    const of = p.key === 'records' && sessions ? sessions.phrase + ' · ' : ''
    return {
      key: p.key, name: w.name, bytes: p.bytes, size: p.size, mb: Math.round(p.bytes / 1e6),
      swatch: SWATCHES[i % SWATCHES.length], what: of + w.what, whatT: of + p.key + ' · ' + fmtCount(p.bytes) + ' bytes',
      lose: '', stays: [], goes: [], restsOn: null,
    }
  }) : []
  const obs = observation && typeof observation === 'object' ? observation : null
  const watching = !!(obs && obs.enabled)
  const list = projects && Array.isArray(projects.projects) ? projects.projects : []
  // the project the per-project settings act on: the one in front of the person, as every screen reads it
  const curId = currentProjectId(list, current)
  const first = list.find((p) => p && p.id === curId) || null
  const permissionsMatch = !!(first && runPermissions && runPermissions.schema_version === 1 &&
    runPermissions.project === curId && Array.isArray(runPermissions.folders))
  const permissionStatus = projectsError ? 'project_failed' : !projects ? 'loading' : !first ? 'no_project'
    : runPermissions && runPermissions.error ? 'failed' : permissionsMatch ? 'read' : 'loading'
  // Settings may open the existing Check sheet only for a current Change whose
  // exact folder the permission read named. Historical rows have no Change to
  // check, and an unread permission document grants no navigation target.
  const checkChoices = permissionsMatch ? runPermissions.folders
    .filter((r) => r && !r.historical && typeof r.change_id === 'string' && r.change_id && typeof r.folder === 'string' && r.folder)
    .map((r) => ({ change: r.change_id, folder: r.folder })) : []
  const templateRead = templatesFromRead(templates, curId)
  // where its pause and path exclusions stand in the record (`holds`);
  // never the page's own guess
  const captureMatches = !!(capture && capture.project && capture.project.id === curId)
  const holds = captureMatches && !capture.error ? holdsOf(capture) : null
  const pausedAt = holds && holds.pausedAt ? new Date(holds.pausedAt) : null
  // whether the hooks Claude Code reads for that project name a ThruLine program that is gone
  // (`alive: false`, API.md "Capture health"): then nothing reaches the record through them, and
  // the page says so (DESIGN-SPEC §3.10 dead). Only a read of this project says it; a failed read says nothing.
  const facts = captureMatches && !capture.error ? captureFacts(capture) : null
  const hooksDead = !!facts && facts.phase === 'problem'
  const lastStepAt = facts ? (facts.record.latestStep && facts.record.latestStep.at) || facts.record.latestAt : null
  // where ThruLine's speaker hooks stand for that project, as the install plan read them
  const inst = install && !install.error ? installFacts(install) : null
  const speakerHooks = inst
    ? { read: 'read', state: inst.speaker.state, where: inst.speaker.where, refusal: inst.speaker.refusal, hooks: inst.hooks.state, mcp: inst.mcp.state, plan: inst.plan }
    : { read: install ? 'failed' : 'unread' }
  // the project's most recently active piece of work (threads come newest first), named only when this
  // project's own view was read and the thread has an id the Change screens take and a title to say
  const newest = projectView && projectView.project && projectView.project.id === curId && Array.isArray(projectView.threads)
    ? projectView.threads.find((t) => t && typeof t.id === 'string' && /^[0-9a-f]{16}$/.test(t.id) && typeof t.title === 'string' && t.title.trim()) || null
    : null
  const plan = planOf(licence)
  return {
    live: true,
    readAt: hhmm(now),
    readNow: now instanceof Date ? now.getTime() : null,
    hq: { stopped: hqStopped === true },
    project: {
      id: first ? first.id : null, name: first ? first.name : null, root: first && first.path_known !== false ? homePath(first.root) : null,
      paused: !!(holds && holds.paused), pausedAt: pausedAt ? hhmm(pausedAt) : null,
      exclusionsHonoured: !!(holds && holds.exclusionsHonoured),
      // read · failed (the capture read, or a document with no `holds`) · unread (not read yet)
      holds: holds ? 'read' : capture && capture.error ? 'failed' : captureMatches ? 'failed' : 'unread',
      // on when the plan read every speaker hook in a file Claude Code reads for this folder
      speaker: inst ? inst.speaker.state === 'installed' : null,
      speakerHooks,
    },
    sources: {
      claudeCode: {
        name: 'Claude Code', status: hooksDead ? 'hooks_dead' : 'observing', since: null,
        lastStep: hooksDead && lastStepAt ? hhmm(new Date(lastStepAt)) : null,
        recent: lanes && lanes.claude_code ? num(lanes.claude_code.live) : null,
        root: lanes && lanes.claude_code ? homePath(lanes.claude_code.root) || null : null,
        hookEvents: { count: HOOK_EVENTS.length, first: HOOK_EVENTS[0], last: HOOK_EVENTS[HOOK_EVENTS.length - 1], events: HOOK_EVENTS.slice(), dormant: DORMANT_EVENTS.slice(), notSubscribed: ['PermissionRequest', 'Notification'], scope: 'global' },
      },
      codex: {
        name: 'Codex',
        status: watching ? (obs.state === 'paused' ? 'paused' : 'observing') : 'not_watched',
        recent: lanes && lanes.codex ? num(lanes.codex.live) : null,
        root: lanes && lanes.codex ? homePath(lanes.codex.root) || null : null,
        observation: obs,
      },
      runCapture: { on: null, tools: { named: SHIM_TOOLS.slice(), more: 0 } },
    },
    lanesWindow: lanes ? num(lanes.window_minutes) : null,
    storage: {
      known: !!(kept && kept.known), bytes: kept ? kept.bytes : null, size: kept ? kept.size : null,
      root: kept && kept.path ? homePath(kept.path) : '', also: kept ? homeless(kept.also) : null, alsoCommandLog: kept ? homeless(kept.alsoCommandLog) : null,
      measured: storage && typeof storage.measured === 'string' ? storage.measured : null,
      mode: null, files: [], parts, kept: sessions,
    },
    // what each agent's capture can see: a description of this version, not a reading of
    // this machine. The one cell the canvas got wrong for `thruline init` is corrected.
    capabilities: DEMO.capabilities.map((c) => (c.key === 'tools'
      ? Object.assign({}, c, { cc: Object.assign({}, c.cc, { tech: 'PreToolUse · PostToolUse' }) })
      : c)),
    unsupportedAgents: lanes && lanes.gemini && lanes.gemini.roadmap ? [{ name: 'Gemini', reason: 'no adapter exists yet' }] : [],
    exclusions: exclusionsFromHolds(holds),
    checkPermissions: list.map((p) => ({ project: p.name, root: p.path_known === false ? null : homePath(p.root), allowed: null, hasRecords: (num(p.sessions) || 0) > 0 })),
    runPermission: { status: permissionStatus, folders: permissionsMatch ? runPermissions.folders : [] },
    checkWhere: { project: first ? first.name : '', folder: checkChoices.length === 1 ? checkChoices[0].folder : '', toolsFound: [], choices: checkChoices },
    criteriaTemplates: templateRead.rows,
    templateRead,
    policy: { path: '.thruline/policy.yml', text: '', wouldNotBeReady: { changes: null, ofLast: null }, example: { name: newest ? newest.title : null, change: newest ? newest.id : null } },
    masking: { detector: null, masked: null },
    egress: { localModel: mind && typeof mind.available === 'boolean' ? mind.available : null, model: mind && mind.model ? mind.model : null },
    prefs: { face: 'plain', maskedCopyOnExport: false, colourVision: false, density: 'comfortable', motion: 'system', notifications: { ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }, weeklyDigest: false, keys: 'on' },
    statePreview: { version: null, scope: [] },
    upgrade: { estimate: null },
    plan,
    // the licence's own words for Settings › Plan & licence (the licence page's one reading)
    licence: (() => { const l = licenceView(licence); return { sentence: l.sentence, fix: l.fix, gateOff: l.gateOff } })(),
  }
}
