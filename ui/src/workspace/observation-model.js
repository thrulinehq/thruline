export function observationView(data, error = '') {
  if (error) return { visible: true, label: '○ HQ disconnected', summary: 'Observation status is unavailable. Restart HQ or wait for reconnection.' }
  if (data === null) return { visible: true, label: '◌ Reading observation status', summary: 'Checking this backend’s configured source.' }
  if (!data?.enabled) return { visible: true, label: '○ Observation not configured', summary: 'Verification receipts do not establish live capture.' }
  const labels = { starting: '◌ Connecting to Codex logs', reading: '● Reading new recorded messages', watching: '● Watching Codex logs', paused: 'Ⅱ Observation paused', stopped: '■ Observation stopped', disconnected: '○ Observation disconnected' }
  return { visible: true, label: labels[data.state] || '○ Observation unknown', summary: data.state === 'watching' ? 'Automatic local log observation · agent reports, not execution proof' : data.message }
}
