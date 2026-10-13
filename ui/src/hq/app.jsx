/* HQ Next — the app the founder designed, running on the real store.
 *
 * One router (routes.js), one reader (data/api.js), and one screen per canvas
 * artboard under screens/<Artboard>/: `template.jsx` (the design's markup,
 * ported), `logic.js` (the design's logic, reading real data through
 * `this.props.data`), and `index.jsx` (what to read for this address).
 * docs/hq-next/PLAN.md is the build; the canvas is the spec.
 */
import React, { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { parse, href, nextRoute, SCREENS } from './routes.js'
import { followInPage } from './in-page.js'
import { FrameContext, ScreenBoundary } from './frame-state.jsx'
import { useKeys } from './keys/use-keys.jsx'
import { useAppearance } from './appearance.jsx'
import { PrefsNotice } from './prefs-notice.jsx'
import { NotificationCenter } from './notification-center.jsx'

const SCREEN_MODULES = {
  Inbox: lazy(() => import('./screens/Inbox/index.jsx')),
  'Change-Brief': lazy(() => import('./screens/Change-Brief/index.jsx')),
  'Every-Statement': lazy(() => import('./screens/Every-Statement/index.jsx')),
  'What-Changed': lazy(() => import('./screens/What-Changed/index.jsx')),
  'Done-Criteria': lazy(() => import('./screens/Done-Criteria/index.jsx')),
  'Repair-Loop': lazy(() => import('./screens/Repair-Loop/index.jsx')),
  'Approve-Sheet': lazy(() => import('./screens/Approve-Sheet/index.jsx')),
  'Whole-Story': lazy(() => import('./screens/Whole-Story/index.jsx')),
  'PR-Face': lazy(() => import('./screens/PR-Face/index.jsx')),
  'Step-View': lazy(() => import('./screens/Step-View/index.jsx')),
  'Record-Over-Time': lazy(() => import('./screens/Record-Over-Time/index.jsx')),
  'Record-Statements': lazy(() => import('./screens/Record-Statements/index.jsx')),
  Settings: lazy(() => import('./screens/Settings/index.jsx')),
  'Team-View': lazy(() => import('./screens/Team-View/index.jsx')),
  Onboarding: lazy(() => import('./screens/Onboarding/index.jsx')),
  // C30 R-NAV: the two home screens with no artboard of their own
  Changes: lazy(() => import('./screens/Changes/index.jsx')),
  Projects: lazy(() => import('./screens/Projects/index.jsx')),
}

const Command = lazy(() => import('./screens/Command-Ask/index.jsx'))

/** The address in the bar, as a route. A hash that is not an address
 *  (`#ws-…`, a place on the page) leaves the route where it was (C30 R-LINKS). */
function useRoute() {
  const [at, setAt] = useState(() => nextRoute(parse(''), window.location.hash))
  useEffect(() => {
    const on = () => setAt((prev) => nextRoute(prev, window.location.hash))
    window.addEventListener('hashchange', on)
    // an in-page link scrolls to its place and never changes the address
    document.addEventListener('click', followInPage)
    return () => {
      window.removeEventListener('hashchange', on)
      document.removeEventListener('click', followInPage)
    }
  }, [])
  return at
}

export default function App() {
  const at = useRoute()
  const [commandOpen, setCommandOpen] = useState(false)
  // every key, ⌘K included, from the one key table (keys/table.js); the `?` map is `keys.map`
  const keys = useKeys({ at, palette: commandOpen, setPalette: setCommandOpen })
  // density, motion and colour vision, from the person's stored choices (C30 R-PREFS), on the app root
  useAppearance()
  // an address carrying `find=1` is its screen with Find, ask, act open over
  // it: the palette opens, and the address goes back to naming the screen
  useEffect(() => {
    if (!at.find) return
    setCommandOpen(true)
    window.location.replace(href(at.screen, { ...at, find: undefined }))
  }, [at.find])
  const Screen = SCREEN_MODULES[SCREENS[at.screen]] || SCREEN_MODULES.Inbox
  // C30 R-NAV: a read that is out, a read that failed, or a screen that throws is said
  // INSIDE the frame (frame-state.jsx) — the person's place is given to it here
  const frame = useMemo(() => ({ at, openCommand: () => setCommandOpen(true) }), [at])
  return (
    <FrameContext.Provider value={frame}>
      <Suspense fallback={<div style={{ minHeight: '100vh', background: '#000000' }} />}>
        <ScreenBoundary resetKey={JSON.stringify(at)}>
          <Screen at={at} openCommand={() => setCommandOpen(true)} keysMode={keys.mode} />
        </ScreenBoundary>
        {commandOpen ? <Command at={at} askFirst={commandOpen === 'ask'} keysMode={keys.mode} onClose={() => setCommandOpen(false)} /> : null}
        {keys.map}
        <PrefsNotice at={at} />
        <NotificationCenter at={at} />
      </Suspense>
    </FrameContext.Provider>
  )
}
