/* A small ThreadView (`GET /api/hq/projects/<p>/threads/<t>`, docs/hq-next/API.md),
 * test-only: one Claude Code session that edits two files, runs the tests once,
 * says "Tests pass.", then edits again — so the claim's proof stops at ⑤.
 *
 * The second edit is read from the request (no line numbers) and adds 300
 * lines, so a test can prove every line reaches the screen. A third edit is a
 * Codex patch (no line numbers, a header). Nothing here is the founder's data.
 */
const T = (m) => `2026-10-07T10:${String(m).padStart(2, '0')}:00.000Z`
const V1 = 'wt-1111111111111111'
const V2 = 'wt-2222222222222222'

export function threadFixture() {
  const many = Array.from({ length: 300 }, (_, i) => ({ kind: 'added', text: `line ${i + 1} of the new file` }))
  return {
    schema_version: 1,
    project: { id: 'p1', name: 'shop', root: '/repo' },
    thread: {
      id: 't1', title: 'Fix the checkout total', current_version_id: V2, whose_move: 'you',
      proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: ['/repo/src/api.py'] },
    },
    prompts: [{ event_id: 'p1', session_id: 's1', seq: 1, ts: T(0), text: 'Fix the checkout total', work: true, version_id: V1, raw_file: '/x.jsonl', raw_line: 1 }],
    edits: [
      { id: 'e1', event_id: 'e1', session_id: 's1', ts: T(1), file: '/repo/src/totals.py', op: 'edit', action: null, moved_to: null,
        hunks: [{ old_start: 10, old_lines: 3, new_start: 10, new_lines: 4, header: 'def total(cart):', lines: [
          { kind: 'context', text: '    s = sum(cart)' }, { kind: 'removed', text: '    return s - d' }, { kind: 'added', text: '    d = min(d, s)' }, { kind: 'added', text: '    return s - d' }, { kind: 'context', text: '' }] }],
        lines_added: 2, lines_removed: 1, ok: true, source: 'structured_patch', via: 'tool', version_id: V1, raw_file: '/x.jsonl', raw_line: 2 },
      { id: 'e2', event_id: 'e2', session_id: 's1', ts: T(5), file: '/repo/src/api.py', op: 'write', action: 'create', moved_to: null,
        hunks: [{ old_start: null, old_lines: 0, new_start: null, new_lines: 300, header: null, lines: many }],
        lines_added: 300, lines_removed: 0, ok: true, source: 'request', via: 'tool', version_id: V2, raw_file: '/x.jsonl', raw_line: 9 },
      { id: 'e3#0', event_id: 'e3', session_id: 's1', ts: T(6), file: '/repo/src/util.py', op: 'patch', action: 'update', moved_to: null,
        hunks: [{ old_start: null, old_lines: 1, new_start: null, new_lines: 1, header: 'def helper():', lines: [{ kind: 'removed', text: '    return 1' }, { kind: 'added', text: '    return 2' }] }],
        lines_added: 1, lines_removed: 1, ok: true, source: 'apply_patch', via: 'codex_patch', version_id: V2, raw_file: '/x.jsonl', raw_line: 10 },
    ],
    commands: [
      { id: 'c1', event_id: 'c1', part: 0, session_id: 's1', ts: T(2), command: 'pytest -q tests/test_totals.py', readable: true, outcome: 'succeeded', exit_code: 0,
        output_excerpt: ['4 passed in 0.1s'], output_lines_total: 1, output_scope: 'command', is_test: true, is_commit: false,
        tally: { passed: 4, failed: 0, skipped: 0, errors: 0, unit: 'tests', runner: 'pytest', lines: 1 }, tally_line: '4 passed in 0.1s', via: 'bash', version_id: V1, raw_file: '/x.jsonl', raw_line: 3 },
      { id: 'c2', event_id: 'c2', part: 0, session_id: 's1', ts: T(3), command: 'ls src', readable: true, outcome: 'succeeded', exit_code: 0,
        output_excerpt: [], output_lines_total: 0, output_scope: 'command', is_test: false, is_commit: false, tally: null, tally_line: null, via: 'bash', version_id: V1, raw_file: '/x.jsonl', raw_line: 4 },
    ],
    claims: [
      { id: 'm1#0', event_id: 'm1', session_id: 's1', ts: T(4), text: 'Tests pass.', kind: 'tests_pass', has_verifier: true, marker: 'completion',
        rests_on_command_id: 'c1', followed_by_passing_test: false, version_id: V1, raw_file: '/x.jsonl', raw_line: 5 },
      { id: 'm1#1', event_id: 'm1', session_id: 's1', ts: T(4), text: 'Checkout total fixed.', kind: 'fixed', has_verifier: false, marker: 'completion',
        rests_on_command_id: 'c1', followed_by_passing_test: false, version_id: V1, raw_file: '/x.jsonl', raw_line: 5 },
    ],
    versions: [
      { id: V1, commit: null, short: null, session_id: 's1', from_seq: 1, to_seq: 4, committed: true, message: 'first', boundary_event_id: 'b1', committed_at: T(4) },
      { id: V2, commit: null, short: null, session_id: 's1', from_seq: 5, to_seq: 9, committed: false, message: null, boundary_event_id: null, committed_at: null },
    ],
    current_version_id: V2,
    sessions: [{ id: 's1', ledger_project_id: 'lp', source: 'claude_code', origin: 'person', started_at: T(0), last_at: T(6), file: '/x.jsonl' }],
    files: [],
    masking: { masked: 0, families: [], complete: true },
  }
}
