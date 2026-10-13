/* C30 R-ROUTES "Step view" (CTO sign-off fix): the step page's side note says
 * what every other screen's status bar says — "Nothing leaves this machine" —
 * from the one shared sentence (`PRIVACY`, data/project.js), and not the older
 * hedge that was copied from the canvas's Whole story ("Raw source and
 * transcripts never leave your device by default"). A privacy sentence that
 * differs from screen to screen reads as a different promise on each. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'
import { thread, NOW, TID } from './step-fixture.js'

const here = dirname(fileURLToPath(import.meta.url))

const M = await (async () => {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { changeSnapshot } from '../../data/change.js'
import * as FC from './from-change.js'
import Logic from './logic.js'
import Template from './template.jsx'
import { withFrame, PRIVACY } from '../../data/project.js'
import { PortedScreen } from '../../screen.jsx'
export { changeSnapshot, FC, PRIVACY, withFrame, Logic }
export function render(data, at) {
  return renderToStaticMarkup(React.createElement(PortedScreen, { Logic: withFrame(Logic), Template, at, project: data.project.id, data, extra: { frame: {} } }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-step-privacy-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const snap = M.changeSnapshot(thread(), null, NOW, { storageBytes: 2.1e9 })
const at = { screen: 'step', change: TID, project: 'p1', step: 2 }
const html = () => M.render(M.FC.stepViewFromChange(snap, at, { timeline: null, timelineError: null }), at)
const doc = () => new JSDOM('<!DOCTYPE html><body>' + html()).window.document

test('the one privacy sentence is the words every other screen says', () => {
  assert.equal(M.PRIVACY, 'Nothing leaves this machine')
  // the frame hands it to every live screen's status bar
  const Frame = M.withFrame(M.Logic)
  const props = { data: M.FC.stepViewFromChange(snap, at, { timeline: null, timelineError: null }), frame: {}, at }
  const inst = new Frame(props)
  inst.props = props
  assert.equal(inst.renderVals().privacy, M.PRIVACY)
})

test('the step page’s side note says exactly that, as a line of its own', () => {
  const d = doc()
  const aside = d.querySelector('aside')
  assert.ok(aside, 'the page has its side column')
  const lines = [...aside.querySelectorAll('span')].map((s) => s.textContent)
  assert.ok(lines.includes(M.PRIVACY), 'a side-note line is exactly “' + M.PRIVACY + '”: ' + JSON.stringify(lines.slice(-4)))
})

test('and the hedge is gone from the whole page: no "by default", no "your device", no "raw source"', () => {
  const text = doc().body.textContent
  for (const gone of [/by default/i, /never leave your device/i, /your device/i, /raw source/i, /makes no network calls/i]) {
    assert.ok(!gone.test(text), 'the page still says ' + gone)
  }
})

test('the source takes the sentence from the shared constant, and writes no privacy sentence of its own', () => {
  const src = readFileSync(join(here, 'template.jsx'), 'utf8')
  assert.match(src, /import \{ PRIVACY \} from '\.\.\/\.\.\/data\/project\.js'/)
  assert.match(src, /<span>\{PRIVACY\}<\/span>/)
  assert.ok(!/leave your device|Nothing leaves|no network calls/.test(src), 'no literal privacy sentence in the template')
})
