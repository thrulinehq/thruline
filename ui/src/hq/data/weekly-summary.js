/* The weekly card is a local, read-time projection. The calendar week is the
 * person's local Monday-to-Sunday week; counts themselves cover the route's
 * rolling last seven days. No record is written. */
export const WEEKLY_SHOWN_KEY = 'thruline.hq.weekly-shown.v1'

export function localWeekKey(now = new Date()) {
  const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  monday.setDate(monday.getDate() - (monday.getDay() + 6) % 7)
  return [monday.getFullYear(), String(monday.getMonth() + 1).padStart(2, '0'), String(monday.getDate()).padStart(2, '0')].join('-')
}

export function validSummary(summary) {
  return !!summary && summary.schema_version === 1 &&
    typeof summary.since === 'string' && Number.isFinite(Date.parse(summary.since)) &&
    typeof summary.through === 'string' && Number.isFinite(Date.parse(summary.through)) &&
    ['approvals', 'not_backed', 'rechecks', 'projects_recorded'].every((key) => Number.isSafeInteger(summary[key]) && summary[key] >= 0)
}

export function summaryMarkdown(summary) {
  if (!validSummary(summary)) return null
  return `# ThruLine weekly summary\n\nLast seven days: ${summary.since} through ${summary.through}\n\n` +
    `- Changes approved: ${summary.approvals}\n` +
    `- Not backed findings: ${summary.not_backed}\n` +
    `- Re-checks run: ${summary.rechecks}\n` +
    `- Projects recorded: ${summary.projects_recorded}\n`
}
