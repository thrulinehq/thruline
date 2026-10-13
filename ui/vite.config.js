import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { cpSync, existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { AI_REVIEW_PACKS } from './src/workspace/ai-review-ui.js'

const HERE = dirname(fileURLToPath(import.meta.url))

// `ui/public` holds two things that must not be treated alike:
//
//   fonts/     7 tracked files, part of the app, belongs in any build.
//   fixtures/  REAL TRANSCRIPT CONTENT from the developer's own corpus —
//              185 MB, entirely gitignored, produced by `thruline export`.
//
// Vite's publicDir copy is all-or-nothing, so `vite build` copied the corpus
// into `dist` verbatim. Nothing has leaked — `dist` is gitignored and untracked
// — but it made any tarball built from it a private-data spill by construction
// (docs/LAUNCH-CHECKLIST.md, the sub-item 10 conflict note).
//
// So the build takes an ALLOWLIST, not a denylist: publicDir is switched off for
// `vite build`, and the directories that belong in a distributable are named and
// copied explicitly. A new private directory dropped under `public/` is then
// excluded by default. That asymmetry is the point — a denylist fails silently
// and in the direction of disclosure, and this is the "Nothing leaves the
// machine" invariant, not a packaging preference.
//
// `vite dev` is deliberately untouched: publicDir stays on, so the app reads the
// §9 fixture contract (index.json + projects/<id>.json) exactly as before.
//
// CONSEQUENCE, stated rather than hidden: a production build now carries NO
// project data. The app still fetches the same §9 paths, so a deployment has to
// supply `fixtures/` alongside `dist` rather than find it baked in. No such
// deployment exists today — the tarball path is blocked on other grounds — so
// this removes a hazard without breaking a live path.
const SHIPPABLE_PUBLIC_DIRS = ['fonts']

function allowlistedPublicAssets() {
  return {
    name: 'thruline-allowlist-public',
    apply: 'build',
    // After the bundle is written, so it cannot be cleared by emptyOutDir.
    closeBundle() {
      for (const dir of SHIPPABLE_PUBLIC_DIRS) {
        const from = resolve(HERE, 'public', dir)
        if (!existsSync(from)) {
          throw new Error(
            `vite.config.js: public/${dir} is on the ship allowlist but does not exist. ` +
              `Fix the allowlist rather than shipping a build missing its assets.`,
          )
        }
        cpSync(from, resolve(HERE, 'dist', dir), { recursive: true })
      }
    },
  }
}

// AI-REVIEW-OUT-OF-LAUNCH-R2 (founder rulings R-2026-09-28-20 and -37): the
// launch build must not claim AI review, and `ui/dist` IS the launch build's
// UI — crates/cli/build.rs embeds every file of it and `serve_asset` serves
// them with no feature gate. Round 1 hid PR-Agent at render time; the bundler
// still inlined every word of it (the whole catalog JSON, every label key).
//
// So `vite build` — and only `vite build`; tests and `vite dev` load the full
// modules — does three things:
//   1. resolves ui/src/workspace/ai-review-ui.js and ai-review-facts.jsx, the
//      only two modules that know an AI-review word, to their `.launch` twins;
//   2. drops the AI_REVIEW_PACKS rows from spec/integrations/catalog.json as
//      the bundle imports it (the file itself is untouched; the launch server
//      serves the same catalog minus the same row, `served_catalog()`);
//   3. fails the build if any output chunk still carries an AI-review needle,
//      so a new import cannot put one back silently.
// There is no switch. A UI build that carries AI review is a deliberate edit
// of this file, recorded as such, not an environment variable.
export const LAUNCH_TWINS = {
  [resolve(HERE, 'src/workspace/ai-review-ui.js')]: resolve(HERE, 'src/workspace/ai-review-ui.launch.js'),
  [resolve(HERE, 'src/workspace/ai-review-facts.jsx')]: resolve(HERE, 'src/workspace/ai-review-facts.launch.jsx'),
}
const INTEGRATION_CATALOG = resolve(HERE, '../spec/integrations/catalog.json')
export function launchCatalog() {
  const catalog = JSON.parse(readFileSync(INTEGRATION_CATALOG, 'utf8'))
  const before = catalog.projects.length
  catalog.projects = catalog.projects.filter((p) => !AI_REVIEW_PACKS.includes(p.id))
  if (catalog.projects.length !== before - AI_REVIEW_PACKS.length) {
    throw new Error(`vite.config.js: expected to drop ${AI_REVIEW_PACKS.length} AI-review catalog row(s), dropped ${before - catalog.projects.length}.`)
  }
  return catalog
}
const AI_REVIEW_NEEDLES = ['pr-agent', 'pr_agent', 'pr agent', 'review a change with']

function launchBuildWithoutAiReview() {
  return {
    name: 'thruline-launch-without-ai-review',
    apply: 'build',
    enforce: 'pre',
    config() {
      return { define: { __THRULINE_LAUNCH_SOURCE_IDS__: JSON.stringify(launchCatalog().projects.map((p) => p.id)) } }
    },
    async resolveId(source, importer, options) {
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true })
      return resolved && LAUNCH_TWINS[resolved.id] ? LAUNCH_TWINS[resolved.id] : null
    },
    load(id) {
      return id === INTEGRATION_CATALOG ? JSON.stringify(launchCatalog()) : null
    },
    generateBundle(_options, bundle) {
      for (const [name, output] of Object.entries(bundle)) {
        const text = (output.type === 'chunk' ? output.code : String(output.source)).toLowerCase()
        const found = AI_REVIEW_NEEDLES.filter((needle) => text.includes(needle))
        if (found.length) this.error(`${name} carries AI-review text (${found.join(', ')}); the launch UI must not.`)
      }
    },
  }
}

// Hot reload via @vitejs/plugin-react (Fast Refresh). In dev the fixture under
// public/fixtures is served statically and re-fetched on reload.
export default defineConfig(({ command }) => ({
  plugins: [launchBuildWithoutAiReview(), react(), allowlistedPublicAssets()],
  publicDir: command === 'build' ? false : 'public',
  // 5173 stays the default; PORT lets a second concurrent session (or CI) take
  // its own port instead of colliding with an already-running dev server.
  //
  // The host is pinned to the IPv4 loopback rather than left to Vite's default,
  // which listens on `[::1]` only: a preview that answers on `localhost` but not
  // on `127.0.0.1` is a preview whose address you cannot state exactly, and
  // "bound to 127.0.0.1" is a claim this project has to be able to make.
  //
  // THRULINE_API (dev only) forwards `/api` to a running `thruline ui`, so the
  // dev build draws the real store. It is a loopback address by construction:
  // anything else is refused rather than proxied.
  server: {
    host: '127.0.0.1',
    port: Number(process.env.PORT) || 5173,
    proxy: devApiProxy(process.env.THRULINE_API),
  },
}))

function devApiProxy(target) {
  if (!target) return undefined
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(target)) {
    throw new Error(`vite.config.js: THRULINE_API must be http://127.0.0.1:<port>, got ${target}`)
  }
  // The server admits a request only when Host is its own address and Origin is
  // empty or its own (uiserve.rs, the Host/Origin check), so both are rewritten
  // to the target's.
  return {
    '/api': {
      target,
      changeOrigin: true,
      configure: (proxy) => {
        proxy.on('proxyReq', (req) => {
          if (req.getHeader('origin')) req.setHeader('origin', target)
        })
      },
    },
  }
}
