/* T8 — facts that depend on the workspace render as Fog when the workspace
 * could not be read.
 *
 * The engine types the cause (`claims --json` `claims[].unobservable_cause`,
 * from `binding.rs` `UnobservableCause`); `adapter.js` `workspaceFactsKnown` is
 * the only reader of it, and every surface — the canvas node, the inspector
 * pill, the Verdict/Brief headline and the Change screen's line (the confirmed
 * session's Verdict cards) — renders the adapter's answer. Acceptance, card T8:
 *   2. the typed cause maps to Fog whatever `capture_degraded` says;
 *   3. every surface shows Fog with `applies.inputs_unknown_version` and no
 *      "changed after" / "still applies" sentence and no file count;
 *   4. without the cause the claim returns to its computed word;
 *   5. no other file reads `unobservable_cause`.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import * as A from './adapter.js'

const { adaptSession, autoLayout } = A
const verdictWord = (...a) => A.verdictWord(...a)
const workspaceFactsKnown = (...a) => A.workspaceFactsKnown(...a)
import { buildVerdict, claimState } from './verdict-model.js'
import { HQ_COPY } from './hq-copy.js'
import { stepRow } from './read-model.js'

const SENTENCE = HQ_COPY.change.coverage.applies.inputs_unknown_version

const explain = { shape: 'thruline.explain', schema_version: 1, session: 's-t8', what_happened: [] }
function claimsDoc(claim, degraded = false) {
  return {
    shape: 'thruline.claims', schema_version: 1, session: 's-t8', capture_degraded: degraded,
    claims: [{
      id: 'clm_t8', kind: 'tests_pass', text: 'All tests pass.', verdict: 'unverifiable', verifier: 'tests_pass@1',
      evidence: ['chk_t8'], reasons: ['not bindable: the tree changed between this check and the final snapshot'],
      warnings: [], findings: [],
      binding: { unobservable: { reason: 'the changed paths could not be computed', cause: 'workspace_unreadable' } },
      unobservable_cause: 'workspace_unreadable',
      ...claim,
    }],
  }
}
const adapt = (claim, degraded) => adaptSession({ key: 's-t8', explain, claims: claimsDoc(claim, degraded) })

function strings(v, out = []) {
  if (typeof v === 'string') out.push(v)
  else if (Array.isArray(v)) v.forEach((x) => strings(x, out))
  else if (v && typeof v === 'object') Object.values(v).forEach((x) => strings(x, out))
  return out
}

test('(t8.2) the typed cause maps to Fog even when capture is not degraded; without it the RSV §3.1 split stands', () => {
  assert.equal(verdictWord('unverifiable', false, false), 'fog')
  assert.equal(verdictWord('unverifiable', false, true), 'couldnt')
  assert.equal(verdictWord('unverifiable', true, true), 'fog')
  assert.equal(workspaceFactsKnown({ unobservable_cause: 'workspace_unreadable' }), false)
  assert.equal(workspaceFactsKnown({ unobservable_cause: 'unplaced' }), true, 'only the workspace cause is Fog here')
  assert.equal(workspaceFactsKnown({}), true)
  const S = adapt({}, false)
  assert.equal(S.claims[0].word, 'fog')
  assert.equal(claimState(S.claims[0], false), 'fog', 'the Verdict model reads the same rule')
})

test('(t8.3) canvas node, inspector pill, headline and the Change line all say Fog with the one sentence', () => {
  const S = adapt({}, false)
  const laid = autoLayout(S)
  const node = (laid.nodes || S.nodes || []).find((n) => n.id === S.claims[0].node)
  assert.ok(node, 'the claim is drawn')
  assert.equal(node.word, 'fog', 'canvas node and inspector pill render the node word')

  for (const register of ['manager', 'operator']) {
    const V = buildVerdict(S, { register })
    assert.equal(V.state, 'fog', register)
    assert.equal(V.counts.fog, 1, register)
    assert.ok(V.headline.includes(register === 'manager' ? SENTENCE : 'Workspace unreadable'), `${register} headline: ${V.headline}`)
    assert.doesNotMatch(V.headline, /not recorded|Recording gap/, 'no recording gap was claimed')
    const card = V.findings.find((c) => c.id === 'clm_t8')
    assert.ok(card, 'the Change screen line (a Verdict card of the confirmed session)')
    assert.equal(card.state, 'fog')
    assert.equal(card.kind, 'workspace_unreadable')
    assert.equal(card.record.text, SENTENCE)
    const said = strings([V.headline, card.record, card.do, card.said, HQ_COPY.kinds[card.kind]]).join(' \n ')
    assert.doesNotMatch(said, /changed after|still applies|\b\d+ (files?|paths?)\b/i, said)
  }
})

test('(t8.4) with git back the same claim returns to its computed word — Fog is the missing fact, not the fixture', () => {
  const proven = adapt({ verdict: 'supported', unobservable_cause: undefined, binding: { no_relevant_change_since: { edits_ignored: 0, paths_ignored: ['tests/test_rate.py'] } } }, false)
  assert.equal(proven.claims[0].word, 'proven')
  assert.equal(buildVerdict(proven).state, 'proven')
  const recorded = adapt({ unobservable_cause: undefined }, false)
  assert.equal(recorded.claims[0].word, 'couldnt', 'unverifiable with no workspace cause stays Recorded')
})

test('(t8.1) a served workspace_since failure reads "Could not finish." and what needs it is withheld', () => {
  const all = [
    { phase: 'workspace_since', status: 'failed', count: null, detail: null, elapsed_ms: null, reason: 'workspace_unreadable' },
    { phase: 'run_verifiers', status: 'withheld', count: null, detail: null, elapsed_ms: null, withheld_by: 'workspace_since' },
  ]
  const ws = stepRow(all[0], all)
  assert.equal(ws.note, HQ_COPY.read.status.failed)
  assert.equal(ws.reason, 'workspace_unreadable')
  assert.equal(ws.count, '')
  const rv = stepRow(all[1], all)
  assert.match(rv.note, /^Withheld: /)
})

test('(t8.5) only adapter.js reads unobservable_cause', () => {
  const root = fileURLToPath(new URL('..', import.meta.url))
  const readers = []
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name)
      if (statSync(p).isDirectory()) walk(p)
      else if (/\.(js|jsx)$/.test(name) && !/\.test\.js$/.test(name) && readFileSync(p, 'utf8').includes('unobservable_cause')) readers.push(p.slice(root.length))
    }
  }
  walk(root)
  assert.deepEqual(readers, ['workspace/adapter.js'])
})
