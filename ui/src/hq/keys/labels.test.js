/* C30 R-KEYS (CTO sign-off fix): a key's LABEL is never cut off.
 *
 * The first sheets showed "What counts as d…" in the `?` map and in
 * Settings › Keyboard: the label cell was one line, hidden past its edge, with
 * an ellipsis. A person reading the keys must be able to read every word of
 * what a key does, so a label wraps onto a second line and its row grows to
 * hold it. The state word beside it ("not yet", "on a Change") already never
 * breaks, so it is the label that gives way — by wrapping, never by clipping.
 *
 * There is no layout engine in this harness (jsdom measures every box as 0),
 * so scrollWidth against clientWidth cannot be asked here. What can be asked,
 * and what decides the clipping, is the style each label's element and its row
 * are drawn with — read from the real rendered markup of both surfaces, in
 * every mode and on every screen — and the source the markup comes from.
 * (The widths were also measured in a real browser when this was fixed; see
 * the DECISIONS entry.) Synthetic fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { JSDOM } from 'jsdom'
import { build } from 'esbuild'

const here = dirname(fileURLToPath(import.meta.url))
const read = (p) => readFileSync(join(here, p), 'utf8')

async function bundle(entry) {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: entry, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-labels-')), 'm.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const U = await bundle(`
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { DcScreen } from '../dc/runtime.jsx'
import { designLink } from '../artboards.jsx'
import { withFrame } from '../data/project.js'
import { KeysMap } from './overlay.jsx'
import { KEY_TABLE, LIVE_SCREENS, entriesFor } from './table.js'
import { MODES } from './rows.js'
import SettingsT from '../screens/Settings/template.jsx'
import SettingsL from '../screens/Settings/logic.js'
import { settingsFromServer } from '../screens/Settings/from-server.js'
export { KEY_TABLE, LIVE_SCREENS, MODES, settingsFromServer, entriesFor }
export function settingsHtml({ data, mode }) {
  const C = withFrame(SettingsL)
  const prefs = { status: 'read', prefs: { keys: mode }, fromFile: ['keys'], file: 'read', pending: {}, writeError: {} }
  const inst = new C({ data, prefs, actions: {}, frame: {}, at: { screen: 'settings' } })
  inst.state = Object.assign({}, inst.state, { nav: 'keyboard' })
  return renderToStaticMarkup(React.createElement(SettingsT, { v: inst.renderVals(), link: (h) => h }))
}
export const galleryHtml = () => renderToStaticMarkup(React.createElement(DcScreen, { Logic: SettingsL, Template: SettingsT, props: {}, link: designLink }))
export const mapHtml = (props) => renderToStaticMarkup(React.createElement(KeysMap, Object.assign({ onClose: () => {} }, props)))
`)

const HOME = '/Users/someone'
const NOW = new Date(2026, 9, 7, 16, 10)
const STORAGE = { bytes: 5.4e9, path: HOME + '/.thruline' }
const LICENCE_FREE = { status: 'absent', paid_gate: true, entitlements: { approve_version: { allowed: false, status: 'absent' } }, technical: {} }
const LIST = { complete: true, projects: [{ id: 'p1', name: 'shop', root: HOME + '/code/shop', complete: true, threads: 3, sessions: 9, claims: 7, sources: ['claude_code'], last_activity: NOW.toISOString(), whose_move: { you: 2, agent: 0, done: 1 } }] }
const DATA = U.settingsFromServer({ storage: STORAGE, licence: LICENCE_FREE, projects: LIST, now: NOW })

/** An element's inline style as {property: value}. */
function styleOf(el) {
  const out = {}
  for (const part of (el.getAttribute('style') || '').split(';')) {
    const i = part.indexOf(':')
    if (i > 0) out[part.slice(0, i).trim().toLowerCase()] = part.slice(i + 1).trim().toLowerCase()
  }
  return out
}
const doc = (html) => new JSDOM('<!DOCTYPE html><body>' + html).window.document

/** The three cells of every row of the table named `label`: [row, key cell, label cell, state cell]. */
function rowsOf(document, label) {
  const table = document.querySelector('[role="table"][aria-label="' + label + '"]')
  assert.ok(table, 'the table “' + label + '” is drawn')
  return [...table.querySelectorAll('[role="row"]')].map((row) => [row, ...row.querySelectorAll('[role="cell"]')])
}

/** A label cell is never clipped: it wraps, it has no ellipsis, and nothing about it or its row hides what overflows. */
function assertNeverCut(row, label, what) {
  const s = styleOf(label)
  const r = styleOf(row)
  assert.ok(!/ellipsis/.test(s['text-overflow'] || ''), what + ': “' + label.textContent + '” has an ellipsis')
  assert.ok(!/^(nowrap|pre)$/.test(s['white-space'] || ''), what + ': “' + label.textContent + '” is held to one line (' + s['white-space'] + ')')
  assert.ok(!/^(hidden|clip)$/.test(s.overflow || ''), what + ': “' + label.textContent + '” clips what overflows')
  assert.ok(/^(anywhere|break-word)$/.test(s['overflow-wrap'] || ''), what + ': “' + label.textContent + '” can wrap')
  assert.ok(!/^(hidden|clip)$/.test(r.overflow || ''), what + ': the row of “' + label.textContent + '” clips what overflows')
  assert.ok(r['min-height'] && !r.height, what + ': the row of “' + label.textContent + '” is a minimum height, so a wrapped label has room (' + (r.height || r['min-height']) + ')')
  assert.ok(!/ellipsis/.test(r['text-overflow'] || ''), what + ': the row has an ellipsis')
}

test('Settings › Keyboard: every label is whole and wraps, in each mode', () => {
  for (const mode of U.MODES) {
    const document = doc(U.settingsHtml({ data: DATA, mode }))
    const rows = rowsOf(document, 'Keyboard map')
    assert.equal(rows.length, U.KEY_TABLE.length, mode + ': one row per key')
    rows.forEach(([row, , label], i) => {
      assert.equal(label.textContent, U.KEY_TABLE[i].label, mode + ': the label is the table’s, word for word')
      assertNeverCut(row, label, 'Settings, ' + mode)
    })
    // the long ones the first sheet cut are among them
    assert.ok(rows.some(([, , l]) => l.textContent === 'What counts as done'))
  }
})

test('the `?` map: every label is whole and wraps, on every screen and in each mode', () => {
  let seen = 0
  for (const screen of U.LIVE_SCREENS) {
    for (const mode of U.MODES) {
      const document = doc(U.mapHtml({ screen, at: { screen: 'x', change: 'c1', project: 'p1' }, mode }))
      const want = [...U.entriesFor(screen).here, ...U.entriesFor(screen).everywhere].map((e) => e.label)
      const got = []
      for (const label of ['On this screen', 'Everywhere']) {
        const table = document.querySelector('[role="table"][aria-label="' + label + '"]')
        if (!table) continue
        for (const row of table.querySelectorAll('[role="row"]')) {
          const cell = row.querySelectorAll('[role="cell"]')[1]
          got.push(cell.textContent)
          assertNeverCut(row, cell, 'the map on ' + screen + ', ' + mode)
          seen += 1
        }
      }
      assert.deepEqual(got, want, screen + ', ' + mode + ': every label of the screen, whole')
    }
  }
  assert.ok(seen > 500, 'enough rows were looked at: ' + seen)
})

test('the design gallery’s Settings draws the same label cell, so the canvas and the product cannot differ in how a label is cut', () => {
  const rows = rowsOf(doc(U.galleryHtml()), 'Keyboard map')
  assert.ok(rows.length > 5)
  for (const [row, , label] of rows) assertNeverCut(row, label, 'the gallery')
})

test('the source: no element that draws a key label is held to one line, hidden past its edge or ellipsed', () => {
  const surfaces = [['../screens/Settings/template.jsx', /<span role="cell" style=\{\{([^}]*)\}\}>\{k\?\.label\}<\/span>/g], ['overlay.jsx', /<span role="cell" style=\{\{([^}]*)\}\}>\{k\.label\}<\/span>/g]]
  for (const [file, re] of surfaces) {
    const found = [...read(file).matchAll(re)]
    assert.equal(found.length, 1, file + ': the one element that draws the label is found')
    const style = found[0][1]
    assert.ok(!/ellipsis|nowrap|overflow: ?['"]?(hidden|clip)/i.test(style), file + ': ' + style)
  }
  // and the row each sits in is not a fixed height
  assert.ok(!/role="row" style=\{\{[^}]*\bheight: ?["']36px/.test(read('../screens/Settings/template.jsx').replace(/minHeight/g, 'min_h')), 'Settings’ keymap row is not a fixed 36 px')
  assert.ok(!/role="row" style=\{\{[^}]*\bheight: ?'36px/.test(read('overlay.jsx').replace(/minHeight/g, 'min_h')), 'the map’s row is not a fixed 36 px')
})
