#!/usr/bin/env node
/* shoot.mjs — screenshot pages in one headless Chrome, for the HQ Next
 * side-by-side: a design artboard and the product screen that must match it.
 *
 *   node scripts/hq/shoot.mjs <out.png> <url> [--w 1512] [--h 920] [--full]
 *        [--wait <css selector>] [--settle <ms>]
 *   node scripts/hq/shoot.mjs --batch <jobs.json>
 *        jobs.json: [{"out": "...png", "url": "...", "w": 1512, "h": 920,
 *                     "full": false, "wait": "#root > *", "settle": 800}]
 *
 * Chrome is launched through scripts/testing/chrome-launch.mjs (fresh profile,
 * kernel-chosen DevTools port) and driven over the DevTools socket, so a page
 * with endless CSS animations still returns as soon as it has drawn — the
 * one-shot `--screenshot` flag waits for virtual time to run out instead.
 * Animations are paused before the capture so two shots of one page agree.
 */
import { mkdtemp, rm, writeFile, readFile, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { spawnChromeWithDevTools } from '../testing/chrome-launch.mjs'

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

function parse(argv) {
  if (argv[0] === '--batch') return { batch: argv[1] }
  const job = { out: argv[0], url: argv[1], w: 1512, h: 920, full: false, wait: null, settle: 800 }
  for (let i = 2; i < argv.length; i += 1) {
    const a = argv[i]
    if (a === '--w') job.w = Number(argv[++i])
    else if (a === '--h') job.h = Number(argv[++i])
    else if (a === '--full') job.full = true
    else if (a === '--wait') job.wait = argv[++i]
    else if (a === '--settle') job.settle = Number(argv[++i])
    else if (a === '--wait-text') job.waitText = argv[++i]
    else if (a === '--then-eval') job.thenEval = argv[++i]
    else if (a === '--then-wait-text') job.thenWaitText = argv[++i]
    else throw new Error(`unknown argument ${a}`)
  }
  if (!job.out || !job.url) throw new Error('usage: shoot.mjs <out.png> <url> [--w] [--h] [--full] [--wait sel] [--settle ms]')
  return { jobs: [job] }
}

async function open() {
  const profile = await mkdtemp(join(tmpdir(), 'thruline-shoot-'))
  const chrome = await spawnChromeWithDevTools({ userDataDir: profile, extraArgs: ['--disable-extensions', '--allow-file-access-from-files', '--hide-scrollbars'] })
  let target = null
  for (let i = 0; i < 100 && !target; i += 1) {
    try {
      target = (await (await fetch(`http://127.0.0.1:${chrome.port}/json/list`)).json()).find((t) => t.type === 'page')
    } catch { /* not yet */ }
    if (!target) await sleep(100)
  }
  if (!target) throw new Error('no Chrome page appeared')
  const ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('DevTools socket did not open')) })
  let id = 0
  const pending = new Map()
  const errors = []
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data)
    if (msg.id && pending.has(msg.id)) {
      const p = pending.get(msg.id)
      pending.delete(msg.id)
      msg.error ? p.no(new Error(msg.error.message)) : p.ok(msg.result)
    } else if (msg.method === 'Runtime.exceptionThrown') {
      errors.push(msg.params.exceptionDetails?.exception?.description || msg.params.exceptionDetails?.text)
    } else if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
      errors.push(msg.params.args.map((a) => a.value ?? a.description).join(' '))
    }
  }
  const send = (method, params = {}) => new Promise((ok, no) => {
    const n = ++id
    const t = setTimeout(() => { pending.delete(n); no(new Error(`${method}: no answer in 30 s`)) }, 30000)
    pending.set(n, { ok: (v) => { clearTimeout(t); ok(v) }, no: (e) => { clearTimeout(t); no(e) } })
    ws.send(JSON.stringify({ id: n, method, params }))
  })
  const evaluate = async (expression) => {
    const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text)
    return r.result.value
  }
  await send('Page.enable')
  await send('Runtime.enable')
  const close = async () => {
    try { ws.close() } catch { /* gone */ }
    try { chrome.proc.kill('SIGKILL') } catch { /* gone */ }
    await sleep(200)
    await rm(profile, { recursive: true, force: true }).catch(() => {})
  }
  return { send, evaluate, errors, close }
}

async function shoot(c, job) {
  const w = job.w || 1512
  const h = job.h || 920
  c.errors.length = 0
  await c.send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: job.scale || 1, mobile: false })
  await c.send('Page.navigate', { url: 'about:blank' })
  await sleep(50)
  await c.send('Page.navigate', { url: job.url })
  const wait = job.wait || 'body > *'
  let drawn = false
  for (let i = 0; i < 200 && !drawn; i += 1) {
    try { drawn = await c.evaluate(`!!document.querySelector(${JSON.stringify(wait)}) && document.readyState === 'complete'`) } catch { /* navigating */ }
    if (!drawn) await sleep(100)
  }
  if (!drawn) throw new Error(`${job.url}: never drew ${wait}`)
  /* A live screen draws its loading state first; --wait-text waits (up to
     90 s) until the page says something only the loaded screen says. */
  if (job.waitText) {
    let said = false
    for (let i = 0; i < 900 && !said; i += 1) {
      try { said = await c.evaluate(`document.body.innerText.includes(${JSON.stringify(job.waitText)})`) } catch { /* navigating */ }
      if (!said) await sleep(100)
    }
    if (!said) throw new Error(`${job.url}: never said ${JSON.stringify(job.waitText)}`)
  }
  /* A screen reached by an act (⌘K, a sheet): run the act once the page has
     loaded, then wait for what only the opened surface says. */
  if (job.thenEval) {
    await c.evaluate(`(() => { ${job.thenEval}; return true })()`)
    if (job.thenWaitText) {
      let said = false
      for (let i = 0; i < 300 && !said; i += 1) {
        try { said = await c.evaluate(`document.body.innerText.includes(${JSON.stringify(job.thenWaitText)})`) } catch { /* drawing */ }
        if (!said) await sleep(100)
      }
      if (!said) throw new Error(`${job.url}: after the act, never said ${JSON.stringify(job.thenWaitText)}`)
    }
  }
  await c.evaluate('document.fonts ? document.fonts.ready.then(() => true) : true')
  await sleep(job.settle ?? 800)
  await c.evaluate(`(() => { document.getAnimations?.().forEach((a) => { try { a.pause(); a.currentTime = 0 } catch {} }); return true })()`)
  let clip
  if (job.full) {
    const size = await c.evaluate('({w: Math.max(document.documentElement.scrollWidth, innerWidth), h: Math.max(document.documentElement.scrollHeight, innerHeight)})')
    await c.send('Emulation.setDeviceMetricsOverride', { width: w, height: size.h, deviceScaleFactor: job.scale || 1, mobile: false })
    await sleep(200)
    clip = { x: 0, y: 0, width: w, height: size.h, scale: 1 }
  }
  const shot = await c.send('Page.captureScreenshot', { format: 'png', ...(clip ? { clip, captureBeyondViewport: true } : {}) })
  await mkdir(dirname(resolve(job.out)), { recursive: true })
  await writeFile(job.out, Buffer.from(shot.data, 'base64'))
  return { out: job.out, errors: [...c.errors] }
}

const args = parse(process.argv.slice(2))
const jobs = args.batch ? JSON.parse(await readFile(args.batch, 'utf8')) : args.jobs
const c = await open()
let failed = 0
try {
  for (const job of jobs) {
    try {
      const r = await shoot(c, job)
      console.log(`shot ${r.out}${r.errors.length ? `  (page errors: ${r.errors.slice(0, 3).join(' | ')})` : ''}`)
    } catch (e) {
      failed += 1
      console.log(`FAILED ${job.out}: ${e.message}`)
    }
  }
} finally {
  await c.close()
}
process.exit(failed ? 1 : 0)
