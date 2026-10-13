/* Copy as Markdown: the only way anything on this page can leave it — the
 * clipboard, which the person then pastes themselves. No fetch, no POST, no
 * address: ThruLine posts nothing to a pull request (C30 R-PR).
 *
 * @param {string} text  exactly what the preview draws: the note, then the setup line (`data.copy`)
 * @param {{clipboard?: {writeText: (t: string) => Promise<void>}}} [nav]
 * @returns {Promise<void>} rejects when this browser gives no clipboard
 */
export function copyMarkdown(text, nav = globalThis.navigator) {
  const clip = nav && nav.clipboard
  if (typeof text !== 'string' || !clip || typeof clip.writeText !== 'function') {
    return Promise.reject(new Error('This browser gives this page no clipboard.'))
  }
  return Promise.resolve(clip.writeText(text))
}

export default copyMarkdown
