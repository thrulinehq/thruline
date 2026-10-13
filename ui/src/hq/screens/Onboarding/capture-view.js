/* First run on a live address, step 3 "Check it's recording" (C30 R-ROUTES
 * "Capture health"; census §3.11 rows 3, 7, 8, 9, 12, 13, 14, 17).
 *
 * PURE: no fetch, no window, no clock but the `now` it is handed.
 *
 *   withCapture(data, {capture, ask, known, now})
 *       First run's data (data/project.js `firstRunData`) with the chosen
 *       folder's capture read folded into that folder, in the screen's own
 *       contract (DATA.md: `folders[].capture`, `folders[].git`). index.jsx
 *       calls it; so do the tests, so they draw exactly what the app draws.
 *
 *   stepThree(proj, choices, now)
 *       Every word step 3 draws for that folder, from what the route served
 *       (data/capture.js `captureOf`) and nothing else. logic.js lays the
 *       words into the canvas's rows, pill, card and footer.
 *
 * The rules it keeps (C30 G3, "Nothing in v0.2 may"):
 *  - "Installed" only when the route found a ThruLine recorder; "Found" only
 *    when git answered that it is a repository; "Received" and "Recording"
 *    only when the hooks have delivered a step to the record.
 *  - What the route does not read — the MCP server, speaker hooks, run
 *    capture — is "Not checked yet" once the hooks are installed, never
 *    "Installed"; before that, those rows are the person's step-2 choices.
 *  - A failed read, a read still out, and a fact the route could not read
 *    are each said as such; none becomes "installed" or "not installed".
 *  - Paths are written with the home folder as ~ (paths.js).
 */
import { captureOf } from '../../data/capture.js'
import { fmtWhen } from '../../data/project.js'
import { fmtCount } from '../../data/count.js'
import { homePath, homeless } from '../../data/paths.js'
import { href } from '../../routes.js'
import { installRows, installedSub } from './install-view.js'

/**
 * First run's data with one capture read folded in.
 * @param {object} data           firstRunData(...), plus loadError
 * @param {object} opts
 * @param {{project, doc, error, reading}|null} opts.capture  useCapture's answer for the chosen folder
 * @param {object|null} opts.ask  useFirstAsk's answer (first-ask.js)
 * @param {Object<string, {repository, toplevel, reason}>} opts.known  each folder's last Git answer
 * @param {Date} opts.now
 */
export function withCapture(data, { capture = null, ask = null, known = {}, now = new Date() } = {}) {
  if (!data || !Array.isArray(data.folders)) return data
  const cur = capture && capture.project ? capture : null
  const vm = cur ? captureOf(cur) : null
  const folders = data.folders.map((f) => {
    if (!f) return f
    const mine = !!vm && cur.project === f.id
    const git = mine && vm.state === 'read' ? vm.git : (known && known[f.id]) || null
    let out = f
    if (git) out = Object.assign({}, out, { git: git.repository, gitReason: git.repository === null ? git.reason || null : null, gitTop: git.toplevel || null })
    if (mine) out = Object.assign({}, out, { capture: Object.assign({}, vm, { ask: ask && ask.project === f.id ? ask : null, now }) })
    return out
  })
  return Object.assign({}, data, { folders })
}

const cap1 = (t) => (t ? t.charAt(0).toUpperCase() + t.slice(1) : t)
/** A served reason as a row's detail: home folder as ~, a capital, no full stop (the canvas's row style). */
const detail = (t) => cap1(homeless(String(t)).trim().replace(/[.]$/, ''))
/** A served reason as a sentence. */
const sentence = (t) => {
  const x = cap1(homeless(String(t)).trim())
  return /[.!?]$/.test(x) ? x : x + '.'
}
const joinList = (a) => (a.length <= 1 ? a.join('') : a.slice(0, -1).join(', ') + ' and ' + a[a.length - 1])
const plural = (n, one, many) => fmtCount(n) + ' ' + (n === 1 ? one : many)

/** "16:01", "yesterday", "5 Sep" — the record's time on this machine's clock. */
const when = (iso, now) => fmtWhen(iso, now) || null
/** "at 16:01", "yesterday", "on 5 Sep": the same, inside a sentence. */
function atWhen(iso, now) {
  const w = when(iso, now)
  if (!w) return null
  if (/^\d\d:\d\d$/.test(w)) return 'at ' + w
  return w === 'yesterday' ? w : 'on ' + w
}

/** First line, spaces collapsed, cut at a word with "…": a quote that fits the card. */
function clip(text, n) {
  const lines = String(text).split('\n').map((x) => x.trim()).filter(Boolean)
  const one = (lines[0] || '').replace(/\s+/g, ' ')
  if (one.length > n) return one.slice(0, n - 1).replace(/\s+\S*$/, '') + '…'
  return lines.length > 1 ? one + '…' : one
}

/** Where the recorder sits, and for whom: "For every folder, from ~/.claude/settings.json · 8 events". */
function whereWords(f, projPath) {
  const root = f.project && f.project.root ? f.project.root.replace(/\/+$/, '') : null
  const files = f.hooks.where.map((w) => {
    const p = w.path || ''
    if ((w.scope === 'project' || w.scope === 'project-local') && root && p.startsWith(root + '/')) return p.slice(root.length + 1)
    return homePath(p)
  }).filter(Boolean)
  const scopes = f.hooks.where.map((w) => w.scope)
  const lead = !files.length ? 'In the Claude Code settings this folder reads'
    : scopes.includes('user') ? 'For every folder, from ' + joinList(files)
      : scopes.every((x) => x === 'project' || x === 'project-local') ? 'For ' + projPath + ' only, from ' + joinList(files)
        : 'From ' + joinList(files)
  const ev = f.hooks.events
  return lead + (typeof ev === 'number' ? ' · ' + plural(ev, 'event', 'events') : '')
}

/**
 * Every word step 3 draws for one folder on a live address.
 *
 * @param {object} proj     the chosen folder, as withCapture leaves it
 * @param {{mcp: boolean, speaker: boolean, runcap: boolean}} choices  step 2's boxes
 * @returns {{phase, installed, head, rows, cards, footer, sub2, sub3, started, ask, doneHref, inboxHref, previewKnown}}
 *   phase: unread · reading · failed · unknown · none · waiting · recording · problem
 *   rows: seven [label, kind, word, detail, extra?] in the canvas's order
 */
export function stepThree(proj, choices, now) {
  const p = proj || {}
  const c = choices || {}
  const cap = p.capture && p.capture.state ? p.capture : null
  const f = cap && cap.state === 'read' ? cap : null
  const t = now || (cap && cap.now) || new Date()
  const name = p.name || 'this folder'
  const phase = !cap ? 'unread' : cap.state === 'reading' ? 'reading' : cap.state === 'failed' ? 'failed' : f.phase
  const installed = !!f && f.hooks.installed === true
  const H = f ? f.hooks : null
  const R = f ? f.record : null
  const loaded = !!R && R.readable && ((R.sessionsTotal || 0) > 0 || !!R.firstStep)
  const allMissing = !!H && H.commands > 0 && H.missing >= H.commands
  const recordersAll = !!H && H.recorders > 0 && H.recordersMissing === H.recorders
  const hookTag = H && H.version ? ' (hook v' + H.version + ')' : ''
  const firstAt = R ? R.firstAt : null

  /* ---- the seven rows ---- */
  let hooks
  if (!f) {
    hooks = phase === 'reading' ? ['wait', 'Checking', 'Reading the Claude Code settings this folder uses']
      : phase === 'failed' ? ['fog', 'Not checked', 'The check didn’t answer; see above']
        : ['fog', 'Not checked yet', 'ThruLine hasn’t checked this folder’s hooks yet. In a terminal there, thruline doctor says whether they are installed']
  } else if (H.installed === null) hooks = ['fog', 'Can’t tell', detail(H.installedReason || 'a settings file Claude Code reads for this folder could not be read')]
  else if (H.installed === false) hooks = ['fog', 'Not installed', 'None of the Claude Code settings this folder reads holds a ThruLine recorder']
  else if (H.alive === false) {
    hooks = ['fog', 'Can’t run', allMissing ? 'Installed, but they point at a ThruLine program that isn’t there any more'
      : 'Installed, but ' + fmtCount(H.missing) + ' of ' + fmtCount(H.commands) + ' point at a ThruLine program that isn’t there any more']
  } else {
    hooks = ['ok', 'Installed', whereWords(f, p.path) + (H.alive === null && H.aliveReason ? '. Whether each one can run is not known: ' + homeless(H.aliveReason) : '')]
  }

  let first
  if (!f) first = phase === 'reading' ? ['wait', 'Checking', 'Reading the record'] : ['fog', 'Not checked yet', 'Read from the record once this folder is checked']
  else if (!R.readable) first = ['fog', 'Can’t tell', detail(R.reason || 'the record could not be read')]
  else if ((R.deliveries || 0) > 0) {
    const w = when(firstAt, t)
    const latest = atWhen(R.latestAt, t)
    first = ['ok', 'Received' + (w ? ' · ' + w : ''), 'Claude Code' + hookTag + ' · ' + plural(R.deliveries, 'hook event', 'hook events') + ' recorded' + (latest ? ', the latest ' + latest : '')]
  } else if (H.installed === false) first = ['off', 'After install', 'Needs the hooks first']
  else if (H.installed === null) first = ['fog', 'Can’t tell', 'Whether the hooks are installed is not known yet']
  else if (H.alive === false) first = ['fog', 'Nothing yet', 'No step has reached ThruLine through the hooks from this folder']
  else first = ['wait', 'Waiting', 'Start Claude Code in this folder; the first step shows up here when it is recorded']

  const root = f && f.project && f.project.root ? f.project.root.replace(/\/+$/, '') : null
  const top = p.gitTop ? String(p.gitTop).replace(/\/+$/, '') : null
  const git = p.git === true ? ['ok', 'Found', top && root && top !== root
    ? 'Inside the Git repository at ' + homePath(top) + ': each version is fingerprinted, unsaved changes included'
    : 'A Git repository: each version is fingerprinted, unsaved changes included']
    : p.git === false ? ['fog', 'Not found', 'No Git repository, so versions can’t be told apart; what rests on one reads as Fog']
      : p.gitReason ? ['fog', 'No answer', detail(p.gitReason)]
        : ['fog', 'Not checked yet', 'ThruLine hasn’t checked this folder for a Git repository yet']

  /* what the capture route does not read. The install route's plan, when it has been read
     (install-view.js installRows), says what is in place for each; without it, the person's step-2
     choices until the hooks are in, then "Not checked yet" */
  const IR = installRows(p, c)
  const mcp = IR ? IR.mcp : installed ? ['fog', 'Not checked yet', 'ThruLine doesn’t check the MCP server from here']
    : c.mcp ? ['chosen', 'Chosen', 'The agent can check its context and hand in a “done” to verify'] : ['off', 'Off · optional', 'Left out in step 2; add it any time from Settings']
  const speaker = IR ? IR.speaker : installed ? ['fog', 'Not checked yet', 'ThruLine doesn’t check the speaker hooks from here']
    : c.speaker ? ['chosen', 'Chosen', 'Up to 3 lines when a session starts, each ending at a step'] : ['off', 'Off', 'You didn’t say yes in step 2']
  const run = IR ? IR.run : installed ? ['fog', 'Not checked yet', 'ThruLine doesn’t check run capture from here']
    : c.runcap ? ['chosen', 'Chosen', 'Test runs recorded as runs, with exit code and version'] : ['off', 'Off · optional', 'Test runs are seen only as text in the transcript', { turnOn: true }]

  const rows = [
    ['Claude Code hooks'].concat(hooks),
    ['First step received'].concat(first),
    ['Git'].concat(git),
    ['MCP server'].concat(mcp),
    ['Speaker hooks'].concat(speaker),
    ['Run capture'].concat(run),
    ['Stays on this machine', 'ok', '127.0.0.1 only', 'No network in the core; stores readable only by you'],
  ]

  /* ---- the headline ---- */
  const unknownWhy = f && f.reason ? f.reason : 'what Claude Code reads for this folder could not be read'
  const hooksUnread = !!H && H.installed === null
  const head = {
    unread: { title: 'ThruLine can’t tell from here whether ' + name + ' is set up to record', sub: 'ThruLine hasn’t read this folder’s Claude Code settings yet. In a terminal in that folder, thruline doctor says whether its hooks are installed, and names what to fix.', kind: 'fog', word: 'Not checked yet' },
    reading: { title: 'Checking whether ' + name + ' is set up to record', sub: 'ThruLine is reading the Claude Code settings this folder uses, and its record. Nothing is changed.', kind: 'wait', word: 'Checking' },
    failed: { title: 'The check of ' + name + ' didn’t answer', sub: sentence(cap && cap.error ? cap.error : 'the check did not answer') + ' Nothing was changed.', kind: 'fog', word: 'Not checked' },
    unknown: { title: 'ThruLine can’t tell whether ' + name + ' is set up to record', sub: sentence(unknownWhy) + (hooksUnread ? ' The command below names what to fix.' : ''), kind: 'fog', word: 'Can’t tell' },
    none: { title: 'Nothing is installed in ' + name + ' yet', sub: 'Step 2 lists everything ThruLine would add. This check runs once you install.', kind: 'fog', word: 'Not installed' },
    waiting: {
      title: 'Installed. Start Claude Code in ' + name + '.',
      sub: loaded
        ? 'No step has reached ThruLine through the hooks yet. What is recorded here so far was read from the agents’ own recordings. Start Claude Code in this folder; the first step shows up here when it is recorded.'
        : 'Nothing recorded in this project yet. Start Claude Code in this folder; the first step shows up here when it is recorded.',
      kind: 'wait', word: 'No step yet',
    },
    recording: { title: name + ' is recording.', sub: (atWhen(firstAt, t) ? 'The first step arrived ' + atWhen(firstAt, t) + '. ' : '') + 'Every step Claude Code takes in this folder is now recorded on this machine, and nothing leaves it.', kind: 'live', word: 'Recording' },
    problem: {
      title: 'The hooks can’t reach ThruLine.',
      sub: allMissing
        ? 'They were installed, but they point at a ThruLine program that isn’t there any more, so nothing is being recorded. One command finds what to fix.'
        : 'They were installed, but ' + fmtCount(H ? H.missing : 0) + ' of ' + fmtCount(H ? H.commands : 0) + ' point at a ThruLine program that isn’t there any more' + (recordersAll ? ', so nothing is being recorded' : '') + '. One command finds what to fix.',
      kind: 'fog', word: recordersAll ? 'Not recording' : 'Can’t run',
    },
  }[phase]

  /* ---- the card, the footer, the rail ---- */
  const ask = cap && cap.ask ? cap.ask : null
  const askRead = phase === 'recording' && !!ask && ask.state === 'read'
  const cards = {
    none: phase === 'none',
    wait: phase === 'waiting',
    rec: askRead && !!ask.thread,
    recGen: askRead && !ask.thread,
    prob: phase === 'problem' || (phase === 'unknown' && hooksUnread),
  }
  const footer = {
    unread: { btn: false, label: '', act: null, link: false, anyway: false, note: p.id ? '' : 'Choose a folder in step 1 first.' },
    reading: { btn: false, label: '', act: null, link: false, anyway: false, note: 'Reading…' },
    failed: { btn: true, label: 'Look again', act: 'lookAgain', link: false, anyway: false, note: 'Look again asks ThruLine once more.' },
    unknown: { btn: true, label: 'Look again', act: 'lookAgain', link: false, anyway: false, note: cards.prob ? 'Run the command above first, then look again.' : 'Look again asks ThruLine once more.' },
    none: { btn: true, label: 'Go to step 2', act: 'step2', link: false, anyway: false, note: 'Step 2 lists what ThruLine would add; Install for this project adds it.' },
    waiting: { btn: true, label: 'Look again', act: 'lookAgain', link: false, anyway: true, note: loaded ? 'The Inbox already holds what was read from the recordings.' : 'The Inbox stays empty until the first step is recorded.' },
    recording: { btn: false, label: '', act: null, link: true, anyway: false, note: 'It opens on what needs a look first.' },
    problem: { btn: true, label: 'Look again', act: 'lookAgain', link: false, anyway: false, note: 'Run the command above first, then look again.' },
  }[phase]
  const sub3 = {
    unread: 'Not checked yet', reading: 'Checking…', failed: 'The check didn’t answer', unknown: 'Can’t tell from here',
    none: 'Runs after you install', waiting: 'Waiting for the first step',
    recording: 'Recording' + (when(firstAt, t) ? ' · since ' + when(firstAt, t) : ''), problem: 'The hooks can’t run',
  }[phase]

  return {
    phase,
    installed,
    head,
    rows,
    cards,
    footer,
    sub2: installed ? installedSub(p, true) : 'Read it, choose, then install',
    sub3,
    started: phase === 'recording' ? 'First step received' + (when(firstAt, t) ? ' ' + when(firstAt, t) : '') + ' · Claude Code' + hookTag : '',
    ask: cards.rec ? {
      step: typeof ask.step === 'number' ? '· step ' + ask.step : '',
      text: ask.text ? '“' + clip(ask.text, 160) + '”' : '',
      change: ask.title || '',
    } : null,
    doneHref: cards.rec ? href('done', { change: ask.thread, project: p.id }) : undefined,
    inboxHref: p.id ? href('inbox', { project: p.id }) : undefined,
    // the step-2 preview may say "nothing is installed" only when the record says so
    previewKnown: phase === 'none',
  }
}
