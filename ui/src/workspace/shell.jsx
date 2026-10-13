import React from 'react'
import { ObservationBar } from './observation.jsx'
import { localVerificationSession, readLocalVerification } from './local-verification-model.js'
import { countPhrase } from '../lib/voice.js'
import { A as ALPHA, BP, C, CLAMP, FONT, FS, GLOW, SIZE, sx } from './tokens.js'
import {
  FACETS,
  FAM,
  GLOSS,
  KIND_LABEL,
  LAYOUT,
  NO_CLAUSE,
  RAIL,
  WIRE,
  bez,
  objType,
  pinStyle,
  severityOf,
  short,
  wordOf,
} from './logic.js'
import { LAYOUT_METRICS, adaptSession, attrs, facetOptions, matches as matchesNode, parseToken } from './adapter.js'
import { placementSeq } from './lod.js'
import { describedAs, outcomeSentence } from './readable.js'
import { API, DESTINATION_SOURCE, readProjection } from './data.js'
import { goToInbox } from '../routes.js'
import { DEMO_LABEL, DEMO_META, DEMO_ORDER, demoSession } from './demo-data.js'
import { AddressNotice, Refusal, Waiting } from './ui-kit.jsx'
import {
  DEEP_LINK_ABSENT,
  READING_LINE,
  SESSIONS_LEAD,
  SESSIONS_LEAD_REFUSED,
  anyReadingHolds,
  centreState,
  chooseListedSession,
  chooseSession,
  connectionLine,
  filterListing,
  listingFromDocument,
  listingHasSession,
  projectListing,
  sessionAbsence,
  sessionIdFromSearch,
  statusFor,
} from './state.js'
import {
  AppBar,
  DemoBanner,
  DocTabs,
  FilterBar,
  Menu,
  Palette,
  Rail,
  RailTooltip,
  SentenceStrip,
  StatusStrip,
  Toast,
  Toolbar,
} from './chrome.jsx'
import {
  ClaimDocument,
  ComparePane,
  DestinationsDrawer,
  GraphCanvas,
  RecordsTable,
  Timeline,
} from './canvas.jsx'
import { BottomDock, Inspector, LeftDock, Splitter } from './docks.jsx'
import {
  FIT_MIN_Z,
  MM_MIN_VIEWPORT,
  boxesIntersect,
  countOnScreen,
  fitView,
  minimapCaption,
  minimapCells,
  minimapLayout,
  minimapMark,
  minimapNote,
  nodeInWindow,
  windowBox,
  wireBox,
} from './viewport.js'
import { HqWorkspace, Connections } from './hq.jsx'
import { ReadRail } from './read-rail.jsx'
import { readStamp } from './read-stamp.js'
/* The door to the Change workspace. It sits above everything else on the
 * landing because a person who opened ThruLine after an agent said "done"
 * came for exactly one thing: the work they are being asked to accept
 * (T-2-4 fix round 1; before it, nothing on this page led to a Change). */
import ChangeWayInLive from './change-way-in.jsx'
import { actionLabel, humanPath } from './plain-language.js'
import { journeyLayout } from './journey-layout.js'
import {
  EDGE_STYLE, cardHeight, checkTitle, discrepancyAnchor, edgeKindOf, factsLines, isAgentWords, laneLayout, legendEntries,
  laneCounts, missingLines, outlineRows, outlineTree, plainFace, plainText, runFacts, stepLabel, threadOf, threadSteps, walkOrder, walkStep,
} from './trace-view.js'
import { TraceOutline } from './trace-outline.jsx'
import { fill } from './hq-narrate.js'
import { chainOf, focusProjection, isFocusGroupId } from './focus-set.js'
/* P6-1 — the altitudes (design record §3.1, §6): the shared top bar and
 * sidebar, and the remembered choice. */
import { ShellSidebar, ShellTopBar } from './shell-chrome.jsx'
import { initialAltitude, rememberAltitude, singleKeysOn } from './altitude.js'
import { HQ_COPY } from './hq-copy.js'
import { escapeMarkdown } from './packet-export.js'
import './workspace.css'

/* The two layouts that flow left to right in record order and ignore an
 * authored column layout: the four lanes (P6-3) and the chapters. Anything
 * else — `vertical`, or no direction at all — is the column layout. */
const FLOWING = Object.freeze({ lanes: true, horizontal: true })

/* ThruLine workspace — the root.
 *
 * `main.jsx` mounts exactly this, in place of `<OsShell index={index}/>`, and
 * changes nothing else: the `/acceptance` route, the `?cast` route, the gallery
 * branch, the single `fetch(INDEX`, its five validations, `fail()`, `Boundary`
 * and `StrictMode` all stay byte-identical. `main.jsx` must keep exactly ONE
 * `fetch(` and it must be `fetch(INDEX` (pinned, `lib/arcade.test.js:228-229`),
 * so this component receives the index as a PROP and performs no boot fetch.
 *
 * `os/shell.jsx` IS NOT EDITED. It asserts exactly two `fetch(` calls
 * (`lib/arcade.test.js:243-246`) and fifteen further structural pins
 * (spec-target §D.2); a fifteen-destination rail needs more reads than two, and
 * loosening that assertion deletes the load-discipline guarantee it exists to
 * hold. So `os/shell.jsx` becomes ONE destination of the rail — the one that
 * owns Inbox, Mission Control, its own Trace tab and the Audit Ledger — and
 * every one of its pins holds by construction with no edit to any `.test.js`.
 *
 * WHY A CLASS. The design's own component is a class with 49 state fields, four
 * window listeners, two refs, a mutable drag descriptor and eight timers
 * (spec-script §A, §C). `DCLogic.setState` is React `setState` semantics
 * exactly (spec-script §E2), so the class port is a near-verbatim translation
 * and the hook port is a rewrite. Rule 2: the least machinery that meets the
 * milestone. React 18 renders classes; `StrictMode` double-invokes the
 * constructor and render, which is safe once the render-phase writes are gone
 * (see `palRaw()` / `findMatches()` below).
 *
 * WHAT IS NOT VERBATIM — the five patterns from the original that must change,
 * because they are broken rather than merely foreign (spec-script §E6):
 *   1. `claimDocInspector` mutates `this.state.sel` directly. Parameterised:
 *      `inspector(selId = this.state.sel)`, called as `this.inspector('clm')`.
 *   2. `this._palRaw` / `this._findResults` are written DURING render and read
 *      by keyboard handlers. In StrictMode the value a handler reads may come
 *      from a discarded render. Both are memoised on `(session, query)` in
 *      `palRaw()` / `findMatches()` and recomputed by the handler.
 *   3. `window.THRULINE_DEMO` / `window.THRULINE_LOGIC` globals are imports;
 *      `THRULINE_LOGIC` had NO fallback and threw on first use.
 *   4. `openMenu` measured against `closest('[data-screen-label]')`, a
 *      Claude-Design editor attribute that does not survive. Measured against
 *      `this.rootRef.current.getBoundingClientRect()`.
 *   5. `componentWillUnmount` never cleared `toastTimer` or the two mount
 *      timeouts. Every timer is cleared here.
 * And one more that is a product hole, not a port hazard: `refresh()` read
 * nothing — it was an 1100ms timeout that logged "nothing changed". Here it
 * calls the server through `data.js` and reports what actually changed,
 * INCLUDING a failure (spec-script §G.4).
 *
 * STATE AND CALLBACKS FLOW ONE WAY: this component holds all of it, builds one
 * plain view-model per render with `buildVals()`, and passes named slices of it
 * down as props. There is no context, no reducer store, no event bus, no new
 * dependency — `ui/package.json` stays `react` + `react-dom` only.
 *
 * `fetch` DOES NOT APPEAR IN THIS FILE. Every read goes through `./data.js`
 * (CONTRACT §6), which is the one auditable place the eight read-discipline
 * rules live.
 */

/** The one persisted key, six fields (CONTRACT §4). */
const PREFS_KEY = 'thruline.trace.prefs.v1'

/** Below this viewport width the docks float over the canvas — `BP.sheet`,
 *  the §6 breakpoint at which the inspector stops sitting beside the canvas.
 *  (It floats at the side today; turning it into a bottom sheet is P6-1.) */
const NARROW = BP.sheet

/**
 * The bundled-demo fallback (spec-target Decision 3, CONTRACT §8 target-3).
 *
 * The operator's ruling is that the bundled corpus does NOT ship, and that the
 * reverse is a one-import change. This constant IS that switch, and it is
 * inert on its own: the fallback engages only when `demoSession(key)` actually
 * returns a session, and when it does the permanent banner is mounted with it —
 * `isDemo` is derived from the data source, never from anything a reader can
 * toggle. With `demo-data.js` empty, this degrades to `<Refusal>`, which is the
 * ruled default.
 */
const DEMO_FALLBACK = true

/**
 * Why the listing's first-ranked session was the one opened, in words.
 *
 * `chooseListedSession` returns the branch it actually took so no caller can
 * report a choice the function did not make; this is that branch said out loud
 * in the log, which is the record of what this app did (Rule 12). Every value
 * is a fact the LISTING carries — never a state word, which only the adapter
 * can derive and only for a session that has been read.
 */
/** How many listing rows the header's popup holds. The generic `Menu` is one
 *  70vh popup shared by eight kinds; the whole list, filterable, is the
 *  Sessions drawer's job. */
const SESSION_MENU_ROWS = 12

/**
 * What a row whose id the server's parameter grammar refuses says when it is
 * clicked (`crates/cli/src/uiserve.rs:1387-1394`).
 *
 * It is LISTED rather than hidden: the store holds it, and quietly dropping a
 * row would be a smaller-looking list that lies about what is there. It just
 * cannot be opened, and saying so is better than a request that returns 400.
 */
const UNOPENABLE_ID = 'That session’s id is not one this app is allowed to ask the local app for, so it cannot be opened here.'

/**
 * What an ABSENT `nextCheck` is allowed to say.
 *
 * THE DEFECT. Four places said, whenever `claims[].nextCheck` was absent, some
 * form of "the declared check is proven" — a sentence written for the bundled
 * `demo-honest` corpus, whose one claim IS proven. `nextCheck` is not carried
 * by the claims projection at all (`adapter.js:105`), so on every live reading
 * it is absent, and the sentence rendered against every claim regardless of its
 * word. Measured 2026-09-08 on session `5e5a0021-…`: the inspector showed
 * `State — Recorded` and, five rows below it, "none — the declared check
 * is proven". One pane, two contradictory statements about the same claim, and
 * the false one was the reassuring one.
 *
 * An absent field may state its own absence. It may not state a verdict.
 *
 * @param {object|null} claim an adapted claim row
 * @returns {string} the note, safe for any word
 */
function nextCheckAbsent(claim) {
  return claim && claim.word === 'proven'
    ? 'none — the declared check is proven; this proves that check, not the project'
    : 'none in this reading — the claims projection carries no next check for this claim'
}

const LISTING_REASON = Object.freeze({
  'stopped-with-changes': 'it finished on its own and it changed files',
  stopped: 'it finished on its own',
  events: 'it holds the events the list counted',
  'first-listed': 'it is the first the list returned',
  'none-openable': 'no session on the list carries an id this app is allowed to ask for',
})

/**
 * A fully-shaped empty session, so every downstream `.map`/`.length` is safe
 * (the design's `S()` fallback, dc.html L30). It is NOT a session: nothing
 * renders from it except through a `<Refusal>`-guarded surface.
 */
const EMPTY_SESSION = Object.freeze({
  key: '',
  label: '',
  project: '',
  nodes: [],
  frames: [],
  wires: [],
  events: [],
  claims: [],
  checks: [],
  promises: [],
  asks: [],
  testChanges: [],
  capture: {},
  verify: { rows: [], claims: {} },
  receipt: {},
  narr: {},
  commentary: {},
  exits: {},
  stop: {},
  evidencePath: [],
  eventsComplete: true,
  compactions: 0,
  sessionWord: null,
})

/** R7: the zoom below which the graph's type stops being readable. The opening
 *  view never goes under it; it shows less of the graph instead. */
/* Card families, WORST FIRST — the same order `worstFam` ranks by, restated as
 * an array because the overview rasterises to a rank and paints back from one.
 * A cell holding several cards takes the lowest index of them. */
const MM_RANK = Object.freeze(['red', 'amber', 'violet', 'ochre', 'blue', 'green', 'grey'])

const READABLE_Z = 0.55

/** R6: under this viewport height the bottom dock is clamped so the canvas
 *  keeps a floor; over ~1400px wide the toolbar shows every group inline. */
const SHORT_VH = 800
const WIDE_VW = 1400
/** Focus-set depth over derivation wires (ruled: 3 — HQ-VERDICT-PANEL-PLAN). */
const FOCUS_DEPTH = 3

/** Row heights, by density prop (spec-script §B.1 #8). */
const ROW_H = Object.freeze({ compact: 28, comfortable: 32 })

/** Dock-size defaults, by viewport (spec-script §B.1 #9). */
const SIZES_WIDE = Object.freeze({ left: 220, right: 380, bottom: 200 })
const SIZES_NARROW = Object.freeze({ left: 200, right: 320, bottom: 180 })

/* ── Root layout styles. Every literal comes from `./tokens.js` (CONTRACT §1);
 *    these five are the only containers this file draws. ── */
const ROOT = {
  height: '100vh',
  display: 'flex',
  flexDirection: 'column',
  background: C.bgApp,
  color: C.ink,
  overflow: 'hidden',
  position: 'relative',
  fontFamily: FONT.sans,
  fontSize: FS.base,
}
const MIDDLE = { flex: 1, display: 'flex', minHeight: 0, position: 'relative' }
const CENTER = { flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, minHeight: 0 }
const CONTENT = { flex: '1 1 auto', position: 'relative', display: 'flex', flexDirection: 'column', minHeight: 240 }
const REFUSAL_BOX = { flex: 1, display: 'flex', minHeight: 0, overflow: 'auto' }

/**
 * @typedef {object} WorkspaceProps
 * @property {object} index the §9 summary index, already validated by
 *   `main.jsx` against `REQUIRED = [contract_version, kind, generated_at,
 *   totals, projects]`. A prop is not a load: nothing here re-reads it.
 * @property {boolean} [commentary=true] whether authored commentary renders at
 *   all. Design prop, tested `!== false`. WHETHER THIS SHIPS is an operator
 *   decision (spec-doctrine D-1); the default here is the design's, and the
 *   accusatory strings do not survive either way.
 * @property {'manager'|'operator'} [register='manager'] which narration
 *   register the CLI-narration blocks read
 * @property {'compact'|'comfortable'} [density='compact'] row height: 28 or 32.
 *   The design has NO UI control for this anywhere (spec-template Decision D9);
 *   until one is ruled it is pinned by this prop.
 */

/**
 * The workspace root.
 *
 * @extends {React.Component<WorkspaceProps>}
 */
export default class Workspace extends React.Component {
  /** @param {WorkspaceProps} props */
  constructor(props) {
    super(props)
    /** the positioning context every overlay measures against (replaces
     *  `closest('[data-screen-label]')`) */
    this.rootRef = React.createRef()
    /** the graph viewport, read by `measure()` */
    this.canvasRef = React.createRef()
    /** the palette input, focused 30ms after open */
    this.palRef = React.createRef()
    /** live drag descriptor, deliberately OUTSIDE state so a mousemove does not
     *  re-render for its own bookkeeping: `{type:'pan'|'node'|'split', …}` */
    this.drag = null
    this.replayTimer = null
    this.toastTimer = null
    this.mountTimers = []
    this.lastPrefs = ''
    /** the element the non-passive wheel listener is attached to (Decision D8) */
    this.wheelEl = null
    /** read epoch — bumped on every load so a slow answer for an abandoned
     *  read cannot overwrite a newer one (read discipline rule 4) */
    this.epoch = 0
    this.live = false
    /** the `?session=` address this page was opened at, or null. Read ONCE, in
     *  the constructor and not in `componentDidMount`, because `loadData()`
     *  needs it on its first pass and `componentDidMount` calls `loadData()`
     *  before it reads the route. An address is not state (`os/shell.jsx`'s
     *  `?project=`/`?node=` precedent, read once in a `useMemo`). */
    this.deepSession = Workspace.sessionFromQuery()
    /** switch epoch — a session switch is its own read, and a slow answer for a
     *  session the reader has already moved off must not land (rule 4). */
    this.switchEpoch = 0
    /** memo caches, keyed by `session.key` and by `(key, query)` */
    /* `graph` and `views` were added by the canvas-cost pass. `graph` holds the
     * node view-models, the wires and the overview's raster — everything the
     * canvas draws — keyed on every input EXCEPT `view`, because nothing about
     * a card changes when the reader pans. `views` holds the saved-view counts,
     * which were being recomputed on every mousemove at a measured 40 ms. */
    this.memo = { facets: null, attrs: null, pal: null, find: null, index: null, flow: null, forced: null, graph: null, views: null }
    /* the last `buildVals()` result, so the STABLE graph handlers can read the
     * current wire list and overview box without being rebuilt every render */
    this.vals = null
    /* the graph's callback bundle, built ONCE. `React.memo` on `NodeCard` buys
     * nothing if its props change identity every render, and `GraphCanvas`'s
     * own wheel `useEffect` keys on `on.canvasWheel`. */
    this.graphOnCache = null
    /* the id of a card to focus after the next commit, set when a find, a
     * replay step or the palette moves the reader to a card that may not have
     * been mounted a moment ago */
    this.focusNodeAfter = null
    /* T-6-4: whether a thread was lit at the last commit, and the element
     * that held focus just before it lit — where Esc hands focus back. */
    this.threadLit = false
    this.focusBeforeThread = null

    const prefs = this.loadPrefs()
    const panels = (() => {
      const wide = typeof window === 'undefined' || window.innerWidth >= NARROW
      const sp = prefs.panels || {}
      return { left: wide, right: wide, bottom: sp.bottom !== false, minimap: sp.minimap !== false }
    })()

    this.state = {
      /* P6-1 · how much to show (Brief, the default, · Explore · Workbench), and how
       * it is worded. `register` is THE register: the HQ panel's Plain /
       * Technical used to be held inside `hq.jsx` and this shell's own came
       * from a prop nobody could change; the top bar's toggle, the settings
       * menu and `T` now move one value, and both readers take it from here. */
      altitude: initialAltitude({
        search: typeof window === 'undefined' ? '' : window.location.search,
        storage: Workspace.storage(),
      }),
      register: props.register === 'operator' ? 'operator' : 'manager',
      /* 1-8 · what is open */
      session: '',
      rep: 'graph',
      evidenceTab: 'verdict',
      /* P6-3: the Trace opens in its four lanes (Request · Work · Execution ·
       * Claims & evidence). The two older layouts are still Workbench's buttons. */
      traceDirection: 'lanes',
      /* P6-3: the Outline twin — the same cards and thread as a list. */
      outline: false,
      connectionsOpen: false,
      doc: 'trace',
      docs: [],
      sel: null,
      inspTab: 'details',
      dockTab: 'findings',
      selFinding: null,
      /* 9-15 · the lens */
      filters: [],
      filterText: '',
      findText: '',
      findOpen: false,
      findIdx: 0,
      findingsOnly: false,
      linked: false,
      /* Trace Focus (HQ-VERDICT-PANEL-PLAN Step 5). `focusMode` is 'focus' on
       * EVERY open and is never persisted (ruled: not remembered between
       * sessions — `savePrefs` must not learn it). `focusExpanded` holds the
       * focus group cards opened in place; `focusChain` the ids "Show proof"
       * outlined in amber. */
      focusMode: 'focus',
      focusExpanded: {},
      focusChain: null,
      /* 16-22 · the canvas */
      expanded: {},
      pos: {},
      widen: {},
      view: { x: 40, y: 30, z: 0.8 },
      cw: 900,
      ch: 500,
      /* R6: the VIEWPORT, tracked in state so nothing reads `window` during
       * render. The vertical budget below 800px and the toolbar's overflow
       * above ~1400px are both decided from these. */
      vw: typeof window !== 'undefined' ? window.innerWidth : 1440,
      vh: typeof window !== 'undefined' ? window.innerHeight : 900,
      replay: { idx: -1, playing: false },
      /* 23-31 · chrome and overlays */
      panels,
      sizes: prefs.sizes || null,
      menu: null,
      menuX: 0,
      menuY: 0,
      drawer: null,
      palette: false,
      palQ: '',
      palIdx: 0,
      /* 32-40 · logs, reading, table */
      cmdOpen: false,
      output: [],
      toast: null,
      reading: { status: 'idle', at: null },
      narrow: false,
      compare: { a: '', b: '' },
      groupBy: prefs.groupBy || 'frame',
      columns: prefs.columns || { seq: true, type: true, state: true, text: true, ref: true, file: true },
      savedViews: prefs.savedViews || [],
      /* 41-49 · the rest */
      saveName: '',
      hoverWire: null,
      railHover: null,
      railClosed: {},
      treeSearch: '',
      treeClosed: { cmds: true, snaps: true, files: true, risk: true },
      eventsFocusSeq: null,
      focusPrev: null,
      /* T10 — epoch ms of the last read that COMPLETED (first load, session
       * switch, Refresh). Null until one has; the top bar and the status
       * strip both render from this one field and nothing else. */
      lastReadAt: null,
      /* the data layer's own slice — the design had none (spec-script §H.7) */
      /* HOME IS THE INBOX (doctrine 6, UX-DOCTRINE §2 and its C7 amendment).
       * This booted into `'trace'`, so the first thing a reader met was the
       * graph — "the history debugger you open on purpose" — over whichever
       * session the store returns when none is named, which on a real machine
       * is chosen by `ORDER BY id` and holds nothing. The Inbox is the app's
       * ONE attention queue, it is backed by the §9 index that is already a
       * prop in hand, so it cannot be mid-flight, and it owns its own three
       * empty states (`os/shell.jsx:331-405`). Trace is one rail click away
       * and `#/trace` still opens it directly. */
      destination: 'trace',
      /* THE SESSIONS ADOPTED SO FAR — not "the reading". It GROWS as sessions
       * are opened and is never replaced, because Compare needs two adapted
       * sessions in it at once and the picker must be able to come back to one
       * already read without re-reading it. */
      corpus: { order: [], sessions: {} },
      isDemo: false,
      dataStatus: 'loading',
      dataRefusal: null,
      live: false,
      /* THE SESSION LISTING — addresses, not readings. Read ONCE at mount and
       * BEFORE the first reading, because the reading's identity depends on it:
       * a lazy listing would leave the first reading to `--last`, which is the
       * whole defect. Shaped like `acceptance`: a lazily-held document with its
       * own four facts (in flight · read and empty · refused · holds rows). */
      sessions: { status: 'idle', rows: [], total: null, refusal: null },
      /** the picker's filter, over id and repository. A view control, so it is
       *  state and not an address. */
      sessionFilter: '',
      projectSearch: '',
      /** a session switch is in flight, and which id — the picker and the
       *  header say so rather than looking inert (`state.READING_LINE`). */
      switching: '',
      /** P6-2 — the session whose Read the rail shows (`read-rail.jsx`), set
       *  when a session is chosen to be opened, and the one whose rail was
       *  skipped. The rail is the engine's own steps, not this page's. */
      readFor: '',
      readSkipped: '',
      /** a one-time notice this page owes the reader, `{title, absent, change,
       *  remedy}`, dismissible. Deliberately NOT the centre refusal: the
       *  reading that DID open is fine, and painting a failure over a working
       *  session would be a second lie about the same fact. */
      notice: null,
      /* F04: the acceptance document, read lazily when the `Agreed checks`
       * drawer is opened and never before — opening the app must not fan out
       * fifteen requests (data.js rule 2). */
      acceptance: { status: 'idle' },
    }
  }

  /* ── Session & preferences (spec-script §B.1) ────────────────────────── */

  /**
   * First claim's node, else the `ending` node, else the first node.
   *
   * @param {object} S a session
   * @returns {string|null}
   */
  defaultSel(S) {
    if (S.claims && S.claims[0]) return S.claims[0].node
    const e = (S.nodes || []).find((n) => n.id === 'ending')
    if (e) return 'ending'
    return S.nodes && S.nodes[0] ? S.nodes[0].id : null
  }

  /**
   * The active session, or a fully-shaped empty one. Memo-free but cheap; the
   * expensive derivations (`facetOptions`, `attrs`) are memoised separately
   * because the original called this ~120× per render (spec-script §G.1).
   *
   * @param {string} [k] session key; defaults to the open one
   * @returns {object}
   */
  /** True when the session read was refused: nothing about the session may be stated. */
  unread() {
    const st = this.state
    return centreState({ dataStatus: st.dataStatus, dataRefusal: st.dataRefusal, session: st.corpus.sessions[st.session] }).kind === 'refused'
  }

  /**
   * Defence in depth for an on-demand action over a refused read: say that the
   * session could not be read, and nothing it holds. True when it spoke.
   */
  unreadToast() {
    if (!this.unread()) return false
    this.toast(HQ_COPY.trace.unread.action)
    return true
  }

  S(k) {
    const key = k || this.state.session
    return this.state.corpus.sessions[key] || EMPTY_SESSION
  }

  /**
   * THE SESSION THE GRAPH DRAWS. In Everything it is `S()` itself, same
   * identity. In Focus it is `S()` with `nodes` replaced by
   * `focusProjection`'s derived list (Step 5) — `S.nodes` is never mutated;
   * every other surface (docks, table, timeline, explorer) keeps reading
   * `S()`. Memoised on the session, its node array, `focusExpanded` and the
   * recorder's condition; `key` carries that signature so `index()`, `flow()`
   * and `graphModel` re-derive when it moves.
   *
   * @returns {object}
   */
  G() {
    const S = this.S()
    const st = this.state
    if (st.focusMode !== 'focus') return S
    const opened = Object.keys(st.focusExpanded)
      .filter((k) => st.focusExpanded[k])
      .sort()
      .join(',')
    const cap = S.capture || {}
    const key = S.key + '#focus:' + opened + (cap.degraded ? ':gap' : '')
    if (this.focusMemo && this.focusMemo.key === key && this.focusMemo.src === S.nodes) return this.focusMemo.G
    const p = focusProjection(S.nodes, S.wires, {
      focusMode: 'focus',
      focusExpanded: st.focusExpanded,
      depth: FOCUS_DEPTH,
      degraded: cap.degraded === true,
      /* The projection appends the object count to the label. */
      gapLabel: 'Recording gap',
      gapLine: cap.recorderLine || 'recorded with gaps — cause not recorded',
    })
    const G = { ...S, key, nodes: p.nodes, focus: p.summary, focusGroups: p.groups }
    this.focusMemo = { key, src: S.nodes, G }
    return G
  }

  /** @param {string} gid @returns {boolean} whether a group card (LOD or focus) is open */
  groupOpen(gid) {
    return isFocusGroupId(gid) ? !!this.state.focusExpanded[gid] : !!this.state.expanded[gid]
  }

  /**
   * The state change that opens one group card. A focus group lives in
   * `focusExpanded` (the projection reads it); a level-of-detail group in
   * `expanded` (`visible()` reads it). Callers spread the result into `setState`.
   *
   * @param {string} gid @returns {object}
   */
  openGroup(gid) {
    const st = this.state
    return isFocusGroupId(gid) ? { focusExpanded: { ...st.focusExpanded, [gid]: true } } : { expanded: { ...st.expanded, [gid]: true } }
  }

  /**
   * Every session this reading has actually loaded, in order. Cross-session
   * drawers list exactly these — never the index's un-read projections, which
   * would be a promise the record cannot keep.
   *
   * @returns {object[]}
   */
  allSessions() {
    return this.state.corpus.order.map((k) => this.state.corpus.sessions[k]).filter(Boolean)
  }

  /**
   * Read the six persisted fields. Runs from the constructor BEFORE
   * `this.state` exists, so it must stay state-independent.
   *
   * @returns {object}
   */
  loadPrefs() {
    try {
      return JSON.parse(localStorage.getItem(PREFS_KEY) || '{}') || {}
    } catch (e) {
      return {}
    }
  }

  /**
   * Serialise and write, only when the string changed. While focus-canvas mode
   * is on it persists `focusPrev`'s bottom/minimap, not the collapsed live
   * values. The `try/catch` is load-bearing: React unmounts the tree on a throw
   * inside `componentDidUpdate`, and the dc runtime swallowed it (spec-script §E3).
   */
  savePrefs() {
    const s = this.state
    const p = JSON.stringify({
      rep: s.rep,
      panels: s.focusPrev
        ? { bottom: s.focusPrev.bottom, minimap: s.focusPrev.minimap }
        : { bottom: s.panels.bottom, minimap: s.panels.minimap },
      sizes: s.sizes,
      groupBy: s.groupBy,
      columns: s.columns,
      savedViews: s.savedViews,
    })
    if (p === this.lastPrefs) return
    this.lastPrefs = p
    try {
      localStorage.setItem(PREFS_KEY, p)
    } catch (e) {
      /* a blocked or full store degrades to defaults, silently and by design */
    }
  }

  /** @returns {string} `'HH:MM:SS UTC'` */
  nowUTC() {
    return new Date().toISOString().slice(11, 19) + ' UTC'
  }

  /** @returns {'op'|'mgr'} */
  reg() {
    const r = this.state && this.state.register ? this.state.register : this.props.register
    return r === 'operator' ? 'op' : 'mgr'
  }

  /** This machine's storage, or null where it is blocked. */
  static storage() {
    try {
      return typeof window === 'undefined' ? null : window.localStorage
    } catch (e) {
      return null
    }
  }

  /**
   * Move to an altitude, because a person chose it (P6-1). Remembered; an
   * address's `?altitude=` never reaches here. Brief and Explore show the
   * Trace document only — the other documents are Workbench's — and the canvas is
   * measured again once it is back on screen, because Brief does not draw it.
   *
   * @param {'brief'|'explore'|'workbench'} altitude
   * @param {Function} [then] what to do once the altitude is drawn (and, out
   *   of Brief, once the canvas is measured and fitted), so an act that lands
   *   in the graph is not undone by the fit that follows it (A3-F10)
   */
  setAltitude(altitude, then) {
    rememberAltitude(Workspace.storage(), altitude, { chosen: true })
    const was = this.state.altitude
    const upd = { altitude, menu: null }
    if (altitude !== 'workbench') upd.doc = 'trace'
    this.setState(upd, () => {
      if (was === 'brief' && altitude !== 'brief') {
        this.mountTimers.push(setTimeout(() => this.measure(true, () => { this.initialView(); if (then) then() }), 0))
      } else if (then) then()
    })
  }

  /** @returns {28|32} */
  rowH() {
    return ROW_H[this.props.density === 'comfortable' ? 'comfortable' : 'compact']
  }

  /**
   * Dock sizes. The original read `window.innerWidth` during render and was
   * therefore not reactive (spec-script §E6); this reads the already-tracked
   * measurement instead.
   *
   * @returns {{left:number, right:number, bottom:number}}
   */
  sizes() {
    const base = this.state.sizes || (this.state.cw >= 1180 || !this.state.narrow ? { ...SIZES_WIDE } : { ...SIZES_NARROW })
    /* R6: at 1280x720 the canvas — the centrepiece — was allocated 204px of
     * 720, less than the chrome above it, and `Fit` resolved to 35%. The
     * bottom dock keeps at most a fifth of a short window so the canvas keeps
     * a floor. An explicitly dragged size is clamped, not discarded: dragging
     * it back up in a taller window restores it. */
    const cap = this.bottomCap()
    if (cap == null) return base
    return base.bottom <= cap ? base : { ...base, bottom: cap }
  }

  /**
   * R6's cap on the bottom dock in a short window, px; null in a window tall
   * enough to have none. One derivation, read by `sizes()` and by the bottom
   * separator, whose "taller" step is off at it (A3-F10 × N13-DRAG-257).
   *
   * @returns {number|null}
   */
  bottomCap() {
    if (this.state.vh >= SHORT_VH) return null
    return Math.max(120, Math.round(this.state.vh * 0.2))
  }

  /* ── Lifecycle (spec-script §B.2) ────────────────────────────────────── */

  /** Binds `mousemove`/`mouseup`/`keydown`/`resize` on `window` — on the window,
   *  not the component's DOM, which is why a drag works outside the canvas and
   *  the keyboard map is global. Starts the one read the active
   *  destination needs, and schedules `measure(true) + initialView()` at 80ms
   *  and `measure()` at 700ms. */
  componentDidMount() {
    this.live = true
    this.onMove = (e) => this.handleMove(e)
    this.onUp = () => this.handleUp()
    this.onKey = (e) => this.handleKey(e)
    this.onResize = () => this.measure()
    window.addEventListener('mousemove', this.onMove)
    window.addEventListener('mouseup', this.onUp)
    window.addEventListener('keydown', this.onKey)
    window.addEventListener('resize', this.onResize)
    this.bindWheel()
    this.loadData()
    this.onLocalVerification = (event) => {
      if (!this.props.localVerification || !event.detail?.enabled) return
      const fingerprint = JSON.stringify([event.detail.pack_results, event.detail.history, event.detail.job, event.detail.evaluation])
      if (fingerprint === this.localFingerprint) return
      this.localFingerprint = fingerprint
      const session = localVerificationSession(event.detail)
      this.memo = { facets: null, attrs: null, pal: null, find: null, index: null, flow: null, forced: null, graph: null, views: null }
      const corpus = { order: [session.key], sessions: { [session.key]: session } }
      if (!this.state.session || this.state.dataStatus !== 'ready') this.adopt(corpus, false, true, session.key)
      else this.setState({ corpus })
    }
    window.addEventListener('thruline-verification-update', this.onLocalVerification)
    if (this.props.localVerification) this.localVerificationTimer = setInterval(async () => {
      try { const data = await readLocalVerification(); if (this.live) this.onLocalVerification({ detail: data }) }
      catch (error) {
        if (!this.live || !this.S().localData) return
        const data = this.S().localData
        this.onLocalVerification({ detail: { ...data, readError: error.message, pack_results: data.pack_results.map(r => ({ ...r, status: 'unknown', explanation: 'Current inputs could not be read. Refresh before relying on this result.', catalog_results: r.catalog_results.map(c => ({ ...c, status: 'unknown' })) })) } })
      }
    }, 3000)
    /* R12: restore the addressed destination. It goes through the IDEMPOTENT
     * door, not `railGo`, which toggles: `StrictMode` calls this method twice
     * on the same instance, and a toggle run twice opens the drawer and then
     * closes it again — the address would have been read and silently
     * discarded. */
    const route = Workspace.routeFromHash()
    if (route) this.openDestination(route)
    this.mountTimers.push(
      setTimeout(() => {
        this.measure(true, () => this.initialView())
      }, 80),
    )
    this.mountTimers.push(setTimeout(() => this.measure(), 700))
  }

  /** Removes all four listeners and clears EVERY timer: `replayTimer`,
   *  `toastTimer` and both mount timeouts. The original leaks the last three. */
  componentWillUnmount() {
    this.live = false
    this.epoch += 1
    window.removeEventListener('thruline-verification-update', this.onLocalVerification)
    if (this.localVerificationTimer) clearInterval(this.localVerificationTimer)
    window.removeEventListener('mousemove', this.onMove)
    window.removeEventListener('mouseup', this.onUp)
    window.removeEventListener('keydown', this.onKey)
    window.removeEventListener('resize', this.onResize)
    this.unbindWheel()
    if (this.replayTimer) clearInterval(this.replayTimer)
    if (this.toastTimer) clearTimeout(this.toastTimer)
    this.mountTimers.forEach((t) => clearTimeout(t))
    this.mountTimers = []
    this.replayTimer = null
    this.toastTimer = null
  }

  /** `savePrefs()` — prefs are checked on every render and written only when the
   *  serialised string changed — plus the wheel listener's re-bind, because the
   *  canvas element is unmounted whenever the representation is not the graph. */
  componentDidUpdate(prevProps, prevState) {
    this.savePrefs()
    if (this.canvasRef.current !== this.wheelEl) {
      this.unbindWheel()
      this.bindWheel()
    }
    /* T-6-4 · FOCUS MOVES WITH SELECTION. The element a person was on when a
     * thread first lit is remembered here — in the commit that lit it, before
     * any focus has moved — so Esc can hand focus back to it (`clearThread`). */
    const lit = !!(this.state.sel && this.state.dataStatus === 'ready' && this.thread())
    if (lit && !this.threadLit) this.focusBeforeThread = this.focusMark()
    if (!lit) this.focusBeforeThread = null
    this.threadLit = lit
    /* A move between altitudes redraws the regions; the selected card, where
     * the new altitude draws it, takes focus back rather than leaving it on
     * the page. Where it is not drawn (Brief), the top bar keeps it. */
    if (prevState && prevState.altitude !== this.state.altitude && this.state.sel && !this.focusNodeAfter) this.focusNodeAfter = this.state.sel
    /* the one deferred focus the canvas owns — see `select` */
    const want = this.focusNodeAfter
    if (want) {
      this.focusNodeAfter = null
      const el = this.cardElement(want)
      if (el && typeof el.focus === 'function') el.focus()
    }
  }

  /**
   * The element that stands for one card where it is drawn now: its row in
   * the Outline when the Outline is showing, else its card on the canvas.
   * Null when neither is drawn.
   *
   * @param {string} id @returns {HTMLElement|null}
   */
  cardElement(id) {
    const esc = String(id).replace(/["\\]/g, '\\$&')
    const root = this.rootRef.current
    if (root && typeof root.querySelector === 'function' && this.state.outline && this.state.doc === 'trace') {
      const row = root.querySelector('.ws-trace-outline [data-section="all"] [data-id="' + esc + '"]')
      if (row) return row
    }
    const c = this.canvasRef.current
    return c && typeof c.querySelector === 'function' ? c.querySelector('[data-id="' + esc + '"]') : null
  }

  /**
   * The canvas wheel handler, as a NON-PASSIVE native listener (Decision D8).
   * React registers `onWheel` passively, so a `preventDefault()` inside a JSX
   * handler is a no-op with a console warning and the page zooms instead of the
   * canvas.
   */
  bindWheel() {
    const el = this.canvasRef.current
    if (!el) return
    this.wheelEl = el
    this.onWheel = (e) => {
      e.preventDefault()
      const r = el.getBoundingClientRect()
      this.zoomBy(e.deltaY < 0 ? 1.12 : 0.89, e.clientX - r.left, e.clientY - r.top)
    }
    el.addEventListener('wheel', this.onWheel, { passive: false })
  }

  /** Detach the non-passive wheel listener, if one is attached. */
  unbindWheel() {
    if (this.wheelEl && this.onWheel) this.wheelEl.removeEventListener('wheel', this.onWheel)
    this.wheelEl = null
  }

  /**
   * Measure the canvas and the viewport width. Updates `cw`/`ch` only when
   * forced or the delta exceeds 1px, and on a narrow flip forces the two side
   * docks to follow. One `setState`, skipped when nothing changed.
   *
   * @param {boolean} [force]
   */
  measure(force, then) {
    const el = this.canvasRef.current
    const narrow = typeof window !== 'undefined' && window.innerWidth < NARROW
    const st = this.state
    const upd = {}
    if (typeof window !== 'undefined') {
      if (window.innerWidth !== st.vw) upd.vw = window.innerWidth
      if (window.innerHeight !== st.vh) upd.vh = window.innerHeight
    }
    if (el) {
      const r = el.getBoundingClientRect()
      if (force || Math.abs(r.width - st.cw) > 1 || Math.abs(r.height - st.ch) > 1) {
        upd.cw = r.width
        upd.ch = r.height
      }
    }
    if (narrow !== st.narrow) {
      upd.narrow = narrow
      upd.panels = { ...st.panels, left: !narrow, right: !narrow }
    }
    const shortV = typeof window !== 'undefined' && window.innerHeight < 720
    if (shortV !== !!st.shortV) {
      upd.shortV = shortV
      if (shortV) upd.bottomPref = st.panels.bottom
      upd.panels = { ...(upd.panels || st.panels), bottom: shortV ? false : (st.bottomPref ?? st.panels.bottom) }
    }
    /* R7: the callback runs AFTER the measurement is committed. `initialView()`
     * used to be called on the line after `measure(true)` and therefore read
     * the 900x500 placeholder, not the real canvas — which is why the graph
     * opened panned off-screen at a zoom computed for the wrong box. */
    if (Object.keys(upd).length) this.setState(upd, then)
    else if (then) then()
  }

  /* ── The data source (spec-script §H.7 — the design had none) ─────────── */

  /**
   * Read the session listing.
   *
   * SEPARATE FROM THE READING, and read FIRST, because the reading's identity
   * depends on it. Its refusal is NOT fatal: the workspace goes on to open the
   * session the server picks by itself, exactly as it did before this route
   * existed, and the picker says why it is empty. A listing failure must never
   * turn a working single-session app into a refusal screen.
   *
   * @param {number} epoch the caller's read epoch, re-checked before setState
   * @returns {Promise<object[]>} the rows, empty when refused or empty
   */
  async loadSessions(epoch) {
    const read = await readProjection(API.sessions)
    if (!this.live || epoch !== this.epoch) return []
    if (read.status === 'refused') {
      this.setState({ sessions: { status: 'refused', rows: [], total: null, refusal: { title: 'The session list could not be read.', missing: read.refusal, remedy: 'thruline sessions' } } })
      this.log('The session list could not be read · ' + read.refusal, C.danger)
      return []
    }
    /* An unknown shape or version refuses LOUDLY rather than being read for the
     * fields it happens to carry — the same discipline `adaptSessionSafe`
     * keeps for the explain document. */
    const listing = listingFromDocument(read.doc)
    if (listing.refusal) {
      this.setState({ sessions: { status: 'refused', rows: [], total: null, refusal: listing.refusal } })
      this.log('The session list could not be read · ' + listing.refusal.missing, C.danger)
      return []
    }
    this.setState({ sessions: { status: 'ready', rows: listing.rows, total: listing.total, perProject: read.doc.limit_scope === 'project', limit: listing.limit, refusal: null } })
    /* Rule 12: a row the listing carried and this app could not use is counted
     * and said, never silently dropped. */
    if (listing.dropped > 0) {
      this.log('The session list carried ' + countPhrase(listing.dropped, 'row', 'rows') + ' with no session id, which cannot be opened and are not listed.', C.danger)
    }
    return listing.rows
  }

  /**
   * Read the CLAIMS projection for one session.
   *
   * WHY THIS EXISTS. `adapt()` passed `claims: null` and nothing on the live
   * path ever fetched `/api/claims`, so `S.claims` was `[]` for every real
   * session — while `DESTINATION_SOURCE.claims` said the Claims destination
   * reads `/api/claims` and `DESTINATION_SOURCE.bindings` said the same of
   * Evidence bindings (`data.js:234-237`). Measured on the operator's store,
   * 2026-09-08: `/api/claims?session=5e5a0021-…` carried one `task_complete`
   * claim over 18 changed files, and the app's Claims drawer said
   * "no completion claims were made · 267 events and no claim to check against
   * them". The record held a claim and the screen denied it — the one thing
   * this product may never do.
   *
   * The claims document is RICHER than explain's `claims_vs_proof` and is the
   * claim identity's source (`adapter.js:488-489`): id, verifier, scope, span,
   * evidence and binding live only here. So it is read as well as explain, not
   * instead of it.
   *
   * A FAILED CLAIMS READ DOES NOT CLOSE THE SESSION. The explain document can
   * still open it; the claim rows are one pane's content, not the reading.
   * It is logged rather than swallowed (Rule 12) and the panes that need it say
   * their own absence.
   *
   * @param {string} key a session id from the listing, or '' for the server's
   *   own default — the same address `/api/explain` was given
   * @param {function():boolean} isCurrent whether this read still belongs to
   *   the active load, checked before logging a late failure
   * @returns {Promise<object|null>} the `Read`, or null when it could not be read
   */
  async readClaimsFor(key, isCurrent = () => this.live) {
    const url = key ? API.claims + '?session=' + encodeURIComponent(key) : API.claims
    let read
    try {
      read = await readProjection(url)
    } catch (e) {
      read = { status: 'refused', refusal: String((e && e.message) || e).slice(0, 200) }
    }
    if (!isCurrent()) return null
    if (read && read.status === 'ready' && read.doc) return read
    this.log('The claims projection could not be read · ' + ((read && read.refusal) || url + ' did not answer') + ' · the reading is unaffected; claim panes will say so.', C.danger)
    return null
  }

  /**
   * Load the reading the workspace opens on.
   *
   * Real documents through the adapter when the server or a fixture answers;
   * the bundled demo corpus otherwise, and ONLY under the permanent banner. An
   * unknown shape, an unreadable document or an adapter refusal becomes a
   * NAMED refusal — never a guess, never an empty canvas (Rule 12).
   *
   * WHICH session it opens is now a decision with a reason, taken from the
   * listing before the reading is asked for: the addressed one when `?session=`
   * names a listed session, else the one `chooseListedSession` ranks first,
   * else — listing refused or empty — whatever `/api/explain` returns with no
   * session named, which is the behaviour that shipped before this route
   * existed.
   *
   * Read discipline: one live flag, one epoch re-checked before every
   * `setState`, and nothing loads that the active destination does not need.
   */
  onObservationUpdate = async (data) => {
    if (this.props.localVerification || this.state.isDemo || this.observationReading) return
    const id = this.state.session
    if (!id || !data.sessions?.includes(id) || !['watching', 'reading'].includes(data.state)) return
    const key = JSON.stringify([id, data.events_written, data.last_observed_at])
    if (key === this.observationFingerprint) return
    this.observationReading = true
    try {
      const [explain, claims] = await Promise.all([
        readProjection(API.explain + '?session=' + encodeURIComponent(id)),
        this.readClaimsFor(id, () => this.live && this.state.session === id),
      ])
      if (!this.live || this.state.session !== id || !explain?.doc || explain.status !== 'ready') return
      /* A FAILED OR EMPTY RE-READ NEVER REPLACES WHAT IS OPEN (STEP0-AUDIT D19 and its request 14 row). A
       * claims read that refused, or answered with nothing, adapts to a session with NO CLAIM AT ALL, and
       * this path put that over a session a reader was looking at: a verdict that had claims flipped to
       * "reported nothing as done" because a read failed — the record still held them. The record is
       * append-only, so a re-read carrying fewer recorded events than the one already open is the same kind
       * of answer, an empty read rather than the record moving. Both are refused here and what is open stays
       * open. The fingerprint is still recorded, so a re-read that cannot be used is not attempted again for
       * the same observation — the cadence this path had before, with the replacement taken out of it. */
      const open = this.state.corpus.sessions[id] || null
      if (!claims) { this.observationFingerprint = key; return }
      const entry = this.state.sessions.rows.find(row => row.id === id) || null
      const session = this.adapt(explain, null, entry, claims)
      if (open && (session.events || []).length < (open.events || []).length) {
        this.log('A re-read of this session carried fewer recorded events than the one already open, so nothing was replaced · ' + id, C.danger)
        this.observationFingerprint = key
        return
      }
      this.memo = { facets: null, attrs: null, pal: null, find: null, index: null, flow: null, forced: null, graph: null, views: null }
      this.setState({ corpus: this.mergeCorpus(session) })
      this.observationFingerprint = key
    } catch (error) { this.toast('New recorded events could not be read. ' + error.message) }
    finally { this.observationReading = false }
  }

  async loadData() {
    if (this.props.localVerification) {
      this.setState({ dataStatus: 'loading', dataRefusal: null })
      try {
        const data = await readLocalVerification()
        if (!data.enabled) throw new Error(data.detail || 'Local verification was not enabled for this server.')
        if (!this.live) return
        const session = localVerificationSession(data)
        this.adopt({ order: [session.key], sessions: { [session.key]: session } }, false, true, session.key)
        this.setState({ sessions: { status: 'ready', rows: [], total: 1, refusal: null }, lastReadAt: Date.now() })
      } catch (error) { if (this.live) this.setState({ dataStatus: 'refused', dataRefusal: { title: 'Local verification could not be read', missing: error.message, detail: 'Use Refresh reading to retry. No prior result is confirmed current.' } }) }
      return
    }
    const epoch = ++this.epoch
    this.setState({ dataStatus: 'loading', dataRefusal: null, sessions: { status: 'loading', rows: [], total: null, refusal: null } })

    let session = null
    let refusal = null
    let live = false

    const rows = await this.loadSessions(epoch)
    if (!this.live || epoch !== this.epoch) return

    /* THE ADDRESS IS CHECKED AGAINST THE LISTING, and only ever against it. The
     * server answers 200 with an all-empty document for any syntactically valid
     * id, so opening an unlisted one would draw an empty screen indistinguish-
     * able from a real empty session — the exact defect, with an extra step. */
    let wanted = ''
    let why = 'no-listing'
    if (!this.state.isDemo && this.state.session && listingHasSession(rows, this.state.session)) {
      /* A RE-READ RE-READS WHAT IS OPEN. `refresh()` calls this method, and
       * re-deciding here would walk the reader off the session they were
       * looking at every time they pressed Refresh. */
      wanted = this.state.session
      why = 'reopened'
    } else if (this.deepSession && listingHasSession(rows, this.deepSession)) {
      wanted = this.deepSession
      why = 'addressed'
    } else if (this.deepSession) {
      /* TRACE-MISSING-SESSION-TRUE (D-332, verifier F4): the link's session is
       * refused by name. No other session is read in its place — not the
       * list's first-ranked one, not `--last` — and the address keeps its id. */
      this.log(DEEP_LINK_ABSENT.title + ' · ' + this.deepSession, C.danger)
      this.setState({
        dataStatus: 'refused',
        dataRefusal: {
          title: 'This reading could not be opened.',
          missing: fill(HQ_COPY.trace.unread.absent, { name: this.deepSession }),
          remedy: HQ_COPY.trace.unread.remedy,
        },
      })
      return
    } else if (rows.length) {
      const picked = chooseListedSession(rows)
      wanted = picked.id
      why = picked.reason
    }

    const url = wanted ? API.explain + '?session=' + encodeURIComponent(wanted) : API.explain
    /* P6-2 — the Read's rail opens with the session, beside the record while it assembles. */
    if (wanted && !this.state.isDemo) this.setState({ readFor: wanted })

    try {
      // The listing establishes identity first; these two projections are
      // independent once that identity is known. Neither request waits for
      // the other's ledger and verification work.
      const [explain, claims] = await Promise.all([
        readProjection(url),
        this.readClaimsFor(wanted, () => this.live && epoch === this.epoch),
      ])
      if (!this.live || epoch !== this.epoch) return
      if (explain && explain.status === 'ready' && explain.doc) {
        const sessionId = explain.doc.session_id || explain.doc.session || wanted
        const entry = rows.find((row) => row.id === sessionId) || null
        // Export order is unrelated to this session. Its repository comes
        // from the matching live listing row, never the first old fixture.
        session = this.adapt(explain, null, entry, claims)
        live = true
      } else if (explain && explain.status === 'refused') {
        refusal = {
          title: 'This reading could not be opened.',
          missing: explain.refusal || 'the explain projection was not returned by ' + url,
          remedy: HQ_COPY.trace.unread.remedy,
        }
      }
    } catch (e) {
      refusal = {
        title: 'This reading could not be opened.',
        missing: e && e.name === 'UnknownShape' ? e.message : 'the explain projection could not be adapted',
        remedy: HQ_COPY.trace.unread.remedy,
        detail: String((e && e.message) || e).slice(0, 2000),
      }
    }

    if (!this.live || epoch !== this.epoch) return

    if (session) {
      /* The chosen key is passed in rather than re-decided: `chooseSession`
       * would run again over a corpus that now GROWS, and the first time a
       * reader opens an empty session it would silently move them off it. */
      this.adopt(this.mergeCorpus(session), false, live, session.key)
      /* T10: a read of the record completed — stamped now, not when it began. */
      this.setState({ lastReadAt: Date.now() })
      this.writeSession(session.key)
      this.log(
        why === 'reopened'
          ? 'Re-read the session already open · ' + session.key
          : why === 'addressed'
            ? 'Opened the session this link named · ' + session.key
            : why === 'addressed-not-listed'
            ? 'The link named a session the list does not contain, so the list’s first-ranked session was opened · ' + session.key
            : why === 'no-listing'
              ? 'No session list to choose from, so the local app chose · ' + session.key
              : 'Opened from the session list · ' + session.key + ' · ' + LISTING_REASON[why],
      )
      return
    }

    /* The bundled fallback. It engages only when the corpus is actually
     * present, and when it does the banner is mounted with it. */
    if (DEMO_FALLBACK && !this.props.liveOnly) {
      const order = (DEMO_ORDER || []).filter((k) => !!demoSession(k))
      if (order.length) {
        const sessions = {}
        order.forEach((k) => {
          sessions[k] = demoSession(k)
        })
        this.adopt({ order, sessions }, true, false)
        return
      }
    }

    this.setState({
      dataStatus: 'refused',
      dataRefusal:
        refusal || {
          title: 'There is no reading open.',
          missing: DESTINATION_SOURCE.trace ? DESTINATION_SOURCE.trace.absent : 'no session projection was returned',
          remedy: 'thruline ingest --apply && thruline export --all',
        },
    })
  }

  /**
   * Hand the documents to the pure adapter. Kept separate so the refusal path
   * is one `catch` and the adapter itself never learns about I/O.
   *
   * @param {object} explain the explain `Read`
   * @param {object|null} project the §9 project document, when one is open
   * @param {object|null} entry the matching live session listing row, or the
   *   index's `projects[]` row for a fixture
   * @param {object|null} [claims] the claims `Read` (`readClaimsFor`), or null
   *   when it could not be read — which is a pane's absence, not a refusal of
   *   the reading
   * @returns {object} a Session
   */
  adapt(explain, project, entry, claims) {
    const doc = explain.doc || {}
    const claimsDoc = (claims && claims.doc) || null
    const sessionId = doc.session_id || doc.session || (entry && entry.id) || 'session'
    return adaptSessionSafe({
      explain: doc,
      claims: claimsDoc,
      asks: null,
      receipt: null,
      verify: null,
      narrOp: null,
      narrMgr: null,
      exits: claims && claims.exit != null
        ? { explain: explain.exit == null ? 0 : explain.exit, claims: claims.exit }
        : { explain: explain.exit == null ? 0 : explain.exit },
      key: String(sessionId),
      project: (entry && entry.repo) || (project && project.project && project.project.name) || (entry && entry.name) || '',
      harness: (entry && entry.harness) || '',
      origin: explain.projection ? 'projection ' + explain.projection : '',
    })
  }

  /**
   * The corpus with one more session in it, in the order it was opened.
   *
   * The corpus GROWS and is never replaced. Re-adopting a session already in it
   * refreshes that session's document and leaves the order alone, so a switch
   * back is free and Compare keeps whatever pair it was pointing at.
   *
   * IT IS NOT CAPPED, and that is a decision rather than an oversight: a cap
   * would have to evict, and an eviction that took the session Compare is
   * pointing at would silently change what Compare is comparing. A reader
   * opening enough sessions to make this matter is a real cost and it is
   * recorded here as owed rather than solved by guessing an eviction rule.
   *
   * @param {object} session an adapted session
   * @returns {{order:string[], sessions:object}}
   */
  mergeCorpus(session) {
    const c = this.state.corpus
    const order = c.order.indexOf(session.key) >= 0 ? c.order : c.order.concat([session.key])
    return { order, sessions: { ...c.sessions, [session.key]: session } }
  }

  /**
   * Adopt a corpus: settle the open session, its default selection, its first
   * finding, and the compare pair — all from the record, none hardcoded
   * (the design pinned `{a:'demo-narrowed', b:'demo-honest'}`, spec-script §G.2).
   *
   * @param {{order:string[], sessions:object}} corpus
   * @param {boolean} isDemo whether the banner must be mounted
   * @param {boolean} live whether these came from a live projection
   * @param {string} [chosenKey] the key the caller has ALREADY decided on. When
   *   it is present in the corpus it is used verbatim and the reason is
   *   `'requested'`; otherwise `chooseSession` decides, exactly as before. The
   *   demo path passes none and is therefore unchanged.
   */
  adopt(corpus, isDemo, live, chosenKey) {
    /* THE DEFAULT-SESSION RULE (`state.chooseSession`): open the first session
     * that holds recorded events, else the first that holds a claim, else the
     * first the reading returned.
     *
     * WHO DECIDES, NOW THAT THERE IS A LISTING. `chooseSession` reads ADAPTED
     * sessions and is the right rule for a corpus already in hand — the demo
     * corpus, and any re-adopt. It is the wrong rule for forty-four thousand
     * addresses, so the live path decides from the listing instead
     * (`state.chooseListedSession`, tiers over what a listing can prove) and
     * passes the answer in as `chosenKey`. Re-deciding here would undo it: the
     * corpus now grows, and the first time a reader opened an empty session
     * `chooseSession` would move them off it without saying so.
     *
     * The paragraph this replaces recorded, accurately, that the rule bound
     * only the demo corpus because "the server has no session-listing
     * endpoint". It has one now (`crates/cli/src/uiserve.rs:656-670`), the live
     * path reads it before the reading, and `?session=` addresses a session
     * directly — so that paragraph was retired in the same change that made it
     * false rather than left standing as a true-sounding sentence about a
     * defect that had been fixed.
     *
     * WHAT THE RULE STILL DELIVERS, live and demo alike: empty sessions are not
     * skipped out of existence — every reading stays in `order` and reachable —
     * `reason` reports which branch actually ran, and when nothing holds
     * anything the pane STATES the emptiness instead of drawing a blank. */
    const chosen = chosenKey && corpus.sessions[chosenKey] ? { key: chosenKey, reason: 'requested' } : chooseSession(corpus)
    const first = chosen.key
    const S = corpus.sessions[first] || EMPTY_SESSION
    this.memo = { facets: null, attrs: null, pal: null, find: null, index: null, flow: null, forced: null, graph: null, views: null }
    this.setState(
      (s) => ({
        corpus,
        isDemo,
        live,
        dataStatus: 'ready',
        dataRefusal: null,
        session: first,
        sel: null,
        selFinding: S.claims && S.claims[0] && S.claims[0].findings && S.claims[0].findings[0] ? S.claims[0].findings[0].key : null,
        docs: (S.claims?.length ? [{ id: 'claim', session: first }] : []).concat(corpus.order.length > 1 ? [{ id: 'compare' }] : []),
        /* SEEDED ONCE, then left alone. This was recomputed on every adopt,
         * which was harmless while the corpus was one session and is not now:
         * every session a reader opens would reset the pair they had chosen to
         * compare. A pair is a reader's choice, not a consequence of a read. */
        compare: s.compare && s.compare.a ? s.compare : { a: first, b: corpus.order[1] || first },
      }),
      () => {
        const S2 = this.S()
        this.log(
          'Read ' +
            S2.label +
            ' · ' +
            (S2.eventCount != null ? countPhrase(S2.eventCount, 'event', 'events') : 'event count not in this reading') +
            (S2.exits && S2.exits.explain != null ? ' · explain exit ' + S2.exits.explain : ''),
        )
        this.log('Selected claim' + (S2.claims[0] ? ' ' + S2.claims[0].id : '') + ' · inspector open on Details · findings dock open')
        /* Said plainly, in the log, when the rule found nothing worth opening.
         * The pane states it on screen; this is the same fact in the record of
         * what this app did, so the choice is never silent (Rule 12). */
        if (chosen.reason === 'nothing-held') {
          this.log(
            corpus.order.length > 1
              ? 'No reading open here holds a recorded event, so the first one was opened.'
              : 'This reading holds no recorded events.',
          )
        }
        this.measure(true)
        this.initialView()
      },
    )
  }

  /* ── Output, toast, clipboard (spec-script §B.3) ─────────────────────── */

  /**
   * Append one line to the log ring buffer, capped at 200.
   *
   * @param {string} text @param {string} [color]
   */
  log(text, color) {
    const t = this.nowUTC().slice(0, 8)
    this.setState((s) => ({ output: s.output.concat([{ t, text, color: color || C.inkCode }]).slice(-200) }))
  }

  /**
   * One toast, no queue, 2600ms. The timer is cleared on unmount.
   *
   * @param {string} text
   */
  toast(text) {
    if (this.toastTimer) clearTimeout(this.toastTimer)
    this.setState({ toast: text })
    this.toastTimer = setTimeout(() => {
      if (this.live) this.setState({ toast: null })
    }, 2600)
  }

  /**
   * Copy to the clipboard. The toast fires on a REAL success only — the
   * original toasted even when the fallback had silently failed (spec-script §E6).
   *
   * @param {string} text @param {string} what what was copied, for the toast
   */
  copy(text, what) {
    const done = () => {
      this.toast('Copied ' + what + ' — copying does not execute.')
      this.log('Copied ' + what)
    }
    const failed = () => {
      this.toast('Could not copy ' + what + ' — this browser refused the clipboard.')
      this.log('Could not copy ' + what + ' — the clipboard was refused', C.amber)
    }
    try {
      navigator.clipboard.writeText(text).then(done, () => {
        if (this.fallbackCopy(text)) done()
        else failed()
      })
    } catch (e) {
      if (this.fallbackCopy(text)) done()
      else failed()
    }
  }

  /**
   * The `execCommand` fallback, which REPORTS whether it worked.
   *
   * @param {string} text
   * @returns {boolean}
   */
  fallbackCopy(text) {
    try {
      const ta = document.createElement('textarea')
      ta.value = text
      ta.style.position = 'fixed'
      ta.style.opacity = '0'
      document.body.appendChild(ta)
      ta.select()
      const ok = document.execCommand('copy')
      document.body.removeChild(ta)
      return !!ok
    } catch (e) {
      return false
    }
  }

  /** Stop the replay interval; only clears `playing` when it was playing. */
  stopReplay() {
    if (this.replayTimer) {
      clearInterval(this.replayTimer)
      this.replayTimer = null
    }
    if (this.state.replay.playing) this.setState((s) => ({ replay: { idx: s.replay.idx, playing: false } }))
  }

  /* ── Graph model — the derivation layer (spec-script §B.4) ───────────── */

  /**
   * The id → node and seq → node indexes, built ONCE per session.
   *
   * `nodeById` was an `Array.find` and it is called from inside three maps:
   * the minimap ran ≈18 million comparisons per render on a 4,243-object
   * session, and the left dock's command list ran ≈5.9 million more. Grouping
   * makes both worse rather than better, so the index lands in the same pass
   * (DESIGN-readable-trace §2.3).
   *
   * @returns {{byId: Map<string, object>, bySeq: Map<number, object>}}
   */
  index() {
    const S = this.G()
    if (this.memo.index && this.memo.index.key === S.key && this.memo.index.n === S.nodes.length) return this.memo.index
    const byId = new Map()
    const bySeq = new Map()
    S.nodes.forEach((n) => {
      byId.set(n.id, n)
      if (n.seq != null && String(n.type).indexOf('EVENT') === 0 && !bySeq.has(n.seq)) bySeq.set(n.seq, n)
    })
    /* A folded member is still the node for its seq; when it is hidden the
     * group standing for it is what a caller wants to centre on, so groups
     * register their whole span. */
    S.nodes.forEach((n) => {
      if (!n.group || !Array.isArray(n.members)) return
      n.members.forEach((m) => {
        const mn = byId.get(m)
        if (mn && mn.seq != null && !bySeq.has(mn.seq)) bySeq.set(mn.seq, mn)
      })
    })
    this.memo.index = { key: S.key, n: S.nodes.length, byId, bySeq }
    return this.memo.index
  }

  /** @param {string} id @returns {object|undefined} */
  nodeById(id) {
    return this.index().byId.get(id)
  }

  /**
   * The node for a recorded seq: the EVENT node, falling back to the GROUP
   * whose members hold it.
   *
   * @param {number} seq @returns {object|undefined}
   */
  nodeBySeq(seq) {
    return this.index().bySeq.get(seq)
  }

  /**
   * Members a filter, a find-in-trace hit or a saved view has selected, which
   * are therefore visible whether or not their group is expanded.
   *
   * NF clause 5 (DESIGN-readable-trace §3) is a property of the READING, not of
   * the record, so it cannot live in the fold: a filter hit inside a collapsed
   * group, with nothing on screen saying so, is a silent drop. Memoised on the
   * reading, because it walks every node.
   *
   * @returns {Set<string>}
   */
  forcedMembers() {
    const S = this.G()
    const st = this.state
    const by = this.activeFilters()
    const q = st.findText.trim().toLowerCase()
    const key = S.key + '|' + JSON.stringify(by) + '|' + (st.findingsOnly ? 'f' : '') + '|' + (st.findOpen ? q : '')
    if (this.memo.forced && this.memo.forced.key === key) return this.memo.forced.set
    const set = new Set()
    const hasFilters = Object.keys(by).length > 0
    if (hasFilters || st.findingsOnly || (st.findOpen && q)) {
      const fctx = st.findingsOnly ? this.findingContext() : null
      S.nodes.forEach((n) => {
        if (!n.member) return
        if (hasFilters && this.matches(n)) set.add(n.id)
        else if (fctx && fctx[n.id]) set.add(n.id)
        else if (q && (String(n.title || '') + ' ' + (n.foot || '')).toLowerCase().indexOf(q) >= 0) set.add(n.id)
      })
    }
    this.memo.forced = { key, set }
    return set
  }

  /** @param {object} n @returns {boolean} */
  visible(n) {
    const ex = this.state.expanded
    if (n.member) return !!ex[n.member] || this.forcedMembers().has(n.id)
    if (n.group) return !ex[n.id]
    return true
  }

  /** @returns {object[]} */
  visibleNodes() {
    return this.G().nodes.filter((n) => this.visible(n))
  }

  /**
   * The vertical push an expanded group applies to the frame below it. The
   * design keyed this to the hardcoded node id `'snap'`; here the authored
   * layout names its own trigger and `'snap'` is only the fallback.
   *
   * @returns {number}
   */
  shiftY() {
    const lay = LAYOUT[this.S().key]
    if (!lay) return 0
    return this.state.expanded[lay.growOn || 'snap'] ? lay.grow || 0 : 0
  }

  /** @param {object} n @returns {string} */
  frameOf(n) {
    const lay = LAYOUT[this.S().key]
    const b = lay && lay.nodes && lay.nodes[n.id]
    return (b && b[3]) || n.frame
  }

  /**
   * THE COLUMN RE-STACK. A pure function of (session, what is expanded, what a
   * filter has forced visible), memoised on exactly that.
   *
   * The adapter lays every column out with its groups COLLAPSED, because the
   * collapsed reading is the one that paints first. The moment a reader expands
   * a group, the column below it holds different cards, and a layout that did
   * not move would paint them on top of each other. `Fit` and the minimap read
   * the same map, which is why the box a reader sees and the box the overview
   * draws cannot disagree.
   *
   * @returns {{key:string, pos:Map<string,{x:number,y:number,w:number}>, frameH:Map<string,number>}}
   */
  flow() {
    const S = this.G()
    const st = this.state
    const forced = this.forcedMembers()
    const key =
      S.key + '|' + st.traceDirection + '|' + JSON.stringify(st.widen) +
      '|' +
      Object.keys(st.expanded)
        .filter((k) => st.expanded[k])
        .sort()
        .join(',') +
      '|' +
      Array.from(forced).sort().join(',')
    if (this.memo.flow && this.memo.flow.key === key) return this.memo.flow

    if (st.traceDirection === 'lanes') {
      const L = laneLayout(S.nodes, {
        visible: (n) => this.visible(n),
        widthOf: (n) => (n.w || 260) + (st.widen[n.id] || 0),
        heightOf: (n, w) => this.cardH(n, w),
      })
      this.memo.flow = { key, ...L }
      return this.memo.flow
    }
    if (st.traceDirection === 'horizontal') {
      this.memo.flow = { key, ...journeyLayout(S.nodes, n => this.visible(n), n => (n.w || 280) + (st.widen[n.id] || 0), (n, w) => this.cardH(n, w)) }
      return this.memo.flow
    }

    const pos = new Map()
    /* Only a computed layout re-stacks. The bundled demo records place their
     * own boxes (tier 2), and a session whose frames this app did not place is
     * left exactly where its author put it. */
    if (!S.frames.some((f) => f.auto)) {
      this.memo.flow = { key, pos, frameH: new Map() }
      return this.memo.flow
    }
    const frameH = new Map()
    const byFrame = new Map()
    S.nodes.forEach((n) => {
      const f = n.frame || 'other'
      if (!byFrame.has(f)) byFrame.set(f, [])
      byFrame.get(f).push(n)
    })
    S.frames.forEach((f) => {
      const list = (byFrame.get(f.id) || []).slice().sort((a, b) => {
        /* The adapter's own order, restated: placement seq, then id. */
        const asv = placementSeq(a)
        const bsv = placementSeq(b)
        const as = asv == null ? Infinity : asv
        const bs = bsv == null ? Infinity : bsv
        if (as !== bs) return as - bs
        return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
      })
      let y = LAYOUT_METRICS.HEADER
      list.forEach((n) => {
        if (!this.visible(n)) return
        pos.set(n.id, { x: n.x, y, w: n.w })
        y += this.cardH(n, n.w) + LAYOUT_METRICS.GAP
      })
      frameH.set(f.id, y + LAYOUT_METRICS.PAD)
    })
    /* A hidden member still needs a box: `centerOn` reads one before it
     * expands the group, and the minimap reads one for every node it draws. It
     * borrows its group's. */
    S.nodes.forEach((n) => {
      if (pos.has(n.id)) return
      const g = n.member ? pos.get(n.member) : null
      pos.set(n.id, g ? { x: g.x, y: g.y, w: n.w } : { x: n.x, y: n.y, w: n.w })
    })
    this.memo.flow = { key, pos, frameH }
    return this.memo.flow
  }

  /**
   * One node's box. Precedence matters (dc.html L70-L76): authored layout box →
   * the node's own `x,y,w`; `w += widen`; the execution shift applies only when
   * there is NO authored layout; a user drag overrides x/y outright; height is
   * never stored and always comes from `estH`.
   *
   * @param {object} n @returns {{x:number,y:number,w:number,h:number}}
   */
  geo(n) {
    const horizontal = FLOWING[this.state.traceDirection] === true
    const lay = horizontal ? null : LAYOUT[this.S().key]
    const b = (lay && lay.nodes && lay.nodes[n.id]) || [n.x, n.y, n.w]
    let x = b[0] || 0
    let y = b[1] || 0
    /* Expanding a group changes what a column holds, so the column re-stacks.
     * Height is never stored (`estH` is the authority), which is what makes
     * this a read rather than a rebuild. An authored demo layout still wins:
     * the three demo sessions place their own boxes. */
    if (!lay) {
      const f = this.flow().pos.get(n.id)
      if (f) {
        x = f.x
        y = f.y
      }
    }
    const w = (b[2] || 0) + (this.state.widen[n.id] || 0)
    if (!horizontal && !lay && n.frame === 'execution' && this.shiftY()) y += this.shiftY()
    const p = this.state.pos[n.id]
    if (p) {
      x = p.x
      y = p.y
    }
    return { x, y, w, h: this.cardH(n, w) }
  }

  /**
   * A card's height: the taller of its Plain and Technical faces plus the
   * lines the Trace adds (where a run ran, the agent-reported line) —
   * `trace-view.cardHeight`, so the layout does not move when T flips the
   * face. Never stored.
   *
   * @param {object} n @param {number} w @returns {number}
   */
  cardH(n, w) {
    return cardHeight(n, w, this.S())
  }

  /**
   * The phase frames, with a rendered count phrase. A frame holding no visible
   * node is NOT returned — an empty box would say "nothing here" in a language
   * the reader cannot check (CONTRACT §2.3).
   *
   * @returns {Array<{id:string,title:string,count:string,x:number,y:number,w:number,h:number}>}
   */
  framesOf() {
    if (FLOWING[this.state.traceDirection] === true) return this.flow().frames
    const S = this.S()
    const lay = LAYOUT[S.key]
    const sh = this.shiftY()
    const vis = this.visibleNodes()
    return S.frames
      .map((f) => {
        const b = lay && lay.frames && lay.frames[f.id]
        let x = b ? b[0] : f.x
        let y = b ? b[1] : f.y
        const w = b ? b[2] : f.w
        /* The frame is as tall as what it currently holds — expanding a group
         * grows it, collapsing shrinks it — for the same reason height is
         * never stored on a card. */
        let h = b ? b[3] : lay ? f.h : this.flow().frameH.get(f.id) || f.h
        const mode = b ? b[4] : ''
        if (mode === 'grow') h += sh
        if (mode === 'shift') y += sh
        const count = vis.filter((n) => this.frameOf(n) === f.id).length
        return { id: f.id, title: f.title, x, y, w, h, n: count, count: countPhrase(count, 'object', 'objects') }
      })
      .filter((f) => f.n > 0)
  }

  /**
   * Wire-endpoint resolution: an expanded group resolves to its representative
   * member; a member of a collapsed group resolves to the group.
   *
   * @param {string} id @returns {object|null}
   */
  resolveEnd(id) {
    const n = this.nodeById(id)
    if (!n) return null
    const ex = this.state.expanded
    if (n.group && ex[n.id]) return n.rep ? this.nodeById(n.rep) || null : null
    if (n.member && !ex[n.member]) return this.nodeById(n.member) || null
    return n
  }

  /**
   * Where a wire attaches. The pin ROW inside a node body sits at `i*20`; the
   * wire attach point uses `+40`. The 10px difference is deliberate.
   *
   * @param {object} n @param {string} pin
   * @returns {{x:number,y:number}|null}
   */
  pinPos(n, pin) {
    const g = this.geo(n)
    let i = (n.pinsL || []).findIndex((p) => p.id === pin)
    if (i >= 0) return { x: g.x, y: g.y + 40 + i * 20 }
    i = (n.pinsR || []).findIndex((p) => p.id === pin)
    if (i >= 0) return { x: g.x + g.w, y: g.y + 40 + i * 20 }
    /* A focus group card carries no pins of its own; a wire into a merged
     * member attaches on the side that member's pin was on (`focus-set.js`
     * `pinSide`), at the card's first row. */
    if (n.pinSide && n.pinSide[pin]) return n.pinSide[pin] === 'R' ? { x: g.x + g.w, y: g.y + 40 } : { x: g.x, y: g.y + 40 }
    return null
  }

  /**
   * Every drawn wire: the record's own, plus the synthesised `order` wires
   * between consecutive visible event nodes.
   *
   * @returns {object[]}
   */
  allWires() {
    const S = this.S()
    const out = []
    S.wires.forEach((w) => {
      const a = this.resolveEnd(w.from[0])
      const b = this.resolveEnd(w.to[0])
      if (!a || !b) return
      const pa = this.pinPos(a, w.from[1])
      const pb = this.pinPos(b, w.to[1])
      if (!pa || !pb) return
      out.push({ id: w.id, a: a.id, b: b.id, pa, pb, kind: w.kind, c: w.c, label: w.label })
    })
    /* A group stands at its first member's place in the record, so it takes
     * its turn in the recorded order like any other card. Reading `n.seq`
     * alone dropped every group out of the order entirely. */
    const evs = this.visibleNodes()
      .filter((n) => placementSeq(n) != null && String(n.type).indexOf('EVENT') === 0)
      .sort((a, b) => placementSeq(a) - placementSeq(b))
    for (let i = 0; i < evs.length - 1; i++) {
      const a = evs[i]
      const b = evs[i + 1]
      const pa = this.pinPos(a, 'o')
      const pb = this.pinPos(b, 'i')
      if (pa && pb) {
        out.push({
          id: 'ord_' + a.id + '_' + b.id,
          a: a.id,
          b: b.id,
          pa,
          pb,
          kind: 'order',
          c: 'order',
          label: 'recorded order · seq ' + (a.lastSeq != null ? a.lastSeq : placementSeq(a)) + ' → seq ' + placementSeq(b),
        })
      }
    }
    return out
  }

  /**
   * Everything joined to one node by a wire, plus the group/member relation.
   *
   * @param {string|null} id @returns {Record<string, boolean>}
   */
  linkedSet(id) {
    const s = {}
    if (!id) return s
    this.allWires().forEach((w) => {
      if (w.a === id) s[w.b] = true
      if (w.b === id) s[w.a] = true
    })
    const n = this.nodeById(id)
    if (n && n.group && n.members) n.members.forEach((m) => (s[m] = true))
    if (n && n.member) s[n.member] = true
    return s
  }

  /**
   * The findings context: every finding node, everything wired to one, and the
   * claim and check the findings are about. The design hardcoded the ids `clm`
   * and `chk`; these come from the record (spec-script §G.2).
   *
   * @returns {Record<string, boolean>}
   */
  findingContext() {
    const S = this.S()
    const s = {}
    S.nodes.forEach((n) => {
      if (!n.finding) return
      s[n.id] = true
      Object.keys(this.linkedSet(n.id)).forEach((k) => (s[k] = true))
    })
    if (S.claims[0] && S.claims[0].node) s[S.claims[0].node] = true
    if (S.checks[0] && S.checks[0].node) s[S.checks[0].node] = true
    return s
  }

  /** @returns {number|null} the seq the replay cursor sits on */
  replaySeq() {
    const r = this.state.replay
    if (r.idx < 0) return null
    const ev = this.S().events
    const e = ev[Math.min(r.idx, ev.length - 1)]
    return e ? e.seq : null
  }

  /**
   * Where a derived object sits on the recorded timeline.
   *
   * @param {object} n @returns {number|null}
   */
  placedSeq(n) {
    if (n.seq != null) return n.seq
    const S = this.S()
    const ot = objType(n)
    if (ot === 'claim') return S.claims[0] ? S.claims[0].seq : null
    if (ot === 'promise') return S.promises[0] ? S.promises[0].seq : null
    if (ot === 'ask') return S.events[1] ? S.events[1].seq : null
    /* A group SPANS a range. `members[0].seq` replayed a band covering
     * seq 8044–12930 as though the whole of it happened at 8044. The placement
     * is the first end; `isDim` reads the last end to decide "fully past". */
    if (n.firstSeq != null) return n.firstSeq
    if (n.group && n.members) {
      const m = this.nodeById(n.members[0])
      return m ? m.seq : null
    }
    return null
  }

  /**
   * Where a node stands relative to the replay head: reached, partly reached,
   * or still ahead. A span is 'partial' between its two ends — the state that
   * did not exist before grouping and that a band needs in order to be honest.
   *
   * @param {object} n @param {number|null} rseq
   * @returns {'past'|'partial'|'future'|null}
   */
  replayState(n, rseq) {
    if (rseq == null) return null
    const start = this.placedSeq(n)
    if (start == null) return null
    if (start > rseq) return 'future'
    const end = n.lastSeq != null ? n.lastSeq : start
    return end > rseq ? 'partial' : 'past'
  }

  /**
   * The right-hand header line of a card: where it sits in the record.
   *
   * A single event says `seq N`. A GROUP says both ends of its span, because
   * seq is sparse and global and one end of a range is not a range. While a
   * replay head is standing inside a span, the header says so rather than
   * letting the card read as entirely past or entirely future.
   *
   * @param {object} n @param {{rseq:number|null}} ctx @returns {string}
   */
  cardSpan(n, ctx) {
    if (n.group && n.firstSeq != null && n.lastSeq != null && n.firstSeq !== n.lastSeq) {
      if (this.replayState(n, ctx.rseq) === 'partial') return 'seq ' + n.firstSeq + '–' + n.lastSeq + ' · reached seq ' + ctx.rseq
      return 'seq ' + n.firstSeq + '–' + n.lastSeq
    }
    if (n.seq != null && !n.group) return 'seq ' + n.seq
    if (n.seq != null && n.group) return 'seq ' + n.seq
    return ''
  }

  /**
   * The family a card should be painted in, given everything it stands for.
   *
   * A collapsed group borrows the worst of its members, so a negative can
   * never be quieter for having been folded. `FAM` order is worst-first in the
   * same sense the state words are; `red` outranks `amber` outranks the rest.
   *
   * @param {object} n @returns {string}
   */
  worstFam(n) {
    if (!n) return 'grey'
    if (!n.group || !Array.isArray(n.members)) return n.fam
    const rank = { red: 0, amber: 1, violet: 2, ochre: 3, blue: 4, green: 5, grey: 6 }
    let best = n.fam
    n.members.forEach((m) => {
      const mn = this.nodeById(m)
      if (!mn) return
      if ((rank[mn.fam] == null ? 9 : rank[mn.fam]) < (rank[best] == null ? 9 : rank[best])) best = mn.fam
    })
    return best
  }

  /**
   * The facet index for one node, memoised per session — the original
   * re-derived it per node per filter check (spec-script §G.1).
   *
   * @param {object} n @returns {Record<string, string[]>}
   */
  attrsOf(n) {
    const S = this.S()
    if (!this.memo.attrs || this.memo.attrs.key !== S.key) this.memo.attrs = { key: S.key, map: new Map() }
    const hit = this.memo.attrs.map.get(n.id)
    if (hit) return hit
    const a = attrs(n, S) || {}
    this.memo.attrs.map.set(n.id, a)
    return a
  }

  /**
   * Every facet value in the loaded record, with counts. Memoised on
   * `session.key`: this is the hottest path in the port.
   *
   * @returns {Record<string, Record<string, number>>}
   */
  facetOpts() {
    const S = this.S()
    if (this.memo.facets && this.memo.facets.key === S.key) return this.memo.facets.opts
    const opts = facetOptions(S) || {}
    this.memo.facets = { key: S.key, opts }
    return opts
  }

  /** @param {string} key @returns {object|null} */
  findingByKey(key) {
    const S = this.S()
    for (let i = 0; i < S.claims.length; i++) {
      const f = (S.claims[i].findings || []).find((x) => x.key === key)
      if (f) return f
    }
    return null
  }

  /** @returns {object[]} every claim's findings, in claim order */
  findings() {
    const out = []
    this.S().claims.forEach((c) => (c.findings || []).forEach((f) => out.push(f)))
    return out
  }

  /* ── Filtering (spec-script §B.5, §F) ────────────────────────────────── */

  /**
   * Parse one token against the loaded record. Grammar is exactly
   * `facet:value`, two aliases, no negation and no free text.
   *
   * @param {string} tok @returns {import('./adapter.js').Chip}
   */
  parseTok(tok) {
    return parseToken(tok, this.facetOpts())
  }

  /** @returns {Record<string, string[]>} the `ok` chips, bucketed by facet */
  activeFilters() {
    const by = {}
    this.state.filters.forEach((c) => {
      if (c.ok) (by[c.facet] = by[c.facet] || []).push(c.value)
    })
    return by
  }

  /**
   * OR within a facet, AND across facets. One predicate, called from both the
   * graph and `viewCount` — the original duplicated it inline.
   *
   * @param {object} n @param {import('./adapter.js').Chip[]} [chips]
   * @returns {boolean}
   */
  matches(n, chips) {
    return matchesNode(n, this.S(), chips || this.state.filters)
  }

  /**
   * Four dimming rules, first hit wins, composing as OR. DIMMING IS NEVER
   * HIDING: the one surface where a filter removes rows is the table, and even
   * there the footer keeps the honest denominator.
   *
   * @param {object} n @param {object} ctx from `ctx()`
   * @returns {boolean}
   */
  isDim(n, ctx) {
    const st = this.state
    if (st.findingsOnly && !ctx.fctx[n.id]) return true
    if (st.linked && st.sel && n.id !== st.sel && !ctx.linked[n.id]) return true
    if (ctx.hasFilters && !this.matches(n)) return true
    if (ctx.rseq != null) {
      /* Dim only what the replay has not reached AT ALL. A band the head is
       * standing inside is partly past, and drawing it as if it had not
       * happened is a false statement about the record. */
      if (this.replayState(n, ctx.rseq) === 'future') return true
    }
    return false
  }

  /**
   * The per-render dimming context, computed ONCE per render and threaded into
   * `isDim`.
   *
   * @returns {{fctx:object, linked:object, hasFilters:boolean, rseq:number|null}}
   */
  ctx() {
    const by = this.activeFilters()
    return {
      fctx: this.state.findingsOnly ? this.findingContext() : {},
      linked: this.state.linked ? this.linkedSet(this.state.sel) : {},
      hasFilters: Object.keys(by).length > 0,
      rseq: this.replaySeq(),
    }
  }

  /* ── Viewport (spec-script §B.6) ─────────────────────────────────────── */

  /**
   * The union box of a node set. Frames fold in only for the empty call — the
   * original's `nodes === this.visibleNodes()` compared against a freshly
   * allocated array and was always false (spec-script §G.3).
   *
   * @param {object[]} nodes
   * @returns {{x0:number,y0:number,x1:number,y1:number}}
   */
  bbox(nodes) {
    let b = null
    const add = (x, y, w, h) => {
      if (!b) b = { x0: x, y0: y, x1: x + w, y1: y + h }
      else {
        b.x0 = Math.min(b.x0, x)
        b.y0 = Math.min(b.y0, y)
        b.x1 = Math.max(b.x1, x + w)
        b.y1 = Math.max(b.y1, y + h)
      }
    }
    nodes.forEach((n) => {
      const g = this.geo(n)
      add(g.x, g.y, g.w, g.h)
    })
    if (!nodes.length) this.framesOf().forEach((f) => add(f.x, f.y, f.w, f.h))
    return b || { x0: 0, y0: 0, x1: 100, y1: 100 }
  }

  /**
   * Centre the view on a point of the graph named in OVERVIEW pixels. Shared by
   * the overview's press and its drag, so both use one mapping.
   *
   * @param {number} clientX @param {number} clientY
   * @param {{left:number,top:number,width:number,height:number}} rect
   */
  minimapTo(clientX, clientY, rect) {
    const mm = this.vals && this.vals.mm
    if (!mm || !rect || !rect.width || !rect.height) return
    const px = ((clientX - rect.left) / rect.width) * mm.w
    const py = ((clientY - rect.top) / rect.height) * mm.h
    const gx = mm.layout.fromX(px)
    const gy = mm.layout.fromY(py)
    const v = this.state.view
    this.setState({ view: { x: this.state.cw / 2 - gx * v.z, y: this.state.ch / 2 - gy * v.z, z: v.z } })
  }

  /**
   * Fit a node set into the viewport — AND SAY SO WHEN IT CANNOT (M3).
   *
   * What this used to do, on the measured session: compute the zoom that would
   * show all 1,481 cards (~1%), clamp it to a hardcoded 0.3 floor, centre on
   * the middle of a ~130,000 px column, leave 98.8% of the graph off-screen and
   * report nothing at all. A control that silently fails is worse than one that
   * refuses, so the maths moved to `viewport.fitView` — which keeps the
   * readable floor, but anchors an axis that cannot fit to the START of the
   * trace rather than its middle — and the refusal is spoken here, with a count
   * the reader can check against the card count above the canvas.
   *
   * @param {object[]} [nodes] @param {number} [minZ]
   */
  fit(nodes, minZ) {
    const st = this.state
    const list = nodes || this.visibleNodes()
    const b = this.bbox(list)
    const f = fitView(b, st.cw, st.ch, { minZ, floorZ: FIT_MIN_Z })
    this.setState({ view: { x: f.x, y: f.y, z: f.z } })
    if (!f.clamped) return
    const boxes = list.map((n) => this.geo(n))
    const on = countOnScreen(boxes, f, st.cw, st.ch)
    this.toast(
      'Fit stopped at ' +
        Math.round(f.z * 100) +
        '% — smaller than that, a card is not readable. ' +
        on +
        ' of ' +
        countPhrase(boxes.length, 'card', 'cards') +
        ' are on screen, at the start of the trace; the overview and find (⌘K) reach the rest.',
    )
    this.log('Fit could not show the whole trace at a readable size — showing ' + countPhrase(on, 'card', 'cards') + ' from the start')
  }

  /**
   * Start across the evidence lanes, at their beginning. Fitting a single
   * derived claim enlarged it to 140% and hid the surrounding graph on entry.
   * Long sessions extend vertically; the minimap and navigation retain access
   * to every record without compressing the entire session into tiny text.
   */
  initialView() {
    const st = this.state
    const b = this.bbox(this.visibleNodes())
    const flowing = FLOWING[st.traceDirection] === true
    const z = flowing ? .85 : Math.max(READABLE_Z, Math.min(.85, (st.cw - 32) / (b.x1 - b.x0 + 40)))
    this.setState({ view: flowing ? { x: 16, y: 16, z } : { x: 16 - b.x0 * z, y: 32 - b.y0 * z, z } })
  }

  /**
   * Centre one node, expanding its parent group first when it is hidden. The
   * post-commit read is a `setState` callback, not the original's `setTimeout(0)`.
   *
   * @param {string} id
   */
  centerOn(id) {
    const n = this.nodeById(id)
    if (!n) return
    const after = () => {
      const g = this.geo(n)
      const v = this.state.view
      this.setState({
        view: {
          x: this.state.cw / 2 - (g.x + g.w / 2) * v.z,
          y: this.state.ch / 2 - (g.y + g.h / 2) * v.z,
          z: v.z,
        },
      })
    }
    if (!this.visible(n) && n.member) this.setState(this.openGroup(n.member), after)
    else after()
  }

  /**
   * Zoom about a viewport point, clamped to `CLAMP.zoom`.
   *
   * @param {number} f @param {number} [mx] @param {number} [my]
   */
  zoomBy(f, mx, my) {
    const st = this.state
    const v = st.view
    const nz = Math.min(CLAMP.zoom[1], Math.max(CLAMP.zoom[0], v.z * f))
    const px = mx == null ? st.cw / 2 : mx
    const py = mx == null ? st.ch / 2 : my
    const gx = (px - v.x) / v.z
    const gy = (py - v.y) / v.z
    this.setState({ view: { x: px - gx * nz, y: py - gy * nz, z: nz } })
  }

  /* ── Selection (spec-script §B.7) ────────────────────────────────────── */

  /**
   * Select a node.
   *
   * @param {string} id
   * @param {{center?:boolean, doc?:boolean, tab?:string, dock?:boolean}} [opts]
   */
  select(id, opts) {
    const o = opts || {}
    const n = this.nodeById(id)
    if (!n) return
    this.stopReplay()
    const upd = { sel: id, findOpen: false, menu: null }
    // Selecting a source from a drawer or search must actually reveal it.
    // Migrate a legacy review representation into the evidence workspace.
    if (this.state.rep === 'review') {
      upd.rep = 'graph'
      upd.doc = 'trace'
      upd.drawer = null
      upd.panels = { ...this.state.panels, right: true, bottom: false }
    }
    if (n.member && !this.groupOpen(n.member)) Object.assign(upd, this.openGroup(n.member))
    if (n.finding) {
      upd.selFinding = n.finding
      if (o.dock !== false) upd.dockTab = 'findings'
    }
    if (o.doc) upd.doc = 'trace'
    if (o.tab) upd.inspTab = o.tab
    this.setState(upd, () => {
      if (o.center) this.centerOn(id)
      /* WINDOWING MUST NOT COST THE READER THEIR PLACE. A card reached by find,
       * by the palette or by a drawer may not have been mounted a moment ago;
       * `centerOn` brings it into the window on the next commit, and this puts
       * the keyboard on it once it is there. Requested here, performed in
       * `componentDidUpdate`, because the card does not exist until React has
       * committed the move. */
      if (o.center) this.focusNodeAfter = id
    })
  }

  /** @returns {number} index of the selected finding, -1 when absent */
  selFindingIdx() {
    return this.findings().findIndex((f) => f.key === this.state.selFinding)
  }

  /**
   * Next / previous finding. Wraps when the current selection is itself a
   * finding node; otherwise enters at the end the step came from.
   *
   * @param {number} d
   */
  stepFinding(d) {
    if (this.unreadToast()) return
    const fs = this.findings()
    if (!fs.length) {
      this.toast('No findings in this record.')
      return
    }
    const i = this.selFindingIdx()
    const sel = this.nodeById(this.state.sel)
    const onFinding = sel && sel.finding
    const ni = onFinding ? (i + d + fs.length) % fs.length : d > 0 ? Math.max(0, i) : i < 0 ? fs.length - 1 : i
    const f = fs[ni]
    this.setState({ selFinding: f.key, dockTab: 'findings' })
    this.select(f.node, { center: true, doc: true })
    this.log('Finding ' + (ni + 1) + ' of ' + fs.length + ' · ' + f.name)
  }

  /* ── Mouse & keyboard (spec-script §B.8, §D.1) ───────────────────────── */

  /**
   * Move the selected card by the arrow key's nudge (N13-DRAG-257, WCAG 2.2
   * SC 2.5.7: the single-pointer alternative to dragging it).
   *
   * @param {-1|0|1} dx @param {-1|0|1} dy
   */
  moveSel(dx, dy) {
    const n = this.nodeById(this.state.sel)
    if (!n) return
    const g = this.geo(n)
    this.setState({ pos: { ...this.state.pos, [n.id]: { x: g.x + dx * CLAMP.nodeStep, y: g.y + dy * CLAMP.nodeStep } } })
  }

  /**
   * The drag dispatcher. `this.drag` is deliberately outside state so a
   * mousemove does not re-render for its own bookkeeping.
   *
   * @param {MouseEvent} e
   */
  handleMove(e) {
    const d = this.drag
    if (!d) return
    const st = this.state
    if (d.type === 'pan') {
      this.setState({ view: { x: d.ox + (e.clientX - d.sx), y: d.oy + (e.clientY - d.sy), z: st.view.z } })
    } else if (d.type === 'node') {
      const dx = (e.clientX - d.sx) / st.view.z
      const dy = (e.clientY - d.sy) / st.view.z
      if (Math.abs(dx) + Math.abs(dy) > 1) d.moved = true
      this.setState({ pos: { ...st.pos, [d.id]: { x: Math.round(d.ox + dx), y: Math.round(d.oy + dy) } } })
    } else if (d.type === 'minimap') {
      this.minimapTo(e.clientX, e.clientY, d.rect)
    } else if (d.type === 'split') {
      const sz = { ...this.sizes() }
      if (d.which === 'left') sz.left = clamp(d.o + (e.clientX - d.sx), CLAMP.left)
      if (d.which === 'right') sz.right = clamp(d.o - (e.clientX - d.sx), CLAMP.right)
      if (d.which === 'bottom') sz.bottom = clamp(d.o - (e.clientY - d.sy), CLAMP.bottom)
      this.setState({ sizes: sz })
    }
  }

  /** End a drag: log a moved node, re-measure after a split, clear the descriptor. */
  handleUp() {
    const d = this.drag
    if (d && d.type === 'node' && d.moved) this.log('Moved node ' + d.id + ' — layout only; the record is unchanged')
    if (d && d.type === 'split') this.measure(true)
    this.drag = null
  }

  /**
   * The global key map. `⌘K`/`Ctrl+K` and `Escape` are handled BEFORE the
   * typing check, so they work from inside any input; every other binding
   * returns early while typing or while the palette is open.
   * `]` next finding · `[` previous finding · `f` fit · `p` proof ·
   * `.` replay forward · `,` replay back.
   *
   * @param {KeyboardEvent} e
   */
  handleKey(e) {
    const tag = (e.target && e.target.tagName) || ''
    const typing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
    if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault()
      this.openPalette()
      return
    }
    if (e.key === 'Escape') {
      const st = this.state
      if (st.palette || st.menu || st.drawer || st.findOpen) {
        this.setState({ palette: false, menu: null, drawer: null, findOpen: false })
      }
      return
    }
    if (typing || this.state.palette) return
    /* T-6-4: while the key list or Ask is open it is modal; nothing behind it moves. */
    if (typeof document !== 'undefined' && document.querySelector && document.querySelector('.ws-shell-dialog[aria-modal="true"]')) return
    /* P6-1: `F` and `.` belong to the §6 map now — the thread and the one
     * primary action — and are answered by the shell's one handler
     * (`altitude.js`). They used to fit the view, show the proof and step
     * forward here; those three acts are the Fit, Show me the proof and Step
     * forward buttons, all still drawn in Workbench. `P` chose the third
     * altitude until A3-WORKBENCH moved the altitudes to 1 / 2 / 3; it is
     * nobody's key now. */
    /* N13-KEYS-214 (WCAG 2.2 SC 2.1.4): [ ] and comma are single-character
     * keys too, and answer to the one switch in the key list. */
    if (!singleKeysOn(typeof window === 'undefined' ? null : window)) return
    if (e.key === ']') this.stepFinding(1)
    else if (e.key === '[') this.stepFinding(-1)
    else if (e.key === ',') this.step(-1)
  }

  /* ── Replay — a scrub over recorded order; nothing executes (§B.9) ────── */

  /**
   * Move the replay cursor.
   *
   * @param {number} d
   */
  step(d) {
    if (this.unreadToast()) return
    const ev = this.S().events
    if (!ev.length) {
      this.toast(this.S().eventsNote || 'No recorded events in this reading.')
      return
    }
    const i = this.state.replay.idx
    const ni = Math.max(0, Math.min(ev.length - 1, i < 0 ? (d > 0 ? 0 : ev.length - 1) : i + d))
    this.setReplay(ni, this.state.replay.playing)
  }

  /**
   * Place the replay cursor and follow it with the selection.
   *
   * @param {number} i @param {boolean} playing
   */
  setReplay(i, playing) {
    const ev = this.S().events
    const e = ev[i]
    this.setState({
      replay: { idx: i, playing: !!playing },
      eventsFocusSeq: e ? e.seq : null,
    })
    if (!e) return
    const n = this.nodeBySeq(e.seq)
    /* THE HEAD IS FOLLOWED, NOT JUST SELECTED. This set `sel` and moved nothing,
     * so on a graph ~130,000 px tall the replay head walked off-screen on its
     * second step and the only sign of where it went was one yellow rect in an
     * overview that drew nothing. Centring is arithmetic on the model
     * (`centerOn`), so it is correct whether or not the card is mounted — and
     * it is what makes the head reachable under a window. */
    if (n) this.setState({ sel: n.id }, () => this.centerOn(n.id))
    this.log('Replay · seq ' + e.seq + ' · ' + (KIND_LABEL[e.kind] || e.kind) + ' · ' + e.text, C.link)
  }

  /** Start or pause the 900ms replay interval. */
  toggleReplay() {
    if (this.unreadToast()) return
    const ev = this.S().events
    if (!ev.length) {
      this.toast(this.S().eventsNote || 'No recorded events to replay in this reading.')
      return
    }
    if (this.state.replay.playing) {
      this.stopReplay()
      this.log('Replay paused')
      return
    }
    const st = this.state.replay.idx
    const start = st < 0 || st >= ev.length - 1 ? -1 : st
    this.setState({ replay: { idx: start, playing: true } })
    this.log('Replay started — scrubbing recorded order; nothing is executed', C.link)
    this.replayTimer = setInterval(() => {
      const i = this.state.replay.idx + 1
      if (i >= ev.length) {
        this.stopReplay()
        this.log('Replay reached the last recorded event')
        return
      }
      this.setReplay(i, true)
    }, 900)
  }

  /* ── The lit thread, the guided walk, the scrubber (P6-3, §3.3) ─────── */

  /**
   * The thread of the selected card, over the record's own wires
   * (`trace-view.threadOf`), memoised on the session and the selection. Null
   * with nothing selected, or when the selected card is linked to nothing.
   *
   * @returns {ReturnType<typeof threadOf>}
   */
  thread() {
    const S = this.S()
    const sel = this.state.sel
    const key = S.key + '|' + (sel || '') + '|' + S.nodes.length + '|' + S.wires.length
    if (this.memo.thread && this.memo.thread.key === key) return this.memo.thread.t
    const t = sel ? threadOf(S, sel) : null
    this.memo.thread = { key, t }
    return t
  }

  /**
   * The ids the picture lights: every thread card, resolved to the card that
   * is actually drawn for it (a member of a closed group lights its group).
   *
   * @param {ReturnType<typeof threadOf>} t
   * @returns {{nodes: Record<string, number>, wires: Record<string, boolean>}}
   */
  litOf(t) {
    const nodes = {}
    const wires = {}
    if (!t) return { nodes, wires }
    t.ids.forEach((id, i) => {
      const n = this.resolveEnd(id)
      if (n && nodes[n.id] == null) nodes[n.id] = i
    })
    t.wires.forEach((id) => (wires[id] = true))
    return { nodes, wires }
  }

  /** The Plain face is the manager register; Technical is the operator's. */
  face() {
    return this.state.register === 'operator' ? 'technical' : 'plain'
  }

  /** A card's title in the face showing, for a spoken walk step. */
  titleIn(n) {
    if (!n) return ''
    return this.face() === 'technical' ? String(n.title || '') : plainFace(n).title
  }

  /**
   * F — focus the thread: fit the lit thread on screen. With nothing lit, the
   * current discrepancy's thread (the worst claim's) is selected and fitted.
   * Returns the sentence the top bar says.
   *
   * @returns {string}
   */
  focusThread() {
    if (this.unread()) return HQ_COPY.trace.unread.action
    const TT = HQ_COPY.trace.thread
    const S = this.S()
    let t = this.thread()
    let sel = this.state.sel
    if (!t) {
      const a = discrepancyAnchor(S)
      const at = a ? threadOf(S, a) : null
      if (!at) return sel ? TT.none : TT.none_focus
      t = at
      sel = a
    }
    const cards = t.ids.map((id) => this.resolveEnd(id)).filter(Boolean)
    const go = () => {
      /* T-6-4: focus moves with the selection F makes. */
      const at = this.resolveEnd(sel)
      if (at) this.focusNodeAfter = at.id
      this.fit(cards)
      this.log('Focus thread · ' + threadSteps(t.ids.length))
    }
    this.setState({ sel, doc: 'trace', rep: 'graph', outline: false, ...this.openFor(t.ids) }, go)
    return fill(TT.focused, { steps: threadSteps(t.ids.length) })
  }

  /** The state change that opens every group hiding one of these ids. */
  openFor(ids) {
    const st = this.state
    const expanded = { ...st.expanded }
    const focusExpanded = { ...st.focusExpanded }
    ids.forEach((id) => {
      const n = this.nodeById(id)
      if (!n || !n.member || this.groupOpen(n.member)) return
      if (isFocusGroupId(n.member)) focusExpanded[n.member] = true
      else expanded[n.member] = true
    })
    return { expanded, focusExpanded }
  }

  /**
   * J / K — one step along the thread, or along record order when nothing is
   * lit. Never wraps; at an end it stays and says so. Returns the sentence the
   * top bar says.
   *
   * @param {1|-1} dir @returns {string}
   */
  walk(dir) {
    const W = HQ_COPY.trace.walk
    const t = this.thread()
    const order = walkOrder(t, this.S().nodes)
    const st = walkStep(order.ids, this.state.sel, dir)
    if (!st) return W.none
    if (st.edge) return st.edge === 'end' ? W.end : W.start
    const n = this.nodeById(st.id)
    this.select(st.id, { center: true, doc: true })
    return fill(order.along === 'thread' ? W.thread : W.record, { i: st.index + 1, n: st.total, title: this.titleIn(n) })
  }

  /**
   * "Walk me through it": start at the first step of the thread — the one
   * lit, else the current discrepancy's — else at the first recorded step.
   */
  startWalk() {
    if (this.unreadToast()) return
    const W = HQ_COPY.trace.walk
    const S = this.S()
    let t = this.thread()
    if (!t) {
      const a = discrepancyAnchor(S)
      t = a ? threadOf(S, a) : null
    }
    const order = walkOrder(t, S.nodes)
    if (!order.ids.length) {
      this.toast(W.none)
      return
    }
    const first = order.ids[0]
    const lit = order.along === 'thread' ? t : null
    this.setState({ outline: false, doc: 'trace', rep: 'graph', ...(lit ? this.openFor(lit.ids) : {}) }, () => this.select(first, { center: true, doc: true }))
    this.toast(fill(order.along === 'thread' ? W.thread : W.record, { i: 1, n: order.ids.length, title: this.titleIn(this.nodeById(first)) }))
  }

  /**
   * What the thread bar over the picture says while a thread is lit: how many
   * steps, which stages the record links nothing to, and the way back.
   *
   * @returns {null | {line: string, missing: string[], showAll: string, showAllHint: string}}
   */
  threadBar() {
    const t = this.thread()
    if (!t) return null
    const TT = HQ_COPY.trace.thread
    return { line: fill(TT.bar, { steps: threadSteps(t.ids.length) }), missing: missingLines(t), showAll: TT.show_all, showAllHint: TT.show_all_hint }
  }

  /**
   * The scrubber's reading: where the head is, in the face showing.
   *
   * @returns {{max: number, value: number, text: string, disabled: boolean}}
   */
  scrubView() {
    const SC = HQ_COPY.trace.scrub
    const ev = this.S().events
    const n = ev.length
    if (!n) return { max: 0, value: 0, text: SC.none, disabled: true }
    const idx = this.state.replay.idx
    if (idx < 0) return { max: n, value: n, text: fill(SC.whole, { steps: threadSteps(n) }), disabled: false }
    const e = ev[Math.min(idx, n - 1)]
    const at = this.face() === 'plain' ? fill(HQ_COPY.trace.face.step, { n: e.seq }) : fill(HQ_COPY.trace.technical.seq, { n: e.seq })
    return { max: n, value: idx, text: fill(SC.at, { step: at, i: idx + 1, n }), disabled: false }
  }

  /** Esc, or "Show everything": nothing selected, so nothing is lit or dimmed. */
  clearThread() {
    const st = this.state
    /* Esc closes an open palette, menu, drawer or find list first (the
     * workspace's own handler); only an Esc with nothing open lets go of the
     * thread. */
    if (st.palette || st.menu || st.drawer || st.findOpen) return
    if (!st.sel) return
    /* T-6-4: focus goes back to where it was before the thread lit — or,
     * when that was nowhere (the page itself) or is gone, to the one tab stop
     * of what is showing, so a keyboard reader is never dropped to the top. */
    const back = this.focusBeforeThread
    this.focusBeforeThread = null
    this.setState({ sel: null, hoverWire: null }, () => this.restoreFocus(back))
  }

  /**
   * Where focus is now, remembered so it can be given back: the element, and
   * — when it is a card or an Outline row — which card, because the picture
   * draws only the cards near the view and may redraw that card as a new
   * element by the time focus goes back to it.
   *
   * @returns {null|{el: Element, id: string|null}}
   */
  focusMark() {
    if (typeof document === 'undefined') return null
    const el = document.activeElement
    if (!el) return null
    const card = typeof el.matches === 'function' && el.matches('[role="application"] [role="button"][data-id], .ws-trace-outline-item[data-id]')
    return { el, id: card ? el.getAttribute('data-id') : null }
  }

  /** Put focus back on a remembered place if it is still on the page, else on
   *  the same card where it is drawn now, else on the tab stop of the Outline
   *  or the canvas. */
  restoreFocus(mark) {
    if (typeof document === 'undefined') return
    const el = mark && mark.el
    if (el && el !== document.body && el.isConnected && typeof el.focus === 'function') {
      el.focus()
      return
    }
    const again = mark && mark.id ? this.cardElement(mark.id) : null
    if (again && typeof again.focus === 'function') {
      again.focus()
      return
    }
    const root = this.rootRef.current
    if (!root || typeof root.querySelector !== 'function') return
    const stop = (this.state.outline && root.querySelector('.ws-trace-outline [data-section="all"] [tabindex="0"]')) ||
      (this.canvasRef.current && this.canvasRef.current.querySelector('[role="button"][data-id][tabindex="0"]'))
    if (stop && typeof stop.focus === 'function') stop.focus()
  }

  /**
   * What the Outline lists: the drawn cards with what each closed group
   * folds inside it, and every line the picture draws — the record's own and
   * the recorded-order lines the picture adds — so each is named by its kind.
   *
   * @returns {object} a Session-shaped value for `outlineTree` / `outlineRows`
   */
  outlineSession() {
    const S = this.S()
    const order = this.allWires()
      .filter((w) => w.kind === 'order')
      .map((w) => ({ id: w.id, from: [w.a], to: [w.b], kind: 'order' }))
    return { ...S, nodes: this.G().nodes, wires: (S.wires || []).concat(order) }
  }

  /**
   * The scrubber: one range over record order. The last position is the
   * whole record (no step dimmed); any other places the replay head there.
   *
   * @param {number} i
   */
  scrubTo(i) {
    const ev = this.S().events
    this.stopReplay()
    if (!ev.length) return
    if (i >= ev.length) {
      this.setState({ replay: { idx: -1, playing: false }, eventsFocusSeq: null })
      return
    }
    this.setReplay(Math.max(0, i), false)
  }

  /* ── Actions (spec-script §B.10) ─────────────────────────────────────── */

  /** "Show me the proof": evidence tab, linked highlighting, centred. */
  proof() {
    const st = this.state
    const n = this.nodeById(st.sel)
    if (!n) return
    this.setState({ inspTab: 'evidence', linked: true, doc: 'trace' })
    this.centerOn(st.sel)
    this.log('Show me the proof · ' + (n.objId || n.id) + ' — linked evidence highlighted')
  }

  /**
   * "Show proof →" from a Verdict card (Step 5): enter Focus with that
   * object's chain — the object and everything within `FOCUS_DEPTH` hops over
   * `ref`/`inf` wires — outlined in amber, every focus group holding a chain
   * member opened in place, and the object selected and centred.
   *
   * @param {string} id
   */
  showProof(id) {
    const S = this.S()
    const chain = chainOf(S.nodes, S.wires, id, FOCUS_DEPTH)
    if (!chain.length) {
      this.toast('That object is not in this reading.')
      return
    }
    const inChain = {}
    chain.forEach((c) => (inChain[c] = true))
    const cap = S.capture || {}
    const p = focusProjection(S.nodes, S.wires, { focusMode: 'focus', focusExpanded: {}, depth: FOCUS_DEPTH, degraded: cap.degraded === true })
    const focusExpanded = { ...this.state.focusExpanded }
    p.groups.forEach((g) => {
      if (g.members.some((m) => inChain[m])) focusExpanded[g.id] = true
    })
    this.setState({ focusMode: 'focus', focusChain: chain, focusExpanded, linked: true }, () => this.select(id, { center: true, doc: true }))
    this.log('Show proof · ' + id + ' — Focus, chain of ' + countPhrase(chain.length, 'object', 'objects') + ' outlined')
  }

  /**
   * Focus / Everything (Step 5). Focus is a VIEW: nothing about the record,
   * its cards, colours or edges changes. The chain outline belongs to the
   * entry that made it, so a mode change clears it.
   *
   * @param {'focus'|'everything'} mode
   */
  setFocusMode(mode) {
    const next = mode === 'everything' ? 'everything' : 'focus'
    if (next === this.state.focusMode) return
    this.memo.flow = null
    this.memo.graph = null
    this.setState({ focusMode: next, focusChain: null, menu: null }, () => this.fit())
    this.log(next === 'focus' ? 'Focus — what is not proven, and its chain; the rest merged and counted' : 'Everything — the full graph')
  }

  /** "Show surrounding events": the events pane, focused on the selection's seq. */
  surrounding() {
    const n = this.nodeById(this.state.sel)
    const seq = n ? this.placedSeq(n) : null
    this.setState({
      dockTab: 'events',
      eventsFocusSeq: seq,
      panels: { ...this.state.panels, bottom: true },
    })
    this.log('Show surrounding events' + (seq == null ? '' : ' · around seq ' + seq))
  }

  /**
   * The copyable receipt path back to the record: object id · session · span ·
   * tree · chain head · origin.
   *
   * @param {object} n @returns {string}
   */
  refText(n) {
    const S = this.S()
    const ot = objType(n)
    const parts = []
    parts.push(n.objId || (n.seq != null ? 'seq ' + n.seq : n.id))
    parts.push('session ' + (S.id || S.label))
    if (ot === 'claim' && S.claims[0]) {
      const c = S.claims[0]
      parts.push(
        'event ' + (c.eventId || 'not exported') + (c.seq != null ? ' · seq ' + c.seq : '') + (c.offset ? ' · chars ' + c.offset.join('–') : ''),
      )
    }
    if (S.finalTree) parts.push('tree ' + S.finalTree)
    if (S.chainHead) parts.push('chain head ' + S.chainHead)
    if (S.origin) parts.push(S.origin)
    return parts.join(' · ')
  }

  /**
   * THE PRIMARY ACTION — line the claim up against the check.
   *
   * The design called it `Find the discrepancy` and asserted one existed before
   * the reader had read anything, on the honest session too (spec-doctrine
   * F-19). This puts the claim and its check side by side and says so; whether
   * they disagree is the reader's to see.
   *
   * The set it fits is derived from the record — the claim, its check, its
   * findings and everything wired to them. The design fitted a hardcoded id
   * list (spec-script §G.2).
   */
  lineUp() {
    if (this.unreadToast()) return
    const S = this.S()
    if (!S.claims[0]) {
      this.toast('No completion claim in this session — nothing to reconcile.')
      this.log('Claim and check, side by side · no completion claim in ' + S.label)
      return
    }
    const c = S.claims[0]
    this.setState({
      doc: 'trace',
      rep: 'graph',
      inspTab: 'evidence',
      linked: true,
      dockTab: 'findings',
      selFinding: c.findings && c.findings[0] ? c.findings[0].key : null,
      findingsOnly: false,
      panels: { ...this.state.panels, right: true, bottom: true },
    })
    this.select(c.node, { center: true })
    const wanted = {}
    if (c.node) wanted[c.node] = true
    if (S.checks[0] && S.checks[0].node) wanted[S.checks[0].node] = true
    ;(c.findings || []).forEach((f) => {
      if (f.node) wanted[f.node] = true
    })
    Object.keys({ ...this.linkedSet(c.node), ...this.linkedSet(S.checks[0] ? S.checks[0].node : null) }).forEach(
      (k) => (wanted[k] = true),
    )
    const ns = Object.keys(wanted)
      .map((id) => this.nodeById(id))
      .filter((n) => n && this.visible(n))
    if (ns.length > 1) this.fit(ns, 0.85)
    this.log(
      'Claim and check, side by side · claim “' +
        c.text +
        '” vs check ' +
        (S.checks[0] ? S.checks[0].command || S.checks[0].id : 'not in this reading') +
        ' · ' +
        countPhrase((c.findings || []).length, 'finding', 'findings'),
      C.amber,
    )
  }

  /** Copy the claim's next check command, or say why there is none. */
  copyNext() {
    if (this.unreadToast()) return
    const c = this.S().claims[0]
    if (c && c.nextCheck) this.copy(c.nextCheck, 'next check command')
    else this.toast(c ? nextCheckAbsent(c) + '.' : 'No claim to check against.')
  }

  /** Select the recorded stop event; say so plainly when there is none. */
  jumpStop() {
    if (this.unreadToast()) return
    const S = this.S()
    const stopN = S.nodes.find((n) => n.seq != null && n.seq === S.stop.seq && n.type === 'EVENT')
    if (S.stop.seq == null || !stopN) {
      if (this.nodeById('ending')) this.select('ending', { center: true, doc: true })
      this.toast('No ending recorded')
      this.log('Jump to stop · no ending recorded for ' + S.label)
      return
    }
    this.select(stopN.id, { center: true, doc: true })
    this.log('Jump to stop · seq ' + S.stop.seq + ' · ' + S.stop.kind)
  }

  /**
   * Re-read. The design's `refresh()` read nothing — an 1100ms timeout that
   * logged "nothing changed" (spec-script §G.4). This asks the server to
   * refresh, polls `/api/state` until it stops, re-reads, and reports what
   * actually changed INCLUDING a failure.
   *
   * NOTE for `data.js`: `docs/UI-SERVER-CONTRACT.md:183` makes `/api/refresh` a
   * POST returning 202, polled through `/api/state`. `readProjection` is the
   * only door — `fetch` may not appear in this file (CONTRACT §6).
   */
  async refresh() {
    if (this.props.localVerification) { await this.loadData(); return }
    if (this.state.reading.status === 'refreshing') return
    this.setState({ reading: { status: 'refreshing', at: null } })
    this.log('Refresh reading · asking the local app to re-read the ledger…')
    const before = this.S()
    const beforeCount = before.eventCount
    try {
      const started = await readProjection(API.refresh)
      if (!this.live) return
      /* Anything that is not an outright READY read is a failure here. A Read
       * that is merely `idle` must never reach the success toast: saying
       * "Reading refreshed" without having re-read is exactly the class of
       * unbacked claim this product exists to catch (Rule 12). */
      if (!started || started.status !== 'ready') {
        const why =
          (started && started.refusal) ||
          'the local app did not answer ' + API.refresh
        this.setState({ reading: { status: 'idle', at: null } })
        this.log('Refresh reading failed · ' + why, C.danger)
        this.toast('Could not refresh — ' + why + '.')
        return
      }
      for (let i = 0; i < 30; i++) {
        const state = await readProjection(API.state)
        if (!this.live) return
        if (!state || state.status !== 'ready') break
        if (!state.doc || state.doc.refreshing !== true) break
        await wait(400)
        if (!this.live) return
      }
      await this.loadData()
      if (!this.live) return
      const after = this.S()
      const changed =
        after.eventCount !== beforeCount
          ? countPhrase(after.eventCount, 'event', 'events') + ' now recorded — was ' + beforeCount
          : 'nothing changed'
      /* The time itself was stamped by `loadData` when its read completed. */
      this.setState({ reading: { status: 'done', at: null } })
      this.log('Reading refreshed · ' + after.label + ' · ' + changed, C.green)
      this.toast('Reading refreshed — ' + changed + '.')
    } catch (e) {
      if (!this.live) return
      this.setState({ reading: { status: 'idle', at: null } })
      this.log('Refresh reading failed · ' + String((e && e.message) || e).slice(0, 200), C.danger)
      this.toast('Could not refresh — the local app did not answer.')
    }
  }

  /**
   * Open a session the picker offered but this reading has not adopted yet.
   *
   * `switchSession`'s `if (!S) return` was unreachable while the corpus held
   * every session the switcher could name. The picker now offers ADDRESSES, so
   * that guard became the normal path, and a silent `return` there would mean
   * clicking a session does nothing and says nothing (Rule 12). This is that
   * path, and both of its outcomes are visible: in flight, through the shipped
   * in-flight vocabulary; refused, through the same toast-and-log pair
   * `refresh()` uses, with the previously-open session left open.
   *
   * @param {string} key a session id from the LISTING — never one composed here
   */
  async openListedSession(key) {
    const epoch = ++this.switchEpoch
    this.setState({ switching: key, menu: null, readFor: key })
    this.log('Reading ' + API.explain + '?session=' + key + '…')
    let read
    let claims
    try {
      ;[read, claims] = await Promise.all([
        readProjection(API.explain + '?session=' + encodeURIComponent(key)),
        this.readClaimsFor(key, () => this.live && epoch === this.switchEpoch),
      ])
    } catch (e) {
      read = { status: 'refused', refusal: String((e && e.message) || e).slice(0, 200) }
    }
    if (!this.live || epoch !== this.switchEpoch) return
    if (!read || read.status !== 'ready' || !read.doc) {
      const why = (read && read.refusal) || 'the local app did not answer ' + API.explain
      this.setState({ switching: '' })
      this.log('That session could not be opened · ' + why, C.danger)
      this.toast('That session could not be opened — ' + why + '.')
      return
    }
    let session
    try {
      const rows = (this.state.sessions && this.state.sessions.rows) || []
      const entry = rows.find((row) => row.id === key) || null
      session = this.adapt(read, null, entry, claims)
    } catch (e) {
      this.setState({ switching: '' })
      const why = e && e.name === 'UnknownShape' ? e.message : 'the explain projection could not be adapted'
      this.log('That session could not be opened · ' + why, C.danger)
      this.toast('That session could not be opened — ' + why + '.')
      return
    }
    if (!this.live || epoch !== this.switchEpoch) return
    /* The adapter keys the session by what the DOCUMENT says, which for an
     * empty document is not the id that was asked for. The corpus is keyed by
     * the address, so the switch lands on the row the reader clicked. */
    const adopted = { ...session, key }
    this.setState({ corpus: this.mergeCorpus(adopted), switching: '', lastReadAt: Date.now() }, () => this.switchSession(key))
  }

  /**
   * Open a different session. Sixteen fields reset; filters, saved views and
   * columns deliberately survive.
   *
   * @param {string} key
   */
  switchSession(key) {
    const S = this.state.corpus.sessions[key]
    /* Not in the corpus means not read yet, which is the picker's normal case
     * now that it lists addresses. It is read, then this method runs again. */
    if (!S) {
      if (key && key !== this.state.switching) this.openListedSession(key)
      return
    }
    this.writeSession(key)
    this.stopReplay()
    this.memo.attrs = null
    this.memo.facets = null
    /* The id index, the column re-stack and the forced-member set are all
     * per-session too; a switch that left them behind would place the new
     * session's cards at the old one's boxes. */
    this.memo.index = null
    this.memo.flow = null
    this.memo.forced = null
    this.setState(
      {
        session: key,
        sel: null,
        selFinding: S.claims[0] && S.claims[0].findings && S.claims[0].findings[0] ? S.claims[0].findings[0].key : null,
        expanded: {},
        pos: {},
        widen: {},
        replay: { idx: -1, playing: false },
        linked: false,
        findingsOnly: false,
        /* Focus on every open (ruled): a session switch re-enters Focus. */
        focusMode: 'focus',
        focusExpanded: {},
        focusChain: null,
        menu: null,
        drawer: null,
        doc: 'trace',
        eventsFocusSeq: null,
        inspTab: 'details',
        dockTab: 'findings',
      },
      () => this.initialView(),
    )
    this.log('Switched to session ' + S.label + (S.origin ? ' · ' + S.origin : ''))
  }

  /**
   * Open a pinned document tab.
   *
   * @param {string} id `'claim'` | `'compare'`
   */
  openDoc(id) {
    const docs = this.state.docs.slice()
    if (!docs.some((d) => d.id === id)) docs.push({ id, session: this.state.session })
    const compare = this.state.compare
    const other = this.state.corpus.order.find(key => key !== compare.a)
    this.setState({ docs, doc: id, drawer: null, menu: null,
      ...(id === 'compare' && compare.a === compare.b && other ? { compare: { a: compare.a, b: other } } : {}),
    })
  }

  /**
   * Close one document tab, falling back to Trace when it was active.
   *
   * @param {string} id
   */
  closeDoc(id) {
    this.setState((s) => ({ docs: s.docs.filter((d) => d.id !== id), doc: s.doc === id ? 'trace' : s.doc }))
  }

  /**
   * Add one filter token. An unsupported token is KEPT as a visible chip and
   * changes nothing about what is shown — a visible record of a request the
   * record could not honour (Rule 12, spec-script §F.3).
   *
   * @param {string} tok
   */
  addFilter(tok) {
    const c = this.parseTok(String(tok).trim())
    if (!c.text) return
    const fs = this.state.filters.filter((x) => x.text !== c.text)
    fs.push(c)
    this.setState({ filters: fs, filterText: '', menu: null })
    this.log('Filter ' + c.text + (c.ok ? '' : ' — not a filter this build reads: ' + c.why), c.ok ? undefined : C.danger)
  }

  /**
   * Apply a saved view: REPLACES the chip list.
   *
   * @param {{name:string, tokens:string[]}} v
   */
  applyView(v) {
    this.setState({ filters: v.tokens.map((t) => this.parseTok(t)), findingsOnly: false })
    this.log('Saved view · ' + v.name + ' · ' + v.tokens.join(' '))
  }

  /**
   * THE PROMOTED FINDING (DESIGN-readable-trace §5).
   *
   * `crates/explain/src/timeline.rs:26-38` is a hard-coded 13-entry list and
   * `risk_of` lowercases a haystack and returns the FIRST substring match:
   * fully deterministic, no model, no threshold. This composes what that
   * detector found into one advisory row, and every word is governed by
   * doctrine rule 5 — a statement about the record, never an accusation, never
   * proof of falsehood.
   *
   * Three things it deliberately does NOT say. It does not say anything was
   * removed: `timeline_of` fills the haystack from the whole serialized input
   * string, so a pattern can also match inside a description or a here-doc,
   * and the surface must not word a substring match as proof of a destructive
   * act. It does not carry a state word: the ruling that maps states to words
   * is the only source of truth for that mapping and it does not rule a word
   * for this, so the row under-claims on purpose and carries its
   * `What would prove it:` clause instead. And it names no number without its
   * noun.
   *
   * @returns {{head:string, basis:string, clause:string, action:string}|null}
   */
  riskNotice() {
    const S = this.S()
    const flagged = S.events.filter((e) => e.risk)
    if (!flagged.length) return null
    const byPattern = new Map()
    flagged.forEach((e) => {
      const m = /`([^`]+)`/.exec(String(e.risk))
      const k = m ? m[1] : String(e.risk)
      byPattern.set(k, (byPattern.get(k) || 0) + 1)
    })
    const pats = Array.from(byPattern.entries()).sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
    const basisParts = []
    if (pats.length === 1) basisParts.push('All ' + flagged.length + ' matched `' + pats[0][0] + '`.')
    else basisParts.push(pats.map(([k, n]) => countPhrase(n, 'matched `' + k + '`', 'matched `' + k + '`')).join(' · ') + '.')
    /* A display cut and a record cut are different facts. The transport cut
     * every command line before this reading, and these are the cards where
     * that matters most, so it is stated here rather than 133 times. */
    const shortened = flagged.filter((e) => /…\s*$/.test(String(e.text))).length
    if (shortened === flagged.length) {
      basisParts.push('Every one of these command lines was shortened before this reading, so the pattern matched more of the input than the card shows.')
    } else if (shortened) {
      basisParts.push(countPhrase(shortened, 'of these command lines was', 'of these command lines were') + ' shortened before this reading, so the pattern matched more of the input than the card shows.')
    }
    return {
      head: countPhrase(flagged.length, 'command', 'commands') + ' matched a destructive pattern in the recorded input.',
      basis: basisParts.join(' '),
      clause: 'What would prove it: open them and read the paths they name.',
      action: 'Show them',
    }
  }

  /** @returns {Array<{name:string, tokens:string[]}>} the four built-in views */
  builtinViews() {
    return [
      { name: 'Not proven', tokens: ['word:not-proven'] },
      { name: 'Loose threads', tokens: ['word:loose-thread'] },
      { name: 'Not bound to final tree', tokens: ['binding:not-bound'] },
      { name: 'Destructive commands', tokens: ['risk:destructive'] },
    ]
  }

  /**
   * What a saved view would show. A view holding ANY unsupported token reports
   * `none in this record` rather than silently applying its valid half.
   *
   * @param {{tokens:string[]}} v @returns {string}
   */
  viewCount(v) {
    /* TRACE-MISSING-SESSION-TRUE (D-332): an unread session has no count. */
    if (this.unread()) return HQ_COPY.trace.unread.tab
    const toks = v.tokens.map((t) => this.parseTok(t))
    if (!toks.every((t) => t.ok)) return 'none in this record'
    /* ONE POPULATION RULE (DESIGN-readable-trace §2.3), the same rule the match
     * line and the records table already keep: `!x.synthetic`, never `!x.group`.
     * A step row IS a recorded `tool_call` that absorbed its result, so
     * excluding it for carrying members made this label disagree with the view
     * it describes — `Destructive commands · 2 objects` on a session where
     * applying it lit 133. Only the one card this app synthesised, the
     * working-tree group, is not an object of the record. */
    const n = this.S().nodes.filter((x) => !x.synthetic && this.matches(x, toks)).length
    return n ? countPhrase(n, 'object', 'objects') : 'none in this record'
  }

  /**
   * Position the one generic menu from its trigger, measured against the ROOT
   * REF — not `closest('[data-screen-label]')`, a Claude-Design editor
   * attribute that does not survive the port (spec-script §E6).
   *
   * @param {string} kind @param {MouseEvent} [e]
   */
  openMenu(kind, e) {
    const same = this.state.menu === kind
    let x = this.state.menuX
    let y = this.state.menuY
    const t = e && e.currentTarget && e.currentTarget.getBoundingClientRect ? e.currentTarget.getBoundingClientRect() : null
    const root = this.rootRef.current ? this.rootRef.current.getBoundingClientRect() : { left: 0, top: 0 }
    if (t) {
      x = t.left - root.left
      y = t.bottom - root.top + 4
      if (typeof window !== 'undefined' && x + 300 > window.innerWidth) x = Math.max(8, window.innerWidth - 320)
    }
    this.setState({ menu: same ? null : kind, menuX: x, menuY: y, findOpen: false })
    this.menuTrigger = same ? null : (e && e.currentTarget) || null
  }

  /** Close the menu and return focus to the control that opened it (Decision D6). */
  closeMenu() {
    const t = this.menuTrigger
    this.menuTrigger = null
    this.setState({ menu: null }, () => {
      if (t && t.focus) t.focus()
    })
  }

  /** Open the palette and focus its input. */
  openPalette() {
    this.paletteTrigger = typeof document !== 'undefined' ? document.activeElement : null
    this.setState({ palette: true, palQ: '', palIdx: 0, menu: null }, () => {
      const t = setTimeout(() => {
        if (this.palRef.current) this.palRef.current.focus()
      }, 30)
      this.mountTimers.push(t)
    })
  }

  /** Close the palette and return focus to whatever opened it. */
  closePalette() {
    const t = this.paletteTrigger
    this.paletteTrigger = null
    this.setState({ palette: false }, () => {
      if (t && t.focus) t.focus()
    })
  }

  /** Back to the default composition: sizes, panels, positions, expansion. */
  resetLayout() {
    this.setState(
      {
        sizes: null,
        panels: { left: !this.state.narrow, right: !this.state.narrow, bottom: true, minimap: true },
        pos: {},
        widen: {},
        expanded: {},
        focusPrev: null,
        doc: 'trace',
      },
      () => {
        this.measure(true)
        this.initialView()
      },
    )
    this.log('Layout reset to the default composition')
  }

  /** Toggle focus-canvas mode, stashing and restoring the panel composition. */
  focusCanvas() {
    if (this.state.focusPrev) this.setState({ panels: this.state.focusPrev, focusPrev: null }, () => this.measure(true))
    else
      this.setState(
        { focusPrev: this.state.panels, panels: { left: false, right: false, bottom: false, minimap: true } },
        () => this.measure(true),
      )
  }

  /**
   * Set or toggle one panel. Always clears `focusPrev` — a panel the reader
   * opened by hand is no longer a stashed composition.
   *
   * @param {'left'|'right'|'bottom'|'minimap'} k @param {boolean} [v]
   */
  setPanel(k, v) {
    const p = { ...this.state.panels }
    p[k] = v == null ? !p[k] : v
    this.setState({ panels: p, ...(k === 'bottom' ? { bottomPref: p[k] } : {}), focusPrev: null }, () => this.measure(true))
  }

  /**
   * The execution store this reading came from. The design hardcoded one
   * absolute path from the author's own machine (spec-script §G.2); a path the
   * record does not carry is NAMED as absent, never invented.
   *
   * @returns {string}
   */
  dbPath() {
    const S = this.S()
    return S.execDb || (S.capture && S.capture.execDb) || '<exec db path — not in this reading>'
  }

  /**
   * Where this reading came from. The decision is `state.connectionLine`'s, and
   * it is THREE-state: `st.live` is set only inside `adopt()`, after the read
   * resolves, so the boolean form said "no local server in this reading" for
   * the entire duration of every read — while the app was at that moment
   * reading from the local server. Both call sites read this one method: the
   * status strip in `buildVals`, and the Operator diagnostics drawer's
   * `connection` row.
   *
   * @param {boolean} [long] include the egress guarantee, for the drawer row
   * @returns {string}
   */
  connLine(long) {
    const st = this.state
    return connectionLine({ live: st.live, dataStatus: st.dataStatus, isDemo: st.isDemo, long })
  }

  /**
   * The seven copyable commands. `missing` explains why one is unavailable.
   * NOTHING HERE RUNS: every command is copied and a human runs it (doctrine 8).
   *
   * @returns {Array<{label:string, cmd:string|null, missing?:string}>}
   */
  commands() {
    const S = this.S()
    const c = S.claims[0]
    const db = this.dbPath()
    const sid = S.id || '<session id — not exported in this reading>'
    return [
      {
        label: 'Copy acceptance command',
        cmd: 'thruline acceptance status --exec-db ' + db + ' --json',
        note: 'shows accepted requirements and any unaccepted draft; accepting needs a reviewed digest',
      },
      {
        label: 'Copy next check command',
        cmd: c && c.nextCheck ? c.nextCheck : null,
        missing: c && !c.nextCheck ? nextCheckAbsent(c) : !c ? 'no claim to check against' : null,
      },
      {
        label: 'Copy receipt command',
        cmd: 'thruline receipt --session ' + sid + ' --exec-db ' + db + ' --trailer',
        missing: S.id ? null : 'session id missing',
      },
      {
        label: 'Copy receipt verification command',
        cmd: 'thruline verify-receipt <receipt file> --offline --exec-db ' + db + ' --format text',
        missing: 'receipt file path not in this reading',
      },
      {
        label: 'Copy rerun command',
        cmd: 'thruline reexec --session ' + sid + ' --mode claimed --backend docker --exec-db ' + db,
        missing: S.capture && S.capture.degraded ? 'refused on degraded capture' : S.id ? null : 'session id missing',
      },
      { label: 'Copy capture diagnostic command', cmd: 'thruline doctor --exec-db ' + db + ' --json' },
      { label: 'Copy handoff command', cmd: 'thruline handoff --project ' + (S.project || '<project>') },
    ]
  }

  /**
   * The inspector, as Markdown. Ends with the copy caveat, which is doctrine.
   *
   * EXPORT-MD-ESCAPE-R4 (D-246): this is text a person pastes and shares, and the
   * title, the sentences, the record's keys and values and the origin in it are
   * what the agent, a check or a transcript wrote. Each is written by
   * `packet-export.js`'s `escapeMarkdown`, the helper the packet export uses, so
   * none can be read as a tag, a link, an image or a heading by any renderer. The
   * id is written as escaped text too, not in a code span: a recorded backtick
   * cannot then end a span the file did not mean to end.
   *
   * @returns {string}
   */
  markdown() {
    const i = this.inspector()
    const w = wordOf(i.wordSlug)
    const t = (v) => (v == null ? '' : escapeMarkdown(v))
    const lines = ['# ' + t(i.title), '', w.word ? '**' + w.word + '** · ' + t(i.typeLabel) + ' · ' + t(i.id) : t(i.typeLabel) + ' · ' + t(i.id), '']
    if (w.negative) lines.push(t(i.clause || NO_CLAUSE), '')
    i.sentences.forEach((s) => lines.push(t(s.text)))
    lines.push('', '## Record')
    i.evidence.forEach((r) => lines.push('- ' + t(r.k) + ': ' + t(r.v)))
    /* The footer's underscore can be an email-name character before an @-first
     * origin, so the helper must see that literal and the recorded text together. */
    lines.push('', t('_' + (this.S().origin || 'this reading')) + ' · copied from the Trace inspector; copying does not execute._')
    return lines.join('\n')
  }

  /* ── Inspector (spec-script §B.11) ───────────────────────────────────── */

  /**
   * One key/value row. An ABSENT value renders its own honest sentence — the
   * caller's `absent`, or the ruled default — never a dash and never `n/a`.
   *
   * @param {string} k @param {*} v
   * @param {{absent?:string, color?:string, mono?:boolean, wordSlug?:string, clause?:string|null, gloss?:boolean}} [o]
   * @returns {object}
   */
  R(k, v, o) {
    const opt = o || {}
    const absent = v == null || v === ''
    return {
      k,
      v: absent ? opt.absent || 'not in this record' : String(v),
      color: opt.color || (absent ? C.inkDim : opt.mono ? C.inkCode : C.ink),
      font: opt.mono ? FONT.mono : 'inherit',
      fontSize: opt.mono ? FS.ctl : FS.body,
      wordSlug: opt.wordSlug || null,
      clause: opt.clause === undefined ? null : opt.clause,
      gloss: !!opt.gloss,
    }
  }

  /**
   * The record's own `What would prove it:` sentence for one object, verbatim.
   * Never re-authored, never paraphrased; when the record holds none, the
   * ruled empty case is STATED, not omitted (spec-doctrine F-2, RSV:309-312).
   *
   * @param {object|null} o anything carrying `clause` or `prove`
   * @returns {string|null}
   */
  clauseOf(o) {
    if (!o) return null
    if (o.clause || o.prove) return o.clause || o.prove
    /* An ask, a promise and a stop each carry their clause on their NODE, not
     * on the row (`adapter.js` puts `clauseOf(S.narr.ask)` there, and the
     * bundled corpus does the same). Reading only the row printed the ruled
     * EMPTY case over records that hold a real clause — which is a false
     * statement about the record, and the opposite of what the empty case is
     * for. */
    if (o.node) {
      const n = this.nodeById(o.node)
      if (n && n.clause) return n.clause
    }
    return null
  }

  /**
   * What one LISTING row says about itself, as words.
   *
   * Doctrine 4 twice over: every count carries its noun, and a zero count does
   * not render as a number — a session with nothing in it says so in a
   * sentence. A count the listing did not carry is a THIRD thing and says that
   * instead; `0` and "not in this listing" must never render alike.
   *
   * There is no state word here and there must not be. A word is derived by the
   * adapter from a session's claims, asks and promises, so a row that has not
   * been read has none — and inventing one, or borrowing a neighbour's, is the
   * class of unbacked claim this product exists to catch.
   *
   * @param {object} r a normalised listing row
   * @returns {string}
   */
  listingLine(r) {
    const parts = []
    if (r.events == null) parts.push('event count not in this list')
    else if (r.events > 0) parts.push(countPhrase(r.events, 'event', 'events'))
    else parts.push('no events recorded')
    if (r.prompts) parts.push(countPhrase(r.prompts, 'prompt', 'prompts'))
    if (r.fileChanges) parts.push(countPhrase(r.fileChanges, 'file change', 'file changes'))
    parts.push(r.stops ? 'finished on its own' : 'no ending recorded')
    if (r.degraded) parts.push('the recorder was degraded here')
    if (!r.openable) parts.push('this id is not one this app may ask the local app for')
    return parts.join(' · ')
  }

  /**
   * The session picker's rows, from the LISTING, in the engine's order.
   *
   * The engine ranks by what the store carries — a recorded ending first, then
   * the newest recorded event, then fold order — and this does not re-sort it:
   * replacing a measured ranking with a proxy would be the app inventing an
   * order it never measured. It only narrows, by id or repository, and only
   * when the reader asks.
   *
   * @param {number} [cap] at most this many rows, for the header's popup
   * @returns {object[]}
   */
  listingRows(cap, repo) {
    const rows = filterListing(this.state.sessions.rows, this.state.sessionFilter).filter(r => !repo || r.repo === repo)
    return cap != null ? rows.slice(0, cap) : rows
  }

  /**
   * The clause for a SESSION's own word. A session is not an item and carries
   * no clause of its own: its word is derived from the items beneath it
   * (`sessionWordFrom`), so its clause is the clause of the item that gave it
   * that word. When no such item holds one, the caller states the ruled empty
   * case — a session word with no clause anywhere is never rendered bare.
   *
   * @param {object|null} S a Session
   * @returns {string|null}
   */
  sessionClauseOf(S) {
    if (!S) return null
    if (S.clause) return S.clause
    const w = S.sessionWord
    if (!w) return null
    const pools = [S.claims || [], S.asks || [], S.promises || []]
    for (let i = 0; i < pools.length; i++) {
      const hit = pools[i].find((o) => o.word === w && this.clauseOf(o))
      if (hit) return this.clauseOf(hit)
    }
    for (let i = 0; i < pools.length; i++) {
      const hit = pools[i].find((o) => this.clauseOf(o))
      if (hit) return this.clauseOf(hit)
    }
    return null
  }

  /**
   * The inspector view-model — the app's proof surface.
   *
   * PARAMETERISED: this is the fix for the design's direct `this.state.sel`
   * mutation in `claimDocInspector` (spec-script §E6). Seven-way branch on
   * `objType(n)` plus `n.finding`.
   *
   * A FINDING GETS A SEVERITY AND NEVER A STATE WORD (spec-doctrine F-4), and
   * an integrity reading is not a state word either — it travels in its own
   * `integrity` field, out of the word slot (F-8).
   *
   * @param {string} [selId=this.state.sel] which node to inspect
   * @returns {object} the shape `docks.Inspector` and `canvas.ClaimDocument` take
   */
  inspector(selId) {
    const S = this.S()
    const reg = this.reg()
    const id = selId === undefined ? this.state.sel : selId
    const n = this.nodeById(id)
    const com = this.props.commentary !== false
    const base = {
      typeLabel: '',
      title: '',
      titleFont: 'inherit',
      id: '',
      wordSlug: null,
      clause: null,
      integrity: null,
      integrityDetail: '',
      sentences: [],
      commentary: '',
      narr: [],
      details: [],
      evidence: [],
      history: [],
      raw: '{}',
      hasFindings: false,
      findings: [],
      notes: [],
      findingsCount: { findings: 0, notes: 0 },
      nextCheck: null,
      closing: 'Not in this record: source line · chain position · measurement tier',
    }
    if (!n) {
      base.title = 'Nothing selected'
      base.sentences = [{ text: 'Select a node, a tree object, or a finding row.', color: C.inkMute }]
      return base
    }
    const ot = objType(n)
    const narr = (item) => (item && item[reg] ? item[reg].map((t) => ({ text: t })) : [])
    const cap = S.capture || {}
    const inst =
      'recorded by ' +
      (cap.sources || []).join(', ') +
      ' · ' +
      (cap.recorderLine || (cap.degraded ? 'The recorder is not vouched for this session.' : 'The recorder was working for this session.'))
    const V = S.verify || { rows: [], claims: {} }
    const okRows = (V.rows || []).filter((r) => r.outcome === 'pass').length
    const noRows = (V.rows || []).length - okRows
    const recInt =
      V.integrity === 'verified'
        ? 'intact — ' + okRows + ' of ' + (V.rows || []).length + ' checks confirmed here'
        : 'partly checked here — ' + okRows + ' confirmed · ' + noRows + ' could not be made here — not a pass'
    const fmap = (f) => ({ key: f.key, name: f.name, severity: f.severity, ref: f.ref, node: f.node })
    // Resolve the selected object by its recorded identity, never its position
    // in a projection. A missing row must not borrow another object's evidence.
    const selectedRows = { claim: S.claims, check: S.checks, promise: S.promises, ask: S.asks }[ot]
    const selectedRow = selectedRows && (
      selectedRows.find((row) => row.node === n.id) ||
      selectedRows.find((row) => row.id && (row.id === n.objId || row.id === n.id))
    )
    if (selectedRows && !selectedRow) {
      base.typeLabel = String(ot) + ' · source unavailable'
      base.title = n.title || 'Selected source is not included in this reading'
      base.id = n.objId || n.id
      base.sentences = [{ text: 'The selected object has no matching source row in this reading.', color: C.inkMute }]
      return base
    }
    const sourcePosition = (row) => [
      row.eventId ? 'event ' + row.eventId : null,
      row.seq != null ? 'seq ' + row.seq : null,
      row.offset ? 'characters ' + row.offset.join('–') : null,
    ].filter(Boolean).join(' · ')
    const checkCounts = (check) => check && check.parsed
      ? [['passed', 'passed'], ['failed', 'failed'], ['skipped', 'skipped'], ['tests_total', 'total']]
        .filter(([key]) => check.parsed[key] != null)
        .map(([key, label]) => check.parsed[key] + ' ' + label).join(' · ')
      : null
    const checkBinding = (check) => check.inputTree && S.finalTree
      ? check.inputTree === S.finalTree ? 'input tree matches the final tree' : 'input tree differs from the final tree'
      : null

    base.id = n.objId && n.objId !== 'receipt' ? n.objId : n.seq != null ? 'seq ' + n.seq : ot === 'receipt' ? (S.receipt || {}).trailer || 'receipt trailer not in this reading' : n.id
    if (com && S.commentary && S.commentary[n.id]) base.commentary = S.commentary[n.id]

    if (ot === 'claim' && selectedRow) {
      const c = selectedRow
      const linkedChecks = S.checks.filter((check) => (c.evidence || []).includes(check.id))
      const chk = linkedChecks.length === 1 ? linkedChecks[0] : null
      base.wordSlug = c.word
      base.clause = this.clauseOf(c)
      base.typeLabel = 'Claim · ' + String(c.kind || '').replace(/_/g, ' ') + ' · derived'
      base.title = '“' + c.text + '”'
      base.id = c.id
      base.sentences = [
        { text: c.plain, color: C.ink },
        { text: this.clauseOf(c) || NO_CLAUSE, color: C.inkSoft },
      ]
      base.narr = narr(c.narr || (S.claims.length === 1 ? S.narr.verdict : null))
      base.details = [
        this.R('Exact claim', c.text),
        this.R('State', wordOf(c.word).word + (GLOSS[c.word] ? ' — ' + GLOSS[c.word] : ''), {
          wordSlug: c.word,
          clause: this.clauseOf(c),
          gloss: true,
          color: wordOf(c.word).ink, // the word's text-safe ink (P6-6: Fog's neutral fails 4.5:1)
        }),
        this.R('Scope', (c.scope === 'all' ? 'about everything (scope all)' : c.scope ? 'scope ' + c.scope : 'scope not in this reading') + (c.extractor ? ' · extracted by ' + c.extractor : '')),
        this.R('Reasons', (c.reasons || []).join(' · ')),
        this.R('Warnings', (c.warnings || []).length ? c.warnings.join(' · ') : null, { absent: 'none recorded' }),
        this.R(
          'Asserted',
          'event ' + (c.eventId || 'not exported') + (c.seq != null ? ' · seq ' + c.seq : '') + (c.offset ? ' · characters ' + c.offset.join('–') : ''),
          { mono: true },
        ),
        this.R('Next useful check', c.nextCheck, {
          mono: true,
          absent: nextCheckAbsent(c),
        }),
      ]
      /* A3-NOTES-NEVER-OPEN: a `fail` or `warn` finding is a finding; an
       * `info` one is a note, listed apart and never counted as a finding.
       * The count is TYPED, so the view renders it from the copy file. */
      const all = (c.findings || []).map(fmap)
      base.hasFindings = all.length > 0
      base.findings = all.filter((f) => f.severity !== 'info')
      base.notes = all.filter((f) => f.severity === 'info')
      base.findingsCount = { findings: base.findings.length, notes: base.notes.length }
      base.nextCheck = c.nextCheck || null
      base.integrity = V.integrity === 'verified' ? 'Receipt intact' : (V.rows || []).length ? 'Partly checked here' : null
      base.integrityDetail = (V.rows || []).length ? recInt + ' — separate from claim support' : ''
      base.evidence = [
        this.R('Evidence references', (c.evidence || []).join(' · '), { mono: true, absent: 'no evidence references included for this claim' }),
        this.R('Referenced checks', linkedChecks.map((check) => check.id).join(' · '), { mono: true, absent: 'no matching check rows in this reading' }),
        this.R('The check', chk ? [chk.id, chk.category, chk.runner, chk.exit != null ? 'exit ' + chk.exit : null].filter(Boolean).join(' · ') : null, { mono: true, absent: linkedChecks.length > 1 ? 'multiple checks are referenced; inspect their individual rows' : 'no single referenced check row in this reading' }),
        this.R('Command', chk ? [chk.command, chk.scopeNote].filter(Boolean).join(' — ') : null, { mono: true }),
        this.R('Run reference', chk ? chk.seqHint : null),
        this.R(
          'Parsed counts',
          checkCounts(chk),
          { mono: true },
        ),
        this.R(
          'Tests that ran',
          chk && chk.parsed && chk.parsed.test_ids && chk.parsed.test_ids.length ? chk.parsed.test_ids.join(', ') : null,
          { mono: true, absent: 'per-test IDs not exported' },
        ),
        this.R('Input tree vs final', chk && checkBinding(chk) ? short(chk.inputTree) + ' vs ' + short(S.finalTree) + ' — ' + checkBinding(chk) : null, { mono: true }),
        this.R('Recorder condition', inst),
        this.R('Receipt integrity', (V.rows || []).length ? recInt + ' · conclusion ' + V.conclusion + ' · exit ' + V.exit : null),
        this.R(
          'Where',
          'event ' + (c.eventId || 'not exported') + (c.seq != null ? ' · seq ' + c.seq + ' (a sequence number, not a line)' : ''),
          { mono: true },
        ),
        this.R('Source', null, { absent: 'Source line not exported' }),
        this.R('How judged', c.verifier ? 'verifier ' + c.verifier + ' · the verifier returned ' + c.verdict : null),
      ]
      base.history = [
        this.R('Recorded order', S.events.length ? S.events.map((e) => 'seq ' + e.seq + ' ' + (KIND_LABEL[e.kind] || e.kind)).join(' → ') : S.eventsNote, {
          mono: true,
        }),
        this.R('Asserted', sourcePosition(c), { mono: true, absent: 'source position not in this reading' }),
        this.R('Assessment', wordOf(c.word).word + ' — at this reading', { wordSlug: c.word, clause: this.clauseOf(c), color: wordOf(c.word).ink }),
        this.R('Earlier assessments', null, { absent: 'none recorded' }),
        this.R('Later assessments', null, {
          absent: 'none recorded — a later confirmation would be added here, never written over this one',
        }),
      ]
      base.raw = safeJson({
        id: c.id,
        kind: c.kind,
        text: c.text,
        span: { event_id: c.eventId, offset: c.offset },
        sequence: c.seq,
        scope: c.scope,
        extractor: c.extractor,
        verdict: c.verdict,
        verifier: c.verifier,
        reasons: c.reasons,
        warnings: c.warnings,
        binding: c.binding,
        findings: (c.findings || []).map((f) => ({ key: f.key, severity: f.severity, detail: f.detail || null })),
        hashes: { final_tree: S.finalTree, commit: S.commit, event_chain_head: S.chainHead },
      })
    } else if (ot === 'check' && selectedRow) {
      const k = selectedRow
      base.typeLabel = 'Check · ' + k.category + ' · ledger row'
      base.title = k.command || k.id
      base.titleFont = FONT.mono
      base.id = k.id
      base.sentences = [
        {
          text: [checkCounts(k), k.exit != null ? 'exit ' + k.exit : 'exit code not in this reading', checkBinding(k)].filter(Boolean).join(' · ') + '.',
          color: C.ink,
        },
        {
          text: k.scopeNote ? 'Recorded run scope: ' + k.scopeNote + '.' : 'Run scope is not included in this reading.',
          color: C.inkSoft,
        },
      ]
      base.details = [
        this.R('Runner', k.runner),
        this.R('Command', k.command, { mono: true, absent: 'argv not in this reading' }),
        this.R('Scope', k.scopeNote),
        this.R(
          'Counts',
          checkCounts(k),
          { mono: true },
        ),
        this.R('Exit code', k.exit, { mono: true }),
        this.R('Binding', checkBinding(k), { mono: true, absent: 'tree comparison is not available in this reading' }),
        this.R('Input tree', k.inputTree, { mono: true }),
        this.R('Re-executed', k.reexecuted === true ? 'yes' : k.reexecuted === false ? 'no' : null),
      ]
      base.evidence = [
        this.R('Run reference', k.seqHint, { absent: 'exec seq not in this reading' }),
        this.R('argv digest', k.argvHash, { mono: true, absent: 'argv digest not in this reading' }),
        this.R('Recorder condition', inst),
        this.R('In receipt', 'predicate.checks[] · the check is the ledger’s own row'),
        this.R('Receipt integrity', (V.rows || []).length ? recInt : null),
        this.R('Source', null, { absent: 'Source line not exported' }),
      ]
      base.history = [
        this.R('Recorded order', k.seqHint, { mono: true, absent: 'exec seq not in this reading' }),
        this.R('Assessment', 'A check is evidence. Its result does not by itself establish whether a claim is supported.'),
        this.R('Referenced by claims', S.claims.filter((claim) => (claim.evidence || []).includes(k.id)).map((claim) => claim.id).join(' · '), { mono: true, absent: 'no claim in this reading references this check' }),
      ]
      base.integrity = V.integrity === 'verified' ? 'Receipt intact' : (V.rows || []).length ? 'Partly checked here' : null
      base.raw = safeJson({
        id: k.id,
        category: k.category,
        runner: k.runner,
        command: k.command,
        exit_code: k.exit,
        argv_hash: k.argvHash,
        input_tree_hash: k.inputTree,
        binding: k.bindingLabel,
        reexecuted: k.reexecuted,
        parsed: k.parsed,
      })
    } else if (n.finding) {
      // A detector can produce findings for several claims. Resolve this
      // selected finding by object identity, not by its shared detector name.
      const owners = S.claims.flatMap((claim) => (claim.findings || [])
        .filter((finding) => finding.node === n.id || (finding.id && (finding.id === n.objId || finding.id === n.id)))
        .map((finding) => ({ claim, finding })))
      const owner = owners.length === 1 ? owners[0] : null
      const f = owner ? owner.finding : null
      const sev = severityOf(f ? f.severity : 'info')
      const c0 = owner ? owner.claim : null
      const relatedCheck = f && f.checkId ? S.checks.find((check) => check.id === f.checkId) : null
      base.typeLabel = 'Finding · derived from the claims projection'
      base.title = f ? f.name || f.key || n.title : n.title
      base.id = f && f.id ? f.id : n.id
      /* NO state word here. A finding carries a significance and supplies the
       * clause for the verdict it attaches to (spec-doctrine F-4). */
      base.severity = f ? f.severity : 'info'
      base.significance = sev.label
      const detail = f ? f.detail || f.derivedDetail : ''
      base.sentences = [
        { text: detail, color: f && f.detail ? C.ink : C.inkMute },
        {
          text:
            f && f.severity === 'info'
              ? 'Informational — it does not fail a check and does not move the claim.'
              : c0
                ? 'On claim “' + c0.text + '” — ' + wordOf(c0.word).word
                : 'This reading does not identify a unique source claim for the selected finding.',
          color: C.inkSoft,
        },
      ]
      base.details = [
        this.R('Finding', f ? f.key : n.finding, { mono: true }),
        this.R('Significance', sev.label, { color: sev.color }),
        this.R('Recorded detail', f && f.detail ? f.detail : null, { absent: 'detector detail not quoted in this reading' }),
        this.R('Related path', f && f.path ? f.path : null, { mono: true, absent: 'a related path is not included in this reading' }),
        this.R('Related check', f && f.checkId ? f.checkId : null, { mono: true, absent: 'a direct check reference is not included for this finding' }),
        this.R('On claim', c0 ? c0.id : null, { mono: true }),
      ]
      base.evidence = [
        this.R('Reference', f ? f.ref : null, { mono: true }),
        this.R(
          'Evidence',
          f && f.path
            ? 'The finding names ' + f.path + '. A shared path alone does not identify a file-change event.'
            : relatedCheck
              ? [relatedCheck.command || relatedCheck.id, checkCounts(relatedCheck)].filter(Boolean).join(' · ')
              : null,
          { mono: true },
        ),
        this.R('Claim evidence references', c0 ? (c0.evidence || []).join(' · ') : null, { mono: true, absent: 'claim evidence references are not included in this reading' }),
        this.R('Recorder condition', inst),
        this.R('Source', null, { absent: 'Source line not exported' }),
      ]
      base.history = [
        this.R('Derived', 'at this reading, from the claims projection’s findings'),
        this.R(
          'Source position',
          f ? sourcePosition(f) : null,
          { mono: true, absent: 'an event position is not included for this finding' },
        ),
      ]
      base.raw = safeJson(f ? { key: f.key, severity: f.severity, detail: f.detail || null, path: f.path || null, ref: f.ref, claim_id: c0.id, check_id: f.checkId || null, sequence: f.seq == null ? null : f.seq } : { key: n.finding, node: n.id })
    } else if (ot === 'promise' && selectedRow) {
      const p = selectedRow
      base.wordSlug = p.word
      base.clause = this.clauseOf(p)
      base.typeLabel = 'Promise · follow-up · derived'
      base.title = '“' + p.text + '”'
      base.id = p.id
      base.sentences = [
        { text: p.status ? 'Recorded follow-up status: ' + String(p.status).replace(/_/g, ' ') + '.' : 'Follow-up status is not included in this reading.', color: C.ink },
        ...(p.word ? [{ text: this.clauseOf(p) || NO_CLAUSE, color: C.inkSoft }] : []),
      ]
      base.narr = narr(p.narr || (S.promises.length === 1 ? S.narr.promise : null))
      base.details = [
        this.R('Status', p.status),
        this.R('Kind', String(p.kind || '').replace(/_/g, ' ')),
        this.R('Opened at', sourcePosition(p), { mono: true, absent: 'source position not in this reading' }),
        this.R('Follow-up evidence', (p.evidence || []).join(' · '), { mono: true, absent: 'supporting references are not included in this reading' }),
      ]
      base.evidence = [
        this.R('Opened in', S.id ? 'session ' + S.id : null, { mono: true }),
        this.R('Fulfilment', p.status === 'fulfilled' ? 'Recorded as fulfilled; this reading does not independently verify completion.' : p.status ? 'Recorded status: ' + p.status : null),
        this.R('Recorder condition', inst),
        this.R('Source', null, { absent: 'Source line not exported' }),
      ]
      base.history = [
        this.R('Opened', sourcePosition(p), { mono: true, absent: 'source position not in this reading' }),
        this.R('Later sessions', null, { absent: 'later-session history is not included in this reading' }),
      ]
      base.raw = safeJson({ id: p.id, text: p.text, kind: p.kind, status: p.status, opened_event_id: p.eventId, sequence: p.seq })
    } else if (ot === 'ask' && selectedRow) {
      const a = selectedRow
      base.wordSlug = a.word
      base.clause = this.clauseOf(a)
      base.typeLabel = 'Ask · ' + a.kind + ' · derived'
      base.title = '“' + a.text + '”'
      base.id = a.id || a.idNote || 'ask id not exported'
      base.sentences = [
        { text: a.status ? 'Recorded request status: ' + String(a.status).replace(/_/g, ' ') + '.' : 'Request status is not included in this reading.', color: C.ink },
        ...(a.word ? [{ text: this.clauseOf(a) || NO_CLAUSE, color: C.inkSoft }] : []),
      ]
      base.narr = narr(a.narr || (S.asks.length === 1 ? S.narr.ask : null))
      if (a.limitation) base.sentences.push({ text: a.limitation, color: C.amber })
      base.details = [
        this.R('Status', a.status),
        this.R('Kind', a.kind),
        this.R('Source position', sourcePosition(a), { mono: true, absent: 'source position not in this reading' }),
        this.R('Extractor', a.extractor),
        this.R('Resolution', a.status ? 'The record classifies this request as ' + String(a.status).replace(/_/g, ' ') + '.' : null),
      ]
      base.evidence = [
        this.R('Source event', a.eventId, { mono: true, absent: 'event id not exported' }),
        this.R('Evidence references', (a.evidence || []).join(' · '), { mono: true, absent: 'supporting references are not included in this reading' }),
        this.R('Recorder condition', inst),
        this.R('Source', null, { absent: 'Source line not exported' }),
      ]
      base.history = [
        this.R('Placed', sourcePosition(a), { mono: true, absent: 'source position not in this reading' }),
        this.R('Later', null, { absent: 'later references are not included in this reading' }),
      ]
      base.raw = safeJson({ id: a.id, kind: a.kind, status: a.status, text: a.text, event_id: a.eventId, offset: a.offset, extractor: a.extractor })
    } else if (ot === 'receipt') {
      const rc = S.receipt || {}
      base.typeLabel = 'Receipt · DSSE envelope · in-toto statement'
      base.title = rc.trailer ? 'ThruLine-Receipt: ' + short(rc.trailer) : V.integrity === 'verified' ? 'Receipt intact' : 'Receipt partly checked here'
      base.titleFont = FONT.mono
      /* An integrity reading is NOT a state word (spec-doctrine F-8). */
      base.integrity = V.integrity === 'verified' ? 'Receipt intact' : 'Partly checked here'
      base.integrityDetail = recInt
      base.sentences = [
        { text: recInt + '.', color: C.ink },
        {
          text:
            'Integrity says the envelope is what it claims to be. It says nothing about whether the claims inside are proven — that is the verdict column — nor about policy, which was not evaluated.',
          color: C.inkSoft,
        },
      ]
      base.details = [
        this.R('Trailer', rc.trailer, { mono: true, absent: 'trailer not in this reading' }),
        this.R('Signed with key', rc.keyid ? rc.keyid + ' (a hint)' : null, { mono: true }),
        this.R('Payload type', rc.payloadType, { mono: true }),
        this.R('Predicate', rc.predicateType, { mono: true }),
        this.R('Verifier', rc.verifierVersion ? rc.verifierVersion + ' · mode ' + rc.mode : null),
        this.R('Policy', 'no policy in force · policy hash ' + short(rc.policyHash), { mono: true }),
        this.R('Conclusion', V.conclusion ? V.conclusion + ' · exit ' + V.exit + ' — gate on the conclusion, never on the exit' : null),
      ]
      base.evidence = (V.rows || []).map((r) =>
        this.R(r.name, (r.outcome === 'pass' ? 'confirmed here — ' : 'could not be made here — not a pass — ') + (r.detail || r.reason || ''), {
          mono: true,
          color: r.outcome === 'pass' ? C.inkCode : C.amber,
        }),
      )
      base.history = [
        this.R(
          'Claims in receipt',
          V.claims && V.claims.total != null
            ? V.claims.total + ' total · ' + V.claims.supported + ' proven · ' + V.claims.unsupported + ' not proven · ' + V.claims.unverifiable + ' recorded'
            : null,
        ),
        this.R('Policy', V.policyNote),
      ]
      base.raw = safeJson({
        trailer: rc.trailer,
        keyid: rc.keyid,
        payloadType: rc.payloadType,
        predicateType: rc.predicateType,
        subject: { name: rc.subjectName, gitTree: S.finalTree, gitCommit: S.commit },
        verify: { conclusion: V.conclusion, status: V.status, exit: V.exit, checks: (V.rows || []).map((r) => ({ name: r.name, outcome: r.outcome })) },
      })
      base.closing = 'Receipts carry hashes, not content. Nothing left this machine.'
    } else if (ot === 'ending') {
      base.wordSlug = S.stop && S.stop.word ? S.stop.word : 'fog'
      base.clause = this.clauseOf(S.stop) || 'What would prove it: a stop event for this session, from a hook that is installed.'
      base.typeLabel = 'Ending · derived'
      base.title = 'No ending recorded'
      base.id = S.id ? 'ses_' + String(S.id).slice(0, 8) + '…' : 'session id not exported'
      base.sentences = [
        { text: 'Fog: either this session is still open, or the Stop hook never fired.', color: C.ink },
        { text: base.clause, color: C.inkSoft },
      ]
      base.narr = narr(S.narr.stop)
      base.details = [
        this.R('Ending', S.stop.kind),
        this.R('Events', S.eventCount != null ? countPhrase(S.eventCount, 'event', 'events') + ' · the line dissolves' : null),
        this.R('Final tree', null, { absent: 'No final tree: nothing to bind to.' }),
      ]
      base.evidence = [
        this.R('The instrument', inst),
        this.R('Chain', S.chainHead ? 'chain head ' + S.chainHead + (S.eventCount != null ? ' · ' + countPhrase(S.eventCount, 'event', 'events') : '') : null, {
          mono: true,
        }),
        this.R('When', S.started ? 'started ' + S.started + ' · no ending recorded' : null),
      ]
      base.history = [this.R('Last events', (S.stop.lastEvents || []).join(' · '))]
      base.raw = safeJson({ ending: S.stop.kind, summary: S.stop.summary, sequence: null, events: S.eventCount })
      base.closing = 'Not in this record: source line · chain position · cause'
    } else {
      const e = S.events.find((x) => x.seq === n.seq)
      const kindLabel = e ? KIND_LABEL[e.kind] || e.kind : n.sub
      /* No ` · recorded` tail. `recorded` occupying a whole label slot is a
       * record enum used as a name (`lib/voice.js` ENUMS, `bannedIn`), which
       * CONTRACT §7 correction 10 rules out of a node line. The auto-layout's
       * risk-frame title already drops it for the same reason
       * (`adapter.js:670-673`); this is the inspector's matching case. */
      base.typeLabel = (n.group ? 'Event group · ' : 'Event · ') + kindLabel
      base.title = n.title
      base.titleFont = n.mono ? FONT.mono : 'inherit'
      base.id = n.seq != null ? 'seq ' + n.seq : n.id
      if (n.group && n.members) {
        const ms = n.members.map((m) => this.nodeById(m)).filter(Boolean)
        /* The invariant first, the count second (§2.2 rule 1): a reader's
         * question is "did anything change here?", not "how many rows were
         * there". The headline the card carries is repeated here as a
         * sentence, and the seq range says where in the record it holds. */
        const span = n.firstSeq != null && n.lastSeq != null ? ' seq ' + n.firstSeq + '–' + n.lastSeq + '.' : ''
        base.sentences = [
          { text: n.title + '.' + span, color: C.ink },
          {
            /* The verb agrees with the count. 1,372 of this session's 1,373
             * step rows hold exactly one member, so the plural verb was the
             * case a reader actually met: "1 recorded event stand behind that." */
            text:
              countPhrase(ms.length, 'recorded event', 'recorded events') +
              (ms.length === 1 ? ' stands' : ' stand') +
              ' behind that. Expand the card to see each one, in order, with its own receipt.',
            color: C.inkSoft,
          },
        ]
        if (n.outcome) {
          base.sentences.push({
            text:
              n.outcome === 'ok'
                ? 'The record states this call completed.'
                : n.outcome === 'failed'
                  ? 'The record states this call did not complete.'
                  : 'The record does not state how this call ended.',
            color: n.outcome === 'ok' ? C.inkSoft : C.amber,
          })
        }
        /* 1,360 rows would be a wall of its own. The first and last are the
         * two the range names; the middle is stated, not silently dropped. */
        const head = ms.slice(0, 12)
        base.details = head.map((m) => this.R('seq ' + m.seq, m.title, { mono: true }))
        if (ms.length > head.length) {
          base.details.push(this.R('And the rest', countPhrase(ms.length - head.length, 'more event', 'more events') + ' — expand the card to read them', {}))
        }
      } else {
        base.sentences = [{ text: e ? 'Recorded ' + String(kindLabel).toLowerCase() + ' at seq ' + e.seq + '.' : 'Recorded event.', color: C.ink }]
        /* `— recorded, not judged.` put the enum `recorded` alone in a label
         * slot (`lib/voice.js` ENUMS, `bannedIn`). The sentence keeps its whole
         * meaning — the record notes this, and nothing here rules on it —
         * without spending a slot on a record enum. */
        if (n.risk) base.sentences.push({ text: 'Risk flag: ' + ((e && e.risk) || 'destructive command') + ' — the record notes it. Nothing here judges it.', color: C.amber })
        const claimHere = S.claims.find((claim) => claim.seq != null && claim.seq === n.seq && claim.eventId)
        const askHere = S.asks.find((ask) => ask.seq != null && ask.seq === n.seq && ask.eventId)
        /* THREE ORIGINS, THREE LABELS (DESIGN-readable-trace.md §1.8). This row
         * set replaces a single `Recorded text` row that showed the same bytes
         * as the card title, so the detail view added nothing over the label.
         * `As recorded` is the record value untouched; `Read as` is this
         * build's derived line; `As the agent described it` is agent-authored
         * text and is never blended into either. */
        const described = e ? describedAs(e) : null
        const outcome = e ? outcomeSentence(e) : null
        base.details = [
          this.R('Kind', e ? e.kind : n.sub, { mono: true }),
          this.R('Sequence', n.seq != null ? n.seq + ' (a sequence number, not a line number)' : null, { mono: true }),
          this.R('As recorded', e ? e.text : n.title, { mono: true }),
          this.R('Read as', e && e.readText ? e.readText : null, { absent: 'no derived line for this row' }),
          e && e.kind === 'tool_call' ? this.R('As the agent described it', described, { absent: 'the agent recorded no description in this reading' }) : null,
          outcome ? this.R('Outcome', outcome) : null,
          this.R('Event id', e && e.eventId || (claimHere && claimHere.eventId) || (askHere && askHere.eventId), { mono: true, absent: 'event id not exported for this row' }),
          this.R('File', n.file, { mono: true, absent: 'no path on this event' }),
          this.R('Receipt', n.seq != null && S.chainHead ? 'seq ' + n.seq + ' · chain head ' + short(S.chainHead) : null, { mono: true, absent: 'chain position not exported for this row' }),
        ].filter(Boolean)
        if (S.stop && S.stop.seq === n.seq) base.narr = narr(S.narr.stop)
      }
      base.evidence = [
        this.R('Chain', S.chainHead ? 'chain head ' + short(S.chainHead) + (S.eventCount != null ? ' · ' + countPhrase(S.eventCount, 'event', 'events') : '') : null, {
          mono: true,
        }),
        this.R('Tree', /tree/.test(n.title) ? S.finalTree : null, { mono: true, absent: 'no tree on this event' }),
        this.R('Recorder condition', inst),
        this.R('Source', null, { absent: 'Source line not exported' }),
      ]
      const idx = S.events.findIndex((x) => x.seq === n.seq)
      base.history = [
        this.R('Before it', idx > 0 ? 'seq ' + S.events[idx - 1].seq + ' · ' + S.events[idx - 1].text : null, { mono: true, absent: 'first recorded event' }),
        this.R('After it', idx >= 0 && idx < S.events.length - 1 ? 'seq ' + S.events[idx + 1].seq + ' · ' + S.events[idx + 1].text : null, {
          mono: true,
          absent: 'last recorded event',
        }),
      ]
      base.raw = safeJson(e ? { seq: e.seq, kind: e.kind, text: e.text, risk: e.risk || undefined } : { node: n.id })
    }
    /* THE VOID UNDER "AS THE CLI NARRATES IT", named rather than left blank.
     * The heading rendered unconditionally over a `narr` that is empty for
     * essentially every object of every real session, for two structural
     * reasons: the inspector fills it for four object types and an event is
     * not one of them, and `S.narr` is null in every slot because
     * `/api/narrate` does not exist (`adapter.js:11`, `data.js:48-54` — six
     * endpoints, no narrate). `narrShow` guards the heading; the absence is
     * stated ONCE per inspector, with the command that would end it, so a
     * whole section does not disappear from every real session in silence. */
    base.narrShow = (base.narr || []).length > 0
    if (!base.narrShow && !S.narr.session && !S.narr.stop) {
      base.narrAbsent = 'No narration is exported over this server, so this reading has none to show.'
      base.narrCommand = 'thruline explain --session ' + (S.id || '<session id — not exported in this reading>') + ' --exec-db ' + this.dbPath() + ' --narrate'
    }
    return base
  }

  /**
   * The pinned Claim document's inspector. PARAMETERISED — no state mutation
   * (spec-script §E6).
   *
   * @returns {object}
   */
  claimDocInspector() {
    const d = this.state.docs.find((x) => x.id === 'claim')
    const key = d ? d.session : this.state.session
    if (key !== this.state.session) return this.inspector()
    const S = this.S()
    return this.inspector(S.claims[0] ? S.claims[0].node : this.state.sel)
  }

  /* ── Drawers (spec-script §B.12) ─────────────────────────────────────── */

  /**
   * One table cell. An absent value renders the caller's own sentence — the
   * design's `'—'` default is a placeholder and does not survive (F-6).
   *
   * A cell that carries a state word carries its CLAUSE with it (DOC-03).
   * Without this channel no drawer lens could ever satisfy doctrine 3 — the
   * word would render as bare text and the `What would prove it:` sentence
   * would have nowhere to go. `wordSlug` set means the drawer renders the
   * word through `<StateWord>`, which is the one component that enforces the
   * rule; `text` is then the rest of the line, not the word.
   *
   * @param {*} text @param {{color?:string, mono?:boolean, link?:boolean,
   *   absent?:string, wordSlug?:string|null, clause?:string|null}} [o]
   * @returns {object}
   */
  cell(text, o) {
    const opt = o || {}
    const absent = text == null || text === ''
    return {
      text: absent ? opt.absent || '' : String(text),
      color: opt.color || (absent ? C.inkDim : opt.mono ? C.inkCode : C.ink),
      font: opt.mono ? FONT.mono : 'inherit',
      fontSize: opt.mono ? FS.ctl : FS.body,
      deco: opt.link ? 'underline dotted' : 'none',
      wordSlug: opt.wordSlug || null,
      clause: opt.clause === undefined ? null : opt.clause,
      /* N13-GLYPHS-ARIA: a state's glyph travels beside its word, never in
       * the same string, so the renderer can hide it from a screen reader. */
      glyph: opt.glyph || null,
    }
  }

  /**
   * One drawer row. A row with an `onClick` is a real control: role, tabIndex
   * and an Enter/Space handler come with it (spec-script §D.2).
   *
   * @param {object[]} cells @param {Function} [onClick] @param {string} [bg]
   * @returns {object}
   */
  row(cells, onClick, bg) {
    return {
      cells,
      onClick: onClick || null,
      onKey: onClick
        ? (e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              onClick()
            }
          }
        : null,
      role: onClick ? 'button' : null,
      tab: onClick ? 0 : null,
      bg: bg || 'transparent',
      cursor: onClick ? 'pointer' : 'default',
    }
  }

  /**
   * The destinations drawer's contents — fourteen branches.
   *
   * A destination whose source is absent renders `<Refusal>` INSIDE the drawer,
   * naming the missing document or endpoint from `data.DESTINATION_SOURCE`. It
   * never renders a review note, a "coming soon", or an empty section
   * (CONTRACT §0 rule 5, §8 target-4).
   *
   * @returns {{destination:string|null, title:string, lead:string, width:number,
   *   sections:object[], refusal:{title:string,missing:string,remedy?:string}|null,
   *   reading:string|null}} `reading` is the address being read, when the
   *   destination's document is in flight — never a refusal.
   */
  drawerData() {
    const st = this.state
    const k = st.drawer
    const S = this.S()
    const all = this.allSessions()
    /* `filter` is the drawer's one optional control: a labelled field above the
     * sections, for the one destination whose list is longer than a panel. It
     * is null everywhere else and renders nothing. */
    const d = { destination: k, title: '', lead: '', width: 520, sections: [], refusal: null, reading: null, filter: null }
    if (!k) return d
    // This route uses the existing HQ verification controls for the selected
    // local project. The legacy acceptance drawer has a different endpoint.
    if (k === 'agreed') return { ...d, destination: null }
    const src = DESTINATION_SOURCE[k]
    const sec = (title, cols, rows, note, actions) =>
      d.sections.push({ title: title || null, cols, rows: rows || [], note: note || null, actions: actions || [] })
    const copyAct = (label, cmd) => ({ label, onClick: () => this.copy(cmd, label.replace(/^Copy /, '')) })
    const refuse = (missing, remedy) => {
      d.refusal = { title: d.title || 'This destination has nothing to show.', missing, remedy }
      d.sections = []
      d.filter = null
      return d
    }
    /* A READ IN FLIGHT IS NOT A REFUSAL — in a drawer either. The `agreed`
     * destination routed `acc.status === 'loading'` through `refuse()`, so a
     * reader on a slow disk was told, in `C.danger`, under the heading
     * "WHAT IS MISSING", that "the acceptance document is still being read"
     * while it was being fetched normally. This is the same fix the centre pane
     * got (`state.js`, THE FOUR FACTS), applied to the one surface that still
     * had the bug. */
    const wait = (endpoint) => {
      d.reading = endpoint
      d.refusal = null
      d.sections = []
      d.filter = null
      return d
    }
    const db = this.dbPath()
    const openSession = (s, then) => () => {
      this.switchSession(s.key)
      this.setState({ drawer: null })
      if (then) then()
    }

    if (k === 'inbox') {
      /* The Inbox is `os/shell.jsx`'s, and it is the app's ONE attention queue
       * (doctrine 6). The drawer never draws a second one.
       *
       * F08 — WHAT THE SHIPPED INBOX PROVIDES, AND WHAT THE DESIGN'S DRAWER
       * HAD THAT IT DOES NOT. Recorded here rather than left to be rediscovered:
       *
       *  · per-session rows — PROVIDED. `os/shell.jsx` `Inbox()` renders one
       *    row per queue entry with the project, the record's own sentence and
       *    its suggested action.
       *  · `Show me the proof` — PROVIDED under a different name and a
       *    narrower promise: the row's action becomes a link into Trace ONLY
       *    when the row has a claim to point at (`hasReceipt(r)`), and stays
       *    plain text otherwise, deliberately, so it never offers a receipt it
       *    cannot open.
       *  · `Nothing here executes; dismissing appends an acknowledgment` —
       *    DROPPED, and correctly: the shipped Inbox has no dismiss action at
       *    all, so the design's sentence would describe a control that is not
       *    there.
       *  · `Demo records and one local fixture.` — DROPPED: the shipped Inbox
       *    reads the §9 index, never the bundled corpus, so the provenance
       *    sentence would be false on that surface.
       *
       * Nothing here is owed to the OsShell; the two lists above are the whole
       * reconciliation. */
      d.title = 'Inbox — the one attention queue'
      d.lead = 'The Inbox is the app’s one attention queue and it has its own screen.'
      return refuse('the Inbox renders os/shell.jsx, not this drawer', undefined)
    }

    if (k === 'sessions') {
      d.title = 'Sessions'
      if (this.props.localVerification) {
        d.lead = 'Open the agent records captured or imported into this workspace’s selected evidence store.'
        sec('Captured agent activity', null, [], 'The project verification graph contains accepted requirements and verifier observations. Agent records retain their own capture or import provenance.', [{ label: 'Open captured sessions', onClick: () => window.location.assign('/sessions#/inbox') }])
        return d
      }
      /* THE FULL PICKER. This drawer used to list `allSessions()` — the
       * sessions THIS reading had adopted, which on the live path was exactly
       * one — under the lead "Every session this reading has opened, newest
       * first." Both halves of that sentence are wrong for a store view: it is
       * not what this reading opened, and nothing here is ordered by time,
       * because `sessions.started_at` and `ended_at` are NULL on every row.
       * (Its own comment had already narrowed the design's "every session this
       * machine has read" for honesty; the same correction now runs the other
       * way.) It reads the LISTING, and the header's bounded popup is its
       * bounded head.
       *
       * The bundled corpus keeps the old list, because the demo records are not
       * on the store's list and listing the store beside them would be offering
       * rows that have nothing to do with what is on screen. */
      d.width = 620
      if (st.isDemo) {
        d.lead = 'The bundled records this build ships with.'
        if (!all.length) return refuse(src.absent, 'thruline export --all')
        sec(
          null,
          '150px 1fr 130px',
          all.map((s) =>
            this.row(
              [
                this.cell(s.label, { mono: true }),
                this.cell(
                  '“' +
                    (s.asks[0] ? s.asks[0].text : '(no text recorded)') +
                    '” · ' +
                    s.harness +
                    ' · ' +
                    (s.eventCount != null ? countPhrase(s.eventCount, 'event', 'events') : 'event count not in this reading') +
                    (s.countLine ? ' · ' + s.countLine : '') +
                    (s.stop && s.stop.seq != null ? ' · finished on its own' : ' · no ending recorded'),
                ),
                this.cell('', { wordSlug: s.sessionWord, clause: this.sessionClauseOf(s) }),
              ],
              openSession(s),
              s.key === S.key ? C.surf2 : undefined,
            ),
          ),
        )
        return d
      }
      /* A read in flight is not a refusal, here either. */
      if (st.sessions.status === 'loading' || st.sessions.status === 'idle') return wait(API.sessions)
      if (st.sessions.status === 'refused') {
        d.lead = SESSIONS_LEAD_REFUSED
        return refuse(
          (st.sessions.refusal && st.sessions.refusal.missing) || 'the session list was not returned by ' + API.sessions,
          (st.sessions.refusal && st.sessions.refusal.remedy) || 'thruline sessions',
        )
      }
      d.lead = SESSIONS_LEAD
      if (!st.sessions.rows.length) return refuse(src.absent, 'thruline doctor')
      /* THE FILTER, because the list is long. It narrows by the two things a
       * row shows as an address — the id and the repository — and it never
       * re-orders: the engine's ranking is a measurement and this panel does
       * not replace it with a guess. */
      d.filter = {
        value: st.sessionFilter,
        label: 'Filter sessions by id or repository',
        placeholder: 'Filter by id or repository…',
        onChange: (v) => this.setState({ sessionFilter: v }),
      }
      const shown = this.listingRows()
      if (!shown.length) {
        sec(null, '1fr', [this.row([this.cell('No session on this list matches what you typed.', { color: C.inkMute })])])
        return d
      }
      /* Doctrine 4: the note carries the shape of the list in nouns, and says
       * out loud that the list is bounded rather than implying it is all. */
      const note =
        countPhrase(shown.length, 'session', 'sessions') +
        ' on this list' +
        (st.sessions.total != null && st.sessions.total > st.sessions.rows.length
          ? ' · the store holds ' + countPhrase(st.sessions.total, 'session', 'sessions') + ', and this list is bounded'
          : '') +
        ' · read from ' +
        this.dbPath()
      sec(
        null,
        '260px 1fr 130px',
        shown.map((r) => {
          const s = st.corpus.sessions[r.id]
          /* A state word ONLY where the record supports one: the adapter
           * derives it from a session's own claims, asks and promises, so a row
           * this reading has not opened carries none. An invented word, or a
           * negative one without its clause, is a doctrine violation either
           * way. */
          const wordCell = s
            ? this.cell('', { wordSlug: s.sessionWord, clause: this.sessionClauseOf(s) })
            : this.cell('not read yet', { color: C.inkMute })
          return this.row(
            [
              this.cell(r.id, { mono: true }),
              this.cell((r.repo ? r.repo + ' · ' : '') + (r.harness ? r.harness + ' · ' : '') + this.listingLine(r)),
              wordCell,
            ],
            r.openable
              ? () => {
                  this.switchSession(r.id)
                  this.setState({ drawer: null })
                }
              : undefined,
            r.id === st.session ? C.surf2 : undefined,
          )
        }),
        note,
      )
    } else if (k === 'claims') {
      d.title = 'Claims'
      d.lead = 'What each agent claimed, and what the record proves.'
      const rows = []
      all.forEach((s) => {
        s.claims.forEach((c) => {
          rows.push(
            this.row(
              [
                this.cell(s.label, { mono: true }),
                this.cell('“' + c.text + '”', { color: C.inkBright }),
                this.cell('', { wordSlug: c.word, clause: this.clauseOf(c) }),
                this.cell((c.reasons || []).join(' · '), { color: C.inkSoft, absent: 'nothing further recorded' }),
              ],
              openSession(s, () => this.select(c.node, { center: true })),
            ),
          )
        })
        if (!s.claims.length) {
          rows.push(
            this.row([
              this.cell(s.label, { mono: true }),
              this.cell('no completion claims were made', { color: C.inkMute }),
              this.cell(''),
              this.cell(
                (s.eventCount != null ? countPhrase(s.eventCount, 'event', 'events') : 'events not enumerated') + ' and no claim to check against them',
                { color: C.inkMute },
              ),
            ]),
          )
        }
      })
      if (!rows.length) return refuse(src.absent, 'thruline claims --json')
      sec(null, '110px 1fr 100px 1fr', rows, 'Verdicts come from the claims projection; nothing here is recomputed or inferred from prose.')
    } else if (k === 'asks') {
      d.title = 'Asks & promises'
      d.lead = 'What you asked for, and what became of it.'
      const ar = []
      const pr = []
      all.forEach((s) => {
        s.asks.forEach((a) =>
          ar.push(
            this.row(
              [
                this.cell(s.label, { mono: true }),
                this.cell(a.kind, { color: C.inkSoft, wordSlug: a.word, clause: this.clauseOf(a) }),
                this.cell('“' + a.text + '”' + (a.limitation ? ' — ' + a.limitation : '')),
              ],
              openSession(s, () => this.select(a.node, { center: true })),
            ),
          ),
        )
        s.promises.forEach((p) =>
          pr.push(
            this.row(
              [
                this.cell(s.label, { mono: true }),
                this.cell('follow-up', { color: C.inkSoft, wordSlug: p.word, clause: this.clauseOf(p) }),
                this.cell('“' + p.text + '”' + (p.eventId ? ' · opened at ' + short(p.eventId) : '')),
              ],
              openSession(s, () => this.select(p.node, { center: true })),
            ),
          ),
        )
      })
      if (!ar.length && !pr.length) return refuse(src.absent)
      if (ar.length) sec(countPhrase(ar.length, 'ask', 'asks'), '110px 150px 1fr', ar)
      if (pr.length) sec(countPhrase(pr.length, 'open promise', 'open promises'), '110px 150px 1fr', pr)
    } else if (k === 'agreed') {
      d.title = 'Agreed checks'
      d.lead = 'Accepted requirements for this repository, with the digest they were accepted at.'
      const acc = this.state.acceptance
      if (acc.status === 'loading' || acc.status === 'idle') return wait(API.acceptance)
      if (acc.status === 'refused') return refuse(acc.refusal || src.absent, 'thruline ui --acceptance')
      if (acc.status !== 'ready' || !acc.doc) return refuse(src.absent, 'thruline ui --acceptance')
      const doc = acc.doc
      const contract = doc.contract || {}
      const evaluation = doc.evaluation || {}
      const criteria = Array.isArray(evaluation.criteria) ? evaluation.criteria : []
      const declared = Array.isArray(contract.checks) ? contract.checks : []
      /* This note is doctrine, verbatim (spec-doctrine): it states what
       * accepting does and does NOT do, and it is the reason the acceptance
       * states stay inside this fold. */
      const ACCEPT_NOTE =
        'Accepting takes the exact reviewed digest and never executes the document’s commands. ' +
        'The acceptance states (verified · failed · unverified) render only inside this drawer’s raw fold, never as first-level words.'
      sec(
        'Accepted document',
        '140px 1fr',
        [
          this.row([
            this.cell('revision / digest'),
            this.cell(
              doc.accepted && (contract.revision != null || doc.contract_digest)
                ? String(contract.revision == null ? '(no revision recorded)' : contract.revision) + ' · ' + short(doc.contract_digest || '')
                : null,
              { mono: true, absent: 'none — this reading carries no .thruline/acceptance.json' },
            ),
          ]),
          this.row([
            this.cell('criteria'),
            this.cell(criteria.length ? countPhrase(criteria.length, 'accepted criterion', 'accepted criteria') : null, {
              absent: 'none accepted',
            }),
          ]),
          this.row([
            this.cell('declared checks'),
            this.cell(declared.length ? countPhrase(declared.length, 'declared check', 'declared checks') : null, {
              absent: 'none declared; a check the agent ran on its own is the agent’s own run, not an agreed check',
            }),
          ]),
        ],
        ACCEPT_NOTE,
      )
    } else if (k === 'receipts') {
      d.title = 'Receipts'
      d.width = 600
      const V = S.verify || { rows: [] }
      if (!(V.rows || []).length) return refuse(src.absent, 'thruline receipt --session ' + (S.id || '<session id>') + ' --exec-db ' + db + ' --trailer')
      const ok = V.rows.filter((r) => r.outcome === 'pass').length
      const no = V.rows.length - ok
      d.lead = no
        ? 'Receipt partly checked here — ' + countPhrase(no, 'check', 'checks') + ' could not be made; not the same as intact.'
        : 'Receipt intact — every check confirmed here.'
      sec('Lines', '140px 1fr', [
        this.row([this.cell('evidence trust'), this.cell('evidence from this machine')]),
        this.row([
          this.cell('claims'),
          this.cell(S.narr && S.narr.session ? (S.narr.session[this.reg()] || S.narr.session.mgr || [])[1] : null, {
            absent: 'The agent made no completion claim this build recognises.',
          }),
        ]),
        this.row([this.cell('policy'), this.cell(V.policyNote)]),
        this.row([
          this.cell('conclusion'),
          this.cell(V.conclusion + ' · exit ' + V.exit + ' — gate on the conclusion, never on the exit', { mono: true }),
        ]),
      ])
      sec(
        countPhrase(V.rows.length, 'check returned by the verifier', 'checks returned by the verifier'),
        '18px 150px 1fr',
        V.rows.map((r) => {
          const pass = r.outcome === 'pass'
          return this.row([
            this.cell(pass ? '✓' : '◌', { color: pass ? C.green : C.amber }),
            this.cell(r.name, { mono: true }),
            this.cell((pass ? 'confirmed here' : 'could not be made here — not a pass') + (r.detail || r.reason ? ' — ' + (r.detail || r.reason) : ''), {
              color: pass ? C.inkSoft : C.amber,
            }),
          ])
        }),
        /* F05: the producer's badge, named and kept INSIDE this fold. It is
         * the producer's own word about its own work, which is exactly why it
         * never reaches a first-level surface. */
        V.status ? 'As GitHub will see it: badge ' + V.status + ' — the producer’s word, kept inside this fold.' : null,
        [
          copyAct('Copy receipt command', 'thruline receipt --session ' + (S.id || '<session id>') + ' --exec-db ' + db + ' --trailer'),
          copyAct('Copy receipt verification command', 'thruline verify-receipt <receipt file> --offline --exec-db ' + db + ' --format text'),
        ],
      )
    } else if (k === 'bindings') {
      d.title = 'Evidence bindings'
      d.lead = 'No command evidence is bound to this workspace.'
      if (!S.key) return refuse(src.absent)
      sec(
        'Observed inputs',
        '140px 1fr',
        [
          this.row([
            this.cell('workspace tree'),
            this.cell(S.finalTree ? short(S.finalTree) + ' (the final tree of the session under review)' : 'no final tree — nothing to bind to', { mono: true }),
          ]),
          this.row([
            this.cell('bound evidence'),
            this.cell('none — bindings appear after `thruline check -- <your command>` runs in this workspace', { color: C.inkMute }),
          ]),
          this.row([
            this.cell('does existing evidence apply?'),
            this.cell(
              S.checks[0]
                ? 'the session’s check ' + S.checks[0].id + ' ran on ' + short(S.checks[0].inputTree) + ' — ' + S.checks[0].bindingLabel + '; it is session evidence, not a workspace binding'
                : 'no checks in this session',
              { color: C.inkSoft },
            ),
          ]),
        ],
        /* F22: the design's sentence opened `STALE is an answer, not an
         * error`. `STALE` is a binding-state enum, and doctrine 2 keeps a
         * verdict enum off any first-level surface — a drawer note is one. The
         * sentence says the same thing in the reader's words. */
        'Evidence bound to an earlier state is an answer, not an error: a binding states both the bound moment and the current one.',
        [copyAct('Copy check command', 'thruline check -- ' + (S.checks[0] && S.checks[0].command ? S.checks[0].command : '<your command>'))],
      )
    } else if (k === 'ledger') {
      d.title = 'Audit Ledger'
      d.lead = 'The execution store this reading came from.'
      if (!S.chainHead && S.eventCount == null) return refuse(src.absent, 'thruline export --all')
      sec('Execution store identity', '140px 1fr', [
        this.row([this.cell('chain head'), this.cell(S.chainHead, { mono: true, absent: 'not in this reading' })]),
        this.row([
          this.cell('events'),
          this.cell(S.eventCount != null ? countPhrase(S.eventCount, 'event', 'events') : null, { mono: true, absent: 'not in this reading' }),
        ]),
        this.row([this.cell('recorded by'), this.cell(((S.capture || {}).sources || []).join(', '), { mono: true, absent: 'not in this reading' })]),
        this.row([this.cell('store'), this.cell(this.dbPath(), { mono: true })]),
      ])
      sec('Transcript ledger', '1fr', [this.row([this.cell('No transcript ledger has been read yet.', { color: C.inkMute })])], null, [
        copyAct('Copy ingest command', 'thruline ingest --apply'),
      ])
    } else if (k === 'explorer') {
      d.title = 'Explorer'
      d.width = 600
      d.lead = 'Every object across the loaded sessions. Type facets in the filter strip to narrow; every row opens its receipt.'
      const rows = []
      all.forEach((s) => {
        /* ONE POPULATION RULE (DESIGN-readable-trace §2.3). The table filtered
         * `!n.group` and included members; this listed `!n.group && !n.member`
         * and excluded them, so the day grouping shipped the two surfaces would
         * have shown different populations of the same record.
         *
         * The rule is `!n.synthetic`, not §2.3's literal `!n.group`, and the
         * difference is the whole point of the rule. A step row IS a real
         * recorded `tool_call` that has absorbed its result; excluding it for
         * carrying members would drop 1,367 real record objects out of the
         * denominator and make `4243 objects in scope` read `2876`. Only the
         * one card this app synthesised — the working-tree group, which stands
         * for events rather than being one — is not an object of the record. */
        s.nodes
          .filter((n) => !n.synthetic)
          .forEach((n) => {
            const w = n.word ? wordOf(n.word) : null
            rows.push(
              this.row(
                [
                  this.cell(s.label, { mono: true }),
                  this.cell(objType(n), { color: C.inkMute }),
                  this.cell(n.title, { color: C.inkBright, mono: !!n.mono }),
                  this.cell(w && w.word ? w.word : n.seq != null ? 'seq ' + n.seq : '', {
                    glyph: w && w.word ? w.glyph : null,
                    color: w && w.word ? w.ink : C.inkDim,
                    mono: !(w && w.word),
                  }),
                ],
                openSession(s, () => this.select(n.id, { center: true })),
              ),
            )
          })
      })
      if (!rows.length) return refuse(src.absent, 'thruline ingest --apply')
      sec(countPhrase(rows.length, 'object', 'objects') + ' across ' + countPhrase(all.length, 'session', 'sessions'), '110px 60px 1fr 110px', rows)
    } else if (k === 'capture') {
      const cap = S.capture || {}
      if (!cap.sources && !cap.condition) return refuse(src.absent, 'thruline doctor --exec-db ' + db + ' --json')
      d.title = 'Capture & setup'
      d.lead = cap.recorderLine || (cap.degraded ? 'recorded with gaps — cause not recorded' : 'The recorder was working for this session.')
      sec(
        'Recorded capture sources',
        '140px 1fr',
        [
          this.row([this.cell('sources'), this.cell((cap.sources || []).join(', '), { mono: true, absent: 'not in this reading' })]),
          this.row([this.cell('condition'), this.cell(cap.recorderLine || cap.condition, { color: cap.degraded ? C.amber : C.ink })]),
          this.row([
            this.cell('redactions'),
            this.cell(cap.redactions ? countPhrase(cap.redactions, 'redaction', 'redactions') : null, { absent: 'none recorded' }),
          ]),
          this.row([this.cell('harness'), this.cell(S.harness + (S.harnessRaw ? ' (' + S.harnessRaw + ')' : ''))]),
          this.row([
            this.cell('diagnostics'),
            this.cell(cap.doctor, { absent: 'not in this reading — `thruline doctor --json` output is not part of it', color: C.inkMute }),
          ]),
        ],
        'Adapters are named only at the strength the capability tests prove. Deferred integrations are not yet supported.',
        [
          copyAct('Copy capture diagnostic command', 'thruline doctor --exec-db ' + db + ' --json'),
          /* F31: the design's command was
           * `thruline hook-install --harness <h> --scope project`. That verb
           * does not exist in this product: `thruline init` is what composes
           * the sensor pack, the MCP server and the speaker hooks (A200), and
           * it takes neither argument. Recorded so a later reader does not
           * "restore" a command the binary would refuse. */
          copyAct('Copy hook install command', 'thruline init'),
        ],
      )
    } else if (k === 'policy') {
      d.title = 'Policy'
      const rc = S.receipt || {}
      if (!rc.policyHash) return refuse(src.absent, 'thruline policy --session ' + (S.id || '<session id>') + ' --exec-db ' + db + ' --json')
      d.lead = 'No policy in force — nothing is gated.'
      sec(
        'Current scope',
        '140px 1fr',
        [
          this.row([this.cell('policy hash'), this.cell(rc.policyHash, { mono: true })]),
          this.row([this.cell('evaluated'), this.cell((S.verify || {}).policyNote)]),
          this.row([
            this.cell('requirements'),
            this.cell(rc.requirements, {
              absent: 'none — `.thruline/policy.yml` is absent from this reading; `thruline policy --init` writes the documented example',
            }),
          ]),
          this.row([
            this.cell('advisory'),
            this.cell('Not proven claims are advisory: claimed, and no proof found in the record. They never block a PR by themselves.'),
          ]),
        ],
        '',
        [copyAct('Copy policy command', 'thruline policy --session ' + (S.id || '<session id>') + ' --exec-db ' + db + ' --json')],
      )
    } else if (k === 'handoff') {
      d.title = 'Handoff'
      d.width = 600
      d.lead = 'Five categories, kept apart: what the record establishes, what the agent asserted, what is unknown, what is still owed, and what to do next.'
      const c0 = S.claims[0]
      const k0 = S.checks[0]
      sec('Established facts (the record)', '1fr', [
        this.row([
          this.cell(
            S.eventCount != null
              ? countPhrase(S.eventCount, 'event recorded', 'events recorded') + ' · ' + (S.finalTree ? 'final tree ' + short(S.finalTree) : 'no final tree')
              : S.eventsNote,
          ),
        ]),
        k0
          ? this.row([
              this.cell(
                (k0.command || k0.id) + ' → exit ' + k0.exit + (k0.parsed ? ' · ' + k0.parsed.passed + ' passed · ' + k0.parsed.skipped + ' skipped' : '') + ' · ' + k0.bindingLabel,
                { mono: true },
              ),
            ])
          : this.row([this.cell('no checks recorded', { color: C.inkMute })]),
      ])
      sec(
        'Agent assertions',
        '1fr',
        c0
          ? [this.row([this.cell('“' + c0.text + '” — ' + wordOf(c0.word).word, { color: wordOf(c0.word).ink })]), this.row([this.cell(this.clauseOf(c0) || NO_CLAUSE, { color: C.inkSoft })])]
          : [this.row([this.cell('no completion claims were made', { color: C.inkMute })])],
      )
      sec('Unknowns', '1fr', [this.row([this.cell('coverage not measured · source lines not exported · per-test IDs not exported', { color: C.inkSoft })])])
      /* DOC-05: `untouched` is an `AskStatus` identifier, not a display
       * label, and it was hardcoded onto every ask including the ones whose
       * status is `dropped`, `refused` or `scope_shrunk`. The line now names
       * the obligation and the clause rides beneath it, exactly as the
       * `Agent assertions` section above does.
       * F28: the section is emitted UNCONDITIONALLY — an absent heading is
       * indistinguishable from a drawer that forgot to build it. */
      const owed = S.promises
        .map((p) => [
          this.row([this.cell('open promise: “' + p.text + '”')]),
          this.row([this.cell(this.clauseOf(p) || NO_CLAUSE, { color: C.inkSoft })]),
        ])
        .concat(
          S.asks.map((a) => [
            this.row([this.cell('ask still owed: “' + a.text + '”')]),
            this.row([this.cell(this.clauseOf(a) || NO_CLAUSE, { color: C.inkSoft })]),
          ]),
        )
        .reduce((acc, pair) => acc.concat(pair), [])
      sec(
        'Remaining obligations',
        '1fr',
        owed.length ? owed : [this.row([this.cell('Nothing is still owed from this session.', { color: C.inkMute })])],
      )
      sec(
        'Recommendations',
        '1fr',
        [
          this.row([
            this.cell(
              c0 && c0.nextCheck
                ? 'run ' + c0.nextCheck + ' on the final tree, with skipped tests accounted for'
                : c0 && c0.word === 'proven'
                  ? 'nothing owed for the declared check; the open ask is the reader’s call'
                  : 'this reading carries no next check for the claim; the open ask is the reader’s call',
              { mono: !!(c0 && c0.nextCheck) },
            ),
          ]),
        ],
        'Export does not confirm a handoff. Nothing left this machine.',
        [copyAct('Copy handoff command', 'thruline handoff --project ' + (S.project || '<project>'))],
      )
    } else if (k === 'diag') {
      d.title = 'Operator diagnostics'
      d.lead = 'Pins, shapes, and exit codes for this reading.'
      if (!Object.keys(S.exits || {}).length) return refuse(src.absent, 'thruline ui')
      /* F03: the drawer's first section. It is sourced only for the bundled
       * corpus — those pins and shapes are pinned fixture inventory, not
       * something a live reading carries — so a real reading gets a named
       * refusal in the section rather than a fabricated one. */
      if (this.state.isDemo) {
        sec('Pinned', '140px 1fr', [
          this.row([this.cell('cores'), this.cell(DEMO_META.pins, { mono: true })]),
          this.row([
            this.cell('shapes known'),
            this.cell(
              (DEMO_META.shapes.known || []).join(' · ') + ' · fixture ' + DEMO_META.shapes.fixture + ' / head ' + DEMO_META.shapes.head,
              { mono: true },
            ),
          ]),
        ])
      } else {
        sec('Pinned', '140px 1fr', [
          this.row([
            this.cell('cores'),
            this.cell('not in this reading — pins and shape inventories come from a pinned fixture bundle', { color: C.inkMute }),
          ]),
        ])
      }
      sec(
        'Exit codes · ' + S.label,
        '140px 1fr',
        Object.keys(S.exits).map((x) => this.row([this.cell(x, { mono: true }), this.cell(String(S.exits[x]), { mono: true })])),
        'An exit 3 means a claim is not proven and still serves HTTP 200. Never gate on an exit code.',
      )
      sec('Reading', '140px 1fr', [
        this.row([this.cell('read at'), this.cell((readStamp(this.state.lastReadAt) || {}).title || '', { mono: true, absent: 'not read yet' })]),
        this.row([this.cell('connection'), this.cell(this.connLine(true))]),
      ])
    } else if (k === 'testchanges') {
      d.title = 'Show test changes'
      if (!S.testChanges.length) return refuse('no test changes are recorded for this session')
      d.lead = 'Recorded assertion changes at the final tree, beside their paths.'
      sec(
        null,
        '130px 120px 1fr',
        S.testChanges.map((t) => {
          const f = this.findingByKey(t.detector)
          return this.row([this.cell(t.test, { mono: true }), this.cell(t.file, { mono: true }), this.cell(t.change + ' · ' + (f ? f.name : t.detector))], () => {
            this.setState({ drawer: null })
            if (f) {
              this.setState({ selFinding: f.key })
              this.select(f.node, { center: true })
            }
          })
        }),
        'A code diff is shown only when before/after file content is available. This reading holds the assertion census, not the file bodies — so no diff.',
      )
    } else {
      d.title = k
      return refuse('no source is declared for the destination “' + k + '”')
    }
    return d
  }

  /* ── Palette & rail (spec-script §B.13) ──────────────────────────────── */

  /**
   * The ⌘K item list, in fixed section order, memoised on `(session, query)`.
   * The design wrote it during render and read it from a keyboard handler,
   * which in StrictMode may read a discarded render (spec-script §E6).
   *
   * @returns {Array<{kind:string,label:string,hint:string,act:Function,font:string}>}
   */
  palRaw() {
    const st = this.state
    const S = this.S()
    const unread = this.unread()
    /* TRACE-MISSING-SESSION-TRUE (verifier try 3): the read state is in the key. */
    const key = (unread ? 'unread ' : '') + S.key + ' ' + st.palQ + ' ' + st.filters.length
    if (this.memo.pal && this.memo.pal.key === key) return this.memo.pal.items
    const q = st.palQ.trim().toLowerCase()
    const items = []
    const push = (kind, label, hint, act, font) => {
      if (q && (label + ' ' + (hint || '') + ' ' + kind).toLowerCase().indexOf(q) < 0) return
      items.push({ kind, label, hint: hint || '', act, font: font || 'inherit' })
    }
    RAIL.forEach((g) => g.items.forEach((it) => push('go to', it[1], g.title, () => this.railGo(it[0]))))
    push('view', 'Graph', 'representation', () => this.setState({ rep: 'graph', doc: 'trace' }))
    push('view', 'Timeline', 'representation', () => this.setState({ rep: 'timeline', doc: 'trace' }))
    push('view', 'Table', 'representation', () => this.setState({ rep: 'table', doc: 'trace' }))
    if (!unread) push('action', 'Line the claim up against the check', 'claim vs check', () => this.lineUp())
    push('action', 'Fit graph', 'F', () => this.fit())
    if (!unread) push('action', 'Jump to stop', '', () => this.jumpStop())
    push('action', 'Reset layout', '', () => this.resetLayout())
    push('action', 'Refresh reading', '', () => this.refresh())
    push('action', 'Compare sessions', '', () => this.openDoc('compare'))
    S.nodes
      .filter((n) => !n.member)
      .forEach((n) =>
        push('object', n.title, n.objId || (n.seq != null ? 'seq ' + n.seq : n.id), () => this.select(n.id, { center: true, doc: true }), n.mono ? FONT.mono : 'inherit'),
      )
    const opts = this.facetOpts()
    Object.keys(opts).forEach((f) =>
      Object.keys(opts[f]).forEach((v) =>
        push('filter', f + ':' + v, countPhrase(opts[f][v], 'object', 'objects'), () => this.addFilter(f + ':' + v), FONT.mono),
      ),
    )
    st.corpus.order.forEach((sk) => {
      const s = st.corpus.sessions[sk]
      if (s) push('session', s.label, s.countLine || '', () => this.switchSession(sk), FONT.mono)
    })
    this.commands().forEach((c) => {
      if (c.cmd) push('command', c.label, c.missing || 'copying does not execute', () => this.copy(c.cmd, c.label.replace(/^Copy /, '')))
    })
    st.output
      .slice(-6)
      .reverse()
      .forEach((o) => push('recent', o.text.slice(0, 80), o.t, () => this.setState({ dockTab: 'output' })))
    this.memo.pal = { key, items }
    return items
  }

  /**
   * "Find in trace" matches, memoised on `(session, query)` for the same reason
   * as `palRaw()`.
   *
   * @returns {object[]}
   */
  findMatches() {
    const st = this.state
    const S = this.S()
    const q = st.findText.trim().toLowerCase()
    const key = S.key + ' ' + q
    if (this.memo.find && this.memo.find.key === key) return this.memo.find.items
    let items = []
    if (q) {
      items = S.nodes.filter((n) => {
        const hay = (n.title + ' ' + (n.lines || []).join(' ') + ' ' + (n.foot || '') + ' ' + (n.objId || '') + ' ' + (n.file || '')).toLowerCase()
        return hay.indexOf(q) >= 0
      })
      /* Searching hidden members is already right and stays. What a reader
       * cannot see is WHERE a hit lives, so a hit inside a fold names the card
       * that currently stands for it — otherwise "centre on it" is a jump to
       * a place that is not on screen. */
      items = items.map((n) => {
        if (!n.member) return n
        const g = this.nodeById(n.member)
        return g ? { ...n, inBand: g.title || g.sub, inBandId: g.id } : n
      })
    }
    this.memo.find = { key, items }
    return items
  }

  /**
   * Write the active destination into the location hash, so a reader can
   * bookmark a screen and a reload does not dump them back on Trace (R12).
   *
   * `replaceState`, not `pushState`, and no `popstate` listener: this build
   * has none anywhere in `ui/src` and adding one is a separate change with its
   * own deep-linking contract (`canvas.jsx`). This is the smaller promise —
   * the address names the screen, and the screen is restored on load.
   *
   * @param {string} key a `logic.RAIL` destination key
   */
  writeRoute(key) {
    if (typeof window === 'undefined' || !window.history || !window.history.replaceState) return
    try {
      /* The bare address is HOME, and home is the Inbox. Trace therefore
       * writes `#/trace` like every other destination, which is also what
       * keeps a reload on Trace on Trace: before this, Trace cleared the hash
       * and a reload landed back on the default. Every existing `#/<key>` deep
       * link — `#/trace` included — is read unchanged by `routeFromHash`. */
      const next = key && key !== 'inbox' ? '#/' + key : window.location.pathname + window.location.search
      window.history.replaceState(null, '', next)
    } catch (_routeError) {
      /* a sandboxed or file: document refuses history writes; the app works
         without an address and says nothing it cannot back up */
    }
  }

  /**
   * Put the open session in the address, so a reader can copy the link to the
   * session they are looking at and hand it to somebody.
   *
   * The QUERY STRING, not the hash: `routeFromHash` matches `^#\/([a-z]+)$`
   * against `logic.RAIL` keys and the hash names the SCREEN, which a session id
   * is not. The house already addresses things this way (`os/shell.jsx`'s
   * `?project=` and `?node=`), and the two compose without changing either —
   * `writeRoute` passes `window.location.search` through when it writes home,
   * so `/?session=<id>#/trace` is one coherent address.
   *
   * @param {string} id the session id, or a falsy value to drop the parameter
   */
  writeSession(id) {
    if (typeof window === 'undefined' || !window.history || !window.history.replaceState) return
    try {
      const q = new URLSearchParams(window.location.search)
      if (id) q.set('session', id)
      else q.delete('session')
      const s = q.toString()
      window.history.replaceState(null, '', window.location.pathname + (s ? '?' + s : '') + window.location.hash)
    } catch (_addressError) {
      /* a sandboxed or file: document refuses history writes; the app works
         without an address and says nothing it cannot back up */
    }
  }

  /**
   * The session id this page was opened at, or null.
   *
   * The parse itself is `state.sessionIdFromSearch`, which is pure and pinned;
   * this only supplies the address. It returns null for anything the server's
   * own parameter grammar would refuse (`uiserve.rs:1387-1394`) — that is not a
   * duplicate of the server's rule, it is the app declining to send a request
   * it already knows will be refused.
   *
   * @returns {string|null}
   */
  static sessionFromQuery() {
    if (typeof window === 'undefined') return null
    return sessionIdFromSearch(window.location.search)
  }

  /**
   * The destination named by the location hash on load, or null.
   *
   * @returns {string|null} a key that exists in `logic.RAIL`, never a guess
   */
  static routeFromHash() {
    if (typeof window === 'undefined') return null
    const m = /^#\/([a-z]+)$/.exec(window.location.hash || '')
    if (!m) return null
    const keys = []
    RAIL.forEach((g) => g.items.forEach((it) => keys.push(it[0])))
    return keys.indexOf(m[1]) >= 0 ? m[1] : null
  }

  /**
   * Open a destination, idempotently — opening one that is already open is not
   * a close. This is what a restored address goes through; the rail's own
   * click goes through `railGo`, which toggles because clicking the open
   * drawer's own rail button is how a reader shuts it.
   *
   * @param {string} key a `logic.RAIL` destination key
   */
  openDestination(key) {
    if (key === 'agreed') {
      this.setState({ drawer: 'agreed', destination: 'trace', evidenceTab: 'verify', panels: { ...this.state.panels, right: true } })
      return
    }
    if (key === 'trace') {
      this.setState({ drawer: null, doc: 'trace', destination: 'trace' })
      return
    }
    if (key === 'inbox') {
      /* A restored `#/inbox` address never reaches this shell — `main.jsx`
       * mounts the Inbox screen for it (T-2-6). The branch stays because
       * `openDestination` is the idempotent door for every rail key, and a
       * key with no branch would silently open a drawer named `inbox`. */
      goToInbox(typeof window === 'undefined' ? null : window)
      return
    }
    if (key === 'compare') {
      this.setState({ destination: 'trace' })
      this.openDoc('compare')
      return
    }
    if (key === 'agreed' && this.state.acceptance.status === 'idle') this.loadAcceptance()
    this.setState({ drawer: key, destination: 'trace' })
  }

  /**
   * Go to a rail destination. `trace` closes the drawer and shows the canvas;
   * `inbox` shows the shipped `os/shell.jsx`; `compare` opens its document tab;
   * everything else toggles its drawer.
   *
   * @param {string} key
   */
  railGo(key) {
    /* R13: remember what had focus so closing the drawer can give it back. */
    this.drawerTrigger = typeof document !== 'undefined' ? document.activeElement : null
    this.setState({ menu: null, palette: false })
    if (key === 'agreed') {
      const opening = this.state.drawer !== 'agreed' || !this.state.panels.right
      this.setState({ drawer: opening ? 'agreed' : null, destination: 'trace', evidenceTab: 'verify', panels: { ...this.state.panels, right: opening } })
      this.writeRoute(opening ? 'agreed' : 'trace')
      return
    }
    if (key === 'trace') {
      this.setState({ drawer: null, doc: 'trace', destination: 'trace' })
      this.writeRoute('trace')
      return
    }
    if (key === 'inbox') {
      /* The control labelled Inbox goes to the Inbox and nowhere else
       * (T-2-6, `B8.1`). It used to open the captured-session picker over
       * this view, which is the baseline's worst moment (H2T-3): a person
       * clicked the word Inbox and met a list of sessions on top of the
       * technical canvas. The Inbox is its own screen now, mounted by
       * `main.jsx` at `#/inbox`, so going there is an address change and a
       * boot — the same thing the Change screen does for the link to the
       * whole story on one page. */
      goToInbox(typeof window === 'undefined' ? null : window)
      return
    }
    if (key === 'compare') {
      this.setState({ destination: 'trace' })
      this.openDoc('compare')
      this.writeRoute('compare')
      return
    }
    if (key === 'agreed' && this.state.acceptance.status === 'idle') this.loadAcceptance()
    this.setState((s) => ({ drawer: s.drawer === key ? null : key, destination: 'trace' }))
    this.writeRoute(this.state.drawer === key ? 'trace' : key)
    this.log('Opened ' + key)
  }

  /**
   * Read `/api/acceptance`, once, when the `Agreed checks` drawer first asks
   * for it. F04: this drawer used to refuse unconditionally, which said "no
   * acceptance document" on machines that have one.
   *
   * @returns {Promise<void>}
   */
  async loadAcceptance() {
    if (this.state.acceptance.status === 'loading') return
    this.setState({ acceptance: { status: 'loading' } })
    const read = await readProjection(API.acceptance)
    if (!this.live) return
    this.setState({ acceptance: read && read.status ? read : { status: 'refused', refusal: 'no answer from ' + API.acceptance } })
  }

  /** Store the current lens as a named view — the `ok` chips only, on this machine. */
  saveView() {
    const st = this.state
    const name = st.saveName.trim()
    if (!name || !st.filters.length) {
      this.toast(st.filters.length ? 'Name the view first.' : 'Add at least one filter chip first.')
      return
    }
    const v = { name, tokens: st.filters.filter((c) => c.ok).map((c) => c.text) }
    this.setState({ savedViews: st.savedViews.filter((x) => x.name !== name).concat([v]), saveName: '', menu: null })
    this.log('Saved view “' + name + '” · ' + v.tokens.join(' ') + ' · stored locally')
  }

  /**
   * Compare two readings, twelve aligned rows. Every word carries its clause,
   * and every zero renders an honest sentence — never `none`
   * (spec-doctrine C-8…C-10, F-6).
   *
   * @returns {object}
   */
  compareData() {
    const st = this.state
    const A = st.corpus.sessions[st.compare.a]
    const B = st.corpus.sessions[st.compare.b]
    const options = st.corpus.order.map((k) => ({ key: k, label: (st.corpus.sessions[k] || {}).label || k }))
    const T = (t, o) => {
      const opt = o || {}
      return {
        text: t,
        color: opt.color || C.ink,
        font: opt.mono ? FONT.mono : 'inherit',
        fontSize: opt.mono ? FS.ctl : FS.body,
        wordSlug: opt.wordSlug || null,
        clause: opt.clause === undefined ? null : opt.clause,
      }
    }
    const side = (S) => {
      if (!S) return {}
      const c = S.claims[0]
      const k = S.checks[0]
      const cap = S.capture || {}
      const V = S.verify || { rows: [] }
      return {
        word: [
          T(S.countLine || '', {
            color: C.inkSoft,
            wordSlug: S.sessionWord,
            clause: this.sessionClauseOf(S),
          }),
        ],
        scope: [
          T(S.eventsNote ? S.eventsNote : countPhrase(S.eventCount, 'event', 'events') + ' · complete record for this reading', {
            color: S.eventsNote ? C.amber : C.inkSoft,
          }),
        ],
        capture: [T(cap.recorderLine || (cap.degraded ? 'recorded with gaps — cause not recorded' : 'The recorder was working for this session.'), { color: cap.degraded ? C.amber : C.ink })],
        claims: c
          ? [T('“' + c.text + '”', { color: C.inkBright, wordSlug: c.word, clause: this.clauseOf(c) }), T((c.reasons || []).join(' · '), { color: C.inkSoft })]
          : [T('no completion claims were made', { color: C.inkMute })],
        findings:
          c && (c.findings || []).length
            ? c.findings.map((f) => T(f.name + ' · ' + severityOf(f.severity).label, { color: severityOf(f.severity).color }))
            : [T('No findings are recorded against this claim.', { color: C.inkMute })],
        checks: k
          ? [
              T((k.command || k.id) + ' → exit ' + k.exit, { mono: true }),
              T(
                (k.parsed ? k.parsed.passed + ' passed · ' + k.parsed.failed + ' failed · ' + k.parsed.skipped + ' skipped · ' + k.parsed.tests_total + ' total · ' : '') + k.bindingLabel,
                { color: C.inkSoft },
              ),
            ]
          : [T('No checks are recorded in this session.', { color: C.inkMute })],
        promises: S.promises.length
          ? S.promises.map((p) => T('“' + p.text + '”', { color: C.ink, wordSlug: p.word, clause: this.clauseOf(p) }))
          : [T('No promises are open in this session.', { color: C.inkMute })],
        asks: S.asks.length
          ? S.asks.map((a) => T('“' + a.text + '”' + (a.limitation ? ' — ' + a.limitation : ''), { color: C.ink, wordSlug: a.word, clause: this.clauseOf(a) }))
          : [T('No asks were extracted from this session.', { color: C.inkMute })],
        risk: (() => {
          const r = S.events.filter((e) => e.risk)
          return r.length
            ? r.map((e) => T('seq ' + e.seq + ' · ' + e.risk, { color: C.amber, mono: true }))
            : [T('No risk flags are recorded in this session.', { color: C.inkMute })]
        })(),
        ending: [
          T(S.stop && S.stop.seq != null ? 'finished on its own · seq ' + S.stop.seq : 'no ending recorded', {
            color: S.stop && S.stop.seq != null ? C.ink : C.inkDim, // Fog's ink: neutral fails 4.5:1 as text (P6-6)
            wordSlug: S.stop && S.stop.seq != null ? null : 'fog',
            clause: this.clauseOf(S.stop),
          }),
        ],
        receipt: [
          T(
            (V.rows || []).length
              ? (V.integrity === 'verified' ? 'intact — every check confirmed here' : 'partly checked here — not the same as intact') + ' · conclusion ' + V.conclusion + ' · exit ' + V.exit
              : 'No receipt was read for this session.',
            { color: V.integrity === 'verified' ? C.ink : C.amber },
          ),
        ],
        tree: [T(S.finalTree ? 'final tree ' + short(S.finalTree) : 'no final tree — nothing to bind to', { mono: true })],
      }
    }
    const a = side(A)
    const b = side(B)
    const keys = [
      ['word', 'Session line'],
      ['scope', 'Scope of this reading'],
      ['capture', 'Capture'],
      ['claims', 'Claims · aligned on “tests pass”'],
      ['findings', 'Findings'],
      ['checks', 'Checks'],
      ['promises', 'Promises'],
      ['asks', 'Asks'],
      ['risk', 'Risk flags'],
      ['ending', 'Ending'],
      ['receipt', 'Receipt'],
      ['tree', 'Binding'],
    ]
    return {
      a: st.compare.a,
      b: st.compare.b,
      aLabel: A ? A.label : '',
      bLabel: B ? B.label : '',
      options,
      rows: keys.map((k) => ({ k: k[1], a: a[k[0]] || [], b: b[k[0]] || [] })),
      /* F02: this strip states where the two readings came from. It was
       * hardcoded to '' while its only source sat unimported in
       * `demo-data.js`, so the element rendered blank forever. The bundled
       * corpus states its own pinned provenance verbatim; a real reading
       * states the store it was actually read from. */
      /* "N sessions read from <db>" read as a claim about the STORE while the
       * corpus was one session. The corpus is what this reading has OPENED, and
       * the store's own size is the listing's fact, not this line's — so this
       * says which it is naming. */
      autopsy: st.isDemo
        ? DEMO_META.autopsyHeader
        : countPhrase(st.corpus.order.length, 'session', 'sessions') +
          ' opened here, from ' +
          this.dbPath() +
          '. Times are UTC. Nothing left this machine.',
    }
  }

  /* ── The view model (spec-script §B.14, option (a)) ──────────────────── */

  /**
   * THE GRAPH'S CALLBACK BUNDLE, BUILT ONCE.
   *
   * `React.memo` on `NodeCard` is worth nothing if its handler props change
   * identity every render, and `GraphCanvas`'s wheel `useEffect` keys on
   * `on.canvasWheel`, so a fresh bundle re-binds a native listener on every
   * pan. Every closure below therefore reads `this.state` and `this.vals` at
   * CALL time rather than closing over the render that made it — which is also
   * why it may never be rebuilt: a stale bundle would be a stale state read.
   *
   * @returns {Record<string, Function>}
   */
  graphOn() {
    if (this.graphOnCache) return this.graphOnCache
    this.graphOnCache = {
      canvasDown: (e) => {
        if (e.button !== 0) return
        this.drag = { type: 'pan', sx: e.clientX, sy: e.clientY, ox: this.state.view.x, oy: this.state.view.y }
        this.setState({ findOpen: false, menu: null })
      },
      canvasWheel: () => {},
      onFit: () => this.fit(),
      onNodeDown: (...args) => {
        const { id, e } = idEvt(args)
        if (!e || e.button !== 0) return
        e.stopPropagation()
        const n = this.nodeById(id)
        if (!n) return
        const g = this.geo(n)
        this.select(id)
        this.drag = { type: 'node', id, sx: e.clientX, sy: e.clientY, ox: g.x, oy: g.y, moved: false }
      },
      onNodeKey: (...args) => {
        const { id, e } = idEvt(args)
        if (!e) return
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          this.select(id, { center: true })
        } else if (/^Arrow/.test(e.key)) {
          e.preventDefault()
          const n = this.nodeById(id)
          if (!n) return
          /* A group states what it contains and opens on click AND by
           * keyboard. Right opens, left closes — the tree gesture every
           * reader already has — and the nudge keeps the other two arrows. */
          if (n.group && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
            const want = e.key === 'ArrowRight'
            if (!!this.state.expanded[id] === want) return
            this.setState({ expanded: { ...this.state.expanded, [id]: want } })
            this.log((want ? 'Expanded ' : 'Collapsed ') + n.sub)
            return
          }
          const g = this.geo(n)
          const d = e.shiftKey ? 1 : CLAMP.nodeStep
          this.setState({
            pos: {
              ...this.state.pos,
              [id]: {
                x: g.x + (e.key === 'ArrowRight' ? d : e.key === 'ArrowLeft' ? -d : 0),
                y: g.y + (e.key === 'ArrowDown' ? d : e.key === 'ArrowUp' ? -d : 0),
              },
            },
          })
        }
      },
      onNodeDouble: (...args) => {
        const { id, e } = idEvt(args)
        if (e && e.stopPropagation) e.stopPropagation()
        const n = this.nodeById(id)
        if (n && n.group) this.setState({ expanded: { ...this.state.expanded, [id]: !this.state.expanded[id] } })
        else this.setState({ inspTab: 'evidence' })
      },
      onNodeToggle: (...args) => {
        const { id, e } = idEvt(args)
        if (e && e.stopPropagation) e.stopPropagation()
        const n = this.nodeById(id)
        if (isFocusGroupId(id)) {
          /* A focus group opens in place: its members render as today. */
          this.setState(this.openGroup(id))
          this.log('Expanded ' + (n ? n.sub : id) + ' in place')
          return
        }
        const next = !this.state.expanded[id]
        this.setState({ expanded: { ...this.state.expanded, [id]: next } })
        this.log((next ? 'Expanded ' : 'Collapsed ') + (n ? n.sub : id))
      },
      onWireEnter: (...args) => this.setState({ hoverWire: idEvt(args).id }),
      onWireLeave: () => this.setState({ hoverWire: null }),
      onWireClick: (...args) => {
        const { id, e } = idEvt(args)
        if (e && e.stopPropagation) e.stopPropagation()
        const w = this.vals.graph.allWires.find((x) => x.id === id)
        if (w) this.select(w.b === this.state.sel ? w.a : w.b)
      },
      /* THE OVERVIEW IS A CONTROL, NOT A PICTURE (M2). A press jumps the view
       * AND starts a drag, so the viewport rectangle can be dragged down a
       * 130,000 px trace instead of clicked at. The mapping is a straight read
       * of `viewport.minimapLayout`, which is the same object the marks were
       * drawn with, so where the reader aims is where the view lands.
       *
       * Accepts either graph coordinates or the raw mouse event; both callers
       * are served, as before. */
      onMinimapDown: (a, b) => {
        if (a && a.currentTarget && a.currentTarget.getBoundingClientRect) {
          a.stopPropagation()
          const mm = this.vals && this.vals.mm
          if (!mm) return
          const r = a.currentTarget.getBoundingClientRect()
          this.drag = { type: 'minimap', rect: { left: r.left, top: r.top, width: r.width, height: r.height } }
          this.minimapTo(a.clientX, a.clientY, this.drag.rect)
          return
        }
        const v = this.state.view
        this.setState({ view: { x: this.state.cw / 2 - a * v.z, y: this.state.ch / 2 - b * v.z, z: v.z } })
      },
    }
    return this.graphOnCache
  }

  /**
   * THE GRAPH MODEL: every card, every wire and the overview's raster — all of
   * what the canvas draws — memoised on `key`.
   *
   * `key` names every input this reads, and `view` is deliberately NOT one of
   * them, because nothing about a card changes when the reader pans. Before
   * this existed, one mousemove rebuilt 1,481 node view-models, ~1,480 wires
   * and 1,481 overview rects, and React then reconciled ~28,700 elements to
   * arrive at the identical tree; measured on a long session, one pan
   * mousemove cost 678 ms (median of 10).
   *
   * `view` is consumed after this returns, by exactly three things: the
   * transform string, the window, and the overview's viewport rectangle.
   *
   * @param {string} key @param {object} ctx from `ctx()` @param {object} ui
   * @returns {{key:string, vnodes:object[], frames:object[], nodes:object[],
   *            wires:object[], mm:object|null}}
   */
  graphModel(key, ctx, ui) {
    if (this.memo.graph && this.memo.graph.key === key) return this.memo.graph
    const st = this.state
    const S = this.S()
    const vnodes = this.visibleNodes()
    const linked = this.linkedSet(st.sel)
    const frames = this.framesOf()
    /* Step 5: every node of a "Show proof" chain wears the amber selection
     * outline, not only `sel`. The legend's words for amber are unchanged. */
    const chain = {}
    if (st.focusMode === 'focus' && Array.isArray(st.focusChain)) st.focusChain.forEach((id) => (chain[id] = true))
    /* P6-3: the face (Plain or Technical, the ONE register) and the lit
     * thread. A lit card wears amber and the one glow token (§4); every card
     * off the thread is dimmed, never hidden. */
    const face = this.face()
    const plain = face === 'plain'
    const TR = HQ_COPY.trace
    const thread = this.thread()
    const lit = this.litOf(thread)
    const threadOn = !!thread
    const threadN = thread ? thread.ids.length : 0
    const nodes = vnodes.map((n) => {
      const g = this.geo(n)
      const fam = FAMof(n.fam)
      const onThread = threadOn && lit.nodes[n.id] != null
      const dim = this.isDim(n, ctx) || (threadOn && !onThread)
      const sel = n.id === st.sel
      const outlined = sel || !!chain[n.id] || onThread
      const pf = plain ? plainFace(n) : null
      const facts = factsLines(runFacts(n, S), face)
      const onPath = st.linked && linked[n.id]
      const rows = Math.max((n.pinsL || []).length, (n.pinsR || []).length)
      const pins = (list) =>
        (list || []).map((p, i) => {
          const ps = pinStyle(p)
          return {
            id: p.id,
            top: i * 20,
            bg: ps.bg,
            border: ps.border,
            radius: ps.radius,
            clip: ps.clip,
            /* Plain keeps the square and says the line kind; the pin's own
             * label ("span", "tree", "evidence") is Technical. */
            label: plain ? '' : p.label,
            title: p.t === 'order' ? TR.edges.order : p.t === 'inf' ? TR.edges.inferred + (plain ? '' : ' · ' + p.label) : TR.edges.reference + (plain ? '' : ' · ' + p.label),
          }
        })
      const f = n.finding ? this.findingByKey(n.finding) : null
      return {
        id: n.id,
        x: g.x,
        y: g.y,
        w: g.w,
        /* `h` is not read by the card — a card sizes itself — it is read by the
         * WINDOW, which asks whether this box is near the view. `estH` is the
         * only authority on a card's height and it is already computed here. */
        h: g.h,
        opacity: dim ? 0.22 : 1,
        hdr: fam[0],
        hdrText: fam[1],
        /* T-6-4: a run with no recorded result is drawn as §4 draws "not
         * recorded" — a dashed edge in the neutral hue (3.99:1 on black), not
         * a hairline (1.47:1) — unless it is lit or selected. */
        border: outlined ? C.amber : onPath ? C.focus : facts.hollowLine ? C.neutral : C.line2,
        shadow: onThread
          ? `0 0 0 1px ${C.amber}, ${GLOW.thread}`
          : outlined ? `0 0 0 1px ${C.amber}, 0 8px 22px ${ALPHA.findShadow}` : `0 3px 10px ${ALPHA.nodeHeaderUnderline}`,
        sel,
        lit: onThread,
        /* The thread is never told by colour alone: a lit card says where it
         * stands on the thread, in words. */
        threadMark: onThread ? fill(TR.thread.position, { i: lit.nodes[n.id] + 1, n: threadN }) : null,
        type: pf ? pf.type : n.type,
        sub: pf ? pf.sub : f ? 'Finding' : n.sub,
        /* A finding carries a SEVERITY and never a state word (F-4). */
        wordSlug: f ? null : n.word || null,
        clause: f ? null : this.clauseOf(n),
        severity: f ? f.severity : null,
        hdrRight: pf ? (this.replayState(n, ctx.rseq) === 'partial' ? plainText(this.cardSpan(n, ctx)) : pf.hdrRight) : this.cardSpan(n, ctx),
        group: !!n.group,
        members: n.members || null,
        expanded: !!st.expanded[n.id],
        /* `NodeCard` has read `toggleLabel`/`toggleAria` since it was written
         * and NOTHING in `ui/src` ever assigned either, so every group toggle
         * on a real session would have rendered as a button with no text. The
         * level-of-detail pass states them; the caret is the only part that
         * belongs to the reading rather than to the record. */
        toggleLabel: n.group ? (st.expanded[n.id] ? '▾' : '▸') + (n.toggleLabel ? n.toggleLabel.slice(1) : '') : null,
        /* N13-F11-AXE-SR: a group the record names nothing for still gets a name. */
        toggleAria: n.group ? (n.toggleAria ? (st.expanded[n.id] ? 'Collapse' + String(n.toggleAria).replace(/^Expand/, '') : n.toggleAria) : (st.expanded[n.id] ? HQ_COPY.a11y.group_close : HQ_COPY.a11y.group_open)) : null,
        pinsH: rows ? rows * 20 : 4,
        pinsL: pins(n.pinsL),
        pinsR: pins(n.pinsR),
        title: pf ? pf.title : objType(n) === 'check' ? checkTitle(n) : n.title,
        titleFont: n.mono && !plain ? FONT.mono : 'inherit',
        lines: (pf ? pf.lines : n.lines || []).map((l) => ({ text: l, color: C.inkSoft, style: 'normal' })),
        /* Where a run ran (typed fields only), the earlier-version band, the
         * hollow card for a run with no result, and the agent's-words line. */
        facts: facts.rows,
        factsAbsent: facts.absentLine,
        band: facts.band,
        bandWhy: facts.bandWhy,
        hollow: !!facts.hollowLine,
        hollowLine: facts.hollowLine,
        agentLine: isAgentWords(n) ? TR.agent_reported : null,
        foot: pf ? pf.foot : n.foot,
        aria: (pf ? pf.aria : n.type + ' ' + n.sub + ': ' + n.title) + (onThread ? ' · ' + fill(TR.thread.position, { i: lit.nodes[n.id] + 1, n: threadN }) : ''),
      }
    })
    const nodeDim = {}
    nodes.forEach((n) => (nodeDim[n.id] = n.opacity < 1))
    const wires = this.allWires().map((w) => {
      const b = bez(w.pa.x, w.pa.y, w.pb.x, w.pb.y)
      const touches = w.a === st.sel || w.b === st.sel
      const hover = st.hoverWire === w.id
      /* P6-3: a line is painted by its KIND (solid recorded order, dashed
       * documented reference, dotted inferred, dash-dot for a kind this view
       * does not know), never by the family of the card it leaves. A thread
       * line keeps its kind's pattern and takes amber. */
      const kind = edgeKindOf(w)
      const onThread = !!lit.wires[w.id]
      const dim = !onThread && (nodeDim[w.a] || nodeDim[w.b])
      const color = onThread ? C.amber : EDGE_STYLE[kind].color
      const words = plain ? TR.edges[kind] : w.label
      const label = hover ? words : words.length > 46 ? words.slice(0, 44) + '…' : words
      return {
        id: w.id,
        a: w.a,
        b: w.b,
        /* A wire is windowed by its OWN bounding box, control points included
         * (`viewport.wireBox`), never by whether its endpoints are on screen —
         * a wire crossing the viewport from off-screen to off-screen must
         * still be drawn. */
        box: wireBox(w.pa, w.pb),
        d: b.d,
        color,
        width: onThread || touches || hover ? 2.6 : 1.6,
        dash: EDGE_STYLE[kind].dash,
        cap: EDGE_STYLE[kind].cap,
        /* `kind` is the line kind drawn; `recordKind` is the wire's own
         * `kind` as the record gave it, which is what the legend classifies. */
        kind,
        recordKind: w.kind,
        lit: onThread,
        opacity: dim ? 0.12 : kind === 'order' ? 0.75 : 0.9,
        showLabel: hover || touches,
        label,
        lx: b.mx,
        ly: b.my - 6,
      }
    })

    /* ONE TAB STOP, not 1,481 (and not "however many happen to be mounted").
     * The card that holds it is pinned into the window below whatever the view,
     * so focus can never be unmounted out from under the reader. */
    const tabId = (nodes.find((n) => n.sel) || nodes[0] || {}).id
    nodes.forEach((n) => {
      n.tabbable = n.id === tabId
    })

    /* THE OVERVIEW'S RASTER, built from the UNWINDOWED list (this is the one
     * site where windowing would have been fatal: window the array upstream and
     * the only navigation aid for a graph this tall becomes a picture of the
     * viewport). Independent axis scales, a floor of one whole cell per card,
     * and WORST WINS inside a cell, so no fold, scale or overlap can quiet a
     * negative. See `viewport.js` for why a true-aspect overview drew nothing.
     *
     * `this.nodeById(n.id)` used to be an `Array.find` INSIDE this map —
     * ≈18 million comparisons per render on the target session. It is a `Map`
     * read now (`index()`). */
    let mm = null
    if (ui.minimapShow) {
      const bb = this.bbox(vnodes)
      const pad = 40
      const box = { x0: bb.x0 - pad, y0: bb.y0 - pad, x1: bb.x1 + pad, y1: bb.y1 + pad }
      const layout = minimapLayout(box, SIZE.minimapW, SIZE.minimapH)
      const boxes = vnodes.map((n) => {
        const g = this.geo(n)
        const fam = this.worstFam(n)
        const r = MM_RANK.indexOf(fam)
        return { x: g.x, y: g.y, w: g.w, h: g.h, rank: r < 0 ? MM_RANK.length : r }
      })
      const selNode = st.sel ? this.nodeById(st.sel) : null
      const selG = selNode ? this.geo(selNode) : null
      const note = minimapNote(layout)
      mm = {
        w: SIZE.minimapW,
        h: SIZE.minimapH,
        box,
        layout,
        cells: minimapCells(boxes, layout).map((c) => ({ ...c, fill: FAMof(MM_RANK[c.rank] || 'grey')[0] })),
        frames: frames.map((f) => ({ id: f.id, ...minimapMark({ x: f.x, y: f.y, w: f.w, h: f.h }, layout, 1) })),
        selBox: selG ? { x: selG.x, y: selG.y, w: selG.w, h: selG.h } : null,
        note,
        caption: minimapCaption(layout),
      }
    }

    this.memo.graph = { key, vnodes, frames, nodes, wires, mm }
    return this.memo.graph
  }

  /**
   * Build the whole view model once per render — the port of `renderVals()`.
   * `render()` destructures named slices and passes them down; the slice names
   * are the design's own so the template stays mechanically checkable against
   * spec-template §A.
   *
   * @returns {object} the flat view model
   */
  buildVals() {
    const st = this.state
    const S = this.S()
    /* WHICH OF THE FOUR SURFACES THE READER IS LOOKING AT, decided once, in a
     * pure function, and echoed back with the phase it was decided from
     * (`state.centreState`). A read in flight, a record that was read and holds
     * nothing, and a read that failed are three different truths; they used to
     * be one refusal that told the reader their data was missing while it was
     * being fetched. */
    const centre = centreState({ dataStatus: st.dataStatus, dataRefusal: st.dataRefusal, session: st.corpus.sessions[st.session] })
    /* A SESSION SWITCH IS A READ, and it says so in the same words. The header
     * button and Refresh go inert while one is in flight rather than showing a
     * session label that is about to be replaced. */
    const reading = st.switching ? API.explain + '?session=' + st.switching : centre.kind === 'reading' ? API.explain : null
    const absence =
      centre.kind === 'empty'
        ? { readings: st.corpus.order.length, anyHeld: anyReadingHolds(st.corpus) }
        : null
    const ctx = this.ctx()
    const sizes = this.sizes()
    const rowH = this.rowH()
    const narrow = st.narrow
    const panels = st.panels
    const com = this.props.commentary !== false
    const demoLabel = st.isDemo ? (/Local fixture/.test(S.origin || '') ? DEMO_LABEL.fixture : DEMO_LABEL.bundled) : null

    const ui = {
      leftShow: panels.left,
      leftW: sizes.left,
      leftPosition: narrow ? 'absolute' : 'relative',
      leftShadow: narrow ? `8px 0 24px ${ALPHA.tooltipShadow}` : 'none',
      leftSplit: !narrow,
      rightShow: panels.right,
      rightW: sizes.right,
      rightPosition: narrow ? 'absolute' : 'relative',
      rightShadow: narrow ? `-8px 0 24px ${ALPHA.tooltipShadow}` : 'none',
      rightSplit: !narrow,
      /* P6-1 — below `BP.sheet` the inspector is a bottom sheet over the
       * canvas, full width, rather than a column squeezed beside it. */
      rightSheet: narrow,
      bottomShow: panels.bottom,
      bottomH: sizes.bottom,
      minimapShow: panels.minimap && st.rep === 'graph',
    }

    /* rail */
    const active = st.destination === 'inbox' ? 'inbox' : st.drawer ? st.drawer : st.doc === 'compare' ? 'compare' : 'trace'

    /* left dock tree */
    const q = st.treeSearch.trim().toLowerCase()
    /* The field is `nodeId`, not `id`: `LeftDock` reads `it.nodeId` for both
     * the row key and `onSelect(it.nodeId)` (CONTRACT §2.4, its `props.groups`
     * typedef at `docks.jsx:386`). Emitting `id` here left every key
     * `undefined` and every tree row inert — the row painted its selected
     * state correctly, because `sel` is computed from the argument, while the
     * click selected nothing. Measured in the running app before the fix. */
    const item = (label, sub, nodeId, word, mono, title, clause) => ({
      nodeId,
      label,
      sub: sub || '',
      title: title || label,
      sel: st.sel === nodeId,
      wordSlug: word || null,
      clause: clause === undefined ? null : clause,
      font: mono ? FONT.mono : 'inherit',
      fontSize: mono ? FS.ctl : FS.body,
    })
    const groups = []
    const G = (key, title, items, noun) => {
      let list = items
      if (!list.length) return
      if (q) list = list.filter((i) => (i.label + ' ' + i.sub).toLowerCase().indexOf(q) >= 0)
      if (!list.length) return
      groups.push({ key, title, count: countPhrase(list.length, noun[0], noun[1]), open: !st.treeClosed[key], items: list })
    }
    /* DOC-04: the sub-line is the record's own KIND, never its `AskStatus` /
     * `PromiseStatus` identifier. Printing `a.status` put `untouched`, `open`,
     * `dropped`, `claimed_only`, `scope_shrunk`, `refused`, `violated` and
     * `held` on screen as first-level text — engine vocabulary describing our
     * pipeline, and one of them (`unverified`'s neighbours) is a verdict enum
     * used as a label. The status stays reachable one click behind, in the
     * inspector. */
    G(
      'asks',
      'Requests',
      S.asks.map((a) => item('“' + a.text + '”', String(a.kind || '').replace(/_/g, ' '), a.node, a.word, false, null, this.clauseOf(a))),
      ['request', 'requests'],
    )
    G('claims', 'Claims', S.claims.map((c) => item('“' + c.text + '”', String(c.kind || '').replace(/_/g, ' '), c.node, c.word, false, null, this.clauseOf(c))), ['claim', 'claims'])
    G('checks', 'Recorded checks', S.checks.map((c) => item(c.exit === 0 ? 'A check finished successfully' : c.exit != null ? 'A check did not pass' : 'A check result was captured', c.runner || 'Open original result', c.node, null, false, c.command || c.id)), ['check', 'checks'])
    G(
      'cmds',
      'Agent actions',
      S.events
        .filter((e) => e.kind === 'tool_call' || e.kind === 'exec')
        .map((e) => {
          const n = this.nodeBySeq(e.seq)
          /* The humanised line, the same one the card carries. The recorded
             text stays reachable in the inspector, under `As recorded`. */
          return n ? item(actionLabel(e), 'event ' + e.seq, n.id, null, false, e.readText || e.text) : null
        })
        .filter(Boolean),
      /* §4.2: `1391 command events` is ~45px wider than the dock has, which is
       * half of why it painted over its own heading. Still noun-bearing. */
      ['command', 'commands'],
    )
    G('files', 'Changed files', S.nodes.filter((n) => n.file && objType(n) === 'event').map((n) => item('Changed ' + humanPath(n.file), 'event ' + n.seq, n.id, null, false, n.file)), ['file', 'files'])
    G(
      'promises',
      'Promises',
      S.promises.map((p) => item('“' + p.text + '”', String(p.kind || '').replace(/_/g, ' '), p.node, p.word, false, null, this.clauseOf(p))),
      ['promise', 'promises'],
    )
    G('snaps', 'Code checkpoints', S.nodes.filter((n) => n.type === 'EVENT' && /^Snapshot/.test(n.sub || '')).map((n) => item('Code state recorded', 'event ' + n.seq, n.id, null, false, n.title)), [
      'snapshot',
      'snapshots',
    ])
    G('risk', 'Risk flags', S.nodes.filter((n) => n.risk).map((n) => item(n.risk || 'destructive command', 'seq ' + n.seq, n.id, null, true, n.title)), ['flag', 'flags'])
    G(
      'ending',
      'Ending',
      S.nodes
        .filter((n) => (n.seq != null && n.seq === S.stop.seq && n.type === 'EVENT') || n.id === 'ending')
        .map((n) =>
          item(
            n.id === 'ending' ? 'No ending recorded' : String(S.stop.kind || '').replace(/_/g, ' '),
            n.seq != null ? 'seq ' + n.seq : '',
            n.id,
            n.id === 'ending' ? 'fog' : null,
            false,
            null,
            this.clauseOf(S.stop),
          ),
        ),
      ['ending', 'endings'],
    )
    G('receipt', 'Receipt', S.nodes.filter((n) => objType(n) === 'receipt').map((n) => item(n.title, (S.verify || {}).integrity === 'verified' ? 'intact' : 'partly checked here', n.id, null, true)), [
      'receipt',
      'receipts',
    ])

    /* MEASURED AT 40 ms PER PAN FRAME. Each row calls `viewCount`, which filters
     * 4,279 nodes through `adapter.matches` → `attrs()`, and `attrs()` resolves
     * an event by a LINEAR SCAN of 4,279 events: ≈36.6 million comparisons per
     * render, to label six rows in a sidebar a pan does not touch. It depends
     * on the session and on the chips, and on nothing a pan changes. */
    /* TRACE-MISSING-SESSION-TRUE (verifier try 2): the read state is part of the
     * key. A count cached while the read was in flight must not survive the
     * refusal that arrives for the same (empty) session. */
    const svKey = centre.kind + '|' + S.key + '|' + JSON.stringify(st.savedViews) + '|' + JSON.stringify(st.filters)
    if (!this.memo.views || this.memo.views.key !== svKey) {
      this.memo.views = {
        key: svKey,
        list: this.builtinViews()
          .concat(st.savedViews)
          .map((v) => ({
            name: v.name,
            query: v.tokens.join(' '),
            count: this.viewCount(v),
            active:
              !!v.tokens.length && v.tokens.every((t) => st.filters.some((c) => c.text === t)) && st.filters.length === v.tokens.length,
          })),
      }
    }
    const savedViews = this.memo.views.list

    /* document tabs — the design's italic tagline at :440 does not ship (F-1) */
    const docTabs = [{ id: 'trace', label: 'Trace · ' + S.label, icon: '◧', iconColor: C.amber, font: 'inherit', closeable: false }].concat(
      st.docs.map((doc) => {
        if (doc.id === 'claim') {
          const cs = st.corpus.sessions[doc.session] || S
          return {
            id: 'claim',
            label: 'Claim ' + (cs.claims[0] ? cs.claims[0].id : 'not in this reading') + ' · pinned',
            icon: '○',
            iconColor: C.amber,
            font: FONT.mono,
            closeable: true,
          }
        }
        return {
          id: 'compare',
          label: 'Compare · ' + (st.compare.a || 'a') + ' ↔ ' + (st.compare.b || 'b'),
          icon: '⧉',
          iconColor: C.focus,
          font: FONT.mono,
          closeable: true,
        }
      }),
    )

    /* filters */
    /* Every object of the record, folded or not. See the Explorer's note on
     * why this is `!n.synthetic` and not `!n.group`. */
    const objects = S.nodes.filter((n) => !n.synthetic)
    const matched = objects.filter((n) => !ctx.hasFilters || this.matches(n)).length
    /* Doctrine 4 again: `0 objects in scope` rendered over a record that had
     * not been read yet, and over one that was read and holds none. A zero
     * count does not render; the pane's own sentence stands in its place. With
     * filters applied the zero DOES render, because `0 of 28 objects match` is
     * a statement about the VIEW, not a count of the record. */
    /* Two numbers, because grouping made one of them ambiguous: the graph now
     * draws fewer CARDS than the record holds OBJECTS, and a reader who counts
     * cards and reads "4243 objects in scope" is owed the reconciliation. The
     * denominator itself is unchanged — it counts objects, folded or not. */
    const drawnCards = this.visibleNodes().length
    const cardLine = drawnCards && drawnCards !== objects.length ? countPhrase(drawnCards, 'card', 'cards') + ' · ' : ''
    const matchLine = ctx.hasFilters
      ? matched + ' of ' + countPhrase(objects.length, 'object', 'objects') + ' match'
      : objects.length
        ? cardLine + countPhrase(objects.length, 'object', 'objects') + ' in scope'
        : ''
    /* THE FOCUS PILL (Step 5). A node Focus does not draw is not dropped: it
     * is a member of a group card on screen and is counted here as "merged".
     * That is `isDim`'s dim-not-hide invariant stated in words for the one
     * view that folds rather than dims — the denominator is the same
     * `objects` every other line uses. */
    const fz = st.focusMode === 'focus' ? this.G().focus : null
    const focusLine = fz && fz.total ? fz.shown + ' of ' + countPhrase(fz.total, 'object', 'objects') + ' shown · ' + countPhrase(fz.merged, 'merged', 'merged') : ''

    /* graph */
    /* The key names every input the model reads. `view` is absent on purpose —
     * see `graphModel`. */
    const gkey = [
      this.G().key,
      st.focusMode === 'focus' && st.focusChain ? st.focusChain.join(',') : '',
      st.traceDirection,
      Object.keys(st.expanded)
        .filter((k) => st.expanded[k])
        .sort()
        .join(','),
      JSON.stringify(st.pos),
      JSON.stringify(st.widen),
      st.sel || '',
      st.hoverWire || '',
      st.linked ? 'L' : '',
      st.findingsOnly ? 'F' : '',
      JSON.stringify(this.activeFilters()),
      st.findOpen ? st.findText : '',
      String(ctx.rseq),
      ui.minimapShow ? 'M' : '',
      st.register,
    ].join('|')
    const gm = this.graphModel(gkey, ctx, ui)
    const frames = gm.frames
    const v = st.view
    const gs = Math.max(8, 24 * v.z)

    /* THE WINDOW (M1 §E.3). Applied HERE, at the render boundary, and nowhere
     * else — never inside `visibleNodes()`, which the card count, the match
     * line, every frame count and the overview all read. A node that is not
     * mounted is still in scope, still counted, still reachable by find, by
     * replay, by a filter and by the overview; windowing is a drawing decision
     * and must never change a number the reader is shown.
     *
     * It is safe because the window is a STRICT SUPERSET of the viewport by at
     * least one viewport in every direction (`viewport.windowBox`), derived
     * from the same `view` object the transform string above is derived from,
     * in this same pass. A dropped card was off-screen anyway. The selected
     * card is pinned in regardless, so the one tab stop is never unmounted. */
    const win = windowBox(v, st.cw, st.ch)
    const drawNodes = gm.nodes.filter((n) => n.sel || n.tabbable || nodeInWindow(win, n))
    const drawWires = gm.wires.filter((w) => boxesIntersect(win, w.box))
    /* Said out loud, in the legend and in the canvas's own `aria-label`. The
     * bottom dock has stated its own ±40 event window in words since it was
     * written; this is that precedent, kept. */
    const windowNote =
      drawNodes.length < gm.nodes.length
        ? 'drawing ' + drawNodes.length + ' of ' + countPhrase(gm.nodes.length, 'card', 'cards') + ' — the rest are off-screen, and still counted'
        : ''
    const graph = {
      view: {
        transform: 'translate(' + v.x + 'px, ' + v.y + 'px) scale(' + v.z + ')',
        gridSize: gs + 'px ' + gs + 'px',
        gridPos: v.x + 'px ' + v.y + 'px',
      },
      frames,
      nodes: drawNodes,
      wires: drawWires,
      /* the UNWINDOWED lists, for everything that resolves an id rather than
         draws a box — `onWireClick` is the one that would break silently */
      allNodes: gm.nodes,
      allWires: gm.wires,
      emptyNote: S.eventsNote || null,
      windowNote,
      replayNote: ctx.rseq != null
        ? (this.face() === 'plain' ? 'replay at ' + fill(HQ_COPY.trace.face.step, { n: ctx.rseq }) : 'replay at seq ' + ctx.rseq) + ' — later records dimmed'
        : '',
      /* P6-3: the permanent legend — the three line kinds, a fourth only when
       * the record carries a kind this view cannot classify, and the thread's
       * own entry while one is lit. */
      legend: legendEntries(gm.wires.map((w) => ({ kind: w.recordKind }))),
      legendThread: this.thread() ? { label: HQ_COPY.trace.edges.thread, hint: HQ_COPY.trace.edges.thread_hint } : null,
      /* the lane labels, kept at the left edge of the view whatever the pan */
      lanes: st.traceDirection === 'lanes' && this.flow().lanes
        ? this.flow().lanes.map((l, i) => ({ id: l.id, label: l.label, count: frames[i] && frames[i].lane === l.id ? frames[i].count : '', top: v.y + l.y * v.z, height: l.h * v.z }))
        : null,
      threadBar: this.threadBar(),
    }
    const mm = gm.mm
      ? {
          ...gm.mm,
          /* the two marks that depend on the view, and the two that a true
             scale would render invisible: both go through `minimapMark`, which
             carries the floor */
          viewport: minimapMark({ x: -v.x / v.z, y: -v.y / v.z, w: st.cw / v.z, h: st.ch / v.z }, gm.mm.layout),
          sel: gm.mm.selBox ? minimapMark(gm.mm.selBox, gm.mm.layout, MM_MIN_VIEWPORT) : null,
        }
      : null

    /* timeline */
    /* NOT BUILT WHEN IT IS NOT MOUNTED. Exactly one of the three trace
     * representations renders at a time, and this one built 4,279 row objects
     * on every pan frame of the GRAPH. Switching representation re-renders, so
     * the rows are there the moment they are shown; nothing is cached and
     * nothing can go stale. */
    const showTimeline = st.doc === 'trace' && st.rep === 'timeline'
    const timeline = []
    if (showTimeline) {
      S.events.forEach((e) => {
        const n = this.nodeBySeq(e.seq)
        const sel = !!(n && st.sel === n.id)
        const dim = n ? this.isDim(n, ctx) : false
        const kind = KIND_LABEL[e.kind] || e.kind
        const claimHere = S.claims[0] && S.claims[0].seq === e.seq
        const askHere = S.asks[0] && S.events[1] && S.events[1].seq === e.seq && S.asks[0].eventId
        const promHere = S.promises[0] && S.promises[0].seq === e.seq
        const mono = e.kind === 'exec' || e.kind === 'tool_call' || e.kind === 'snapshot' || e.kind === 'note'
        timeline.push({
          id: n ? n.id : 'seq-' + e.seq,
          seq: 'seq ' + e.seq,
          mark: claimHere ? '○' : askHere || promHere ? '⁓' : e.risk ? '⚠' : '·',
          markColor: claimHere ? C.amber : askHere || promHere ? C.link : e.risk ? C.amber : C.text3,
          kind,
          kindColor: kindColorOf(e.kind),
          text: e.readText || e.text,
          font: mono ? FONT.mono : 'inherit',
          fontSize: mono ? FS.ctl : FS.body,
          ref: e.risk ? e.risk : claimHere ? HQ_COPY.trace.marks.claim_here + ' · ' + short(S.claims[0].eventId) : promHere ? HQ_COPY.trace.marks.promise_here : askHere ? HQ_COPY.trace.marks.ask_here + ' · ' + short(S.asks[0].eventId) : '',
          sel,
          bg: sel ? C.accent : 'transparent',
          edge: sel ? C.amber : 'transparent',
          /* LEG-DIM: a dimmed row desaturates (text2 ink, neutral mark) and
           * stays readable; it is never drawn at a lower opacity. */
          dim: !!dim,
        })
      })
      if (S.claims[0] && S.claims[0].seq != null) {
        const c = S.claims[0]
        const cw = wordOf(c.word)
        timeline.push({
          id: c.node,
          seq: 'derived',
          mark: cw.glyph,
          markColor: cw.color,
          kind: 'Claim',
          kindColor: cw.ink,
          text: '“' + c.text + '” — ' + cw.word,
          font: 'inherit',
          fontSize: FS.body,
          ref: c.id,
          wordSlug: c.word,
          clause: this.clauseOf(c),
          sel: st.sel === c.node,
          bg: st.sel === c.node ? C.accent : 'transparent',
          edge: st.sel === c.node ? C.amber : 'transparent',
          dim: false,
        })
      }
    }
    const timelineCaption = S.events.length
      ? 'You are looking at the recorded events around ' +
        (S.claims[0] ? '“' + S.claims[0].text + '”' : S.asks[0] ? '“' + S.asks[0].text + '”' : 'this session') +
        (S.started ? ', ' + S.started.slice(0, 10) : '') +
        '. White = recorded order; marks show where derived objects were placed.'
      : ''
    /* §1.6.4: the reading says ONCE that values reached it already shortened.
       Per-card noise on 1,391 cards would be worse than the defect; Rule 12
       requires it be loud once, not 1,391 times. */
    const timelineNote =
      [S.eventsNote || (S.stop.seq == null ? 'No ending recorded — the line dissolves after the last event.' : null), S.readingNote].filter(Boolean).join(' · ') || null

    /* table */
    const cols = st.columns
    const head = []
    const tmpl = []
    const col = (k, label, width) => {
      if (!cols[k]) return
      head.push({ key: k, label })
      tmpl.push(width)
    }
    col('seq', 'seq', '64px')
    col('type', 'type', '130px')
    col('state', 'state', '110px')
    col('text', 'text', 'minmax(0,1fr)')
    col('ref', 'reference', '220px')
    col('file', 'file / tree', '150px')
    /* NOT BUILT WHEN IT IS NOT MOUNTED, for the same reason as the timeline:
     * this filtered AND SORTED 4,279 objects on every pan frame of the graph. */
    const showTable = st.doc === 'trace' && st.rep === 'table'
    const trows = !showTable
      ? []
      : S.nodes
          .filter((n) => !n.synthetic && (!ctx.hasFilters || this.matches(n)) && (!st.findingsOnly || ctx.fctx[n.id]))
          .sort((a, b) => (this.placedSeq(a) == null ? 9999 : this.placedSeq(a)) - (this.placedSeq(b) == null ? 9999 : this.placedSeq(b)))
    const gmap = {}
    const gorder = []
    trows.forEach((n) => {
      const key = st.groupBy === 'frame' ? n.frame || 'other' : st.groupBy === 'type' ? n.type : 'all'
      if (!gmap[key]) {
        gmap[key] = []
        gorder.push(key)
      }
      gmap[key].push(n)
    })
    const frameTitle = (k) => {
      const f = S.frames.find((x) => x.id === k)
      return f ? f.title : k
    }
    const table = {
      cols: tmpl.join(' '),
      head,
      groups: gorder.map((k) => ({
        title: st.groupBy === 'none' ? null : st.groupBy === 'frame' ? frameTitle(k) : k,
        count: gmap[k].length ? countPhrase(gmap[k].length, 'object', 'objects') : null,
        rows: gmap[k].map((n) => {
          const w = n.word ? wordOf(n.word) : null
          const f = n.finding ? this.findingByKey(n.finding) : null
          const sev = f ? severityOf(f.severity) : null
          const sel = st.sel === n.id
          const cells = []
          if (cols.seq)
            cells.push({
              text: n.seq != null ? String(n.seq) : this.placedSeq(n) != null ? '(' + this.placedSeq(n) + ')' : '',
              color: C.inkMute,
              font: FONT.mono,
              fontSize: FS.sm,
            })
          if (cols.type) cells.push({ text: n.type + ' · ' + n.sub, color: C.inkSoft, font: 'inherit', fontSize: FS.ctl })
          if (cols.state)
            cells.push({
              text: w && w.word ? w.word : sev ? sev.label : '',
              glyph: w && w.word ? w.glyph : null,
              color: w && w.word ? w.ink : sev ? sev.color : C.inkFaint,
              font: 'inherit',
              fontSize: FS.ctl,
              wordSlug: f ? null : n.word || null,
              clause: f ? null : this.clauseOf(n),
              severity: f ? f.severity : null,
            })
          if (cols.text) cells.push({ text: n.title, color: C.inkBright, font: n.mono ? FONT.mono : 'inherit', fontSize: n.mono ? FS.ctl : FS.body })
          if (cols.ref)
            cells.push({ text: n.objId && n.objId !== 'receipt' ? n.objId : n.foot, color: C.inkMute, font: FONT.mono, fontSize: FS.cap })
          if (cols.file)
            cells.push({ text: n.file || (/tree/.test(n.title) ? short(S.finalTree) : ''), color: C.ink, font: FONT.mono, fontSize: FS.sm })
          return { id: n.id, cells, sel, bg: sel ? C.accent : 'transparent', edge: sel ? C.amber : 'transparent' }
        }),
      })),
      /* "same scope, selection and filters as the graph" became false the day
       * grouping shipped: the graph draws cards and this draws objects, and a
       * folded member is an object the graph is not currently drawing. */
      foot: trows.length + ' of ' + countPhrase(objects.length, 'object', 'objects') + ' · the same objects the graph holds, folded or not',
    }

    /* inspector */
    const insp = st.doc === 'claim' ? this.claimDocInspector() : this.inspector()
    const cmds = this.commands()
    const copyNextLabel = S.claims[0] && S.claims[0].nextCheck ? 'Copy next check command' : 'No next check owed'

    /* bottom dock */
    const fs = this.findings()
    const findingRows = fs.map((f) => ({
      key: f.key,
      node: f.node,
      nodeId: f.node,
      name: f.name,
      severity: f.severity,
      basis: f.basis,
      detail: f.detail || f.derivedDetail,
      detailStyle: f.detail ? 'normal' : 'italic',
      ref: f.ref,
      sel: st.selFinding === f.key,
    }))
    const evAll = S.events
    let evRows = evAll
    let eventsNote =
      S.eventsNote ||
      (evAll.length
        ? countPhrase(evAll.length, 'recorded event', 'recorded events') + ' · seq numbers are ledger order, not line numbers' + (S.stop.seq == null ? ' · no ending recorded' : '')
        : null)
    if (evAll.length > 120) {
      const focus = st.eventsFocusSeq != null ? st.eventsFocusSeq : S.claims[0] ? S.claims[0].seq : evAll[0].seq
      const ci = Math.max(0, evAll.findIndex((e) => e.seq === focus))
      evRows = evAll.slice(Math.max(0, ci - 40), ci + 41)
      eventsNote = 'Showing ' + evRows.length + ' of ' + countPhrase(evAll.length, 'event', 'events') + ', windowed ±40 around seq ' + evAll[ci].seq
    }
    if (S.readingNote) eventsNote = [eventsNote, S.readingNote].filter(Boolean).join(' · ')
    const focusIdx = st.eventsFocusSeq == null ? -1 : evAll.findIndex((x) => x.seq === st.eventsFocusSeq)
    const eventRows = evRows.map((e) => {
      const n = this.nodeBySeq(e.seq)
      const sel = !!(n && st.sel === n.id)
      const isFocus = focusIdx >= 0 && Math.abs(evAll.indexOf(e) - focusIdx) <= 2
      const dim = ctx.rseq != null && e.seq > ctx.rseq
      const mono = e.kind === 'exec' || e.kind === 'tool_call' || e.kind === 'snapshot' || e.kind === 'note'
      return {
        id: n ? n.id : 'seq-' + e.seq,
        nodeId: n?.id,
        seq: String(e.seq),
        kind: KIND_LABEL[e.kind] || e.kind,
        kindColor: kindColorOf(e.kind),
        text: e.readText || e.text,
        font: mono ? FONT.mono : 'inherit',
        fontSize: mono ? FS.ctl : FS.body,
        mark: ctx.rseq === e.seq ? '▶' : e.risk ? '⚠' : isFocus ? '•' : '',
        /* N13-GLYPHS-ARIA: ⚠ has its words in `ref`; ▶ and • have none beside them. */
        markLabel: ctx.rseq === e.seq ? HQ_COPY.trace.marks.replay_here : e.risk ? null : isFocus ? HQ_COPY.trace.marks.near_focus : null,
        markColor: ctx.rseq === e.seq ? C.link : e.risk ? C.amber : C.focus,
        ref: e.risk ? e.risk : /tree/.test(e.text) ? short(S.finalTree) : '',
        refColor: e.risk ? C.amber : C.inkDim,
        sel,
        bg: sel ? C.accent : isFocus && !dim ? C.surf2Hi : 'transparent',
        edge: sel ? C.amber : 'transparent',
        /* LEG-DIM: past the replay cursor the row desaturates; its text stays text2. */
        dim: !!dim,
      }
    })
    const checkRows = S.checks.map((c) => ({
      id: c.node,
      nodeId: c.node,
      runner: c.runner,
      command: c.command,
      scope: c.scopeNote,
      counts: c.parsed ? c.parsed.passed + ' / ' + c.parsed.failed + ' / ' + c.parsed.skipped + '  of ' + c.parsed.tests_total : '',
      binding: c.bindingLabel + ' · ' + short(c.inputTree),
      bindColor: c.bound ? C.green : C.amber,
      ref: c.id,
      exit: String(c.exit),
      sel: st.sel === c.node,
      bg: st.sel === c.node ? C.accent : 'transparent',
      edge: st.sel === c.node ? C.amber : 'transparent',
    }))
    const promiseRows = S.promises.map((p) => ({
      id: p.node,
      nodeId: p.node,
      wordSlug: p.word,
      clause: this.clauseOf(p),
      text: '“' + p.text + '”',
      followUp: p.followUp ? 'Follow-up evidence: none — ' + p.followUp + '.' : 'Follow-up evidence: none recorded.',
      ref: p.id + (p.eventId ? ' · opened at ' + short(p.eventId) : '') + (p.seq != null ? ' · seq ' + p.seq : ''),
      sel: st.sel === p.node,
      bg: st.sel === p.node ? C.accent : 'transparent',
      edge: st.sel === p.node ? C.link : 'transparent',
    }))
    const V = S.verify || { rows: [] }
    const okRows = (V.rows || []).filter((r) => r.outcome === 'pass').length
    const receiptRows = (V.rows || []).map((r, i) => {
      const pass = r.outcome === 'pass'
      return {
        id: r.name || 'row-' + i,
        glyph: pass ? '✓' : '◌',
        color: pass ? C.green : C.amber,
        name: r.name,
        detail: (pass ? 'confirmed here' : 'could not be made here — not a pass') + (r.detail || r.reason ? ' — ' + (r.detail || r.reason) : ''),
        detailColor: pass ? C.inkSoft : C.amber,
      }
    })
    /* F10/F32: `<Count>` renders nothing for a null or zero count, so a tab
     * whose count was unknown and a tab whose count was zero both rendered as
     * a bare label — and "not measured" and "none" are not the same fact. Each
     * tab therefore carries a `note`, which `BottomDock` renders when there is
     * no counted phrase to render. */
    const dockTabs = [
      { key: 'findings', label: 'Findings', count: fs.length, singular: 'finding', plural: 'findings', note: fs.length ? null : 'none' },
      {
        key: 'events',
        label: 'Events',
        count: S.eventCount != null ? S.eventCount : null,
        singular: 'event',
        plural: 'events',
        note: S.eventCount == null ? 'not in this reading' : S.eventCount === 0 ? 'none' : null,
      },
      { key: 'checks', label: 'Checks', count: S.checks.length, singular: 'check', plural: 'checks', note: S.checks.length ? null : 'none' },
      {
        key: 'promises',
        label: 'Promises',
        count: S.promises.length,
        singular: 'open promise',
        plural: 'open promises',
        note: S.promises.length ? null : 'none open',
      },
      { key: 'output', label: 'Output', count: st.output.length, singular: 'line', plural: 'lines', note: st.output.length ? null : 'none' },
      /* F26: the design's tab reported `okRows of N confirmed`. Reporting the
       * bare row count reads as "N checks" whether none or all of them were
       * confirmed, which is the one thing this tab exists to distinguish. */
      {
        key: 'receipt',
        label: 'Receipt checks',
        count: null,
        note: (V.rows || []).length ? okRows + ' of ' + countPhrase(V.rows.length, 'check', 'checks') + ' confirmed' : 'none in this reading',
      },
    ]
      /* TRACE-MISSING-SESSION-TRUE (D-332): over a refused read every tab is unread, never "none". */
      .map((t) => (centre.kind === 'refused' ? { ...t, count: null, note: HQ_COPY.trace.unread.tab } : t))
      .map((t) => ({ ...t, active: st.dockTab === t.key }))
    /* The dock caption is authored-and-labelled, or it does not render (F-16). */
    const captionText = com ? S.commentary[st.dockTab === 'findings' ? 'findings' : st.dockTab === 'checks' ? 'chk' : st.dockTab === 'promises' ? 'prm' : ''] : ''
    const dockCaption = captionText ? { kicker: 'authored', text: captionText } : null

    /* menus */
    const menu = { kind: st.menu, title: '', x: st.menuX, y: st.menuY, width: 240, items: [], input: false, inputNote: '' }
    const mi = (label, onClick, checked, hint, font) => ({ label, onClick, checked: !!checked, check: checked ? '✓' : '', hint: hint || '', font: font || 'inherit' })
    if (st.menu === 'project') {
      menu.title = 'Project'
      menu.width = 480
      const rows = st.isDemo ? [] : st.sessions.rows
      const projects = projectListing(rows, st.corpus, st.projectSearch)
      menu.search = {
        value: st.projectSearch, label: 'Search projects', placeholder: 'Search by project name or path…',
        onChange: value => this.setState({ projectSearch: value }),
        note: st.sessions.status === 'refused' ? 'The project list could not be read. Previously opened projects remain available.'
          : st.sessions.perProject ? `Recorded projects · up to ${st.sessions.limit} sessions per project. Folders appear after their activity is captured or imported.`
          : 'Projects in the available session list. Folders appear after their activity is captured or imported.',
      }
      menu.items = projects.map(project => mi(project.repo, () => {
        if (!project.session) { this.toast(UNOPENABLE_ID); return }
        this.setState({ sessionFilter: '' }, () => this.switchSession(S.project === project.repo ? st.session : project.session))
      }, S.project === project.repo, countPhrase(project.count, 'listed session', 'listed sessions')))
      if (!projects.length) menu.items.push({ header: st.projectSearch ? 'No recorded project matches this search.' : 'No project is available in this reading.' })
      menu.items.push(mi('Browse recorded sessions', () => this.setState({ menu: null, drawer: 'sessions', sessionFilter: '' })))
    } else if (st.menu === 'session') {
      /* THE PICKER. It used to list `st.corpus.order`, which on the live path
       * held exactly one session — the one already open, with a checkmark
       * against it — so a control whose accessible name promises a SWITCHER
       * could not switch to anything. It now lists what the store returned.
       *
       * The popup is BOUNDED and says so: the generic `Menu` is one 70vh popup
       * serving eight kinds with no filter field of its own, and a store holds
       * far more sessions than belong in it. The bounded head lives here and
       * the whole list, filterable, lives in the Sessions drawer — which is why
       * the last row opens it rather than paging in place. */
      menu.title = 'Session'
      menu.width = 380
      /* THE BUNDLED CORPUS IS NOT ON THE STORE'S LIST. When it is what
       * rendered, the picker lists it — unchanged, exactly as it always did —
       * because a picker that listed the store while the demo corpus was on
       * screen would be offering rows that have nothing to do with what the
       * reader is looking at. */
      const listed = st.isDemo ? [] : this.listingRows(SESSION_MENU_ROWS, S.project)
      const all = st.isDemo ? [] : this.listingRows(undefined, S.project)
      menu.items = st.isDemo
        ? st.corpus.order.map((k) => {
            const s = st.corpus.sessions[k]
            const w = wordOf(s.sessionWord)
            return mi(
              s.label + '  ·  ' + (w.word ? w.word + '  ·  ' : '') + (s.countLine || ''),
              () => this.switchSession(k),
              k === st.session,
              w.negative ? this.sessionClauseOf(s) || NO_CLAUSE : 'demo',
              FONT.mono,
            )
          })
        : listed.map((r) => {
            const s = st.corpus.sessions[r.id]
            const w = wordOf(s ? s.sessionWord : null)
            /* DOC-06/DOC-07: a row that carries a state word carries its clause
             * when the word is negative — the row switches sessions rather than
             * opening the item, so the "one tap behind" exemption does not
             * reach it. A row for a session this reading has NOT adopted
             * carries no word at all, because none has been derived for it. */
            return mi(
              r.id + (w.word ? '  ·  ' + w.word : '') + '  ·  ' + this.listingLine(r),
              () => (r.openable ? this.switchSession(r.id) : this.toast(UNOPENABLE_ID)),
              r.id === st.session,
              w.negative ? this.sessionClauseOf(s) || NO_CLAUSE : s ? 'read here' : 'not read yet',
              FONT.mono,
            )
          })
      if (!st.isDemo && !listed.length) {
        /* THE FOUR FACTS, in a popup. In flight, refused, narrowed to nothing
         * and genuinely empty are four different sentences, and a picker that
         * showed one blank list for all four would be the same conflation the
         * centre pane was fixed for. */
        menu.items = [
          mi(
            st.sessions.status === 'refused'
              ? 'The session list could not be read — the Sessions panel says why.'
              : st.sessions.status === 'loading' || st.sessions.status === 'idle'
                ? READING_LINE
                : st.sessionFilter
                  ? 'No session on the list matches what you typed.'
                  : 'The session list came back empty.',
            () => this.railGo('sessions'),
          ),
        ]
      }
      if (!st.isDemo) {
        menu.items = menu.items.concat([
          { header: 'The whole list' },
          mi(
            'Open the Sessions list',
            () => this.railGo('sessions'),
            false,
            all.length > listed.length ? countPhrase(all.length - listed.length, 'more session', 'more sessions') : 'search and filter',
          ),
        ])
      }
    } else if (st.menu === 'more') {
      menu.title = 'More tools'
      menu.width = 260
      menu.items = [
        ...(this.unread() ? [] : [mi('Jump to stop', () => this.jumpStop())]),
        mi('Show test changes', () => this.setState({ drawer: 'testchanges' })),
        mi('Compare sessions', () => this.openDoc('compare')),
        mi('Reset layout', () => this.resetLayout()),
        ...(st.doc === 'trace'
          ? [
              { header: 'Replay the record' },
              mi('Step back', () => {
                this.stopReplay()
                this.step(-1)
              }),
              mi(st.replay.playing ? 'Pause replay' : 'Replay record', () => this.toggleReplay(), st.replay.playing),
              mi('Step forward', () => {
                this.stopReplay()
                this.step(1)
              }),
            ]
          : []),
      ]
    } else if (st.menu === 'panels') {
      menu.title = 'Panels'
      menu.width = 260
      menu.items = [
        mi('Left dock — session objects', () => this.setPanel('left'), panels.left, sizes.left + ' px'),
        mi('Right inspector', () => this.setPanel('right'), panels.right, sizes.right + ' px'),
        mi('Bottom dock', () => this.setPanel('bottom'), panels.bottom, sizes.bottom + ' px'),
        mi('Minimap', () => this.setPanel('minimap'), panels.minimap),
        { header: 'Sizes (keyboard alternative to dragging)' },
        mi('Inspector · narrow 300', () => this.setState({ sizes: { ...sizes, right: 300 } }), sizes.right === 300),
        mi('Inspector · default 340', () => this.setState({ sizes: { ...sizes, right: 340 } }), sizes.right === 340),
        mi('Inspector · wide 420', () => this.setState({ sizes: { ...sizes, right: 420 } }), sizes.right === 420),
        mi(
          'Bottom dock · 160 / 200 / 300',
          () => this.setState({ sizes: { ...sizes, bottom: sizes.bottom < 200 ? 200 : sizes.bottom < 300 ? 300 : 160 } }),
          false,
          sizes.bottom + ' px',
        ),
        { header: 'Composition' },
        mi('Focus canvas', () => this.focusCanvas(), !!st.focusPrev),
        mi('Reset layout', () => this.resetLayout()),
      ]
    } else if (st.menu === 'filters') {
      menu.title = 'Facets — values present in the loaded record'
      menu.width = 320
      const opts = this.facetOpts()
      FACETS.forEach((f) => {
        if (!opts[f]) return
        menu.items.push({ header: f })
        Object.keys(opts[f])
          .sort()
          .forEach((val) => {
            const tok = f + ':' + val
            menu.items.push(
              mi(tok, () => this.addFilter(tok), st.filters.some((c) => c.text === tok), countPhrase(opts[f][val], 'object', 'objects'), FONT.mono),
            )
          })
      })
    } else if (st.menu === 'group') {
      menu.title = 'Group by (table & timeline; the graph groups by phase)'
      menu.items = ['frame', 'type', 'none'].map((g) =>
        mi(g === 'frame' ? 'Phase frame' : g === 'type' ? 'Object type' : 'No grouping', () => this.setState({ groupBy: g, menu: null }), st.groupBy === g),
      )
    } else if (st.menu === 'columns') {
      menu.title = 'Columns (table)'
      menu.items = Object.keys(cols).map((c) => mi(c, () => this.setState({ columns: { ...cols, [c]: !cols[c] } }), cols[c]))
    } else if (st.menu === 'save') {
      menu.title = 'Save the current lens as a view'
      menu.width = 300
      menu.input = true
      menu.inputNote = st.filters.length
        ? 'Saves: ' + st.filters.map((c) => c.text).join(' ') + ' — stored on this machine only.'
        : 'Add at least one filter chip first.'
      menu.items = st.savedViews.map((view) =>
        mi(view.name, () => this.setState({ savedViews: st.savedViews.filter((x) => x.name !== view.name) }), false, 'remove'),
      )
    } else if (st.menu === 'focusTools') {
      /* Step 5: in Focus the four lens controls sit behind one button. Each
       * item opens the EXISTING menu, positioned under the item that opened it. */
      menu.title = 'Filters · Group by · Columns'
      menu.width = 260
      menu.items = [
        mi('Filters — add a facet…', (e) => this.openMenu('filters', e), st.filters.length > 0, st.filters.length ? countPhrase(st.filters.length, 'chip', 'chips') : ''),
        mi('Group by · ' + (st.groupBy === 'frame' ? 'phase' : st.groupBy) + '…', (e) => this.openMenu('group', e)),
        mi('Columns…', (e) => this.openMenu('columns', e)),
        mi('Save view…', (e) => this.openMenu('save', e)),
      ]
    } else if (st.menu === 'settings') {
      menu.title = 'Settings'
      menu.width = 300
      menu.items = [
        { header: 'Register — swaps sentences, never levels' },
        /* P6-1: these set THE register the top bar's Plain / Technical shows. */
        mi('Manager register (default)', () => this.setState({ register: 'manager', menu: null }), this.reg() === 'mgr'),
        mi('Operator register', () => this.setState({ register: 'operator', menu: null }), this.reg() === 'op'),
        { header: 'Voice' },
        mi(
          'Authored commentary',
          () => this.toast('Authored commentary is set where this workspace is mounted.'),
          com,
          'off by default · never inside quoted record text',
        ),
        { header: 'Motion & theme' },
        mi('Reduced motion follows the OS setting', () => {}, false),
        mi('Charcoal theme (fixed)', () => {}, true),
      ]
    }

    /* palette — headers interleaved, the active index counts ITEMS ONLY */
    const palRaw = st.palette ? this.palRaw() : []
    const palItems = []
    let lastKind = null
    let idx = 0
    palRaw.forEach((it) => {
      if (it.kind !== lastKind) {
        palItems.push({ header: it.kind })
        lastKind = it.kind
      }
      const i = idx++
      palItems.push({
        kind: it.kind,
        label: it.label,
        hint: it.hint,
        font: it.font,
        active: i === st.palIdx,
        onClick: () => {
          this.closePalette()
          it.act()
        },
        onEnter: () => this.setState({ palIdx: i }),
      })
    })

    /* find */
    const findAll = st.findOpen ? this.findMatches() : []
    const findResults = findAll.slice(0, 40).map((n, i) => ({
      id: n.id,
      kind: objType(n),
      text: n.title,
      /* A hit inside a fold names the card that currently stands for it, so
       * "centre on it" never lands somewhere the reader cannot see. */
      where: (n.inBand ? 'in ' + n.inBand + ' · ' : '') + (n.objId || (n.seq != null ? 'seq ' + n.seq : n.id)),
      active: i === st.findIdx,
      /* `chrome.jsx` renders every row as `<button role="option" onClick={r.onClick}>`
       * with a hover style, and this map never supplied one — so a mouse click
       * on a find hit did nothing at all and only ArrowDown+Enter could reach
       * it. Same body as the Enter branch of `onFindKey`, so both doors land on
       * the same node and write the same log line. */
      onClick: () => {
        this.select(n.id, { center: true, doc: true })
        this.setState({ findOpen: false, findIdx: i })
        this.log('Find in trace · “' + st.findText + '” · ' + countPhrase(this.findMatches().length, 'match', 'matches') + ' · focused ' + n.id)
      },
    }))

    /* status strip — the freshness slot has a ruled string (F-6, BRIEF:1510).
     * It is the one surface that renders in EVERY state, so it is where a
     * reader learns which state they are in: `statusFor` overrides the slots a
     * non-ready state cannot honestly fill, and `scope` goes BLANK rather than
     * printing `0 of 0 events` over a record nobody has read yet. */
    const stamp = readStamp(st.lastReadAt)
    const read = st.live ? 'Captured sessions · local reading' : stamp ? 'Ledger read ' + stamp.text : 'Ledger ' + HQ_COPY.shell.topbar.read_unknown
    const statusOverride = statusFor(centre, API.explain)
    const status = {
      origin: S.origin || (st.isDemo ? 'bundled demo records' : 'origin not in this reading'),
      scope:
        (S.eventCount != null ? S.events.length + ' of ' + countPhrase(S.eventCount, 'event', 'events') + ' in view' : 'events not enumerated in this reading') +
        ' · ' +
        countPhrase(objects.length, 'object', 'objects') +
        /* F23: the filter state is ALWAYS named. Saying nothing about
         * filtering when nothing is filtered leaves the reader unable to tell
         * a full scope from a narrowed one they have forgotten about. */
        ' · ' +
        (ctx.hasFilters ? matched + ' match' : 'no filters'),
      read: read + (st.reading.status === 'refreshing' ? ' · refreshing…' : ''),
      /* F23: the design's `local: fixture mode — no server supplied · UTC`
       * asserted fixture mode unconditionally. This reading knows whether a
       * local server answered it, so it says which, and the egress guarantee
       * is stated either way. `connLine` keeps it THREE-state — the boolean
       * form said "no local server in this reading" for the whole duration of
       * every read, which is a negative fact stated while it was being read. */
      conn: this.connLine(true) + ' · UTC',
    }
    if (statusOverride) {
      if (statusOverride.origin != null) status.origin = statusOverride.origin
      if (statusOverride.read != null) status.read = statusOverride.read
      status.scope = statusOverride.scope
    }
    if (S.localVerification) status.read = 'Project verification · local reading'

    return {
      centre,
      reading,
      /* what else is open, so an emptiness can say whether it is this session's
         alone or the whole reading's — never a count, always a sentence */
      absenceCtx: absence,
      /* chrome */
      demoLabel,
      isDemo: st.isDemo,
      projectLabel: S.project || (this.props.index && this.props.index.projects && this.props.index.projects[0] ? this.props.index.projects[0].name : 'no project open'),
      sessionLabel: S.label || 'no session open',
      sessionWord: S.sessionWord || null,
      /* DOC-08: the left dock's session line is NOT a control, so the
       * clause-free exemption does not reach it. It gets the record's own
       * clause through this prop instead of hiding one it never had. */
      sessionClause: this.sessionClauseOf(S),
      refreshStatus: st.reading.status,
      active,
      railTip: st.railHover,
      ui,
      rowH,
      /* left dock */
      /* Doctrine 4: a zero count does not render. `0 objects` painted here on
         every empty and every in-flight read — a naked number, and a claim
         about a record that had not been read yet. The emptiness is stated
         once, by the pane, as a sentence. */
      treeCountLine: objects.length ? S.countLine || countPhrase(objects.length, 'object', 'objects') : S.countLine || '',
      tree: groups,
      savedViews,
      /* centre */
      docTabs,
      chips: st.filters,
      matchLine,
      focusLine,
      groupByLabel: st.groupBy === 'frame' ? 'phase' : st.groupBy,
      sentences: (S.narr && S.narr.session && S.narr.session[this.reg()]) || [],
      riskNotice: this.riskNotice(),
      /* The identity line states three facts about a record. While one is
         being read it states none: `session id not exported · events not
         enumerated · no final tree` is three absences asserted about a
         document that has not arrived. */
      identLine: centre.kind === 'reading' ? '' :
        'session ' +
        (S.id || 'id not exported') +
        ' · ' +
        (S.eventCount != null ? countPhrase(S.eventCount, 'event', 'events') : 'events not enumerated') +
        ' · ' +
        (S.finalTree ? 'tree ' + short(S.finalTree) : 'no final tree') +
        (S.harness ? ' · ' + S.harness : ''),
      graph,
      mm,
      timeline,
      timelineCaption,
      timelineNote,
      table,
      /* inspector + docks */
      insp,
      cmds,
      copyNextLabel,
      dockTabs,
      dockCaption,
      panes: {
        findings: findingRows,
        events: eventRows,
        checks: checkRows,
        promises: promiseRows,
        output: st.output.slice().reverse(),
        receipt: receiptRows,
        /* TRACE-MISSING-SESSION-TRUE (D-332): a failed read states nothing about the session. */
        findingsNote: centre.kind === 'refused'
          ? HQ_COPY.trace.unread.pane
          : fs.length
          ? fs.some((f) => f.severity === 'info')
            ? 'Coverage not measured is informational — it is not a failed check and does not move the claim.'
            : null
          : 'No findings — ' + (S.claims.length ? 'the record raised none against this claim.' : 'no completion claim was made, so there is nothing to find against.'),
        eventsNote,
        checksNote: centre.kind === 'refused'
          ? HQ_COPY.trace.unread.pane
          : S.checks.length
          ? 'A check is evidence for a claim at its stated scope. Passing one declared check never proves the whole project.'
          : 'No checks are recorded in this session.',
        promisesNote: centre.kind === 'refused'
          ? HQ_COPY.trace.unread.pane
          : S.promises.length
          ? 'A promise nothing later touched stays open. Fulfilment is a cross-session filename heuristic that errs toward “still open”.'
          : 'No promises are open in this session.',
        receiptHead: (V.rows || []).length
          ? countPhrase(V.rows.length, 'named check returned by the verifier', 'named checks returned by the verifier') +
            ' · ' +
            okRows +
            ' confirmed here' +
            (okRows < V.rows.length ? ' · ' + (V.rows.length - okRows) + ' could not be made here — not a pass' : '') +
            ' · conclusion ' +
            V.conclusion +
            ' · exit ' +
            V.exit
          : /* F25: the design's string was `No receipt checks in this reading`.
             * Every other empty case in this object is a full sentence with a
             * verb and a stop (F-6), and a pane head that reads as a fragment
             * beside five that do not is a copy inconsistency, not fidelity. */
            'No receipt checks are in this reading.',
      },
      /* overlays */
      menu,
      palette: {
        open: st.palette,
        query: st.palQ,
        items: palItems,
        empty: st.palette && !palItems.length ? 'Nothing matches “' + st.palQ + '” in the loaded record. Filters use facet:value.' : '',
      },
      findResults,
      findCountLine: countPhrase(findAll.length, 'match', 'matches') + ' in loaded object text, commands, paths and ids',
      zoomPct: Math.round(v.z * 100) + '%',
      replayLabel: st.replay.playing ? (ctx.rseq != null ? 'seq ' + ctx.rseq : 'Pause replay') : ctx.rseq != null ? 'seq ' + ctx.rseq : 'Replay record',
      replayGlyph: st.replay.playing ? '❚❚' : '▶',
      compare: this.compareData(),
      drawer: this.drawerData(),
      status,
      sizes,
    }
  }

  /* ── Render (spec-template §A's region tree) ─────────────────────────── */

  /**
   * One render, one view model, named slices threaded down as props.
   *
   * Every destination stays in the evidence workspace. The legacy inbox
   * route opens the captured-session picker instead of a second dashboard.
   *
   * @returns {JSX.Element}
   */
  render() {
    const st = this.state
    const v = this.buildVals()
    /* the stable graph handlers read this instead of closing over the render
       that made them (`graphOn`) */
    this.vals = v
    const ui = v.ui
    const inbox = st.destination === 'inbox'
    /* P6-3: the lanes carry sections too (twelve cards each, in the order the
     * lanes read), so Workbench's section controls work over them as before. */
    const chapters = st.traceDirection === 'horizontal' ? this.flow().frames : st.traceDirection === 'lanes' ? this.flow().chapters || [] : []
    const viewLeft = -st.view.x / st.view.z
    const chapterIndex = st.traceDirection === 'lanes'
      ? chapters.reduce((k, f, i) => (f.x <= viewLeft + 1 ? i : k), 0)
      : Math.max(0, chapters.findIndex(f => f.x + f.w > viewLeft))
    const showChapter = f => { if (f) this.setState({ view: { ...st.view, x: 16 - f.x * st.view.z, y: 16 } }) }
    const inspectSource = (id) => {
      this.setState({ doc: 'trace', rep: 'graph', drawer: null, panels: { ...st.panels, right: true } }, () => {
        this.measure()
        this.select(id, { center: true, doc: true })
      })
    }
    /* THE FOUR FACTS, KEPT FOUR. This used to be:
     *
     *   const ready = st.dataStatus === 'ready'
     *   const refusal = st.dataRefusal || (ready ? null : { title: 'This
     *     reading is not open yet.', missing: 'no session projection has been
     *     read', remedy: 'thruline export --all' })
     *
     * — one manufactured refusal for EVERY non-ready status. While the app was
     * reading the reader's own record it told them, in `C.danger`, that their
     * data was missing and to run an export. `dataStatus` is `'loading'` from
     * mount until the first `adopt()`, and on a 4,565-event session that read
     * takes seconds, so the refusal screen is what a reader actually met.
     *
     * The fallback object is DELETED. A refusal now renders only when the read
     * site composed a NAMED one (`loadData`, `:594` and `:631`); a read in
     * flight renders `<Waiting>`; a record that was read and holds nothing
     * renders `<Absence>` on the surface that cannot draw it. */
    const centre = v.centre
    const refusal = centre.refusal
    const reading = v.reading
    const absenceOf = (surface) => (centre.kind === 'empty' ? this.S().localVerification ? {
      title: 'No verification evidence yet.',
      absent: 'This project graph has no accepted requirement or captured check result to draw.',
      change: 'Open Agreed checks to review the project’s verification setup. Open Sessions to inspect captured or imported agent activity and its provenance.',
    } : surface === 'graph' ? {
      /* T-6-6 (`HQ_COPY.empty.trace`): the empty Trace says what the Trace
       * is before why it is empty, and its one fill is a control — the same
       * session list the header's session button opens. */
      ...sessionAbsence(surface, v.absenceCtx),
      what: HQ_COPY.empty.trace.what,
      /* The fill sentence, then its control, then the command it leads into —
       * never a sentence ending "run" with a button between it and the command. */
      change: HQ_COPY.empty.trace.fill,
      remedyLead: HQ_COPY.empty.trace.doctor,
      action: { label: HQ_COPY.empty.trace.action, onClick: (e) => this.openMenu('session', e) },
    } : sessionAbsence(surface, v.absenceCtx) : null)

    /* toolbar — 27 callbacks, keyed by the design's own handler names */
    const on = {
      onRepGraph: () => {
        this.stopReplay()
        this.setState({ rep: 'graph', doc: 'trace', outline: false })
      },
      onRepTimeline: () => {
        this.stopReplay()
        this.setState({ rep: 'timeline', doc: 'trace', outline: false })
      },
      onRepTable: () => {
        this.stopReplay()
        this.setState({ rep: 'table', doc: 'trace', outline: false })
      },
      onFind: (e) => this.setState({ findText: e && e.target ? e.target.value : String(e || ''), findOpen: true, findIdx: 0 }),
      onFindFocus: () => {
        if (st.findText) this.setState({ findOpen: true })
      },
      onFindKey: (e) => {
        const res = this.findMatches().slice(0, 40)
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          this.setState({ findIdx: Math.min(res.length - 1, st.findIdx + 1) })
        } else if (e.key === 'ArrowUp') {
          e.preventDefault()
          this.setState({ findIdx: Math.max(0, st.findIdx - 1) })
        } else if (e.key === 'Enter') {
          const n = res[st.findIdx]
          if (n) {
            this.select(n.id, { center: true, doc: true })
            this.setState({ findOpen: false })
            this.log('Find in trace · “' + st.findText + '” · ' + countPhrase(this.findMatches().length, 'match', 'matches') + ' · focused ' + n.id)
          }
        } else if (e.key === 'Escape') this.setState({ findOpen: false })
      },
      onPrevFinding: () => this.stepFinding(-1),
      onNextFinding: () => this.stepFinding(1),
      onFindingsOnly: () => {
        this.setState({ findingsOnly: !st.findingsOnly })
        this.log(st.findingsOnly ? 'Showing all objects' : 'Showing findings only — linked claim, check and paths kept as context')
      },
      onFocusSel: () => {
        /* R11: these two used to return in silence with nothing selected —
         * a fully enabled button whose click produced a byte-identical DOM.
         * The replay controls already toast in the same situation; so do these. */
        if (!st.sel) {
          this.toast('Select an object first.')
          return
        }
        this.setState({ doc: 'trace', rep: 'graph' })
        this.centerOn(st.sel)
      },
      onLinked: () => this.setState({ linked: !st.linked }),
      onFit: () => this.fit(),
      onZoomIn: () => this.zoomBy(1.2),
      onZoomOut: () => this.zoomBy(1 / 1.2),
      onExpand: () => {
        const ex = {}
        const fx = { ...st.focusExpanded }
        this.G().nodes.forEach((n) => {
          if (!n.group) return
          if (isFocusGroupId(n.id)) fx[n.id] = true
          else ex[n.id] = true
        })
        this.setState({ expanded: ex, focusExpanded: fx })
        this.log('Expanded all groups — every recorded event shown individually')
      },
      onCollapse: () => {
        this.setState({ expanded: {}, focusExpanded: {} })
        this.log('Collapsed groups by type')
      },
      onFocusMode: (mode) => this.setFocusMode(mode),
      onFocusTools: (e) => this.openMenu('focusTools', e),
      onWiden: () => {
        if (!st.sel) {
          this.toast('Select an object first.')
          return
        }
        this.setState({ widen: { ...st.widen, [st.sel]: Math.min(240, (st.widen[st.sel] || 0) + 60) } })
      },
      onStepBack: () => {
        this.stopReplay()
        this.step(-1)
      },
      onReplay: () => this.toggleReplay(),
      onStepFwd: () => {
        this.stopReplay()
        this.step(1)
      },
      onJumpStop: this.unread() ? null : () => this.jumpStop(),
      onTestChanges: () => this.setState({ drawer: 'testchanges' }),
      onCompare: () => this.openDoc('compare'),
      onDiscrepancy: () => this.lineUp(),
      onPanelsMenu: (e) => this.openMenu('panels', e),
      /* R6: the overflow menu. It holds exactly what the compact toolbar
       * stopped showing inline — nothing is dropped, it moves. */
      onMoreMenu: (e) => this.openMenu('more', e),
      onFocusCanvas: () => this.focusCanvas(),
      onResetLayout: () => this.resetLayout(),
    }

    const graphOn = this.graphOn()

    const inspOn = {
      onProof: () => this.proof(),
      onSurrounding: () => this.surrounding(),
      onCopyRef: () => {
        const n = this.nodeById(st.sel)
        if (n) this.copy(this.refText(n), 'reference ' + (n.objId || n.id))
      },
      onCopyNext: () => this.copyNext(),
      onOpenReceipt: () => {
        this.setState({ drawer: 'receipts' })
        this.log('Opened receipt')
      },
      onCopyMd: () => this.copy(this.markdown(), 'Markdown'),
      onToggleCmd: () => this.setState({ cmdOpen: !st.cmdOpen }),
      onHide: () => this.setPanel('right', false),
      /* N13-DRAG-257: a card on the picture can be moved by a click, not only by a drag. */
      onMove: st.doc === 'trace' && st.rep === 'graph' && st.sel && this.nodeById(st.sel) ? (dx, dy) => this.moveSel(dx, dy) : null,
      onCopyCommand: (cmd) => {
        const c = this.commands().find((x) => x.cmd === cmd)
        if (cmd) this.copy(cmd, c ? c.label.replace(/^Copy /, '') : 'command')
        else this.toast((c && c.missing) || 'Nothing to copy.')
      },
    }

    const splitDown = (which) => (e) => {
      e.preventDefault()
      this.drag = { type: 'split', which, sx: e.clientX, sy: e.clientY, o: v.sizes[which] }
    }
    /* The bottom separator's own top: the drag's clamp, or R6's short-window
     * cap when that is lower — where its "taller" step stops and says why. */
    const bottomCap = this.bottomCap()
    const bottomMax = bottomCap == null ? CLAMP.bottom[1] : Math.min(CLAMP.bottom[1], bottomCap)
    const splitKey = (which) => (e) => {
      const step = CLAMP.keyStep
      let d = 0
      if (which === 'left') d = e.key === 'ArrowRight' ? step : e.key === 'ArrowLeft' ? -step : 0
      if (which === 'right') d = e.key === 'ArrowLeft' ? step : e.key === 'ArrowRight' ? -step : 0
      if (which === 'bottom') d = e.key === 'ArrowUp' ? step : e.key === 'ArrowDown' ? -step : 0
      if (!d) return
      e.preventDefault()
      this.setState({ sizes: { ...v.sizes, [which]: clamp(v.sizes[which] + d, CLAMP[which]) } })
    }

    /* P6-1 — the altitude decides which of the regions below are drawn. Workbench
     * draws every one of them exactly as before; Explore draws the canvas with
     * one slim toolbar and three dock tabs; Brief draws the story alone. */
    const alt = st.altitude
    const workbench = alt === 'workbench'
    const brief = alt === 'brief'
    const SH = HQ_COPY.shell
    /* T10 — a read time only from a read that COMPLETED (first load, session
     * switch or Refresh), in local HH:MM with the date and zone on hover. The
     * same field the status strip reads; a failed read leaves it where it was. */
    const readStampNow = readStamp(st.lastReadAt)
    const readWhen = readStampNow ? readStampNow.text : null
    const readTitle = readStampNow ? readStampNow.title : null
    const EXPLORE_TABS = ['findings', 'events', 'checks']
    /* A3-F10 — Brief draws no graph, so "Show proof", "Locate in graph" and a
     * receipt chip that only selected a node there changed nothing a person
     * could see: a dead control. From Brief they go to Explore first, where
     * the graph is, and then do what they say — the Change screen's rule for
     * its Ask doors (P6-4), and remembered as a choice the same way. */
    const toGraph = (act) => (brief ? (...args) => this.setAltitude('explore', () => act(...args)) : act)
    const hqBody = this.props.localVerification && !this.S().localVerification ? <section className="ws-hq-section" role="status">
                    <p>{st.dataRefusal ? 'The initial project reading could not be completed. Project actions are waiting for that reading.' : 'Reading project verification…'}</p>
                    {st.dataRefusal ? <button onClick={() => this.loadData()}>Retry project reading</button> : null}
                  </section> : v.centre.kind === 'refused' ? <section className="ws-hq-section ws-hq-unread" role="status">
                    {/* TRACE-MISSING-SESSION-TRUE (D-332): no verdict over a session that could not be read. */}
                    <p>{HQ_COPY.trace.unread.verdict}</p>
                    {brief && v.centre.refusal ? <Refusal title={v.centre.refusal.title} missing={v.centre.refusal.missing} remedy={v.centre.refusal.remedy} detail={v.centre.refusal.detail} /> : null}
                  </section> : <HqWorkspace key={st.session} session={this.S()} mode={st.evidenceTab} onMode={mode => this.setState({ evidenceTab: mode })} selected={st.sel} reading={reading} riskNotice={v.riskNotice} onShowRisk={() => this.applyView({ name: 'Destructive commands', tokens: ['risk:destructive'] })} onInspect={toGraph(inspectSource)} onShowProof={toGraph((id) => this.showProof(id))} onCopy={(text, label) => this.copy(text, label)} register={st.register} onRegister={(r) => this.setState({ register: r })} actions={{ onCommands: () => this.setState({ cmdOpen: true }), onProof: inspOn.onProof, onAround: inspOn.onSurrounding, onCopyRef: inspOn.onCopyRef, onReceipt: inspOn.onOpenReceipt, onCopyMarkdown: inspOn.onCopyMd }} inspector={
                <Inspector
                  width="100%"
                  position="relative"
                  shadow="none"
                  insp={v.insp}
                  tab={st.inspTab}
                  registerLabel={this.reg() === 'op' ? 'operator register' : 'manager register'}
                  cmdOpen={st.cmdOpen}
                  commands={v.cmds}
                  copyNextLabel={v.copyNextLabel}
                  reading={reading}
                  onTab={(t) => this.setState({ inspTab: t })}
                  onFinding={(key) => this.onFinding(key)}
                  on={inspOn}
                />
                  } />
    const savedList = <ul className="ws-shell-saved-list">{v.savedViews.map((sv) => <li key={sv.name}><button type="button" className="ws-shell-side-view" aria-pressed={!!sv.active} title={sv.query} onClick={() => {
      const view = this.builtinViews().concat(st.savedViews).find((x) => x.name === sv.name)
      if (view) this.applyView(view)
    }}>{sv.name}</button></li>)}</ul>

    return (
      <div className="ws-root" ref={this.rootRef} style={ROOT} data-altitude={alt}>
        {/* The banner marks the bundled corpus, and ONLY it. The `inbox`
            destination is `os/shell.jsx` reading the real §9 index and real
            project fixtures, so leaving the banner up there hung "this is
            made-up data" over the developer's own sessions — the inverse
            error, and the one `lib/inbox.test.js:353-370` calls the worse lie
            ("a demo banner was hung over a real-data surface"). Seen on screen
            before the fix. Still unconditional in the demo's own surfaces, and
            still nothing anywhere can dismiss it. */}
        {/* The way in comes FIRST, above the demo banner. With the banner on
            top, the strip read as part of the bundled demo — "Demo session —
            a bundled run, not your project" directly over a list of the
            person's own real Changes, which is the inverse of the lie the
            banner exists to prevent. Read in order it now says: here is your
            work, and below it, this workspace is showing a demo. */}
        <ChangeWayInLive />
        {v.isDemo && !inbox ? <DemoBanner label={v.demoLabel} /> : null}
        <ShellTopBar
          crumbs={[
            { key: 'project', text: v.projectLabel, onClick: (e) => this.openMenu('project', e) },
            { key: 'work', text: v.sessionLabel, work: true, onClick: (e) => this.openMenu('session', e) },
          ]}
          altitude={alt}
          onAltitude={(a) => this.setAltitude(a)}
          register={st.register}
          onRegister={(r) => this.setState({ register: r })}
          readWhen={readWhen}
          readTitle={readTitle}
          onRefresh={reading ? null : () => this.refresh()}
          refreshing={st.reading.status === 'refreshing'}
          onWalk={inbox || brief ? null : (dir) => this.walk(dir)}
          onThread={inbox || brief ? null : () => this.focusThread()}
          onEscape={() => this.clearThread()}
        />
        {workbench ? <AppBar
          projectLabel={v.projectLabel}
          sessionLabel={v.sessionLabel}
          sessionWord={v.sessionWord}
          demoLabel={v.demoLabel}
          refreshStatus={v.refreshStatus}
          reading={!!reading}
          onProjectMenu={(e) => this.openMenu('project', e)}
          onSessionMenu={(e) => this.openMenu('session', e)}
          onSettingsMenu={(e) => this.openMenu('settings', e)}
          onOpenPalette={() => this.openPalette()}
          onRefresh={() => this.refresh()}
          onConnections={() => this.setState({ connectionsOpen: true })}
        /> : null}
        {workbench && !v.isDemo ? <ObservationBar onUpdate={this.onObservationUpdate} /> : null}
        {/* One notice, about the ADDRESS this page was opened at. It sits here
            rather than over the canvas because the session that DID open is
            fine; it is dismissible because it is a fact about the link, not a
            finding about the record; and there is only ever one, because the
            app has exactly one attention queue and this is not it. */}
        <AddressNotice notice={st.notice} onDismiss={() => this.setState({ notice: null })} />
        <div className="ws-middle" style={MIDDLE}>
          {workbench ? <Rail
            rail={RAIL}
            active={v.active}
            closed={st.railClosed}
            onToggleGroup={(k) => this.setState({ railClosed: { ...st.railClosed, [k]: !st.railClosed[k] } })}
            onGo={(k) => this.railGo(k)}
            onHover={(key, label, top) => this.setState({ railHover: { text: label, top } })}
            onHoverEnd={() => this.setState({ railHover: null })}
          /> : <ShellSidebar
            workName={v.sessionLabel}
            onWork={() => this.railGo('inbox')}
            saved={alt === 'explore' ? savedList : null}
            onSaved={() => this.setAltitude('explore')}
            outlineOn={!!st.outline && alt === 'explore'}
            onOutline={() => {
              /* The Outline is the Trace as a list: at Brief it opens Explore
               * with the list showing; at Explore it toggles the list. */
              if (alt !== 'explore') this.setAltitude('explore')
              this.setState({ outline: alt !== 'explore' ? true : !st.outline, doc: 'trace', drawer: null })
            }}
          />}
          {workbench ? <RailTooltip tip={v.railTip} /> : null}
          {/* P6-2 — the Read: the engine's steps of opening this work, beside the record while it assembles.
            * It stays when the work could not be opened and the bundled demo stands in: the rail is about the
            * work that was asked for, and it is where the step that failed is said. */}
          {!inbox && st.readFor && st.readSkipped !== st.readFor && !this.props.localVerification && v.centre.kind !== 'refused'
            ? <ReadRail key={st.readFor} session={st.readFor} onSkip={() => this.setState({ readSkipped: st.readFor })} />
            : null}

          {inbox ? null : brief ? (
            /* Brief: the story of this work and nothing else — no toolbar, no
             * filter strip, no dock, no tree, and no canvas, because nothing
             * this build derives isolates one discrepancy thread (P6-3). */
            <main className="ws-shell-story" aria-label={SH.brief.work_story_label} data-primary-action={v.centre.kind === 'refused' ? undefined : ''}>
              <div className="ws-shell-story-inner">{hqBody}</div>
            </main>
          ) : (
            <React.Fragment>
              {workbench && ui.leftShow ? (
                <LeftDock
                  width={ui.leftW}
                  position={ui.leftPosition}
                  shadow={ui.leftShadow}
                  sessionLabel={v.sessionLabel}
                  sessionWord={v.sessionWord}
                  sessionClause={v.sessionClause}
                  treeCountLine={v.treeCountLine}
                  treeSearch={st.treeSearch}
                  groups={v.tree}
                  savedViews={v.savedViews}
                  rowH={v.rowH}
                  reading={reading}
                  absence={absenceOf('tree')}
                  onTreeSearch={(val) => this.setState({ treeSearch: val })}
                  onToggleGroup={(k) => this.setState({ treeClosed: { ...st.treeClosed, [k]: !st.treeClosed[k] } })}
                  onSelect={(id) => this.select(id, { center: true, doc: true })}
                  onApplyView={(name) => {
                    const view = this.builtinViews().concat(st.savedViews).find((x) => x.name === name)
                    if (view) this.applyView(view)
                  }}
                  onHide={() => this.setPanel('left', false)}
                />
              ) : null}
              <Splitter
                which="left"
                orientation="vertical"
                label="Resize left dock (arrow keys)"
                visible={workbench && ui.leftShow && ui.leftSplit}
                value={v.sizes.left}
                min={CLAMP.left[0]}
                max={CLAMP.left[1]}
                onDown={splitDown('left')}
                onKey={splitKey('left')}
              />

              <div className="ws-center" style={CENTER}>
                {workbench ? <DocTabs
                  tabs={v.docTabs}
                  active={st.doc}
                  onSelect={(id) => this.setState({ doc: id, drawer: null })}
                  onClose={(id) => this.closeDoc(id)}
                /> : null}
                <Toolbar
                  slim={!workbench}
                  slimLabel={SH.explore.toolbar_label}
                  primaryQuiet={centre.kind === 'empty'}
                  primaryHidden={centre.kind === 'refused'}
                  slots={<>
                    <button type="button" className="ws-hv ws-trace-tool" title={SH.slots.focus_thread_hint} onClick={() => this.toast(this.focusThread())}>{SH.slots.focus_thread}</button>
                    <button type="button" className="ws-hv ws-trace-tool" title={SH.slots.walk_hint} onClick={() => this.startWalk()}>{SH.slots.walk}</button>
                  </>}
                  scrubber={centre.kind === 'refused' ? null : (() => {
                    const sv = this.scrubView()
                    /* T-6-6 (rule 6): off only when the record holds no step,
                     * and then it says why and what would turn it on. At the
                     * record's two ends it stays on: the walk says "That is
                     * the last step." instead. */
                    const NW = HQ_COPY.trace.scrub.none_why
                    return <label className="ws-trace-scrub">
                      <input
                        type="range"
                        min={0}
                        max={sv.max}
                        step={1}
                        value={sv.value}
                        disabled={sv.disabled}
                        aria-label={HQ_COPY.trace.scrub.label}
                        aria-valuetext={sv.text}
                        aria-describedby={sv.disabled ? 'ws-trace-scrub-why' : undefined}
                        title={sv.disabled ? NW.reason + ' ' + NW.enabler : sv.text}
                        onChange={(e) => this.scrubTo(Number(e.target.value))}
                      />
                      <span className="ws-trace-scrub-text" aria-hidden="true">{sv.text}</span>
                      {sv.disabled ? <span id="ws-trace-scrub-why" className="ws-shell-sr">{NW.reason} {NW.enabler}</span> : null}
                    </label>
                  })()}
                  extra={<button type="button" className="ws-shell-story-toggle" aria-pressed={!!ui.rightShow} title={SH.explore.story_hint} onClick={() => this.setPanel('right')}>{SH.explore.story}</button>}
                  rep={st.rep}
                  traceDoc={st.doc === 'trace'}
                  compact={st.vw < WIDE_VW}
                  findText={st.findText}
                  findOpen={st.findOpen}
                  findResults={v.findResults}
                  findCountLine={v.findCountLine}
                  findingsOnly={st.findingsOnly}
                  linked={st.linked}
                  focusMode={st.focusMode}
                  focusLine={v.focusLine}
                  focusCanvas={!!st.focusPrev}
                  zoomPct={v.zoomPct}
                  replayLabel={v.replayLabel}
                  replayGlyph={v.replayGlyph}
                  replayPlaying={st.replay.playing}
                  on={on}
                />
                {workbench ? <FilterBar
                  collapsed={st.doc === 'trace' && st.focusMode === 'focus'}
                  chips={v.chips}
                  filterText={st.filterText}
                  groupByLabel={v.groupByLabel}
                  matchLine={v.matchLine}
                  onFilterText={(val) => this.setState({ filterText: val })}
                  onFilterKey={(e) => {
                    if (e.key === 'Enter' && st.filterText.trim()) st.filterText.trim().split(/\s+/).forEach((t) => this.addFilter(t))
                  }}
                  onRemove={(text) => this.setState({ filters: st.filters.filter((x) => x.text !== text) })}
                  on={{
                    onFiltersMenu: (e) => this.openMenu('filters', e),
                    onGroupMenu: (e) => this.openMenu('group', e),
                    onColumnsMenu: (e) => this.openMenu('columns', e),
                    onSaveMenu: (e) => this.openMenu('save', e),
                  }}
                /> : null}
                <SentenceStrip
                  sentences={v.sentences}
                  identLine={v.identLine}
                  notice={null}
                />
                {workbench ? <div className="ws-evidence-strip ws-trace-controls">
                  <span title="Captured evidence from the selected session"><span aria-hidden="true">◌ </span>{reading ? 'Reading session…' : `${this.S().events?.length.toLocaleString('en-US') || 0} recorded events`}</span>
                  <div role="group" aria-label="Trace direction">{['lanes', 'horizontal', 'vertical'].map(direction => <button key={direction} aria-pressed={st.traceDirection === direction} onClick={() => { this.memo.flow = null; this.memo.graph = null; this.setState({ traceDirection: direction, pos: {} }, () => this.initialView()) }}>{direction === 'lanes' ? '≡ ' + HQ_COPY.trace.lanes.label : direction === 'horizontal' ? '→ Horizontal' : '↓ Vertical'}</button>)}</div>
                  {FLOWING[st.traceDirection] === true && !reading && chapters.length > 0 ? <><button aria-label="Previous trace section" aria-describedby="ws-trace-part-of" disabled={chapterIndex === 0} onClick={() => showChapter(chapters[chapterIndex - 1])}>←</button><button aria-label="Next trace section" aria-describedby="ws-trace-part-of" disabled={chapterIndex >= chapters.length - 1} onClick={() => showChapter(chapters[chapterIndex + 1])}>→</button><select aria-label="Jump to trace section" value="" onChange={e => showChapter(chapters.find(f => f.firstNode === e.target.value))}><option value="" id="ws-trace-part-of">Part {chapterIndex + 1} of {chapters.length} · jump…</option>{chapters.map((f, i) => <option key={f.id} value={f.firstNode}>Part {i + 1} · {f.firstSeq == null ? 'linked evidence' : `events ${f.firstSeq}–${f.lastSeq}`}{f.alerts ? ` · ${f.alerts} review points` : ''}</option>)}</select></> : null}
                  <button className="ws-hq-open" onClick={() => this.setState({ evidenceTab: 'verdict', panels: { ...st.panels, right: true } })}>Explain this →</button>
                </div> : null}

                <div className="ws-content" style={CONTENT}>
                  {/* A REFUSAL IS A REFUSAL AND NOTHING ELSE. It renders here
                      only for a read that actually failed or a shape this build
                      does not read — loud, red, named, exactly as before. A read
                      in flight never reaches this branch. */}
                  {refusal ? (
                    <div style={REFUSAL_BOX}>
                      <Refusal title={refusal.title} missing={refusal.missing} remedy={refusal.remedy} detail={refusal.detail} />
                    </div>
                  ) : reading && st.doc !== 'trace' ? (
                    /* The three trace representations carry the waiting line
                       themselves, in their own geometry. The two documents have
                       none of their own, so the working state renders here. */
                    <div style={REFUSAL_BOX}>
                      <Waiting endpoint={reading} />
                    </div>
                  ) : st.doc === 'trace' && st.outline ? (
                    /* P6-3: the Outline twin — the same cards and thread as a
                       list, no canvas. Selection is the canvas's own `sel`. */
                    <TraceOutline
                      rows={outlineTree(this.outlineSession(), this.visibleNodes(), this.face())}
                      threadRows={(() => {
                        /* The thread's own rows come from the whole record, so a
                           thread card folded into a group is still listed. */
                        const t = this.thread()
                        if (!t) return []
                        const rows = outlineRows(this.outlineSession(), t.ids.map((id) => this.nodeById(id)).filter(Boolean), this.face())
                        const at = new Map(rows.map((r) => [r.id, r]))
                        return t.ids.map((id) => at.get(id)).filter(Boolean)
                      })()}
                      thread={this.thread()}
                      lanes={laneCounts(this.visibleNodes())}
                      status={this.threadBar() ? this.threadBar().line : null}
                      selected={st.sel}
                      face={this.face()}
                      onSelect={(id) => {
                        /* T-6-4: selecting a folded card opens its group, which
                           redraws the list; focus follows it into the new row. */
                        this.focusNodeAfter = id
                        this.select(id)
                      }}
                      onBack={() => this.setState({ outline: false })}
                    />
                  ) : st.doc === 'trace' && st.rep === 'graph' ? (
                    <GraphCanvas
                      canvasRef={this.canvasRef}
                      view={v.graph.view}
                      frames={v.graph.frames}
                      wires={v.graph.wires}
                      nodes={v.graph.nodes}
                      emptyNote={v.graph.emptyNote}
                      reading={reading}
                      absence={absenceOf('graph')}
                      minimap={v.mm}
                      replayNote={v.graph.replayNote}
                      windowNote={v.graph.windowNote}
                      legend={v.graph.legend}
                      legendThread={v.graph.legendThread}
                      lanes={v.graph.lanes}
                      threadBar={v.graph.threadBar}
                      onShowAll={() => this.clearThread()}
                      on={graphOn}
                    />
                  ) : st.doc === 'trace' && st.rep === 'timeline' ? (
                    <Timeline
                      caption={v.timelineCaption}
                      rows={v.timeline}
                      note={v.timelineNote}
                      rowH={v.rowH}
                      reading={reading}
                      absence={absenceOf('timeline')}
                      onSelect={(id) => this.select(id)}
                      onKey={(id, e) => {
                        if (e.key === 'Enter') this.select(id)
                      }}
                    />
                  ) : st.doc === 'trace' && st.rep === 'table' ? (
                    <RecordsTable
                      cols={v.table.cols}
                      head={v.table.head}
                      groups={v.table.groups}
                      foot={v.table.foot}
                      rowH={v.rowH}
                      reading={reading}
                      absence={absenceOf('table')}
                      onSelect={(id) => this.select(id)}
                      onKey={(id, e) => {
                        if (e.key === 'Enter') this.select(id)
                      }}
                    />
                  ) : null}

                  {!refusal && !reading && st.doc === 'claim' ? (
                    <ClaimDocument
                      insp={v.insp}
                      registerLabel={this.reg() === 'op' ? 'operator register' : 'manager register'}
                      onFinding={(key) => this.onFinding(key)}
                    />
                  ) : null}
                  {!refusal && !reading && st.doc === 'compare' ? (
                    <ComparePane
                      a={v.compare.a}
                      b={v.compare.b}
                      options={v.compare.options}
                      aLabel={v.compare.aLabel}
                      bLabel={v.compare.bLabel}
                      rows={v.compare.rows}
                      autopsy={v.compare.autopsy}
                      onA={(key) => this.setState({ compare: { a: key, b: st.compare.b } })}
                      onB={(key) => this.setState({ compare: { a: st.compare.a, b: key } })}
                      onChooseSession={() => this.railGo('sessions')}
                    />
                  ) : null}

                  <DestinationsDrawer
                    destination={v.drawer.destination}
                    title={v.drawer.title}
                    lead={v.drawer.lead}
                    width={v.drawer.width}
                    sections={v.drawer.sections}
                    demoLabel={v.demoLabel}
                    filter={v.drawer.filter}
                    refusal={v.drawer.refusal}
                    reading={v.drawer.reading}
                    onClose={() => {
                      this.setState({ drawer: null })
                      this.writeRoute('trace')
                      /* R13: focus goes back where it came from. */
                      const t = this.drawerTrigger
                      if (t && t.focus && document.contains(t)) t.focus()
                    }}
                  />
                </div>

                <Splitter
                  which="bottom"
                  orientation="horizontal"
                  label="Resize bottom dock (arrow keys)"
                  visible={ui.bottomShow}
                  value={v.sizes.bottom}
                  min={CLAMP.bottom[0]}
                  max={bottomMax}
                  capped={bottomMax < CLAMP.bottom[1]}
                  onDown={splitDown('bottom')}
                  onKey={splitKey('bottom')}
                />
                {ui.bottomShow ? (
                  <BottomDock
                    height={ui.bottomH}
                    tab={workbench || EXPLORE_TABS.includes(st.dockTab) ? st.dockTab : 'findings'}
                    tabs={workbench ? v.dockTabs : v.dockTabs.filter((t) => EXPLORE_TABS.includes(t.key))}
                    caption={v.dockCaption}
                    panes={v.panes}
                    rowH={v.rowH}
                    reading={reading}
                    onTab={(k) => this.setState({ dockTab: k })}
                    onSelect={(id) => this.select(id, { center: true })}
                    onProof={(key) => this.onProofFinding(key)}
                  />
                ) : null}
              </div>

              <Splitter
                which="right"
                orientation="vertical"
                label="Resize inspector (arrow keys)"
                visible={ui.rightShow && ui.rightSplit}
                value={v.sizes.right}
                min={CLAMP.right[0]}
                max={CLAMP.right[1]}
                onDown={splitDown('right')}
                onKey={splitKey('right')}
              />
              {ui.rightShow ? (
                <aside
                  className={'ws-evidence-panel' + (ui.rightSheet ? ' ws-shell-sheet' : '')}
                  aria-label="ThruLine HQ"
                  style={ui.rightSheet
                    /* P6-1: below `BP.sheet`, a bottom sheet across the canvas. */
                    ? { width: 'auto', maxWidth: 'none', position: 'absolute', top: 'auto', left: 0, right: 0, bottom: 0, maxHeight: '60vh', boxShadow: ui.rightShadow }
                    : { width: ui.rightW, position: ui.rightPosition, boxShadow: ui.rightShadow }}
                >
                  <div className="ws-evidence-panel-head"><strong>{st.evidenceTab === 'checks' ? 'Checks' : 'ThruLine HQ'}</strong><span>{this.S().label || 'This reading'}</span><button type="button" aria-label="Hide analysis panel" onClick={() => this.setPanel('right', false)}>×</button></div>
                  {hqBody}
                </aside>
              ) : null}
            </React.Fragment>
          )}

          {st.connectionsOpen ? <Connections session={this.S()} onClose={() => this.setState({ connectionsOpen: false })} onSessions={() => this.setState({ connectionsOpen: false, drawer: 'sessions' })} /> : null}
          <Menu
            kind={v.menu.kind}
            title={v.menu.title}
            x={v.menu.x}
            y={v.menu.y}
            width={v.menu.width}
            items={v.menu.items}
            input={v.menu.input}
            search={v.menu.search}
            saveName={st.saveName}
            inputNote={v.menu.inputNote}
            onSaveName={(val) => this.setState({ saveName: val })}
            onSaveView={() => this.saveView()}
            onClose={() => this.closeMenu()}
          />
          <Palette
            open={v.palette.open}
            query={v.palette.query}
            items={v.palette.items}
            empty={v.palette.empty}
            onInput={(val) => this.setState({ palQ: val, palIdx: 0 })}
            onKey={(e) => {
              const raw = this.palRaw()
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                this.setState({ palIdx: Math.min(raw.length - 1, st.palIdx + 1) })
              } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                this.setState({ palIdx: Math.max(0, st.palIdx - 1) })
              } else if (e.key === 'Enter') {
                const it = raw[st.palIdx]
                if (it) {
                  this.closePalette()
                  it.act()
                }
              }
            }}
            onClose={() => this.closePalette()}
            inputRef={this.palRef}
          />
          <Toast text={st.toast} />
        </div>
        {workbench ? <StatusStrip origin={v.status.origin} scope={v.status.scope} read={v.status.read} conn={v.status.conn} /> : null}
      </div>
    )
  }

  /**
   * Select a finding from a list row.
   *
   * @param {string} key
   */
  onFinding(key) {
    const f = this.findingByKey(key)
    if (!f) return
    this.setState({ selFinding: key, dockTab: 'findings' })
    this.select(f.node, { center: true, doc: true })
  }

  /**
   * "Show proof" from a findings row: the finding, its evidence tab, and its
   * linked evidence highlighted.
   *
   * @param {string} key
   */
  onProofFinding(key) {
    const f = this.findingByKey(key)
    if (!f) return
    this.setState({ selFinding: key, linked: true })
    this.select(f.node, { center: true, doc: true, tab: 'evidence' })
    this.log('Show proof · finding ' + key + ' · ' + f.ref)
  }
}

/**
 * A node family's `[headerBg, headerInk]`, with the grey fallback.
 *
 * @param {string} fam @returns {string[]}
 */
function FAMof(fam) {
  return FAM[fam] || FAM.grey || [C.surf2, C.ink]
}

/**
 * A wire colour by family, with the grey fallback.
 *
 * @param {string} c @returns {string}
 */
function WIREof(c) {
  return WIRE[c] || C.inkMute
}

/**
 * The recorded-kind colour for a timeline or events row. Transcribed from
 * dc.html L424 / L447; the two rows must agree or the same event reads as two
 * different kinds on two surfaces.
 *
 * @param {string} kind @returns {string}
 */
function kindColorOf(kind) {
  /* P6-6: each kind takes its §4 meaning hue — the agent's words violet,
     commands and execution cyan, the request blue; a file change is plain text,
     because §4 gives files no hue of their own. */
  if (kind === 'stop') return C.violet
  if (kind === 'exec' || kind === 'tool_call') return C.cyan
  if (kind === 'fs_change' || kind === 'snapshot') return C.ink
  if (kind === 'user_prompt') return C.blue
  return C.inkSoft
}

/**
 * Clamp a size to its `[min, max]` pair.
 *
 * @param {number} v @param {number[]} pair
 * @returns {number}
 */
function clamp(v, pair) {
  return Math.max(pair[0], Math.min(pair[1], v))
}

/**
 * Split a canvas callback's arguments into `{id, e}` whichever order the leaf
 * passed them, and whether or not it passed both.
 *
 * The design attached every one of these as a closure created per node inside
 * `renderVals` (dc.html L404-L408), so no signature was ever written down. The
 * ported leaves take a callback bundle instead, and this keeps the root working
 * for either calling convention rather than failing silently on the one that
 * was not guessed.
 *
 * @param {Array} args the callback's own arguments
 * @returns {{id: string|null, e: object|null}}
 */
function idEvt(args) {
  let id = null
  let e = null
  for (const a of args) {
    if (a && typeof a === 'object' && ('key' in a || 'clientX' in a || 'button' in a || 'currentTarget' in a)) e = a
    else if (typeof a === 'string') id = a
  }
  if (!id && e && e.currentTarget && e.currentTarget.dataset) id = e.currentTarget.dataset.id || null
  return { id, e }
}

/**
 * A resolved delay, for the refresh poll. The only timer in this file that is
 * not held on the instance, and it is awaited, not stored.
 *
 * @param {number} ms
 * @returns {Promise<void>}
 */
function wait(ms) {
  return new Promise((r) => setTimeout(r, ms))
}

/**
 * Serialise a raw projection for the inspector's Raw tab. A value that cannot
 * be serialised says so rather than blanking the tab.
 *
 * @param {object} o
 * @returns {string}
 */
function safeJson(o) {
  try {
    return JSON.stringify(o, null, 2)
  } catch (e) {
    return '{\n  "raw": "this object could not be serialised for display"\n}'
  }
}

/**
 * Call the pure adapter and let its refusal travel as an error the caller
 * names. Kept out of the class so the class never imports the adapter's
 * failure mode into its own control flow.
 *
 * @param {object} docs the seven optional documents
 * @returns {object} a Session
 * @throws {UnknownShape}
 */
function adaptSessionSafe(docs) {
  return adaptSession(docs)
}

/**
 * Named export, for tests and for a future embedder. The default export is what
 * `main.jsx` mounts.
 */
export { Workspace }
