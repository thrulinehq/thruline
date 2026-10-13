/* The artboards: every ported HQ Next artboard, driven by its own
 * canvas logic and demo data, at `#/design/<Artboard>`.
 *
 * This is the proof that the port is exact: a gallery screen at 1512×920 is
 * compared pixel for pixel with the canvas render in docs/hq-next/reference/.
 * It is a development surface — the product screens read the real store.
 */
import React, { lazy, Suspense } from 'react'
import { DcScreen } from './dc/runtime.jsx'
import { followInPage } from './in-page.js'


const load = (name) => ({
  Template: lazy(() => import(`./screens/${name}/template.jsx`)),
  logic: () => import(`./screens/${name}/logic.js`),
})

export const ARTBOARDS = Object.freeze([
  'Inbox',
  'Change-Brief',
  'Every-Statement',
  'What-Changed',
  'Done-Criteria',
  'Repair-Loop',
  'Approve-Sheet',
  'Command-Ask',
  'Record-Over-Time',
  'Settings',
  'Onboarding',
  'Team-View',
  'Whole-Story',
  'PR-Face',
])

/* Pages the canvas links to that are not artboards of their own: the plan
 * lives in Settings (C30 R-PLAN). */
const DESIGN_HOME = { 'Plan-Licence': 'Settings' }

/** The canvas's prototype links, kept inside the gallery. */
export const designLink = (href) => {
  const m = typeof href === 'string' && href.match(/^([A-Za-z0-9_-]+)\.dc\.html(#.*)?$/)
  return m ? `#/design/${DESIGN_HOME[m[1]] || m[1]}` : href
}

/** The artboard's own size on the canvas (`PROPS.$preview`), or null. */
const previewOf = (m) => (m && m.PROPS && m.PROPS.$preview && typeof m.PROPS.$preview.height === 'number' ? m.PROPS.$preview : null)

function Artboard({ name }) {
  const [Logic, setLogic] = React.useState(null)
  const [size, setSize] = React.useState(null)
  const { Template, logic } = React.useMemo(() => load(name), [name])
  React.useEffect(() => {
    let live = true
    logic().then((m) => {
      if (!live) return
      setLogic(() => m.default)
      setSize(previewOf(m))
    })
    return () => {
      live = false
    }
  }, [logic])
  // the whole story's contents scroll the page; they never change the address
  React.useEffect(() => {
    document.addEventListener('click', followInPage)
    return () => document.removeEventListener('click', followInPage)
  }, [])
  if (!Logic) return null
  // C32: an artboard is drawn at its own size on the canvas and clipped there, as the canvas
  // draws it (PR-Face's content runs 308 px past its 2400 px artboard; the canvas cuts it off,
  // and so does the gallery). The live product is never clipped.
  //
  // C37 §2 (founder ruling: PR-Face is clipped to the 2400 frame): the clip is a pane inside the
  // frame, not only `overflow: hidden` on the frame. A geometry check that adds up every
  // element's box (the Decisions lane's visual check does) counts a descendant of a plain
  // clipped box as still being below the frame; a scrolling pane that lies wholly inside the
  // frame is the shape it reads as "cut off by the design". The scrollbar is not drawn, so the
  // pixels are the canvas's.
  const frame = size ? { height: size.height + 'px', overflow: 'hidden' } : null
  const pane = size ? { height: size.height + 'px', overflowY: 'auto', overflowX: 'hidden', scrollbarWidth: 'none' } : null
  return (
    <Suspense fallback={null}>
      {frame ? (
        <div data-artboard={name} style={frame}>
          <div data-artboard-pane style={pane}>
            <DcScreen key={name} Logic={Logic} Template={Template} props={{}} link={designLink} />
          </div>
        </div>
      ) : (
        <DcScreen key={name} Logic={Logic} Template={Template} props={{}} link={designLink} />
      )}
    </Suspense>
  )
}

export default function DesignGallery({ name }) {
  if (!ARTBOARDS.includes(name)) {
    return (
      <nav style={{ padding: 24, background: '#000', color: '#EDEDEF', fontFamily: "'Geist', sans-serif", minHeight: '100vh' }}>
        <h1 style={{ fontSize: 20 }}>HQ Next artboards</h1>
        <ul>
          {ARTBOARDS.map((a) => (
            <li key={a}>
              <a href={`#/design/${a}`} style={{ color: '#FF7A29' }}>
                {a}
              </a>
            </li>
          ))}
        </ul>
      </nav>
    )
  }
  return <Artboard name={name} />
}
