/* C7 — the Inbox IA.
 *
 * Two things are pinned here. The **order rule** (C7-O1), because a queue whose
 * order is incidental reshuffles when the export's row order moves. And the
 * **empty state**, because "Everything checks out" is a claim: this suite fails
 * if the surface can reach that sentence without every project having been
 * evaluated.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import { ACTIONABLE_KINDS, CALM_KIND, ORDER_RULE, calmIsEarned, hasReceipt, inboxQueue } from './inbox.js'

const here = dirname(fileURLToPath(import.meta.url))
const src = (rel) => readFileSync(join(here, '..', rel), 'utf8')
const REPRESENTATIVE_FIXTURE = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

/* A project row shaped exactly as the §9 index emits it — four keys per item
   and nothing deeper, which is what the Part 0 probe measured. */
const proj = (name, fixture, items) => ({
  name,
  fixture,
  attention: {
    headline: '',
    needing_attention: items.filter((i) => i.kind !== CALM_KIND).length,
    items,
  },
})
const item = (kind, id) => ({ kind, id, sentence: `s:${id}`, action: `a:${id}` })
const calm = (id) => item(CALM_KIND, id)

test('the order rule is named, and the queue obeys it', () => {
  assert.equal(ORDER_RULE, 'C7-O1')
  const index = {
    projects: [
      proj('zeta', 'z', [item('not_read', 'att_9')]),
      proj('alpha', 'a', [item('capture_attention', 'att_5')]),
      proj('mid', 'm', [item('contradiction', 'att_7'), item('staleness', 'att_2')]),
      proj('beta', 'b', [item('contradiction', 'att_1')]),
    ],
  }
  const q = inboxQueue(index)
  assert.deepEqual(
    q.rows.map((r) => [r.kind, r.project, r.id]),
    [
      // 1. kind rank, in ACTIONABLE_KINDS order …
      ['contradiction', 'beta', 'att_1'],
      ['contradiction', 'mid', 'att_7'],
      // … 2. then project name inside a kind …
      ['staleness', 'mid', 'att_2'],
      ['capture_attention', 'alpha', 'att_5'],
      ['not_read', 'zeta', 'att_9'],
    ],
    'the queue is not ordered by C7-O1',
  )
})

test('the order rule breaks every tie, so input order cannot leak into the queue', () => {
  // Same kind, same project: only the id can separate these. If the comparator
  // ever falls through, the two orderings below stop matching.
  const rows = [item('contradiction', 'att_b'), item('contradiction', 'att_a')]
  const forward = inboxQueue({ projects: [proj('p', 'p', rows)] })
  const reversed = inboxQueue({ projects: [proj('p', 'p', [...rows].reverse())] })
  assert.deepEqual(
    forward.rows.map((r) => r.id),
    ['att_a', 'att_b'],
  )
  assert.deepEqual(
    forward.rows.map((r) => r.id),
    reversed.rows.map((r) => r.id),
    'the queue order depends on the order the index happened to arrive in',
  )
})

test('calm is never a row — it is the reason there are none', () => {
  const q = inboxQueue({ projects: [proj('p', 'p', [calm('att_c')])] })
  assert.equal(q.rows.length, 0, 'the calm item was listed as work')
  assert.equal(q.calm, 1)
  assert.ok(calmIsEarned(q))
})

test('a kind this build does not know still reaches the person, ranked last', () => {
  const q = inboxQueue({
    projects: [
      proj('p', 'p', [item('some_future_kind', 'att_x'), item('contradiction', 'att_y')]),
    ],
  })
  assert.deepEqual(
    q.rows.map((r) => r.kind),
    ['contradiction', 'some_future_kind'],
    'an unrecognised kind was dropped or misranked',
  )
  assert.deepEqual(q.unrecognized, ['some_future_kind'], 'the unknown kind was not surfaced')
})

test('"never evaluated" is work, and it is not calm', () => {
  const q = inboxQueue({ projects: [proj('p', 'p', [item('not_read', 'att_n')])] })
  assert.equal(q.rows.length, 1)
  assert.equal(q.calm, 0)
  assert.equal(calmIsEarned(q), false, 'an unread project cannot produce a calm screen')
})

test('a project that returned no item at all is surfaced, never counted as calm', () => {
  // The §9 contract says every project carries at least one item. If that ever
  // stops being true, the failure must NOT arrive as a calm screen — that is
  // precisely how "everything checks out" once shipped over 43 unexamined
  // projects (A93/A94).
  const q = inboxQueue({
    projects: [proj('quiet', 'q', []), proj('fine', 'f', [calm('att_c')])],
  })
  assert.deepEqual(q.silent, ['q'], 'a project with no items vanished')
  assert.equal(q.calm, 1, 'the silent project was counted as calm')
  assert.equal(calmIsEarned(q), false, 'a silent project must block the strong sentence')
})

test('an empty index earns nothing', () => {
  const q = inboxQueue({ projects: [] })
  assert.equal(calmIsEarned(q), false, 'zero projects is not "everything checks out"')
  assert.equal(calmIsEarned(inboxQueue(undefined)), false)
})

test('the Inbox writes no English — every sentence comes from the index', () => {
  const text = src('lib/inbox.js').replace(/\/\*[\s\S]*?\*\//g, '')
  for (const [, quoted] of text.matchAll(/'([^'\n]{4,})'/g)) {
    assert.ok(
      !quoted.includes(' '),
      `inbox.js composes prose (${quoted}); sentences belong to the voice authority`,
    )
  }
})

test('there is exactly one attention queue in the app', () => {
  // Mission Control's "Needs attention" card is this surface's ancestor. After
  // C7 it must be gone, not duplicated: two queues over one derivation is two
  // places for the same fact to disagree.
  // Comments are stripped first: this bans the rendered card, not the prose
  // recording why it is gone. The house convention (arcade.test.js) is the
  // same — a structural walk that convicts its own changelog teaches people to
  // delete the changelog.
  const shell = src('os/shell.jsx').replace(/\/\*[\s\S]*?\*\//g, '')
  const reads = [...shell.matchAll(/attention[?.]*\.items/g)]
  assert.equal(
    reads.length,
    0,
    'os/shell.jsx reads attention items directly; the queue must come from inbox.js',
  )
  assert.ok(shell.includes('inboxQueue'), 'the shell does not use the one queue')
  assert.ok(
    !/Needs attention/.test(shell),
    'the ancestor "Needs attention" card is still rendering alongside the Inbox',
  )
})

/* A188 — the exact-claim deep link. The dangerous half is the absence: four of
   the five attention kinds carry no claim, and a row that offers a receipt it
   cannot open is worse than one that stays text. */

test('a row carries a receipt only when the index gave it one', () => {
  const withNode = { ...item('contradiction', 'att_1'), node_id: 'node-7' }
  const without = item('not_read', 'att_2')
  const q = inboxQueue({
    projects: [proj('p', 'p', [withNode]), proj('q', 'q', [without])],
  })
  const [c, n] = q.rows
  assert.equal(c.nodeId, 'node-7')
  assert.ok(hasReceipt(c), 'a claim-bearing row must reach its claim')
  assert.equal(n.nodeId, undefined, 'absence must survive the queue as undefined')
  assert.ok(!hasReceipt(n), 'a project-level row must not offer a receipt')
})

test('an empty or null node_id is not a receipt', () => {
  // The export emits the key only when it means it. If a future producer ever
  // emits a placeholder, the UI must treat it as absence rather than link to
  // nothing — that is the whole failure mode this guards.
  for (const bad of ['', null, undefined, 0, false]) {
    const q = inboxQueue({
      projects: [proj('p', 'p', [{ ...item('contradiction', 'att_x'), node_id: bad }])],
    })
    assert.ok(!hasReceipt(q.rows[0]), `a node_id of ${JSON.stringify(bad)} must not be a link`)
  }
})

test('the receipt link is wired to the claim, not just the project', () => {
  const shell = src('os/shell.jsx').replace(/\/\*[\s\S]*?\*\//g, '')
  assert.match(
    shell,
    /onTrace\(r\.fixture, r\.nodeId\)/,
    'the receipt affordance must pass the node through, not only the project',
  )
  assert.match(shell, /hasReceipt\(r\)/, 'the affordance must be gated on having a claim')
  assert.match(
    shell,
    /initialNode=\{traceNode\}/,
    'Trace must receive the claim to open',
  )
  // And a deep link that does not resolve fails visibly, never silently.
  const mac = src('mac/shell.jsx')
  assert.match(mac, /deepLinkMiss/, 'a missed deep link must be represented')
  assert.ok(
    /could not be opened/.test(mac),
    'a missed deep link must say so on screen',
  )
})

test('the graph is called Trace on every rendered surface', () => {
  // NORTHSTAR-v3: the graph is Trace, the investigation lens inside a session,
  // never the homepage. "Decision Graph" was RA6-era chrome and is dead as a
  // label. The route key and the ?project= deep link deliberately stay 'graph'
  // — an address is not a label, and renaming it breaks links without making
  // one sentence truer.
  for (const file of ['os/shell.jsx', 'main.jsx', 'mac/shell.jsx']) {
    assert.ok(
      !/Decision Graph/i.test(src(file)),
      `${file} still renders or documents the dead "Decision Graph" label`,
    )
  }
  const shell = src('os/shell.jsx').replace(/\/\*[\s\S]*?\*\//g, '')
  assert.ok(/\['graph', 'Trace'\]/.test(shell), 'the graph tab is not labelled Trace')
  assert.ok(
    !/the graph tab'/.test(shell),
    'a user-facing string still points at "the graph tab" by its old name',
  )
})

/* The two vocabularies, named once so the assertions below cannot drift from
   the CSS comment that defines them (os.css, "The two state machines"). */
const RUNTIME_VOCAB = [/os-runtime-/, /var\(--g\)/, /var\(--s3\)/]
const EVIDENCE_VOCAB = [/os-evidence-/, /var\(--o\)/]

/** A top-level component's body: `function Name(` up to the next top-level `function`. */
function componentBody(text, name) {
  const start = text.indexOf(`function ${name}(`)
  assert.notEqual(start, -1, `component ${name} not found`)
  const rest = text.slice(start + 1)
  // `export default function` counts as the next top level too — missing it
  // silently swallowed the whole shell into `Lane` and made this pin vacuous.
  const end = rest.search(/\n(?:export default )?function /)
  return end === -1 ? rest : rest.slice(0, end)
}

test('the two state machines do not share a vocabulary', () => {
  // Before C7 one widget served both: `.os-dot` rendered an agent lane's
  // liveness AND the privacy assurance, same green, same glyph. Runtime health
  // ("is something running?") and evidence verdicts ("what does the record
  // say?") are different questions with different lifetimes, and a person
  // should never have to learn that one green circle means "the agent is
  // writing" and another means "the claim holds up".
  const shell = src('os/shell.jsx')

  // Comments stripped: the CSS names the retired class in the block that
  // explains why it is retired, and a walk that convicts its own changelog
  // teaches people to delete the changelog.
  const code = (f) => src(f).replace(/\/\*[\s\S]*?\*\//g, '')
  assert.ok(
    !/["']os-dot["']|\.os-dot\b/.test(code('os/shell.jsx') + code('os.css')),
    'the shared dot class is back; it is the exact widget that served both machines',
  )

  // `Lane` is pure runtime. `Inbox` is pure evidence. Neither may speak the
  // other's vocabulary — not the class prefix, and not the palette.
  const lane = componentBody(shell, 'Lane')
  for (const pattern of EVIDENCE_VOCAB) {
    assert.ok(!pattern.test(lane), `the runtime Lane speaks evidence vocabulary: ${pattern}`)
  }
  const inbox = componentBody(shell, 'Inbox')
  for (const pattern of RUNTIME_VOCAB) {
    assert.ok(!pattern.test(inbox), `the Inbox speaks runtime vocabulary: ${pattern}`)
  }

  // The dot glyph is runtime-only, everywhere, not just inside Lane.
  for (const [, tag] of shell.matchAll(/<span([^>]*os-runtime-dot[^>]*)>/g)) {
    assert.ok(
      !/os-evidence-/.test(tag),
      `a dot and an evidence class on one element: ${tag.trim()}`,
    )
  }

  // Both regions are named in the markup, so the separation is inspectable in
  // the DOM and not only in this file.
  assert.ok(/data-machine="runtime"/.test(shell), 'no runtime region is marked')
  assert.ok(/data-machine="evidence"/.test(shell), 'no evidence region is marked')
})

test('"Unverified" is dead on every human surface', () => {
  // C0.2: it hid three different situations (no proof found · insufficient
  // evidence · not evaluated) behind one accusatory bucket.
  for (const file of ['os/shell.jsx', 'mac/shell.jsx', 'main.jsx']) {
    const text = src(file).replace(/\/\*[\s\S]*?\*\//g, '')
    assert.ok(!/unverified/i.test(text), `${file} still carries the dead word`)
  }
})

/* C7 Part 4 — every region that renders made-up data says so, permanently.
 *
 * The walk that produced this set is recorded in docs/recon/c7/README.md.
 * Getting it wrong in either direction is a defect: a banner over real data is
 * a lie, and a missing banner over demo data is the worse lie. */
/* The Advanced Trace workspace ships the bundled demo corpus as its fallback,
 * so `workspace/chrome.jsx` — which owns the banner — joins both lists, and the
 * other workspace surfaces join ALL_SURFACES so a banner appearing over real
 * data there is caught too (CONTRACT §5 edit 5). */
const DEMO_SURFACES = ['cast.jsx', 'gallery.jsx', 'workspace/chrome.jsx']
const ALL_SURFACES = [
  'cast.jsx',
  'gallery.jsx',
  'main.jsx',
  'refresh.jsx',
  'os/shell.jsx',
  'mac/shell.jsx',
  'mac/work-map.jsx',
  'mac/decision-flow.jsx',
  'mac/graph.jsx',
  'mac/inspector.jsx',
  'mac/sprites.jsx',
  'mac/distilled.jsx',
  'workspace/chrome.jsx',
  'workspace/shell.jsx',
  'workspace/canvas.jsx',
  'workspace/docks.jsx',
  'workspace/ui-kit.jsx',
]

test('exactly the demo-data regions carry a demo banner', () => {
  const carrying = ALL_SURFACES.filter((f) => src(f).includes('data-demo-banner'))
  assert.deepEqual(
    carrying,
    DEMO_SURFACES,
    'the demo-banner set drifted: a real-data surface gained a banner, or a mock lost one',
  )
})

test('a demo banner is permanent and cannot be dismissed', () => {
  for (const file of DEMO_SURFACES) {
    const text = src(file)
    const banner = text.slice(text.indexOf('data-demo-banner'))
    assert.match(banner, /position: 'sticky'/, `${file}: the banner scrolls out of view`)
    assert.match(banner, /top: 0/, `${file}: the banner does not stay at the top`)
    // Non-dismissible, as far as source can show it: nothing in the file
    // names a way to turn a banner off. This does not prove the absence of
    // every conceivable hiding mechanism — it proves none is written here.
    assert.ok(
      !/dismiss|hideBanner|bannerHidden|setBanner/i.test(text.replace(/\/\*[\s\S]*?\*\//g, '')),
      `${file}: something in this file can hide the banner`,
    )
    // And it is not conditionally rendered.
    assert.ok(
      !/\{\s*\w+\s*&&\s*<div\s+data-demo-banner/.test(text),
      `${file}: the banner renders only under a condition`,
    )
  }
})

test('the demo banner says the data is made up, in plain words', () => {
  assert.match(src('cast.jsx'), /on made-up data\. Nothing here describes a real project\./)
  assert.match(src('gallery.jsx'), /placeholder data, not a product route\./)
})

test('no placeholder surface renders in the shell', () => {
  // The Incidents and Recovery Runs tabs, and Mission Control's "what this
  // will grow into" card, rendered a review note and NO DATA AT ALL. The
  // Live Truth Loop pivot deleted them: every tab that remains renders real
  // content, and a navigation entry that leads to a review note may not come
  // back. The designed versions stay reviewable in the marked mock gallery.
  const shell = src('os/shell.jsx').replace(/\/\*[\s\S]*?\*\//g, '')
  assert.ok(!shell.includes('REVIEW_NOTE'), 'a review-note placeholder is back in the shell')
  assert.ok(!/'incidents'|'runs'/.test(shell), 'a deleted placeholder tab route is back')
  assert.ok(
    !/Incidents|Recovery Runs|grow into/.test(shell),
    'a placeholder surface label is back in the shell',
  )
  assert.ok(
    !src('os/shell.jsx').includes('data-demo-banner'),
    'a demo banner was hung over a real-data surface',
  )
})

test('the synthetic index contract retains the Part 0 queue guarantees', () => {
  const representative = JSON.parse(readFileSync(REPRESENTATIVE_FIXTURE, 'utf8'))
  const index = {
    projects: [proj(
      representative.project.name,
      representative.project.id,
      [item('not_read', 'att_public_synthetic')],
    )],
  }
  const q = inboxQueue(index)
  assert.equal(q.silent.length, 0, 'a project in the synthetic index returned no attention item')
  assert.deepEqual(q.unrecognized, [], 'the synthetic index carries a kind this build cannot rank')
  for (const r of q.rows) {
    assert.ok(ACTIONABLE_KINDS.includes(r.kind))
    assert.ok(r.sentence && r.action, 'a synthetic row is missing its sentence or its action')
  }
})
