/* C30 P0-7: First run states only what the record holds. Nothing served says
 * whether a folder is a Git repository or whether its hooks are installed
 * (the capture-health route is C30 §R-ROUTES), so on the live product both
 * read "Not checked yet" — never "Git repository", "Found" or "Installed".
 * The gallery (DEMO) is unchanged. Synthetic fixtures only. */
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
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import * as P from '../../data/project.js'
import { LIST, NOW } from '../../data/project-fixture.js'
import Template from './template.jsx'
import Logic from './logic.js'
export function render({ live = true } = {}) {
  const data = live ? P.firstRunData({ projects: LIST, now: NOW }) : undefined
  const props = { data }
  const inst = new (P.withFrame(Logic))(props)
  inst.props = props
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}
export const firstRun = () => P.firstRunData({ projects: LIST, now: NOW })`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), 'hq-first-run-')), 'first-run.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const text = (html) => html.replace(/<[^>]+>/g, ' ').replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/\s+/g, ' ')

test('the data never says a folder is a Git repository when nothing served it', () => {
  for (const f of M.firstRun().folders) assert.equal(f.git, null, f.name)
})

test('live First run says "Not checked yet" for Git and for the hooks, and claims neither', () => {
  const t = text(M.render())
  assert.doesNotMatch(t, /Git repository(?! yet)/, 'no folder is claimed to be a Git repository')
  assert.doesNotMatch(t, /\bFound\b/)
  assert.doesNotMatch(t, /Not known/)
  assert.match(t, /Git not checked yet/)
  assert.match(t, /Claude Code hooks ThruLine hasn’t checked this folder’s hooks yet[^◌]*◌ Not checked yet/)
})

test('the gallery still draws what the canvas drew', () => {
  const t = text(M.render({ live: false }))
  assert.match(t, /Git repository/)
})
