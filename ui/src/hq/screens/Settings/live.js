/* Settings on the record: what the live page says where the gallery draws the
 * canvas's demo. Called by logic.js only when `this.props.data` is present,
 * after every gallery value is computed, so the design rendering cannot move.
 *
 * Three rules, from docs/hq-next/WAVE2-CARD.md:
 *  - a fact comes from a served field (from-server.js) or is said to be missing;
 *  - a control nothing in this version backs is drawn disabled, and the
 *    sentence beside it says so in plain words, then what it would do;
 *  - the canvas's factual errors are corrected (7 hook events and 1 dormant,
 *    installed for every project at once; 15 run-capture tools), and the plan
 *    is Free or Pro and nothing else: "Approving a version needs Pro. Everything
 *    else is free."
 */

import { recordingPill } from '../../data/project.js'
import { href } from '../../routes.js'
import { PLANS_URL } from '../../../workspace/licence-view.js'
import { fmtCount } from '../../data/count.js'
import { PLAN_WORDS } from '../../data/plan.js'
import { KEYS_NOTE, keyboardPane } from './keyboard.js'
import { appearanceText } from './appearance.js'
import { exclusionsFromHolds, hhmm } from './from-server.js'
import { safePermissionAction } from './run-permissions.js'

export const NA = 'Not available in this version.'

/** What a pause does, in the server's own first sentence (crates/core capture_hold.rs
 *  HONOURED_NOTE; settings-pause.test.js pins the two together). C34 Q5: a pause follows the
 *  session-start rule, so the canvas's "While paused nothing is recorded" is not said. */
export const PAUSE_SENTENCE = 'While paused, ThruLine reads nothing from this folder, and a session started while it was paused is never read.'
export const EXCLUSIONS_SENTENCE = "Never show or export these paths. ThruLine\u0027s sealed copy of each transcript still holds what Claude Code wrote."
/* The pricing page is named in one place only, the licence view's click-only
 * link (PRICING-TRUTHS, ui/src/workspace/licence.test.js). */
export const PRICING_URL = PLANS_URL
export const PLAN_SENTENCE = 'Approving a version needs Pro. Everything else is free.'

const listWords = (xs) => (xs.length <= 1 ? xs.join('') : xs.slice(0, -1).join(', ') + ' and ' + xs[xs.length - 1])

/** The canvas's consent line, true now that the switch is one project's own (C46 §1.2(2)). */
export const SPEAKER_CONSENT = 'Those lines become part of what your agent sends to its own provider. Off until you agree, one project at a time.'

/** A Claude Code settings level (doctor.rs `merged_settings_paths_for` scopes), as a sentence names it. */
const levelWords = (scope) => ({ user: 'your user', project: 'this project’s shared', managed: 'the managed', 'managed-drop-in': 'the managed' }[scope] || 'the ' + scope)
const plural = (n, one, many) => fmtCount(n) + ' ' + (n === 1 ? one : many)
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
/** A recorded instant as the design draws it, HH:MM and the day it belongs to: "09:40 today", "09:40 yesterday",
 *  "09:40, 5 Oct", with the year when it is not this one. Null when the instant cannot be read. */
function whenOf(iso, nowMs) {
  const d = new Date(iso)
  const clock = hhmm(d)
  if (!clock) return null
  const n = new Date(typeof nowMs === 'number' ? nowMs : Date.now())
  const day = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const ago = Math.round((day(n) - day(d)) / 86400000)
  if (ago === 0) return clock + ' today'
  if (ago === 1) return clock + ' yesterday'
  return clock + ', ' + d.getDate() + ' ' + MONTHS[d.getMonth()] + (d.getFullYear() === n.getFullYear() ? '' : ' ' + d.getFullYear())
}

export function liveOverrides(v, ctx) {
  const { D, s, tech, stopped, NOW, OFF, ui, mkSwitch, colors, setState, actions } = ctx
  const { T1, T2, T3, OR, CTRL, BLUE } = colors
  const CC = D.sources.claudeCode
  const CX = D.sources.codex
  const OBS = CX.observation || null
  const win = D.lanesWindow || 5
  const recentCC = CC.recent
  const recentCX = CX.recent
  const known = recentCC != null || recentCX != null
  const recent = (recentCC || 0) + (recentCX || 0)
  // hooks that name a missing program (from-server.js `hooks_dead`): the page keeps the dead
  // wording the design draws for the row, the pill and the status bar (logic.js), and no thread is lit
  const dead = !stopped && CC.status === 'hooks_dead'
  const lit = !stopped && !dead && recent > 0
  const noop = () => {}

  // ---- controls nothing in this version backs take the disabled look ----
  ;['perm', 'look', 'tpl', 'remove', 'confirm'].forEach((k) => { v.uiK[k] = OFF })
  v.uiK.retry = stopped ? OFF : ui
  v.addPath = noop
  v.lookElsewhere = noop
  v.pathNote = false
  v.lookNote = false
  const blocked = (on) => mkSwitch(on, noop, true)

  // the project in front of the person, and whether the record holds it paused (C46 §1.2(1))
  const P = D.project || {}
  const paused = P.holds === 'read' && !!P.paused

  // ---- header and status bar ----
  v.readStamp = stopped ? 'read ' + NOW + ' · not refreshing' : 'read ' + NOW + ' ↻'
  if (!stopped) {
    // the one recording derivation the pill uses on every screen (data/project.js)
    const rp = recordingPill({ lanes: known ? { claude_code: { live: recentCC || 0 }, codex: { live: recentCX || 0 }, window_minutes: win } : null })
    if (!dead) v.pill = paused
      ? { word: 'Paused by you', title: 'The record holds ' + P.name + ' paused' + (P.pausedAt ? ' since ' + P.pausedAt : '') + '.', color: T2, dotBg: 'transparent', dotBd: '1px dashed ' + T3, anim: 'none' }
      : { word: rp.text, title: rp.title, color: rp.color, dotBg: rp.dot, dotBd: rp.on ? '0' : '1px solid ' + T3, anim: rp.on ? ui.pulse : 'none' }
    const ccPart = paused ? 'Recording paused by you in ' + P.name : recentCC == null ? CC.name + ': not read' : CC.name + ': ' + plural(recentCC, 'recording', 'recordings') + ' written in the last ' + win + ' minutes'
    const cxPart = CX.status === 'observing' ? CX.name + ' watched live' : CX.status === 'paused' ? CX.name + ' paused by you' : CX.name + ' not watched live'
    if (!dead) v.statusLeft = ccPart + ' · ' + cxPart + ' · run capture: shown in a terminal'
  }

  // ---- where the record comes from ----
  v.bus = {
    line: lit ? '2px solid ' + OR : '2px dashed ' + CTRL,
    glow: lit ? '0 0 6px rgba(255,154,82,.55)' : 'none',
    stub: lit ? '2px solid ' + OR : '2px dashed ' + CTRL,
  }
  v.busCaption = lit ? 'the lit thread is what is being recorded now' : dead ? 'nothing is reaching the record right now' : 'nothing has reached the record in the last ' + win + ' minutes'
  const ccLive = !stopped && recentCC > 0
  const hookList = CC.hookEvents.events || []
  if (!dead) v.src.cc = Object.assign({}, v.src.cc, {
    live: ccLive,
    notLive: !ccLive,
    word: recentCC == null ? 'Not read' : ccLive ? 'Recording now' : 'Nothing new',
    color: ccLive ? BLUE : T1,
    sentence: 'ThruLine reads what ' + CC.name + ' records' + (CC.root ? ' in ' + CC.root : '') + '. Its hooks are installed for every project at once, by thruline init. ThruLine never sees your Claude sign-in.',
    stamp: recentCC == null ? 'read ' + NOW : plural(recentCC, 'recording', 'recordings') + ' in the last ' + win + ' min',
    hasMeta: tech,
    meta: plural(hookList.length, 'hook event', 'hook events') + ' · ' + hookList.join(', ') + ' · plus ' + (CC.hookEvents.dormant || []).join(', ') + ', wired but not yet observed · installed for every project by thruline init',
    anim: ccLive ? ui.pulse : 'none',
    nodeBg: lit ? T1 : 'transparent',
    nodeBd: lit ? '0' : '1.5px dashed ' + T3,
    nodeGlow: lit ? '0 0 0 3px rgba(255,122,41,.28)' : 'none',
  })
  if (paused && !stopped && !dead) {
    // the record holds this project paused: the row says so, in the session-start rule's words
    v.src.cc = Object.assign({}, v.src.cc, {
      live: false,
      notLive: true,
      word: 'Paused by you',
      color: T1,
      sentence: 'ThruLine reads nothing from ' + P.name + ' while it is paused' + (P.pausedAt ? ', since ' + P.pausedAt : '') + ', and a session started while it was paused is never read. ' + CC.name + ' in your other projects is recorded as before.',
      stamp: P.pausedAt ? 'paused ' + P.pausedAt : 'paused',
      anim: 'none',
    })
  }
  const tried = !!s.codexTried
  const cxWord = CX.status === 'observing' ? 'Watched live' : CX.status === 'paused' ? 'Paused by you' : 'Not watched live'
  v.src.cx = Object.assign({}, v.src.cx, {
    word: cxWord,
    column: CX.status === 'observing' ? '· watched live' : CX.status === 'paused' ? '· paused by you' : '· not watched live',
    line: CX.status === 'observing'
      ? plural(typeof OBS.events_written === 'number' ? OBS.events_written : 0, 'step', 'steps') + ' recorded since HQ started watching ' + CX.name + '.'
      : CX.status === 'paused'
        ? 'Nothing new from ' + CX.name + ' is recorded until you resume.'
        : CX.name + ' sessions reach the record when the project is loaded. This HQ is not watching for new ones.',
    sub: (tried ? 'Read again at ' + NOW + '. ' : '')
      + (CX.status === 'not_watched' ? 'Watching starts when HQ is started to watch a ' + CX.name + ' folder.' : (OBS && OBS.boundary) || '')
      + (tech && OBS && OBS.message ? ' ' + OBS.message : ''),
    // Read both the watch and selected project capture health again; it starts nothing.
    retryLabel: 'Read again',
    retry: stopped ? noop : () => {
      setState({ codexTried: true })
      if (actions.reloadObservation) actions.reloadObservation()
      if (actions.reloadCapture) actions.reloadCapture()
    },
  })
  // the button opens the steps and a Copy command action: run capture is turned on in a terminal
  v.run = Object.assign({}, v.run, { openLabel: 'Show how' })
  v.runCap = {
    word: 'Run capture: shown in a terminal',
    tools: 'Turning it on puts a small recorder ahead of the ' + fmtCount(D.sources.runCapture.tools.named.length) + ' tools it knows (' + listWords(D.sources.runCapture.tools.named) + '), those of them found on your PATH, so each run is recorded as a run, with its exit code.',
  }
  const size = D.storage.size
  v.src.rec = Object.assign({}, v.src.rec, {
    line: (size || 'Size not read') + (tech ? ' · append-only · hash-chained' : ' · added to, never rewritten'),
    nodeBg: lit ? T1 : 'transparent',
    nodeBd: lit ? '0' : '1.5px solid ' + T2,
  })
  v.content.recordTech = (D.storage.root || 'The record') + ' · append-only · hash-chained'

  // ---- what each agent can see ----
  v.notes.caps = 'This table describes this version of ThruLine. It is not read from your machine.'
  v.content.unsupported = D.unsupportedAgents.map((a) => a.name).join(', ')
  v.content.unsupportedWhy = D.unsupportedAgents.length ? ' · Not supported — ' + D.unsupportedAgents.map((a) => a.reason).join('; ') + '.' : ''

  // ---- what ThruLine may tell your agent (C46 §1.2(2)) ----
  // Where the speaker hooks stand comes from the install plan of the project in front of the person.
  // On is the install route with only the speaker ticked (write address 20), Off is the removal
  // (write address 30), both in the project's own .claude/settings.local.json. Hooks turned on at
  // another level (thruline init, the user's settings) are shown and never changed here.
  const SP = P.speakerHooks || { read: 'unread' }
  const onHere = SP.read === 'read' && SP.state === 'installed'
  const where = Array.isArray(SP.where) ? SP.where : []
  const onlyLocal = onHere && where.length > 0 && where.every((w) => w === 'project-local')
  const pressSpeaker = (on) => () => {
    if (s.speakerBusy || !actions.setSpeaker) return
    setState({ speakerBusy: true, speakerNote: null })
    Promise.resolve(actions.setSpeaker(on)).then(
      (a) => setState({ speakerBusy: false, speakerNote: a && a.kind === 'refused' ? a.detail : null }),
      () => setState({ speakerBusy: false, speakerNote: 'ThruLine’s local server did not answer, so this may not have changed. Reload the page to see where it stands.' }),
    )
  }
  v.s_speaker = onHere
  let consent = SPEAKER_CONSENT
  if (!P.id) {
    v.sw.speaker = blocked(false)
    consent = 'No project is recorded on this machine yet, so there is nowhere to add these lines.'
  } else if (SP.read === 'unread') {
    v.sw.speaker = blocked(false)
    consent = 'Reading whether ThruLine’s lines are on for ' + P.name + '…'
  } else if (SP.read === 'failed') {
    v.sw.speaker = blocked(false)
    consent = 'Whether ThruLine’s lines are on for ' + P.name + ' could not be read just now. Reload this page to try again.'
  } else if (onHere && !onlyLocal) {
    v.sw.speaker = blocked(true)
    consent = 'On for every session Claude Code starts in ' + P.name + ', set in ' + listWords(where.filter((w) => w !== 'project-local').map(levelWords)) + ' Claude Code settings, which this page does not change. Turn it off in a terminal: thruline init --undo.'
  } else if (onHere) {
    v.sw.speaker = mkSwitch(true, pressSpeaker(false), !!s.speakerBusy)
    // no consent time is recorded anywhere, so none is drawn ("you agreed at" in the canvas)
    v.content.speakerOn = 'On for ' + P.name + ' from its next session · in .claude/settings.local.json'
  } else if (SP.state === 'will_add' && SP.hooks === 'installed') {
    v.sw.speaker = mkSwitch(false, pressSpeaker(true), !!s.speakerBusy)
  } else if (SP.state === 'will_add') {
    v.sw.speaker = blocked(false)
    consent = 'ThruLine’s hooks are not installed for ' + P.name + ' yet, so there is nothing to add these lines to. Install them on First run, then turn this on.'
  } else {
    v.sw.speaker = blocked(false)
    consent = (SP.refusal && SP.refusal.plain) || 'ThruLine could not tell whether these lines can be added for ' + P.name + '.'
  }
  v.content.speakerConsent = s.speakerNote || consent
  // "Installed" only where the plan read for this project says the MCP server is (C43 G3: a record behind the word)
  v.content.mcpWord = SP.read === 'read'
    ? (SP.mcp === 'installed' ? 'Installed by thruline init' : SP.mcp === 'will_add' ? 'Not installed for ' + (P.name || 'this project') : SP.mcp === 'refused' ? 'Not installed by ThruLine here' : 'Could not be read just now')
    : SP.read === 'failed' ? 'Could not be read just now' : 'Not read yet'

  // ---- pause and leave out ----
  // C46 §1.2(1): the pause is the record's (write address 26, `actions.setPause`), for the project in
  // front of the person, and what the switch shows is what the record holds (`holds`), never the
  // page's own state. The six capture callers honour it; the same route pauses the Codex watch too.
  v.s_paused = paused
  v.content.pauseLabel = P.name ? 'Pause recording in ' + P.name : 'Pause recording'
  v.content.pauseSub = PAUSE_SENTENCE
  v.content.pausedGap = 'Paused by you' + (P.pausedAt ? ', ' + P.pausedAt + ' – now' : '')
  if (!P.id) {
    v.sw.pause = blocked(false)
    v.notes.pause = 'No project is recorded on this machine yet, so there is nothing to pause.'
  } else if (P.holds !== 'read') {
    v.sw.pause = blocked(false)
    v.notes.pause = P.holds === 'failed'
      ? 'Whether ' + P.name + ' is paused could not be read just now, so this switch waits until it is. Reload this page to try again.'
      : 'Reading whether ' + P.name + ' is paused…'
  } else {
    const act = paused ? 'resumed' : 'paused'
    v.sw.pause = mkSwitch(paused, () => {
      if (s.pauseBusy || !actions.setPause) return
      setState({ pauseBusy: true, pauseNote: null })
      Promise.resolve(actions.setPause(act)).then(
        (a) => setState({ pauseBusy: false, pauseNote: a && a.kind === 'refused' ? a.detail : null }),
        () => setState({ pauseBusy: false, pauseNote: 'ThruLine did not answer, so it may not have kept this. Reload this page to see what the record holds.' }),
      )
    }, !!s.pauseBusy)
    v.notes.pause = s.pauseNote || null
  }
  // A successful write answers with current holds before the capture GET it
  // triggers has completed. Keep that answer until the GET replaces its doc;
  // the checkbox and its next act must agree during that interval.
  const waitingForRead = !!s.exclusionAnswer && s.exclusionAnswer.project === P.id && s.exclusionAnswer.from === actions.captureDoc
  const answeredHolds = waitingForRead && s.exclusionAnswer.holds && Array.isArray(s.exclusionAnswer.holds.exclusions) ? s.exclusionAnswer.holds : null
  const exclusionRows = answeredHolds ? exclusionsFromHolds(answeredHolds) : D.exclusions
  const canExclude = !!P.id && P.holds === 'read' && (answeredHolds ? answeredHolds.exclusionsHonoured : P.exclusionsHonoured)
  const exclusionReady = canExclude && !s.exclusionBusy && !waitingForRead && !stopped
  v.uiK.addPath = exclusionReady ? ui : OFF
  v.uiK.excl = exclusionReady ? ui : OFF
  v.content.exclTitle = 'Paths left out of views and exports'
  v.content.exclGroup = 'Paths left out of views and exports'
  const exclusionReason = !P.id
    ? 'No project is recorded on this machine yet, so there are no paths to leave out.'
    : P.holds === 'unread'
      ? 'Reading which paths ' + P.name + ' leaves out…'
      : P.holds === 'failed'
        ? 'Which paths ' + P.name + ' leaves out could not be read just now. Reload this page to try again.'
        : waitingForRead
          ? 'Reading which paths ' + P.name + ' leaves out after that change…'
          : !canExclude
          ? 'This copy of ThruLine cannot leave paths out when showing or exporting a view.'
          : null
  v.notes.excl = [s.exclusionProject === P.id ? s.exclusionNote : null, exclusionReason, EXCLUSIONS_SENTENCE].filter(Boolean).join(' ')
  let exclusionPending = !!s.exclusionBusy
  const keepExclusion = (act, pattern, added) => {
    if (!exclusionReady || exclusionPending || !actions.setExclusion) return
    exclusionPending = true
    setState({ exclusionBusy: true, exclusionNote: null, exclusionProject: P.id })
    Promise.resolve(actions.setExclusion(act, pattern)).then(
      (a) => {
        const answer = { project: P.id, from: actions.captureDoc, holds: a && a.kind === 'kept' ? a.holds : null }
        setState(a && a.kind === 'kept' && added
          ? { exclusionBusy: false, exclusionNote: null, exclusionAnswer: answer, pathNote: false, pathDraft: '' }
          : { exclusionBusy: false, exclusionNote: a && a.kind === 'refused' ? a.detail : null, exclusionAnswer: answer })
      },
      () => {
        setState({ exclusionBusy: false, exclusionNote: 'ThruLine did not answer, so it may not have kept this. Reload this page to see which paths are left out.', exclusionAnswer: { project: P.id, from: actions.captureDoc, holds: null } })
      },
    )
  }
  v.excl = P.holds === 'read' ? exclusionRows.map((e) => ({
    key: e.key,
    path: e.path,
    note: e.note || 'your path',
    checked: e.on ? 'true' : 'false',
    mark: e.on ? '✓' : '',
    boxBg: e.on ? (s.exclusionBusy ? T2 : T1) : 'transparent',
    boxBd: e.on ? '1px solid ' + (s.exclusionBusy ? T2 : T1) : (s.exclusionBusy ? '1px dashed ' + CTRL : '1px solid ' + CTRL),
    toggle: () => keepExclusion(e.on ? 'included' : 'excluded', e.key, false),
  })) : []
  v.pathNote = false
  v.addPath = () => {
    if (exclusionReady && !exclusionPending) setState({ pathNote: s.pathProject !== P.id || !s.pathNote, pathProject: P.id, pathDraft: '', exclusionNote: null })
  }
  v.pathEditor = {
    open: !!s.pathNote && s.pathProject === P.id && canExclude,
    value: s.pathDraft || '',
    busy: !!s.exclusionBusy || waitingForRead,
    change: (value) => { if (!exclusionPending) setState({ pathDraft: value }) },
    submit: (event) => {
      if (event && event.preventDefault) event.preventDefault()
      if (s.pathProject === P.id) keepExclusion('excluded', s.pathDraft || '', true)
    },
    cancel: () => { if (!exclusionPending) setState({ pathNote: false, pathDraft: '', exclusionNote: null }) },
  }

  // ---- checks and permissions: exact recorded folders, including old grants ----
  const permissions = D.runPermission || { status: 'loading', folders: [] }
  const permissionBusy = !!s.permissionBusy && s.permissionProject === P.id
  const permissionNote = s.permissionProject === P.id ? s.permissionNote : null
  const permissionNoteFor = s.permissionProject === P.id ? s.permissionNoteFor : null
  v.permissionNotice = permissionNote && permissionNoteFor &&
    !permissions.folders.some((r) => r.id === permissionNoteFor) ? permissionNote : null
  let permissionPress = false // ignore a second press before React has rendered pending state
  const plainRow = (word, sub) => ({ decided: true, project: P.name || 'No project yet', folder: null, word, color: T2, sub, btn: null, toggle: noop, look: OFF, divider: '0' })
  if (permissions.status === 'project_failed') {
    v.permRows = [plainRow('Not known', 'ThruLine could not read the projects on this machine. Reload the page to see check permissions.')]
  } else if (permissions.status === 'no_project') {
    v.permRows = [plainRow('No project yet', 'Pick a project to see where checks may run.')]
  } else if (permissions.status === 'failed') {
    v.permRows = [plainRow('Not known', 'Permissions for this project could not be read just now. Reload the page to try again.')]
  } else if (permissions.status !== 'read') {
    v.permRows = [plainRow('Reading permissions…', 'Reading the record for this project’s check folders…')]
  } else if (!permissions.folders.length) {
    v.permRows = [plainRow('No permission folders yet', 'The first check in a recorded folder asks for permission there.')]
  } else {
    v.permRows = permissions.folders.map((r, i) => {
      const grant = r.state === 'given'
      const taken = r.state === 'revoked'
      const missing = r.state === 'file_missing'
      const unrecorded = r.state === 'not_in_record'
      const never = r.state === 'never'
      const when = r.at ? whenOf(r.at, D.readNow) : null
      const word = grant ? 'Allowed' + (r.actor ? ' by ' + r.actor : '') + (when ? ' · ' + when : '')
        : taken ? 'Taken back' : missing || unrecorded ? 'Not allowed here' : never ? 'Not asked yet' : 'Not known'
      const detail = grant
        ? r.historical ? 'This older folder is no longer named by a current Change. Taking back permission leaves earlier runs untouched.' : 'Checks may run from this folder only when someone presses Check it again.'
        : taken ? 'The next check for this folder asks first. Nothing already run is changed.'
          : missing ? 'The permission file is missing, so checks are not allowed here. Earlier runs are unchanged.'
            : unrecorded ? 'A permission file was found without a matching record, so checks are not allowed here.'
              : never ? 'ThruLine asks for this folder when someone first checks. A check runs only when pressed.'
                : 'ThruLine could not tell where this permission stands.'
      const safeAction = safePermissionAction(r)
      const canAct = !stopped && !permissionBusy && !!actions.setRunPermission && !!safeAction
      const btn = !safeAction ? null : safeAction === 'revoke' ? 'Take back permission' : 'Allow again'
      const rowBusy = permissionBusy && s.permissionBusy === r.id
      const toggle = canAct ? () => {
        if (permissionPress) return
        permissionPress = true
        const request = Symbol('permission press')
        setState({ permissionBusy: r.id, permissionProject: P.id, permissionRequest: request, permissionNote: null, permissionNoteFor: null })
        const finish = (note) => setState((current) =>
          current.permissionProject === P.id && current.permissionBusy === r.id && current.permissionRequest === request
            ? { permissionBusy: null, permissionRequest: null, permissionNote: note, permissionNoteFor: r.id }
            : null)
        let work
        try { work = actions.setRunPermission(r) } catch (_e) { work = Promise.reject(_e) }
        Promise.resolve(work).then((answer) => {
          finish(answer && answer.kind === 'refused' ? answer.detail : null)
        }, () => {
          finish('ThruLine could not read this permission again. Reload the page to see where it stands.')
        })
      } : noop
      return {
        id: r.id, decided: true, project: P.name, folder: r.folder, word: rowBusy ? 'Checking permission…' : word,
        color: grant && !rowBusy ? T1 : T2, btn, toggle,
        look: canAct ? ui : OFF, divider: i === 0 ? '0' : '1px solid #1F1F23',
        sub: rowBusy ? 'Reading the permission again…' : permissionNote && permissionNoteFor === r.id ? permissionNote : detail,
      }
    })
    if (v.permRows.some((r) => r.look !== OFF)) v.uiK.perm = ui
  }
  v.where = {
    project: D.checkWhere.project || 'No project yet',
    folder: D.checkWhere.folder || '',
    found: permissions.status === 'project_failed' ? 'ThruLine could not read the projects on this machine. Reload the page to choose a check folder.'
      : permissions.status === 'no_project' ? 'Pick a project to see which Changes have a check folder.'
        : permissions.status === 'failed' ? 'ThruLine could not read this project’s check folders. Reload the page to try again.'
          : permissions.status !== 'read' ? 'Reading this project’s check folders…'
            : (D.checkWhere.choices || []).length === 0 ? 'No current Change has a check folder here. Open a Change with a check to choose where it runs.'
              : 'Checks use each Change’s recorded folder. Open its Check sheet to see places ThruLine found; picking one runs the check there.',
  }
  const checkChoices = D.checkWhere.choices || []
  const canBrowse = !stopped && permissions.status === 'read' && checkChoices.length > 0 && !!actions.openCheckPicker
  v.uiK.look = canBrowse ? ui : OFF
  v.lookElsewhere = canBrowse ? () => {
    if (checkChoices.length === 1) actions.openCheckPicker(checkChoices[0].change)
    else setState({ lookNote: !(s.lookNote && s.lookProject === P.id), lookProject: P.id })
  } : noop
  v.lookChoices = canBrowse && checkChoices.length > 1 && s.lookNote && s.lookProject === P.id
    ? checkChoices.map((choice) => ({
      label: choice.folder,
      open: () => actions.openCheckPicker(choice.change),
    })) : []
  v.notes.checks = 'Checking is never paid.'

  // ---- rules ----
  const templateRead = D.templateRead || { status: 'loading', rows: [], detail: null }
  const templateWords = {
    no_project: 'Choose a project to read its templates.',
    loading: 'Reading this project’s committed base policy…',
    failed: 'ThruLine could not read this project’s templates just now. Reload to try again.',
    absent: 'This project has no .thruline/policy.yml on its base branch, so it has no templates to preview.',
    empty: 'This project’s committed base policy holds no named templates.',
    unreadable: 'ThruLine could not read this project’s committed base policy, so no template is shown.',
    read: 'These template lines are available to pick in What counts as done; they are never added automatically.',
  }
  const templateState = templateRead.status
  const templateWord = templateWords[templateState] || templateWords.failed
  const templateDetail = tech && templateRead.detail ? ' ' + templateRead.detail : ''
  const templateRows = templateRead.rows || []
  v.tpls = templateRows.length ? templateRows.map((t, i) => ({
    text: t.text,
    lines: t.lines,
    divider: i === 0 ? '0' : '1px solid #1F1F23',
    word: 'From the committed base policy · available to pick, never added automatically',
    btn: 'Stop offering',
    disabled: true,
    toggle: noop,
  })) : [{
    text: 'No template lines to preview', lines: [], divider: '0', word: templateWord + templateDetail,
    btn: 'Offer to new Changes', disabled: true, toggle: noop,
  }]
  v.content.tplNote = (templateRows.length ? templateWord : '') + (templateRows.length ? ' ' : '') + 'Not yet. Settings can preview templates but cannot save changes to the committed base policy. To offer or stop offering a template, update .thruline/policy.yml on the base branch this machine reads, then reload. Picking a template still requires you to accept its lines.'
  v.content.policyText = 'The full policy file is not served here. Named template lines from the committed base branch are previewed above.'
  v.content.policyPreview = 'Not yet. This page cannot count how an unsaved policy edit would affect past Changes.'
  // the Change the link names and opens (from-server.js `policy.example`); with none read it stays the generic words and no link
  const eg = D.policy.example || {}
  v.content.policyExample = eg.name && eg.change ? 'What counts as done for ' + eg.name + ' ' : 'What counts as done, Change by Change '
  if (eg.name && eg.change && P.id) v.content.policyHref = href('done', { change: eg.change, project: P.id })

  // ---- privacy and storage (L1.1) ----
  const st = D.storage
  const parts = st.parts || []
  v.totalText = size || 'an unknown amount'
  v.store = {
    headline: size ? 'ThruLine is keeping ' + size + ' of records on this machine' : 'How much ThruLine keeps could not be read just now. Reload the page to try again.',
    where: (st.root || '') + (st.also ? ' and ' + st.also : '') + (st.alsoCommandLog ? ' and ' + st.alsoCommandLog : '') + (tech && st.measured ? ' · ' + st.measured : ''),
    segs: parts.map((p, i) => ({ grow: String(p.bytes), bg: [T1, T2, CTRL, '#2A2A30'][i % 4], sel: i % 4 === 3 ? 'inset 0 0 0 1px ' + CTRL : 'none' })),
    rows: parts.map((p, i) => ({
      divider: i === 0 ? '0' : '1px solid #1F1F23',
      swatch: [T1, T2, CTRL, '#2A2A30'][i % 4],
      swatchBd: i % 4 === 3 ? '1px solid ' + CTRL : '0',
      name: p.name,
      nameColor: T1,
      what: tech ? p.whatT : p.what,
      size: p.size,
      sizeColor: T1,
      canRemove: true,
      isRemoved: false,
      expanded: 'false',
      askBg: 'transparent',
      ask: noop,
      open: false,
      confirmLabel: '', lose: '', pieces: [], hasGuard: false, guard: '', confirmText: '', confirm: noop, keep: noop,
    })),
  }
  // C29: the sessions count First run states too, from the one derivation (data/project.js sessionsKept), with its basis
  v.notes.storage = 'Removing is not available in this version: ThruLine removes nothing yet. Each part above says what it holds.'
    + (st.kept ? ' Records of your sessions: ' + st.kept.phrase + ', the sum over every project ThruLine lists. ' + st.kept.basis + (st.kept.reading ? ' ' + st.kept.reading : '') : '')
  v.content.localModel = D.egress.localModel == null ? 'Not read' : D.egress.localModel ? 'Running' : 'Not running'
  v.content.masking = 'detector v2 · best-effort'
  // the masked copy, the notifications and the weekly summary: the person's stored choices (appearance.js, below)

  // ---- appearance and keyboard ----
  v.content.previewMeta = ''
  const offSeg = (segs) => segs.map((o, i) => Object.assign({}, o, { dis: 'true', bl: i === 0 ? '0' : '1px dashed ' + CTRL, pick: noop }))
  // C30 R-KEYS: the choice is kept in preferences.json, and the map is the one key table
  Object.assign(v, keyboardPane({ prefs: ctx.prefs || null, mkSeg: ctx.mkSeg || offSeg, setPreference: actions.setPreference }))
  v.content.triageNote = KEYS_NOTE
  // C30 R-PREFS: face, density, motion, colour vision, masked copy, notifications and the weekly summary are kept
  // in preferences.json too; this says beside each what it does now, and what a write that was not kept left standing
  appearanceText(v, { prefs: ctx.prefs || null, s, mkSwitch, notificationPermission: ctx.notificationPermission })

  // ---- plan (C21-b: Free or Pro, nothing else) ----
  // the word is the one plan rule's (data/plan.js); a licence not read states no plan
  const planWord = D.plan ? PLAN_WORDS[D.plan] || null : null
  v.content.planNav = planWord ? 'Plan: ' + planWord : 'Plan'
  v.content.asideNote = 'Nothing on this page is sent anywhere. ' + (planWord ? 'Plan: ' + planWord + '. ' : '') + PLAN_SENTENCE + ' '
  v.planLink = { href: PRICING_URL, label: 'See plans' }
  v.planPane = Object.assign({}, v.planPane, { plansHref: PRICING_URL })
  return v
}
