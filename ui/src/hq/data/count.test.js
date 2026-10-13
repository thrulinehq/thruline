/* C29: the one count formatter. Thousands separated by a comma on every count
 * of 1,000 or more, the same on every machine (it never asks the locale). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { fmtCount, countOf } from './count.js'

test('a count of 1,000 or more is grouped by threes with a comma; smaller counts are untouched', () => {
  const cases = [
    [0, '0'], [7, '7'], [999, '999'], [1000, '1,000'], [1679, '1,679'], [4330, '4,330'], [5646, '5,646'],
    [76746, '76,746'], [100000, '100,000'], [1234567, '1,234,567'], [11700000000, '11,700,000,000'],
    [-1204, '-1,204'], [1234.5, '1,234.5'], [-0, '0'],
  ]
  for (const [n, want] of cases) assert.equal(fmtCount(n), want, String(n))
})

test('anything that is not a finite number reads exactly as String(x) did, so it is a drop-in', () => {
  for (const x of [undefined, null, NaN, Infinity, '5646', '—', true]) assert.equal(fmtCount(x), String(x))
})

test('it does not depend on the machine’s locale', () => {
  const was = Number.prototype.toLocaleString
  Number.prototype.toLocaleString = function () { return 'LOCALE' } // eslint-disable-line no-extend-native
  try {
    assert.equal(fmtCount(76746), '76,746')
  } finally {
    Number.prototype.toLocaleString = was // eslint-disable-line no-extend-native
  }
  const src = readFileSync(new URL('./count.js', import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
  assert.ok(!/toLocaleString|Intl\./.test(src), 'the formatter asks the locale')
})

test('countOf names the noun, singular for exactly one', () => {
  assert.equal(countOf(1, 'step', 'steps'), '1 step')
  assert.equal(countOf(1679, 'step', 'steps'), '1,679 steps')
  assert.equal(countOf(0, 'session', 'sessions'), '0 sessions')
})

test('no HQ file formats a count through the machine’s locale: count.js is the one formatter', () => {
  const root = join(dirname(fileURLToPath(import.meta.url)), '..')
  const walk = (d) => readdirSync(d).flatMap((f) => {
    const p = join(d, f)
    return statSync(p).isDirectory() ? walk(p) : /\.(js|jsx)$/.test(f) && !/\.test\.js$/.test(f) ? [p] : []
  })
  const offenders = walk(root).filter((p) => /toLocaleString\(|Intl\.NumberFormat/.test(readFileSync(p, 'utf8')))
  assert.deepEqual(offenders, [])
})
