import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  INITIAL_RECEIPT_COUNT,
  receiptRegionId,
  receiptWindow,
} from './receipt-window.js'

const receipts = (total) => Array.from({ length: total }, (_, index) => ({
  handle: `receipt_${index.toString(16).padStart(16, '0')}`,
  node: `node-${index + 1}`,
}))

test('zero, two, and four receipts render completely without a disclosure control', () => {
  for (const total of [0, 2, 4]) {
    const source = receipts(total)
    const window = receiptWindow(source)
    assert.equal(window.total, total)
    assert.equal(window.items.length, total)
    assert.equal(window.hasDisclosure, false)
    assert.equal(window.controlLabel, null)
    assert.deepEqual(window.items.map((item) => item.receipt), source)
  }
})

test('five receipts show the exact first four and expand in original order', () => {
  const source = receipts(5)
  const initial = receiptWindow(source)
  assert.equal(initial.items.length, INITIAL_RECEIPT_COUNT)
  assert.equal(initial.items[0].receipt, source[0])
  assert.equal(initial.items.at(-1).receipt, source[3])
  assert.equal(initial.controlLabel, 'Show all 5 receipts')

  const expanded = receiptWindow(source, true)
  assert.equal(expanded.items.length, 5)
  assert.deepEqual(expanded.items.map((item) => item.receipt), source)
  assert.equal(expanded.items.at(-1).receipt, source[4])
  assert.equal(expanded.controlLabel, 'Show fewer receipts')
})

test('thirty-five receipts collapse four of thirty-five and expand all thirty-five without loss', () => {
  const source = receipts(35)
  const initial = receiptWindow(source)
  assert.equal(initial.items.length, 4)
  assert.deepEqual(initial.items.map((item) => item.receipt), source.slice(0, 4))
  assert.equal(initial.controlLabel, 'Show all 35 receipts')

  const expanded = receiptWindow(source, true)
  assert.equal(expanded.items.length, 35)
  assert.deepEqual(expanded.items.map((item) => item.receipt), source)
  assert.equal(expanded.items.at(-1).receipt.node, 'node-35')
  assert.equal(new Set(expanded.items.map((item) => item.visibleLabel)).size, 35)
  assert.equal(new Set(expanded.items.map((item) => item.ariaLabel)).size, 35)
  assert.equal(expanded.items[0].visibleLabel, 'Receipt 1 of 35')
  assert.equal(expanded.items.at(-1).ariaLabel, 'Open exact receipt 35 of 35')

  const collapsed = receiptWindow(source, false)
  assert.deepEqual(collapsed.items.map((item) => item.receipt), source.slice(0, 4))
  assert.equal(collapsed.items.at(-1).receipt.node, 'node-4')
})

test('controlled region identity is stable, row-scoped, and fail-closed', () => {
  assert.equal(receiptRegionId('row_0123456789abcdef0123'), 'wm-receipts-row_0123456789abcdef0123')
  assert.notEqual(receiptRegionId('row_0123456789abcdef0123'), receiptRegionId('row_abcdef0123456789abcd'))
  assert.equal(receiptRegionId('row/unsafe'), null)
})

test('the Work Map keeps the controlled disclosure and exact receipt door wired', () => {
  const source = readFileSync(
    fileURLToPath(new URL('../mac/work-map.jsx', import.meta.url)),
    'utf8',
  )
  const helper = readFileSync(
    fileURLToPath(new URL('./receipt-window.js', import.meta.url)),
    'utf8',
  )
  const css = readFileSync(
    fileURLToPath(new URL('../macos.css', import.meta.url)),
    'utf8',
  )
  assert.match(source, /id=\{regionId\}/)
  assert.match(source, /aria-controls=\{regionId\}/)
  assert.match(source, /aria-expanded=\{window\.expanded\}/)
  assert.match(helper, /Show fewer receipts/)
  assert.match(helper, /Show all \$\{total\} receipts/)
  assert.match(source, /onSelect\(receipt\.node\)/)
  assert.match(source, /event\.currentTarget\.focus\(\)/)
  assert.match(css, /\.wm-cited-receipts[^}]*min-width: 0[^}]*minmax\(min\(100%, 152px\), 1fr\)/)
  assert.match(css, /\.wm-positioned-receipt[^}]*white-space: normal[^}]*overflow-wrap: anywhere/)
})
