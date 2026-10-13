/* Settings, census settings#3 (HQ stopped): what the page draws once the page has seen HQ go away.
 *
 * DESIGN-SPEC §3.10 "System-state bar conditions" (stopped): "‖ HQ stopped." / "This page shows what it
 * read at HH:MM, and every action on it is off. Recording carries on without it." with `thruline ui` and
 * a "Copy restart command" button that is never disabled; the pill reads "HQ stopped", the read stamp
 * "read HH:MM · not refreshing". The live page used to hard-code `hq.stopped = false`. Synthetic data. */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { settingsFromServer } from './from-server.js'

const here = dirname(fileURLToPath(import.meta.url))
const PROJECTS = { projects: [{ id: 'shop', name: 'shop', root: '/scratch/shop', sessions: 3 }] }
const STORAGE = { bytes: 1200, path: '/scratch/record' }
const LANES = { claude_code: { live: 2, root: '~/.claude/projects' }, codex: { live: 0, root: '~/.codex/sessions' }, window_minutes: 5 }
const READ_AT = new Date(2026, 9, 12, 10, 41, 3)
const hm = '10:41'

let modulePromise
function settingsModule() {
  if (!modulePromise) modulePromise = (async () => {
    const source = `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import Logic from './logic.js'; import Template from './template.jsx'; export { Logic }; export const render = (v) => renderToStaticMarkup(React.createElement(Template, { v, link: (x) => x }))`
    const built = await build({
      stdin: { contents: source, resolveDir: here, loader: 'jsx' },
      bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
      banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
      loader: { '.css': 'empty' }, logLevel: 'silent',
    })
    const path = join(await mkdtemp(join(tmpdir(), 'settings-stopped-')), 'module.mjs')
    await writeFile(path, built.outputFiles[0].text)
    return import(pathToFileURL(path).href)
  })()
  return modulePromise
}
const draw = async (over = {}) => {
  const { Logic, render } = await settingsModule()
  const data = settingsFromServer({ storage: STORAGE, lanes: LANES, projects: PROJECTS, current: 'shop', now: READ_AT, ...over })
  const v = new Logic({ data, at: { screen: 'settings' }, actions: {} }).renderVals()
  return { data, v, markup: render(v) }
}

test('the served shape says HQ is stopped only when the page was told so', () => {
  assert.equal(settingsFromServer({ storage: STORAGE, now: READ_AT }).hq.stopped, false)
  assert.equal(settingsFromServer({ storage: STORAGE, now: READ_AT, hqStopped: false }).hq.stopped, false)
  assert.equal(settingsFromServer({ storage: STORAGE, now: READ_AT, hqStopped: true }).hq.stopped, true)
})

test('HQ stopped draws the system-state bar with the restart command, the HQ stopped pill and the not-refreshing stamp (DESIGN-SPEC §3.10)', async () => {
  const { v, markup } = await draw({ hqStopped: true })
  assert.equal(v.sys.show, true)
  assert.equal(v.sys.lead, 'HQ stopped.')
  assert.equal(v.sys.text, 'This page shows what it read at ' + hm + ', and every action on it is off. Recording carries on without it.')
  assert.equal(v.sys.cmd, 'thruline ui')
  assert.equal(v.sys.label, 'Copy restart command')
  assert.match(markup, /<div role="status"[^>]*>.*HQ stopped\./)
  assert.match(markup, /Copy restart command/)
  assert.equal(v.pill.word, 'HQ stopped')
  assert.equal(v.readStamp, 'read ' + hm + ' · not refreshing')
  assert.equal(v.statusLeft, 'HQ stopped · showing what was read at ' + hm)
})

test('a running HQ draws no system-state bar', async () => {
  const { v, markup } = await draw({ hqStopped: false })
  assert.equal(v.sys.show, false)
  assert.doesNotMatch(markup, /HQ stopped/)
})

test('the live Settings screen wires the heartbeat and freezes the read time once HQ stops', async () => {
  const src = await readFile(join(here, 'index.jsx'), 'utf8')
  assert.match(src, /useHqStopped\(/, 'Settings listens for HQ going away')
  assert.match(src, /hqStopped/, 'and tells the served shape')
  assert.match(src, /readAtRef|lastRead/, 'the read time stays what was last read while stopped')
})
