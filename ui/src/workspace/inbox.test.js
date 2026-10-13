import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-2-6 — the Inbox, rendered.
 *
 * The same harness the Change screen uses: the surface is bundled and rendered
 * to static markup, and every assertion below is about the STRINGS AND
 * CONTROLS A PERSON MEETS. A second bundle is built over a PROBE copy — the
 * real `HQ_COPY` with two Inbox strings replaced — because equality with the
 * real copy cannot tell a string that came from the copy file from a literal
 * that happens to match it.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-inbox-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { InboxScreen } from './inbox.jsx';
  import { inboxItems, inboxSections, storageReading } from './inbox-view.js';
  import { HQ_COPY } from './hq-copy.js';
  export { inboxItems, inboxSections, storageReading, HQ_COPY };
  export const renderInbox = props => renderToStaticMarkup(React.createElement(InboxScreen, props));
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

const M = await load('inbox.mjs')

const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('inbox-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("title: 'What needs your attention',", "title: 'probe-inbox-title',")
        .replace("line: 'Nothing counts as done yet, so there is nothing for the work to be judged against.',", "line: 'probe-awaiting-definition',"),
      loader: 'js',
    }))
  },
}])

const { inboxItems, inboxSections, storageReading, HQ_COPY, renderInbox } = M

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/\s+/g, ' ').trim()

const SESSION_A = 'cccccccc-0000-4000-8000-00000000s001'

function aChange(over = {}) {
  return {
    id: 'cccccccc-0000-4000-8000-00000000c001',
    project_id: 'cccccccc-0000-4000-8000-00000000p001',
    opened_at: '2026-01-02T09:00:00Z',
    request: { title: 'Retry failed uploads', purpose: 'Uploads that fail once are lost.', requester: 'sam', source: 'person' },
    accepted_scope: null,
    implementation: null,
    associations: [],
    examination: 'none_yet',
    decision: 'none_yet',
    ...over,
  }
}

const accepted = (criteria) => ({
  accepted_scope: {
    version: 1, proposed_at: '2026-01-02T09:30:00Z', criteria,
    exclusions: [], allowed_scope: [], constraints: [], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T10:00:00Z', rationale: 'That is what I asked for.' },
  },
})

const linked = (ids, provenance = 'confirmed') => ({
  associations: ids.map((id, i) => ({
    id: 'cccccccc-0000-4000-8000-00000000a00' + (i + 1),
    session_id: id, provenance, basis: 'person', actor: 'sam',
    asserted_at: '2026-01-02T11:0' + i + ':00Z', reason: null,
  })),
})

const bound = {
  implementation: {
    version: 1, bound_at: '2026-01-04T09:00:00Z', repository_identity: 'uploads-api',
    base_revision: 'a1b2c3d4e5f6', snapshot_id: null, workspace_identity: null,
  },
}

const work = (id, findings) => ({ id, findings, verdict: null, focus: null, steps: 3 })
const finding = (state, kind = 'claim') => ({ id: 'f-' + state + '-' + kind, state, kind })

const STORAGE = storageReading({ bytes: 7_200_000_000, path: '/tmp/fixture-store' })

/** A ready screen over one or more Change inputs.
 *
 * The MODULE is a parameter, not just its render function: an item's sentence
 * is chosen by the model (`reasonLine`) and carried to the view as a template,
 * so a probe that swapped only the renderer would read the real copy through
 * the real model and pass while the view held a literal. */
function screen(inputs, over = {}, mod = M) {
  const { items, settled } = mod.inboxItems(inputs)
  return mod.renderInbox({
    state: {
      status: 'ready', items, settled,
      total: inputs.length, unread: 0, newest: inputs.length ? inputs[0].change.id : null,
      storage: STORAGE, notes: [], ...over,
    },
  })
}

const THREE = [
  { change: aChange({ id: 'cccccccc-0000-4000-8000-00000000c001', ...accepted(['The queue stops after three failures.', 'The log records the retry.']), ...linked([SESSION_A]), ...bound }), work: [work(SESSION_A, [finding('not_proven'), finding('couldnt')])] },
  { change: aChange({ id: 'cccccccc-0000-4000-8000-00000000c002', opened_at: '2026-01-03T09:00:00Z', request: { title: 'Sign-in rate limit', purpose: 'p', requester: 'sam', source: 'person' }, ...accepted(['Three bad attempts pause the account.']) }) },
  { change: aChange({ id: 'cccccccc-0000-4000-8000-00000000c003', opened_at: '2026-01-05T09:00:00Z', request: { title: 'Export the report', purpose: 'p', requester: 'sam', source: 'person' }, ...linked([SESSION_A], 'suggested') }) },
]

/* ── one obvious thing to do ───────────────────────────────────────────── */

test('every item carries exactly one primary action, and it is a link to the stage that settles its reason', () => {
  const html = screen(THREE)
  const items = html.split('<li class="ws-inbox-item"').slice(1)
  assert.equal(items.length, 3)
  for (const item of items) {
    const primaries = [...item.matchAll(/class="ws-inbox-primary"/g)]
    assert.equal(primaries.length, 1, `an item has ${primaries.length} primary actions: ${text(item)}`)
    assert.match(item, /<a class="ws-inbox-primary" href="#\/changes\/[^"]+\/(define|prepare|build|verify|review|approve)"/)
  }
  /* A3-F14 (report line 1181) — the order is whose turn it is first, in the
   * report's fixed section order, and the priority within a section: the
   * Change nobody has defined is waiting on YOU, so it heads the screen; then,
   * under "Needs a check", the record holding the opposite of what was said
   * comes before a line with no evidence yet. Before A3-F14 this asserted
   * priority alone across the whole list (the undefined Change last). */
  const order = text(html)
  assert.ok(order.indexOf('Export the report') < order.indexOf('Retry failed uploads'))
  assert.ok(order.indexOf('Retry failed uploads') < order.indexOf('Sign-in rate limit'))
  assert.ok(order.indexOf('Waiting on you') < order.indexOf('Export the report'))
  assert.ok(order.indexOf('Export the report') < order.indexOf('Needs a check'))
  assert.ok(order.indexOf('Needs a check') < order.indexOf('Retry failed uploads'))
  assert.match(text(items[0]), /Nothing counts as done yet, so there is nothing for the work to be judged against\./)
  assert.match(text(items[1]), /The record shows the opposite of what was said, in 1 thing about the work you linked\./)
})

test('the empty state teaches, and carries exactly one primary action', () => {
  const html = screen([], { total: 0, newest: null })
  const plain = text(html)
  assert.match(plain, /Nothing needs your attention because nothing is being tracked yet\. Start by picking what you are working on\./)
  assert.match(plain, /thruline change new --title "what the work is" --requester "your name"/)
  assert.equal([...html.matchAll(/class="ws-inbox-primary"/g)].length, 1)
  /* T-6-6: the one primary is picking a project, in the product; the command
   * is still there, as the second way, and styled as secondary. */
  assert.match(html, /<a class="ws-inbox-primary" href="#\/start">Pick what you are working on<\/a>/)
  assert.match(html, /<button type="button" class="ws-first-secondary">Copy this command<\/button>/)
})

test('Changes that need nothing are said, not hidden, and that state has its own words and its own one action', () => {
  const html = screen([{ change: aChange(accepted(['a'])), evidence: [{ criterion: 1, state: 'proven' }] }], { total: 1 })
  const plain = text(html)
  assert.match(plain, /Nothing needs your attention right now/)
  assert.match(plain, /1 Change is open and nothing on it is waiting on you\. Nothing here says the work is good: it says nothing is waiting\./)
  assert.equal([...html.matchAll(/class="ws-inbox-primary"/g)].length, 1)
  /* It is never the empty state's words: "nothing is tracked" and "nothing is
   * waiting" are different facts. */
  assert.doesNotMatch(plain, /nothing is being tracked yet/)
})

test('a Change beyond the read bound is counted and said, never silently dropped', () => {
  const html = screen(THREE, { total: 30, unread: 27 })
  assert.match(text(html), /27 more Changes are open, and this screen has not looked at them yet\./)
})

/* ── a suggestion is a suggestion ──────────────────────────────────────── */

test('a suggested piece of work is a secondary line under the action, never a reason and never a count of evidence', () => {
  const change = aChange({ ...accepted(['a']), ...linked([SESSION_A], 'suggested') })
  const html = screen([{ change, work: [work(SESSION_A, [finding('not_proven')])] }])
  const plain = text(html)
  assert.match(plain, /1 thing that counts as done has no evidence yet\./)
  assert.match(plain, /1 piece of work might belong here\. Nobody has confirmed it, so nothing from it is counted above\./)
  assert.doesNotMatch(plain, /The record shows the opposite/, 'a suggestion contributed a reason')
  /* It sits after the one action, so it cannot be read as part of why the
   * item is here. */
  assert.ok(html.indexOf('ws-inbox-suggested') > html.indexOf('ws-inbox-primary'))
  /* And the guard is live: confirm the same link and the reason changes. */
  const confirmed = screen([{ change: aChange({ ...accepted(['a']), ...linked([SESSION_A]) }), work: [work(SESSION_A, [finding('not_proven')])] }])
  assert.match(text(confirmed), /The record shows the opposite/)
  assert.doesNotMatch(text(confirmed), /might belong here/)
})

/* ── numbers ───────────────────────────────────────────────────────────── */

test('every digit on the page is inside a sentence a typed field filled', () => {
  const html = screen(THREE, { total: 30, unread: 27 })
  const { items } = inboxItems(THREE)
  /* What the model decided, plus the record's own words. Everything else is
   * this product talking, and this product may not put a number on this
   * surface (`B8.6`): no event volume, no session count, no productivity
   * statistic, no rank, no age. */
  const licensed = [
    ...items.map((i) => i.title),
    ...items.map((i) => fillOf(i.line)),
    ...items.flatMap((i) => i.also.map((a) => fillOf(a.line))),
    ...items.map((i) => i.scope.repository || ''),
    /* A3-F14: each whose-turn section's count, a typed count of the items in it. */
    ...new Set(inboxSections(items, 0).filter((x) => x.count > 0).map((x) => (x.count === 1 ? HQ_COPY.change.inbox.sections.count_one : HQ_COPY.change.inbox.sections.count_many.replace('{n}', String(x.count))))),
    /* A3-F2: the Change-level count, filled from `attentionOf(...).items`. */
    ...new Set(items.map((i) => (i.attention.items === 1 ? HQ_COPY.change.attention.items_one : HQ_COPY.change.attention.items_many.replace('{n}', String(i.attention.items))))),
    'version a1b2c3d · 2026-01-04',
    '3 Changes need your attention.',
    '1 piece of work might belong here. Nobody has confirmed it, so nothing from it is counted above.',
    '27 more Changes are open, and this screen has not looked at them yet.',
    'ThruLine is keeping 7.2 GB of records on this machine.',
    /* The technical layer, one click down: the exact byte count the server
     * measured, which is the same typed field the sentence above rounds. */
    '7200000000',
  ]
  let rest = text(html)
  for (const licence of licensed) {
    if (!licence) continue
    assert.ok(rest.includes(licence), `the page does not carry the licensed string: ${licence}`)
    rest = rest.split(licence).join(' ')
  }
  assert.doesNotMatch(rest, /\d/, `a digit on the page came from nothing typed: ${rest}`)
})

function fillOf(line) {
  return String(line.template).replace(/\{n\}/g, String(line.values.n))
}

test('the digit sweep is a live instrument', () => {
  /* The same removal over a page that also said something nobody asked for
   * must leave a digit behind. */
  const html = screen(THREE) + '<p class="ws-inbox-muted">412 events across 7 sessions this week</p>'
  let rest = text(html)
  for (const licence of ['3 Changes need your attention.', 'ThruLine is keeping 7.2 GB of records on this machine.']) rest = rest.split(licence).join(' ')
  assert.match(rest, /\d/)
})

test('the storage sentence says what the served field measured, and a different field is a different sentence', () => {
  const small = screen(THREE, { storage: storageReading({ bytes: 41_000_000, path: '/tmp/fixture-store' }) })
  assert.match(text(small), /ThruLine is keeping 41\.0 MB of records on this machine\./)
  const big = screen(THREE, { storage: storageReading({ bytes: 7_200_000_000, path: '/tmp/fixture-store' }) })
  assert.match(text(big), /ThruLine is keeping 7\.2 GB of records on this machine\./)
  /* The path and the exact byte count are the technical layer: one click,
   * never hidden, never on top (rule 1). */
  assert.match(big, /<details class="ws-inbox-technical"><summary>Where the records are kept<\/summary>/)
  assert.match(text(big), /7200000000/)
  assert.ok(big.indexOf('ws-inbox-storage-line') < big.indexOf('ws-inbox-technical'))
  /* And a size that could not be measured is said, never guessed as zero. */
  const unknown = screen(THREE, { storage: storageReading(null) })
  assert.match(text(unknown), /ThruLine could not measure how much it is keeping on this machine, so it will not guess\./)
  assert.doesNotMatch(text(unknown), /0 B|keeping 0/)
  assert.doesNotMatch(unknown, /ws-inbox-technical/)
})

/* T-2-10, finding 1: the sentence names the folder, so the number is the
 * folder's — and the half a person needs when it grows is WHICH part grew.
 * The breakdown is one click down, in the words of the copy file, and the
 * rounding the sentence does is stated there rather than assumed. */
test('the technical layer takes the number apart, in the copy file\'s words, and states the rounding', () => {
  const html = screen(THREE, { storage: storageReading({
    bytes: 3_174_400,
    path: '/tmp/fixture-store',
    parts: [
      { key: 'records', bytes: 2_109_440 },
      { key: 'preserved_transcripts', bytes: 876_544 },
      { key: 'command_log', bytes: 188_416 },
    ],
  }) })
  const S = HQ_COPY.change.inbox.storage
  assert.match(text(html), /ThruLine is keeping 3\.2 MB of records on this machine\./)
  const shown = text(html)
  for (const [noun, size] of [[S.parts.records, '2.1 MB'], [S.parts.preserved_transcripts, '876.5 KB'], [S.parts.command_log, '188.4 KB']]) {
    assert.ok(shown.includes(noun + ' \u2014 ' + size), `the breakdown is missing "${noun} \u2014 ${size}": ${shown}`)
  }
  assert.ok(shown.includes(S.technical_rounding), 'the rounding the sentence does is not stated')
  /* Every part of it is inside the technical layer, never on top of it. */
  assert.ok(html.indexOf('ws-inbox-storage-parts') > html.indexOf('ws-inbox-technical'))
})

/* T-7-1 rehearsal: with THRULINE_HOME set apart from HOME the server counts
 * the command log's folder too and serves it as `also`. The sentence is the
 * one number; the technical layer names BOTH folders and says the number is
 * both — a layer naming one folder under a number that counts two would be
 * the T-2-10 gap again, one level down. */
test('a second counted folder is named in the technical layer, and the note says the number is both', () => {
  const S = HQ_COPY.change.inbox.storage
  const two = screen(THREE, { storage: storageReading({
    bytes: 1_278_322, path: '/tmp/fixture-home/.thruline', also: '/tmp/fixture-thruline-home',
    parts: [{ key: 'records', bytes: 1_094_002 }, { key: 'command_log', bytes: 184_320 }],
  }) })
  const said = text(two)
  assert.match(said, /ThruLine is keeping 1\.3 MB of records on this machine\./)
  assert.ok(said.includes('/tmp/fixture-home/.thruline'), said)
  assert.ok(said.includes(S.technical_also_path) && said.includes('/tmp/fixture-thruline-home'), said)
  assert.ok(said.includes(S.technical_note_two), said)
  assert.ok(!said.includes(S.technical_note), 'the one-folder note under a two-folder number')
  assert.ok(two.indexOf('/tmp/fixture-thruline-home') > two.indexOf('ws-inbox-technical'), 'the second folder is on top')

  const one = text(screen(THREE, { storage: storageReading({ bytes: 1_094_002, path: '/tmp/fixture-home/.thruline' }) }))
  assert.ok(one.includes(S.technical_note), one)
  assert.ok(!one.includes(S.technical_also_path) && !one.includes(S.technical_note_two), one)
})

/* T-7-1 verifier: with THRULINE_HOME equal to HOME the command log is a file
 * in a folder that HOLDS the records folder. The server counts that one file
 * (never the home directory around it) and serves it as `also_command_log`.
 * The technical layer names the file and says the number is the folder plus
 * that file — not "every file in that folder", which would be short of the
 * number above it, and not "two folders", which would claim a home directory
 * was counted. */
test('a command log counted on its own is named in the technical layer, and the note says what was added', () => {
  const S = HQ_COPY.change.inbox.storage
  const html = screen(THREE, { storage: storageReading({
    bytes: 1_278_322, path: '/tmp/fixture-home/.thruline', also: null, also_command_log: '/tmp/fixture-home/exec.db',
    parts: [{ key: 'records', bytes: 1_094_002 }, { key: 'command_log', bytes: 184_320 }],
  }) })
  const said = text(html)
  assert.match(said, /ThruLine is keeping 1\.3 MB of records on this machine\./)
  assert.ok(said.includes(S.technical_command_log_path) && said.includes('/tmp/fixture-home/exec.db'), said)
  assert.ok(said.includes(S.technical_note_command_log), said)
  assert.ok(!said.includes(S.technical_note) && !said.includes(S.technical_note_two), said)
  assert.ok(!said.includes(S.technical_also_path), 'a file named as a second folder')
  assert.ok(html.indexOf('/tmp/fixture-home/exec.db') > html.indexOf('ws-inbox-technical'), 'the file is on top')
})

/* ── the words ─────────────────────────────────────────────────────────── */

test('no word off the banned list reaches the rendered Plain surface', () => {
  const banned = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree hash\b/i, /\bpredicates?\b/i, /\bsessions?\b/i]
  for (const html of [screen(THREE, { total: 30, unread: 27 }), screen([], { total: 0, newest: null }), screen([{ change: aChange(accepted(['a'])), evidence: [{ criterion: 1, state: 'proven' }] }], { total: 1 })]) {
    const plain = text(html)
    for (const re of banned) assert.doesNotMatch(plain, re, `the Inbox renders a banned word: ${plain.slice(0, 300)}`)
    for (const token of HQ_COPY.banned) assert.doesNotMatch(plain, new RegExp('\\b' + token + '\\b', 'i'), plain.slice(0, 300))
    /* No state word: the five describe ONE piece of evidence and the Inbox
     * shows none. And nothing here is a percentage. */
    for (const s of Object.values(HQ_COPY.states)) assert.ok(!plain.includes(s.word), `the Inbox renders the state word "${s.word}"`)
    assert.doesNotMatch(plain, /\d\s?%/)
  }
})

test('the rendered banned-word sweep is a live instrument', () => {
  const banned = [/\bevents?\b/i, /\bsessions?\b/i, /\bpacket\b/i]
  assert.ok(banned.every((re) => re.test('412 events · 7 sessions · packet v3')))
})

test('every string the screen renders comes from the copy file', () => {
  const real = text(screen(THREE))
  const probed = text(screen(THREE, {}, PROBED))
  assert.ok(real.includes('What needs your attention'), real.slice(0, 200))
  assert.ok(!probed.includes('What needs your attention'), 'the heading is a literal in the source')
  assert.ok(probed.includes('probe-inbox-title'))
  assert.ok(probed.includes('probe-awaiting-definition'), 'a reason sentence is a literal in the source')
})

test('the refused and loading states say so, and claim nothing about what is waiting', () => {
  const loading = renderInbox({ state: { status: 'loading' } })
  assert.match(loading, /role="status"/)
  assert.match(text(loading), /Looking at what needs your attention…/)
  const refused = renderInbox({ state: { status: 'refused', refusal: '/api/changes did not answer' } })
  const plain = text(refused)
  assert.match(plain, /What needs your attention could not be read: \/api\/changes did not answer\. Nothing is claimed about what is or is not waiting for you\./)
  assert.doesNotMatch(plain, /Nothing needs your attention/)
})

/* ── reachable without a mouse, and still when motion is off ────────────── */

test('every primary action is reachable by keyboard and nothing takes it out of the tab order', () => {
  const html = screen(THREE) + screen([], { total: 0, newest: null })
  for (const control of html.match(/<(a|button)[^>]*class="ws-inbox-primary"[^>]*>/g) || []) {
    assert.doesNotMatch(control, /tabindex="-1"/, control)
    assert.doesNotMatch(control, /disabled/, control)
    if (control.startsWith('<a')) assert.match(control, /href="[^"]+"/, 'a link with no address is not a control')
  }
  assert.equal((html.match(/class="ws-inbox-primary"/g) || []).length, 4)
  /* The only interactive things on this surface are those controls and the
   * technical disclosure — no div carrying an onclick, which no keyboard
   * reaches. */
  assert.doesNotMatch(html, /<div[^>]*onclick/i)
})

test('the screen has no motion, and says so where a later one would have to meet it', () => {
  const css = readFileSync(join(here, 'workspace.css'), 'utf8')
  const block = /@media \(prefers-reduced-motion: reduce\) \{\s*\.ws-inbox-primary, \.ws-inbox-technical summary \{\s*animation: none !important;\s*transition: none !important;/
  assert.match(css, block)
  const rendered = screen(THREE)
  assert.doesNotMatch(rendered, /animation|transition/i, 'the rendered surface carries an inline animation')
})

/* ── what was read, said ───────────────────────────────────────────────── */

test('anything that could not be read is said on the page rather than swallowed', () => {
  const html = screen(THREE, { notes: ['/api/storage did not answer — is `thruline ui` serving this page?'] })
  assert.match(text(html), /\/api\/storage did not answer/)
})

test('the Inbox reads and never writes: there is no POST in the file', () => {
  const source = readFileSync(join(here, 'inbox.jsx'), 'utf8')
  assert.doesNotMatch(source, /method: 'POST'/)
  assert.doesNotMatch(source, /fetch\(/, 'the read layer is the only place that fetches')
})

/* ── BUILD-14 (path step 2, PW-F3): the card names the project, not the hash ── */

/* A synthetic workspace identity in the engine's `wsid1:` shape. Not a real
 * digest of anything. */
const WSID = 'wsid1:' + 'e'.repeat(64)

const plainFace = (html) => html.replace(/<details[\s\S]*?<\/details>/g, ' ')
const technicalOf = (html) => (html.match(/<details class="ws-inbox-where-technical"[\s\S]*?<\/details>/g) || []).join(' ')

function boundAt(workspace) {
  return {
    implementation: {
      version: 1, bound_at: '2026-01-04T09:00:00Z', repository_identity: WSID,
      base_revision: 'a1b2c3d4e5f6', snapshot_id: null, workspace_identity: workspace,
    },
  }
}

test('BUILD-14: the card names the project by its recorded folder, and the wsid1 hash is one level down, never gone', () => {
  const html = screen([{ change: aChange({ ...accepted(['The queue stops after three failures.']), ...boundAt('/tmp/fixture/shop') }) }])
  const plain = text(plainFace(html))
  assert.doesNotMatch(plain, /wsid1:/, 'the hash is on the plain face of the card')
  assert.match(html, /<span class="ws-inbox-where">shop<\/span>/, 'the card does not name the project by its folder')
  const technical = text(technicalOf(html))
  assert.ok(technical.includes(WSID), 'the hash is not in the technical detail: ' + technical)
  assert.ok(technical.includes('/tmp/fixture/shop'), 'the recorded folder is not in the technical detail: ' + technical)
})

test('BUILD-14: with no folder recorded the card drops the project rather than showing the hash, and the hash is still one level down', () => {
  const html = screen([{ change: aChange({ ...accepted(['The queue stops after three failures.']), ...boundAt(null) }) }])
  assert.doesNotMatch(text(plainFace(html)), /wsid1:/)
  assert.doesNotMatch(html, /class="ws-inbox-where"/, 'a project segment with nothing to name')
  assert.ok(text(technicalOf(html)).includes(WSID))
  /* Nothing bound at all is still said in the words it always was. */
  const none = screen([{ change: aChange({ ...accepted(['The queue stops after three failures.']) }) }])
  assert.match(text(none), new RegExp(HQ_COPY.change.inbox.scope.no_repository))
  assert.equal(technicalOf(none), '')
})

/* ── BUILD-13 (path step 2, PW-F2): the one button names the next act ────── */

test('BUILD-13: a Change with no evidence yet offers a button that says what it opens, and never "See what we checked"', () => {
  const html = screen([{ change: aChange({ ...accepted(['When the cart is empty, the total shows zero.']) }) }])
  const { items } = inboxItems([{ change: aChange({ ...accepted(['When the cart is empty, the total shows zero.']) }) }])
  assert.equal(items.length, 1)
  assert.equal(items[0].reason.id, 'missing_evidence')
  /* The act the button performs: it opens the Change at the stage whose top
   * line is "1 thing still needs evidence:" (`readiness_needs_evidence_one`). */
  assert.equal(items[0].href, '#/changes/cccccccc-0000-4000-8000-00000000c001/verify')
  assert.ok(HQ_COPY.change.model.readiness.states.readiness_needs_evidence_one.includes('still needs evidence'))
  const primary = html.match(/<a class="ws-inbox-primary"[^>]*>([^<]*)<\/a>/)
  assert.ok(primary, 'no primary action on the card')
  assert.equal(primary[1], 'See what still needs evidence')
  assert.doesNotMatch(text(html), /See what we checked/)
  /* Words only: the same address as before. */
  assert.match(primary[0], /href="#\/changes\/cccccccc-0000-4000-8000-00000000c001\/verify"/)
})
