/* C1 capture tool. Its bytes are embedded in the Decisions binary and included
 * in visual_tool_identity. Inputs are local routes and fixed scenarios only. */
import { spawn } from 'node:child_process'
import { readFile, writeFile, mkdir, appendFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function portFromProfile(profile, child) {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (child.exitCode !== null) throw new Error('renderer exited before DevTools started')
    try {
      const [port, socket] = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')
      if (Number(port) > 0 && socket?.startsWith('/')) return Number(port)
    } catch { /* renderer still starting */ }
    await delay(100)
  }
  throw new Error('renderer did not start DevTools')
}

async function connect(port, origin, allowedPaths, housekeepingPaths, targetId) {
  let target
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const all = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
      target = all.find((item) => item.type === 'page' && (!targetId || item.id === targetId))
    } catch { /* renderer still starting */ }
    if (target) break
    await delay(100)
  }
  if (!target) throw new Error('renderer page did not appear')
  const ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => {
    ws.onopen = resolve
    ws.onerror = () => reject(new Error('DevTools socket did not open'))
  })
  let next = 0
  const pending = new Map()
  const inFlight = new Set()
  let lastNetworkAt = Date.now()
  let unexpectedRequest = false
  const housekeepingResponses = new Map()
  const allowedRequest = (url) => {
    if (url === 'about:blank' || url.startsWith('data:')) return true
    try {
      const parsed = new URL(url)
      if (parsed.protocol === 'blob:') return parsed.origin === origin
      return parsed.origin === origin && !parsed.username && !parsed.password &&
        (!allowedPaths || (!parsed.search &&
          (allowedPaths.has(parsed.pathname) || housekeepingPaths?.has(parsed.pathname))))
    } catch { return false }
  }
  ws.onmessage = ({ data }) => {
    const message = JSON.parse(data)
    if (message.method === 'Fetch.requestPaused') {
      const allowed = allowedRequest(message.params.request.url)
      if (!allowed) unexpectedRequest = true
      void send(allowed ? 'Fetch.continueRequest' : 'Fetch.failRequest', {
        requestId: message.params.requestId, ...(!allowed ? { errorReason: 'BlockedByClient' } : {}),
      }).catch(() => { unexpectedRequest = true })
    } else if (message.method === 'Network.responseReceived') {
      try {
        const url = new URL(message.params.response.url)
        if (url.origin === origin && !url.search && housekeepingPaths?.has(url.pathname)) {
          const status = message.params.response.status
          housekeepingResponses.set(url.pathname, {path: url.pathname, status})
          if (status !== 204) unexpectedRequest = true
        }
      } catch { unexpectedRequest = true }
    } else if (message.method === 'Network.webSocketCreated') {
      // Screens in the frozen capture contract do not open socket transports.
      unexpectedRequest = true
    } else if (message.method === 'Network.requestWillBeSent') {
      inFlight.add(message.params.requestId)
      lastNetworkAt = Date.now()
    } else if (message.method === 'Network.loadingFinished' || message.method === 'Network.loadingFailed') {
      inFlight.delete(message.params.requestId)
      lastNetworkAt = Date.now()
    }
    const entry = pending.get(message.id)
    if (!entry) return
    pending.delete(message.id)
    clearTimeout(entry.timer)
    if (message.error) entry.reject(new Error(message.error.message))
    else entry.resolve(message.result)
  }
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++next
    const timer = setTimeout(() => {
      pending.delete(id)
      reject(new Error(`${method} timed out`))
    }, 30000)
    pending.set(id, { resolve, reject, timer })
    ws.send(JSON.stringify({ id, method, params }))
  })
  const evaluate = async (expression) => {
    const answer = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (answer.exceptionDetails) throw new Error('page evaluation failed')
    return answer.result.value
  }
  await send('Page.enable')
  await send('Runtime.enable')
  await send('Network.enable')
  await send('Network.setBypassServiceWorker', { bypass: true })
  await send('Fetch.enable', { patterns: [{ urlPattern: '*', requestStage: 'Request' }] })
  return {
    ws, send, evaluate,
    networkQuiet: (ms) => inFlight.size === 0 && Date.now() - lastNetworkAt >= ms,
    assertRequests: () => { if (unexpectedRequest) throw new Error('screen attempted an unexpected request outside its local capture contract') },
    resetRequests: () => { unexpectedRequest = false; housekeepingResponses.clear() },
    housekeeping: () => [...housekeepingResponses.values()],
  }
}

function localUrl(origin, route) {
  if (typeof route === 'string' && (route.startsWith('#/') || route.startsWith('/')) &&
      !route.startsWith('//') && !route.includes('://') && !route.includes('\\')) {
    const url = new URL(route, `${origin}/`)
    if (url.origin === origin && !url.username && !url.password) return url.href
  }
  throw new Error('route is not local')
}

function checkedOrigin(value) {
  const url = new URL(value)
  const port = Number(url.port || '80')
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || url.username || url.password ||
      !Number.isInteger(port) || port < 1 || port > 65535 || url.pathname !== '/' || url.search || url.hash) {
    throw new Error('server is not an exact loopback origin')
  }
  return url.origin
}

const designRoute = (route) => route.startsWith('#/design/') || route.startsWith('/#/design/')

async function settled(client) {
  if (!client.networkQuiet(1000)) return false
  try {
    return await client.evaluate('document.readyState === "complete" && !!document.querySelector("#root > *") && performance.now() - (globalThis.__thrulineCaptureMutationAt ?? performance.now()) >= 1000')
  } catch {
    return false
  }
}

async function waitForSettled(client, deadline) {
  while (Date.now() < deadline) {
    client.assertRequests()
    if (await settled(client)) return
    await delay(100)
  }
  throw new Error('screen did not settle after its record reads')
}

async function screenshot(client, job) {
  client.assertRequests()
  return client.send('Page.captureScreenshot', {
    format: 'png', clip: { x: 0, y: 0, width: job.width, height: job.height, scale: 1 },
    captureBeyondViewport: false,
  })
}

async function contentGeometry(client, job) {
  return client.evaluate(`(() => {
    const frameWidth = ${job.width}, frameHeight = ${job.height};
    const root = document.querySelector('#root');
    const protectedRoots = new Set([document.documentElement, document.body, root, root?.firstElementChild]);
    const scroll_regions = [], clipped_regions = [];
    let left = 0, top = 0, right = frameWidth, bottom = frameHeight, count = 0;
    const extend = (x, y, width, height) => {
      left = Math.min(left, x); top = Math.min(top, y);
      right = Math.max(right, x + width); bottom = Math.max(bottom, y + height);
    };
    const walk = (element) => {
      if (++count > 20000) throw new Error('screen exceeds geometry element bounds');
      const css = getComputedStyle(element), rect = element.getBoundingClientRect();
      if (css.display === 'none') return;
      const x = rect.x + scrollX, y = rect.y + scrollY;
      const width = rect.width, height = rect.height;
      if (![x,y,width,height].every(Number.isFinite)) throw new Error('screen geometry is invalid');
      extend(x, y, width, height);
      const content_width = Math.max(Math.ceil(width), element.scrollWidth);
      const content_height = Math.max(Math.ceil(height), element.scrollHeight);
      const region = {x,y,width,height,content_width,content_height,
        overflow_x: content_width > Math.ceil(width), overflow_y: content_height > Math.ceil(height)};
      const inside = x >= 0 && y >= 0 && x + width <= frameWidth && y + height <= frameHeight;
      const scrollXStyle = /^(auto|scroll)$/.test(css.overflowX);
      const scrollYStyle = /^(auto|scroll)$/.test(css.overflowY);
      const boundedScroll = !protectedRoots.has(element) && inside &&
        ((scrollXStyle && region.overflow_x) || (scrollYStyle && region.overflow_y));
      if (boundedScroll) {
        scroll_regions.push(region);
        // Scroll contents remain clipped by a designed pane inside the artboard.
        // A non-scrolling axis can still escape the reference frame.
        extend(x, y, scrollXStyle ? width : content_width, scrollYStyle ? height : content_height);
        return;
      }
      const clipped = /^(hidden|clip)$/.test(css.overflowX) || /^(hidden|clip)$/.test(css.overflowY);
      if (clipped && (region.overflow_x || region.overflow_y)) clipped_regions.push(region);
      if (protectedRoots.has(element) || (clipped && css.textOverflow !== 'ellipsis')) {
        extend(x, y, content_width, content_height);
      }
      // A normal single-line ellipsis is fully represented by its visible box.
      if (clipped && css.textOverflow === 'ellipsis' && inside) return;
      for (const child of element.children) walk(child);
    };
    walk(document.documentElement);
    return {content_width: Math.ceil(right - Math.min(0,left)), content_height: Math.ceil(bottom - Math.min(0,top)),
      overflow_x: left < 0 || right > frameWidth, overflow_y: top < 0 || bottom > frameHeight,
      scroll_regions, clipped_regions};
  })()`)
}

async function capture(client, origin, job) {
  if (!['fixed-record', 'default', 'design', 'command_overlay'].includes(job.scenario)) {
    throw new Error('scenario is not supported by the pinned capture tool')
  }
  if (job.scenario === 'design' && !designRoute(job.route)) {
    throw new Error('design scenario needs an explicit design route')
  }
  if (job.scenario !== 'design' && designRoute(job.route)) {
    throw new Error('design route needs the design scenario')
  }
  if (!Number.isInteger(job.width) || !Number.isInteger(job.height) ||
      job.width < 320 || job.height < 240 || job.width > 4096 || job.height > 4096) {
    throw new Error('viewport is outside capture bounds')
  }
  await client.send('Emulation.setDeviceMetricsOverride', {
    width: job.width, height: job.height, deviceScaleFactor: 1, mobile: false,
  })
  await client.send('Page.navigate', { url: 'about:blank' })
  let cleared = false
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      cleared = await client.evaluate('location.href === "about:blank" && document.readyState === "complete"')
    } catch { /* navigation still running */ }
    if (cleared) break
    await delay(50)
  }
  if (!cleared) throw new Error('previous screen did not clear')
  client.resetRequests()
  const targetUrl = localUrl(origin, job.route)
  await client.send('Page.navigate', { url: targetUrl })
  let drawn = false
  for (let attempt = 0; attempt < 300 && !drawn; attempt += 1) {
    client.assertRequests()
    try {
      drawn = await client.evaluate(`location.href === ${JSON.stringify(targetUrl)} && document.readyState === "complete" && !!document.querySelector("#root > *")`)
    } catch { /* navigation still running */ }
    if (!drawn) await delay(100)
  }
  if (!drawn) throw new Error('page did not draw its root')
  const deadline = Date.now() + 30000
  await client.evaluate('document.fonts ? document.fonts.ready.then(() => true) : true')
  await waitForSettled(client, deadline)
  if (job.scenario === 'command_overlay') {
    await client.evaluate('document.dispatchEvent(new KeyboardEvent("keydown", {key:"k",code:"KeyK",metaKey:true,bubbles:true}))')
    let opened = false
    for (let attempt = 0; attempt < 50 && !opened; attempt += 1) {
      opened = await client.evaluate('!!document.querySelector("section[role=dialog][aria-labelledby=tl-cmdk-label], div[role=dialog][aria-label^=Command]")')
      if (!opened) await delay(100)
    }
    if (!opened) throw new Error('command overlay did not open')
  }
  await waitForSettled(client, deadline)
  const errorRoot = await client.evaluate('(() => { const root = document.querySelector("#root"); const first = root?.firstElementChild; return first?.tagName === "PRE" && /ThruLine — (cannot render|render failed)/.test(first.innerText || "") })()')
  if (errorRoot) throw new Error('exact version rendered an error page')
  if (!await client.evaluate(`Date.now() === ${job.fixedMs} && +new Date() === ${job.fixedMs}`)) {
    throw new Error('page did not retain the pinned capture clock')
  }
  let image
  while (Date.now() < deadline) {
    await waitForSettled(client, deadline)
    await client.evaluate('(() => { document.getAnimations?.().forEach((animation) => { try { animation.pause(); animation.currentTime = 0 } catch {} }); return true })()')
    const first = await screenshot(client, job)
    await delay(500)
    if (!await settled(client)) continue
    const second = await screenshot(client, job)
    if (first.data !== second.data) continue
    await delay(500)
    if (!await settled(client)) continue
    const third = await screenshot(client, job)
    if (second.data === third.data && await settled(client)) {
      image = third
      break
    }
  }
  if (!image) throw new Error('screen pixels did not settle')
  client.assertRequests()
  const geometry = await contentGeometry(client, job)
  client.assertRequests()
  await mkdir(dirname(job.output), { recursive: true })
  await writeFile(job.output, Buffer.from(image.data, 'base64'))
  return { screen: job.screen, screenshot_path: job.output, error: null,
    housekeeping_responses: client.housekeeping(), ...geometry }
}

async function main() {
  const input = JSON.parse(await readFile(process.argv[2], 'utf8'))
  const origin = checkedOrigin(input.origin)
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(input.capture_clock || '')) {
    throw new Error('capture clock is not pinned to a UTC second')
  }
  const fixedMs = Date.parse(input.capture_clock)
  if (!Number.isFinite(fixedMs) || new Date(fixedMs).toISOString().replace('.000Z', 'Z') !== input.capture_clock) {
    throw new Error('capture clock is invalid')
  }
  if (!/^[0-9a-f]{64}$/.test(input.fixture_sha256 || '')) throw new Error('capture fixture hash is missing or invalid')
  if (!Array.isArray(input.jobs) || input.jobs.length < 1 || input.jobs.length > 64) throw new Error('capture jobs are outside bounds')
  if (input.renderer_version !== 'Chrome/154.0.8037.98') throw new Error('renderer version is not the approved CDP product')
  let allowedPaths
  if (input.allowed_paths != null) {
    if (!Array.isArray(input.allowed_paths) || input.allowed_paths.length < 1 || input.allowed_paths.length > 20000) {
      throw new Error('static capture paths are outside bounds')
    }
    allowedPaths = new Set()
    for (const path of input.allowed_paths) {
      if (typeof path !== 'string') throw new Error('static capture path is invalid')
      const parsed = new URL(path, `${origin}/`)
      if (!path.startsWith('/') || path.startsWith('//') || path.includes('\\') || parsed.origin !== origin ||
          parsed.pathname !== path || parsed.search || parsed.hash || allowedPaths.has(path)) {
        throw new Error('static capture path is invalid or duplicated')
      }
      allowedPaths.add(path)
    }
  }
  let housekeepingPaths
  if (input.housekeeping_paths != null) {
    if (!allowedPaths || !Array.isArray(input.housekeeping_paths) ||
        input.housekeeping_paths.length !== 1 || input.housekeeping_paths[0] !== '/favicon.ico') {
      throw new Error('generated housekeeping route is outside the closed static contract')
    }
    housekeepingPaths = new Set(input.housekeeping_paths)
  }
  const names = new Set(), outputs = new Set()
  for (const job of input.jobs) {
    if (typeof job.screen !== 'string' || !job.screen || typeof job.output !== 'string' || !job.output) {
      throw new Error('capture job name or output is invalid')
    }
    const output = resolve(job.output)
    if (names.has(job.screen) || outputs.has(output)) throw new Error('capture jobs have duplicate names or outputs')
    names.add(job.screen); outputs.add(output)
  }
  const profileStage = async (stage, started, screen) => {
    // Optional timing diagnostics never decide whether a screen is checkable.
    try {
      if (input.profile_path) await appendFile(input.profile_path, JSON.stringify({stage, ...(screen ? {screen} : {}), elapsed_ms: performance.now() - started}) + '\n', {mode: 0o600})
    } catch { /* diagnostic path became unavailable */ }
  }
  const profile = join(dirname(process.argv[2]), 'chrome-profile')
  await mkdir(profile, { recursive: true })
  const chromeStarted = performance.now()
  const chrome = spawn(input.renderer, [
    '--headless=new', '--use-mock-keychain', '--password-store=basic',
    '--no-first-run', '--no-default-browser-check', '--disable-background-networking',
    '--remote-debugging-port=0', '--disable-extensions', '--hide-scrollbars',
    '--disable-background-timer-throttling', '--disable-renderer-backgrounding', '--disable-backgrounding-occluded-windows',
    `--user-data-dir=${profile}`, 'about:blank',
  ], { stdio: 'ignore' })
  try {
    const port = await portFromProfile(profile, chrome)
    const client = await connect(port, origin, allowedPaths, housekeepingPaths)
    const clients = [client]
    await profileStage('chrome_start', chromeStarted)
    try {
      const version = await client.send('Browser.getVersion')
      if (version.product !== input.renderer_version) throw new Error('running renderer version does not match the approved product')
      const tabsStarted = performance.now()
      for (let index = 1; index < Math.min(4, input.jobs.length); index += 1) {
        const { targetId } = await client.send('Target.createTarget', { url: 'about:blank' })
        clients.push(await connect(port, origin, allowedPaths, housekeepingPaths, targetId))
      }
      const source = `(() => {
          const fixedMs = ${fixedMs};
          const NativeDate = globalThis.Date;
          function PinnedDate(...args) {
            if (!new.target) return new NativeDate(fixedMs).toString();
            return Reflect.construct(NativeDate, args.length ? args : [fixedMs], new.target);
          }
          Object.setPrototypeOf(PinnedDate, NativeDate);
          PinnedDate.prototype = NativeDate.prototype;
          Object.defineProperty(PinnedDate.prototype, 'constructor', {value: PinnedDate, configurable: true, writable: true});
          Object.defineProperty(PinnedDate, 'now', {value: () => fixedMs, configurable: true, writable: true});
          globalThis.Date = PinnedDate;
          globalThis.__thrulineCaptureMutationAt = performance.now();
          new MutationObserver(() => { globalThis.__thrulineCaptureMutationAt = performance.now(); })
            .observe(document, { subtree: true, childList: true, characterData: true, attributes: true });
        })()`
      await Promise.all(clients.map((tab) => tab.send('Page.addScriptToEvaluateOnNewDocument', { source })))
      await profileStage('tab_setup', tabsStarted)
      const results = new Array(input.jobs.length)
      let nextJob = 0, completed = 0
      await Promise.all(clients.map(async (tab) => {
        while (nextJob < input.jobs.length) {
          const index = nextJob++, job = input.jobs[index]
          const started = performance.now()
          const basis = {capture_clock: input.capture_clock, fixture_sha256: input.fixture_sha256,
            renderer_version: version.product, frame_width: job.width, frame_height: job.height,
            content_width: null, content_height: null, overflow_x: null, overflow_y: null,
            scroll_regions: [], clipped_regions: [], housekeeping_responses: []}
          try {
            results[index] = {...basis, ...await capture(tab, origin, {...job, fixedMs})}
          } catch (error) {
            results[index] = { ...basis, housekeeping_responses: tab.housekeeping(), screen: job.screen,
              screenshot_path: null, error: String(error.message).slice(0, 200) }
          }
          // Synchronous counting and output preserve progress order even when
          // tabs finish out of order or a timing-log append yields.
          process.stderr.write(`Captured ${++completed} of ${input.jobs.length}\n`)
          await profileStage('capture_screen', started, job.screen)
        }
      }))
      process.stdout.write(JSON.stringify(results))
    } finally {
      for (const tab of clients) tab.ws.close()
    }
  } finally {
    chrome.kill('SIGKILL')
  }
}

main().catch((error) => {
  process.stderr.write(String(error.message).slice(0, 200))
  process.exitCode = 1
})
