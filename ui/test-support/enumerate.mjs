/* Exhaustive enumeration of recorded texts, through the real renderers
 * (EXPORT-MD-ESCAPE-R3).
 *
 * Round 2's adversary found the two breaks round 3 closes by enumerating token
 * strings (19 million through `markdownOf`, 116 hits, all in `marked`) after every
 * test the round-2 coder had written passed. A sampled or hand-picked test finds
 * what its author thought of; an enumeration finds what is there. This runs one
 * in the committed suite: every string up to a length over an alphabet, each put
 * in the exported file the way recorded text is (`markdownOf`), each rendered by
 * CommonMark, `marked` and `markdown-it`, each read for anything live
 * (`hazardsRendered`).
 *
 * It uses worker threads, because the work is CPU only and shards cleanly: each
 * worker walks the same sequence and judges every `shards`-th string. The
 * workers import the production helper themselves, so what is judged is the
 * file the export writes and not a copy of it.
 *
 * Lives outside `ui/src`, like the rest of `ui/test-support`: nothing in the app
 * imports it.
 */

import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads'
import { availableParallelism } from 'node:os'

/* Every string of length `min..max` over `symbols`, in a fixed order. */
function* strings(symbols, min, max) {
  for (let n = min; n <= max; n += 1) {
    const at = new Array(n).fill(0)
    for (;;) {
      let s = ''
      for (const i of at) s += symbols[i]
      yield s
      let p = n - 1
      while (p >= 0 && (at[p] += 1) === symbols.length) { at[p] = 0; p -= 1 }
      if (p < 0) break
    }
  }
}

/** The character that ends one field and starts the next, in the `fields` context. */
export const FIELD_END = '\u0001'

/* How a string is put in the file. Each returns the Markdown `markdownOf` writes. */
const CONTEXTS = {
  /* The text alone, as a list row: the way the file carries a criterion, a run's name, a decision. */
  item: (markdownOf) => (s) => markdownOf([{ t: 'ul', items: [{ text: s, lines: [], gaps: [] }] }]),
  /* The text as the label of a link the person recorded with an https target: the label is where `marked` reads a backslash and the punctuation after it before it reads anything else. */
  https_label: (markdownOf) => (s) => markdownOf([{ t: 'ul', items: [{ text: `[${s}](https://ok.example/)`, lines: [], gaps: [] }] }]),
  /* Several fields of one row at once: `\u0001` in the string ends a field. The first is the row's text, the second a line under it, the third its labelled gap line, any more further lines under it. One paragraph in the file: what one field opens another can close. */
  fields: (markdownOf) => (s) => {
    const [text, line, gap, ...more] = s.split(FIELD_END)
    return markdownOf([{ t: 'ul', items: [{ text, lines: [line, ...more].filter((x) => x !== undefined), gaps: gap === undefined ? [] : [gap] }] }])
  },
}

async function judgeShard({ symbols, min, max, context, shard, shards }) {
  const PE = await import('../src/workspace/packet-export.js')
  const MI = await import('./markdown-instrument.mjs')
  const put = CONTEXTS[context](PE.markdownOf)
  let total = 0
  let judged = 0
  let found = 0
  const samples = []
  for (const s of strings(symbols, min, max)) {
    total += 1
    if ((total - 1) % shards !== shard) continue
    judged += 1
    const live = MI.hazardsRendered(put(s))
    if (live.length) {
      found += 1
      if (samples.length < 12) samples.push({ input: s, finding: live[0] })
    }
  }
  return { total, judged, found, samples }
}

/* A worker: judge one shard and post the answer. */
if (!isMainThread && workerData && workerData.enumerate) {
  judgeShard(workerData.enumerate).then((r) => parentPort.postMessage(r), (e) => parentPort.postMessage({ error: String((e && e.stack) || e) }))
}

/**
 * Every string of length 1..`max` over `symbols`, through `markdownOf` in
 * `context`, through all three renderers.
 * @param {{symbols: string[], max: number, context?: string, min?: number}} o
 * @returns {Promise<{total: number, judged: number, found: number, samples: {input: string, finding: string}[], workers: number}>}
 */
export async function enumerate({ symbols, max, min = 1, context = 'item' }) {
  const shards = Math.max(1, Math.min(4, availableParallelism() - 1))
  const parts = await Promise.all(Array.from({ length: shards }, (_, shard) => new Promise((resolve, reject) => {
    const w = new Worker(new URL(import.meta.url), { workerData: { enumerate: { symbols, min, max, context, shard, shards } } })
    w.once('message', (m) => (m.error ? reject(new Error(m.error)) : resolve(m)))
    w.once('error', reject)
  })))
  return {
    total: parts[0].total,
    judged: parts.reduce((a, p) => a + p.judged, 0),
    found: parts.reduce((a, p) => a + p.found, 0),
    samples: parts.flatMap((p) => p.samples).slice(0, 12),
    workers: shards,
  }
}
