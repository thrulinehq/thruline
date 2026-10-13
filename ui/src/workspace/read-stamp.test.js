/* T10 acceptance 3 — local HH:MM, with date and zone on hover, from a pure
 * function of (clock value, zone). */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readStamp } from './read-stamp.js'

/* 2026-09-26T10:34:41Z — 19:34 in Tokyo, 06:34 in New York. */
const MS = Date.UTC(2026, 8, 26, 10, 34, 41)

test('(t10.3) a positive offset: the visible text is local HH:MM, the title is date, time and offset', () => {
  assert.deepEqual(readStamp(MS, 'Asia/Tokyo'), { text: '19:34', title: '2026-09-26 19:34 +09:00' })
})

test('(t10.3) a negative offset and UTC itself', () => {
  assert.deepEqual(readStamp(MS, 'America/New_York'), { text: '06:34', title: '2026-09-26 06:34 -04:00' })
  assert.deepEqual(readStamp(MS, 'UTC'), { text: '10:34', title: '2026-09-26 10:34 +00:00' })
})

test('(t10.5) no completed read gives no stamp — never a guessed time', () => {
  assert.equal(readStamp(null, 'UTC'), null)
  assert.equal(readStamp(undefined, 'UTC'), null)
  assert.equal(readStamp(NaN, 'UTC'), null)
  assert.equal(readStamp('10:34', 'UTC'), null)
})

/* T10 half 1 is already true (the card's evidence: no span is composed
 * anywhere). This guard keeps it true: a clock-time range "a–b" may only be
 * drawn from two recorded times, and today none is drawn at all. */
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { HQ_COPY } from './hq-copy.js'

const TIMEISH = /[tT]ime|[wW]hen|[cC]lock|[hH]our|[sS]tart|[eE]nd(?!_?[sS]eq)|(^|_)at$|[a-z]At$/

test('(t10.4) no copy template and no screen composes a clock-time span', () => {
  const leaves = (v, out = []) => {
    if (typeof v === 'string') out.push(v)
    else if (v && typeof v === 'object') Object.values(v).forEach((x) => leaves(x, out))
    return out
  }
  for (const text of leaves(HQ_COPY)) {
    for (const m of text.matchAll(/\{([a-z_]+)\}\s*[–-]\s*\{([a-z_]+)\}/g)) {
      assert.ok(!TIMEISH.test(m[1]) && !TIMEISH.test(m[2]), `a copy template composes a time span: ${text}`)
    }
  }
  const root = fileURLToPath(new URL('..', import.meta.url))
  const walk = (dir, out = []) => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name)
      if (statSync(p).isDirectory()) walk(p, out)
      else if (/\.(js|jsx)$/.test(name) && !/\.test\.js$/.test(name) && name !== 'demo-data.js') out.push(p)
    }
    return out
  }
  for (const file of walk(root)) {
    const src = readFileSync(file, 'utf8')
    for (const m of src.matchAll(/([\w.$]+)\)?\s*\+\s*['"`]\s*–\s*['"`]\s*\+\s*\(?([\w.$]+)/g)) {
      assert.ok(!TIMEISH.test(m[1].split('.').pop()) && !TIMEISH.test(m[2].split('.').pop()), `${file.slice(root.length)} composes a time span: ${m[0]}`)
    }
  }
})
