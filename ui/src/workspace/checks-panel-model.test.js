import test from 'node:test'
import assert from 'node:assert/strict'
import { nativeRows, recordedRows, toolRows, checkSummary, applicableChecks, checkReport, simpleResults, resultsOverview, resultState, resultReport } from './checks-panel-model.js'

function project() {
  return { enabled: true, accepted: true, contract: { criteria: [], checks: ['first', 'second'].map(id => ({ id, category: 'test', verification: { scope: ['main.py'], cases: [{}] } })) },
    pack_results: [
      { id: 'first', status: 'fail', run_id: 'receipt-a', catalog_results: [{ check_id: 'EVD-04', status: 'pass' }, { check_id: 'FUN-04', status: 'fail' }] },
      { id: 'second', status: 'pass', run_id: 'receipt-b', catalog_results: [{ check_id: 'FUN-04', status: 'pass' }] },
    ], history: [], job: { running: false } }
}

test('two scopes retain distinct outcomes; a successful command does not turn a failed behavior green', () => {
  const rows = nativeRows(project())
  assert.deepEqual(rows.map(r => [r.key, r.status, r.receipt]), [['first:EVD-04', 'pass', 'receipt-a'], ['first:FUN-04', 'fail', 'receipt-a'], ['second:FUN-04', 'pass', 'receipt-b']])
  const summary = checkSummary(rows)
  assert.equal(summary.receipts, 2)
  assert.equal(summary.counts.fail, 1)
  assert.equal(summary.title, '1 check condition is not met.')
  assert.match(checkReport(rows), /Requirement failed/)
})

test('availability, no receipts and absent observations cannot manufacture a pass', () => {
  const data = project(); data.pack_results = []
  const rows = nativeRows(data)
  assert.equal(rows.length, 12)
  assert.ok(rows.every(r => r.status === 'not_run' && !r.receipt))
  assert.equal(checkSummary([]).title, 'No check receipts in this reading.')
  assert.equal(checkSummary(rows).receipts, 0)
  assert.deepEqual(nativeRows({ enabled: false }), [])
})

test('only the active command animates; a read failure suppresses even a cached running or passing state', () => {
  const data = project(); data.job = { running: true, check: 'first' }
  assert.deepEqual(nativeRows(data).map(r => r.status), ['running', 'running', 'pass'])
  assert.ok(nativeRows(data, 'Cannot read inputs').every(r => r.status === 'unknown'))
  assert.ok(nativeRows({ ...data, readError: 'Disconnected' }).every(r => r.status === 'unknown'))
})

test('changed requirements invalidate current display while retaining original receipts', () => {
  const data = project(); data.pending_draft_change = true
  const rows = nativeRows(data)
  assert.ok(rows.every(r => r.status === 'stale'))
  assert.equal(rows[0].receipt, 'receipt-a')
  assert.equal(data.pack_results[0].catalog_results[0].status, 'pass')
})

test('completion accounting never invents a process execution check', () => {
  const ids = applicableChecks({ verification: { pack: { kind: 'completion' } } })
  assert.ok(ids.includes('REQ-03'))
  assert.ok(!ids.includes('EVD-04'))
})

test('exit zero in imported session evidence stays a report, and missing tool evidence stays absent', () => {
  const rows = recordedRows({ checks: [{ id: 'raw-one', exit: 0, bound: true }] })
  assert.equal(rows[0].status, 'reported')
  assert.notEqual(checkSummary(rows).state, 'pass')
  const tool = { results: { runs: [{ binding_id: 'tool-one', record: { plan: { pack: 'semgrep', criterion: {} } }, assessment: { state: 'reported' } }] } }
  assert.equal(toolRows(tool)[0].status, 'reported')
  assert.deepEqual(toolRows(tool, 'Disconnected'), [])
  assert.deepEqual(toolRows({ ...tool, results_error: 'Missing store' }), [])
})

test('the simple overview counts check outcomes without counting supporting assessments as passes', () => {
  const data = project()
  data.pack_results[0].catalog_results = applicableChecks(data.contract.checks[0]).map(check_id => ({ check_id, status: check_id === 'FUN-04' ? 'fail' : 'pass' }))
  const rows = simpleResults(data)
  assert.equal(rows.length, 2)
  assert.equal(rows[0].evidence.length, 6)
  assert.equal(rows[0].status, 'fail')
  assert.equal(rows[0].receipt, 'receipt-a')
  const summary = resultsOverview(rows)
  assert.deepEqual(summary.counts, { fail: 1, pass: 1 })
  assert.equal(summary.title, 'Needs attention')
  assert.match(summary.next, /First.*rerun/)
})

test('a failed result explains actual captured values and retains technical evidence in its report', () => {
  const data = project()
  data.history = [{ record: { run_id: 'receipt-a', finish: { pack: { observations: [{ id: 'case', outcome: 'failed', expected: 99, observed: 75 }] } } } }]
  const rows = simpleResults(data)
  assert.equal(rows[0].description, 'Expected 99; got 75.')
  assert.match(resultReport(rows), /First — Failed\nExpected 99; got 75/)
  assert.match(resultReport(rows), /EVD-04: Passed/)
  assert.match(resultReport(rows), /Receipt: receipt-a/)
  data.pending_draft_change = true
  assert.equal(simpleResults(data)[0].status, 'stale')
  assert.doesNotMatch(simpleResults(data)[0].description, /got 75/)
})

test('receipt gaps, input read failures and active runs cannot display a current pass', () => {
  const data = project()
  delete data.pack_results[1].run_id
  assert.equal(simpleResults(data)[1].status, 'unknown')
  data.job = { running: true, check: 'first' }
  assert.equal(simpleResults(data)[0].status, 'running')
  assert.match(simpleResults(data)[0].description, /not ready/)
  assert.ok(simpleResults(data, 'Disconnected').every(row => row.status === 'unknown'))
  const summary = resultsOverview(simpleResults(data, 'Disconnected'), { readError: 'Disconnected' })
  assert.equal(summary.state, 'unknown')
  assert.equal(summary.title, 'Results could not be refreshed')
})

test('outstanding checks and unavailable tool evidence qualify a passing overview', () => {
  const data = project()
  data.pack_results = [data.pack_results[1]]
  const results = simpleResults(data)
  assert.equal(results[0].status, 'not_run')
  assert.equal(resultsOverview(results).title, 'Some checks are still outstanding')
  assert.match(resultsOverview(results).next, /Run First/)
  const passed = results.filter(row => row.status === 'pass')
  assert.equal(resultsOverview(passed).title, 'The checked work passed')
  const unavailable = resultsOverview(passed, { toolsError: 'Unreadable' })
  assert.equal(unavailable.state, 'unknown')
  assert.equal(unavailable.title, 'Some evidence is unavailable')
  assert.doesNotMatch(unavailable.dial, /#6fc28a/)
})

test('no results and imported exit-zero reports remain unverified and use plain labels', () => {
  assert.equal(resultsOverview([]).title, 'Not verified yet')
  assert.match(resultsOverview([], { enabled: true }).explanation, /No checks are configured/)
  const rows = simpleResults(null, '', recordedRows({ checks: [{ id: 'report', exit: 0 }] }))
  assert.equal(rows[0].status, 'reported')
  assert.equal(resultsOverview(rows).title, 'Not verified yet')
  assert.equal(resultState('reported').label, 'Recorded only')
  assert.equal(resultState('not_run').label, 'Not checked')
  assert.equal(resultsOverview(rows, { loading: true }).title, 'Reading the results…')
})

test('a wrong process exit is explained even when the output value matches', () => {
  const data = project()
  const observation = { id: 'case', outcome: 'failed', expected: 75, observed: 75, expected_exit: 0, exit: 1 }
  data.history = [{ record: { run_id: 'receipt-a', finish: { pack: { observations: [observation] } } } }]
  assert.equal(simpleResults(data)[0].description, 'Expected process exit 0; got 1.')
  observation.expected = null
  assert.equal(simpleResults(data)[0].description, 'Expected null; got 75.')
})
