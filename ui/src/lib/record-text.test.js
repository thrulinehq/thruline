import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import { isExportExcerpt, normalizeRecordText, recordTextPreview } from './record-text.js'

test('headings, emphasis, lists, and table separators become readable plain text', () => {
  const raw = [
    '# **Backend findings**',
    '',
    '- *First* item',
    '2) __Second__ item',
    '',
    '| File | State |',
    '| --- | :---: |',
    '| `app.ts` | **changed** |',
  ].join('\n')
  assert.equal(
    normalizeRecordText(raw),
    ['Backend findings', '', '• First item', '2. Second item', '', 'File · State', 'app.ts · changed'].join('\n'),
  )
})

test('fence markers disappear while code content and indentation survive', () => {
  const raw = ['```js', '  const snake_case = `literal`', '  console.log(snake_case)', '```'].join('\n')
  assert.equal(normalizeRecordText(raw), ['  const snake_case = `literal`', '  console.log(snake_case)'].join('\n'))
})

test('http and javascript Markdown links both become inert label text', () => {
  const raw = '[Docs](https://example.test/a) and [Run](javascript:alert(1)) plus ![diagram](https://example.test/x.png)'
  const text = normalizeRecordText(raw)
  assert.equal(text, 'Docs and Run plus diagram')
  assert.doesNotMatch(text, /javascript:|https:\/\//)
})

test('a source excerpt truncated inside a safe Markdown target becomes inert label text', () => {
  const raw = 'Inspect [the function](supabase/functions/inspect-record…'
  const text = normalizeRecordText(raw)
  assert.equal(text, 'Inspect the function')
  assert.doesNotMatch(text, /\]\(|supabase\/functions/)
  assert.equal(isExportExcerpt(raw), true)
})

test('a source excerpt truncated inside a javascript target never activates or exposes it', () => {
  const raw = 'Choose [Run](javascript:alert(…'
  const text = normalizeRecordText(raw)
  assert.equal(text, 'Choose Run')
  assert.doesNotMatch(text, /javascript:|\]\(/)
  assert.equal(isExportExcerpt(raw), true)
})

test('a dangling target with Unicode label and source ellipsis remains code-point safe', () => {
  const raw = 'Open [功能😀](docs/功能😀…'
  const preview = recordTextPreview(raw, 8)
  assert.equal(preview.fullText, 'Open 功能😀')
  assert.ok([...preview.fullText].every((point) => !(point.length === 1 && /[\uD800-\uDFFF]/.test(point))))
  assert.equal(isExportExcerpt(raw), true)
  assert.equal(raw.endsWith('…'), true, 'the source ellipsis remains in the untouched input')
})

test('ordinary content, bare URLs, and identifier underscores remain content', () => {
  const raw = 'Use snake_case at https://example.test/raw and keep a <script>tag</script> literal.'
  assert.equal(normalizeRecordText(raw), raw)
})

test('preview expansion contains only normalized bytes already supplied', () => {
  const raw = `# ${'word '.repeat(100).trim()}`
  const preview = recordTextPreview(raw, 80)
  assert.equal(preview.truncated, true)
  assert.ok(preview.text.endsWith('…'))
  assert.equal(preview.fullText, normalizeRecordText(raw))
  assert.ok(preview.fullText.length > preview.text.length)
})

test('bounded previews never split a Unicode code point', () => {
  const preview = recordTextPreview('abcd😀efgh', 5)
  assert.equal(preview.text, 'abcd😀…')
  assert.ok([...preview.text].every((point) => !(point.length === 1 && /[\uD800-\uDFFF]/.test(point))))
  assert.equal(preview.fullText, 'abcd😀efgh')
})

test('export excerpts are named from their actual terminal ellipsis', () => {
  assert.equal(isExportExcerpt('exported bytes…'), true)
  assert.equal(isExportExcerpt('exported bytes...  '), true)
  assert.equal(isExportExcerpt('complete exported bytes'), false)
})

test('new React surfaces render strings only and contain no HTML injection escape hatch', () => {
  for (const relative of ['../mac/decision-flow.jsx', '../mac/work-map.jsx']) {
    const source = readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8')
    assert.ok(!source.includes('dangerouslySetInnerHTML'), `${relative} enables HTML injection`)
    assert.ok(!source.includes('innerHTML'), `${relative} writes HTML directly`)
  }
})
