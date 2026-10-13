/* A browser permission is asked only on an explicit switch-on gesture. The
 * preference stays Off unless the browser grants it. No prompt belongs to an
 * event, and an existing denial is never prompted again. */
export async function allowBrowserNotifications(NotificationClass = typeof Notification === 'undefined' ? null : Notification) {
  if (!NotificationClass || typeof NotificationClass.requestPermission !== 'function') {
    return { allowed: false, reason: 'This browser cannot show notifications from the HQ page. The switch is still Off.' }
  }
  let permission = NotificationClass.permission
  if (permission === 'default') {
    try { permission = await NotificationClass.requestPermission() }
    catch { permission = 'denied' }
  }
  if (permission !== 'granted') return { allowed: false, reason: 'Browser notification permission was not granted. The switch is still Off.' }
  return { allowed: true, reason: null }
}
