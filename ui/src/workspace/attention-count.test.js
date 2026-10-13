import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* A3-F2 — ONE attention derivation, one noun per level, and the property test.
 *
 * Report `#ui-counts`, fix-list row F2, done when (verbatim): "A property
 * test: every surface's count is equal".
 *
 * The acceptance, predeclared in the card before any code:
 *   1. exactly ONE exported function decides whether a Change needs a person
 *      and how many items on it need you; no other file computes either;
 *   2. over ≥ 200 generated stores (0–40 Changes, so the 25-Change read limit
 *      is crossed), every surface that states a Change-level attention count
 *      states the number `attentionOf` gives, over the Changes that were
 *      read; when not every Change was read, that is SAID; each Change's
 *      "items that need you" equals `attentionOf(...).items`, notes never
 *      contribute, and no rendered string pairs "open" with a notes count.
 *
 * The surfaces are RENDERED — bundled with esbuild and drawn to static
 * markup — and the numbers are read back out of the markup, so a surface
 * that printed its own number would be caught even if its model were right.
 * The stores are generated from a printed seed (`attention-count.fixture.js`);
 * a failure names the seed, and the store rebuilds exactly.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-attention-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { InboxScreen, assembleInbox, READ_LIMIT } from './inbox.jsx';
  import { attentionOf } from './attention.js';
  import { deriveChangeModel } from './change-model.js';
  import { HQ_COPY } from './hq-copy.js';
  import { aStore, aWork } from './attention-count.fixture.js';
  export { assembleInbox, READ_LIMIT, attentionOf, deriveChangeModel, HQ_COPY, aStore, aWork };
  export const renderInbox = (state) => renderToStaticMarkup(React.createElement(InboxScreen, { state }));
`

const result = await build({
  bundle: true, write: false, format: 'esm', platform: 'node',
  stdin: { contents: ENTRY, resolveDir: here },
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const bundled = join(temporary, 'attention.mjs')
await writeFile(bundled, result.outputFiles[0].text)
const M = await import(pathToFileURL(bundled).href)
const { HQ_COPY } = M

/* ── reading numbers back out of rendered markup ───────────────────────── */

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
const texts = (html, cls) => [...html.matchAll(new RegExp(`<[a-z0-9]+ class="(?:[^"]* )?${cls}(?: [^"]*)?"[^>]*>([\\s\\S]*?)</[a-z0-9]+>`, 'g'))].map((m) => unescape(m[1].replace(/<[^>]+>/g, '')))
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** The number a one/many pair of copy strings states, or null when the text is neither. */
function numberFrom(text, one, many) {
  if (text === one) return 1
  const m = new RegExp('^' + escapeRe(many).replace(escapeRe('{n}'), '(\\d+)') + '$').exec(text)
  return m ? Number(m[1]) : null
}

const I = () => HQ_COPY.change.inbox
const A = () => HQ_COPY.change.attention

/** The Inbox heading's number: its count line, or 0 when it states that nothing is waiting. */
function inboxHeadingNumber(html) {
  const lines = texts(html, 'ws-inbox-count')
  if (!lines.length) return 0
  assert.equal(lines.length, 1, 'the Inbox states its count once')
  const n = numberFrom(lines[0], I().one, I().many)
  assert.notEqual(n, null, `the Inbox count line is one of its two copy strings: ${lines[0]}`)
  return n
}

/** What the test itself expects, from the one derivation over the Changes the Inbox reads. */
function expected(store) {
  const sorted = store.all.slice().sort((a, b) => String(b.opened_at).localeCompare(String(a.opened_at)))
  const read = sorted.slice(0, M.READ_LIMIT)
  const per = read.map((row) => {
    const d = store.docsById.get(row.id)
    const hq = M.deriveChangeModel(d.change, d.coverage, d.applicability, d.proposals, d.work)
    const a = M.attentionOf({ change: d.change, proposals: d.proposals, work: d.work, applicability: [], hq })
    return { id: row.id, a, hq }
  })
  return { per, needing: per.filter((p) => p.a.needsPerson).length, unread: sorted.length - read.length }
}

const SEEDS = Array.from({ length: 240 }, (_, k) => 1000 + k)

/* ── 1. one derivation ─────────────────────────────────────────────────── */

test('F2 · exactly one file exports the attention derivation, and nothing else decides whether a Change needs a person', () => {
  const offenders = []
  const definers = []
  for (const f of readdirSync(here)) {
    if (!/\.(jsx?|mjs)$/.test(f) || /\.test\.|\.fixture\./.test(f)) continue
    const src = readFileSync(join(here, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:\\])\/\/.*$/gm, '$1')
    if (/export function attentionOf\b/.test(src)) definers.push(f)
    if (f === 'attention.js') continue
    if (/function (reasonsFor|findingCounts|attentionOf)\b/.test(src)) offenders.push(`${f}: defines a second attention derivation`)
    if (/needsPerson\s*[:=]/.test(src)) offenders.push(`${f}: decides needsPerson itself`)
    if (/\breasonsFor\s*\(/.test(src)) offenders.push(`${f}: calls reasonsFor directly instead of attentionOf`)
  }
  assert.deepEqual(definers, ['attention.js'], 'one exported attentionOf, in attention.js')
  assert.deepEqual(offenders, [])
  const inboxView = readFileSync(join(here, 'inbox-view.js'), 'utf8')
  assert.match(inboxView, /const attention = attentionOf\(i\)\s*\n\s*if \(!attention\.needsPerson\) return null/, 'the Inbox lists a Change on attentionOf’s decision')
})

/* ── 2. the property ───────────────────────────────────────────────────── */

test(`F2 · property over ${SEEDS.length} generated stores: the Inbox heading states the number of read Changes attentionOf says need a person`, () => {
  let crossed = 0
  let nonzero = 0
  for (const seed of SEEDS) {
    const store = M.aStore(seed)
    const want = expected(store)
    const state = { ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] }
    const html = M.renderInbox(state)
    const got = inboxHeadingNumber(html)
    assert.equal(got, want.needing, `seed ${seed}: the Inbox heading says ${got}; attentionOf says ${want.needing} of the ${want.per.length} read Changes need a person`)
    assert.equal(state.items.length, want.needing, `seed ${seed}: the Inbox's items are the Changes attentionOf lists`)
    if (want.needing) nonzero += 1
    /* Not every Change was read: the count is stated as covering what was
     * read, never as a total — the unread line is on the screen. */
    const unread = texts(html, 'ws-inbox-muted').filter((t) => numberFrom(t, I().unread_one, I().unread_many) != null)
    if (want.unread) {
      crossed += 1
      assert.equal(unread.length, 1, `seed ${seed}: ${want.unread} Changes were not read and the screen does not say so`)
      assert.equal(numberFrom(unread[0], I().unread_one, I().unread_many), want.unread, `seed ${seed}: the unread line's number`)
    } else {
      assert.equal(unread.length, 0, `seed ${seed}: every Change was read, and the screen claims some were not`)
    }
  }
  assert.ok(crossed >= 20, `the read limit was crossed in only ${crossed} stores`)
  assert.ok(nonzero >= 100, `only ${nonzero} stores had a Change needing a person`)
})

test('F2 · property: for each Change, the items the Inbox row states equal attentionOf(...).items and the Change model’s own count', () => {
  let rows = 0
  for (const seed of SEEDS) {
    const store = M.aStore(seed)
    const want = expected(store)
    const byId = new Map(want.per.map((p) => [p.id, p]))
    const state = { ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] }
    for (const item of state.items) {
      const p = byId.get(item.id)
      assert.ok(p, `seed ${seed}: the Inbox lists ${item.id}, which it did not read`)
      assert.ok(p.hq.attention, `seed ${seed}: the Change model states no attention count`)
      assert.equal(p.hq.attention.items, p.a.items, `seed ${seed} ${item.id}: the Change model's items`)
      assert.equal(item.attention.items, p.a.items, `seed ${seed} ${item.id}: the Inbox row's items`)
      assert.ok(p.a.items >= 1, `seed ${seed} ${item.id}: a Change that needs a person has at least one item`)
      /* notes never contribute: the count is the sum of the reasons alone. */
      const fromReasons = p.a.reasons.reduce((s, r) => s + (typeof r.values.n === 'number' ? r.count : 1), 0)
      assert.equal(p.a.items, fromReasons, `seed ${seed} ${item.id}: items is the reasons' sum and nothing else`)
      /* And what the row PRINTS is that number, in the Change-level noun. */
      const html = M.renderInbox({ status: 'ready', items: [item], settled: 0, total: 1, unread: 0, storage: { known: false }, notes: [] })
      const printed = texts(html, 'ws-inbox-attention')
      assert.equal(printed.length, 1, `seed ${seed} ${item.id}: the row states its items once`)
      assert.equal(numberFrom(printed[0], A().items_one, A().items_many), p.a.items, `seed ${seed} ${item.id}: the row's printed items`)
      rows += 1
    }
  }
  assert.ok(rows >= 1000, `only ${rows} rows were compared`)
})

test('F2 · no rendered Inbox string, and no copy string, pairs the word "open" with a notes count', () => {
  const pairs = /\bnotes?\b[^.]*\bopen\b|\bopen\b[^.]*\bnotes?\b/i
  for (const seed of SEEDS.slice(0, 60)) {
    const store = M.aStore(seed)
    const html = M.renderInbox({ ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] })
    for (const t of unescape(html.replace(/<[^>]+>/g, '\n')).split('\n')) {
      if (/\d/.test(t)) assert.doesNotMatch(t, pairs, `seed ${seed}: ${t}`)
    }
  }
  const walk = (o, at) => Object.entries(o).forEach(([k, v]) => {
    if (v && typeof v === 'object') walk(v, `${at}.${k}`)
    else if (typeof v === 'string' && /\{n\}|\d/.test(v)) assert.doesNotMatch(v, pairs, `${at}.${k}: ${v}`)
  })
  walk(HQ_COPY, 'HQ_COPY')
})

/* ── A3-F14: the list of Changes agrees with the Inbox ─────────────────── */
/* Report fix-list row F14, done when (verbatim): "Picker count = Inbox
 * count". The product's list of Changes is the way-in strip ("Work you are
 * being asked to accept"); it is rendered from the SAME reader's result the
 * Inbox renders, and read back out of its markup. */

const W = () => HQ_COPY.change.wayin

/** Every number the strip's count sentence states, by placeholder ({total} open, {n} needing you, {looked} looked at), or null when it is none of its templates. */
function stripCount(html) {
  const lines = texts(html, 'ws-wayin-count')
  assert.equal(lines.length, 1, 'the strip states its count once')
  for (const template of Object.values(W().count)) {
    const names = []
    const re = new RegExp('^' + escapeRe(template).replace(/\\\{(n|total|looked)\\\}/g, (_, k) => { names.push(k); return '(\\d+)' }) + '$')
    const m = re.exec(lines[0])
    if (!m) continue
    const out = { template }
    names.forEach((k, i) => { out[k] = Number(m[i + 1]) })
    return out
  }
  return null
}

test(`F14 · property over ${SEEDS.length} generated stores: the list of Changes says the same number need a person as the Inbox heading`, async () => {
  const W14 = await import(pathToFileURL(await bundleStrip()).href)
  for (const seed of SEEDS) {
    const store = M.aStore(seed)
    if (!store.all.length) continue
    const inbox = { ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] }
    const heading = inboxHeadingNumber(M.renderInbox(inbox))
    const html = W14.renderStrip({ status: 'ready', ...W14.wayInRows(inbox) })
    const said = stripCount(html)
    assert.ok(said, `seed ${seed}: the strip's count is not one of its copy strings: ${texts(html, 'ws-wayin-count')[0]}`)
    const m = said.n != null ? said.n : /needs your attention|need your attention/.test(said.template) ? 1 : 0
    assert.equal(m, heading, `seed ${seed}: the strip says ${m} need a person; the Inbox says ${heading}`)
    assert.equal(said.total != null ? said.total : 1, store.all.length, `seed ${seed}: the strip's total`)
    const want = expected(store)
    if (want.unread) {
      assert.equal(said.looked, want.per.length, `seed ${seed}: the strip says how many it looked at`)
      const unread = texts(html, 'ws-wayin-muted').filter((t) => numberFrom(t, I().unread_one, I().unread_many) != null)
      assert.equal(unread.length, 1, `seed ${seed}: the strip does not say that ${want.unread} were not looked at`)
    }
  }
})

test('F14 · property: every row on the list of Changes states the sentence the Change’s own headline states', async () => {
  const W14 = await import(pathToFileURL(await bundleStrip()).href)
  let compared = 0
  for (const seed of SEEDS) {
    const store = M.aStore(seed)
    const inbox = { ...M.assembleInbox(store.all, store.docsById), storage: { known: false }, notes: [] }
    const { rows } = W14.wayInRows(inbox)
    const html = W14.renderStrip({ status: 'ready', ...W14.wayInRows(inbox) })
    const printed = texts(html, 'ws-wayin-conclusion')
    assert.equal(printed.length, rows.length, `seed ${seed}: one sentence per row`)
    rows.forEach((row, k) => {
      const d = store.docsById.get(row.id)
      const headline = M.deriveChangeModel(d.change, d.coverage, d.applicability, d.proposals, d.work).headline.text
      assert.equal(printed[k], headline, `seed ${seed} ${row.id}: the strip says "${printed[k]}"; the Change's own headline says "${headline}"`)
      compared += 1
    })
  }
  assert.ok(compared >= 500, `only ${compared} rows were compared`)
})

let stripBundle = null
async function bundleStrip() {
  if (stripBundle) return stripBundle
  const out = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    stdin: {
      contents: `
        import React from 'react';
        import { renderToStaticMarkup } from 'react-dom/server';
        import { ChangeWayIn, wayInRows } from './change-way-in.jsx';
        export { wayInRows };
        export const renderStrip = (state) => renderToStaticMarkup(React.createElement(ChangeWayIn, { state }));
      `,
      resolveDir: here,
    },
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  })
  stripBundle = join(temporary, 'strip.mjs')
  await writeFile(stripBundle, out.outputFiles[0].text)
  return stripBundle
}

/* ── A3-NOTES-NEVER-OPEN: notes never contribute ───────────────────────── */

test(`NOTES · property over ${SEEDS.length} generated stores: taking every info-severity finding away changes no Change's items, and no Change enters or leaves the Inbox`, () => {
  let withInfo = 0
  for (const seed of SEEDS) {
    const store = M.aStore(seed)
    const stripped = new Map()
    for (const [id, d] of store.docsById) {
      const work = d.work.map((w) => {
        const infos = w.session.claims.reduce((n, c) => n + c.findings.filter((f) => f.severity === 'info').length, 0)
        withInfo += infos
        return M.aWork({ ...w.session, claims: w.session.claims.map((c) => ({ ...c, findings: c.findings.filter((f) => f.severity !== 'info') })) })
      })
      stripped.set(id, { ...d, work })
    }
    const a = M.assembleInbox(store.all, store.docsById)
    const b = M.assembleInbox(store.all, stripped)
    assert.deepEqual(a.items.map((i) => i.id), b.items.map((i) => i.id), `seed ${seed}: an info finding put a Change in the Inbox`)
    a.items.forEach((item, k) => assert.equal(item.attention.items, b.items[k].attention.items, `seed ${seed} ${item.id}: info findings changed the items count`))
  }
  assert.ok(withInfo >= 200, `only ${withInfo} info findings were generated`)
})
