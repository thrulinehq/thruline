/* T10 — when THIS screen last finished reading, in words a person can check.
 *
 * PURE. The clock value and the zone are arguments, so the same input gives
 * the same text on every machine and in every test. The visible text is local
 * `HH:MM`; the hover title carries the date and the UTC offset
 * (`2026-09-26 16:04 +09:00`), per the report's time rule: HQ shows local
 * HH:MM with date and zone on hover.
 *
 * `ms` is the epoch time a read COMPLETED. Nothing here reads a clock: a
 * screen that has not finished a read passes null and gets null back, and the
 * top bar then says "read time not shown" instead of a guess.
 */

/** The zone this browser is in, or UTC when it cannot say. */
export function localZone() {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
  } catch {
    return 'UTC'
  }
}

/**
 * @param {number|null|undefined} ms epoch milliseconds of a completed read
 * @param {string} [tz] an IANA zone; the browser's own when omitted
 * @returns {{text: string, title: string}|null}
 */
export function readStamp(ms, tz = localZone()) {
  if (typeof ms !== 'number' || !Number.isFinite(ms)) return null
  let parts
  try {
    parts = new Intl.DateTimeFormat('en-GB', {
      timeZone: tz,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hourCycle: 'h23',
      timeZoneName: 'longOffset',
    }).formatToParts(new Date(ms))
  } catch {
    return null
  }
  const at = (type) => (parts.find((p) => p.type === type) || {}).value || ''
  const time = at('hour') + ':' + at('minute')
  /* `longOffset` is "GMT+09:00", "GMT-04:00", or bare "GMT" at zero. */
  const zone = at('timeZoneName').replace(/^GMT/, '') || '+00:00'
  return { text: time, title: `${at('year')}-${at('month')}-${at('day')} ${time} ${zone}` }
}
