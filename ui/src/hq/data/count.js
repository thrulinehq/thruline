/* The one way HQ writes a count (C29): thousands separated by a comma, on
 * every count of 1,000 or more — "5,646", "76,746 sessions", "1,679 steps".
 *
 * Deterministic and locale-independent: it never asks the machine's locale
 * (no toLocaleString, no Intl), so the same record reads the same on every
 * machine and in every test.
 *
 * Only COUNTS go through it. Identifiers and positions keep their digits
 * bare: a step number ("step 1666"), a version, a commit id, a port, a year,
 * a time, a line number. Those are names, not amounts.
 *
 * Anything that is not a finite number is returned as `String(x)`, exactly
 * what `String(x)` or `'' + x` gave before, so swapping it in for either never
 * changes what a missing value reads as.
 */

/**
 * @param {unknown} x a count
 * @returns {string} "5,646"; "999"; "-1,204"; "1,234.5"; String(x) for a non-number
 */
export function fmtCount(x) {
  if (typeof x !== 'number' || !Number.isFinite(x)) return String(x)
  const s = String(Math.abs(x))
  if (/e/i.test(s)) return String(x) // beyond 1e21: no grouping is honest about its digits
  const dot = s.indexOf('.')
  const whole = dot === -1 ? s : s.slice(0, dot)
  const frac = dot === -1 ? '' : s.slice(dot)
  let grouped = ''
  for (let i = 0; i < whole.length; i++) {
    if (i > 0 && (whole.length - i) % 3 === 0) grouped += ','
    grouped += whole[i]
  }
  return (x < 0 ? '-' : '') + grouped + frac
}

/** "1 session" / "76,746 sessions": a count and its noun, the count formatted. */
export function countOf(n, one, many) {
  return fmtCount(n) + ' ' + (n === 1 ? one : many)
}
