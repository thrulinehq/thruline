export const REFRESH_ENDPOINTS = Object.freeze({
  state: '/api/state',
  start: '/api/refresh',
})

export const REFRESH_RECOVERY =
  'The browser cannot cancel the worker by itself. Press Ctrl+C in the terminal to stop the foreground app and its refresh. If the app stops or fails, this page keeps the reading already loaded. Restart thruline ui before trying again.'

const BUSY_MESSAGE =
  'Reading already loaded stays visible, and navigation remains usable. Terminal output names progress. This page reloads only after refresh finishes successfully.'

const RETRY_MESSAGE =
  'Reading already loaded remains available. Terminal output has the failure detail.'

export function refreshPresentation(server, phase = 'idle') {
  if (!server) {
    return {
      visible: false,
      phase: 'unavailable',
      title: '',
      message: '',
      actionLabel: '',
      busy: false,
      error: false,
      reloadEligible: false,
    }
  }

  const resolved = phase === 'idle' && server.last_refresh?.ok === false
    ? 'prior_failure'
    : phase

  const states = {
    idle: {
      title: 'Reading on disk',
      message: 'Refresh reads the projects again from this device.',
      actionLabel: 'Refresh reading',
    },
    starting: {
      title: 'Refresh is starting',
      message: BUSY_MESSAGE,
      actionLabel: 'Starting refresh…',
      busy: true,
    },
    busy: {
      title: 'Refresh is running',
      message: BUSY_MESSAGE,
      actionLabel: 'Refreshing…',
      busy: true,
    },
    already_running: {
      title: 'Refresh already running',
      message: BUSY_MESSAGE,
      actionLabel: 'Refreshing…',
      busy: true,
    },
    prior_failure: {
      title: 'Last refresh failed',
      message: RETRY_MESSAGE,
      actionLabel: 'Try refresh again',
      error: true,
    },
    start_failure: {
      title: 'Refresh could not start',
      message: RETRY_MESSAGE,
      actionLabel: 'Try refresh again',
      error: true,
    },
    connection_failure: {
      title: 'Local app could not be reached',
      message: 'Reading already loaded remains available. Restart thruline ui before trying again.',
      actionLabel: 'Try refresh again',
      error: true,
    },
    refresh_failure: {
      title: 'Refresh failed',
      message: RETRY_MESSAGE,
      actionLabel: 'Try refresh again',
      error: true,
    },
    complete: {
      title: 'Refresh finished',
      message: 'The refreshed reading is ready to load.',
      actionLabel: 'Reloading reading…',
      busy: true,
      reloadEligible: true,
    },
  }
  const selected = states[resolved] || states.connection_failure
  return {
    visible: true,
    phase: states[resolved] ? resolved : 'connection_failure',
    title: selected.title,
    message: selected.message,
    actionLabel: selected.actionLabel,
    busy: selected.busy === true,
    error: selected.error === true,
    reloadEligible: selected.reloadEligible === true,
  }
}
