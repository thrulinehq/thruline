/* A synthetic served ThreadView (docs/hq-next/API.md) and session timeline for
 * the step view's tests. Never the founder's record; every path, id and word
 * here is made up. Used only by the tests in this folder.
 *
 * The record, by step (a row's place in the thread, ordered by time):
 *   1  prompt   "Fix the checkout total"           00:00
 *   2  edit     /repo/src/totals.py  (V1)          00:01
 *   3  command  pytest -q tests/test_totals.py     00:02   4 passed, exit 0
 *   4  prompt   "Also check the rounding"          00:03   (not the Change's first ask)
 *   5  claim    "Tests pass."                      00:04   rests on step 3
 *   6  claim    "Checkout total fixed."            00:04
 *   7  edit     /repo/src/api.py  (V2)             00:05   only with `moved`
 *   8  command  ls src (not a test, no result)     00:06   only with `moved`
 */
export const TID = '0123456789abcdef'
export const V1 = 'wt-1111111111111111'
export const V2 = 'wt-2222222222222222'
export const HOME_FILE = '/Users/ana/.claude/projects/-repo/s1.jsonl'
const T = (m) => `2026-10-07T10:${String(m).padStart(2, '0')}:00.000Z`
export const NOW = new Date('2026-10-07T10:12:00.000Z')

export function thread({ moved = false, single = false, noRun = false } = {}) {
  const edit = (id, m, file, version, line, extra = {}) => ({
    id, event_id: id, session_id: 's1', ts: T(m), file, op: 'edit', action: null, moved_to: null,
    hunks: [], lines_added: 1, lines_removed: 1, ok: true, source: 'structured_patch', via: 'tool', version_id: version,
    raw_file: HOME_FILE, raw_line: line, ...extra,
  })
  const command = (id, m, text, test, version, line, extra = {}) => ({
    id: id + '#0', event_id: id, part: 0, session_id: 's1', ts: T(m), command: text, readable: true, outcome: 'succeeded', exit_code: 0,
    output_excerpt: [], output_lines_total: 0, output_scope: 'command', is_test: test, is_commit: false,
    tally: null, tally_line: null, via: 'bash', version_id: version, raw_file: HOME_FILE, raw_line: line, ...extra,
  })
  const edits = [edit('e1', 1, '/repo/src/totals.py', V1, 4, {
    hunks: [{ old_start: 10, old_lines: 2, new_start: 10, new_lines: 3, header: 'def total(items)', lines: [
      { kind: 'context', text: '    s = 0' }, { kind: 'removed', text: '    s = s + 1' }, { kind: 'added', text: '    s = s + item.price' }] }],
  })]
  const commands = [command('c1', 2, 'pytest -q tests/test_totals.py', true, V1, 6, {
    exit_code: 0, output_excerpt: ['collected 4 items', '....', '4 passed in 0.2s'], output_lines_total: 12,
    tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, tally_line: '4 passed in 0.2s',
  })]
  const claims = [
    { id: 'm1#0', event_id: 'm1', session_id: 's1', ts: T(4), text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, marker: 'completion', rests_on_command_id: 'c1#0', followed_by_passing_test: false, version_id: V1, raw_file: HOME_FILE, raw_line: 12 },
    { id: 'm1#1', event_id: 'm1', session_id: 's1', ts: T(4), text: 'Checkout total fixed.', kind: 'fixed', has_verifier: false, marker: 'completion', rests_on_command_id: 'c1#0', followed_by_passing_test: false, version_id: V1, raw_file: HOME_FILE, raw_line: 12 },
  ]
  const prompts = [
    { event_id: 'p1', session_id: 's1', seq: 1, ts: T(0), text: 'Fix the checkout total', segments: [{ kind: 'typed', text: 'Fix the checkout total' }], work: true, version_id: V1, raw_file: HOME_FILE, raw_line: 1 },
    { event_id: 'p2', session_id: 's1', seq: 9, ts: T(3), text: 'Also check the rounding in /Users/ana/code/shop/src/totals.py', segments: [{ kind: 'typed', text: 'Also check the rounding in /Users/ana/code/shop/src/totals.py' }], work: true, version_id: V1, raw_file: HOME_FILE, raw_line: 9 },
  ]
  if (moved) {
    edits.push(edit('e2', 5, '/repo/src/api.py', V2, 16))
    commands.push(command('c2', 6, 'ls src', false, V2, 18, { outcome: 'unknown', exit_code: null }))
  }
  if (noRun) {
    commands.length = 0
    claims.forEach((c) => { c.rests_on_command_id = null })
  }
  const current = moved ? V2 : V1
  const onlyOne = single
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'shop', root: '/repo' },
    thread: { id: TID, title: 'Fix the checkout total', current_version_id: current, whose_move: 'you', proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: moved ? ['/repo/src/api.py'] : [] } },
    prompts: onlyOne ? [prompts[0]] : prompts,
    edits: onlyOne ? [] : edits,
    commands: onlyOne ? [] : commands,
    claims: onlyOne ? [] : claims,
    versions: [
      { id: V1, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 8, committed: true, message: 'first', boundary_event_id: 'b1', committed_at: T(4) },
      ...(moved ? [{ id: V2, commit: null, short: null, session_id: 's1', from_seq: 9, to_seq: 14, committed: false, message: null, boundary_event_id: null, committed_at: null }] : []),
    ],
    current_version_id: current,
    sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(8), file: HOME_FILE }],
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}

/** `GET /api/hq/sessions/s1` for the record above (shortened to what the page reads). */
export function timeline() {
  const item = (event_id, seq, m, kind, extra = {}) => ({ event_id, seq, ts: T(m), kind, text: null, tool: null, sidechain: false, raw_file: HOME_FILE, raw_line: seq, ...extra })
  return {
    schema_version: 1, session_id: 's1', ledger_project_id: 'lp', hq_project_id: 'p1', source: 'claude_code', origin: 'person', file: HOME_FILE,
    items: [
      item('p1', 1, 0, 'prompt', { text: 'Fix the checkout total' }),
      item('a0', 2, 0, 'assistant', { text: 'I will look at totals.py first.' }),
      item('e1', 4, 1, 'tool', { tool: { name: 'Edit', source_name: 'Edit', summary: ['src/totals.py (+1 −1)'], outcome: 'succeeded', exit_code: null, output_excerpt: [], output_lines_total: 0 } }),
      item('c1', 6, 2, 'tool', { tool: { name: 'Bash', source_name: 'Bash', summary: ['pytest -q tests/test_totals.py'], outcome: 'succeeded', exit_code: 0, output_excerpt: ['4 passed in 0.2s'], output_lines_total: 1 } }),
      item('t1', 7, 2, 'thinking', { text: 'The run passed, so I can say so.' }),
      item('p2', 9, 3, 'prompt', { text: 'Also check the rounding in /Users/ana/code/shop/src/totals.py' }),
      item('m1', 12, 4, 'assistant', { text: 'Tests pass. Checkout total fixed.' }),
      item('x1', 13, 4, 'summary', { text: 'Fixed the total.' }),
      item('e2', 16, 5, 'tool', { tool: { name: 'Edit', source_name: 'Edit', summary: ['src/api.py (+1 −1)'], outcome: 'succeeded', exit_code: null, output_excerpt: [], output_lines_total: 0 } }),
      item('c2', 18, 6, 'tool', { tool: { name: 'Bash', source_name: 'Bash', summary: ['ls src'], outcome: null, exit_code: null, output_excerpt: [], output_lines_total: 0 } }),
      item('x2', 19, 6, 'assistant', { text: 'Anything else?' }),
    ],
    masking: { masked: 0, families: [], complete: true },
  }
}
