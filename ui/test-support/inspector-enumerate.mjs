/* EXPORT-MD-ESCAPE-R5: production Workspace.markdown, eight recorded fields,
 * all three pinned renderers. The fixture replaces the inspector's data supply,
 * never the Markdown writer. At most four worker threads judge the full files.
 * Test-only, outside ui/src; nothing here reaches the production bundle. */
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads'
import { availableParallelism } from 'node:os'
import { pathToFileURL } from 'node:url'
import * as MI from './markdown-instrument.mjs'

export const INSPECTOR_FIELDS = ['title', 'typeLabel', 'id', 'clause', 'sentence', 'evidenceKey', 'evidenceValue', 'origin']

export function inspectorMarkdown(Workspace, field, text, wordSlug = 'not_proven') {
  const i = { title: 'Title', wordSlug, typeLabel: 'Claim', id: 'id1', clause: 'clause', sentences: [{ text: 'sentence' }], evidence: [{ k: 'key', v: 'value' }] }
  let origin = 'this reading'
  if (field === 'sentence') i.sentences[0].text = text
  else if (field === 'evidenceKey') i.evidence[0].k = text
  else if (field === 'evidenceValue') i.evidence[0].v = text
  else if (field === 'origin') origin = text
  else i[field] = text
  const w = Object.create(Workspace.prototype)
  w.inspector = () => i
  w.S = () => ({ origin })
  return w.markdown()
}

function* strings(symbols, max) {
  for (let n = 1; n <= max; n += 1) {
    const at = new Array(n).fill(0)
    for (;;) {
      yield at.map((i) => symbols[i]).join('')
      let p = n - 1
      while (p >= 0 && (at[p] += 1) === symbols.length) { at[p] = 0; p -= 1 }
      if (p < 0) break
    }
  }
}

async function judge({ compiled, symbols, max, shard, shards }) {
  const { Workspace } = await import(pathToFileURL(compiled).href)
  let total = 0
  let judged = 0
  let found = 0
  const samples = []
  for (const text of strings(symbols, max)) {
    total += 1
    if ((total - 1) % shards !== shard) continue
    for (const field of INSPECTOR_FIELDS) {
      const md = inspectorMarkdown(Workspace, field, text)
      const live = MI.hazardsRendered(md)
      judged += 1
      if (live.length) {
        found += 1
        if (samples.length < 12) samples.push({ field, text, finding: live[0] })
      }
    }
  }
  return { total, judged, found, samples }
}

if (!isMainThread && workerData?.inspectorEnumeration) {
  judge(workerData.inspectorEnumeration).then((r) => parentPort.postMessage(r), (e) => parentPort.postMessage({ error: e.stack || String(e) }))
}

export async function enumerateInspector({ compiled, symbols, max }) {
  const shards = Math.max(1, Math.min(4, availableParallelism() - 1))
  const parts = await Promise.all(Array.from({ length: shards }, (_, shard) => new Promise((resolve, reject) => {
    const w = new Worker(new URL(import.meta.url), { workerData: { inspectorEnumeration: { compiled, symbols, max, shard, shards } } })
    w.once('message', (r) => r.error ? reject(new Error(r.error)) : resolve(r))
    w.once('error', reject)
  })))
  const judged = parts.reduce((n, p) => n + p.judged, 0)
  return { strings: parts[0].total, fields: INSPECTOR_FIELDS, judged, renders: judged * MI.RENDERERS.length, workers: shards, found: parts.reduce((n, p) => n + p.found, 0), samples: parts.flatMap((p) => p.samples).slice(0, 12) }
}
