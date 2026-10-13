/* A3-F4's "done when", in a real browser: the Change route's one primary is
 * visible without scrolling at 1512×920 and at 1024×768, at altitude Brief,
 * with the leading line and eight finding cards on the screen.
 *
 * It bundles the Change shell from `ui/src/workspace` with esbuild (the UI's
 * own dependency), renders it over synthetic fixtures into a page in a fresh
 * temporary directory, opens that page in headless Chrome launched through
 * `chrome-launch.mjs` (ephemeral DevTools port, a fresh temporary profile),
 * and reads the primary's box on first paint. Nothing is served; nothing
 * leaves the machine; both temporary directories are removed at the end.
 *
 * Run:   node scripts/testing/primary-visible.mjs [<ui dir>]
 *        SHOT_DIR=<dir> also saves one screenshot per case there.
 * Exit:  0 every case visible · 1 a case not visible · 2 the check could not run
 */
import { mkdtemp, rm, writeFile, cp } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { spawnChromeWithDevTools } from './chrome-launch.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const UI = resolve(process.argv[2] || join(here, '../../ui'))
const require = createRequire(join(UI, 'package.json'))
const { build } = require('esbuild')

const VIEWPORTS = [[1512, 920], [1024, 768]]

/* Synthetic, and in the shapes the engine serves (see failure-shown.test.js,
 * a11y-fixture.js and change-workspace.test.js's CARD). */
const ENTRY = `
import React from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { ChangeShell } from './change-workspace.jsx'
import { buildChangeView } from './change-view.js'
import { deriveChangeModel } from './change-model.js'
import { A11Y_CHANGE, A11Y_COVERAGE } from './a11y-fixture.js'

const CRITERIA = ['Two retry requests for the same delivery do not enqueue duplicate work.', 'A waiting delivery survives a restart and is attempted again.']
const FAILING = {
  id: 'cccccccc-0000-4000-8000-00000000c001', project_id: 'cccccccc-0000-4000-8000-00000000p001', opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Retry a failed delivery', purpose: CRITERIA[0], requester: 'sam', source: 'person' },
  accepted_scope: { version: 1, criteria: CRITERIA, exclusions: [], allowed_scope: ['src/**'], constraints: [], check_profile: ['cargo-test'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:06:00Z', rationale: 'this is what I want checked' }, proposed_at: '2026-01-02T09:05:00Z' },
  implementation: { version: 1, bound_at: '2026-01-02T10:00:00Z', repository_identity: 'repo-synthetic-0001', base_revision: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', snapshot_id: null, workspace_identity: 'workspace-synthetic-0001' },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
}
const ITEM = { link_id: 'cce_1', kind: 'check_run', evidence_id: 'evb_1', session_id: 'cli:check:2026-01-02T10:05:00Z', basis: 'named', basis_slug: 'basis_named',
  name: 'cargo test --test=duplicate_retry', observed: 'not_proven', bears: true, applies: true, applies_slug: 'inputs_applies', asserted_at: '2026-01-02T10:05:00Z',
  execution: 'completed', execution_slug: 'execution_completed', failure_line: 'assertion failed: a second retry request must return the same job\\nleft: "job_0002"\\nright: "job_0003"',
  evidence_class: 'seeded', evidence_class_slug: 'evidence_class_seeded' }
const FAILING_COVERAGE = { schema_version: 1, change_id: FAILING.id, scope_version: 1, link_rows: 1, executions: [], criteria: [
  { index: 1, text: CRITERIA[0], state: 'evidence', state_slug: 'coverage_evidence', items: [ITEM], removed: [] },
  { index: 2, text: CRITERIA[1], state: 'none', state_slug: 'coverage_no_test_ran', items: [], removed: [] } ] }
const READY = { ...A11Y_COVERAGE, readiness: { ready: true, target_version: 'e71c5b0aaaa', headline_slug: 'readiness_ready',
  obligations: [{ id: 'o1', criterion_index: 1, text: 'The totals add up', outcome: 'satisfied', evidence_name: 'tests pass' }],
  rests_on: [{ obligation_id: 'o1', text: 'The totals add up', evidence_name: 'tests pass' }] } }
const CARD = (n) => ({ id: 'clm_' + n, sessionId: 'session-synthetic-0001', state: 'not_proven', kind: 'said_done_no_proof',
  said: { parts: [{ text: 'The AI said this was done at step ' + (40 + n) + ': ' }, { quote: 'All tests pass.', speaker: 'agent' }], refs: ['r-' + n] },
  record: { text: 'Not proven: nothing in the record shows this was done.', role: 'record', refs: ['r-' + n] },
  do: { text: 'Ask the AI for proof: the output of a check, not a sentence.', role: 'do', refs: [], prompt: 'Run the check and paste the full output.' },
  detector: null, proofNode: null })
const CARDS = Array.from({ length: 8 }, (_, n) => CARD(n + 1))

const which = new URLSearchParams(location.search).get('case')
const [change, coverage] = which === 'ready' ? [A11Y_CHANGE, READY] : [FAILING, FAILING_COVERAGE]
const hq = deriveChangeModel(change, coverage, null, null, [])
const model = { ...buildChangeView({ change, work: [] }), coverage, hq, findings: CARDS }
const root = createRoot(document.getElementById('root'))
flushSync(() => root.render(React.createElement(ChangeShell, { model, stage: 'verify', altitude: 'brief', register: 'manager',
  onGo() {}, onCopy() {}, onReload() {}, onAltitude() {}, onRegister() {} })))
window.__painted = true
`

function cdp(ws) {
  let id = 0
  const waiting = new Map()
  const events = []
  ws.addEventListener('message', (m) => {
    const msg = JSON.parse(m.data)
    if (msg.id && waiting.has(msg.id)) { const { ok, no } = waiting.get(msg.id); waiting.delete(msg.id); msg.error ? no(new Error(JSON.stringify(msg.error))) : ok(msg.result) }
    else events.push(msg)
  })
  const send = (method, params = {}, sessionId) => new Promise((ok, no) => { id += 1; waiting.set(id, { ok, no }); ws.send(JSON.stringify({ id, method, params, sessionId })) })
  return { send, events }
}

async function main() {
  const page = await mkdtemp(join(tmpdir(), 'thruline-primary-page-'))
  const profile = await mkdtemp(join(tmpdir(), 'thruline-primary-chrome-'))
  let chrome = null
  const results = []
  try {
    const out = await build({
      bundle: true, write: false, format: 'esm', platform: 'browser', outdir: page, entryNames: 'app',
      loader: { '.woff2': 'empty', '.woff': 'empty' }, define: { 'import.meta.env': '{}', 'process.env.NODE_ENV': '"production"' },
      logLevel: 'silent', stdin: { contents: ENTRY, resolveDir: join(UI, 'src/workspace'), loader: 'jsx' },
    })
    for (const f of out.outputFiles) await writeFile(f.path, f.contents)
    await cp(join(UI, 'public/fonts'), join(page, 'fonts'), { recursive: true })
    await writeFile(join(page, 'index.html'), '<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><link rel="stylesheet" href="fonts/plex.css"><link rel="stylesheet" href="app.css"></head><body><div id="root"></div><script type="module" src="app.js"></script></body></html>')

    chrome = await spawnChromeWithDevTools({ userDataDir: profile, extraArgs: ['--allow-file-access-from-files'] })
    const ws = new WebSocket(chrome.wsEndpoint)
    await new Promise((ok, no) => { ws.addEventListener('open', ok); ws.addEventListener('error', no) })
    const { send } = cdp(ws)
    for (const which of ['failing', 'ready']) {
      for (const [width, height] of VIEWPORTS) {
        const { targetId } = await send('Target.createTarget', { url: 'about:blank' })
        const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true })
        await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false }, sessionId)
        await send('Page.enable', {}, sessionId)
        await send('Page.navigate', { url: pathToFileURL(join(page, 'index.html')).href + '?case=' + which }, sessionId)
        let read = null
        for (let i = 0; i < 100; i += 1) {
          await new Promise((r) => setTimeout(r, 50))
          const r = await send('Runtime.evaluate', { expression: `(() => {
            if (!window.__painted) return null
            const b = document.querySelector('#ws-change-decision .ws-change-primary')
            if (!b) return { primary: null }
            const r = b.getBoundingClientRect()
            return { primary: b.textContent.trim(), disabled: b.disabled, top: Math.round(r.top), bottom: Math.round(r.bottom),
              innerHeight: innerHeight, innerWidth: innerWidth, scrollY: scrollY,
              cards: document.querySelectorAll('.ws-change-cards > *').length,
              leading: !!document.querySelector('.ws-change-inspector .ws-cov-line-state') }
          })()`, returnByValue: true }, sessionId)
          if (r.result && r.result.value) { read = r.result.value; break }
        }
        if (process.env.SHOT_DIR && read) {
          const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId)
          await writeFile(join(process.env.SHOT_DIR, `primary-${which}-${width}x${height}.png`), Buffer.from(shot.data, 'base64'))
        }
        await send('Target.closeTarget', { targetId })
        const visible = !!(read && read.primary && read.top >= 0 && read.bottom <= read.innerHeight && read.scrollY === 0)
        results.push({ case: which, viewport: `${width}x${height}`, visible, ...read })
      }
    }
    ws.close()
  } catch (e) {
    console.error('could not run:', e.message)
    return 2
  } finally {
    if (chrome && chrome.proc) { try { chrome.proc.kill('SIGKILL') } catch { /* gone */ } }
    await new Promise((r) => setTimeout(r, 300))
    await rm(page, { recursive: true, force: true })
    await rm(profile, { recursive: true, force: true })
  }
  for (const r of results) console.log(JSON.stringify(r))
  const failed = results.filter((r) => !r.visible || r.cards !== 8 || !r.leading)
  console.log(failed.length ? `NOT VISIBLE: ${failed.length} of ${results.length}` : `visible: ${results.length} of ${results.length}`)
  return failed.length ? 1 : 0
}

process.exitCode = await main()
