// A workspace export temporarily owns the shared projection lock. Retry only
// that read-only 503; never repeat an action or hide a different server failure.
function pause(milliseconds, signal) {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(signal.reason)
    const abort = () => { clearTimeout(timer); reject(signal.reason) }
    const timer = setTimeout(() => { signal?.removeEventListener('abort', abort); resolve() }, milliseconds)
    signal?.addEventListener('abort', abort, { once: true })
  })
}
export async function readIntegration(url, { signal, fetcher = fetch, wait = pause } = {}) {
  // A complete compiler closure can take tens of seconds to re-read while
  // another projection holds the lock. Keep this read retry bounded and
  // cancellable across project changes; never retry an action request.
  const delays = [250, 500, 1000, 2000, ...Array(15).fill(4000)]
  for (let attempt = 0; ; attempt++) {
    signal?.throwIfAborted()
    const response = await fetcher(url, { signal, cache: 'no-store' })
    const body = await response.json()
    if (response.ok) return body
    if (response.status === 503 && body.error === 'a_reading_is_in_progress') {
      if (attempt < delays.length) { await wait(delays[attempt], signal); continue }
      throw new Error('HQ is still refreshing this workspace. Refresh tool evidence again when the reading finishes; saved receipts are retained.')
    }
    throw new Error(body.error || `Could not read tool evidence (${response.status})`)
  }
}
