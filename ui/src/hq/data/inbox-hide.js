/* One append-only Inbox hide address. The server computes each content digest
 * from recorded event IDs; the browser sends back only the digest it read.
 * A new recorded step gives the row a new digest and shows it again. */

export const INBOX_HIDES_ADDRESS = (project) => `/api/hq/projects/${encodeURIComponent(project)}/inbox-hides`

/** The current row addresses by thread. The server offers one row per thread. */
export function hideRows(doc, project) {
  if (!doc || String(doc.project) !== String(project) || !Array.isArray(doc.rows)) return null
  return new Map(doc.rows.filter((row) => row && row.thread_id && row.row_kind && row.row_sha256)
    .map((row) => [String(row.thread_id), row]))
}

/** Keep one hide or show act. A refusal preserves the server's sentence. */
export async function writeInboxHide(project, row, act = 'hidden', fetchImpl = typeof fetch === 'function' ? fetch : null) {
  if (!project || !row || !row.thread_id || !['thread', 'prompt'].includes(row.row_kind) || !/^[0-9a-f]{64}$/.test(row.row_sha256 || '') || !['hidden', 'shown'].includes(act)) {
    throw new Error('This Inbox row cannot be hidden until its record is read again.')
  }
  if (!fetchImpl) throw new Error('ThruLine’s local server did not answer. Reload the page to try again.')
  let response
  try {
    response = await fetchImpl(INBOX_HIDES_ADDRESS(project), {
      method: 'POST', credentials: 'same-origin',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ act, row_kind: row.row_kind, thread_id: row.thread_id, expected_sha256: row.row_sha256 }),
    })
  } catch {
    throw new Error('ThruLine’s local server did not answer. Reload the page to try again.')
  }
  const answer = await response.json().catch(() => null)
  if (!response.ok) throw new Error(answer && answer.message ? answer.message : 'ThruLine could not keep that change. Reload the Inbox to try again.')
  return answer
}
