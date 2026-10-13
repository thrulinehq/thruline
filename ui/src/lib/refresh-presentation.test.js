import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  REFRESH_ENDPOINTS,
  REFRESH_RECOVERY,
  refreshPresentation,
} from './refresh-presentation.js'

const server = (lastRefresh = null) => ({
  have_export: true,
  refreshing: false,
  last_refresh: lastRefresh,
  generated_at: '2026-08-15T04:25:58Z',
})

test('refresh presentation is a closed hidden, idle, busy, failure, and completion state machine', () => {
  const hidden = refreshPresentation(null)
  assert.equal(hidden.visible, false)
  assert.equal(hidden.reloadEligible, false)

  const idle = refreshPresentation(server())
  assert.deepEqual(
    [idle.phase, idle.actionLabel, idle.busy, idle.error, idle.reloadEligible],
    ['idle', 'Refresh reading', false, false, false],
  )

  for (const [phase, title] of [
    ['starting', 'Refresh is starting'],
    ['busy', 'Refresh is running'],
    ['already_running', 'Refresh already running'],
  ]) {
    const view = refreshPresentation(server(), phase)
    assert.equal(view.title, title)
    assert.equal(view.busy, true)
    assert.equal(view.reloadEligible, false)
    assert.match(view.message, /Reading already loaded stays visible/)
    assert.match(view.message, /navigation remains usable/)
    assert.match(view.message, /Terminal output names progress/)
    assert.match(view.message, /reloads only after refresh finishes successfully/)
  }

  const prior = refreshPresentation(server({ ok: false, detail: 'private detail' }))
  assert.equal(prior.phase, 'prior_failure')
  assert.equal(prior.title, 'Last refresh failed')
  assert.equal(prior.error, true)
  assert.doesNotMatch(JSON.stringify(prior), /private detail/)

  for (const phase of ['start_failure', 'connection_failure', 'refresh_failure']) {
    const view = refreshPresentation(server(), phase)
    assert.equal(view.error, true)
    assert.equal(view.actionLabel, 'Try refresh again')
    assert.equal(view.reloadEligible, false)
  }

  const complete = refreshPresentation(server({ ok: true }), 'complete')
  assert.equal(complete.reloadEligible, true)
  assert.equal(complete.actionLabel, 'Reloading reading…')
})

test('recovery copy is exact about cancellation, foreground stop, retained reading, and restart', () => {
  assert.match(REFRESH_RECOVERY, /browser cannot cancel the worker by itself/)
  assert.match(REFRESH_RECOVERY, /Ctrl\+C in the terminal/)
  assert.match(REFRESH_RECOVERY, /stop the foreground app and its refresh/)
  assert.match(REFRESH_RECOVERY, /page keeps the reading already loaded/)
  assert.match(REFRESH_RECOVERY, /Restart thruline ui before trying again/)

  const copy = [
    REFRESH_RECOVERY,
    ...['idle', 'starting', 'busy', 'already_running', 'start_failure', 'connection_failure', 'refresh_failure', 'complete']
      .flatMap((phase) => Object.values(refreshPresentation(server(), phase))),
  ].join(' ')
  assert.doesNotMatch(copy, /\b\d+(?:\.\d+)?%|\bpercent\b|\bETA\b|remaining time/i)
})

test('production wiring has one in-flow Refresh region and only the measured endpoints', () => {
  const refresh = readFileSync(fileURLToPath(new URL('../refresh.jsx', import.meta.url)), 'utf8')
  const main = readFileSync(fileURLToPath(new URL('../main.jsx', import.meta.url)), 'utf8')
  const shell = readFileSync(fileURLToPath(new URL('../os/shell.jsx', import.meta.url)), 'utf8')
  const css = readFileSync(fileURLToPath(new URL('../os.css', import.meta.url)), 'utf8')
  const renders = [main, shell].map((source) => source.match(/<Refresh\b/g)?.length || 0)
  assert.equal(renders.reduce((sum, count) => sum + count, 0), 1)
  assert.doesNotMatch(main, /Refresh/)
  assert.match(shell, /<header className="os-topbar">[\s\S]*<Refresh \/>[\s\S]*<\/header>/)
  assert.deepEqual(REFRESH_ENDPOINTS, { state: '/api/state', start: '/api/refresh' })
  assert.match(refresh, /fetch\(REFRESH_ENDPOINTS\.state/)
  assert.match(refresh, /fetch\(REFRESH_ENDPOINTS\.start/)
  assert.doesNotMatch(refresh, /\/api\/(?!state|refresh)[A-Za-z_-]+|cancel|abort/i)
  assert.doesNotMatch(refresh, /position:\s*['"]fixed|bottom:|right:|zIndex:\s*40/)
  assert.equal((refresh.match(/role="status"/g) || []).length, 1)
  assert.match(refresh, /disabled=\{view\.busy \|\| view\.reloadEligible\}/)
  assert.match(refresh, /clearInterval\(timer\)/)
  assert.match(refresh, /view\.reloadEligible[\s\S]*window\.location\.reload\(\)/)
  assert.match(refresh, /server\.last_refresh\?\.ok === false|last_refresh/)
  assert.doesNotMatch(refresh + shell + main, /localStorage|sessionStorage|indexedDB/)
  assert.match(css, /\.os-refresh-region[^}]*min-width:\s*0/)
  assert.match(css, /@media \(max-width: 900px\)[\s\S]*\.os-topbar[^}]*flex-wrap:\s*wrap/)
})
