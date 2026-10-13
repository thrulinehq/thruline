/* The export rail's three touches of the outside world, and nothing else:
 *
 *   readRecords(id)  GET /api/changes/<id>/export — whether each record the
 *                    Change rests on can be read now (T-5-4). A read only.
 *   save(file)       hands exactly `file.text` to the browser as a download
 *                    (Blob → download), named `file.filename`.
 *   copy(text)       puts `text` on the clipboard.
 *
 * Nothing is posted and nothing is written on the server. The read and the
 * save are the workspace export sheet's own (`export-sheet.jsx`), so the two
 * doors make the same request and the same file. */
import { readExportRecords, saveText } from '../../../workspace/export-sheet.jsx'

/** @returns {Promise<{status: 'ready', doc: object} | {status: 'failed', note: ?string}>} */
export async function readRecords(id) {
  const { records, note } = await readExportRecords(id)
  return records ? { status: 'ready', doc: records } : { status: 'failed', note: note || null }
}

export const IO = Object.freeze({
  save: (file) => saveText(file),
  copy: (text) => {
    const clip = typeof window !== 'undefined' && window.navigator ? window.navigator.clipboard : null
    return clip && typeof clip.writeText === 'function' ? clip.writeText(text) : Promise.reject(new Error('no clipboard'))
  },
})
