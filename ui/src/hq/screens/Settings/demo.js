/* Settings — the canvas's demo content, as the screen's data contract.
 *
 * Every fact Settings shows (agents, recording, checks, rules, privacy and
 * storage, notifications, appearance) arrives through one object of this
 * shape: `this.props.data` in the product, `DEMO` in the design gallery.
 * Formatting (sizes, sentences built from states and times, glyphs, colours)
 * stays in logic.js `renderVals()`. DATA.md documents each field, and which of
 * them anything in ThruLine backs today.
 *
 * The values below are exactly what the canvas shows. They are illustrative
 * demo records, not a reading of this machine.
 */

export const DEMO = Object.freeze({
  // when HQ last read the record; also the time of any act taken on this page
  readAt: '16:10',
  hq: { stopped: false },

  // the project the per-project settings act on (pause, the speaker, checks)
  project: { name: 'checkout-web', paused: false, speaker: false },

  // where the record comes from
  sources: {
    claudeCode: {
      name: 'Claude Code',
      status: 'observing', // observing | hooks_dead | upgrade_pending
      since: '16:01',
      lastStep: '16:09',
      hookEvents: { count: 10, first: 'SessionStart', last: 'SessionEnd', notSubscribed: ['PermissionRequest', 'Notification'] },
    },
    codex: { name: 'Codex', status: 'not_answering' },
    runCapture: { on: false, tools: { named: ['pytest', 'cargo', 'npm'], more: 8 } },
  },

  // what ThruLine keeps on this machine (L1.1)
  storage: {
    root: '~/.thruline',
    mode: '0700',
    files: ['ledger.db', 'exec.db', 'mirror', 'snapshots'],
    parts: [
      { key: 'checkout', name: 'checkout-web', mb: 612, swatch: 'project',
        what: '4 sessions · 3 signed session records · run output', whatT: '4 sessions · 3 receipts · exec output',
        lose: 'Removing it loses the proof behind every statement in those sessions.',
        stays: [], goes: ['4 sessions', 'the proof behind their statements'],
        restsOn: { change: 'Fix double-applied discount on checkout total', ready: true } },
      { key: 'output', name: 'Run output older than 30 days', mb: 540, swatch: 'output',
        what: 'what 61 runs printed', whatT: 'stdout/stderr from 61 runs',
        lose: 'Exit codes and pass counts stay; the full output text goes.',
        stays: ['exit codes', 'pass counts'], goes: ['full output text'], restsOn: null },
      { key: 'demo', name: 'demo', mb: 188, swatch: 'demo',
        what: '3 example sessions', whatT: '3 fixture sessions · receipts',
        lose: 'Removing it loses the demo records only.',
        stays: ['your own projects'], goes: ['the demo records'], restsOn: null },
      { key: 'snap', name: 'Saved versions of the work', mb: 96, swatch: 'saved_versions',
        what: '212 versions', whatT: 'workspace snapshots · 212 trees',
        lose: '“Ran on an earlier version” can still be said; what changed line by line cannot.',
        stays: ['“ran on an earlier version”'], goes: ['what changed, line by line'], restsOn: null },
    ],
  },

  // what ThruLine can see from each agent: mark is yes | partly | notyet | no
  capabilities: [
    { key: 'prompts', label: 'Your requests',
      cc: { mark: 'yes', reason: 'Each request is recorded word for word, the moment you send it.', tech: 'UserPromptSubmit' },
      cx: { mark: 'yes', reason: 'Each request is recorded from Codex’s own hook when you send it.', tech: 'UserPromptSubmit' } },
    { key: 'tools', label: 'Tool calls',
      cc: { mark: 'yes', reason: 'Every tool the agent calls and what came back, including calls that failed.', tech: 'PreToolUse · PostToolUse · PostToolUseFailure' },
      cx: { mark: 'yes', reason: 'Every tool call and its result, from Codex’s own hooks.', tech: 'PreToolUse · PostToolUse' } },
    { key: 'commands', label: 'Commands',
      cc: { mark: 'yes', reason: 'Each shell command the agent runs, exactly as it ran it.', tech: 'Bash tool calls' },
      cx: { mark: 'yes', reason: 'Each command Codex runs, with the folder it ran in.', tech: 'CommandReport · argv · cwd' } },
    { key: 'exit', label: 'Exit codes',
      cc: { mark: 'partly', reason: 'Seen as text inside the tool result. With run capture on, a test run is recorded as a run, with its exit code.', tech: 'exec recorder via the PATH shim · off' },
      cx: { mark: 'partly', reason: 'Codex reports the exit code itself; ThruLine keeps it as Codex’s report, not as a run it saw. Run capture changes that.', tech: 'CommandReport.exit_code · harness-reported' } },
    { key: 'edits', label: 'File edits',
      cc: { mark: 'yes', reason: 'Each edit names its file, and the version after it is recorded.', tech: 'fs_change · workspace identity after the edit' },
      cx: { mark: 'yes', reason: 'Each patch Codex applied. A patch that failed is recorded, and not counted as an edit.', tech: 'patch_apply_end → fs_change' } },
    { key: 'perm', label: 'Permission prompts',
      cc: { mark: 'notyet', reason: 'Claude Code’s permission and notification hooks aren’t subscribed, so a waiting prompt isn’t in the record. ThruLine can’t answer one for you either.', tech: 'PermissionRequest · Notification: not subscribed' },
      cx: { mark: 'no', reason: 'Left out on purpose: a permission prompt records what the agent asked to do, not what it did.', tech: 'PermissionRequest · SubagentStart: unsubscribed (9 of 11 events)' } },
    { key: 'transcripts', label: 'Transcripts',
      cc: { mark: 'yes', reason: 'Sealed into ThruLine’s own copy before it is read, so it outlives Claude Code deleting its copy after 30 days.', tech: 'byte mirror · sealed before parse' },
      cx: { mark: 'yes', reason: 'Codex’s session files are sealed before they are read, and survive being moved or deleted.', tech: 'rollout files · sealed before parse' } },
  ],
  unsupportedAgents: [{ name: 'Gemini', reason: 'no adapter exists' }],

  // paths never recorded
  exclusions: [
    { key: 'env', path: '.env  .env.*', note: 'environment files', on: true },
    { key: 'secrets', path: 'secrets/', note: 'the whole folder', on: true },
    { key: 'gen', path: 'generated files', note: 'files your repo marks as generated', on: false },
  ],

  // who allowed checks, per project; allowed: true | false | null (not asked yet)
  checkPermissions: [
    { project: 'checkout-web', allowed: true, at: { time: '16:09', day: 'today' } },
    { project: 'demo', allowed: null, hasRecords: true },
    { project: 'notes-cli', allowed: null, hasRecords: false },
  ],
  checkWhere: { project: 'checkout-web', folder: '~/code/checkout-web', toolsFound: ['pytest'] },

  // rules
  criteriaTemplates: [
    { key: 't1', text: 'Tests for every touched file pass on the final version.', offered: false },
    { key: 't2', text: 'No file outside the declared folders changed.', offered: false },
  ],
  policy: {
    path: '.thruline/policy.yml',
    text: 'require_receipt: true\nclaims:\n  tests_pass:\n    require_reexecuted: true\nstop_gate:\n  enabled: false',
    wouldNotBeReady: { changes: 2, ofLast: 10 },
    example: { name: 'the discount fix' },
  },

  // privacy
  masking: { detector: 'v2', masked: 3 },
  egress: { localModel: false },

  // remembered on this machine, for this person
  prefs: {
    face: 'plain', maskedCopyOnExport: true, colourVision: false, density: 'comfortable', motion: 'system',
    notifications: { ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }, weeklyDigest: false, keys: 'on',
  },

  // a sample for the colour preview: one proven statement's version and scope
  statePreview: { version: 'e71c5b0', scope: ['tests/test_totals.py', 'checkout/'] },

  // a pending upgrade's time estimate (upgrade::estimate_phrase); null until served
  upgrade: { estimate: null },

  // DESIGN ONLY: the canvas's notes to its reader, drawn in the gallery and never on the record
  designNotes: {
    codex: 'Needs engine: no Codex app server client exists yet.',
    caps: 'Needs engine: this table isn’t served to the page yet; these cells are drawn from the capture code. Once served, each cell comes from the adapter capability tests and is no stronger than a test proves.',
    pause: 'Needs engine for Claude Code’s hooks; pausing Codex observation already works.',
    excl: 'Each one is listed under “What was not recorded” on any Change it touches. Needs engine.',
    checks: 'Built on the run lane (P6-5, P6-5B), not merged yet. Checking is never paid.',
    policy: 'Illustrative values. The file’s schema is real and closed; reusable lines and the live preview need engine.',
    storage: 'Served today: the size by part. Sizes by project and Remove need engine (L1.1); this version of ThruLine removes nothing.',
    notif: 'Needs engine for system notifications; nothing in ThruLine sends them yet.',
  },

  // DESIGN ONLY: what the canvas's DEMO STATE switcher swaps in over the above
  variants: {
    dead: { sources: { claudeCode: { status: 'hooks_dead' } } },
    upgrade: { sources: { claudeCode: { status: 'upgrade_pending' } } },
    stopped: { hq: { stopped: true } },
  },
})
