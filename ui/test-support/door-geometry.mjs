#!/usr/bin/env node
/* PATH-UI-2-R3 — the real-browser half of the door landing guard.
 *
 * `src/workspace/packet-link-door-r3.test.js` pins, in jsdom, what the code
 * ASKS the browser to do (jsdom draws nothing, so it has no scroll). This
 * measures what a browser DOES: headless Chrome, the real app, the real
 * stylesheet, a Change with N accepted lines, one door pressed, and then the
 * geometry of what is on the glass.
 *
 * WHAT A DOOR MUST LEAVE ON THE GLASS, for a line k the person was sent to
 * (the card's Done-when 1):
 *   (a) line k's row on the Verify stage, and the control that links it, are
 *       inside the viewport;
 *   (b) the name box that gates that control is focused AND inside the
 *       viewport;
 *   (c) the landing says which line it is: line k's row is marked current
 *       (`aria-current`), and the name box's accessible description names
 *       line k's text.
 * "The row" is the row that HOLDS the control — the entry in the controls
 * list under the one name box — not the summary entry in the list of accepted
 * lines above the Inspector, which is a separate, far-away element (about
 * three thousand pixels away on a Change with twelve lines).
 *
 * TWO SOURCES OF THE CHANGE.
 *   default            synthetic documents (`door-fixture.mjs`) answered to the
 *                      real app bundle by a shim; nothing is read from a
 *                      machine and no server of ours is started.
 *   --base URL --change ID
 *                      a running `thruline ui` (its own store, its own
 *                      project) — the real binary's real documents.
 *   --shots            also write one PNG of the glass per door under --work.
 *   --ui DIR           bundle the app from another checkout's `ui/` (to measure the
 *                      same doors against the code a fix started from).
 *
 * DOORS EXERCISED (--via):
 *   address     `?door=<k>#/changes/<id>/verify` loaded fresh — exactly what
 *               the whole-story page's "link it" anchor navigates to.
 *   packet      the whole-story page opened, the anchor for line k clicked.
 *   inspector   the Brief altitude's one primary, "Go to <line> to link it",
 *               clicked (it names the first line still needing a link).
 *   broke       a Change whose last line's check FAILED: the Brief altitude's one
 *               primary, "Look at what broke", clicked. A look-at door, not a
 *               link door: what it must leave is the line that broke, on the
 *               glass and focused (`lookVerdict`); it takes no name box.
 *
 * Nothing is deleted: every directory this makes is fresh (D-211), and lives
 * under `--work <dir>` (default: a new directory under the OS temp dir).
 *
 * Exit status: 0 = every door measured passed; 1 = at least one failed;
 * 2 = the check itself could not run (no Chrome, page never became ready).
 * A check that cannot run never passes.
 */
import { mkdtemp, mkdir, writeFile, readFile, access } from 'node:fs/promises'
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { extname, join, resolve } from 'node:path'
import { cp } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'
import { CHROME_EXECUTABLE, spawnChromeWithDevTools } from '../../scripts/testing/chrome-launch.mjs'
import { CHANGE_ID, linesOf, routes } from './door-fixture.mjs'

const UI = resolve(import.meta.dirname, '..')
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/* ── arguments ─────────────────────────────────────────────────────────── */

function args(argv) {
  const a = { lines: [2, 5, 12, 40], viewports: ['1280x800', '390x844'], via: ['address'], doors: 'ends', base: null, change: null, work: null, json: false, shots: false, ui: null }
  for (let i = 0; i < argv.length; i += 1) {
    const k = argv[i]
    const v = () => { i += 1; if (argv[i] === undefined) throw new Error(`${k} needs a value`); return argv[i] }
    if (k === '--lines') a.lines = v().split(',').map(Number)
    else if (k === '--viewports') a.viewports = v().split(',')
    else if (k === '--via') a.via = v().split(',')
    else if (k === '--doors') a.doors = v()
    else if (k === '--base') a.base = v().replace(/\/$/, '')
    else if (k === '--change') a.change = v()
    else if (k === '--work') a.work = v()
    else if (k === '--json') a.json = true
    else if (k === '--shots') a.shots = true
    else if (k === '--ui') a.ui = v()
    else throw new Error(`unknown argument ${k}`)
  }
  for (const n of a.lines) if (!Number.isInteger(n) || n < 1) throw new Error(`--lines wants whole numbers, not ${n}`)
  for (const vp of a.viewports) if (!/^\d+x\d+$/.test(vp)) throw new Error(`--viewports wants WxH, not ${vp}`)
  if (a.base && !a.change) throw new Error('--base needs --change')
  return a
}

/** Which doors of an N-line Change to press: the first, the middle and the last, or all of them. */
function doorsFor(n, which) {
  if (which === 'all') return Array.from({ length: n }, (_, i) => i + 1)
  return [...new Set([1, Math.ceil(n / 2), n])]
}

/* ── the synthetic page: the real app, answering from `window.__ROUTES__` ── */

async function buildPage(dir, ui = UI) {
  const { build } = createRequire(join(UI, 'package.json'))('esbuild')
  const SHIM = `
    const INDEX = { contract_version: 1, kind: 'index', generated_at: '2026-09-23T00:00:00Z', totals: {}, projects: [] };
    const json = (doc, status) => new Response(JSON.stringify(doc), { status: status || 200, headers: { 'content-type': 'application/json' } });
    const real = window.fetch.bind(window);
    window.fetch = async (input, init) => {
      const url = new URL(typeof input === 'string' ? input : input.url, location.href);
      if (url.origin === location.origin) {
        const ROUTES = window.__ROUTES__ || {};
        if (url.pathname === '/fixtures/index.json') return json(INDEX);
        if (Object.prototype.hasOwnProperty.call(ROUTES, url.pathname)) return json(ROUTES[url.pathname]);
        if (url.pathname.startsWith('/api/')) return json({ error: 'not served by this check' }, 404);
      }
      return real(input, init);
    };
  `
  const result = await build({
    entryPoints: { app: 'door-entry' },
    bundle: true, format: 'esm', platform: 'browser', outdir: dir, write: true, jsx: 'automatic',
    loader: { '.js': 'jsx', '.woff2': 'file' }, define: { 'import.meta.env': '{}' }, logLevel: 'error',
    plugins: [{
      name: 'door-entry',
      setup(b) {
        b.onResolve({ filter: /^door-(entry|shim)$/ }, (a) => ({ path: a.path, namespace: 'door' }))
        b.onLoad({ filter: /^door-shim$/, namespace: 'door' }, () => ({ contents: SHIM, loader: 'js', resolveDir: UI }))
        b.onLoad({ filter: /^door-entry$/, namespace: 'door' }, () => ({ contents: `import 'door-shim'; import ${JSON.stringify(join(ui, 'src/main.jsx'))};`, loader: 'js', resolveDir: UI }))
      },
    }],
  })
  if (result.errors.length) throw new Error('the page did not build: ' + result.errors.map((e) => e.text).join('; '))
  await cp(join(ui, 'public/fonts'), join(dir, 'fonts'), { recursive: true })
  await writeFile(join(dir, 'app.html'), '<!DOCTYPE html><html><head><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><title>ThruLine door geometry (synthetic data)</title><link rel="stylesheet" href="/fonts/plex.css" /><link rel="stylesheet" href="/app.css" /></head><body><div id="root"></div><script type="module" src="/app.js"></script></body></html>')
}

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2' }

function serve(dir) {
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://127.0.0.1').pathname)
    const file = path.replace(/^\/+/, '') || 'app.html'
    try {
      const body = await readFile(join(dir, file))
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' })
      res.end(body)
    } catch {
      res.writeHead(404)
      res.end()
    }
  })
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)))
}

/* ── Chrome, driven over its own DevTools socket ───────────────────────── */

async function openChrome(profile) {
  const chrome = await spawnChromeWithDevTools({ userDataDir: profile, extraArgs: ['--disable-extensions'] })
  let target = null
  for (let i = 0; i < 50 && !target; i += 1) {
    try { target = (await (await fetch(`http://127.0.0.1:${chrome.port}/json/list`)).json()).find((t) => t.type === 'page') } catch { /* not yet */ }
    if (!target) await sleep(100)
  }
  if (!target) throw new Error('Chrome opened no page')
  const ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('the DevTools socket did not open')) })
  let next = 0
  const pending = new Map()
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data)
    if (msg.id && pending.has(msg.id)) { const p = pending.get(msg.id); pending.delete(msg.id); msg.error ? p.no(new Error(msg.error.message)) : p.ok(msg.result) }
  }
  const send = (method, params) => new Promise((ok, no) => {
    const id = ++next
    const timer = setTimeout(() => { pending.delete(id); no(new Error(`${method} got no answer in 15 s`)) }, 15000)
    pending.set(id, { ok: (v) => { clearTimeout(timer); ok(v) }, no: (e) => { clearTimeout(timer); no(e) } })
    ws.send(JSON.stringify({ id, method, params: params || {} }))
  })
  const evaluate = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception ? r.exceptionDetails.exception.description : r.exceptionDetails.text)
    return r.result.value
  }
  return { chrome, ws, send, evaluate }
}

/* ── what the glass shows ──────────────────────────────────────────────── */

/**
 * In the page: everything the card's (a), (b), (c) ask, for line `k` whose text
 * is `text`. Written as one expression so it runs in the page, and looks the
 * row up BY ITS TEXT, so it does not depend on any id a fix might add.
 */
export const measureExpr = (k, text) => `(() => {
  const want = ${JSON.stringify(text)}
  const rows = [...document.querySelectorAll('.ws-cov-rows > li')]
  const row = rows.find((r) => { const p = r.querySelector('.ws-cov-criterion'); return p && p.textContent === want }) || null
  const input = document.querySelector('.ws-cov .ws-cov-actor input')
  const control = row ? [...row.querySelectorAll('button')].find((b) => b.textContent.trim() === 'This bears on it') || null : null
  const vh = window.innerHeight
  const box = (e) => { if (!e) return null; const b = e.getBoundingClientRect(); return { top: Math.round(b.top), bottom: Math.round(b.bottom) } }
  const whole = (b) => !!b && b.top >= 0 && b.bottom <= vh
  const r = box(row), c = box(control), i = box(input)
  const described = input && input.getAttribute('aria-describedby')
    ? input.getAttribute('aria-describedby').split(/\\s+/).map((id) => { const e = document.getElementById(id); return e ? e.textContent : '' }).join(' ').trim()
    : ''
  const current = [...document.querySelectorAll('.ws-cov-rows > li[aria-current]')]
  const controls = rows.map((r) => [...r.querySelectorAll('button')].find((b) => b.textContent.trim() === 'This bears on it') || null)
  return JSON.stringify({
    rowCount: rows.length,
    allControlsInView: rows.length > 0 && controls.every((c) => whole(box(c))),
    url: location.pathname + location.search + location.hash,
    vh, scrollY: Math.round(window.scrollY),
    rowFound: !!row, row: r, control: c, input: i,
    rowInView: !!r && r.top >= 0 && r.top < vh && (r.bottom <= vh || whole(c)),
    controlInView: whole(c),
    inputInView: whole(i),
    focused: !!input && document.activeElement === input,
    current: current.length === 1 && current[0] === row,
    currentCount: current.length,
    described,
    describesLine: described.includes(want),
    line: ${k},
  })
})()`

/** The verdict on one measurement. `reasons` is empty when the door did what the card asks. */
export function verdict(m) {
  const reasons = []
  if (!m.rowFound) reasons.push('the line has no row in the controls list')
  if (!m.rowInView) reasons.push(`the line's row is not on the glass (row ${JSON.stringify(m.row)}, viewport ${m.vh})`)
  if (!m.controlInView) reasons.push(`the link control is not on the glass (control ${JSON.stringify(m.control)}, viewport ${m.vh})`)
  if (!m.inputInView) reasons.push(`the name box is not on the glass (input ${JSON.stringify(m.input)}, viewport ${m.vh})`)
  if (!m.focused) reasons.push('the name box is not focused')
  if (!m.current) reasons.push(`the row is not the one marked current (${m.currentCount} rows are)`)
  if (!m.describesLine) reasons.push('the name box does not describe the line')
  /* The walk fixture: with two lines a landing must still leave BOTH controls on the glass (the card's Done-when 3). */
  if (m.rowCount <= 2 && !m.allControlsInView) reasons.push('with two lines both link controls should be on the glass')
  return reasons
}

/** The look-at door's measurement: the SUMMARY entry of line k (`#ws-cov-line-k`, the line that broke) and where the cursor is. */
export const lookExpr = (k) => `(() => {
  const el = document.getElementById('ws-cov-line-${k}')
  const vh = window.innerHeight
  const b = el ? el.getBoundingClientRect() : null
  return JSON.stringify({
    vh, scrollY: Math.round(window.scrollY),
    found: !!el,
    entry: b ? { top: Math.round(b.top), bottom: Math.round(b.bottom) } : null,
    entryInView: !!b && b.top >= 0 && b.top < vh,
    focused: !!el && document.activeElement === el,
    inName: !!document.activeElement && !!document.activeElement.closest && !!document.activeElement.closest('.ws-cov-actor'),
    line: ${k},
  })
})()`

/** What "Look at what broke" must leave: the line that broke on the glass with the cursor on it. It is not a link door, so a name box is not asked of it. */
export function lookVerdict(m) {
  const reasons = []
  if (!m.found) reasons.push('the line that broke has no entry')
  if (!m.entryInView) reasons.push(`the line that broke is not on the glass (entry ${JSON.stringify(m.entry)}, viewport ${m.vh})`)
  if (!m.focused) reasons.push('the cursor is not on the line that broke')
  return reasons
}

/* ── the run ───────────────────────────────────────────────────────────── */

async function main() {
  const a = args(process.argv.slice(2))
  try { await access(CHROME_EXECUTABLE) } catch { console.error(`could not run: no Chrome at ${CHROME_EXECUTABLE}`); process.exit(2) }
  const work = a.work ? resolve(a.work) : await mkdtemp(join(tmpdir(), 'thruline-door-geometry-'))
  await mkdir(work, { recursive: true })
  const profile = await mkdtemp(join(work, 'profile-'))
  let server = null
  let origin = a.base
  let session = null
  const table = []
  try {
    if (!a.base) {
      const dir = await mkdtemp(join(work, 'app-'))
      await buildPage(dir, a.ui ? resolve(a.ui) : UI)
      server = await serve(dir)
      origin = `http://127.0.0.1:${server.address().port}`
    }
    session = await openChrome(profile)
    const { send, evaluate } = session
    await send('Page.enable')
    let scriptId = null
    const show = async (address, width, height, ready, table_) => {
      await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: width < 800 })
      if (scriptId) { await send('Page.removeScriptToEvaluateOnNewDocument', { identifier: scriptId }); scriptId = null }
      /* Every document starts as a person's first visit would: this origin's storage is emptied before the app reads it (the altitude a door chooses is remembered, and would otherwise turn the next door's start into the last one's end). */
      const source = `try { localStorage.clear() } catch (e) {} window.__errors = []; addEventListener('error', (e) => window.__errors.push(String(e.message))); addEventListener('unhandledrejection', (e) => window.__errors.push(String(e.reason)));` + (table_ ? ` window.__ROUTES__ = ${JSON.stringify(table_)};` : '')
      scriptId = (await send('Page.addScriptToEvaluateOnNewDocument', { source })).identifier
      await send('Page.navigate', { url: 'about:blank' })
      await sleep(300)
      const nav = await send('Page.navigate', { url: origin + address })
      if (nav && nav.errorText) throw new Error(`Chrome could not open ${origin + address}: ${nav.errorText}`)
      for (let i = 0; i < 150; i += 1) {
        if (await evaluate(`Boolean(${ready})`).catch(() => false)) {
          await evaluate('document.fonts ? document.fonts.ready.then(() => true) : true')
          await sleep(150)
          return
        }
        await sleep(100)
      }
      const seen = await evaluate('(document.body ? document.body.innerText.slice(0, 300) : "(no body)") + " | errors: " + JSON.stringify(window.__errors || []) + " | html: " + document.documentElement.outerHTML.slice(0, 400) + " | resources: " + JSON.stringify(performance.getEntriesByType("resource").map((r) => r.name + " " + r.responseStatus))').catch(() => '(page not readable)')
      throw new Error(`${address} at ${width}x${height} never became ready: ${ready}; the page says: ${JSON.stringify(seen)}`)
    }
    const settle = () => sleep(1200)

    const sources = a.base ? [{ n: null }] : a.lines.map((n) => ({ n }))
    for (const src of sources) {
      let id = a.change
      let lines
      let table_ = null
      if (a.base) {
        const doc = await (await fetch(`${a.base}/api/changes/${encodeURIComponent(a.change)}`)).json()
        lines = (doc.change || doc).accepted_scope.criteria.map((c) => (typeof c === 'string' ? c : c.text))
      } else {
        id = CHANGE_ID
        lines = linesOf(src.n)
        table_ = routes(lines, id)
      }
      const n = lines.length
      const brokenTable = a.base ? null : routes(lines, id, { broke: n })
      const verify = (k) => `/?door=${k}#/changes/${id}/verify`
      for (const vp of a.viewports) {
        const [w, h] = vp.split('x').map(Number)
        for (const via of a.via) {
          const ks = via === 'inspector' || via === 'broke' ? [null] : doorsFor(n, a.doors)
          for (const k of ks) {
            let line = k
            if (via === 'address') {
              await show(verify(k), w, h, `document.querySelector('.ws-cov .ws-cov-actor input')`, table_)
              await settle()
            } else if (via === 'packet') {
              await show(`/#/changes/${id}/packet`, w, h, `document.querySelector('a.ws-packet-link-door')`, table_)
              const text = lines[k - 1]
              const clicked = await evaluate(`(() => { const a = [...document.querySelectorAll('a.ws-packet-link-door')].find((x) => x.textContent.includes(${JSON.stringify(text)})); if (!a) return false; a.click(); return true })()`)
              if (!clicked) throw new Error(`the whole-story page has no door for line ${k}`)
              await sleep(800)
              await evaluate(`new Promise((ok) => { const t = Date.now(); const f = () => document.querySelector('.ws-cov .ws-cov-actor input') || Date.now() - t > 20000 ? ok(true) : setTimeout(f, 100); f() })`)
              await settle()
            } else if (via === 'inspector') {
              await show(`/?altitude=brief#/changes/${id}`, w, h, `document.querySelector('.ws-what-broke')`, table_)
              const door = await evaluate(`(() => { const b = document.querySelector('.ws-what-broke'); if (!b) return null; const d = b.getAttribute('data-door'); b.click(); return d })()`)
              const m = /^ws-cov-line-(\d+)$/.exec(door || '')
              if (!m) throw new Error(`the Inspector's door names no line: ${door}`)
              line = Number(m[1])
              await sleep(800)
              await evaluate(`new Promise((ok) => { const t = Date.now(); const f = () => document.querySelector('.ws-cov .ws-cov-actor input') || Date.now() - t > 20000 ? ok(true) : setTimeout(f, 100); f() })`)
              await settle()
            } else if (via === 'broke') {
              await show(`/?altitude=brief#/changes/${id}`, w, h, `document.querySelector('.ws-what-broke')`, brokenTable)
              const door = await evaluate(`(() => { const b = document.querySelector('.ws-what-broke'); if (!b) return null; const d = b.getAttribute('data-door'); b.click(); return d })()`)
              const m = /^ws-cov-line-(\d+)$/.exec(door || '')
              if (!m) throw new Error(`"Look at what broke" names no line: ${door}`)
              line = Number(m[1])
              await sleep(800)
              await settle()
              const look = JSON.parse(await evaluate(lookExpr(line)))
              if (a.shots) {
                const png = await send('Page.captureScreenshot', { format: 'png' })
                await writeFile(join(work, `shot-${n}-${vp}-${via}-${line}.png`), Buffer.from(png.data, 'base64'))
              }
              const reasons = lookVerdict(look)
              table.push({ lines: n, viewport: vp, via, door: line, ok: reasons.length === 0, reasons, look })
              continue
            } else {
              throw new Error(`unknown door route ${via}`)
            }
            const m = JSON.parse(await evaluate(measureExpr(line, lines[line - 1])))
            if (a.shots) {
              const png = await send('Page.captureScreenshot', { format: 'png' })
              await writeFile(join(work, `shot-${n}-${vp}-${via}-${line}.png`), Buffer.from(png.data, 'base64'))
            }
            const reasons = verdict(m)
            table.push({ lines: n, viewport: vp, via, door: line, ok: reasons.length === 0, reasons, m })
          }
        }
      }
    }
  } catch (e) {
    console.error('could not run: ' + (e && e.message ? e.message : e))
    process.exitCode = 2
    return
  } finally {
    if (session) {
      try { session.ws.close() } catch { /* gone */ }
      const gone = new Promise((ok) => session.chrome.proc.once('exit', ok))
      try { session.chrome.proc.kill('SIGTERM') } catch { /* gone */ }
      await Promise.race([gone, sleep(5000)])
    }
    if (server) await new Promise((ok) => server.close(ok))
  }
  if (a.json) console.log(JSON.stringify(table, null, 1))
  for (const t of table) {
    const m = t.m
    if (t.look) {
      console.log(`${t.ok ? 'PASS' : 'FAIL'}  lines=${String(t.lines).padStart(2)}  ${t.viewport.padEnd(8)}  via=${t.via.padEnd(9)}  door=${String(t.door).padStart(2)}  scrollY=${String(t.look.scrollY).padStart(5)}  entry=${t.look.entry ? t.look.entry.top + '..' + t.look.entry.bottom : 'none'}  focused=${t.look.focused}  cursorInNameBox=${t.look.inName}${t.ok ? '' : '  <- ' + t.reasons.join('; ')}`)
      continue
    }
    console.log(`${t.ok ? 'PASS' : 'FAIL'}  lines=${String(t.lines).padStart(2)}  ${t.viewport.padEnd(8)}  via=${t.via.padEnd(9)}  door=${String(t.door).padStart(2)}  scrollY=${String(m.scrollY).padStart(5)}  row=${m.row ? m.row.top + '..' + m.row.bottom : 'none'}  control=${m.control ? m.control.top + '..' + m.control.bottom : 'none'}  input=${m.input ? m.input.top + '..' + m.input.bottom : 'none'}  focused=${m.focused}  current=${m.current}${t.ok ? '' : '  <- ' + t.reasons.join('; ')}`)
  }
  const failed = table.filter((t) => !t.ok).length
  console.log(`\n${table.length} doors measured, ${table.length - failed} passed, ${failed} failed`)
  process.exitCode = failed ? 1 : 0
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await main()
