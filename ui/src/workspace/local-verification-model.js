import { adaptSession } from './adapter.js'

export const LOCAL_STATES = { pass: ['✓', 'Passed'], fail: ['×', 'Requirement failed'], stale: ['↻', 'Needs a fresh check'], blocked: ['⊘', 'Blocked'], unknown: ['?', 'Incomplete'], not_run: ['◌', 'Not run'], running: ['●', 'Running'] }
export function localVerificationSession(data) {
  const version = data.pack_version || data.coverage?.version || 'local-verification-pack/3'
  const project = data.location?.repo || 'Local project'
  const key = 'local-verification'
  const entries = data.contract?.objective ? [{ seq: 1, kind: 'user_prompt', text: data.contract.objective }] : []
  const receipts = {}
  const labels = {}
  for (const receipt of [...(data.history || [])].reverse()) {
    const r = receipt.record
    const current = data.pack_results?.find(c => c.run_id === r.run_id)
    const state = current?.status || r.historical_state
    let seq = entries.length + 1
    entries.push({ seq, kind: 'snapshot', text: `Inputs examined for ${r.check.id}. Code snapshot: ${r.start.inputs.tree || 'unavailable'}` })
    receipts[`ev${seq}`] = r.run_id
    labels[`ev${seq}`] = { title: `Code examined · ${r.check.id}`, state }
    for (const o of r.finish?.pack?.observations || []) {
      seq = entries.length + 1
      entries.push({ seq, kind: o.observed?.protocol === 'thruline-completion/2' ? 'verification' : 'exec', tool: r.check.runner, text: `${o.description}\n${o.reason}\n${o.expected != null ? `Expected: ${JSON.stringify(o.expected)}\nObserved: ${JSON.stringify(o.observed)}\n` : ''}Exit: ${o.exit ?? 'no process outcome claimed'}\nReceipt: ${r.run_id}`, input: JSON.stringify({ argv: r.check.argv, input: o.input }) })
      receipts[`ev${seq}`] = r.run_id
      labels[`ev${seq}`] = { title: o.description, state: state === 'stale' ? 'stale' : { passed: 'pass', failed: 'fail' }[o.outcome] || 'unknown' }
    }
    seq = entries.length + 1
    entries.push({ seq, kind: 'verification', text: `${LOCAL_STATES[state]?.[1] || 'Incomplete'} · ${r.check.id}\n${current?.explanation || receipt.verification.explanation}\nReceipt: ${r.run_id}` })
    receipts[`ev${seq}`] = r.run_id
    labels[`ev${seq}`] = { title: `${LOCAL_STATES[state]?.[1] || 'Incomplete'} · ${r.check.id}`, state }
  }
  const claims = (data.pack_results || []).filter(r => r.id && (data.contract?.criteria || []).some(c => c.check_ids.includes(r.id))).map(result => {
    const record = (data.history || []).find(h => h.record.run_id === result.run_id)?.record
    const failures = result.status === 'fail' ? (record?.finish?.pack?.observations || []).filter(o => o.outcome === 'failed') : []
    return { id: `local-requirement:${result.id}`, kind: 'accepted_requirement', text: data.contract.criteria.filter(c => c.check_ids.includes(result.id)).map(c => c.description).join(' '),
      verdict: { pass: 'supported', fail: 'unsupported' }[result.status] || 'unverifiable', verifier: version, scope: result.scope.join(', '), extractor: 'accepted local requirement', reasons: [result.explanation], warnings: [], evidence: [],
      findings: failures.map(o => ({ detector: `${result.id}-${o.id}`, severity: 'fail', basis: 'observed', detail: `${o.description}. ${o.expected != null ? `Expected ${JSON.stringify(o.expected)}; observed ${JSON.stringify(o.observed)}.` : o.reason} ${o.exit != null ? `Process exit ${o.exit}.` : 'Evaluation of captured evidence; no process outcome claimed.'}`, path: result.scope[0], resolveWith: 'Repair the demonstrated mismatch and rerun this accepted check.' })) }
  })
  const session = adaptSession({ key, project, origin: 'Local collector · accepted requirements and captured verification runs', harness: version, explain: { shape: 'thruline.explain', schema_version: 1, session: key, what_happened: entries }, claims: { shape: 'thruline.claims', schema_version: 1, session: key, claims } })
  session.nodes = session.nodes.filter(n => n.id !== 'ending')
  session.wires = session.wires.filter(w => w.from[0] !== 'ending' && w.to[0] !== 'ending')
  session.stop = {}
  for (const claim of session.claims) {
    claim.localCheckId = claim.id.replace('local-requirement:', '')
    const result = data.pack_results.find(r => r.id === claim.localCheckId)
    claim.prove = result?.explanation || 'Run the accepted observable check.'
    if (result?.run_id) {
      receipts[claim.node] = result.run_id
      for (const finding of claim.findings) receipts[finding.node] = result.run_id
      const event = Object.keys(receipts).filter(id => id.startsWith('ev') && receipts[id] === result.run_id).at(-1)
      const from = session.nodes.find(n => n.id === event), to = session.nodes.find(n => n.id === claim.node)
      if (from && to) {
        from.pinsR.push({ id: 'accepted-result', label: 'captured result', t: 'ref' }); to.pinsL.push({ id: 'accepted-result', label: 'required evidence', t: 'ref' })
        session.wires.push({ id: `local-${claim.node}`, from: [from.id, 'accepted-result'], to: [to.id, 'accepted-result'], kind: 'ref', c: 'blue', label: 'Captured check evaluates this accepted requirement' })
      }
    }
  }
  session.label = project.split('/').at(-1)
  session.localVerification = true
  // Project requirements and check receipts are not an agent-session capture.
  // adaptSession's capture defaults cannot establish recorder health here.
  session.capture = {
    sources: [],
    redactions: 0,
    condition: 'not assessed — no captured session evidence',
    recorderLine: 'No captured session evidence is available in this project verification reading. Local check receipts do not establish recorder health.',
  }
  session.localReceiptByNode = receipts
  session.localData = data
  session.readingNote = 'Accepted requirement → examined code → observed result → receipt. Current applicability is checked against the local project.'
  for (const node of session.nodes) {
    if (labels[node.id]) {
      node.title = labels[node.id].title
      node.fam = ({ pass: 'green', fail: 'red', stale: 'amber', blocked: 'amber' })[labels[node.id].state] || 'grey'
    }
  }
  return session
}

export function coverageCounts(rows = []) {
  return { total: rows.length, implemented: rows.filter(r => r.availability === 'implemented').length, validated: rows.filter(r => r.validation_status === 'validated').length }
}

// One in-flight status read across graph, catalog and HQ; slow snapshots cannot
// create overlapping polls or starve the last successful response.
let reading = null
export function readLocalVerification() {
  if (!reading) reading = (async () => {
    const signal = AbortSignal.timeout(20000)
    // Another browser or CLI reader may hold the snapshot lock. Contention is
    // not missing evidence: wait for a fresh reading, within one total deadline.
    // Every other error still reaches the workspace's incomplete-state path.
    for (;;) {
      signal.throwIfAborted()
      const response = await fetch('/api/verification', { cache: 'no-store', signal })
      const data = await response.json()
      if (response.status === 503 && data.error === 'verification_read_busy') {
        await new Promise(resolve => setTimeout(resolve, 500))
        continue
      }
      if (!response.ok) throw new Error(data.detail || data.error || `HTTP ${response.status}`)
      return data
    }
  })().finally(() => { reading = null })
  return reading
}
