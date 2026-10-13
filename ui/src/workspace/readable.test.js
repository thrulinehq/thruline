/* Unit tests for THE HUMANISER (DESIGN-readable-trace.md §1, acceptance §6.1
 * T1–T7).
 *
 * Every fixture below is a REAL shape. Tool names, argument key sets, the
 * 2,000-character capture cap and the 50-character transport cut are the ones
 * `ground-corpus.md` §C and `research-text.md` §0.2/§B measured over
 * ~/.thruline/exec.db (239,245 events, 44,107 sessions). Nothing here is an
 * imagined payload: `Glob` has zero calls in that store and is therefore
 * asserted to reach the fallback rather than a renderer written against
 * imagination, and `Mark chapter` is asserted with the space its one recorded
 * call actually has in its name.
 */

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { bannedIn } from '../lib/voice.js'
import { titleLines } from './logic.js'
import {
  ADV_BODY,
  ADV_MONO,
  CARD_CW,
  ELLIPSIS,
  TITLE_BUDGET_BODY,
  TITLE_BUDGET_MONO,
  TITLE_LINES,
  composeLine,
  countArrayItems,
  extractString,
  readableEntry,
  readableLine,
  readableText,
  shortHash,
  shortenPath,
  shortenUrl,
  topLevelKeys,
  truncateWords,
  viewFromEntry,
} from './readable.js'

/* ── helpers ───────────────────────────────────────────────────────────────── */

/** The timeline entry `explain` emits for a tool call: `"{tool} {input}"`,
 *  where the input is `truncate(input_redacted, 50)` — first line, masked, cut,
 *  and `…` appended when characters were removed
 *  (`crates/cli/src/explain_cmd.rs:435`, `:502-509`). */
const callEntry = (seq, tool, input, cut = 50) => {
  const one = String(input).split('\n')[0]
  const body = Array.from(one)
  const kept = body.slice(0, cut).join('')
  return { seq, kind: 'tool_call', text: tool + ' ' + kept + (kept.length < body.length ? ELLIPSIS : '') }
}

/** The same entry with the whole input surviving — the Stage A fidelity, which
 *  must not change the phrasing, only how much of the value it carries. */
const wholeCall = (seq, tool, input) => callEntry(seq, tool, input, Infinity)

const line = (entry) => readableText(entry)

/* ── T1 — the table: one case per row of §1.2 and §1.3 ─────────────────────── */

test('T1 per-kind table: every kind gets a sentence, never a bare identifier', () => {
  assert.equal(line({ seq: 1, kind: 'session_start', text: 'session started' }), 'Session started')
  assert.equal(line({ seq: 2, kind: 'session_end', text: 'session ended' }), 'Session ended')
  assert.equal(line({ seq: 3, kind: 'user_prompt', text: 'Read the design doc and build it' }), 'The operator asked Read the design doc and build it')
  /* Empty prompts were measured in two legacy-imported sessions: 120 in one and 157 in the other. */
  assert.equal(line({ seq: 4, kind: 'user_prompt', text: '' }), "The operator's message is not in this record")
  /* `subagent_stop` has no renderer branch today and draws its own kind name
     47 times in the target session. */
  assert.equal(line({ seq: 5, kind: 'subagent_stop', text: 'subagent_stop' }), 'A subagent stopped')
  assert.equal(line({ seq: 6, kind: 'stop', text: 'stopped: Both suites pass and the build is clean.' }), 'The agent stopped Both suites pass and the build is clean.')
  /* 1,590 of 1,591 note rows in a legacy-imported session arrive as the literal word `note`. */
  assert.equal(line({ seq: 7, kind: 'note', text: 'note' }), 'The agent noted · the note text is not in this reading')
  assert.equal(line({ seq: 8, kind: 'fs_change', text: 'modify /Users/dev/thruline/ui/src/workspace/adapter.js' }), 'Modified ~/thruline/ui/src/workspace/adapter.js')
  assert.equal(line({ seq: 9, kind: 'exec', text: 'cargo test --workspace → exit 0' }), 'Ran cargo test --workspace · exit 0')
  assert.equal(line({ seq: 10, kind: 'heartbeat', text: 'heartbeat' }), 'Recorded event · heartbeat')
})

test('T1 snapshot: the hash survives and no equality is claimed', () => {
  /* The real label, byte for byte, for every snapshot of a long session. */
  const out = line({ seq: 11, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' })
  assert.equal(out, 'Working tree recorded as tree 7e3a9c01b5d2 · taken at fold')
  assert.ok(!/unchanged/.test(out))
})

test('T1 the four tools that are 84.7% of the corpus', () => {
  assert.equal(line(wholeCall(20, 'Bash', '{"command":"cargo test -p thruline-core","description":"Run the core suite"}')), 'Ran cargo test -p thruline-core')
  assert.equal(
    line(wholeCall(21, 'Edit', '{"file_path":"/Users/dev/thruline/ui/src/workspace/adapter.js","old_string":"a","new_string":"b","replace_all":true}')),
    'Edited ~/thruline/ui/src/workspace/adapter.js · every occurrence',
  )
  assert.equal(line(wholeCall(22, 'Read', '{"file_path":"/Users/dev/thruline/ui/src/lib/voice.js","offset":40,"limit":80}')), 'Read ~/thruline/ui/src/lib/voice.js · lines 40–119')
  assert.equal(line(wholeCall(23, 'Read', '{"file_path":"/tmp/spec.pdf","pages":3}')), 'Read /tmp/spec.pdf · 3 pages')
  assert.equal(line(wholeCall(24, 'Write', '{"content":"hello","file_path":"/tmp/out.txt"}')), 'Wrote /tmp/out.txt · 5 characters')
})

test('T1 multi-line commands say a body was withheld, with its noun', () => {
  /* 69% of this session's Bash commands are multi-line (ground-corpus §D.2). */
  const out = line(wholeCall(25, 'Bash', '{"command":"set -e\\ncargo fmt --check\\ncargo clippy"}'))
  assert.equal(out, 'Ran set -e · 2 more lines')
  assert.equal(line(wholeCall(26, 'Bash', '{"command":"set -e\\ncargo fmt"}')), 'Ran set -e · 1 more line')
  /* Cut before this reading: the surviving lines are a LOWER bound, so no
     count is printed — the absence is named instead. This is the shape 133 of
     this session's risk cards actually arrive in. */
  assert.equal(line(callEntry(27, 'Bash', '{"command":"set -e\\nR=/private/tmp/scratch/scratchpad\\nrm -rf $R/lab"}')), 'Ran set -e' + ELLIPSIS + ' · more lines not in this reading')
})

test('T1 the measured tail, one case per row', () => {
  const rows = [
    ['TaskUpdate', '{"status":"completed","taskId":"a1b2c3d4e5f6a7b8"}', 'Set task a1b2c3d4e5f6 to completed'],
    ['TaskCreate', '{"activeForm":"x","description":"y","subject":"Port the humaniser"}', 'Created task “Port the humaniser”'],
    ['StructuredOutput', '{"correction":"a","reason":"b","refuted":false}', 'Returned structured output · 3 fields'],
    ['ToolSearch', '{"max_results":5,"query":"notebook jupyter"}', 'Looked up tools matching “notebook jupyter”'],
    ['Workflow', '{"description":"Rebuild the fixtures","script":"a\\nb"}', 'Ran workflow: Rebuild the fixtures'],
    ['Workflow', '{"script":"a\\nb\\nc"}', 'Ran workflow · 3 lines of script'],
    ['TodoWrite', '{"todos":[{"a":1},{"b":2},{"c":3}]}', 'Updated the todo list · 3 items'],
    ['SendMessage', '{"to":"BUILD-2","message":"go"}', 'Sent a message to BUILD-2'],
    ['Monitor', '{"command":"npm test","description":"watch"}', 'Watched npm test'],
    ['Agent', '{"description":"Audit the ledger","prompt":"p","subagent_type":"general-purpose"}', 'Delegated to general-purpose: Audit the ledger'],
    ['AskUserQuestion', '{"questions":[{"q":1},{"q":2}]}', 'Asked the operator · 2 questions'],
    ['TaskStop', '{"task_id":"0123456789abcdef"}', 'Stopped task 0123456789ab'],
    ['TaskOutput', '{"block":true,"task_id":"0123456789abcdef","timeout":5}', 'Read output of task 0123456789ab'],
    ['SendUserFile', '{"caption":"c","files":["a.png","b.png","c.png"],"status":"normal"}', 'Sent 3 files'],
    ['SendUserFile', '{"files":["only.png"],"status":"normal"}', 'Sent 1 file'],
    ['DesignSync', '{"method":"pull","projectId":"p"}', 'DesignSync pull'],
    ['WebFetch', '{"prompt":"p","url":"https://opentelemetry.io/blog/2025/how-to-name-your-spans/?utm=1"}', 'Fetched opentelemetry.io/how-to-name-your-spans'],
    ['Artifact', '{"favicon":"x","file_path":"/tmp/a.html","label":"Trace review"}', 'Published artifact “Trace review”'],
    ['ListAgents', '{}', 'Listed agents'],
    ['ScheduleWakeup', '{"delaySeconds":300,"reason":"r"}', 'Scheduled a wake-up in 300 seconds'],
    ['ScheduleWakeup', '{"stop":true}', 'Cancelled the wake-up'],
    ['Skill', '{"skill":"code-review"}', 'Invoked skill code-review'],
    ['WebSearch', '{"query":"span naming"}', 'Searched the web for “span naming”'],
    /* The ONE recorded Grep call, exactly as stored — no path field exists. */
    ['Grep', '{"output_mode":"files_with_matches","pattern":"shop_store_snapshots"}', 'Searched for “shop_store_snapshots”'],
  ]
  rows.forEach(([tool, input, want], i) => assert.equal(line(wholeCall(100 + i, tool, input)), want, tool))
})

test('T1 the measured MCP rows, and the generic MCP shape', () => {
  const rows = [
    ['mcp__Claude_in_Chrome__browser_batch', '{"actions":[{"a":1},{"b":2}]}', 'Ran a browser batch · 2 actions'],
    ['mcp__Claude_Browser__javascript_tool', '{"action":"javascript_exec","text":"1+1"}', 'Ran JavaScript in the browser'],
    ['mcp__Claude_Preview__preview_eval', '{"expression":"window.title","serverId":"s"}', 'Evaluated window.title'],
    ['mcp__Claude_Browser__computer', '{"action":"screenshot","tabId":"t"}', 'Browser: screenshot'],
    ['mcp__Claude_Browser__navigate', '{"tabId":"t","url":"https://docs.honeycomb.io/reference/trace-waterfall"}', 'Navigated to docs.honeycomb.io/trace-waterfall'],
    ['mcp__ccd_session__mark_chapter', '{"summary":"s","title":"Wiring the humaniser"}', 'Marked chapter “Wiring the humaniser”'],
    ['mcp__ccd_session_mgmt__send_message', '{"message":"m","session_id":"abcdef0123456789"}', 'Sent a message to session abcdef012345'],
    ['mcp__Claude_Preview__preview_resize', '{"height":812,"serverId":"s","width":375}', 'Resized the preview to 375×812'],
    /* Not in the measured table: method as the verb, server in the detail. */
    ['mcp__terminal__read_terminal', '{"lines":200}', 'Read terminal · 1 argument · terminal'],
  ]
  rows.forEach(([tool, input, want], i) => assert.equal(line(wholeCall(200 + i, tool, input)), want, tool))
})

test('T1 the universal fallback: four branches, no guessed verb', () => {
  /* 1. a tool with no rule but a priority key — `Glob` has ZERO calls in this
        store, so it must reach here rather than a fabricated renderer. */
  assert.equal(line(wholeCall(300, 'Glob', '{"pattern":"**/*.jsx"}')), 'Called Glob “**/*.jsx”')
  /* 2. a tool name that is not identifier-shaped: `Mark chapter`, one call. */
  assert.equal(line(wholeCall(301, 'Mark chapter', '{"title":"Codebase exploration"}')), 'Called Mark chapter “Codebase exploration”')
  /* 3. no key in the priority list: the count carries its noun. */
  assert.equal(line(wholeCall(302, 'NotebookEdit', '{"cell_id":"c1","edit_mode":"replace"}')), 'Called NotebookEdit · 2 arguments')
  /* 4. nothing readable survived: loud, never blank, never the broken JSON. */
  const broken = { seq: 303, kind: 'tool_call', text: 'ExitPlanMode {"pl' + ELLIPSIS }
  assert.equal(line(broken), 'Called ExitPlanMode · argument text could not be read')
  assert.equal(readableEntry(broken).cutUpstream, true)
})

test('T1 an argument-less call keeps its verb and adds nothing', () => {
  assert.equal(line(wholeCall(304, 'ListAgents', '{}')), 'Listed agents')
  assert.equal(line({ seq: 305, kind: 'tool_call', text: 'ListAgents' }), 'Listed agents')
})

/* ── T2 — no JSON on any headline (defect 1, as a property) ────────────────── */

test('T2 no produced line is serialized JSON', () => {
  /* The three constants that are 98.4% of the target session's 4,243 cards,
     plus every shape above, at the real 50-character transport cut. */
  const entries = [
    callEntry(1, 'Bash', '{"command":"cat -n crates/policy/src/lib.rs","description":"Read the policy crate"}'),
    callEntry(2, 'Bash', '{"command":"cd /private/tmp/scratch/scratchpad && ls -la","description":"List the scratchpad"}'),
    callEntry(3, 'StructuredOutput', '{"correction":"the fixture was stale","reason":"measured","refuted":true}'),
    callEntry(4, 'Edit', '{"file_path":"/Users/dev/thruline/crates/core/src/redact.rs","old_string":"x","new_string":"y"}'),
    { seq: 5, kind: 'tool_result', text: 'Bash ok' },
    { seq: 6, kind: 'tool_result', text: 'Bash FAILED' },
    { seq: 7, kind: 'tool_result', text: 'unknown outcome not stated' },
    { seq: 8, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' },
    { seq: 9, kind: 'subagent_stop', text: 'subagent_stop' },
    { seq: 10, kind: 'note', text: 'note' },
  ]
  entries.forEach((e) => {
    const out = line(e)
    assert.ok(out.length > 0, 'never a blank card: ' + JSON.stringify(e))
    assert.ok(!/^[{[]/.test(out), 'headline starts as JSON: ' + out)
    assert.ok(!/"[a-z_]+"\s*:/.test(out), 'headline carries a JSON key: ' + out)
  })
})

test('T2 the real screenshot label becomes a readable sentence', () => {
  /* The exact string in the operator's screenshot. */
  const entry = { seq: 8044, kind: 'tool_call', text: 'Bash {"command":"cat -n crates/policy/src/lib.rs","desc' + ELLIPSIS }
  assert.equal(line(entry), 'Ran cat -n crates/policy/src/lib.rs')
})

test('T2 no produced line reaches first level with banned vocabulary', () => {
  /* The existing first-level check, `lib/voice.js bannedIn` — jargon anywhere,
     and a record enum used as the name of a state. Every line this module can
     produce is a first-level label. */
  const entries = [
    wholeCall(1, 'Bash', '{"command":"cargo test"}'),
    wholeCall(2, 'Glob', '{"pattern":"**/*.rs"}'),
    wholeCall(3, 'ListAgents', '{}'),
    { seq: 4, kind: 'tool_result', text: 'unknown outcome not stated' },
    { seq: 5, kind: 'tool_result', text: 'Bash FAILED' },
    { seq: 6, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' },
    { seq: 7, kind: 'note', text: 'note' },
    { seq: 8, kind: 'subagent_stop', text: 'subagent_stop' },
    { seq: 9, kind: 'user_prompt', text: '' },
    { seq: 10, kind: 'heartbeat', text: 'heartbeat' },
  ]
  entries.forEach((e) => {
    const out = line(e)
    assert.equal(bannedIn(out), null, 'banned in first-level copy: ' + out)
  })
})

/* ── T3 — determinism ─────────────────────────────────────────────────────── */

test('T3 same input, same line — twice, and under a shuffled iteration order', () => {
  const entries = [
    callEntry(1, 'Bash', '{"command":"ls -la","description":"list"}'),
    { seq: 2, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' },
    { seq: 3, kind: 'tool_result', text: 'Bash ok' },
    wholeCall(4, 'Read', '{"file_path":"/a/b/c.js","limit":10}'),
  ]
  const first = entries.map(line)
  const second = entries.map(line)
  assert.deepEqual(second, first)
  const shuffled = [3, 1, 0, 2].map((i) => ({ i, out: line(entries[i]) }))
  shuffled.forEach(({ i, out }) => assert.equal(out, first[i]))
})

test('T3 the module holds no clock, no locale and no randomness', () => {
  const src = readFileSync(fileURLToPath(new URL('./readable.js', import.meta.url)), 'utf8')
  assert.ok(!src.includes('toLocaleString'), 'locale-dependent formatting')
  assert.ok(!src.includes('Intl.'), 'locale-dependent formatting')
  assert.ok(!/\bDate\b/.test(src), 'a clock')
  assert.ok(!src.includes('Math.random'), 'randomness')
  assert.ok(!src.includes('fetch('), 'network')
})

test('T3 reading an entry never mutates it', () => {
  const entry = callEntry(1, 'Bash', '{"command":"rm -rf $R/lab && mkdir -p $R/lab/home"}')
  const before = JSON.stringify(entry)
  readableEntry(entry)
  assert.equal(JSON.stringify(entry), before)
})

/* ── T4 — the never-assert list (§1.9) ────────────────────────────────────── */

test('T4 `unchanged` is never printed: this reading compares nothing', () => {
  const a = line({ seq: 1, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' })
  const b = line({ seq: 2, kind: 'snapshot', text: 'tree 0011223344ff (taken at fold)' })
  assert.ok(!/unchanged/.test(a) && !/unchanged/.test(b))
  assert.notEqual(a, b)
})

test('T4 the outcome slot has three values, never two', () => {
  assert.equal(line({ seq: 1, kind: 'tool_result', text: 'Bash ok' }), 'Result of a tool call · Bash')
  assert.equal(line({ seq: 2, kind: 'tool_result', text: 'Bash FAILED' }), 'Result of a tool call · Bash · did not complete')
  const notStated = line({ seq: 3, kind: 'tool_result', text: 'Bash outcome not stated' })
  assert.equal(notStated, 'Result of a tool call · Bash · outcome not stated')
  assert.ok(!/\bok\b/.test(notStated) && !/failed/i.test(notStated))
  /* `tool: "unknown"` is 64.9% of result rows store-wide: named, never guessed. */
  assert.equal(line({ seq: 4, kind: 'tool_result', text: 'unknown ok' }), 'Result of a tool call · tool not recorded')
})

test('T4 a description never becomes the headline and never fills a gap', () => {
  const entry = wholeCall(1, 'Bash', '{"command":"cargo test -p thruline-core","description":"Run the whole core suite for the ledger"}')
  const out = line(entry)
  assert.equal(out, 'Ran cargo test -p thruline-core')
  assert.ok(!out.includes('Run the whole core suite'))
  /* And when the command is absent it is NOT substituted: the call falls to
     the loud fallback instead of borrowing the agent's own words. */
  const noCommand = wholeCall(2, 'Bash', '{"description":"Run the whole core suite"}')
  assert.equal(line(noCommand), 'Called Bash · 1 argument')
})

test('T4 a result is never joined to a call by this function', () => {
  /* The humaniser renders one event. Pairing needs the §2.1 R3 proof, and a
     result whose tool differs from the preceding call must not borrow it. */
  const call = readableEntry(wholeCall(1, 'Bash', '{"command":"ls"}'))
  const result = readableEntry({ seq: 2, kind: 'tool_result', text: 'Read ok' })
  assert.equal(composeLine(call), 'Ran ls')
  assert.equal(composeLine(result), 'Result of a tool call · Read')
  assert.ok(!composeLine(call).includes('ok'))
})

/* ── T5 — shortening (§1.7) ───────────────────────────────────────────────── */

test('T5 paths keep the last two segments and elide the middle once', () => {
  /* The design's worked examples. NOTE the budget: §1.7 rule 2 stops before
     eliding when the home-folded path already fits, so the first example is
     asserted at the budget where its elision is actually required. */
  assert.equal(shortenPath('/Users/dev/thruline/ui/src/workspace/adapter.js', 37).text, '~/thruline/' + ELLIPSIS + '/workspace/adapter.js')
  assert.equal(shortenPath('/private/tmp/claude-501/' + ELLIPSIS + '/scratchpad/mod3/_template.html', 44).text, '/private/' + ELLIPSIS + '/mod3/_template.html')
  /* Rule 2: an ellipsis that elides nothing is a lie about length. */
  const roomy = shortenPath('/Users/dev/thruline/ui/src/workspace/adapter.js', 44)
  assert.equal(roomy.text, '~/thruline/ui/src/workspace/adapter.js')
  assert.equal(roomy.elided, false)
  assert.ok(!roomy.text.includes(ELLIPSIS))
  /* Rule 3: the parent survives, so two same-named files stay distinguishable. */
  const a = shortenPath('/Users/dev/thruline/ui/src/mac/shell.jsx', 32).text
  const b = shortenPath('/Users/dev/thruline/ui/src/workspace/shell.jsx', 32).text
  assert.notEqual(a, b)
  assert.ok(a.endsWith('mac/shell.jsx') && b.endsWith('workspace/shell.jsx'))
  /* Rule 6: the filename alone is truncated FROM THE LEFT. */
  assert.equal(shortenPath('/a/b/an-extremely-long-generated-name-fixtures.json', 20).text, ELLIPSIS + '-name-fixtures.json')
})

test('T5 a short path produces no ellipsis at all', () => {
  const r = shortenPath('/tmp/out.txt', 60)
  assert.equal(r.text, '/tmp/out.txt')
  assert.equal(r.elided, false)
})

test('T5 a URL loses its query string entirely', () => {
  const r = shortenUrl('https://example.com/a/b/c?token=abc123#frag', 60)
  assert.equal(r.text, 'example.com/c')
  assert.ok(!r.text.includes('token'))
  assert.equal(shortenUrl('https://example.com', 60).text, 'example.com')
})

test('T5 a hash is twelve characters, matching the engine constant', () => {
  assert.equal(shortHash('7e3a9c01b5d24f68a0c9d3e1'), '7e3a9c01b5d2')
  assert.equal(shortHash('sha256:89d68072fb303e4cd212d64a0effbacf'), '89d68072fb30')
  assert.equal(shortHash('7e3a9c01b5d24f68a0c9d3e1').length, 12)
})

test('T5 the ellipsis appears if and only if characters were removed', () => {
  const short = truncateWords('cargo test', 40)
  assert.equal(short.text, 'cargo test')
  assert.equal(short.elided, false)
  const long = truncateWords('cargo test --workspace --all-features --no-fail-fast', 20)
  assert.ok(long.text.endsWith(ELLIPSIS))
  assert.ok(long.elided)
  assert.ok(!long.text.includes('...'))
  /* Word boundary, not a mid-word chop. */
  assert.equal(long.text, 'cargo test' + ELLIPSIS)
})

test('T5 a redaction marker is never split by the cut', () => {
  /* `[masked: high_entropy]` markers occur in this store's real text; splitting
     one strands `[masked:` on the card. */
  const out = truncateWords('export TOKEN=[masked: high_entropy] && run', 26)
  assert.ok(!out.text.includes('[masked:') || out.text.includes('[masked: high_entropy]'))
  assert.equal(out.text, 'export TOKEN=' + ELLIPSIS)
})

/* ── T6 — geometry agreement (the overlap bug, as a test) ──────────────────── */

test('T6 every produced title fits the lines `estH` budgets for it', () => {
  const entries = [
    callEntry(1, 'Bash', '{"command":"cd /private/tmp/scratch/scratchpad && python3 _runs.py --all"}'),
    wholeCall(2, 'Bash', '{"command":"' + 'x'.repeat(1400) + '"}'),
    wholeCall(3, 'Read', '{"file_path":"' + '/very-long-directory-name'.repeat(9) + '/file.js","offset":1,"limit":2}'),
    wholeCall(4, 'WebSearch', '{"query":"' + 'span naming conventions '.repeat(20) + '"}'),
    { seq: 5, kind: 'user_prompt', text: 'y'.repeat(400) },
    { seq: 6, kind: 'stop', text: 'stopped: ' + 'z'.repeat(400) },
    { seq: 7, kind: 'snapshot', text: 'tree 7e3a9c01b5d2 (taken at fold)' },
  ]
  entries.forEach((e) => {
    const l = readableEntry(e)
    const title = composeLine(l)
    const rendered = Math.ceil((title.length * (l.mono ? ADV_MONO : ADV_BODY)) / CARD_CW)
    /* The height authority, reading the same advance constant. */
    const budgeted = titleLines({ title, mono: l.mono }, CARD_CW)
    assert.ok(budgeted >= rendered, 'estH under-budgets: ' + title)
    assert.ok(rendered <= TITLE_LINES, 'title spills past ' + TITLE_LINES + ' lines (' + rendered + '): ' + title)
  })
})

test('T6 the two budgets are the same arithmetic the height authority uses', () => {
  assert.equal(TITLE_BUDGET_MONO, TITLE_LINES * Math.floor(CARD_CW / ADV_MONO))
  assert.equal(TITLE_BUDGET_BODY, TITLE_LINES * Math.floor(CARD_CW / ADV_BODY))
  assert.equal(titleLines({ title: 'x'.repeat(TITLE_BUDGET_MONO), mono: true }, CARD_CW), TITLE_LINES)
  assert.equal(titleLines({ title: 'x'.repeat(TITLE_BUDGET_BODY), mono: false }, CARD_CW), TITLE_LINES)
})

/* ── T7 — the receipt survives ─────────────────────────────────────────────── */

test('T7 the recorded value is never the derived line, and is reachable whole', () => {
  /* One of the 133 destructive-pattern matches of one long session, in the
     shape the transport delivers: the command's own closing quote survived the
     50-character cut, the description did not. */
  const entry = callEntry(1, 'Bash', '{"command":"rm -rf \\"$W\\"; mkdir -p \\"$W/home\\"","description":"Reset the workspace"}')
  const derived = readableText(entry)
  /* The record value the adapter received is untouched by this module, which
     is what lets the inspector show `As recorded` byte-equal and copy it. */
  assert.equal(entry.text, 'Bash {"command":"rm -rf \\"$W\\"; mkdir -p \\"$W/home\\"","' + ELLIPSIS)
  assert.notEqual(derived, entry.text)
  assert.equal(derived, 'Ran rm -rf "$W"; mkdir -p "$W/home"')
  /* Two different facts, and the reading keeps them apart (§1.6.4): nothing
     was removed from the value SHOWN, so the line carries no elision claim —
     while the payload behind it was cut, which `cutUpstream` records. */
  assert.ok(!derived.includes(ELLIPSIS))
  assert.equal(readableEntry(entry).elided, false)
  assert.equal(readableEntry(entry).cutUpstream, true)
})

test('T7 a value the record itself no longer holds ends in one elision glyph', () => {
  const entry = callEntry(2, 'Bash', '{"command":"cargo test --workspace --all-features --no-fail-fast --quiet"}')
  const derived = readableText(entry)
  assert.ok(derived.endsWith(ELLIPSIS))
  assert.equal(derived.split(ELLIPSIS).length - 1, 1)
  assert.equal(readableEntry(entry).cutUpstream, true)
})

test('T7 a value the transport delivered whole carries no elision claim', () => {
  const entry = wholeCall(1, 'Bash', '{"command":"npm test"}')
  const l = readableEntry(entry)
  assert.equal(composeLine(l), 'Ran npm test')
  assert.equal(l.elided, false)
  assert.equal(l.cutUpstream, false)
})

/* ── the tolerant extractor, on the shapes that break `JSON.parse` ─────────── */

test('the extractor reads a value out of a prefix cut mid-JSON', () => {
  const cut = '{"command":"cargo test --workspace --all-featur'
  const hit = extractString(cut, 'command')
  assert.equal(hit.value, 'cargo test --workspace --all-featur')
  assert.equal(hit.cut, true)
  assert.equal(topLevelKeys(cut), null)
  assert.equal(countArrayItems(cut, 'todos'), null)
})

test('the extractor unescapes what JSON escaped, and stops at the real quote', () => {
  const whole = '{"command":"echo \\"hi\\"\\nls","description":"say hi"}'
  const hit = extractString(whole, 'command')
  assert.equal(hit.value, 'echo "hi"\nls')
  assert.equal(hit.cut, false)
  assert.deepEqual(topLevelKeys(whole), ['command', 'description'])
})

test('the view keeps a tool name that contains a space', () => {
  const v = viewFromEntry(wholeCall(1, 'Mark chapter', '{"title":"Wiring"}'))
  assert.equal(v.tool, 'Mark chapter')
  assert.equal(v.argsText, '{"title":"Wiring"}')
})

test('a line built from a widened payload phrases identically to the cut one', () => {
  /* Stage A changes fidelity, never phrasing — which is what makes these tests
     survive the transport widening. */
  const stageA = readableLine({ kind: 'tool_call', tool: 'Bash', args: { command: 'npm test', description: 'run it' }, argsText: '' })
  assert.equal(composeLine(stageA), 'Ran npm test')
  const stageB = readableEntry(wholeCall(1, 'Bash', '{"command":"npm test","description":"run it"}'))
  assert.equal(composeLine(stageB), composeLine(stageA))
})
