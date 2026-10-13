/* The rendered-accessibility page builder: bundle the app into a page a browser can open.
 *
 *   cd ui && node ../scripts/testing/a11y-drive.mjs <out-dir>
 *
 * NOT a product route and NOT a data path. Static markup has no keyboard, so this bundles the REAL app
 * entry (`ui/src/main.jsx`, unchanged) with esbuild into `<out-dir>/app.html`
 * + `app.js` + `app.css`, which runs in a browser with no server behind it.
 *
 * The only thing added in front of the entry is a `fetch` that answers from
 * IN-REPO SYNTHETIC data and nothing else:
 *   - `/fixtures/index.json` → an empty summary index, so the workspace mounts;
 *   - the Change reads (`/api/changes/ch1…`) → `ui/src/workspace/a11y-fixture.js`;
 *   - every other `/api/*` read → 404, so the workspace opens its bundled demo
 *     record (`ui/src/workspace/demo-data.js`) under its permanent demo banner,
 *     exactly as it does with no server.
 * No product data, no ledger, no network beyond the page's own origin.
 *
 * Pages:
 *   app.html?altitude=explore#/a11y      the workspace at Explore (demo record)
 *   app.html?altitude=brief#/changes/ch1 the Change screen at Brief (synthetic Change)
 * `?altitude=` is a view of one page and is never remembered.
 */
import { cp, mkdir, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { join, resolve } from 'node:path'

const ui = resolve('.')
const { build } = createRequire(join(ui, 'package.json'))('esbuild')
const out = resolve(process.argv[2] || 'a11y-walk')
await mkdir(out, { recursive: true })

const SHIM = `
  import { A11Y_CHANGE_ROUTES } from ${JSON.stringify(join(ui, 'src/workspace/a11y-fixture.js'))};
  const INDEX = { contract_version: 1, kind: 'index', generated_at: '2026-09-23T00:00:00Z', totals: {}, projects: [] };
  const json = (doc, status) => new Response(JSON.stringify(doc), { status: status || 200, headers: { 'content-type': 'application/json' } });
  const real = window.fetch.bind(window);
  window.fetch = async (input, init) => {
    const url = new URL(typeof input === 'string' ? input : input.url, location.href);
    if (url.origin === location.origin) {
      if (url.pathname === '/fixtures/index.json') return json(INDEX);
      if (Object.prototype.hasOwnProperty.call(A11Y_CHANGE_ROUTES, url.pathname)) return json(A11Y_CHANGE_ROUTES[url.pathname]);
      if (url.pathname.startsWith('/api/')) return json({ error: 'not served by the key-drive aid' }, 404);
    }
    return real(input, init);
  };
  window.__thrulineKeyDriveAid = true;
`

const result = await build({
  entryPoints: { app: 'a11y-entry' },
  bundle: true,
  format: 'esm',
  platform: 'browser',
  outdir: out,
  write: true,
  jsx: 'automatic',
  loader: { '.js': 'jsx', '.woff2': 'file' },
  define: { 'import.meta.env': '{}' },
  logLevel: 'warning',
  plugins: [{
    name: 'a11y-entry',
    setup(b) {
      b.onResolve({ filter: /^a11y-(entry|shim)$/ }, (a) => ({ path: a.path, namespace: 'a11y' }))
      b.onLoad({ filter: /^a11y-shim$/, namespace: 'a11y' }, () => ({ contents: SHIM, loader: 'js', resolveDir: ui }))
      /* The shim first, then the real entry, unchanged. */
      b.onLoad({ filter: /^a11y-entry$/, namespace: 'a11y' }, () => ({ contents: `import 'a11y-shim'; import ${JSON.stringify(join(ui, 'src/main.jsx'))};`, loader: 'js', resolveDir: ui }))
    },
  }],
})
if (result.errors.length) { console.error(result.errors); process.exit(1) }

await cp(join(ui, 'public/fonts'), join(out, 'fonts'), { recursive: true })
await writeFile(join(out, 'app.html'), `<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>ThruLine — key-drive aid (synthetic data)</title>
    <link rel="stylesheet" href="/fonts/plex.css" />
    <link rel="stylesheet" href="/app.css" />
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/app.js"></script>
  </body>
</html>
`)
console.log(`wrote ${join(out, 'app.html')}, app.js, app.css, fonts/`)
console.log(`  workspace: /app.html?altitude=explore#/a11y`)
console.log(`  Change:    /app.html?altitude=brief#/changes/ch1`)
