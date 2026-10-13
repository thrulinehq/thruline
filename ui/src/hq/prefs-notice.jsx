/* A preference the server did not keep, said on every screen but Settings (C30
 * R-PREFS). The top-bar Plain / Technical button, the `t` key and the Inbox's
 * Density buttons have no line of their own to say it on, so a write that was
 * refused (a newer file, a file that is not an ordinary file, a bad value, no
 * answer) is said here, in words, with what stands. Settings says it beside the
 * control instead (screens/Settings/appearance.js).
 *
 * It draws nothing when nothing was refused, and it is dismissed by the person
 * or by the next write of the same choice. */
import React from 'react'
import { preferences, usePreferences, valueOf } from './data/preferences.js'
import { refusedLine } from './data/preference-words.js'

export function PrefsNotice({ at, store = preferences }) {
  const state = usePreferences(store)
  if (at && at.screen === 'settings') return null
  const entries = Object.entries(state.writeError || {})
  if (!entries.length) return null
  const [name, refusal] = entries[entries.length - 1]
  return (
    <div
      role="alert"
      data-prefs-notice={name}
      style={{ position: 'fixed', left: '50%', bottom: '40px', transform: 'translateX(-50%)', zIndex: 60, maxWidth: '640px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '12px', padding: '8px 8px 8px 14px', background: '#111113', border: '1px solid #66666F', borderRadius: '8px', color: '#EDEDEF', fontFamily: "'Geist', -apple-system, 'Helvetica Neue', sans-serif", fontSize: '13px', lineHeight: '19px' }}
    >
      <span>{refusedLine(name, refusal, valueOf(state.prefs, name))}</span>
      <button
        type="button"
        className="tl-ghost"
        onClick={() => store.dismiss(name)}
        style={{ flex: '0 0 auto', height: '26px', padding: '0 10px', background: 'transparent', border: '1px solid #66666F', borderRadius: '6px', color: '#A1A1AA', font: 'inherit', fontSize: '12.5px', cursor: 'pointer' }}
      >
        Dismiss
      </button>
    </div>
  )
}
