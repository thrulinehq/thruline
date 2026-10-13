/* The shared rendered-accessibility harness.
 *
 * WHAT IT DOES. Builds the page the key-drive aid builds
 * (`scripts/testing/a11y-drive.mjs`: the REAL app entry `ui/src/main.jsx`,
 * bundled with esbuild, with a `fetch` in front that answers only from
 * in-repo synthetic data — the bundled demo record for the workspace and
 * `ui/src/workspace/a11y-fixture.js` for the Change screen) into a throwaway
 * directory, serves that directory from Node on 127.0.0.1 with a port the
 * kernel picks, and opens headless Chrome through `spawnChromeWithDevTools`
 * (`--remote-debugging-port=0`, a fresh mktemp `--user-data-dir`,
 * `--headless=new`). No product data, no ledger, no network beyond the
 * page's own origin.
 *
 * RESULT BUCKETS. Every probe built on this reports passed · failed · could
 * not run. `COULD_NOT_RUN` (exit 2) is what a missing Chrome, a page that
 * never mounted or a build that failed returns — it is never folded into a
 * pass.
 */
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'
import { mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { extname, join, normalize, resolve, sep } from 'node:path'
import { CHROME_EXECUTABLE, spawnChromeWithDevTools } from './chrome-launch.mjs'

export const PASSED = 0
export const FAILED = 1
export const COULD_NOT_RUN = 2

export const REPO = resolve(import.meta.dirname, '../..')
export const UI = join(REPO, 'ui')

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** A failure that means the probe could not run, not that the product failed. */
export class CouldNotRun extends Error {}

/** Build the key-drive aid's page into a fresh directory. */
export async function buildPage() {
  const out = await mkdtemp(join(tmpdir(), 'thruline-a11y-page-'))
  const code = await new Promise((ok) => {
    const p = spawn(process.execPath, [join(REPO, 'scripts/testing/a11y-drive.mjs'), out], { cwd: UI, stdio: ['ignore', 'ignore', 'inherit'] })
    p.on('exit', ok)
    p.on('error', () => ok(-1))
  })
  if (code !== 0) throw new CouldNotRun(`the page build exited ${code}`)
  /* The aid writes its own page shell; the document element is taken from
   * the product's ui/index.html, so what a probe measures about <html> (its
   * language, above all) is the product's, not the aid's. */
  const product = (await readFile(join(UI, 'index.html'), 'utf8')).match(/<html\b[^>]*>/i)
  const page = join(out, 'app.html')
  if (product) await writeFile(page, (await readFile(page, 'utf8')).replace(/<html\b[^>]*>/i, product[0]))
  return out
}

const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.json': 'application/json', '.svg': 'image/svg+xml' }

/** Serve a directory on 127.0.0.1 at a port the kernel picks. */
export function serve(dir, extra = {}) {
  const root = resolve(dir)
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname)
    if (Object.prototype.hasOwnProperty.call(extra, path)) {
      res.writeHead(200, { 'content-type': TYPES[extname(path)] || 'application/octet-stream' })
      res.end(extra[path])
      return
    }
    const file = normalize(join(root, path === '/' ? 'app.html' : path))
    if (!file.startsWith(root + sep)) { res.writeHead(403); res.end(); return }
    try {
      if (!(await stat(file)).isFile()) throw new Error('not a file')
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' })
      res.end(await readFile(file))
    } catch {
      res.writeHead(404); res.end()
    }
  })
  return new Promise((ok, no) => {
    server.once('error', no)
    server.listen(0, '127.0.0.1', () => ok({ port: server.address().port, close: () => new Promise((r) => server.close(() => r())) }))
  })
}

async function waitFor(fn, what, ms = 15000) {
  const end = Date.now() + ms
  let last = null
  while (Date.now() < end) {
    try { const v = await fn(); if (v) return v } catch (e) { last = e }
    await sleep(150)
  }
  throw new CouldNotRun(`timed out waiting for ${what}${last ? ': ' + last.message : ''}`)
}

/**
 * Open headless Chrome on an ephemeral DevTools port with a throwaway profile.
 * @returns {Promise<{send: Function, evaluate: Function, goto: Function, press: Function, close: Function, port: number}>}
 */
export async function openChrome({ width = 1512, height = 920 } = {}) {
  try { await stat(CHROME_EXECUTABLE) } catch { throw new CouldNotRun(`no Chrome at ${CHROME_EXECUTABLE}`) }
  const profile = await mkdtemp(join(tmpdir(), 'thruline-a11y-chrome-'))
  let chrome
  try {
    chrome = await spawnChromeWithDevTools({ userDataDir: profile, extraArgs: ['--disable-extensions', `--window-size=${width},${height}`] })
  } catch (e) {
    await rm(profile, { recursive: true, force: true }).catch(() => {})
    throw new CouldNotRun(e.message)
  }
  const target = await waitFor(async () => (await (await fetch(`http://127.0.0.1:${chrome.port}/json/list`)).json()).find((t) => t.type === 'page'), 'a Chrome page')
  const ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new CouldNotRun('the DevTools socket did not open')) })
  let nextId = 0
  const pending = new Map()
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data)
    if (msg.id && pending.has(msg.id)) { const p = pending.get(msg.id); pending.delete(msg.id); msg.error ? p.no(new Error(msg.error.message)) : p.ok(msg.result) }
  }
  const send = (method, params) => new Promise((ok, no) => {
    const id = ++nextId
    const timer = setTimeout(() => { pending.delete(id); no(new Error(`${method} got no answer in 20 s`)) }, 20000)
    pending.set(id, { ok: (v) => { clearTimeout(timer); ok(v) }, no: (e) => { clearTimeout(timer); no(e) } })
    ws.send(JSON.stringify({ id, method, params: params || {} }))
  })
  const evaluate = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception ? r.exceptionDetails.exception.description : r.exceptionDetails.text)
    return r.result.value
  }
  await send('Page.enable')
  await send('Runtime.enable')
  await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  /** Load a page and wait until the app has drawn something. */
  const goto = async (url, ready = '.ws-root, .ws-shell') => {
    await send('Page.navigate', { url: 'about:blank' })
    await sleep(100)
    await send('Page.navigate', { url })
    await waitFor(() => evaluate(`!!document.querySelector(${JSON.stringify(ready)})`), `${url} to draw ${ready}`)
    await sleep(600)
  }
  const KEYS = {
    Tab: { key: 'Tab', code: 'Tab', vk: 9 },
    'Shift+Tab': { key: 'Tab', code: 'Tab', vk: 9, modifiers: 8 },
    Escape: { key: 'Escape', code: 'Escape', vk: 27 },
    Enter: { key: 'Enter', code: 'Enter', vk: 13, text: '\r' },
  }
  const press = async (name, pause = 60) => {
    const k = KEYS[name]
    const base = { key: k.key, code: k.code, windowsVirtualKeyCode: k.vk, nativeVirtualKeyCode: k.vk, modifiers: k.modifiers || 0 }
    await send('Input.dispatchKeyEvent', { type: k.text ? 'keyDown' : 'rawKeyDown', ...base, ...(k.text ? { text: k.text } : {}) })
    await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base })
    await sleep(pause)
  }
  const close = async () => {
    try { ws.close() } catch { /* gone */ }
    try { chrome.proc.kill('SIGTERM') } catch { /* gone */ }
    await sleep(300)
    await rm(profile, { recursive: true, force: true }).catch(() => {})
  }
  return { send, evaluate, goto, press, close, port: chrome.port }
}

/**
 * Build, serve and open once; hand the session to `run`; tear everything
 * down. Exit code: what `run` returns, or COULD_NOT_RUN if the harness
 * itself could not come up.
 */
export async function withPage(name, run, opts = {}) {
  let page = null
  let server = null
  let browser = null
  try {
    page = await buildPage()
    server = await serve(page, opts.extra || {})
    browser = await openChrome(opts)
    const base = `http://127.0.0.1:${server.port}/app.html`
    return await run({ ...browser, base })
  } catch (e) {
    console.log(`${name}: could not run — ${e.message}`)
    return COULD_NOT_RUN
  } finally {
    if (browser) await browser.close()
    if (server) await server.close()
    if (page) await rm(page, { recursive: true, force: true }).catch(() => {})
  }
}

/** The pages every probe visits: the workspace and the Change screen. */
export const PAGES = Object.freeze([
  Object.freeze({ name: 'Trace · Brief', path: '?altitude=brief#/a11y' }),
  Object.freeze({ name: 'Trace · Explore', path: '?altitude=explore#/a11y' }),
  Object.freeze({ name: 'Trace · Workbench', path: '?altitude=workbench#/a11y' }),
  Object.freeze({ name: 'Change · Brief', path: '?altitude=brief#/changes/ch1' }),
  Object.freeze({ name: 'Change · Explore', path: '?altitude=explore#/changes/ch1' }),
])
