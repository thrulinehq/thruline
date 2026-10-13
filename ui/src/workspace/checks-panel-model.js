import { C } from './tokens.js'

// Presentation copy from the supplied Checks design. Results only come from
// the server; catalog membership, elapsed time and exit zero cannot invent one.
export const CHECK_GROUPS = [
  { id: 'ran', title: 'Did the commands actually run?' },
  { id: 'tests', title: 'Do the tests really cover this work?' },
  { id: 'binding', title: 'Is this about your code, and still current?' },
  { id: 'shape', title: 'Does the code still hold its shape?' },
  { id: 'done', title: 'Is the request finished?' },
  { id: 'tools', title: 'Extra checks from tools you configured' },
  { id: 'record', title: 'Check receipts in this session' },
]
export const NATIVE_CHECKS = [
  ['EVD-04', 'ran', 'Checking command execution…', 'Command execution and outcome'],
  ['EVD-10', 'ran', 'Checking evidence completeness…', 'Complete captured evidence'],
  ['EVD-12', 'ran', 'Verifying the result receipt…', 'A reproducible result receipt'],
  ['EVD-05', 'tests', 'Checking test counts and scope…', 'The accepted test suite and counts'],
  ['TST-01', 'tests', 'Checking that required tests actually ran…', 'Required tests actually executed'],
  ['FUN-04', 'tests', 'Checking expected behavior and edge cases…', 'Expected behavior and edge cases'],
  ['EVD-06', 'binding', 'Matching evidence to your code…', 'Evidence bound to the examined code'],
  ['EVD-07', 'binding', 'Checking whether results are current…', 'Results still match current inputs'],
  ['FUN-01', 'shape', 'Checking build and types…', 'Build and type correctness'],
  ['ARC-01', 'shape', 'Checking architecture boundaries…', 'Accepted architecture boundaries'],
  ['ARC-02', 'shape', 'Checking dependency cycles…', 'Prohibited dependency cycles'],
  ['PER-02', 'shape', 'Measuring memory use…', 'Memory use within the accepted budget'],
  ['DEL-01', 'shape', 'Checking build outputs…', 'Expected build artifacts'],
  ['REQ-03', 'done', 'Checking completion coverage…', 'Coverage of the accepted criteria'],
].map(([id, group, running, title]) => ({ id, group, running, title }))

export const CHECK_STATES = {
  fail: { label: 'Requirement failed', short: 'failed', color: C.coral, glyph: '×', rank: 0 },
  blocked: { label: 'Blocked', short: 'blocked', color: C.yellow, glyph: '⊘', rank: 1 },
  unknown: { label: 'Incomplete', short: 'incomplete', color: C.yellow, glyph: '?', rank: 2 },
  stale: { label: 'Needs a fresh check', short: 'stale', color: C.yellow, glyph: '↻', rank: 3 },
  running: { label: 'Running', short: 'running', color: C.blue, glyph: '◌', rank: 4 },
  reported: { label: 'Recorded report', short: 'reported', color: C.inkSoft, glyph: '“', rank: 5 },
  pass: { label: 'Passed', short: 'passed', color: C.green, glyph: '✓', rank: 6 },
  not_run: { label: 'Not run', short: 'not run', color: C.inkDim, glyph: '○', rank: 7 },
}
export const checkState = state => CHECK_STATES[state] || CHECK_STATES.unknown
const normalize = state => ({ passed: 'pass', failed: 'fail' }[state] || (CHECK_STATES[state] ? state : 'unknown'))
const PASS_COPY = {
  'EVD-04': 'The captured process outcomes match the accepted exit outcomes.',
  'EVD-05': 'The suite identities and reported counts meet this accepted evidence condition.',
  'EVD-06': 'The evidence is bound to the examined source and declared environment.',
  'EVD-07': 'The recorded inputs match the current project reading.',
  'EVD-10': 'The required evidence was captured completely enough to admit this result.',
  'EVD-12': 'Replaying the recorded evidence agrees with the stored outcome.',
  'TST-01': 'The required test discovery and execution condition was met. Individual test outcomes are retained in the receipt.',
}

export function applicableChecks(check) {
  const v = check?.verification
  if (!v) return []
  const ids = ['EVD-04', 'EVD-06', 'EVD-07', 'EVD-10', 'EVD-12']
  const p = v.pack
  if (p?.kind === 'suite') ids.push('EVD-05', 'TST-01')
  if (p?.kind === 'completion') { ids.splice(ids.indexOf('EVD-04'), 1); ids.push('REQ-03') }
  if (p?.kind === 'artifact') ids.push('DEL-01')
  if (p?.kind === 'architecture') ids.push(p.check === 'cycles' ? 'ARC-02' : 'ARC-01')
  if (p?.kind === 'resource') ids.push('PER-02')
  if (['build', 'typecheck'].includes(check.category)) ids.push('FUN-01')
  if (v.cases?.length) ids.push('FUN-04')
  return ids
}

export function nativeRows(data, error = '') {
  if (!data?.enabled) return []
  const rows = []
  for (const check of data.contract?.checks || []) {
    const result = data.pack_results?.find(r => r.id === check.id)
    const assessments = result?.catalog_results?.length ? result.catalog_results : applicableChecks(check).map(check_id => ({ check_id, status: 'not_run' }))
    for (const assessment of assessments) {
      const info = NATIVE_CHECKS.find(c => c.id === assessment.check_id)
      if (!info) continue
      // Only the active command animates. Its evidence assessments settle when
      // the receipt arrives; no fictitious per-assessment progress is exposed.
      const status = error || data.readError ? 'unknown' : data.job?.running && data.job.check === check.id ? 'running' : data.pending_draft_change ? 'stale' : normalize(assessment.status)
      const history = (data.history || []).find(h => h.record.run_id === result?.run_id)
      rows.push({ ...info, key: `${check.id}:${info.id}`, kind: 'native', check, result, status,
        receipt: result?.run_id || null, record: history?.record,
        description: error || data.readError || (status === 'running' ? 'Capturing a new attempt. Its result is not yet known.' : status === 'pass' && PASS_COPY[info.id] ? PASS_COPY[info.id] : result?.explanation || 'No result is recorded for this accepted check.'),
        criteria: (data.contract.criteria || []).filter(c => c.check_ids.includes(check.id)),
        scope: (result?.scope || check.verification?.scope || []).join(', '),
      })
    }
  }
  return rows
}

export function toolRows(data, error = '') {
  if (!data || data.results_error || error) return []
  // Keep receipt identities distinct: two runs/scopes from the same tool are
  // never silently merged into a single green badge.
  return (data.results?.runs || []).filter(r => r.record?.plan).map(row => ({
    key: `tool:${row.binding_id}`, id: row.record.plan.pack, group: 'tools', kind: 'tool',
    title: row.record.plan.criterion?.requirement || row.record.plan.pack,
    status: normalize(row.assessment?.state), description: row.assessment?.explanation || 'The evidence does not establish this condition.',
    receipt: row.binding_id, toolResult: row, scope: row.record.plan.root,
  }))
}

export function recordedRows(session) {
  return (session.checks || []).map(c => ({ key: `record:${c.id}`, id: c.runner || c.category || 'Captured check', group: 'record', kind: 'record',
    title: c.command || c.runner || 'Recorded check', status: 'reported',
    description: `${c.exit == null ? 'No process exit was captured.' : `Recorded process exit: ${c.exit}.`} ${c.bound ? 'Bound to the final code in this reading.' : 'Final-code binding is not established.'}`,
    receipt: c.id, node: c.node, scope: c.scopeNote || 'Only the recorded command scope.', record: c,
  }))
}

export function checkSummary(rows) {
  const counts = {}
  for (const row of rows) counts[row.status] = (counts[row.status] || 0) + 1
  const receipts = new Set(rows.map(r => r.receipt).filter(Boolean)).size
  const attention = ['fail', 'blocked', 'unknown', 'stale'].reduce((n, k) => n + (counts[k] || 0), 0)
  let end = 0
  const stops = Object.keys(CHECK_STATES).filter(k => counts[k]).map(k => {
    const start = end; end += counts[k] / rows.length * 100
    return `${checkState(k).color} ${start}% ${end}%`
  })
  const state = counts.running ? 'running' : counts.fail ? 'fail' : attention ? 'unknown' : counts.not_run ? 'not_run' : counts.reported ? 'reported' : counts.pass ? 'pass' : 'not_run'
  return { counts, receipts, attention, state, dial: stops.length ? `conic-gradient(from -90deg, ${stops.join(', ')})` : `conic-gradient(${C.yellow} 0turn .02turn, ${C.surf2} .02turn 1turn)`,
    title: counts.running ? 'Checking the work…' : counts.fail ? `${counts.fail} check ${counts.fail === 1 ? 'condition is' : 'conditions are'} not met.` : attention ? 'Some checks still need attention.' : counts.not_run ? 'Accepted checks are waiting to run.' : counts.reported ? 'Recorded evidence is ready to inspect.' : counts.pass ? 'The recorded checks passed within their scope.' : 'No check receipts in this reading.',
  }
}

export function checkReport(rows, project) {
  return [`ThruLine checks · ${project || 'Current reading'}`, 'Results cover their recorded scope only.', ...rows.map(r => `\n${r.id} · ${r.check?.id || r.title}\n${checkState(r.status).label}: ${r.description}\nScope: ${r.scope || 'Not established'}\nReceipt: ${r.receipt || 'None recorded'}`)].join('\n')
}

// The overview counts project checks, not the individual evidence conditions
// used to evaluate them. The original rows remain attached for inspection.
export function resultState(status) {
  const labels = { fail: 'Failed', pass: 'Passed', not_run: 'Not checked', stale: 'Run again', unknown: 'Not confirmed', blocked: 'Could not run', reported: 'Recorded only', running: 'Checking' }
  return { ...checkState(status), label: labels[status] || labels.unknown }
}

export function checkName(check) {
  const name = String(check?.id || 'Check').replace(/[_:-]+/g, ' ').replace(/\s+/g, ' ').trim()
  return name.charAt(0).toUpperCase() + name.slice(1)
}

const briefValue = value => {
  const full = typeof value === 'string' ? value : JSON.stringify(value)
  return full == null ? 'not captured' : full.length > 90 ? `${full.slice(0, 87)}…` : full
}

function resultExplanation(row) {
  if (row.status === 'running') return 'Running now. The new result is not ready yet.'
  if (row.status === 'not_run') return 'This check has not run yet.'
  if (row.status === 'stale') return 'The code, inputs or requirements changed. This result needs a fresh check.'
  if (row.status === 'unknown') return 'There is not enough current evidence to confirm this result.'
  if (row.status === 'blocked') return 'This check could not run. Review what is missing in its details.'
  if (row.status === 'reported') return 'An outcome was recorded, but it does not verify the requirement.'
  if (row.status === 'pass') return 'The agreed conditions passed for this check.'
  const observations = row.record?.finish?.pack?.observations || []
  const failed = observations.filter(o => o.outcome === 'failed')
  const mismatch = failed.find(o => Object.hasOwn(o, 'expected') && JSON.stringify(o.expected) !== JSON.stringify(o.observed))
  if (mismatch) return `Expected ${briefValue(mismatch.expected)}; got ${briefValue(mismatch.observed)}.`
  const exit = failed.find(o => o.expected_exit != null && o.exit != null && o.expected_exit !== o.exit)
  if (exit) return `Expected process exit ${exit.expected_exit}; got ${exit.exit}.`
  return 'This check did not meet its agreed requirement.'
}

export function nextForResult(row) {
  const name = row.title
  if (row.status === 'fail') return `Review what failed in ${name}, resolve the mismatch, then rerun the check.`
  if (row.status === 'stale') return `Run ${name} again against the current code and requirements.`
  if (row.status === 'blocked') return `Review the setup needed for ${name} before trying again.`
  if (row.status === 'unknown') return `Inspect the missing evidence for ${name} before relying on its result.`
  if (row.status === 'not_run') return `Run ${name} to get its first result.`
  if (row.status === 'reported') return `Review the recorded outcome for ${name} and collect a check against the requirement.`
  if (row.status === 'running') return `Wait for ${name} to finish. Its result will appear here.`
  return 'No action is needed for this check while its inputs remain current.'
}

export function simpleResults(data, localError = '', otherRows = []) {
  const evidence = nativeRows(data, localError)
  const results = data?.enabled ? (data.contract?.checks || []).map(check => {
    const result = data.pack_results?.find(r => r.id === check.id)
    const record = (data.history || []).find(h => h.record.run_id === result?.run_id)?.record
    let status = localError || data.readError ? 'unknown'
      : data.job?.running && data.job.check === check.id ? 'running'
        : data.pending_draft_change ? 'stale' : result ? normalize(result.status) : 'not_run'
    if (status === 'pass' && !result?.run_id) status = 'unknown'
    return {
      key: `check:${check.id}`, id: check.id, kind: 'native', title: checkName(check), status, check, result, record,
      receipt: result?.run_id || null,
      detail: localError || data.readError || result?.explanation || 'No result has been captured for this check.',
      scope: (result?.scope || check.verification?.scope || []).join(', '),
      criteria: (data.contract?.criteria || []).filter(c => c.check_ids.includes(check.id)),
      evidence: evidence.filter(row => row.check.id === check.id),
    }
  }) : []
  return [...results, ...otherRows.map(row => ({ ...row, detail: row.description, evidence: [] }))].map(row => ({
    ...row, description: resultExplanation(row), next: nextForResult(row),
  }))
}

export function resultsOverview(results, { loading = false, readError = '', toolsError = '', enabled = false } = {}) {
  const summary = checkSummary(results)
  const focus = [...results].sort((a, b) => checkState(a.status).rank - checkState(b.status).rank)[0]
  let title, explanation, next
  if (loading) {
    title = 'Reading the results…'; explanation = 'Checking which results belong to this reading.'; next = ''
  } else if (readError) {
    title = 'Results could not be refreshed'; explanation = 'Current local results are not confirmed.'; next = 'Refresh the evidence. Review the connection details if it remains unavailable.'
  } else if (summary.counts.running) {
    title = 'Checking the work…'; explanation = 'Results update as each check finishes.'; next = 'Wait for the running check to finish, or stop it below.'
  } else if (summary.attention) {
    title = 'Needs attention'; explanation = `${summary.attention} ${summary.attention === 1 ? 'check needs' : 'checks need'} your attention. Open a result to see why.`; next = nextForResult(focus)
  } else if (toolsError && results.length) {
    title = 'Some evidence is unavailable'; explanation = 'The results below are available, but tool evidence could not be read.'; next = 'Refresh the evidence or review tool setup before drawing a wider conclusion.'
  } else if (summary.counts.not_run) {
    title = summary.counts.pass ? 'Some checks are still outstanding' : 'Ready to check'; explanation = `${summary.counts.not_run} ${summary.counts.not_run === 1 ? 'check has' : 'checks have'} not run yet.`; next = nextForResult(results.find(r => r.status === 'not_run'))
  } else if (summary.counts.reported) {
    title = 'Not verified yet'; explanation = 'These recorded outcomes do not establish that the agreed requirements passed.'; next = nextForResult(results.find(r => r.status === 'reported'))
  } else if (summary.counts.pass) {
    title = 'The checked work passed'; explanation = 'All checks shown here passed within their agreed scope.'; next = 'Review the evidence if needed. Work outside these checks has not been verified here.'
  } else {
    title = 'Not verified yet'; explanation = enabled ? 'No checks are configured for this project yet.' : 'No verified check results are available in this reading.'
    next = enabled ? 'Define the checks for this project, then run them to get results.' : 'Review the session findings below, or set up project checks to verify the work.'
  }
  const unavailable = Boolean(readError || toolsError && results.length && !summary.counts.running && !summary.counts.fail)
  return { ...summary, title, explanation, next, focus,
    state: unavailable ? 'unknown' : summary.state,
    dial: unavailable ? `conic-gradient(${C.yellow} 0turn .02turn, ${C.surf2} .02turn 1turn)` : summary.dial,
  }
}

export function resultReport(results, project) {
  return [`ThruLine checks · ${project || 'Current reading'}`, 'Results cover their recorded scope only.', ...results.map(row => [
    '', `${row.title} — ${resultState(row.status).label}`, row.description,
    `Next step: ${row.next}`, `Scope: ${row.scope || 'Not established'}`,
    `Check: ${row.id}`, `Receipt: ${row.receipt || 'None recorded'}`,
    ...(row.evidence || []).map(e => `  ${e.id}: ${checkState(e.status).label} — ${e.description}`),
  ].join('\n'))].join('\n')
}
