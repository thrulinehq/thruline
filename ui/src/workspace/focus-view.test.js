import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { FOCUS_GROUP_PREFIX, chainOf, focusProjection, isFocusGroupId } from './focus-set.js'

/* The Trace Focus PROJECTION (HQ-VERDICT-PANEL-PLAN Step 5): the derived node
 * list `shell.jsx` `G()` hands the graph. Pinned here without React, on the
 * adapter's own shapes, so the behaviour behind the Focus / Everything toggle
 * is a function a test can refute. */

const ev = (seq, kind, extra) => ({ id: 'ev' + seq, objType: 'event', kind, seq, type: 'EVENT', fam: 'blue', frame: 'execution', w: 280, title: kind + ' ' + seq, lines: [], pinsL: [{ id: 'i', label: 'recorded order', t: 'order' }], pinsR: [{ id: 'o', label: 'recorded order', t: 'order' }, { id: 'ev', label: 'event', t: 'ref' }], ...extra })
const claim = (id, word) => ({ id, objType: 'claim', word, type: 'DERIVED', fam: 'red', frame: 'claims', w: 300, sub: 'Claim · done', title: '“' + id + '”', pinsL: [{ id: 'chk', label: 'check', t: 'ref' }], pinsR: [] })
const check = (id) => ({ id, objType: 'check', type: 'DERIVED', fam: 'green', frame: 'claims', w: 300, sub: 'Check · test', title: 'cargo test · exit 0', pinsL: [{ id: 'from', label: 'from', t: 'inf' }], pinsR: [{ id: 'k', label: 'check', t: 'ref' }] })
const receipt = () => ({ id: 'rcpt', objType: 'receipt', type: 'DERIVED', fam: 'grey', frame: 'claims', w: 300, sub: 'DSSE', title: 'Receipt intact', pinsL: [{ id: 'r', label: 'receipt', t: 'ref' }], pinsR: [] })
const wire = (id, from, to, kind) => ({ id, from, to, kind })

/* One not-proven claim with a chain; proven noise; and a level-of-detail
 * fold: ev9 (call) groups ev10 (result), the shape `lod.js:203-222` writes. */
function fixture() {
  const nodes = [
    ev(1, 'exec'),
    ev(2, 'fs_change', { file: 'src/a.rs' }),
    ev(3, 'fs_change', { file: 'src/b.rs' }),
    ev(9, 'tool_call', { group: true, members: ['ev10'], memberKinds: ['tool_result'], rep: 'ev10', firstSeq: 9, lastSeq: 10, toggleLabel: '▸ 1 result', toggleAria: 'Expand 1 nearby result' }),
    ev(10, 'tool_result', { member: 'ev9' }),
    ev(11, 'snapshot', { title: 'tree 7e3a9c01b5d2', lines: ['tree 7e3a9c01b5d2'] }),
    claim('c1', 'not_proven'),
    claim('c2', 'proven'),
    check('k1'),
    check('k2'),
    receipt(),
  ]
  const wires = [
    wire('w1', ['k1', 'k'], ['c1', 'chk'], 'ref'),
    wire('w2', ['ev1', 'ev'], ['k1', 'from'], 'inf'),
    wire('w3', ['ev2', 'ev'], ['ev1', 'i'], 'ref'),
    wire('w4', ['ev3', 'ev'], ['ev2', 'i'], 'ref'),
    wire('w5', ['k2', 'k'], ['c2', 'chk'], 'ref'),
    wire('w6', ['c2', 'chk'], ['rcpt', 'r'], 'ref'),
  ]
  return { nodes, wires }
}

test('everything returns the caller‘s own array, untouched', () => {
  const { nodes, wires } = fixture()
  const p = focusProjection(nodes, wires, { focusMode: 'everything' })
  assert.equal(p.mode, 'everything')
  assert.equal(p.nodes, nodes, 'same identity — today‘s graph is today‘s graph')
  assert.deepEqual(p.groups, [])
  assert.equal(p.summary, null)
  assert.equal(focusProjection(nodes, wires).mode, 'everything', 'no mode means Everything')
})

test('focus hides every node outside the set behind a group card, with `member` set to the group id', () => {
  const { nodes, wires } = fixture()
  const before = JSON.stringify(nodes)
  const p = focusProjection(nodes, wires, { focusMode: 'focus', focusExpanded: {} })
  assert.equal(JSON.stringify(nodes), before, 'S.nodes is never written through')
  assert.equal(p.mode, 'focus')

  const by = new Map(p.nodes.map((n) => [n.id, n]))
  /* The chain: c1 ← k1 ← ev1 ← ev2 (depth 3). */
  ;['c1', 'k1', 'ev1', 'ev2'].forEach((id) => assert.equal(by.get(id).member, undefined, id + ' is shown'))
  /* Everything else is a member of exactly one focus group. */
  assert.equal(by.get('ev3').member, 'grp_focus_fs')
  assert.equal(by.get('ev9').member, 'grp_focus_holds')
  assert.equal(by.get('ev10').member, 'grp_focus_holds', 'a level-of-detail member is re-homed to the focus group that holds its call')
  assert.equal(by.get('ev11').member, 'grp_focus_trees')
  assert.equal(by.get('c2').member, 'grp_focus_record')
  assert.equal(by.get('k2').member, 'grp_focus_record')
  assert.equal(by.get('rcpt').member, 'grp_focus_record')
  assert.ok(by.get('ev9').group && by.get('ev9').members, 'the folded call keeps its own group shape')

  /* The group cards, in the shape the existing group-card path renders. */
  const cards = p.nodes.filter((n) => n.focusGroup)
  assert.deepEqual(
    cards.map((c) => [c.id, c.title, c.members]),
    [
      ['grp_focus_holds', '2 events that hold up', ['ev9', 'ev10']],
      ['grp_focus_fs', '1 file change', ['ev3']],
      ['grp_focus_trees', '1 tree recorded', ['ev11']],
      ['grp_focus_record', '3 for the record', ['c2', 'k2', 'rcpt']],
    ],
  )
  cards.forEach((c) => {
    assert.ok(isFocusGroupId(c.id) && c.id.indexOf(FOCUS_GROUP_PREFIX) === 0)
    assert.equal(c.group, true)
    assert.equal(c.synthetic, true, 'a card that stands for objects is not an object')
    assert.equal(c.type, 'MERGED')
    assert.equal(c.rep, c.members[0])
    assert.equal(typeof c.toggleLabel, 'string')
    assert.ok(/^▸ /.test(c.toggleLabel), 'the toggle carries its caret and a word, never a bare glyph')
    assert.ok(/^Expand /.test(c.toggleAria))
    assert.equal(c.sub, 'Focus group')
    assert.ok(c.lines.length === 1 && c.lines[0].length > 0, 'the one line, under the label')
    assert.ok(c.foot.indexOf('merged in Focus') === 0)
  })
  const holds = cards[0]
  assert.equal(holds.firstSeq, 9)
  assert.equal(holds.lastSeq, 10)
  assert.equal(holds.frame, 'execution', 'the card sits in the frame most of its members sit in')
  assert.deepEqual(holds.memberKinds, ['tool_call', 'tool_result'])
  assert.equal(holds.pinSide.ev, 'R', 'a wire into a merged member attaches on the side the member‘s pin was on')
  assert.equal(holds.pinSide.i, 'L')
  assert.equal(cards[2].lines[0], 'tree 7e3a9c01b5d2')

  /* The accounting: objects of the record only, and nothing dropped. */
  assert.deepEqual(p.summary, { shown: 4, total: 11, merged: 7, allClear: false, degraded: false })
  const merged = cards.reduce((s, c) => s + c.members.length, 0)
  assert.equal(p.summary.shown + merged, 11)
  assert.equal(p.nodes.length, nodes.length + cards.length, 'every node is present, plus one card per group')
})

test('expanding a group in place drops its card and frees its members', () => {
  const { nodes, wires } = fixture()
  const p = focusProjection(nodes, wires, { focusMode: 'focus', focusExpanded: { grp_focus_record: true } })
  const by = new Map(p.nodes.map((n) => [n.id, n]))
  assert.equal(by.has('grp_focus_record'), false, 'an opened group has no card')
  ;['c2', 'k2', 'rcpt'].forEach((id) => assert.equal(by.get(id).member, undefined, id + ' renders as today'))
  assert.equal(by.get('ev3').member, 'grp_focus_fs', 'the other groups are untouched')
  assert.deepEqual(p.summary, { shown: 7, total: 11, merged: 4, allClear: false, degraded: false })
  const rec = p.groups.find((g) => g.id === 'grp_focus_record')
  assert.equal(rec.expanded, true)
  assert.deepEqual(rec.members, ['c2', 'k2', 'rcpt'], 'the group list still names it, so a caller can find the group a chain member is in')

  /* Opening the holds group returns the folded call and its result to the
   * level-of-detail relation exactly as the fold wrote it — and the result,
   * behind its call's fold card, is on screen: it counts as shown. */
  const q = focusProjection(nodes, wires, { focusMode: 'focus', focusExpanded: { grp_focus_holds: true } })
  const qb = new Map(q.nodes.map((n) => [n.id, n]))
  assert.equal(qb.get('ev10').member, 'ev9')
  assert.equal(qb.get('ev9').member, undefined)
  assert.deepEqual(q.summary, { shown: 6, total: 11, merged: 5, allClear: false, degraded: false })
  const closedCounts = q.groups.filter((g) => !g.expanded).reduce((s, g) => s + g.count, 0)
  assert.equal(q.summary.shown + closedCounts, q.summary.total, 'shown + Σ closed-group counts == total after an Expand')
})

test('a fold inside an opened group counts its members as shown, so the pill reconciles after any Expand', () => {
  /* ADJUDICATIONS item F finding 1: on the live session, opening a group
   * freed 155 level-of-detail-folded snapshots that were counted as "merged"
   * with no Focus card standing for them. A member of a fold has a card on
   * screen — its fold card — and is shown. */
  const { nodes, wires } = fixture()
  const fold = { id: 'grp_tree_11', objType: 'event', kind: 'snapshot', synthetic: true, group: true, members: ['ev11', 'ev12'], memberKinds: ['snapshot'], rep: 'ev11', firstSeq: 11, lastSeq: 12, type: 'EVENT', fam: 'grey', sub: 'Working tree', title: 'Repeated code checkpoints', lines: ['tree 7e3a9c01b5d2 · seq 11–12'], pinsL: [], pinsR: [], w: 280 }
  const withFold = nodes.concat([ev(12, 'snapshot', { title: 'tree 7e3a9c01b5d2', member: 'grp_tree_11' }), fold])
  withFold.find((n) => n.id === 'ev11').member = 'grp_tree_11'
  const invariant = (p) => {
    const closed = p.groups.filter((g) => !g.expanded).reduce((s, g) => s + g.count, 0)
    assert.equal(p.summary.shown + closed, p.summary.total, `shown ${p.summary.shown} + closed ${closed} ≠ total ${p.summary.total}`)
    assert.equal(p.summary.merged, closed, 'merged is exactly what the closed cards account for')
  }
  const closed = focusProjection(withFold, wires, { focusMode: 'focus', focusExpanded: {} })
  assert.deepEqual(closed.summary, { shown: 4, total: 12, merged: 8, allClear: false, degraded: false })
  invariant(closed)
  const opened = focusProjection(withFold, wires, { focusMode: 'focus', focusExpanded: { grp_focus_trees: true } })
  const by = new Map(opened.nodes.map((n) => [n.id, n]))
  assert.equal(by.get('ev11').member, 'grp_tree_11', 'the snapshot is back behind its fold card')
  assert.equal(by.get('ev12').member, 'grp_tree_11')
  assert.equal(by.get('grp_tree_11').member, undefined, 'the fold card is drawn')
  assert.equal(by.has('grp_focus_trees'), false)
  assert.deepEqual(opened.summary, { shown: 6, total: 12, merged: 6, allClear: false, degraded: false }, 'the two folded snapshots count as shown; 6 merged = holds 2 + fs 1 + record 3')
  invariant(opened)
  const all = focusProjection(withFold, wires, { focusMode: 'focus', focusExpanded: { grp_focus_trees: true, grp_focus_holds: true, grp_focus_fs: true, grp_focus_record: true } })
  assert.deepEqual(all.summary, { shown: 12, total: 12, merged: 0, allClear: false, degraded: false }, 'every group open: everything is shown, nothing is merged')
  invariant(all)
})

test('the shell opens in Focus on every session and never persists the choice', () => {
  /* ADJUDICATIONS item F finding 3: `shell.jsx` has no unit tests, so the
   * ruling ("Focus by default, not remembered") is pinned on its source. */
  const src = readFileSync(fileURLToPath(new URL('./shell.jsx', import.meta.url)), 'utf8')
  const stateOpen = src.indexOf('this.state = {')
  assert.ok(stateOpen > 0)
  const stateClose = src.indexOf('\n    }', stateOpen)
  const state = src.slice(stateOpen, stateClose)
  assert.match(state, /\n\s+focusMode: 'focus',/, 'the constructor opens in Focus')
  assert.doesNotMatch(state, /focusMode: (?!'focus')/, 'the initial focusMode is the literal \'focus\', not a pref')
  const save = src.match(/\n  savePrefs\(\) \{([\s\S]*?)\n  \}/)
  assert.ok(save, 'savePrefs() is where prefs are serialised')
  /* `focusPrev` (the focus-canvas mode's saved panel sizes) is older and unrelated; the three Step 5 fields are what must stay out. */
  assert.doesNotMatch(save[1], /focusMode|focusExpanded|focusChain/, 'savePrefs never learns focusMode, focusExpanded or focusChain')
  assert.doesNotMatch(src, /prefs\.focus(?:Mode|Expanded|Chain)/, 'nothing reads a focus pref back')
  const switchAt = src.indexOf("focusMode: 'focus'", stateClose)
  assert.ok(switchAt > 0, 'a session switch resets to Focus')
})

test('a kept node whose level-of-detail group is merged is freed, never hidden behind a merged card', () => {
  const { nodes, wires } = fixture()
  /* Make the RESULT the seed‘s neighbour while its call stays outside the set. */
  const ws = wires.concat([wire('w7', ['ev10', 'ev'], ['c1', 'chk'], 'ref')])
  const p = focusProjection(nodes, ws, { focusMode: 'focus', focusExpanded: {} })
  const by = new Map(p.nodes.map((n) => [n.id, n]))
  assert.equal(by.get('ev9').member, 'grp_focus_holds', 'the call is merged')
  assert.equal(by.get('ev10').member, undefined, 'the kept result is on screen')
  assert.equal(nodes.find((n) => n.id === 'ev10').member, 'ev9', 'the source node still says what the fold said')
})

test('a merged level-of-detail fold card is hidden with its members but never counted as an object', () => {
  const { nodes, wires } = fixture()
  /* The shape `lod.js:252-280` writes for a run of unchanged snapshots. */
  const fold = { id: 'grp_tree_11', objType: 'event', kind: 'snapshot', synthetic: true, group: true, members: ['ev11', 'ev12'], memberKinds: ['snapshot'], rep: 'ev11', firstSeq: 11, lastSeq: 12, type: 'EVENT', fam: 'grey', sub: 'Working tree', title: 'Repeated code checkpoints', lines: ['tree 7e3a9c01b5d2 · seq 11–12'], pinsL: [], pinsR: [], w: 280 }
  const withFold = nodes.concat([ev(12, 'snapshot', { title: 'tree 7e3a9c01b5d2', member: 'grp_tree_11' }), fold])
  withFold.find((n) => n.id === 'ev11').member = 'grp_tree_11'
  const p = focusProjection(withFold, wires, { focusMode: 'focus', focusExpanded: {} })
  const trees = p.nodes.find((n) => n.id === 'grp_focus_trees')
  assert.deepEqual(trees.members, ['ev11', 'ev12', 'grp_tree_11'], 'the fold card is merged like a member — it must not be drawn')
  assert.equal(trees.title, '2 trees recorded', 'the label counts the two snapshots, not the card that stands for them')
  assert.equal(trees.foot, 'merged in Focus · 2 objects · expand to see each one')
  const by = new Map(p.nodes.map((n) => [n.id, n]))
  assert.equal(by.get('grp_tree_11').member, 'grp_focus_trees')
  assert.equal(p.summary.total, 12)
  const merged = p.nodes.filter((n) => n.focusGroup).reduce((s, c) => s + p.groups.find((g) => g.id === c.id).count, 0)
  assert.equal(p.summary.shown + merged, p.summary.total, 'shown + every card‘s object count = the denominator')
})

test('the chain "Show proof" outlines is the seed and its derivation neighbours, and names the groups to open', () => {
  const { nodes, wires } = fixture()
  const chain = chainOf(nodes, wires, 'c1', 3)
  assert.deepEqual(chain, ['c1', 'k1', 'ev1', 'ev2'])
  const p = focusProjection(nodes, wires, { focusMode: 'focus', focusExpanded: {} })
  const inChain = new Set(chain)
  const toOpen = p.groups.filter((g) => g.members.some((m) => inChain.has(m))).map((g) => g.id)
  assert.deepEqual(toOpen, [], 'a chain from a seed is inside the set by construction — no group needs opening')

  /* A chain headed by a merged node reaches into its group, which must open. */
  const c2chain = chainOf(nodes, wires, 'c2', 3)
  assert.deepEqual(c2chain, ['c2', 'k2', 'rcpt'])
  const open2 = p.groups.filter((g) => g.members.some((m) => c2chain.indexOf(m) >= 0)).map((g) => g.id)
  assert.deepEqual(open2, ['grp_focus_record'])
})

test('all-Proven: one "Everything holds up" card and nothing else; degraded with no seed: the caller‘s recording-gap copy', () => {
  const nodes = [ev(1, 'exec'), ev(2, 'fs_change'), claim('c1', 'proven'), check('k1'), receipt()]
  const wires = [wire('w1', ['k1', 'k'], ['c1', 'chk'], 'ref')]
  const p = focusProjection(nodes, wires, { focusMode: 'focus', focusExpanded: {} })
  const cards = p.nodes.filter((n) => n.focusGroup)
  assert.equal(cards.length, 1)
  assert.equal(cards[0].id, 'grp_focus_all')
  assert.equal(cards[0].title, 'Everything holds up')
  assert.equal(p.nodes.filter((n) => !n.member && !n.focusGroup).length, 0, 'nothing else is drawn')
  assert.deepEqual(p.summary, { shown: 0, total: 5, merged: 5, allClear: true, degraded: false })

  const fog = focusProjection(nodes, wires, { focusMode: 'focus', focusExpanded: {}, degraded: true, gapLabel: 'Recording gap', gapLine: 'recorded with gaps — cause not recorded' })
  const fc = fog.nodes.filter((n) => n.focusGroup)
  assert.equal(fc.length, 1)
  assert.equal(fc[0].title, 'Recording gap · 5 steps recorded', 'the caller‘s words, then the module‘s count')
  assert.deepEqual(fc[0].lines, ['recorded with gaps — cause not recorded'])
  assert.equal(fog.summary.allClear, false)
  assert.equal(fog.summary.degraded, true)
  assert.ok(JSON.stringify(fog.nodes).indexOf('Everything holds up') < 0, 'the confident phrase never appears over a degraded seedless record')

  const dflt = focusProjection(nodes, wires, { focusMode: 'focus', degraded: true })
  assert.equal(dflt.nodes.filter((n) => n.focusGroup)[0].title, 'Recording gap · 5 steps recorded', 'with no caller copy, the label is the bare fact and the count')
})

test('zero nodes, null input and a null node are tolerated', () => {
  assert.deepEqual(focusProjection([], [], { focusMode: 'focus' }), { mode: 'focus', nodes: [], groups: [], summary: { shown: 0, total: 0, merged: 0, allClear: true, degraded: false } })
  assert.equal(focusProjection(null, null, { focusMode: 'focus' }).nodes.length, 0)
  const p = focusProjection([null, { id: 'x' }, claim('c1', 'couldnt')], [], { focusMode: 'focus' })
  assert.equal(p.nodes.length, 3, 'x, c1, and one "other" card')
  assert.equal(p.summary.total, 2)
})

test('deterministic, and the source names no clock, locale or random', () => {
  const a = fixture()
  const b = fixture()
  const st = { focusMode: 'focus', focusExpanded: { grp_focus_fs: true } }
  assert.deepEqual(focusProjection(a.nodes, a.wires, st), focusProjection(b.nodes, b.wires, st))
  const source = readFileSync(fileURLToPath(new URL('./focus-set.js', import.meta.url)), 'utf8')
  assert.ok(!/toLocaleString|Intl\.|Math\.random|new Date|Date\.now/.test(source))
})
