/* First run on a live address: every word step 2's Install and step 3's
 * run-capture and undo panels draw, from the install plan the route served
 * (install.js) and the boxes the person ticked (C30 R-ROUTES "Install";
 * census §3.11 rows 5, 6, 10, 11).
 *
 * PURE: no fetch, no window, no clock.
 *
 *   installView(proj, choices, cdLine)
 *     proj     the chosen folder, with `install` folded in (install.js withInstall)
 *     choices  step 2's boxes {mcp, speaker, runcap}; the hooks are always on
 *     cdLine   "cd <the folder>" as the screen writes it; empty when the folder's path is not known yet
 *
 * The rules it keeps (C30 G3, "Nothing in v0.2 may"):
 *  - "Installed" only for a part the plan read in a file; a plan still being
 *    read, or one that could not be read, is said as such.
 *  - The button installs only when something chosen would be added, and says
 *    which file each part writes before the click.
 *  - Never "Not available in this version".
 */

/** The four parts, in the screen's words. */
export const NAMES = { hooks: 'Claude Code hooks', mcp: 'MCP server', speaker: 'speaker hooks', run: 'run capture' }

/** The project-level files the route writes, and the shim's place. */
export const HOOKS_FILE = '.claude/settings.local.json'
export const MCP_FILE = '.mcp.json'
/** The terminal commands that do what Install does, and undo it, in this folder. */
export const INIT_CMD = 'thruline init --settings ' + HOOKS_FILE + ' --claude-json ' + MCP_FILE
export const PLAN_CMD = 'thruline sensor plan --settings ' + HOOKS_FILE
export const UNDO_CMD = 'thruline init --undo --settings ' + HOOKS_FILE + ' --claude-json ' + MCP_FILE

const joinList = (a) => (a.length <= 1 ? a.join('') : a.slice(0, -1).join(', ') + ' and ' + a[a.length - 1])
const cap1 = (t) => (t ? t.charAt(0).toUpperCase() + t.slice(1) : t)
const uniq = (a) => a.filter((x, i) => a.indexOf(x) === i)

/** Where a part already is, in words: "for every folder, from your own Claude Code settings". */
function whereWords(part, path) {
  if (!part) return ''
  const w = part.where || []
  if (w.includes('thruline-home')) return 'in ThruLine’s own folder'
  if (w.includes('user') || w.includes('managed') || w.includes('managed drop-in')) return 'for every folder, from your own Claude Code settings'
  if (w.includes('local')) return 'for ' + path + ' only, from your Claude Code config'
  if (w.length) return 'for ' + path + ' only'
  return ''
}

/** The file a part writes, as a person reads it. */
const fileOf = (key, F) => (key === 'run' ? 'the shim folder in ThruLine’s home' : F[key].file)

/**
 * @returns {{sum, planInst, ch, shim, undo, initCmd, planCmd, turnOn, facts}}
 *   sum      step 2's footer: on, off, installLabel, install|goStep3, disabled, opacity, why
 *   planInst every chosen part is installed, by the plan
 *   ch       "What you've chosen" values and colours
 *   shim     the "Turn on run capture" panel (show, cmd, copy, step2, shape)
 *   undo     "Changed your mind?" (text, cmd)
 *   turnOn   what the run-capture row's "Turn on" does: 'install' or 'choose'
 */
export function installView(proj, choices, cdLine) {
  const p = proj || {}
  const c = choices || {}
  const path = p.pathKnown === false ? 'this folder' : p.path || 'this folder'
  const I = p.install || null
  const F = I && I.state === 'read' ? I.facts : null
  const chosen = ['hooks'].concat(c.mcp ? ['mcp'] : [], c.speaker ? ['speaker'] : [], c.runcap ? ['run'] : [])
  const leftOut = [].concat(c.mcp ? [] : ['mcp'], c.speaker ? [] : ['speaker'], c.runcap ? [] : ['run'])
  const names = (keys) => keys.map((k) => NAMES[k])
  const leftLine = leftOut.length ? 'Left out: ' + joinList(names(leftOut)) + ' · add later in Settings' : 'Nothing left out'
  const result = I && I.result ? I.result : null

  /* ---- step 2's footer ---- */
  let sum
  if (!p.id) {
    sum = { mode: 'off', installLabel: 'Install for this project', on: 'Choose a folder in step 1 first', off: 'Nothing is installed until you press Install for this project.', why: 'Choose a folder in step 1 first.' }
  } else if (!I || I.state === 'reading') {
    sum = { mode: 'off', installLabel: 'Install for this project', on: 'Adds ' + joinList(names(chosen)) + ' · ' + path + ' only', off: 'Reading what is already installed for this folder…', why: 'Reading what is already installed for this folder…' }
  } else if (I.state === 'failed' || !F) {
    const why = I.error || 'ThruLine could not read what Install would add.'
    sum = { mode: 'off', installLabel: 'Install for this project', on: 'Adds ' + joinList(names(chosen)) + ' · ' + path + ' only', off: why, why }
  } else if (I.busy) {
    sum = { mode: 'off', installLabel: 'Installing…', on: 'Adds ' + joinList(names(chosen)) + ' · ' + path + ' only', off: 'Writing the parts you chose…', why: 'Writing the parts you chose…' }
  } else {
    const toAdd = chosen.filter((k) => F[k].state === 'will_add')
    const already = chosen.filter((k) => F[k].state === 'installed')
    const refused = chosen.filter((k) => F[k].state === 'refused')
    const inFolder = toAdd.filter((k) => k !== 'run')
    const refusedLine = refused.length ? 'Can’t add ' + joinList(names(refused)) + ': ' + ((F[refused[0]].refusal && F[refused[0]].refusal.plain) || 'ThruLine left it alone.') : ''
    const resultLine = result && !result.ok ? result.plain : result && result.ok ? result.plain : ''
    if (toAdd.length) {
      const on = inFolder.length
        ? 'Adds ' + joinList(names(inFolder)) + ' · ' + path + ' only' + (toAdd.includes('run') ? ', and run capture in ThruLine’s own folder' : '')
        : 'Adds run capture · in ThruLine’s own folder'
      const here = uniq(inFolder.map((k) => F[k].file).filter(Boolean))
      const bits = (here.length ? [joinList(here) + ' in this folder'] : []).concat(toAdd.includes('run') ? [fileOf('run', F)] : [])
      const why = 'Writes ' + joinList(bits) + '. Nothing else.'
      const offParts = [refusedLine || null, already.length ? 'Already there: ' + joinList(names(already)) : null, leftLine]
      sum = { mode: 'install', installLabel: 'Install for this project', on, off: resultLine || offParts.filter(Boolean).join(' · '), why }
    } else if (F.hooks.state === 'installed') {
      const inProject = already.every((k) => (F[k].where || []).every((w) => w === 'project' || w === 'project-local'))
      const where = whereWords(F.hooks, path)
      sum = {
        mode: 'step3',
        installLabel: 'Installed · go to step 3',
        on: 'Installed: ' + joinList(names(already)) + (inProject ? ' · ' + path + ' only' : ''),
        off: resultLine || refusedLine || (!inProject && where ? cap1(NAMES.hooks) + ' are on ' + where + ' · ' + leftLine : leftLine),
        why: 'Every part you chose is already installed. Go to step 3.',
      }
    } else {
      sum = { mode: 'off', installLabel: 'Install for this project', on: 'Nothing can be added here', off: resultLine || refusedLine, why: refusedLine || 'Nothing can be added here.' }
    }
  }
  const planInst = !!F && chosen.every((k) => F[k].state === 'installed')

  /* ---- "What you've chosen" (step 3's right panel) ---- */
  const chWord = (key, on, yes, no) => {
    if (!F) return { word: on ? yes : no, on }
    const s = F[key].state
    if (on) return s === 'installed' ? { word: 'Yes · installed', on: true } : s === 'refused' ? { word: 'Yes · can’t be added here', on: false } : { word: yes + ' · not installed yet', on: true }
    return s === 'installed' ? { word: 'Installed already', on: true } : { word: no, on: false }
  }
  const ch = {
    mcp: chWord('mcp', !!c.mcp, 'Yes', 'No · left out'),
    speaker: chWord('speaker', !!c.speaker, 'Yes · you said yes', 'No · off'),
    runcap: chWord('run', !!c.runcap, 'Yes', 'No · optional'),
  }

  /* ---- "Turn on run capture" (rows 10, 11) ---- */
  const run = F ? F.run : null
  const shimIn = !!run && run.state === 'installed'
  const shim = {
    show: shimIn && run.onPath !== true && !!run.pathLine,
    cmd: run && run.pathLine ? run.pathLine : 'thruline shim install',
    copy: run && run.pathLine ? run.pathLine : 'thruline shim install',
    step2: 'Add this line to your shell profile, then restart the terminal Claude Code runs in. ThruLine wrote the shim folder; it never edits your shell profile, so this line is yours to add.',
    shape: run && run.wrappers.length ? 'It wraps ' + joinList(run.wrappers) + ' · in ThruLine’s own folder' : 'In ThruLine’s own folder',
  }
  // "Turn on" installs run capture only where the hooks are already in; otherwise it ticks the box for step 2's Install
  const turnOn = !!F && F.hooks.state === 'installed' && !!run && run.state === 'will_add' ? 'install' : 'choose'

  /* ---- "Changed your mind?" ---- */
  const u = F ? F.undo : { project: null, user: null, shim: null }
  const tail = u.shim ? ' && ' + u.shim : ''
  const undo = u.project || (!u.user && !u.shim)
    ? { cmd: (cdLine ? cdLine + ' && ' : '') + (u.project || UNDO_CMD) + tail, text: 'This removes what ThruLine added to this folder’s Claude Code settings. What was already recorded stays until you remove it in Settings, which says what would be lost first.' }
    : u.user
      ? { cmd: u.user + tail, text: 'These hooks are in your own Claude Code settings, for every folder, so this removes them everywhere. What was already recorded stays until you remove it in Settings, which says what would be lost first.' }
      : { cmd: u.shim, text: 'This removes the shim folder from ThruLine’s home. What was already recorded stays until you remove it in Settings, which says what would be lost first.' }

  return { sum, planInst, ch, shim, undo, initCmd: INIT_CMD, planCmd: PLAN_CMD, turnOn, facts: F }
}

/**
 * Step 3's MCP, speaker and run-capture rows from the plan, in the canvas's
 * row shape [kind, word, detail, extra?]; null when the plan is not read.
 */
export function installRows(proj, choices) {
  const p = proj || {}
  const c = choices || {}
  const F = p.install && p.install.state === 'read' ? p.install.facts : null
  if (!F) return null
  const path = p.pathKnown === false ? 'this folder' : p.path || 'this folder'
  const refusedRow = (part) => ['fog', 'Can’t add', (part.refusal && part.refusal.plain ? part.refusal.plain : 'ThruLine left it alone').replace(/[.]$/, '')]
  const mcp = F.mcp.state === 'installed' ? ['ok', 'Installed', 'The agent can check its context and hand in a “done” to verify · ' + (whereWords(F.mcp, path) || 'for ' + path + ' only') + (F.mcp.where.includes('project') ? ', from ' + MCP_FILE : '')]
    : c.mcp ? (F.mcp.state === 'refused' ? refusedRow(F.mcp) : ['chosen', 'Chosen', 'Install adds it to ' + MCP_FILE + ' in this folder'])
      : ['off', 'Off · optional', 'Left out in step 2; add it any time from Settings']
  const speaker = F.speaker.state === 'installed' ? ['ok', 'Installed', 'Up to 3 lines when a session starts, each ending at a step · ' + (whereWords(F.speaker, path) || 'for ' + path + ' only')]
    : c.speaker ? (F.speaker.state === 'refused' ? refusedRow(F.speaker) : ['chosen', 'Chosen', 'Install adds them to ' + HOOKS_FILE + ' in this folder'])
      : ['off', 'Off', 'You didn’t say yes in step 2']
  const r = F.run
  const run = r.state === 'installed'
    ? (r.onPath === true ? ['ok', 'Installed', 'Test runs recorded as runs, with exit code and version · first on the search path HQ started with']
      : ['wait', 'One line to add', 'Test runs recorded as runs, with exit code and version; add the line below to your shell profile'])
    : c.runcap ? (r.state === 'refused' ? refusedRow(r) : ['chosen', 'Chosen', 'Install writes the shim folder in ThruLine’s own folder'])
      : ['off', 'Off · optional', 'Test runs are seen only as text in the transcript', { turnOn: r.state === 'will_add' }]
  return { mcp, speaker, run }
}

/** Step 2's rail line: "Installed · hooks", "Installed · hooks + MCP", "Installed · hooks + 2 more". */
export function installedSub(proj, hooksInstalled) {
  const p = proj || {}
  const F = p.install && p.install.state === 'read' ? p.install.facts : null
  if (!hooksInstalled) return null
  const on = ['hooks'].concat(F && F.mcp.state === 'installed' ? ['MCP'] : [], F && F.speaker.state === 'installed' ? ['speakers'] : [], F && F.run.state === 'installed' ? ['run capture'] : [])
  return on.length > 2 ? 'Installed · hooks + ' + (on.length - 1) + ' more' : 'Installed · ' + on.join(' + ')
}
