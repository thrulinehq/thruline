/* Deterministic Markdown-ish record text → readable plain text.
 *
 * The source record is never changed. This is a presentation preview only:
 * React renders the returned string as text, and Inspector keeps the untouched
 * receipt one disclosure deeper. No URL becomes a live link here. */

const FENCE = /^\s{0,3}(`{3,}|~{3,})(?:\s*[^`]*)?$/
const TABLE_SEPARATOR = /^:?-{3,}:?$/

function inlinePlain(text) {
  let out = String(text)
  /* One balanced parenthesis level covers ordinary URLs and deliberately
     hostile `javascript:alert(1)` targets. Only the inert label survives. */
  out = out.replace(/!?\[([^\]]*)\]\((?:[^()\\]|\\.|\([^()]*\))*\)/g, '$1')
  /* A source excerpt can end while its Markdown target is still open. The
     terminal export ellipsis proves truncation; keep only the exported label,
     just as for a complete link, and never reconstruct or activate a target. */
  out = out.replace(/!?\[([^\]\n]*)\]\([^\n]*(?:…|\.{3})$/g, '$1')
  out = out.replace(/\[([^\]]+)\]\[[^\]]*\]/g, '$1')
  out = out.replace(/<((?:https?:\/\/)[^>]+)>/g, '$1')
  out = out.replace(/`([^`\n]+)`/g, '$1')
  out = out.replace(/(\*\*|__)(\S(?:.*?\S)?)\1/g, '$2')
  out = out.replace(/~~(\S(?:.*?\S)?)~~/g, '$1')
  out = out.replace(/(^|[\s([{"'])\*([^*\n]+)\*(?=$|[\s)\]}.,!?;:])/g, '$1$2')
  out = out.replace(/(^|[\s([{"'])_([^_\n]+)_(?=$|[\s)\]}.,!?;:])/g, '$1$2')
  return out
}

function tableCells(line) {
  const trimmed = line.trim()
  if (!trimmed.startsWith('|') || !trimmed.endsWith('|')) return null
  return trimmed
    .slice(1, -1)
    .split('|')
    .map((cell) => cell.trim())
}

/** Return readable plain text while preserving the record's content bytes elsewhere. */
export function normalizeRecordText(raw) {
  if (raw == null) return ''
  const source = String(raw).replace(/\r\n?/g, '\n')
  const lines = []
  let fence = null

  for (const original of source.split('\n')) {
    const marker = original.match(FENCE)
    if (marker) {
      if (!fence) fence = marker[1][0]
      else if (marker[1][0] === fence) fence = null
      continue
    }
    if (fence) {
      lines.push(original)
      continue
    }

    let line = original.trimEnd()
    const cells = tableCells(line)
    if (cells) {
      if (cells.length > 0 && cells.every((cell) => TABLE_SEPARATOR.test(cell))) continue
      line = cells.map(inlinePlain).filter(Boolean).join(' · ')
      lines.push(line)
      continue
    }

    if (/^\s{0,3}(?:=+|-+)\s*$/.test(line)) continue
    if (/^\s*\[[^\]]+\]:\s*\S+/.test(line)) continue
    line = line.replace(/^\s{0,3}#{1,6}\s+/, '')
    line = line.replace(/^\s{0,3}>\s?/, '')
    line = line.replace(/^\s*[-*+]\s+/, '• ')
    line = line.replace(/^\s*(\d+)[.)]\s+/, '$1. ')
    lines.push(inlinePlain(line))
  }

  while (lines.length > 0 && lines[0].trim() === '') lines.shift()
  while (lines.length > 0 && lines.at(-1).trim() === '') lines.pop()

  const compact = []
  let blanks = 0
  for (const line of lines) {
    if (line.trim() === '') {
      blanks++
      if (blanks <= 1) compact.push('')
    } else {
      blanks = 0
      compact.push(line)
    }
  }
  return compact.join('\n')
}

/** A bounded preview of bytes already present; `fullText` never adds source bytes. */
export function recordTextPreview(raw, limit = 420) {
  const fullText = normalizeRecordText(raw)
  const points = [...fullText]
  if (points.length <= limit) return { text: fullText, fullText, truncated: false }
  const before = points.slice(0, limit + 1)
  let breakAt = -1
  for (let index = 0; index < before.length; index++) {
    if (before[index] === '\n' || before[index] === ' ') breakAt = index
  }
  const cut = breakAt >= Math.floor(limit * 0.6) ? breakAt : limit
  return {
    text: `${points.slice(0, cut).join('').trimEnd()}…`,
    fullText,
    truncated: true,
  }
}

/** The export deliberately ends long opening asks with an ellipsis. */
export const isExportExcerpt = (raw) => /(?:…|\.\.\.)\s*$/.test(String(raw || ''))
