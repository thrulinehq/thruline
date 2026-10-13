/* Labels explain observed action types. They never invent the agent's intent,
 * infer success from a command, or replace the original record. */
import { viewFromEntry, extractString } from './readable.js'

const clipped = (s, n = 100) => String(s || '').length > n ? String(s).slice(0, n - 1) + '…' : String(s || '')
export const humanPath = path => String(path || '').split('/').filter(Boolean).slice(-2).join('/')

export function actionLabel(event) {
  if (!event) return 'Recorded activity'
  const v = viewFromEntry(event)
  const raw = String(event.text || '')
  const tool = v.tool || raw.split(' ')[0]
  const command = v.argv || extractString(v.argsText, 'command')?.value || raw
  const path = v.path || extractString(v.argsText, 'file_path')?.value || extractString(v.argsText, 'path')?.value
  if (event.kind === 'tool_result') return /FAILED|failed/.test(raw) ? 'An action returned a failure' : /outcome not stated/.test(raw) ? 'An action returned · outcome not recorded' : /\bok\b/.test(raw) ? 'An action finished successfully' : 'An action returned a result'
  if (event.kind === 'snapshot') return 'Saved a picture of the code'
  if (event.kind === 'user_prompt') return clipped(raw.replace(/^<[^>]+>/, ''), 110) || 'A new request arrived'
  if (event.kind === 'fs_change') return `Changed ${humanPath(v.path) || 'a project file'}`
  if (event.kind === 'session_start') return 'Work began'
  if (event.kind === 'session_end') return 'The session ended'
  if (event.kind === 'stop') return 'The agent handed back its work'
  if (/compaction/.test(event.kind || '')) return 'The conversation was condensed'
  if (['Read', 'read_file'].includes(tool)) return `Read ${humanPath(path) || 'a project file'}`
  if (['Edit', 'Write', 'MultiEdit', 'apply_patch'].includes(tool)) return `Update ${humanPath(path) || 'project files'}`
  if (['Grep', 'Glob'].includes(tool)) return 'Search the project'
  if (['tool_call', 'exec'].includes(event.kind)) {
    // Recognized executable spellings only. A word inside a quoted message
    // cannot establish that a test was run; the title remains an attempt.
    const c = String(command).replace(/\\n/g, '\n')
    if (/(?:^|[;&|\n]|"command"\s*:\s*")\s*(?:python(?:3)?\s+-m\s+pytest|pytest|cargo\s+test|npm\s+(?:run\s+)?test|pnpm\s+test|vitest)\b/.test(c)) return 'Run automated checks'
    if (/\bgit\s+(?:diff|status|log|show)\b/.test(c)) return 'Inspect the project changes'
    if (/\bgit\s+(?:commit|add)\b/.test(c)) return 'Save changes in version history'
    if (/(?:^|[;&|\n]|"command"\s*:\s*")\s*(?:rg|grep|find)\b/.test(c)) return 'Search files for relevant code'
    if (/(?:^|[;&|\n]|"command"\s*:\s*")\s*(?:ls|pwd|cat|sed|head|tail)\b/.test(c)) return 'Inspect files and project setup'
    if (event.risk) return 'Review a potentially destructive action'
    return tool && !['Bash', 'exec', 'shell', 'shell_command'].includes(tool) ? `${tool.replace(/_/g, ' ')} activity` : 'Run a terminal action'
  }
  return clipped(event.readText || raw || 'Recorded activity')
}

export function selectedRecord(session, selected) {
  if (!selected) return {}
  const node = (session.nodes || []).find(n => n.id === selected)
  const claim = (session.claims || []).find(c => c.node === selected || (node?.objId && c.id === node.objId))
  const check = (session.checks || []).find(c => c.node === selected || (node?.objId && c.id === node.objId))
  const finding = (session.claims || []).flatMap(c => c.findings || []).find(f => f.node === selected || (node?.finding && f.key === node.finding))
  const promise = (session.promises || []).find(p => p.node === selected)
  const event = node?.seq == null ? undefined : (session.events || []).find(e => e.seq === node.seq)
  return { node, claim, check, finding, promise, event }
}

export function explainSelection(session, selected) {
  if (session.localVerification) {
    const claim = session.claims?.find(c => c.node === selected)
    const runId = session.localReceiptByNode?.[selected]
    const result = session.localData?.pack_results?.find(r => claim ? r.id === claim.localCheckId : r.run_id === runId)
    if (result) return { label: 'ACCEPTED REQUIREMENT · LOCAL VERIFICATION', title: result.status === 'fail' ? 'This check found a concrete mismatch' : result.status === 'pass' ? 'The accepted check passed' : result.status === 'stale' ? 'This result needs a fresh check' : 'This requirement is not settled yet', fact: result.explanation, meaning: claim?.text || 'The graph shows the actual inputs and outputs captured for this local check. These are observations, not an agent’s completion message.', next: 'Open Verify → Local checks to inspect the receipt or run this accepted check again.' }
  }
  const { node, claim, check, finding, promise, event } = selectedRecord(session, selected)
  const base = { sources: selected ? [selected] : [], fact: '', meaning: '', next: '', title: '', label: 'About this evidence' }
  if (!node) return { ...base, title: 'Did the work actually get finished?', fact: `This reading contains ${(session.events || []).length.toLocaleString('en-US')} recorded events and ${(session.checks || []).length} check receipts.`, meaning: 'Select an action or review point in the graph. I’ll explain what it shows, what it does not establish, and the next useful check.', next: 'Start with a request, a reported completion, or a failed result. The full technical record stays one click away.' }
  if (finding) return { ...base, label: 'A point to investigate', title: finding.name || finding.key.replace(/_/g, ' '), fact: finding.detail || finding.derivedDetail || 'A detector raised this condition.', meaning: finding.basis === 'gap' ? 'Something needed to decide is missing. That alone does not prove the agent made a mistake.' : 'Check the linked evidence and any later recovery before deciding whether this is still a problem.', next: finding.resolveWith || 'Compare the affected work with the accepted requirement, then look for a check that covers it.' }
  if (claim) {
    const derived = claim.kind === 'task_complete' && claim.verifier === 'unclaimed_completion@1'
    const yes = claim.word === 'proven', no = ['not-proven', 'not_proven'].includes(claim.word)
    return { ...base, label: derived ? 'Completion review · derived from the record' : 'What the agent reported', title: yes ? 'Evidence supports this specific statement' : no ? 'The statement is not established by this record' : 'We cannot establish completion yet', fact: derived ? 'ThruLine is reviewing whether the work was finished. The quoted final message is context; it may be an offer to do more work.' : `The agent said: “${claim.text}”`, meaning: yes ? 'This applies only to the evidence and scope of this reading. It does not automatically cover later edits or prove every part of the product is correct.' : 'The record does not give us enough support to call this finished. It does not mean nothing was done or that the agent was dishonest.', next: 'Match the original request to the changed work and a fresh check on the code being delivered.' }
  }
  if (check) {
    const failed = Number.isInteger(check.exit) && check.exit !== 0
    const counts = check.parsed || {}
    return { ...base, label: 'Recorded check result', title: failed ? 'This check did not pass' : check.exit === 0 ? 'This run finished successfully' : 'The result is not fully recorded', fact: Number.isFinite(counts.passed) ? `${counts.passed} tests passed${Number.isFinite(counts.failed) ? ` and ${counts.failed} failed` : ''} in this recorded run.` : 'Open the receipt for the exact command and captured outcome.', meaning: check.bound ? 'The receipt is tied to the final code state in this reading. Whether it covers the accepted requirement is a separate question.' : 'We cannot tie this result to the final code state in this reading. An earlier pass cannot verify later changes.', next: failed ? 'Read the failing case, check for a matching recovery, and repeat the affected check after a correction.' : 'Confirm that this check exercises the behavior you asked for, with the current code, dependencies, and environment.' }
  }
  if (promise) return { ...base, label: 'Follow-up mentioned by the agent', title: 'Does this follow-up still need doing?', fact: `The agent said: “${promise.text}”`, meaning: 'An offer may depend on your decision. An accepted task may still be pending. This text alone does not establish either completion or abandonment.', next: 'Look for acceptance, a later completion, an explicit cancellation, or a remaining dependency.' }
  if (event) {
    const failure = event.kind === 'tool_result' && /FAILED|failed/.test(event.text || '')
    return { ...base, label: 'What happened here', title: actionLabel(event), fact: event.kind === 'snapshot' ? 'ThruLine recorded an identifier for the code state at this point.' : event.kind === 'tool_call' ? 'The record shows an action was requested. Its result is a separate event.' : event.kind === 'tool_result' ? 'This is a recorded tool response.' : 'This event is part of the captured session history.', meaning: failure ? 'A failure is a place to investigate. It may have been expected or fixed later; a matching recovery needs its own evidence.' : event.kind === 'snapshot' ? 'Think of this as a version marker. Matching markers can link a check to code, but the marker alone does not prove the code works.' : 'Recorded order tells us what came before and after. It does not, by itself, prove why an action happened.', next: failure ? 'Inspect the original error and follow the related work to see whether the same problem was resolved.' : 'Open the original record or follow its evidence links to see the result and any later check.' }
  }
  return { ...base, title: node.readTitle || node.title || 'Selected evidence', fact: 'This object summarizes or references part of the record.', meaning: 'Expand it to inspect the individual sources. A group is a way to navigate, not an extra claim of correctness.', next: 'Follow a source or open the technical details.' }
}
