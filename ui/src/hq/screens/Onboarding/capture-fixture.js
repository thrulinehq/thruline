/* Synthetic `GET /api/hq/projects/<id>/capture` documents, in the route's wire
 * shape (docs/hq-next/API.md, crates/cli/tests/hq_capture_health.rs), for
 * tests only. No real record, no real settings file: every path names a
 * made-up home folder (/Users/someone) so a test can prove none of it is drawn.
 *
 * Times are local (new Date(y, m, d, h, mm)), so a test reads the same in
 * every time zone: NOW is 7 Oct 2026, 16:10 on the machine running it. */

export const NOW = new Date(2026, 9, 7, 16, 10)
export const at = (h, m, day = 7) => new Date(2026, 9, day, h, m).toISOString()

export const HOME = '/Users/someone'
export const ROOT = HOME + '/code/shop'
export const EVENTS = ['PostToolUse', 'PreCompact', 'PreToolUse', 'SessionEnd', 'SessionStart', 'Stop', 'StopFailure', 'UserPromptSubmit']

const recorder = (event, over) => Object.assign({
  event, command: '/opt/homebrew/bin/thruline sensor append ' + event, kind: 'recorder',
  program_exists: true, missing_program: null, this_program: true,
}, over)

const absent = (scope, path) => ({ scope, path, state: 'absent', reason: null, commands: [] })

/** Installed at the user level, every program on the machine, nothing recorded yet. */
export function waiting(id = 'p1') {
  return {
    schema_version: 1,
    checked_at: '2026-10-07T14:44:30Z',
    project: { id, root: ROOT },
    claude_code_hooks: {
      installed: true, installed_reason: null,
      alive: true, alive_reason: null,
      events: EVENTS.slice(),
      missing_programs: 0,
      version: '0.1.0', version_reason: null,
      levels: [
        { scope: 'user', path: HOME + '/.claude/settings.json', state: 'read', reason: null, commands: EVENTS.map((e) => recorder(e)) },
        absent('project', ROOT + '/.claude/settings.json'),
        absent('project-local', ROOT + '/.claude/settings.local.json'),
        absent('managed', '/Library/Application Support/ClaudeCode/managed-settings.json'),
      ],
    },
    codex: { configured: false, state: null, paused: null, watches_this_project: null, reason: 'HQ was not started with an observation source' },
    git: { repository: true, toplevel: ROOT, reason: null },
    record: {
      sessions: { claude_code: 0, codex: 0 },
      first_step: null,
      latest_step: null,
      through_hooks: { deliveries: 0, first_at: null, latest_at: null, latest_event: null },
      reason: null,
    },
    masking: { masked: 0, families: [], complete: true },
  }
}

/** The same, after the hooks delivered: the first at 16:01, the latest at 16:08. */
export function recording(id = 'p1') {
  const d = waiting(id)
  d.record = {
    sessions: { claude_code: 1, codex: 0 },
    first_step: { at: at(16, 1), agent: 'claude_code' },
    latest_step: { at: at(16, 8), agent: 'claude_code' },
    through_hooks: { deliveries: 1204, first_at: at(16, 1), latest_at: at(16, 8), latest_event: 'PostToolUse' },
    reason: null,
  }
  return d
}

/** Nothing installed anywhere Claude Code reads for this folder. */
export function none(id = 'p1') {
  const d = waiting(id)
  d.claude_code_hooks = {
    installed: false, installed_reason: null,
    alive: null, alive_reason: 'no ThruLine recorder is installed',
    events: [], missing_programs: 0,
    version: null, version_reason: 'no ThruLine recorder is installed',
    levels: [
      absent('user', HOME + '/.claude/settings.json'),
      absent('project', ROOT + '/.claude/settings.json'),
      absent('project-local', ROOT + '/.claude/settings.local.json'),
      absent('managed', '/Library/Application Support/ClaudeCode/managed-settings.json'),
    ],
  }
  return d
}

/** Installed, and every hook names a ThruLine program that is gone. */
export function problem(id = 'p1') {
  const d = waiting(id)
  const gone = HOME + '/old/thruline'
  d.claude_code_hooks = {
    installed: true, installed_reason: null,
    alive: false, alive_reason: '1 of 1 ThruLine hook commands name a program that is not on this machine, a recorder among them',
    events: ['Stop'], missing_programs: 1,
    version: null, version_reason: 'no recorder names a program that is on this machine',
    levels: [
      absent('user', HOME + '/.claude/settings.json'),
      { scope: 'project', path: ROOT + '/.claude/settings.json', state: 'read', reason: null,
        commands: [recorder('Stop', { command: gone + ' sensor append Stop', program_exists: false, missing_program: gone, this_program: null })] },
      absent('project-local', ROOT + '/.claude/settings.local.json'),
      absent('managed', '/Library/Application Support/ClaudeCode/managed-settings.json'),
    ],
  }
  return d
}

/** Installed and recording, but one other ThruLine hook (a speaker) names a program that is gone. */
export function partlyBroken(id = 'p1') {
  const d = waiting(id)
  const gone = HOME + '/old/thruline'
  d.claude_code_hooks.alive = false
  d.claude_code_hooks.alive_reason = '1 of 9 ThruLine hook commands name a program that is not on this machine'
  d.claude_code_hooks.missing_programs = 1
  d.claude_code_hooks.levels[0].commands.push({ event: 'SessionStart', command: gone + ' brief --hook session-start', kind: 'other', program_exists: false, missing_program: gone, this_program: null })
  return d
}

/** A settings file could not be read, and none that was read holds a recorder. */
export function unreadable(id = 'p1') {
  const d = none(id)
  const why = 'Permission denied (os error 13)'
  d.claude_code_hooks.installed = null
  d.claude_code_hooks.installed_reason = 'a settings file Claude Code reads for this project could not be read (user: ' + why + ')'
  d.claude_code_hooks.alive_reason = d.claude_code_hooks.installed_reason
  d.claude_code_hooks.levels[0] = { scope: 'user', path: HOME + '/.claude/settings.json', state: 'unreadable', reason: why, commands: [] }
  return d
}

/** Installed and alive, and the ledger could not be read. */
export function ledgerUnreadable(id = 'p1') {
  const d = waiting(id)
  d.record = { sessions: null, first_step: null, latest_step: null, through_hooks: null, reason: 'the record could not be read: database is locked' }
  return d
}

/** Steps read from the recordings (a load), none through the hooks yet. */
export function loadedNotHooked(id = 'p1') {
  const d = waiting(id)
  d.record.sessions = { claude_code: 3, codex: 2 }
  d.record.first_step = { at: at(9, 0, 1), agent: 'codex' }
  d.record.latest_step = { at: at(15, 0), agent: 'claude_code' }
  return d
}

/** A copy of `doc` with a different git answer. */
export function withGit(doc, git) {
  return Object.assign({}, doc, { git })
}
