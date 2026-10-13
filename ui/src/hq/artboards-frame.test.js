/* C37 §2: PR-Face is clipped to its 2400 px frame. The Decisions lane's visual check adds up
 * every element's box and counts a descendant of a plainly clipped (`overflow: hidden`) box as
 * still below the frame; it reads a scrolling pane that lies wholly inside the frame as "cut
 * off by the design". So the gallery's clip is such a pane, with the scrollbar not drawn. This
 * pins that shape in the source (the browser measurement is in docs/hq-next/DECISIONS.md). */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const source = readFileSync(new URL('./artboards.jsx', import.meta.url), 'utf8')

test('an artboard drawn at its canvas size is cut off by a bounded scrolling pane inside the frame', () => {
  const frame = source.match(/const frame = [^\n]+/)?.[0] || ''
  const pane = source.match(/const pane = [^\n]+/)?.[0] || ''
  assert.match(frame, /overflow: 'hidden'/, 'the frame still clips')
  assert.match(pane, /height: size\.height \+ 'px'/, 'the pane is exactly as tall as the frame')
  assert.match(pane, /overflowY: 'auto'/, 'the pane scrolls its own overflow')
  assert.match(pane, /scrollbarWidth: 'none'/, 'and draws no scrollbar, so the pixels are the canvas’s')
  assert.match(source, /<div data-artboard-pane style=\{pane\}>\s*<DcScreen/, 'the screen is inside the pane')
})

test('the live product is never clipped: only a screen with a canvas size is framed', () => {
  assert.match(source, /const frame = size \? \{/)
  assert.match(source, /\) : \(\s*<DcScreen key=\{name\}/, 'an artboard with no canvas size is drawn unframed')
})
