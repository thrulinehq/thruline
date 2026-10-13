/* AI-REVIEW-OUT-OF-LAUNCH-R2 — the launch build's embedded UI assets carry no
 * PR-Agent entry, label or catalog row.
 *
 * Round 1 gated what the screen RENDERS on the server's `ai_review` flag, and
 * `ai-review-out.test.js` checks the rendered DOM. But the bundler inlines
 * whatever a module imports — the whole of spec/integrations/catalog.json,
 * every key of a static label map — so the committed `ui/dist`, which
 * crates/cli/build.rs embeds whole and `serve_asset` serves with no feature
 * gate, still said "Review a change with PR-Agent" and "Implemented;
 * qualification pending." to any client of a default `thruline ui`.
 *
 * This file reads EVERY committed file under ui/dist, byte for byte, and
 * fails on any of the needles below. It does not build anything: it checks
 * what is committed, which is what ships.
 *
 * Needles (case-insensitive):
 *   pr-agent              the capability id, the display name "PR-Agent",
 *                         its report format "pr-agent-json-v1" and its
 *                         upstream repository "The-PR-Agent/pr-agent"
 *   pr_agent              its facts (pr_agent_options, pr_agent_plain_diff)
 *   pr agent              the name spelled with a space
 *   review a change with  the recipe title's verb phrase
 * The PR-Agent row's feature, "local-repair-advisors", is not a needle:
 * Aider, OpenHands and mini-swe-agent share it and stay shipped.
 *
 * Deliberately not a needle: "11436". The bundle carries it only inside
 * T-4-1a's plain sentence ("Review findings aren't available: the review
 * model at 127.0.0.1:11436 isn't answering."), which round 1's item 3 keeps
 * and hq-copy.golden.json pins. It is reported to the orchestrator. */
import assert from 'node:assert/strict'
import test from 'node:test'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const DIST = fileURLToPath(new URL('../../dist/', import.meta.url))
const NEEDLES = ['pr-agent', 'pr_agent', 'pr agent', 'review a change with']

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    return statSync(path).isDirectory() ? files(path) : [path]
  })
}

test('every committed ui/dist file is free of PR-Agent entries, labels, capabilities and catalog rows', () => {
  const all = files(DIST)
  assert.ok(all.some((f) => /assets\/integrations-[^/]+\.js$/.test(f)), 'the tool-evidence chunk is committed')
  assert.ok(all.some((f) => f.endsWith('index.html')), 'the entry page is committed')
  const carried = []
  for (const file of all) {
    const text = readFileSync(file).toString('latin1').toLowerCase()
    for (const needle of NEEDLES) if (text.includes(needle)) carried.push(`${relative(DIST, file)}: ${needle}`)
  }
  assert.deepEqual(carried, [], 'no committed ui/dist file names PR-Agent')
})

test('the committed tool-evidence chunk still carries the deterministic catalog and recipe titles', () => {
  const chunk = files(DIST).find((f) => /assets\/integrations-[^/]+\.js$/.test(f))
  const text = readFileSync(chunk, 'utf8')
  for (const kept of ['ast-grep', 'Aider', 'mini-swe-agent', 'Check Python imports and variables', 'Change review against registered base']) {
    assert.ok(text.includes(kept), `${kept} is still shipped`)
  }
})

/* The launch twins, exercised. Round 1's DOM tests load the full modules (the
 * test bundler does not run the vite plugin). This bundles the tool-evidence
 * screen the way `vite build` does — ai-review-ui.js and ai-review-facts.jsx
 * resolved to their `.launch` twins, the launch source ids defined — and
 * mounts it with a reading that lists a ready PR-Agent recipe and source AND
 * says `ai_review: true`, the hard case: the launch UI shows it nowhere, and
 * still shows the deterministic recipe and source. (What the bytes carry is
 * the committed-dist test's job above: this bundle is not filtered by the
 * vite plugin's catalog step, so it is checked for behaviour only.) */
test('the launch twins: even a reading that says ai_review: true shows no PR-Agent recipe or source', async () => {
  const { build: bundle } = await import('esbuild')
  const { JSDOM } = await import('jsdom')
  const { mkdtemp, rm, writeFile } = await import('node:fs/promises')
  const { tmpdir } = await import('node:os')
  const { pathToFileURL } = await import('node:url')
  const { LAUNCH_TWINS, launchCatalog } = await import('../../vite.config.js')
  const here = fileURLToPath(new URL('.', import.meta.url))
  const temp = await mkdtemp(join(tmpdir(), 'thruline-ai-review-launch-'))
  const out = await bundle({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, logLevel: 'silent',
    define: { 'import.meta.env': '{}', __THRULINE_LAUNCH_SOURCE_IDS__: JSON.stringify(launchCatalog().projects.map((p) => p.id)) },
    plugins: [{ name: 'launch-twins', setup(b) {
      b.onResolve({ filter: /ai-review-(ui|facts)\.jsx?$/ }, (args) => {
        const twin = LAUNCH_TWINS[join(args.resolveDir, args.path)]
        return twin ? { path: twin } : undefined
      })
    } }],
    stdin: {
      contents: `
        import React from 'react';
        import { createRoot } from 'react-dom/client';
        import { flushSync } from 'react-dom';
        import { IntegrationWorkspace } from './integrations.jsx';
        export { React, createRoot, flushSync, IntegrationWorkspace };
      `,
      resolveDir: here,
    },
  })
  const code = out.outputFiles[0].text
  const file = join(temp, 'launch.mjs')
  await writeFile(file, code)
  const L = await import(pathToFileURL(file).href)
  await rm(temp, { recursive: true, force: true })

  const RECIPE = (id) => ({ id, ready: true, accepted: false, requirement: `${id} requirement`, why: 'why', support: null, review_digest: 'd', next: 'next' })
  const PACK = (id) => ({ id, feature: 'structure', feature_name: 'structure', contribution: `${id} contribution`, limits: '', candidate_check_ids: [], repository: 'x/y', commit: 'c', declared_main_license: 'MIT', submodules: 0 })
  const reading = {
    schema: 'thruline.integration-workspace.v1', session: 's', root: '/synthetic/repo',
    workflow: { schema: 'thruline.integration-workflow.v1', ai_review: true, accepted_revision: null, recipes: [RECIPE('ast-grep'), RECIPE('pr-agent')], trust: 'Local acceptance.' },
    project_profile: null,
    catalog: { projects: [PACK('ast-grep'), PACK('pr-agent')] },
    results: { contract: 'thruline.integration-observation.v1', runs: [] },
  }
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/' })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) if (w[n] !== undefined) Object.defineProperty(globalThis, n, { value: n === 'window' ? w : w[n], configurable: true, writable: true })
  const realFetch = globalThis.fetch
  globalThis.fetch = async (url) => {
    const answer = (body) => ({ ok: true, status: 200, json: async () => body })
    if (String(url).startsWith('/api/integrations?')) return answer(reading)
    if (String(url) === '/api/review-projects') return answer({ projects: [] })
    return { ok: false, status: 404, json: async () => ({ error: 'not in this fixture' }) }
  }
  const root = L.createRoot(w.document.getElementById('root'))
  try {
    L.flushSync(() => root.render(L.React.createElement(L.IntegrationWorkspace, { session: { id: 's' }, onCopy() {} })))
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 10))
    const text = w.document.body.textContent
    assert.ok(text.includes('ast-grep'), 'the deterministic recipe and source are shown')
    assert.doesNotMatch(text, /pr-agent|pr_agent|review a change with/i, 'the launch UI names no AI reviewer')
  } finally {
    root.unmount()
    globalThis.fetch = realFetch
    for (const [n, dsc] of held) { if (dsc) Object.defineProperty(globalThis, n, dsc); else delete globalThis[n] }
    w.close()
  }
})
