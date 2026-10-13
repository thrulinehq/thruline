/* Synthetic `GET /api/hq/projects/<id>/install` documents and POST answers, in
 * the route's wire shape (docs/hq-next/API.md, crates/cli/tests/
 * hq_install_route.rs), for tests only. No real settings file, no real home. */
import { ROOT } from './capture-fixture.js'

export const PLAN_ID = 'a'.repeat(64)
export const NEXT_PLAN_ID = 'b'.repeat(64)
export const HOOKS_FILE = '.claude/settings.local.json'
export const MCP_FILE = '.mcp.json'
export const SENSOR_EVENTS = ['SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'Stop', 'SessionEnd', 'PreCompact', 'StopFailure', 'Notification', 'SubagentStop']
export const SPEAKERS = ['SessionStart brief', 'Stop audit', 'SessionEnd digest']
export const PATH_LINE = 'export PATH="$HOME/.thruline/shim:$PATH"'

const part = (state, file, level, adds, where = [], refusal = null) => ({
  state, file, level,
  adds: state === 'will_add' ? adds : [],
  where: state === 'installed' ? where : [],
  refusal: state === 'refused' ? refusal : null,
})

/**
 * A plan. Each part is 'will_add' (default), 'installed' or 'refused'.
 * @param {object} o
 */
export function plan(o = {}) {
  const id = o.id || 'p1'
  const st = Object.assign({ hooks: 'will_add', speaker: 'will_add', mcp: 'will_add', run: 'will_add' }, o.states || {})
  const where = Object.assign({ hooks: ['project-local'], speaker: ['project-local'], mcp: ['project'], run: ['thruline-home'] }, o.where || {})
  const refusal = Object.assign({
    hooks: { code: 'another_recorder', plain: 'Another ThruLine recorder is already in the user Claude Code settings this folder reads. Two would record every step twice, so nothing was added. In a terminal there, thruline doctor names it and how to remove it.' },
    speaker: { code: 'split', plain: 'Some speaker hooks are already in the user Claude Code settings, and adding the rest here would split them across two files. Nothing was added.' },
    mcp: { code: 'name_taken', plain: 'A server named thruline is already in .mcp.json, and ThruLine did not write it, so it was left alone.' },
    run: { code: 'occupied', plain: 'The shim folder in ThruLine’s home holds files ThruLine did not write (notes.txt), so it was left alone. Nothing was added.' },
  }, o.refusal || {})
  const run = part(st.run, '<ThruLine home>/shim', 'thruline-home', ['pytest', 'cargo', 'go', 'npm', 'make'], where.run, refusal.run)
  run.wrappers = st.run === 'installed' ? (o.wrappers || ['pytest', 'make']) : []
  run.candidates = ['pytest', 'cargo', 'go', 'npm', 'make']
  run.path_line = PATH_LINE
  run.on_path = st.run === 'installed' ? (o.onPath === true) : null
  const inProject = (k) => st[k] === 'installed' && where[k].some((w) => w === 'project' || w === 'project-local')
  return {
    schema_version: 1,
    project: { id, root: ROOT },
    plan: o.plan || PLAN_ID,
    parts: {
      hooks: part(st.hooks, HOOKS_FILE, 'project-local', SENSOR_EVENTS, where.hooks, refusal.hooks),
      speaker: part(st.speaker, HOOKS_FILE, 'project-local', SPEAKERS, where.speaker, refusal.speaker),
      mcp: part(st.mcp, MCP_FILE, 'project', ['mcpServers.thruline'], where.mcp, refusal.mcp),
      run_capture: run,
    },
    undo: {
      project: inProject('hooks') || inProject('speaker') || inProject('mcp') ? 'thruline init --undo --settings .claude/settings.local.json --claude-json .mcp.json' : null,
      user: where.hooks.includes('user') && st.hooks === 'installed' ? 'thruline init --undo' : null,
      shim: st.run === 'installed' ? 'thruline shim uninstall' : null,
    },
    masking: { masked: 0, families: [], complete: true },
  }
}

/** A POST answer: each part's outcome, and the plan read after writing. */
export function answer(outcomes, after, plain) {
  const o = (k, file) => {
    const x = outcomes[k] || 'not_chosen'
    return { outcome: x, file, entries: x === 'wrote' ? ['…'] : [], where: x === 'already' ? ['project-local'] : [], refusal: x === 'refused' ? { code: 'symlink', plain: '.mcp.json is a link to somewhere else, so ThruLine left it alone. Nothing was added.' } : null }
  }
  const parts = { hooks: o('hooks', HOOKS_FILE), speaker: o('speaker', HOOKS_FILE), mcp: o('mcp', MCP_FILE), run_capture: o('run', '<ThruLine home>/shim') }
  const changed = Object.values(parts).some((p) => p.outcome === 'wrote')
  return { schema_version: 1, project: { id: 'p1', root: ROOT }, changed, parts, plain: plain || (changed ? 'Wrote .claude/settings.local.json.' : 'Nothing changed: every part you chose was already there.'), after }
}
