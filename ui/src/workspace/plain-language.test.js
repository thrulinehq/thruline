import test from 'node:test'
import assert from 'node:assert/strict'
import { actionLabel, selectedRecord, explainSelection } from './plain-language.js'

test('action names describe recognizable attempts and preserve raw commands', () => {
  const event = { kind: 'tool_call', text: 'Bash {"command":"cd app && python -m pytest tests/test_totals.py"}' }
  assert.equal(actionLabel(event), 'Run automated checks')
  assert.match(event.text, /cd app && python/)
  assert.equal(actionLabel({ kind: 'tool_call', text: 'Bash {"command":"echo \\"pytest passed\\""}' }), 'Run a terminal action')
  assert.equal(actionLabel({ kind: 'tool_call', text: 'Read {"file_path":"/project/cart/totals.py"}' }), 'Read cart/totals.py')
})

test('missing identifiers never select an unrelated claim or event', () => {
  const session = { claims: [{ text: 'Unrelated' }], checks: [{}], events: [{}], nodes: [{ id: 'other' }] }
  assert.deepEqual(selectedRecord(session, null), {})
  assert.equal(selectedRecord(session, 'other').claim, undefined)
  assert.equal(selectedRecord(session, 'other').event, undefined)
})

test('widened captured input names the action beyond the old display truncation', () => {
  const event = { kind: 'tool_call', text: 'Bash {"command":"cd /a/very/long/path…', tool: 'Bash', input: '{"command":"cd /a/very/long/path && pytest tests/test_checkout.py"}' }
  assert.equal(actionLabel(event), 'Run automated checks')
  assert.ok(event.text.endsWith('…'))
})

test('only the fallback completion verifier marks a statement as derived', () => {
  const claim = { id: 'c', node: 'clm', kind: 'task_complete', extractor: 'rules', text: 'Finished the change.', word: 'unverifiable' }
  const session = { nodes: [{ id: 'clm' }], claims: [claim] }
  assert.equal(explainSelection(session, 'clm').label, 'What the agent reported')
  claim.verifier = 'unclaimed_completion@1'
  assert.match(explainSelection(session, 'clm').fact, /quoted final message is context/)
})

test('a failed action does not imply an unresolved bug or dishonest agent', () => {
  const session = { nodes: [{ id: 'ev1', seq: 1 }], events: [{ seq: 1, kind: 'tool_result', text: 'Bash FAILED' }] }
  assert.match(explainSelection(session, 'ev1').meaning, /fixed later/)
  assert.doesNotMatch(explainSelection(session, 'ev1').meaning, /hallucinat|dishonest/)
})
