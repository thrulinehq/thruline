/* M6.R2.3 — L2 Claim & Receipt.
 *
 * The gate here is progressive disclosure with nothing lost: the sentence is
 * first, the engine's own wording survives byte-for-byte behind a fold, and the
 * receipt's dimensions stay labeled and separate. */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

import {
  boundedOutsidePathPresentation,
  claimTextPresentation,
  claimView,
  evidencePathPresentation,
  evidencePresentation,
  judgedDetail,
  ledgerLine,
  safeEvidenceSummary,
} from './receipt.js'
import { CLAIM_STORY, VERDICT, bannedIn, claimStory, sentence } from './voice.js'

const here = dirname(fileURLToPath(import.meta.url))
const src = (rel) => readFileSync(join(here, '..', rel), 'utf8')
const REPRESENTATIVE_FIXTURE = fileURLToPath(new URL('../../../crates/core/tests/fixtures/grounded-mind-public.json', import.meta.url))

/* RA5: L2's screen is superseded; the receipt now docks as the macOS
 * inspector. Same selection, same rules, new structural target. */
const INSPECTOR = src('mac/inspector.jsx')

const DOC = {
  contract_version: 2,
  project: { id: 'p', name: 'synth', source: 'claude_code' },
  diagnostics: false,
  sessions: [{ id: 's1', label: 'Session 1', date: 'Jul 29', msgs: 12 }],
  nodes: [
    { id: 'n1', label: 'tests', sort_key: 'contradicted', session_id: 's1', evidence_count: 2,
      anchor_event_id: 'EV1', accusation: true,
      dimensions: { lifecycle: 'completed', evidence: 'contradicted', currency: 'current' } },
    { id: 'n2', label: 'later', sort_key: 'pending', session_id: 's1', evidence_count: 0,
      anchor_event_id: 'EV2', accusation: false,
      dimensions: { lifecycle: 'proposed', evidence: 'unknown', currency: 'current' } },
  ],
  edges: [],
  receipts: {
    n1: {
      claim: {
        text: 'all 33 tests pass', session: 's1', message_no: 18, ts: '2026-07-29T10:00:00Z',
        claims: [{
          text: 'all 33 tests pass', kind: 'work', marker: 'x', sort_key: 'contradicted',
          reason: 'r', span: [0, 5], candidates: 1,
          evidence_scopes: [{
            event_id: 'EV1', session: 's1', seq: 19, cwd: '/w', git_branch: 'main',
            command_identity: 'bash:cargo test', freshness: 'stale', superseded_by: null,
            freshness_note: 'A later run of the same command reported a different result.',
          }],
        }],
      },
      evidence: [
        { kind: 'command', summary: 'cargo test', path: undefined,
          scope: { session: 's1', seq: 19, cwd: '/w', git_branch: 'main', command_identity: 'bash:cargo test' } },
        { kind: 'edit', summary: 'edited lib.rs', path: 'crates/core/src/lib.rs', scope: { session: 's1', seq: 20 } },
      ],
      verdict: { text: 'deterministic · local', entailed: false, score: 0.97, model: 'deterministic' },
      ledger: { event_no: 19, total: 1567, verified: true },
    },
  },
}

/* ── part 1 and 2: the sentence, then what happened ────────────────────── */

test('the sentence is first, and it is the authority\'s', () => {
  const v = claimView(DOC, 'n1')
  assert.equal(v.sentence, sentence('contradicted'))
  assert.equal(v.sentence, 'The AI said this was done — the record shows otherwise.')
  assert.equal(bannedIn(v.sentence), null)
})

test('what happened is one level behind, and comes from the authority too', () => {
  const v = claimView(DOC, 'n1')
  assert.equal(v.story, claimStory('contradicted'))
  assert.notEqual(v.story, v.sentence, 'level 2 must add something, not repeat level 1')
  assert.equal(bannedIn(v.story), null)
})

test('the claim-story table is total over every state the board can show', () => {
  for (const slug of Object.keys(VERDICT)) {
    assert.ok(CLAIM_STORY[slug], `no "what happened" line for ${slug}`)
    assert.equal(bannedIn(CLAIM_STORY[slug].sentence), null, `lexicon in the ${slug} story`)
  }
  // One fact reached by a different door is still one fact.
  assert.equal(claimStory('contradicted'), 'The record holds both the claim and a run that disagrees with it.')
  // An unrecognised state falls back to a sentence, never to a slug.
  assert.equal(claimStory('nonsense'), CLAIM_STORY.unknown.sentence)
})

test('receipt counts carry their noun, and a claim with none drops the clause', () => {
  assert.equal(claimView(DOC, 'n1').receiptPhrase, '2 receipts')
  assert.equal(claimView(DOC, 'n2').receiptPhrase, '', 'a "0 receipts" clause is a zero-count element')
  assert.equal(claimView(DOC, 'n2').receiptCount, 0)
})

/* ── part 3: the receipt, to file and line, dimensions never conflated ── */

test('the receipt reaches file and line, with every dimension labeled', () => {
  const v = claimView(DOC, 'n1')
  assert.equal(v.claimText, 'all 33 tests pass')
  assert.equal(v.run, 'Session 1')
  assert.equal(v.messageNo, 18)
  assert.equal(v.anchor, 'EV1')
  const [cmd, edit] = v.evidence
  const labels = (e) => Object.fromEntries(e.where.map((w) => [w.label, w.value]))
  assert.deepEqual(labels(cmd), {
    'Working directory': '/w', Branch: 'main', Command: 'bash:cargo test', Run: 's1', Line: '19',
  })
  assert.equal(labels(edit).File, 'crates/core/src/lib.rs')
  assert.equal(labels(edit).Line, '20')
  // Labeled, not run together: every value arrives with its own label.
  for (const e of v.evidence) for (const w of e.where) assert.ok(w.label && w.value)
})

test('first-level evidence paths are relative inside the workspace and bounded outside it', () => {
  assert.deepEqual(evidencePathPresentation('/repo/src/lib/deep/file.js', '/repo'), {
    display: 'src/lib/deep/file.js', label: 'Workspace path', outside: false, rawPath: '/repo/src/lib/deep/file.js',
  })
  assert.deepEqual(evidencePathPresentation('/private/other/long/tree/file.js', '/repo'), {
    display: '…/long/tree/file.js', label: 'Outside workspace', outside: true, rawPath: '/private/other/long/tree/file.js',
  })
  assert.deepEqual(evidencePathPresentation('crates/core/src/lib.rs', '/repo'), {
    display: 'crates/core/src/lib.rs', label: 'Workspace path', outside: false, rawPath: 'crates/core/src/lib.rs',
  })
  assert.deepEqual(evidencePathPresentation('/var/tmp/file.log', '/'), {
    display: 'var/tmp/file.log', label: 'Workspace path', outside: false, rawPath: '/var/tmp/file.log',
  })
})

test('evidence presentation uses human actions while retaining raw receipt fields', () => {
  const evidence = { kind: 'command', summary: 'cargo test --workspace', path: '/repo/Cargo.toml', scope: { cwd: '/repo', seq: 19, command_identity: 'zsh:cargo test --workspace' } }
  assert.deepEqual(evidencePresentation(evidence), {
    action: 'Command run',
    summary: '',
    line: '19',
    path: { display: 'Cargo.toml', label: 'Workspace path', outside: false, rawPath: '/repo/Cargo.toml' },
  })
  const view = claimView(DOC, 'n1')
  assert.deepEqual(view.rawReceiptDetails.evidence, DOC.receipts.n1.evidence)
  assert.equal(view.rawReceiptDetails.node_id, 'n1')
  assert.equal(view.rawReceiptDetails.event_id, 'EV1')
  assert.deepEqual(view.rawReceiptDetails.ledger, DOC.receipts.n1.ledger)
})

test('first-level summaries suppress command shapes, absolute paths, cwd values, and result dumps', () => {
  const scope = { cwd: '/Users/example/work', command_identity: 'bash:/users/example/work:rg secret' }
  const unsafe = [
    'Bash rg secret /Users/example/work → 4 matches',
    'Read /Users/example/work/private.txt',
    'bash:/users/example/work:rg secret',
    'Search complete => output: four lines',
    'line one\nline two',
  ]
  for (const summary of unsafe) assert.equal(safeEvidenceSummary({ kind: 'command', summary, scope }), '', summary)
  assert.equal(safeEvidenceSummary({ kind: 'command', summary: 'Read guide.tsx', scope }), 'Read guide.tsx')
  assert.equal(safeEvidenceSummary({ kind: 'edit', summary: '**Edit** guide.tsx', scope }), 'Edit guide.tsx')
})

test('bounded outside paths retain the raw value without exposing its prefix in display', () => {
  const raw = '/Users/example/private/project/src/file.js'
  assert.deepEqual(boundedOutsidePathPresentation(raw, 'Workspace relation unproven'), {
    display: '…/project/src/file.js',
    label: 'Workspace relation unproven',
    outside: true,
    rawPath: raw,
  })
  assert.equal(boundedOutsidePathPresentation('/Users/example/file.js').display, '…/file.js')
  assert.equal(boundedOutsidePathPresentation('C:\\Users\\example\\file.js').display, '…/file.js')
})

test('first-level claim text replaces exact paths and withholds command or result dumps', () => {
  const file = '/Users/example/private/project/src/file.js'
  const exact = { kind: 'read', path: file, scope: { cwd: '/Users/example/private/project', command_identity: 'read:file' } }
  assert.deepEqual(claimTextPresentation(`Reviewed ${file}.`, [exact]), {
    text: 'Reviewed [Project-relative path: src/file.js].',
    reduced: true,
  })
  const unproven = claimTextPresentation(`Reviewed ${file}.`, [])
  assert.equal(unproven.reduced, true)
  assert.match(unproven.text, /Workspace relation unproven/)
  assert.doesNotMatch(unproven.text, /\/Users\//)

  const ambiguous = claimTextPresentation(`Reviewed ${file}.`, [
    exact,
    { kind: 'read', path: file, scope: { cwd: '/Users/example/private/project/src' } },
  ])
  assert.match(ambiguous.text, /Workspace relation ambiguous/)
  assert.doesNotMatch(ambiguous.text, /\/Users\//)

  const dumped = `Bash rg TODO ${file} → output: 4 matches`
  assert.deepEqual(claimTextPresentation(dumped, [{ ...exact, kind: 'command' }]), {
    text: 'Command run; exact text is available in Raw receipt details.',
    reduced: true,
  })
  assert.deepEqual(claimTextPresentation('result: private output', []), {
    text: 'Exported record; exact text is available in Raw receipt details.',
    reduced: true,
  })
})

/* ── part 4: demoted, intact ───────────────────────────────────────────── */

test('the engine detail is demoted, not deleted, and survives byte-for-byte', () => {
  const v = claimView(DOC, 'n1')
  assert.equal(v.judged, 'deterministic · local · 0.97')
  assert.equal(judgedDetail({ text: 'x' }), 'x')
  // It IS engine vocabulary — that is the point of putting it behind the fold.
  assert.ok(bannedIn(v.judged), 'the engine detail is engine vocabulary, by design')
  // And the fold starts closed in the inspector, opened only by the user.
  assert.match(INSPECTOR, /Show how this was judged[\s\S]{0,700}v\.judged/)
  assert.match(INSPECTOR, /useState\(false\)/, 'the fold must start closed')
})

test('no first-level string on the inspector carries the engine detail', () => {
  const v = claimView(DOC, 'n1')
  for (const [where, s] of [['sentence', v.sentence], ['story', v.story], ['word', v.word]]) {
    assert.equal(bannedIn(s), null, `engine lexicon in the first-level ${where}: ${s}`)
    assert.doesNotMatch(s, /0\.\d\d/, `a score reached the first level via ${where}`)
  }
})

/* ── part 5: history and currency ──────────────────────────────────────── */

test('the ledger line states the count with its noun and never a check mark', () => {
  /* A150 (P1): the two facts, separated. "chain broken" is dead vocabulary —
     an intact chain with dead receipts must never wear it again. */
  assert.equal(
    ledgerLine({ event_no: 19, total: 1567, verified: true, chain_intact: true, raw_files_missing: 0 }),
    '1,567 events on the ledger · chain checked',
  )
  assert.equal(
    ledgerLine({ event_no: 1, total: 33880, verified: false, chain_intact: true, raw_files_missing: 7 }),
    '33,880 events on the ledger · chain checked · 7 source files no longer on disk',
    'the A147 specimen: intact chain, dead receipts — named separately',
  )
  assert.equal(
    ledgerLine({ event_no: 1, total: 9, verified: false, chain_intact: false }),
    '9 events on the ledger · chain damaged',
  )
  assert.equal(
    ledgerLine({ event_no: 1, total: 9, verified: false }),
    '9 events on the ledger · chain unchecked',
    'an export from before the split abstains rather than accusing',
  )
  assert.ok(!ledgerLine({ total: 9, verified: false }).includes('broken'))
  assert.equal(ledgerLine(null), '')
  assert.doesNotMatch(ledgerLine({ total: 9, verified: true }), /[✓✗]|Ledger/)
  assert.equal(bannedIn(ledgerLine({ total: 9, verified: true })), null)
})

test('the stale ghost appears only when the record says stale', () => {
  assert.equal(claimView(DOC, 'n1').stale, true, 'a stale freshness note is a stale claim')
  assert.equal(claimView(DOC, 'n2').stale, false)
  /* The ghosts moved into the canvas (RA5 directive 4): the inspector is
     chrome and renders no sprite at all; DUSK's seat is its stale node in
     mac/graph.jsx, behind the state filter, and nowhere else. */
  assert.ok(!/Sprite|SpriteSvg|GhostAt/.test(INSPECTOR), 'a sprite reached the inspector chrome')
  assert.match(src('mac/graph.jsx'), /state === 'stale' && !dimmed\(n\)/)
})

test('currency is its own dimension and is never merged with truth', () => {
  const v = claimView(DOC, 'n1')
  assert.equal(v.currency, 'current')
  assert.equal(v.state, 'contradicted')
  // A contradicted claim that is still current: two facts, and the screen says
  // both. Merging them is the failure M5.R2 separated them to prevent.
})

/* ── the standing gates, on the new surface ────────────────────────────── */

function displayStrings(text) {
  const stripped = text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
  const out = []
  for (const m of stripped.matchAll(/>([^<>{}]{4,})</g)) {
    const s = m[1].replace(/\s+/g, ' ').trim()
    if (s && /[A-Za-z]/.test(s)) out.push(s)
  }
  return out
}

test('every first-level string on the inspector passes the lexicon gate', () => {
  const strings = displayStrings(INSPECTOR)
  assert.ok(strings.length > 0)
  for (const s of strings) assert.equal(bannedIn(s), null, `banned term in ${JSON.stringify(s)}`)
})

test('the inspector executes nothing and re-runs nothing (§5u)', () => {
  for (const forbidden of ['child_process', 'exec(', 'spawn(', 'eval(', 'new Function']) {
    assert.ok(!INSPECTOR.includes(forbidden), `inspector contains ${forbidden}`)
  }
  // The design's "Re-check" button does not come across: nothing on a screen
  // re-runs anything. Copy receipt is the one action, and it only copies.
  assert.ok(!/Re-check/.test(INSPECTOR))
  assert.ok(!/Mark resolved|Create task/.test(INSPECTOR.replace(/\/\*[\s\S]*?\*\//g, '')),
    'a button with no machinery behind it reached the inspector')
  assert.match(INSPECTOR, /navigator\.clipboard/)
  assert.equal([...INSPECTOR.matchAll(/fetch\(/g)].length, 0, 'the inspector loads nothing')
})

test('the inspector never renders command identity at first level', () => {
  assert.ok(!INSPECTOR.includes('presentation.command'))
  assert.ok(!INSPECTOR.includes('Command:'))
  assert.match(INSPECTOR, /Raw receipt details/)
  assert.match(INSPECTOR, /v\.claimPresentation\.text/)
  assert.doesNotMatch(INSPECTOR, />\s*\{v\.claimText\}\s*</)
})

test('no shadow class reaches the inspector', () => {
  const stripped = INSPECTOR.replace(/\/\*[\s\S]*?\*\//g, '')
  for (const shadow of ['unsupported', 'dropped', 'no proof found', 'abandoned']) {
    assert.ok(!new RegExp(`\\b${shadow}\\b`, 'i').test(stripped), `inspector names ${shadow}`)
  }
})

/* ── over the tracked public synthetic export ─────────────────────────── */

test('every claim in the tracked synthetic project derives a complete six-part view', (t) => {
  let claims = 0
  let withReceipts = 0
  const doc = JSON.parse(readFileSync(REPRESENTATIVE_FIXTURE, 'utf8'))
  for (const node of doc.nodes || []) {
    const v = claimView(doc, node.id)
    assert.ok(v, `${node.id} has no view`)
    assert.ok(v.sentence, `${node.id} has no sentence`)
    assert.ok(v.story, `${node.id} has no story`)
    assert.equal(bannedIn(v.sentence), null, `${node.id}: lexicon in a sentence`)
    assert.equal(bannedIn(v.story), null, `${node.id}: lexicon in a story`)
    if (v.receiptCount > 0) {
      assert.match(v.receiptPhrase, /\d+ receipts?$/)
      withReceipts += 1
    } else {
      assert.equal(v.receiptPhrase, '')
    }
    claims += 1
  }
  assert.equal(claims, 228)
  t.diagnostic(`${claims} synthetic claims, ${withReceipts} with receipts, 0 shadow`)
})

test('tracked representative is privacy-safe and first-level receipt presentation preserves raw bytes', () => {
  const source = readFileSync(REPRESENTATIVE_FIXTURE, 'utf8')
  const doc = JSON.parse(source)
  assert.doesNotMatch(source, /\/Users\/|\\Users\\|command_identity|BEGIN [A-Z ]*PRIVATE KEY|AKIA/)
  for (const node of doc.nodes || []) {
    const view = claimView(doc, node.id)
    const firstLevel = [
      view.claimPresentation.text,
      view.sentence,
      view.story,
      view.run,
      view.runDate,
      view.ledgerLine,
      ...view.evidence.flatMap((item) => [
        item.presentation.action,
        item.presentation.summary,
        item.presentation.path?.label,
        item.presentation.path?.display,
        item.presentation.line,
      ]),
    ]
    for (const value of firstLevel) {
      assert.doesNotMatch(String(value || ''), /\/Users\//)
      assert.doesNotMatch(String(value || ''), /Command:/)
    }
    assert.deepEqual(view.rawReceiptDetails.claim, doc.receipts?.[node.id]?.claim || { text: node.label || '' })
  }
  assert.equal(Object.keys(doc.receipts).length, doc.nodes.length)
})
