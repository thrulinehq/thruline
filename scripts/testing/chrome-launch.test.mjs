/* R-2026-09-23-12 — the flags are in the shared launch, and no launcher in
 * this repository can get Chrome open without them.
 *
 * Run: node --test scripts/testing/chrome-launch.test.mjs
 * (also listed in ui/package.json's `test` script, with a relative path out
 * of `ui/`, so `npm test` covers it.)
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import {
  CHROME_ARGS,
  KEYCHAIN_FLAGS,
  CHROME_EXECUTABLE,
  DEVTOOLS_ARGS,
  LITERAL_PORT_REFUSAL,
  launchChromium,
  refuseLiteralPort,
  spawnChromeWithDevTools,
} from './chrome-launch.mjs'

const REQUIRED = ['--use-mock-keychain', '--password-store=basic', '--no-first-run', '--no-default-browser-check']
const HERE = import.meta.dirname

test('the four ruled flags are in CHROME_ARGS', () => {
  for (const flag of REQUIRED) assert.ok(CHROME_ARGS.includes(flag), `CHROME_ARGS is missing ${flag}`)
  for (const flag of REQUIRED) assert.ok(KEYCHAIN_FLAGS.includes(flag), `KEYCHAIN_FLAGS is missing ${flag}`)
})

test('launchChromium passes them to playwright, and extra args cannot remove them', async () => {
  let seen = null
  const chromium = { launch: async (options) => { seen = options; return 'browser' } }
  const browser = await launchChromium(chromium, { args: ['--window-size=800,600'], timeout: 1234 })
  assert.equal(browser, 'browser')
  assert.equal(seen.executablePath, CHROME_EXECUTABLE)
  assert.equal(seen.headless, true)
  assert.equal(seen.timeout, 1234)
  for (const flag of REQUIRED) assert.ok(seen.args.includes(flag), `launch args missing ${flag}`)
  assert.ok(seen.args.includes('--window-size=800,600'), 'caller args are kept')
})

/* ── R-2026-09-24-28 — no fixed DevTools port, anywhere ──────────────── */

test('CHROME_ARGS asks for an ephemeral DevTools port and no other', () => {
  assert.deepEqual([...DEVTOOLS_ARGS], ['--remote-debugging-port=0'])
  const ports = CHROME_ARGS.filter((a) => a.startsWith('--remote-debugging-port='))
  assert.deepEqual(ports, ['--remote-debugging-port=0'], 'CHROME_ARGS must carry exactly one DevTools port flag, and it must be 0')
})

test('a literal DevTools port is refused, and the refusal says so', () => {
  assert.throws(
    () => spawnChromeWithDevTools({ userDataDir: '/tmp/never-spawned', extraArgs: ['--remote-debugging-port=9337'] }),
    (e) => e instanceof Error && e.message.includes(LITERAL_PORT_REFUSAL) && e.message.includes('9337'),
    'spawnChromeWithDevTools accepted the port that crossed two lanes on 2026-09-24',
  )
  /* every digit 1–9 in the leading position, and the two ports that have
   * actually been hand-typed in this repository's harnesses */
  for (const port of ['1', '9222', '9233', '9234', '9337', '20000', '35000', '45000', '55000', '65000', '80', '9999']) {
    assert.throws(
      () => spawnChromeWithDevTools({ userDataDir: '/tmp/never-spawned', extraArgs: ['--window-size=800,600', `--remote-debugging-port=${port}`] }),
      (e) => e.message.includes(LITERAL_PORT_REFUSAL),
      `--remote-debugging-port=${port} was not refused`,
    )
    assert.throws(() => refuseLiteralPort([`--remote-debugging-port=${port}`]), /literal DevTools port is refused/)
  }
  assert.doesNotThrow(() => refuseLiteralPort(['--remote-debugging-port=0', '--window-size=800,600']), 'port 0 is the ruled form and must be allowed')
})

test('launchChromium never receives a literal port either', async () => {
  let seen = null
  const chromium = { launch: async (options) => { seen = options; return 'browser' } }
  await launchChromium(chromium)
  const ports = seen.args.filter((a) => a.startsWith('--remote-debugging-port='))
  assert.deepEqual(ports, ['--remote-debugging-port=0'], 'playwright was handed a pinned port')
  assert.throws(
    () => launchChromium(chromium, { args: ['--remote-debugging-port=9337'] }),
    (e) => e.message.includes(LITERAL_PORT_REFUSAL),
  )
})

test('spawnChromeWithDevTools reads the real port out of DevToolsActivePort', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'chrome-launch-test-'))
  try {
    let spawned = null
    const proc = { exitCode: null, kill() {} }
    const spawnFn = (exe, args) => {
      spawned = { exe, args }
      /* stand in for Chrome: write the file it writes into its own profile */
      fs.writeFile(path.join(dir, 'DevToolsActivePort'), '52341\n/devtools/browser/abc-123\n')
      return proc
    }
    const chrome = await spawnChromeWithDevTools({ userDataDir: dir, extraArgs: ['--window-size=800,600'], spawnFn })
    assert.equal(chrome.port, 52341)
    assert.equal(chrome.wsEndpoint, 'ws://127.0.0.1:52341/devtools/browser/abc-123')
    assert.equal(chrome.proc, proc)
    assert.equal(spawned.exe, CHROME_EXECUTABLE)
    assert.ok(spawned.args.includes('--remote-debugging-port=0'), 'the spawn did not ask for an ephemeral port')
    assert.ok(spawned.args.includes(`--user-data-dir=${dir}`), 'the spawn did not use the profile it was given')
    for (const flag of REQUIRED) assert.ok(spawned.args.includes(flag), `the spawn is missing ${flag}`)
  } finally {
    await fs.rm(dir, { recursive: true, force: true })
  }
})

test('spawnChromeWithDevTools refuses to share a profile it was not given', () => {
  assert.throws(() => spawnChromeWithDevTools({ extraArgs: [] }), /fresh profile/)
})

test('every .mjs under scripts/ that mentions a DevTools port imports the helper', async () => {
  const roots = [path.resolve(HERE, '..')]
  /* The helper defines the rule and this file is the guard: both name the
   * refused port on purpose, so neither is scanned for hard-coded ports. */
  const SELF = ['chrome-launch.mjs', 'chrome-launch.test.mjs']
  const files = []
  async function walk(dir) {
    for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name.startsWith('.')) continue
      const full = path.join(dir, entry.name)
      if (entry.isDirectory()) await walk(full)
      else if (entry.name.endsWith('.mjs')) files.push(full)
    }
  }
  for (const root of roots) await walk(root)
  assert.ok(files.length >= 9, `expected to scan the harness .mjs files, found ${files.length}`)
  let mentions = 0
  for (const full of files) {
    if (SELF.includes(path.basename(full))) continue
    const source = await fs.readFile(full, 'utf8')
    if (!/remote-debugging-port|DevToolsActivePort/.test(source)) continue
    mentions += 1
    assert.match(source, /from\s*'[^']*chrome-launch\.mjs'/, `${full} names a DevTools port without importing scripts/testing/chrome-launch.mjs`)
    assert.ok(!/--remote-debugging-port=[1-9]/.test(source), `${full} hard-codes a DevTools port number`)
    assert.ok(!/--remote-debugging-port=\$\{/.test(source), `${full} interpolates a DevTools port — use spawnChromeWithDevTools`)
  }
  assert.ok(mentions >= 1, 'the scan found nothing to guard — it is not reading the harnesses')
})
