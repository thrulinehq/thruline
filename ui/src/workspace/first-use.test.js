import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-2-7 — the first-use path, rendered.
 *
 * The same harness the Inbox and the Change screen use: the surface is bundled
 * and rendered to static markup, and every assertion below is about the
 * STRINGS AND CONTROLS A PERSON MEETS. A second bundle is built over a PROBE
 * copy — the real `HQ_COPY` with three first-use strings replaced — because
 * equality with the real copy cannot tell a string that came from the copy file
 * from a literal that happens to match it.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-first-use-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { FirstUseScreen } from './first-use.jsx';
  import { landingScreen } from './first-use.jsx';
  import { candidateRows, proposalView, pickAction, rowFacts, criteriaLines, checkAvailability } from './first-use-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { candidateRows, proposalView, pickAction, rowFacts, criteriaLines, checkAvailability, landingScreen, HQ_COPY };
  export const render = props => renderToStaticMarkup(React.createElement(FirstUseScreen, props));
`

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}

async function load(name, plugins) {
  const result = await build({ ...COMMON, stdin: { contents: ENTRY, resolveDir: here }, plugins })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await load('first-use.mjs')

const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('first-use-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("      heading: 'Pick what you are working on',", "      heading: 'probe-pick-heading',")
        .replace("      sentence: 'ThruLine will keep its records in one folder on this machine. Nothing leaves it.',", "      sentence: 'probe-storage-sentence',")
        .replace("      unavailable: 'not available here: {tool}',", "      unavailable: 'probe-unavailable {tool}',"),
      loader: 'js',
    }))
  },
}])

const { candidateRows, proposalView, pickAction, HQ_COPY, render } = M
const C = HQ_COPY.first_use

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/\s+/g, ' ').trim()

/* ── synthetic documents, in the shape the routes serve ────────────────── */

const OVERVIEW = {
  schema_version: 1,
  registered: 0,
  store: { folder: '/tmp/fixture-home/.thruline', bytes: 897024 },
  candidates: [
    {
      key: 'cccccccc00000001', label: '/tmp/fixture/uploads-api', source: 'claude_code',
      recordings: 14, not_read: 0, first_recorded: '2026-01-02T09:00:00Z', last_recorded: '2026-01-09T09:00:00Z',
      registered: false, project: null,
    },
    {
      key: 'cccccccc00000002', label: '/tmp/fixture/site', source: 'here',
      recordings: 0, not_read: 0, first_recorded: null, last_recorded: null,
      registered: false, project: null,
    },
  ],
  codex_recordings: 0,
  notes: [],
}

/* The same machine, arrived at a second time: the project is picked and one
 * Change is already being tracked in it. Every field is the server's own typed
 * one. */
const CAME_BACK = {
  ...OVERVIEW,
  registered: 1,
  candidates: [
    { ...OVERVIEW.candidates[0], registered: true, project: 'cccccccc-0000-4000-8000-00000000p001', changes: 1 },
    { ...OVERVIEW.candidates[1], registered: true, project: 'cccccccc-0000-4000-8000-00000000p002', changes: 0 },
  ],
}

const came_back = (over = {}, mod = M) => ({
  status: 'ready', step: 'pick', selected: null,
  ...mod.candidateRows(CAME_BACK),
  storage: { known: true, size: '0.9 MB', path: CAME_BACK.store.folder, bytes: CAME_BACK.store.bytes },
  ...over,
})

const PROPOSAL = {
  schema_version: 1,
  project: 'cccccccc-0000-4000-8000-00000000p001',
  proposed: {
    session: 'cccccccc-0000-4000-8000-00000000s002',
    recorded_at: '2026-01-09T09:02:00Z',
    title: 'Fix the retry loop in the uploader',
    request: 'Fix the retry loop in the uploader. It drops the last entry.',
    request_event: 'cccccccc-0000-4000-8000-00000000e001',
    files_touched: 2,
    profile: {
      checks: [
        { id: 'cargo_test', tool: 'cargo', category: 'tests', mandatory: true, availability_slug: 'check_ready', availability_name: '' },
        { id: 'semgrep', tool: 'semgrep', category: 'static', mandatory: false, availability_slug: 'check_not_available_here', availability_name: 'semgrep' },
        { id: 'cargo_clippy', tool: 'cargo-clippy', category: 'static', mandatory: false, availability_slug: 'check_needs_a_recipe', availability_name: '.thruline/clippy.json' },
      ],
    },
  },
}

const pick = (over = {}, mod = M) => ({
  status: 'ready', step: 'pick', selected: null,
  ...mod.candidateRows(OVERVIEW),
  storage: { known: true, size: '0.9 MB', path: OVERVIEW.store.folder, bytes: OVERVIEW.store.bytes },
  ...over,
})

const propose = (doc = PROPOSAL, over = {}, mod = M) => ({
  status: 'ready', step: 'propose', project: 'p1', proposal: mod.proposalView(doc), ...over,
})

const done = (over = {}) => ({
  status: 'ready', step: 'done', change: 'chg_1',
  criteria: [{ number: 1, text: 'The last entry is not dropped when a retry fails.' }],
  ...over,
})

/* ── the model: every number is a typed field ──────────────────────────── */

test('candidates carry the counts the record states, and a count that is not a number is never rendered as zero', () => {
  const { rows, registered, storeFolder, storeBytes } = candidateRows(OVERVIEW)
  assert.equal(rows.length, 2)
  assert.deepEqual(
    rows.map((r) => [r.label, r.recorded, r.newest, r.here]),
    [['/tmp/fixture/uploads-api', 14, '2026-01-09', false], ['/tmp/fixture/site', 0, null, true]],
  )
  assert.equal(registered, 0)
  assert.equal(storeFolder, '/tmp/fixture-home/.thruline')
  assert.equal(storeBytes, 897024)

  const unknown = candidateRows({ candidates: [{ key: 'k', label: '/x', recordings: 'lots' }] })
  assert.equal(unknown.rows[0].recorded, null, 'an uncountable count became a number')
  assert.deepEqual(M.rowFacts(unknown.rows[0])[0], { key: 'recorded_unknown', values: {} })
  assert.equal(candidateRows(null).rows.length, 0)
})

test('the proposal is the record speaking: a missing one is missing, never an empty Change', () => {
  const p = proposalView(PROPOSAL)
  assert.equal(p.have, true)
  assert.equal(p.title, 'Fix the retry loop in the uploader')
  assert.equal(p.touched, 2)
  /* The two reasons a check cannot run are counted apart, because they say
     opposite things about the machine. */
  assert.equal(p.absent, 1)
  assert.equal(p.needsSettings, 1)
  assert.deepEqual(p.checks.map((c) => [c.tool, c.available]), [['cargo', true], ['semgrep', false], ['cargo-clippy', false]])
  assert.deepEqual(M.checkAvailability(p.checks[1]), { key: 'unavailable', values: { tool: 'semgrep' } })
  assert.deepEqual(M.checkAvailability(p.checks[2]), { key: 'needs_settings', values: { name: '.thruline/clippy.json' } })
  assert.equal(M.checkAvailability(p.checks[0]), null)
  assert.equal(proposalView({ project: 'p1', proposed: null }).have, false)
  assert.equal(proposalView(null).have, false)
})

test('the one action is off until something is picked, and says why and what would turn it on', () => {
  const { rows } = candidateRows(OVERVIEW)
  const off = pickAction(null, rows)
  assert.equal(off.enabled, false)
  assert.equal(off.reason, C.pick.blocked.reason)
  assert.equal(off.enabler, C.pick.blocked.enabler)
  assert.ok(off.reason && off.enabler, 'a disabled control with no reason')
  assert.equal(pickAction('cccccccc00000001', rows).enabled, true)
  assert.equal(pickAction('not-a-key', rows).enabled, false, 'a key the server never sent is not a pick')
})

/* ── the screen ────────────────────────────────────────────────────────── */

test('step 1 lists what this machine already recorded, with the counts beside each one', () => {
  const html = render({ state: pick() })
  const body = text(html)
  assert.ok(body.includes('/tmp/fixture/uploads-api'), body)
  assert.ok(body.includes('14 pieces of recorded work'), body)
  assert.ok(body.includes('most recent 2026-01-09'), body)
  assert.ok(body.includes('Nothing recorded yet'), body)
  assert.ok(body.includes(C.pick.here), body)
  /* Picking is a choice between the rows, not a row of equal actions. */
  assert.equal((html.match(/type="radio"/g) || []).length, 2, html)
})

test('fresh and mixed first-use rows sit under truthful group headings', () => {
  const fresh = { ...OVERVIEW, candidates: [OVERVIEW.candidates[1]] }
  const freshHtml = render({ state: pick({ ...candidateRows(fresh) }) })
  const freshText = text(freshHtml)
  assert.ok(freshText.includes('ThruLine has not recorded work for any of these folders yet. Pick one to start.'), freshText)
  assert.ok(!freshText.includes('These are the repositories this machine has already recorded work for.'), freshText)
  assert.ok(freshText.includes('/tmp/fixture/site'), freshText)
  assert.ok(freshText.includes('Nothing recorded yet'), freshText)

  const mixed = render({ state: pick() })
  const recorded = mixed.match(/<section[^>]*aria-labelledby="ws-first-recorded"[^>]*>([\s\S]*?)<\/section>/)?.[1]
  const other = mixed.match(/<section[^>]*aria-labelledby="ws-first-other"[^>]*>([\s\S]*?)<\/section>/)?.[1]
  assert.ok(recorded, mixed)
  assert.ok(other, mixed)
  assert.ok(text(recorded).includes('These are the repositories this machine has already recorded work for.'), recorded)
  assert.ok(text(recorded).includes('/tmp/fixture/uploads-api'), recorded)
  assert.ok(!text(recorded).includes('Nothing recorded yet'), recorded)
  assert.ok(text(other).includes('Other folders you can start from'), other)
  assert.ok(text(other).includes('/tmp/fixture/site'), other)
  assert.ok(text(other).includes('Nothing recorded yet'), other)

  const unknown = { ...fresh, candidates: [{ ...fresh.candidates[0], recordings: 'unreadable' }] }
  const unknownText = text(render({ state: pick({ ...candidateRows(unknown) }) }))
  assert.ok(unknownText.includes('Some folders’ recorded work could not be counted. Pick one to start.'), unknownText)
  assert.ok(!unknownText.includes('ThruLine has not recorded work for any of these folders yet.'), unknownText)
})

test('the storage sentence is on the screen BEFORE anything is registered, and the folder is one level down', () => {
  const html = render({ state: pick() })
  const body = text(html)
  assert.ok(body.includes(C.storage.sentence), body)
  /* T-7-1 rehearsal: the line under it used to promise "Nothing is written
     there until you pick a project below." while the folder already held the
     record store and a first reading, written when `thruline ui` started. It
     is the measured L1.1 sentence now — the Inbox's words, the served number. */
  const kept = HQ_COPY.change.inbox.storage
  assert.ok(body.includes('ThruLine is keeping 0.9 MB of records on this machine.'), body)
  assert.ok(body.includes(kept.note), body)
  assert.ok(!/Nothing is written there/.test(body), body)
  /* The path is behind a disclosure, like every other path on a plain
     surface — present, never on top. */
  assert.match(html, /<details[^>]*>\s*<summary>Where it will keep them<\/summary>/, html)
  const beforeDetails = body.slice(0, body.indexOf(C.storage.where_open))
  assert.ok(!beforeDetails.includes(OVERVIEW.store.folder), 'the folder is on top of the screen')

  /* And it is the copy file's sentence, not a literal in the view. */
  const probed = text(PROBED.render({ state: pick({}, PROBED) }))
  assert.ok(probed.includes('probe-storage-sentence'), probed)
  assert.ok(!probed.includes(C.storage.sentence), probed)
})

/* T-7-1 rehearsal: the empty-machine first screen carried no storage line at
 * all, while `thruline ui` had already written the record store. It says what
 * will be kept and what is kept now, measured — and, when the size could not
 * be measured, says that rather than a zero. */
test('an empty machine still says what ThruLine keeps and how much, measured', () => {
  const kept = HQ_COPY.change.inbox.storage
  const empty = { schema_version: 1, registered: 0, store: { folder: '/tmp/fixture-home/.thruline', bytes: 1_093_632 }, candidates: [], codex_recordings: 0, notes: [] }
  const html = render({ state: { status: 'ready', step: 'pick', selected: null, ...candidateRows(empty), storage: { known: true, size: '1.1 MB', path: empty.store.folder, bytes: empty.store.bytes } } })
  const body = text(html)
  assert.ok(body.includes(C.pick.none.heading), body)
  assert.ok(body.includes(C.storage.sentence), body)
  assert.ok(body.includes('ThruLine is keeping 1.1 MB of records on this machine.'), body)
  assert.ok(body.includes(kept.note), body)
  /* The folder is one level down here too, and the empty state keeps its one primary. */
  assert.match(html, /<details[^>]*>\s*<summary>Where it will keep them<\/summary>/, html)
  assert.equal((html.match(/class="ws-inbox-primary"/g) || []).length, 1, html)

  const unmeasured = text(render({ state: { status: 'ready', step: 'pick', selected: null, ...candidateRows(empty), storage: { known: false } } }))
  assert.ok(unmeasured.includes(kept.unknown), unmeasured)
  assert.ok(!/keeping 0|0 B/.test(unmeasured), unmeasured)
})

test('the one primary action on step 1 is disabled with both halves of the reason, and there is exactly one of it', () => {
  const html = render({ state: pick() })
  assert.match(html, /<button type="button" class="ws-inbox-primary" disabled=""/, html)
  assert.match(html, /aria-describedby="ws-first-why-pick"/, html)
  const body = text(html)
  assert.ok(body.includes(C.pick.blocked.reason), body)
  assert.ok(body.includes(C.pick.blocked.enabler), body)
  assert.equal((html.match(/class="ws-inbox-primary"/g) || []).length, 1, html)

  const picked = render({ state: pick({ selected: 'cccccccc00000001' }) })
  assert.ok(!picked.includes('disabled'), picked)
  assert.equal((picked.match(/class="ws-inbox-primary"/g) || []).length, 1, picked)
})

test('step 2 shows the Change in the words the record holds, and what it suggests checking is not evidence', () => {
  const html = render({ state: propose() })
  const body = text(html)
  assert.ok(body.includes('Fix the retry loop in the uploader'), body)
  assert.ok(body.includes('It drops the last entry.'), body)
  assert.ok(body.includes('The record shows 2 files touched while this was worked on.'), body)
  assert.ok(body.includes(C.propose.checks_note), body)
  assert.ok(body.includes(C.propose.action), body)
  /* No state word anywhere on this screen: nothing here is a verdict. */
  for (const word of ['Proven', 'Not proven', "Recorded", 'Loose thread', 'Fog']) {
    assert.ok(!body.includes(word), `${word} on the first-use screen: ${body}`)
  }
})

test('a tool this machine does not have is named as absent, apart from anything about the work (B11.6)', () => {
  const html = render({ state: propose() })
  const body = text(html)
  assert.ok(body.includes('not available here: semgrep'), body)
  assert.ok(body.includes(C.propose.unavailable_note), body)
  /* And a program that IS installed here is never called absent: the other
     reason is its own sentence, naming the settings file. */
  assert.ok(body.includes('installed here, but there is no settings file for it yet (.thruline/clippy.json), so it cannot run'), body)
  assert.ok(!body.includes('not available here: cargo-clippy'), body)
  assert.ok(body.includes(C.propose.needs_settings_note), body)
  /* It is a fact about the machine, so it is not styled as a failure: nothing
     on this screen is red, and the absent line is the muted class. */
  assert.match(html, /class="ws-first-absent"/, html)

  const probed = text(PROBED.render({ state: propose(PROPOSAL, {}, PROBED) }))
  assert.ok(probed.includes('probe-unavailable semgrep'), probed)
})

test('a project with nothing captured says exactly that, offers one action, and never says setup required', () => {
  const html = render({ state: propose({ project: 'p1', proposed: null }) })
  const body = text(html)
  assert.ok(
    body.includes('Nothing has been captured for this project yet. Install the capture hooks and the next session will appear here.'),
    body,
  )
  assert.ok(body.includes('thruline init'), body)
  assert.ok(body.includes(C.propose.none.copy_command), body)
  assert.ok(!/setup required/i.test(body), body)
  /* One action, and it is the one that would change the situation. */
  assert.equal((html.match(/<button/g) || []).length, 1, html)
})

test('both first-use init invitations describe the writes that a declined speaker prompt still permits', () => {
  /* `thruline init` asks only whether to add speaker hooks. Answering `n`
     still installs the sensor hooks and the context-server entry. A blanket
     promise that it asks before writing anything gives the wrong consent
     boundary on both cards that offer the command. */
  for (const [card, state] of [
    ['no project', pick({ rows: [] })],
    ['no recordings', propose({ project: 'p1', proposed: null })],
  ]) {
    const body = text(render({ state }))
    assert.ok(body.includes('thruline init'), `${card}: ${body}`)
    assert.ok(body.includes(C.propose.none.installs), `${card}: missing init disclosure: ${body}`)
  }

  const disclosure = C.propose.none.installs
  /* REVIEW-EMPTY-TRUE addendum 2 (D-333): the ruled sentence. */
  assert.equal(disclosure, "It writes ThruLine's recording hooks and its context server into your agent's settings, asks before installing anything optional, prints every file it touched, and reverses with `thruline init --undo`.")
  assert.match(disclosure, /\bcontext server\b/i, 'name the MCP entry installed even after `n`')
  assert.doesNotMatch(disclosure, /\basks before it writes anything\b/i, 'there is no all-writes consent prompt')
})

test('step 3: the Change is written, the Inbox is the one primary action, and what counts as done is a separate offered click', () => {
  const html = render({ state: done() })
  const body = text(html)
  assert.ok(body.includes(C.done.heading), body)
  assert.ok(body.includes(C.done.action), body)
  assert.ok(body.includes(C.done.criteria_action), body)
  assert.ok(body.includes(C.done.criteria_note), body)
  /* The Inbox link is the only primary; accepting is visibly secondary. */
  assert.equal((html.match(/class="ws-inbox-primary"/g) || []).length, 1, html)
  assert.match(html, /href="#\/inbox"/, html)
  assert.match(html, /class="ws-first-secondary"/, html)

  const accepted = text(render({ state: done({ accepted: 1 }) }))
  assert.ok(accepted.includes('Accepted as what counts as done (list 1). Accepting this is not approving the work.'), accepted)
  assert.ok(!accepted.includes(C.done.criteria_action), 'the accept control is still offered after it was used')
})

test('a read that failed is said, and nothing is claimed about what this machine has', () => {
  const body = text(render({ state: { status: 'refused', refusal: '/api/first-use answered HTTP 503' } }))
  assert.ok(body.includes('/api/first-use answered HTTP 503'), body)
  assert.ok(!body.includes(C.pick.none.heading), body)
  assert.ok(!body.includes('Nothing recorded yet'), body)
})

test('a write that failed is said, and never reads as a success', () => {
  const body = text(render({ state: pick({ selected: 'cccccccc00000001', failed: 'refused: store_unavailable' }) }))
  assert.ok(body.includes('This project could not be picked: refused: store_unavailable. Nothing was written down.'), body)
})

/* ── the vocabulary, on the surface a person meets ─────────────────────── */

const BANNED = ['lie', 'lied', 'lying', 'liar', 'fake', 'faked', 'dishonest', 'cheat', 'cheated', 'cheating', 'hallucinated', 'lazy', 'deceptive']
/* `03-USER-LENS.md` rule 2: the words that belong to the technical layer. The
 * bare word "session" is not one of them — "session id" is — and the sentence
 * this card requires about capture uses it. */
const ENGINE = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdisposition\b/i, /\bledger\b/i, /\bpredicate\b/i, /\btree hash\b/i, /\bsession id\b/i, /\bpacket version\b/i]

test('no banned word and no engine noun reaches this screen, in any of its steps', () => {
  const screens = [
    pick(), pick({ selected: 'cccccccc00000001' }), propose(),
    propose({ project: 'p1', proposed: null }), done(), done({ accepted: 1 }),
    came_back(), came_back({ selected: 'cccccccc00000001' }), done({ already: true }),
  ]
  for (const state of screens) {
    const body = text(render({ state }))
    for (const word of BANNED) {
      assert.ok(!new RegExp(`\\b${word}\\b`, 'i').test(body), `"${word}" on step ${state.step}: ${body}`)
    }
    for (const re of ENGINE) {
      assert.doesNotMatch(body, re, `engine vocabulary on step ${state.step}: ${body}`)
    }
    /* No percentage, and nothing that reads as a score. */
    assert.doesNotMatch(body, /\d+\s?%/, `a percentage on step ${state.step}: ${body}`)
  }
})

test('the engine-noun guard is a live instrument', () => {
  for (const bad of ['3 events in view', 'the ledger was read', 'a receipt for this', '2 candidates', 'session id 41']) {
    assert.ok(ENGINE.some((re) => re.test(bad)), `guard misses: ${bad}`)
  }
})

/* ── keyboard and motion ───────────────────────────────────────────────── */

test('every control is reachable and named without a mouse, and the screen has no motion to reduce', () => {
  const html = render({ state: pick() })
  /* The choice is native radio inputs inside labels: tab, arrow keys and the
     label text all work without a line of JavaScript. */
  assert.equal((html.match(/<label class="ws-first-pick">/g) || []).length, 2, html)
  assert.match(html, /name="ws-first-project"/, html)
  /* The disabled control points at its own reason, so a screen reader reads
     the reason with the control rather than after it. */
  assert.match(html, /aria-describedby="ws-first-why-pick"[^>]*>[^<]*<\/button>/, html)
  assert.match(html, /<p class="ws-inbox-muted" id="ws-first-why-pick">/, html)
  /* Working states are announced, not just drawn. */
  assert.match(render({ state: pick({ busy: true }) }), /role="status"/)

  const css = readFileSync(join(here, 'workspace.css'), 'utf8')
  const reduced = css.slice(css.indexOf('── the first-use path (T-2-7)'))
  assert.match(reduced, /@media \(prefers-reduced-motion: reduce\)/, 'the new screen has no reduced-motion block')
  for (const cls of ['ws-first-secondary', 'ws-first-pick']) {
    assert.ok(reduced.includes(cls), `${cls} is outside the reduced-motion block`)
  }
  /* Nothing on this screen carries meaning by colour: no green, no red. */
  assert.ok(!/ws-first-[a-z-]*\s*{[^}]*(green|#0f0|#00ff00)/i.test(reduced), reduced)
})

/* ── coming back to the first screen ──────────────────────────────────────
 *
 * `#/start` is reachable by a bookmark, a typed address, a second tab and the
 * back button. Every arrival used to offer "Use this project" for a project
 * already picked, with no note that it was, and each walk through wrote the same
 * recorded work down as another Change — three identical items in one Inbox,
 * with nothing on screen to say why there were three. That is the *confused*
 * failure mode `03-USER-LENS.md` §2 names, and T-2-6's "one obvious thing to
 * do", broken by this product rather than by the record.
 */

test('a project that is already picked and already tracked says so, in the row', () => {
  const { rows } = candidateRows(CAME_BACK)
  assert.deepEqual(M.rowFacts(rows[0]).at(-1), { key: 'tracked_one', values: {} })
  /* Picked with nothing tracked in it yet is the OTHER fact, and gets the
     other sentence. */
  assert.deepEqual(M.rowFacts(rows[1]).at(-1), { key: 'already', values: {} })
  /* A count that is not a number is not read as a count. */
  const odd = candidateRows({ candidates: [{ key: 'k', label: '/x', registered: true, changes: 'some' }] })
  assert.equal(odd.rows[0].changes, null)
  assert.deepEqual(M.rowFacts(odd.rows[0]).at(-1), { key: 'already', values: {} })

  const html = render({ state: came_back() })
  const body = text(html)
  assert.ok(body.includes('Already picked, and 1 Change is being tracked here'), body)
  assert.ok(body.includes('Already picked'), body)
})

test('picking a project that is already tracked offers the Inbox, not a second copy of the same work', () => {
  const { rows } = candidateRows(CAME_BACK)
  const tracked = pickAction('cccccccc00000001', rows)
  assert.equal(tracked.kind, 'inbox')
  assert.equal(tracked.enabled, true)
  assert.equal(tracked.label, C.pick.already_action)
  assert.equal(tracked.note, C.pick.already_note)
  /* Registered with nothing tracked in it keeps the ordinary action: there is
     nothing to duplicate, and an Inbox with nothing of theirs in it is not
     help. */
  assert.equal(pickAction('cccccccc00000002', rows).kind, 'use')
  assert.equal(pickAction('cccccccc00000002', rows).label, C.pick.action)
  /* And a store that has never seen this project is untouched by any of it. */
  assert.equal(pickAction('cccccccc00000001', candidateRows(OVERVIEW).rows).kind, 'use')

  const html = render({ state: came_back({ selected: 'cccccccc00000001' }) })
  const body = text(html)
  assert.ok(body.includes(C.pick.already_note), body)
  assert.ok(!body.includes(C.pick.action), 'the screen still offers to use a project it has already used: ' + body)
  /* Still exactly one primary action (rule 7), and it explains itself before
     it is clicked rather than after. */
  assert.equal((html.match(/class="ws-inbox-primary"/g) || []).length, 1, html)
  assert.match(html, /aria-describedby="ws-first-why-pick"/, html)
  assert.ok(body.indexOf(C.pick.already_note) < body.indexOf(C.pick.already_action), body)
})

test('a second arrival says nothing was written, and never claims an act it did not perform', () => {
  const body = text(render({ state: done({ already: true }) }))
  assert.ok(body.includes('This work is already written down'), body)
  assert.ok(
    body.includes('A Change for this work was written down earlier, so nothing was written again. It is in your Inbox.'),
    body,
  )
  assert.ok(!body.includes(C.done.heading), 'a second arrival still says the Change was just written down: ' + body)
  /* The one primary action is still the Inbox. */
  assert.equal((text(render({ state: done({ already: true }) })).match(/See what needs your attention/g) || []).length, 1)
})

test('coming back to #/start with nothing left to pick opens the Inbox, not the pick screen again', () => {
  const ready = (doc) => ({ status: 'ready', doc })
  /* The verifier's own machine: one repository, already picked, already
     tracked. Every visit to `#/start` — bookmark, typed address, second tab —
     opens the Inbox, where that one Change already is. */
  const nothing_left = ready({ ...CAME_BACK, candidates: [CAME_BACK.candidates[0]] })
  assert.equal(M.landingScreen(nothing_left, 'first-use'), 'inbox')
  /* But a repository this machine can see and is NOT tracking keeps the door
     open: sending a person to the Inbox there would leave them no way to pick
     it at all. */
  assert.equal(M.landingScreen(ready(CAME_BACK), 'first-use'), 'first-use')
  /* Nothing picked yet: the first screen, from either address. */
  assert.equal(M.landingScreen(ready(OVERVIEW), 'first-use'), 'first-use')
  assert.equal(M.landingScreen(ready(OVERVIEW), 'inbox'), 'first-use')
  /* The bare address is the Inbox once anything is picked (T-2-6, `B8.1`) —
     the pick screen is never what a bare address opens on a used machine. */
  assert.equal(M.landingScreen(ready(CAME_BACK), 'inbox'), 'inbox')
  /* A read that failed is never read as "you have nothing": that would be this
     product inventing a fact about somebody's machine. */
  assert.equal(M.landingScreen({ status: 'refused', refusal: 'HTTP 503' }, 'first-use'), 'inbox')
  assert.equal(M.landingScreen(ready({ registered: 'none' }), 'first-use'), 'inbox')
  assert.equal(M.landingScreen(null, 'first-use'), 'inbox')

  const main = readFileSync(join(here, '..', 'main.jsx'), 'utf8')
  assert.match(main, /at\.screen === 'inbox' \|\| at\.screen === 'first-use'/, 'the pick screen is mounted by an address alone again')
  assert.equal((main.match(/<Landing address=\{at\.screen\} \/>/g) || []).length, 1, 'two landings is two answers to one question')
  assert.ok(!/const \{ default: FirstUse \}/.test(main), 'main.jsx mounts the pick screen directly again: ' + main)
})
