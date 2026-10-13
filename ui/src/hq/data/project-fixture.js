/* A synthetic project in the engine's wire shape (docs/hq-next/API.md), for
 * tests only: one thread of each kind the Inbox files. No real record. */
export const NOW = new Date(2026, 9, 7, 16, 10) // local 7 Oct 2026, 16:10
export const at = (h, m, day = 7) => new Date(2026, 9, day, h, m).toISOString()
const tests0 = { total: 0, succeeded: 0, failed: 0, unknown: 0 }
const thread = (id, over) => Object.assign({
  id, title: `Thread ${id}`, title_basis: 'instruction', started_at: at(15, 0), last_activity_at: at(15, 30),
  sessions: 1, prompts: 1, conversation: 0, edits: 0, files: 0, top_files: [], commands: tests0, tests: tests0, commits: 0, claims: 1,
  whose_move: 'done', whose_move_basis: 'nothing_open',
  proof: { last_test: null, last_passing_test_at: null, edited_after_last_pass: [] }, current_version_id: '3f9a1c2deadbeef',
}, over)

export const PROJECT = {
  id: 'p1', name: 'shop', root: '/Users/me/code/shop', threads: 6, last_activity: at(16, 5),
  sessions: 9, prompts: 12, edits: 30, commands: { total: 40, succeeded: 30, failed: 2, unknown: 8 }, tests: { total: 5, succeeded: 4, failed: 1, unknown: 0 }, claims: 7,
  sources: ['claude_code', 'codex'], whose_move: { you: 3, agent: 1, done: 2 },
}
export const VIEW = {
  schema_version: 1,
  project: PROJECT,
  threads: [
    thread('live1', { whose_move: 'agent', whose_move_basis: 'recent_activity', last_activity_at: at(16, 5), started_at: at(16, 1) }),
    thread('re1', { title: 'Fix the “total” — verbatim, masked', whose_move: 'you', whose_move_basis: 'claim_without_passing_test',
      proof: { last_test: { outcome: 'succeeded' }, last_passing_test_at: at(15, 0), edited_after_last_pass: ['/Users/me/code/shop/checkout/api.py', '/Users/me/code/shop/b.py'] } }),
    thread('np1', { whose_move: 'you', whose_move_basis: 'claim_without_passing_test', current_version_id: 'wt-00bb958a3c8b2ae7' }),
    thread('ask1', { whose_move: 'you', whose_move_basis: 'agent_asked', last_activity_at: at(16, 4) }),
    thread('done1', { last_activity_at: at(9, 0, 6) }),
    thread('done2', { last_activity_at: at(9, 0, 1) }),
  ],
}
export const LIST = { schema_version: 1, projects: [PROJECT, { ...PROJECT, id: 'p0', name: 'old', last_activity: at(9, 0, 1), whose_move: { you: 0, agent: 0, done: 1 }, threads: 1 }] }


/** A quick row (API.md: `GET /api/hq/projects` before a project's index is
 *  built): every index-only count is null, never 0. */
export const QUICK = {
  complete: false, id: 'p9', name: 'notes', root: '/Users/me/code/notes', sources: ['codex'], members: [],
  sessions: 4, events: 1200, first_activity: at(8, 0, 1), last_activity: at(9, 30, 1),
  prompts: null, conversation_prompts: null, edits: null, commands: null, tests: null, claims: null, threads: null,
  conversation_only: null, unattached_turns: null, copied_rows_skipped: null, whose_move: null,
}
/** The same list while one row is still quick. */
export const LIST_QUICK = { schema_version: 1, complete: false, projects_complete: 1, projects: [{ ...PROJECT, complete: true }, QUICK] }
