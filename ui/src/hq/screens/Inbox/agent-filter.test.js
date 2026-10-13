/* C30 P0-5: every Inbox filter's count equals the rows it shows. On the live
 * record a thread that is the agent's move is filed under "Recording now", not
 * "Waiting on the agent" (no note was copied), so "The agent's 1" used to show
 * an empty list. Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import * as P from '../../data/project.js'
import { decisionsModel } from '../../data/decisions.js'
import { DECISIONS_LIST } from '../../data/decisions-fixture.js'
import { LIST, VIEW, NOW } from '../../data/project-fixture.js'
import Logic from './logic.js'
export function vals({ live = true, filter = 'all', decisions } = {}) {
  const data = live ? P.inboxData({ projects: LIST, view: VIEW, now: NOW }) : undefined
  const props = { data, decisions }
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  inst.state = { ...inst.state, filter }
  return inst.renderVals()
}
export const served = () => decisionsModel({ status: 'served', doc: DECISIONS_LIST })`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-inbox-filter-')), 'inbox.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

/** The rows a filter shows: every drawn group's rows, plus decision rows. */
function shown(v) {
  let n = 0
  for (const g of ['you', 'check', 'agent', 'live']) if (v.g[g] === 'flex') n += v.groups[g].length
  if (v.slot && v.slot.decisionGroup) n += v.slot.decisionGroup.props.rows.length
  return n
}
const chip = (v, key) => v.filters.find((f) => f.key === key).count

test('"The agent’s" shows the agent’s threads it counts', () => {
  const v = M.vals({ filter: 'agent' })
  assert.equal(chip(v, 'agent'), '1')
  assert.equal(shown(v), 1, 'the thread the agent is working on is listed')
  assert.ok(v.groups.live.every((r) => r.id === 'live-live1'))
})

test('every filter’s count equals the rows it shows, with decisions on the record', () => {
  const decisions = M.served()
  for (const [key, filter] of [['you', 'you'], ['agent', 'agent'], ['live', 'live']]) {
    const v = M.vals({ filter, decisions })
    assert.equal(String(shown(v)), chip(v, key), `filter ${key}`)
  }
  // the decision group is drawn under All, and the All list holds it
  const all = M.vals({ filter: 'all', decisions })
  assert.ok(all.slot.decisionGroup, 'All draws the decision group')
})

test('the gallery’s "The agent’s" still draws what the canvas drew', () => {
  const v = M.vals({ live: false, filter: 'agent' })
  assert.equal(v.g.agent, 'flex')
  assert.equal(v.g.live, 'none')
})
