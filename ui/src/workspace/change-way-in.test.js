import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* T-2-4 fix round 1 — the door to the Change workspace.
 *
 * WHAT THIS FILE IS FOR. The verifier failed T-2-4 on one measurement: the
 * app's landing page contained zero occurrences of the word "changes" and no
 * link to `#/changes/`, so the screen the card built could be reached only by
 * typing an id nothing had shown you. These tests hold the door open — one of
 * them reads `shell.jsx` itself, because a component that renders perfectly
 * and is mounted nowhere is exactly the defect that was shipped.
 *
 * Same harness as the Change screen's own tests: the surface is bundled and
 * rendered to static markup, and every assertion is about the strings and
 * controls a person meets. The fixture Changes are synthetic.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-wayin-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { ChangeWayIn, wayInRows, changeHref, SHOWN } from './change-way-in.jsx';
  import { assembleInbox } from './inbox.jsx';
  import { deriveChangeModel } from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  export { wayInRows, changeHref, SHOWN, HQ_COPY, assembleInbox, deriveChangeModel };
  export const render = props => renderToStaticMarkup(React.createElement(ChangeWayIn, props));
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

const M = await load('wayin.mjs')
const { wayInRows, changeHref, SHOWN, HQ_COPY, render, assembleInbox, deriveChangeModel } = M

/* The probe: the real copy file with two of this strip's strings swapped. A
 * surface that reads them from HQ_COPY renders the probe; one carrying a
 * literal renders the real words and fails here. */
const COPY_FILE = join(here, 'hq-copy.js')
const PROBED = await load('wayin-probe.mjs', [{
  name: 'probe-copy',
  setup(b) {
    b.onLoad({ filter: /hq-copy\.js$/ }, () => ({
      contents: readFileSync(COPY_FILE, 'utf8')
        .replace("heading: 'Work you are being asked to accept'", "heading: 'probe-wayin-heading'")
        .replace("copy_command: 'Copy this command'", "copy_command: 'probe-wayin-copy'"),
      loader: 'js',
    }))
  },
}])

const SOURCE = readFileSync(join(here, 'change-way-in.jsx'), 'utf8')
const SHELL = readFileSync(join(here, 'shell.jsx'), 'utf8')

const change = (id, title, over = {}) => ({
  id,
  project_id: 'cccccccc-0000-4000-8000-00000000p001',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title, purpose: '', requester: 'sam', source: 'person' },
  accepted_scope: null,
  implementation: null,
  associations: [],
  examination: 'none_yet',
  decision: 'none_yet',
  ...over,
})

const ACCEPTED = {
  version: 1,
  criteria: ['A failed upload is tried again three times.', 'The person is told when it finally gives up.'],
  exclusions: [], allowed_scope: [], constraints: [], check_profile: [],
  content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
  acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: '' },
  proposed_at: '2026-01-02T09:05:00Z',
}

const text = (html) => html.replace(/<[^>]*>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ').trim()
/* A3-F14: the strip renders from what the Inbox's reader returned — the list
 * plus each Change's documents — never from the list alone. Every Change
 * here is read with no coverage and no linked work. */
const docsOf = (c) => ({ status: 'ready', change: c, proposals: null, coverage: null, applicability: null, work: [], notes: [] })
const inboxOf = (doc) => {
  const all = doc && Array.isArray(doc.changes) ? doc.changes : []
  return assembleInbox(all, new Map(all.filter((c) => c && c.id).map((c) => [c.id, docsOf(c)])))
}
const ready = (doc) => ({ status: 'ready', ...wayInRows(inboxOf(doc)) })

/* ── the door itself (T24-1) ───────────────────────────────────────────── */

test('the landing mounts the way in: the Change screen is reachable without typing an address', () => {
  /* This is the assertion the shipped defect would have failed. A component
     that renders and is mounted nowhere is what was measured as "zero
     occurrences of changes in the landing DOM". */
  assert.match(SHELL, /import ChangeWayInLive from '\.\/change-way-in\.jsx'/, 'the landing does not import the way in')
  assert.match(SHELL, /<ChangeWayInLive \/>/, 'the landing never renders the way in')
  const root = SHELL.indexOf('<div className="ws-root"')
  const mounted = SHELL.indexOf('<ChangeWayInLive />')
  const bar = SHELL.indexOf('<AppBar')
  assert.ok(root > 0 && mounted > root && mounted < bar, 'the way in is not at the top of the page')
  /* And above the demo banner. Under it, "Demo session — a bundled run, not
     your project" sat directly over a list of the person's own Changes. */
  const banner = SHELL.indexOf('<DemoBanner')
  assert.ok(banner > mounted, 'the demo banner is hung over the list of real Changes')
})

test('every listed Change is a link to its own screen', () => {
  const html = render({ state: ready({ changes: [change('chg_A', 'Retry failed uploads'), change('chg_B', 'Stop double charging')] }) })
  assert.ok(html.includes('href="#/changes/chg_A"'), 'the first Change is not a link')
  assert.ok(html.includes('href="#/changes/chg_B"'))
  assert.equal(changeHref('a b/c'), '#/changes/a%20b%2Fc', 'an id with punctuation in it builds a broken address')
})

test('the strip names each Change in its own words and says what it is waiting for', () => {
  const c = change('chg_A', 'Retry failed uploads', { accepted_scope: ACCEPTED })
  const body = text(render({ state: ready({ changes: [c] }) }))
  assert.ok(body.includes('Work you are being asked to accept'))
  assert.ok(body.includes('Retry failed uploads'))
  /* The same sentence the Change's own top says. Two places deciding a
     Change's conclusion separately is two places that can disagree in front
     of a person. A3-F14: equal to the Change model's headline, not merely a
     sentence of the same shape. */
  const headline = deriveChangeModel(c, null, null, null, []).headline.text
  assert.equal(headline, 'Accepted: 2 things count as done. None has evidence yet.')
  assert.ok(body.includes(headline))
  assert.ok(body.includes('1 Change is open, and it needs your attention.'))
})

test('A3-F14 · a Change whose every accepted line is proven reads as proven on the strip, as on its own screen', () => {
  const c = change('chg_P', 'Retry failed uploads', { accepted_scope: ACCEPTED, implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-01-02T09:10:00Z', repository_identity: 'shop' } })
  const item = (n) => ({ link_id: 'l' + n, evidence_id: 'e' + n, session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass', observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies' })
  const coverage = { scope_version: 1, link_rows: 2, criteria: ACCEPTED.criteria.map((t, k) => ({ index: k + 1, text: t, state: 'evidence', items: [item(k + 1)] })) }
  const docs = { ...docsOf(c), coverage }
  const body = text(render({ state: { status: 'ready', ...wayInRows(assembleInbox([c], new Map([[c.id, docs]]))) } }))
  const headline = deriveChangeModel(c, coverage, null, null, []).headline.text
  assert.doesNotMatch(headline, /None has evidence yet/)
  assert.ok(body.includes(headline), `the strip does not state the Change's own headline: ${headline}`)
  assert.doesNotMatch(body, /None has evidence yet/, 'the strip answers from a document it was never given')
})

test('newest first, five at most, and the rest are counted rather than dropped', () => {
  const many = []
  for (let i = 1; i <= 8; i += 1) many.push(change('chg_' + i, 'Change ' + i, { opened_at: `2026-01-0${i > 8 ? 8 : i}T09:00:00Z` }))
  const { rows, more } = wayInRows(inboxOf({ changes: many }))
  assert.equal(rows.length, SHOWN)
  assert.equal(more, 3)
  assert.deepEqual(rows.map((r) => r.id), ['chg_8', 'chg_7', 'chg_6', 'chg_5', 'chg_4'], 'the newest Changes are not first')
  const body = text(render({ state: ready({ changes: many }) }))
  assert.ok(body.includes('3 more Changes are open and not listed here.'), 'the ones not shown are not accounted for')
  assert.ok(body.includes('8 Changes are open, and 8 of them need your attention.'))
})

/* ── rule 12: an empty strip teaches, and offers the one act that fills it ─ */

test('with nothing open it says what would put something there, and gives the exact command', () => {
  const body = text(render({ state: ready({ changes: [] }) }))
  assert.ok(body.includes(HQ_COPY.change.wayin.empty))
  assert.ok(body.includes('thruline change new --title "what the work is" --requester "your name"'))
  assert.ok(body.includes('Copy this command'))
  /* The command is the one the CLI actually takes. A copyable command that
     does not run is worse than none. */
  assert.match(HQ_COPY.change.wayin.command, /^thruline change new /)
})

test('a read that failed says so, with the reader’s own reason, and claims nothing either way', () => {
  const body = text(render({ state: { status: 'refused', refusal: 'HTTP 500' } }))
  assert.ok(body.includes('HTTP 500'))
  assert.ok(body.includes('Nothing is claimed about what is or is not waiting for you.'))
  assert.ok(!body.includes(HQ_COPY.change.wayin.empty), 'a failed read was rendered as an empty list')
})

test('while it is reading it says it is reading, and shows no list', () => {
  const body = text(render({ state: { status: 'loading' } }))
  assert.ok(body.includes(HQ_COPY.change.wayin.reading))
  assert.ok(!body.includes('Work you are being asked to accept'))
})

/* ── the words, and what may never be among them ───────────────────────── */

test('every string on the strip comes from the copy file', () => {
  const html = PROBED.render({ state: { status: 'ready', ...PROBED.wayInRows({ changes: [] }) } })
  assert.ok(text(html).includes('probe-wayin-heading'), 'the heading is a literal in the view')
  assert.ok(text(html).includes('probe-wayin-copy'), 'the copy control’s label is a literal in the view')
})

test('no word off the banned list reaches the strip, and nothing is a state word', () => {
  const rendered = [
    render({ state: ready({ changes: [change('chg_A', 'Retry failed uploads', { accepted_scope: ACCEPTED })] }) }),
    render({ state: ready({ changes: [] }) }),
    render({ state: { status: 'refused', refusal: 'HTTP 500' } }),
  ].map(text).join(' ')
  const banned = [/\bevents?\b/i, /\breceipts?\b/i, /\bcandidates?\b/i, /\bapplicability\b/i, /\bdispositions?\b/i, /\bpacket\b/i, /\bledger\b/i, /\bsession id\b/i, /\btree hash\b/i, /\bpredicates?\b/i]
  for (const re of banned) assert.doesNotMatch(rendered, re, `a word Sam has never heard reached the strip: ${re}`)
  /* The sweep is a live instrument: each pattern is shown to match the thing
     it exists to catch, so a pattern that silently stops matching fails here
     rather than passing forever. */
  const leak = 'read from the ledger: 3 events, two receipts, the candidate tree, its applicability, the disposition, packet v2, session id 44, tree hash e71c, a predicate'
  for (const re of banned) assert.match(leak, re, `the sweep would miss ${re}`)
  for (const word of Object.values(HQ_COPY.states).map((s) => s.word)) {
    assert.ok(!rendered.includes(word), `the strip renders the state word "${word}", which describes one piece of evidence and never a Change`)
  }
  assert.doesNotMatch(rendered, /\bgreen\b/i)
})

test('nothing on the strip is a badge, and at most one control is primary', () => {
  for (const state of [ready({ changes: [change('chg_A', 'Retry failed uploads')] }), ready({ changes: [] })]) {
    const html = render({ state })
    assert.ok(!/class="[^"]*badge/i.test(html), 'a badge collapses two dimensions into one mark')
    const primaries = html.match(/ws-wayin-primary/g) || []
    assert.ok(primaries.length <= 1, `${primaries.length} primary controls compete on the landing`)
  }
})

/* ── it reads, and it can never write ──────────────────────────────────── */

test('the way in writes nothing: one GET-only reader, no POST, no write route', () => {
  assert.ok(!/method:\s*'POST'/.test(SOURCE), 'a POST reached the landing strip')
  assert.ok(!SOURCE.includes('/associations') && !SOURCE.includes('/scope/accept'), 'a write route is named in the landing strip')
  /* A3-F14 (a TEST-WEAKENING commit, reason in its message). This asserted
   * exactly ONE read, `/api/changes`. For each row to state the sentence the
   * Change's own headline states, the strip has to read what the Change
   * screen reads — so it reads through the Inbox's reader, `readInbox`, which
   * reads every open Change's documents. The bound is now one READER, not one
   * read; and that reader is held to the same rule: no POST, and it reads the
   * Change index route. */
  const direct = SOURCE.match(/readProjection\(/g) || []
  const viaInbox = SOURCE.match(/readInbox\(/g) || []
  assert.equal(direct.length + viaInbox.length, 1, 'the strip reads through more than one reader')
  if (direct.length) assert.ok(SOURCE.includes('API.changes'), 'the strip does not read the Change index route')
  if (viaInbox.length) {
    const INBOX = readFileSync(join(here, 'inbox.jsx'), 'utf8')
    assert.ok(!/method:\s*'POST'/.test(INBOX), 'the reader the strip reads through writes')
    assert.ok(INBOX.includes('readProjection(API.changes)'), 'the reader the strip reads through does not read the Change index route')
  }
})

test('a malformed or empty document is a list of nothing, never a crash and never an invention', () => {
  for (const doc of [null, undefined, {}, { changes: null }, { changes: [{}, { id: '' }] }]) {
    const { rows, more } = wayInRows(doc)
    assert.deepEqual(rows, [])
    assert.equal(more, 0)
  }
})
