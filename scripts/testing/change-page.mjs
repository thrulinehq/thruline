/* The Change screen in a real Chrome, over synthetic documents — the shared
 * harness for the layout guards A3-F12 (`inspector-height.test.mjs`) and
 * A3-F15 (`brief-390.test.mjs`).
 *
 * It bundles the REAL app entry (`ui/src/main.jsx`, unchanged) with esbuild,
 * the way `docs/loop/p6b/a11y-drive.mjs` does, behind one `fetch` that answers
 * from the routes a test hands it and says 404 to every other `/api/*` read.
 * The page is served from a Node server on 127.0.0.1 at a port the kernel
 * picks, and Chrome is launched through `spawnChromeWithDevTools` —
 * `--headless=new`, `--remote-debugging-port=0`, a throwaway `--user-data-dir`
 * — and driven over its own DevTools socket. No product data, no ledger, no
 * network beyond the page's own origin. Everything it writes is removed.
 */
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { extname, join, resolve } from 'node:path'
import { access } from 'node:fs/promises'
import { CHROME_EXECUTABLE, spawnChromeWithDevTools } from './chrome-launch.mjs'

const UI = resolve(import.meta.dirname, '../../ui')
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** Is there a Chrome to drive? A guard that cannot run says so; it never passes. */
export async function chromeAvailable() {
  try { await access(CHROME_EXECUTABLE); return true } catch { return false }
}

/** Write the app, answering every read from `routes`, into a fresh directory. */
async function buildPage(routes) {
  const { build } = createRequire(join(UI, 'package.json'))('esbuild')
  const out = await mkdtemp(join(tmpdir(), 'thruline-change-page-'))
  const SHIM = `
    const ROUTES = ${JSON.stringify(routes)};
    const INDEX = { contract_version: 1, kind: 'index', generated_at: '2026-09-23T00:00:00Z', totals: {}, projects: [] };
    const json = (doc, status) => new Response(JSON.stringify(doc), { status: status || 200, headers: { 'content-type': 'application/json' } });
    const real = window.fetch.bind(window);
    window.fetch = async (input, init) => {
      const url = new URL(typeof input === 'string' ? input : input.url, location.href);
      if (url.origin === location.origin) {
        if (url.pathname === '/fixtures/index.json') return json(INDEX);
        const key = url.pathname + url.search;
        if (Object.prototype.hasOwnProperty.call(ROUTES, key)) return json(ROUTES[key]);
        if (Object.prototype.hasOwnProperty.call(ROUTES, url.pathname)) return json(ROUTES[url.pathname]);
        if (url.pathname.startsWith('/api/')) return json({ error: 'not served by this guard' }, 404);
      }
      return real(input, init);
    };
  `
  const result = await build({
    entryPoints: { app: 'guard-entry' },
    bundle: true, format: 'esm', platform: 'browser', outdir: out, write: true, jsx: 'automatic',
    loader: { '.js': 'jsx', '.woff2': 'file' }, define: { 'import.meta.env': '{}' }, logLevel: 'error',
    plugins: [{
      name: 'guard-entry',
      setup(b) {
        b.onResolve({ filter: /^guard-(entry|shim)$/ }, (a) => ({ path: a.path, namespace: 'guard' }))
        b.onLoad({ filter: /^guard-shim$/, namespace: 'guard' }, () => ({ contents: SHIM, loader: 'js', resolveDir: UI }))
        b.onLoad({ filter: /^guard-entry$/, namespace: 'guard' }, () => ({ contents: `import 'guard-shim'; import ${JSON.stringify(join(UI, 'src/main.jsx'))};`, loader: 'js', resolveDir: UI }))
      },
    }],
  })
  if (result.errors.length) throw new Error('the page did not build: ' + result.errors.map((e) => e.text).join('; '))
  await cp(join(UI, 'public/fonts'), join(out, 'fonts'), { recursive: true })
  await writeFile(join(out, 'app.html'), '<!DOCTYPE html><html><head><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><title>ThruLine layout guard (synthetic data)</title><link rel="stylesheet" href="/fonts/plex.css" /><link rel="stylesheet" href="/app.css" /></head><body><div id="root"></div><script type="module" src="/app.js"></script></body></html>')
  return out
}

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2' }

function serve(dir) {
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://127.0.0.1').pathname)
    try {
      const body = await readFile(join(dir, path.replace(/^\/+/, '') || 'app.html'))
      res.writeHead(200, { 'content-type': TYPES[extname(path)] || 'application/octet-stream' })
      res.end(body)
    } catch {
      res.writeHead(404)
      res.end()
    }
  })
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)))
}

/**
 * Open the app over `routes` in headless Chrome and hand `run` a page it can
 * size, point at an address and ask questions of. Everything is torn down.
 *
 * @param {object} routes  address (path, or path + query) → the document it answers
 * @param {(page: {show: Function, evaluate: Function}) => Promise<any>} run
 */
export async function withChangePage(routes, run) {
  const dir = await buildPage(routes)
  const profile = await mkdtemp(join(tmpdir(), 'thruline-change-page-profile-'))
  const server = await serve(dir)
  let chrome = null
  let ws = null
  try {
    chrome = await spawnChromeWithDevTools({ userDataDir: profile, extraArgs: ['--disable-extensions'] })
    let target = null
    for (let i = 0; i < 50 && !target; i += 1) {
      try { target = (await (await fetch(`http://127.0.0.1:${chrome.port}/json/list`)).json()).find((t) => t.type === 'page') } catch { /* not yet */ }
      if (!target) await sleep(100)
    }
    if (!target) throw new Error('Chrome opened no page')
    ws = new WebSocket(target.webSocketDebuggerUrl)
    await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('the DevTools socket did not open')) })
    let next = 0
    const pending = new Map()
    ws.onmessage = (m) => {
      const msg = JSON.parse(m.data)
      if (msg.id && pending.has(msg.id)) { const p = pending.get(msg.id); pending.delete(msg.id); msg.error ? p.no(new Error(msg.error.message)) : p.ok(msg.result) }
    }
    const send = (method, params) => new Promise((ok, no) => {
      const id = ++next
      const timer = setTimeout(() => { pending.delete(id); no(new Error(`${method} got no answer in 10 s`)) }, 10000)
      pending.set(id, { ok: (v) => { clearTimeout(timer); ok(v) }, no: (e) => { clearTimeout(timer); no(e) } })
      ws.send(JSON.stringify({ id, method, params: params || {} }))
    })
    const evaluate = async (expr) => {
      const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception ? r.exceptionDetails.exception.description : r.exceptionDetails.text)
      return r.result.value
    }
    const origin = `http://127.0.0.1:${server.address().port}`
    /** Size the viewport, open `address`, and wait until `ready` (an expression) is true. */
    const show = async (address, width, height, ready) => {
      await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: width < 800 })
      await send('Page.navigate', { url: 'about:blank' })
      await send('Page.navigate', { url: origin + address })
      for (let i = 0; i < 100; i += 1) {
        if (await evaluate(`Boolean(${ready})`).catch(() => false)) {
          await evaluate('document.fonts ? document.fonts.ready.then(() => true) : true')
          await sleep(100)
          return
        }
        await sleep(100)
      }
      throw new Error(`${address} at ${width}×${height} never became ready: ${ready}`)
    }
    return await run({ show, evaluate })
  } finally {
    if (ws) try { ws.close() } catch { /* gone */ }
    if (chrome) try { chrome.proc.kill('SIGTERM') } catch { /* gone */ }
    await new Promise((ok) => server.close(ok))
    await sleep(300)
    await rm(profile, { recursive: true, force: true }).catch(() => {})
    await rm(dir, { recursive: true, force: true }).catch(() => {})
  }
}
