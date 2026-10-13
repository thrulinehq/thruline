/* Settings.dc.html: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { liveOverrides } from './live.js'
import { fmtCount } from '../../data/count.js'
import { SETTINGS_PANES } from '../../routes.js'
import { choose, switchFace } from '../../data/face.js'
import { storedSettings } from './appearance.js'

/** The pane Settings draws for a route, or null when the route names none. */
export function paneOf(at) {
  const p = at && at.screen === 'settings' && at.pane;
  return p && SETTINGS_PANES[p] ? SETTINGS_PANES[p] : null;
}

/* A demo variant laid over DEMO (design gallery only): plain objects merge, the rest replace. */
function overlay(base, over) {
  if (!over || typeof over !== 'object' || Array.isArray(over)) return over === undefined ? base : over;
  const out = Object.assign({}, base);
  Object.keys(over).forEach((k) => { out[k] = overlay(base ? base[k] : undefined, over[k]); });
  return out;
}

/* The settings a person can change, as the record holds them; this.state keeps
 * only what was changed on this page since (same keys the canvas used). */
function settingsOf(D) {
  const P = D.checkPermissions && D.checkPermissions[0];
  const ex = {};
  (D.exclusions || []).forEach((e) => { ex[e.key] = !!e.on; });
  const tpl = {};
  (D.criteriaTemplates || []).forEach((t) => { tpl[t.key] = !!t.offered; });
  return {
    face: D.prefs.face,
    speaker: !!D.project.speaker,
    paused: !!D.project.paused,
    excl: ex,
    redactCopy: !!D.prefs.maskedCopyOnExport,
    cvd: !!D.prefs.colourVision,
    density: D.prefs.density,
    motion: D.prefs.motion,
    perm: !!(P && P.allowed),
    notif: Object.assign({}, D.prefs.notifications),
    digest: !!D.prefs.weeklyDigest,
    keys: D.prefs.keys,
    tpl: tpl,
  };
}

/* Settings › Plan & licence (C30 R-PLAN): Free or Pro, the licence on this
 * machine, and what needs Pro — from the one plan rule's answer (D.plan,
 * data/plan.js) and the licence document's own words (D.licence). A licence
 * that was not read states no plan. */
const PLAN_WORD = { pro: 'Pro', free: 'Free' };
export function planPaneOf(D) {
  const word = D && D.plan ? PLAN_WORD[D.plan] || null : null;
  const L = (D && D.licence) || null;
  const gateOff = !!(L && L.gateOff);
  return {
    lead: 'Approving a version needs Pro. Everything else is free.',
    word: word || 'Not known',
    planSub: word === 'Pro' ? 'This machine’s licence allows approving a version.'
      : word === 'Free' ? (gateOff ? 'Nothing in this copy of ThruLine needs a licence.' : 'Everything works except approving a version.')
        : 'ThruLine could not read the licence on this machine, so it does not say which plan this is.',
    licence: L && L.sentence ? L.sentence : 'ThruLine couldn’t find out whether this machine has a licence.',
    fix: L && L.fix ? L.fix : '',
    needs: gateOff ? 'Nothing, in this copy of ThruLine. Approving a version works without a licence here too.' : 'Approving a version.',
    plansHref: null,
  };
}

export const PROPS = {"$preview":{"width":1512,"height":920}}

export default class Component extends DCLogic {
  // UI state only; the settings themselves come from the record (settingsOf)
  state = {
    nav: 'agents',
    demo: 'normal',
    cell: 'cc:exit',
    codexTried: false,
    runOpen: false,
    copied: '',
    pathNote: false,
    removing: '',
    removed: {},
    lookNote: false
  };

  copyText(text, key) {
    try {
      if (typeof navigator !== 'undefined' && navigator.clipboard) navigator.clipboard.writeText(text);
    } catch (e) { /* the copied state below still tells the truth: prepared, not run */ }
    this.setState({ copied: key });
  }

  /* The pane an address names (`#/settings?pane=storage`, C30 R-LINKS): taken
   * once each time the address names a different pane, so the nav still moves
   * between panes after it. */
  adoptPane() {
    const pane = paneOf(this.props.at);
    if (!pane || pane === this.__pane) return;
    this.__pane = pane;
    this.state = Object.assign({}, this.state, { nav: pane });
  }

  renderVals() {
    this.adoptPane();
    // content arrives only through D: the record in the product, the canvas's demo in the gallery
    const isDemo = !this.props.data;
    const D = isDemo ? (DEMO.variants[this.state.demo] ? overlay(DEMO, DEMO.variants[this.state.demo]) : DEMO) : this.props.data;
    // C30 R-PREFS: on the record every choice is drawn as the server last kept it; the gallery keeps the canvas's
    const base = settingsOf(D);
    if (!isDemo) Object.assign(base, storedSettings(this.props, base));
    const s = Object.assign({}, base, this.state);
    const cur = () => Object.assign({}, base, this.state);
    const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', OR = '#FF7A29', LINE = '#2A2A30', CTRL = '#66666F', BLUE = '#52A8FF', HAIR = '#1F1F23';
    const CC = D.sources.claudeCode, CX = D.sources.codex, RUN = D.sources.runCapture;
    const NOW = D.readAt;
    // the system state the page is in, from the record (the switcher only swaps the record in the gallery)
    const sysState = D.hq.stopped ? 'stopped' : CC.status === 'hooks_dead' ? 'dead' : CC.status === 'upgrade_pending' ? 'upgrade' : 'normal';
    const stopped = sysState === 'stopped';
    const tech = s.face === 'technical';
    const act = (fn) => (stopped ? () => {} : fn);

    // ---- shared control looks (HQ stopped greys every action: dashed outline, t2, no-op) ----
    const ui = {
      cur: stopped ? 'not-allowed' : 'pointer',
      dis: stopped ? 'true' : 'false',
      pBg: stopped ? 'transparent' : OR,
      pFg: stopped ? T2 : '#000000',
      pBd: stopped ? '1px dashed ' + CTRL : '1px solid ' + OR,
      sFg: stopped ? T2 : T1,
      sBd: stopped ? '1px dashed ' + CTRL : '1px solid ' + CTRL,
      segBd: stopped ? '1px dashed ' + CTRL : '1px solid ' + CTRL,
      pulse: (s.motion === 'reduce') ? 'none' : 'tl-pulse 1.6s ease-in-out infinite'
    };
    // each control's own look; in the gallery every one is the shared look above,
    // and on the record a control nothing backs takes the disabled look (live.js)
    const OFF = { cur: 'not-allowed', dis: 'true', pBg: 'transparent', pFg: T2, pBd: '1px dashed ' + CTRL, sFg: T2, sBd: '1px dashed ' + CTRL, segBd: '1px dashed ' + CTRL, pulse: ui.pulse };
    const uiK = {};
    ['retry', 'runOpen', 'runCopy', 'addPath', 'excl', 'perm', 'look', 'tpl', 'remove', 'confirm'].forEach((k) => { uiK[k] = ui; });

    const mkSwitch = (on, fn, blocked) => {
      const off = stopped || !!blocked;
      return {
        checked: on ? 'true' : 'false',
        dis: off ? 'true' : 'false',
        word: on ? 'On' : 'Off',
        wordColor: off ? T2 : (on ? T1 : T2),
        trackBg: off ? 'transparent' : (on ? T1 : 'transparent'),
        trackBd: off ? '1px dashed ' + CTRL : (on ? '1px solid ' + T1 : '1px solid ' + CTRL),
        thumbBg: off ? LINE : (on ? '#000000' : T3),
        justify: on ? 'flex-end' : 'flex-start',
        cur: off ? 'not-allowed' : 'pointer',
        toggle: off ? () => {} : fn
      };
    };

    const mkSeg = (opts, cur, onPick) => opts.map((o, i) => ({
      label: o.label,
      pressed: o.key === cur ? 'true' : 'false',
      bg: o.key === cur ? LINE : 'transparent',
      color: o.key === cur ? T1 : T2,
      bl: i === 0 ? '0' : (stopped ? '1px dashed ' + CTRL : '1px solid ' + CTRL),
      pick: act(() => onPick(o.key))
    }));

    // ---- nav + the two panes (left = chosen; right = its companion) ----
    const TITLES = {
      agents: 'Agents & recording', checks: 'Checks & permissions', rules: 'Rules', privacy: 'Privacy & storage',
      notifications: 'Notifications', appearance: 'Appearance & accessibility', keyboard: 'Keyboard',
      plan: 'Plan & licence'
    };
    const right = s.nav === 'privacy' ? 'agents' : 'privacy';
    const nv = {}, pn = {};
    Object.keys(TITLES).forEach((k) => {
      const sel = k === s.nav;
      const beside = k === right;
      nv[k] = {
        bg: sel ? '#16161A' : 'transparent',
        color: sel ? T1 : T2,
        weight: sel ? '500' : '400',
        bar: sel ? OR : 'transparent',
        glow: sel ? '0 0 6px rgba(255,154,82,.55)' : 'none',
        current: sel ? 'page' : 'false',
        beside: beside,
        pick: () => this.setState({ nav: k })
      };
      pn[k] = {
        display: (sel || beside) ? 'flex' : 'none',
        order: sel ? '1' : '3',
        right: beside,
        moveLeft: () => this.setState({ nav: k })
      };
    });
    const seeWhat = () => this.setState({ nav: 'privacy' });

    // ---- storage (L1.1): prototype STORAGE figures; the total is recomputed from the parts ----
    const SWATCH = { project: T1, output: T2, demo: CTRL, saved_versions: LINE };
    const STORE = D.storage.parts.map((r) => Object.assign({}, r, {
      swatch: SWATCH[r.swatch] || CTRL,
      guard: r.restsOn ? '“' + r.restsOn.change + '” is ' + (r.restsOn.ready ? 'ready for approval' : 'resting') + ' because of these records. Without them, it isn’t.' : '',
    }));
    const fmt = (mb) => (mb >= 1000 ? (Math.round(mb / 100) / 10).toFixed(1) + ' GB' : mb + ' MB');
    const totalMb = STORE.reduce((a, r) => a + (s.removed[r.key] ? 0 : r.mb), 0);
    const totalText = fmt(totalMb);

    const mkPieces = (r) => {
      const out = [];
      const litItem = (t) => ({ isLabel: false, isItem: true, isThread: false, isCut: false, text: t, bg: '#111113', bd: '1px solid ' + CTRL, color: T1, h: '0', glow: 'none' });
      const goneItem = (t) => ({ isLabel: false, isItem: true, isThread: false, isCut: false, text: t, bg: 'transparent', bd: '1px solid ' + LINE, color: T2, h: '0', glow: 'none' });
      const label = (t) => ({ isLabel: true, isItem: false, isThread: false, isCut: false, text: t, bg: '', bd: '', color: '', h: '0', glow: 'none' });
      const thread = (lit) => ({ isLabel: false, isItem: false, isThread: true, isCut: false, text: '', bg: lit ? OR : LINE, bd: '', color: '', h: lit ? '2px' : '1px', glow: lit ? '0 0 6px rgba(255,154,82,.6)' : 'none' });
      const cut = { isLabel: false, isItem: false, isThread: false, isCut: true, text: '', bg: '', bd: '', color: '', h: '0', glow: 'none' };
      if (r.stays.length) {
        out.push(label('Stays'));
        r.stays.forEach((t, i) => { if (i) out.push(thread(true)); out.push(litItem(t)); });
        out.push(cut);
      }
      out.push(label('Goes'));
      r.goes.forEach((t, i) => { if (i) out.push(thread(false)); out.push(goneItem(t)); });
      return out;
    };

    const rows = STORE.map((r, i) => {
      const gone = !!s.removed[r.key];
      const open = !gone && s.removing === r.key;
      return {
        divider: i === 0 ? '0' : '1px solid ' + HAIR,
        swatch: gone ? 'transparent' : r.swatch,
        swatchBd: gone ? '1px dashed ' + T3 : (r.swatch === LINE ? '1px solid ' + CTRL : '0'),
        name: r.name,
        nameColor: gone ? T2 : T1,
        what: gone ? 'Removed at ' + NOW + ' · ' + fmtCount(r.mb) + ' MB freed' : (tech ? r.whatT : r.what),
        size: gone ? '0 MB' : fmt(r.mb),
        sizeColor: gone ? T3 : T1,
        canRemove: !gone,
        isRemoved: gone,
        expanded: open ? 'true' : 'false',
        askBg: open ? '#16161A' : 'transparent',
        ask: act(() => this.setState({ removing: open ? '' : r.key })),
        open: open,
        confirmLabel: 'Before you remove ' + r.name,
        lose: r.lose,
        pieces: mkPieces(r),
        hasGuard: !!r.guard,
        guard: r.guard,
        confirmText: 'Remove ' + fmt(r.mb),
        confirm: act(() => {
          const next = Object.assign({}, cur().removed);
          next[r.key] = true;
          this.setState({ removed: next, removing: '' });
        }),
        keep: () => this.setState({ removing: '' })
      };
    });
    const segs = STORE.filter((r) => !s.removed[r.key]).map((r) => {
      const base = r.swatch === LINE ? 'inset 0 0 0 1px ' + CTRL : '';
      const sel = s.removing === r.key ? '0 0 0 2px ' + OR : '';
      return { grow: String(r.mb), bg: r.swatch, sel: [base, sel].filter(Boolean).join(', ') || 'none' };
    });
    const store = {
      headline: 'ThruLine is keeping ' + totalText + ' of records on this machine',
      where: tech ? D.storage.root + ' · owner-only (' + D.storage.mode + ') · ' + D.storage.files.join(' · ') : D.storage.root + ' · only your user account can open it',
      segs: segs,
      rows: rows
    };

    // ---- where the record comes from ----
    const lit = !s.paused && (sysState === 'normal' || sysState === 'stopped');
    const bus = {
      line: lit ? '2px solid ' + OR : '2px dashed ' + CTRL,
      glow: lit ? '0 0 6px rgba(255,154,82,.55)' : 'none',
      stub: lit ? '2px solid ' + OR : '2px dashed ' + CTRL
    };
    const hookMeta = fmtCount(CC.hookEvents.count) + ' hook events · ' + CC.hookEvents.first + ' … ' + CC.hookEvents.last + ' · ' + CC.hookEvents.notSubscribed.join(', ') + ' not subscribed';
    let cc;
    if (sysState === 'dead') {
      cc = { live: false, word: 'Hooks not firing', color: T1,
        sentence: 'Its hooks point at a ThruLine program that is no longer on this machine.',
        stamp: CC.lastStep ? 'last step ' + CC.lastStep : '', hasMeta: true,
        meta: 'nothing recorded' + (CC.lastStep ? ' since ' + CC.lastStep : '') + ' · thruline sensor doctor names the hook' };
    } else if (sysState === 'upgrade') {
      cc = { live: false, word: 'Held until the upgrade', color: T1,
        sentence: 'The hooks still fire, and keep what they receive to one side until the upgrade runs.',
        stamp: 'last step read ' + CC.lastStep, hasMeta: false, meta: '' };
    } else if (s.paused) {
      cc = { live: false, word: 'Paused by you', color: T1,
        sentence: 'Nothing in ' + D.project.name + ' is recorded from ' + NOW + ' until you turn it back on.',
        stamp: 'since ' + CC.since + ' · paused ' + NOW, hasMeta: false, meta: '' };
    } else if (stopped) {
      cc = { live: true, word: 'Connected · observing', color: BLUE,
        sentence: 'As HQ last read it at ' + NOW + '. The hooks keep recording while HQ is stopped; this row catches up when it restarts.',
        stamp: 'as read ' + NOW, hasMeta: tech,
        meta: hookMeta };
    } else {
      cc = { live: true, word: 'Connected · observing', color: BLUE,
        sentence: 'Official hooks, installed per project. ThruLine never sees your Claude sign-in.',
        stamp: 'since ' + CC.since + ' · last step ' + CC.lastStep, hasMeta: tech,
        meta: hookMeta };
    }
    cc.notLive = !cc.live;
    cc.anim = stopped ? 'none' : ui.pulse;
    cc.nodeBg = lit ? T1 : 'transparent';
    cc.nodeBd = lit ? '0' : '1.5px dashed ' + T3;
    cc.nodeGlow = lit ? '0 0 0 3px rgba(255,122,41,.28)' : 'none';
    cc.name = CC.name;
    const src = {
      cc: cc,
      cx: {
        name: CX.name,
        sub: s.codexTried ? 'Tried again at ' + NOW + ' — still not answering. Start ' + CX.name + ' first; ThruLine doesn’t launch it for you.' : 'Start ' + CX.name + ', then try again. ThruLine doesn’t install or launch it for you.',
        word: CX.status === 'not_answering' ? 'App server not answering' : 'Connected · observing',
        line: CX.status === 'not_answering' ? 'Nothing from ' + CX.name + ' is being recorded until its app server answers.' : '',
        colHead: CX.name + ' ',
        column: CX.status === 'not_answering' ? '· not answering now' : '',
        retry: act(() => this.setState({ codexTried: true }))
      },
      rec: {
        line: tech ? totalText + ' · append-only · hash-chained' : totalText + ' · added to, never rewritten',
        nodeBg: lit ? T1 : 'transparent',
        nodeBd: lit ? '0' : '1.5px solid ' + T2
      }
    };
    const runCap = {
      word: RUN.on ? 'Run capture: on' : 'Run capture: off',
      tools: 'Turning it on puts a small recorder ahead of ' + RUN.tools.named.join(', ') + (RUN.tools.more ? ' and ' + fmtCount(RUN.tools.more) + ' more' : '') + ' on your PATH, so each run is recorded as a run, with its exit code.',
    };
    const busCaption = lit ? 'the lit thread is what is being recorded now' : 'nothing is reaching the record right now';

    const run = {
      closed: !s.runOpen,
      isOpen: s.runOpen,
      open: act(() => this.setState({ runOpen: true })),
      close: () => this.setState({ runOpen: false }),
      copied: s.copied === 'shim',
      copyLabel: s.copied === 'shim' ? 'Copied' : 'Copy command',
      copy: act(() => this.copyText('thruline shim install', 'shim'))
    };

    // ---- capability matrix (cells follow docs/ADAPTER-CAPABILITIES.md and crates/capture) ----
    const MARK = {
      yes: { glyph: '✓', word: 'Yes', color: T1, g: T1 },
      partly: { glyph: '~', word: 'Partly', color: T1, g: T2 },
      notyet: { glyph: '◌', word: 'Not captured yet', color: T2, g: T3 },
      no: { glyph: '—', word: 'No, on purpose', color: T2, g: T3 }
    };
    const CAPS = D.capabilities.map((c) => ({ key: c.key, label: c.label, cc: [c.cc.mark, c.cc.reason, c.cc.tech], cx: [c.cx.mark, c.cx.reason, c.cx.tech] }));
    const AG = { cc: CC.name, cx: CX.name };
    let why = { head: '', word: '', text: '', tech: '' };
    const caps = CAPS.map((c, i) => {
      const cell = (ag) => {
        const v = c[ag];
        const m = MARK[v[0]];
        const id = ag + ':' + c.key;
        const sel = s.cell === id;
        if (sel) why = { head: AG[ag] + ' · ' + c.label, word: m.word, text: v[1], tech: v[2] };
        return {
          glyph: m.glyph, word: m.word, color: m.color, gColor: m.g,
          pressed: sel ? 'true' : 'false',
          aria: AG[ag] + ', ' + c.label + ': ' + m.word + '. Show the reason.',
          bg: sel ? '#16161A' : 'transparent',
          bd: sel ? '1px solid ' + OR : '1px solid transparent',
          glow: sel ? '0 0 0 1px rgba(255,122,41,.25), 0 0 12px rgba(255,122,41,.15)' : 'none',
          pick: () => this.setState({ cell: id })
        };
      };
      return { label: c.label, divider: i === 0 ? '0' : '1px solid ' + HAIR, cc: cell('cc'), cx: cell('cx') };
    });

    // ---- switches ----
    const sw = {
      speaker: mkSwitch(s.speaker, () => this.setState({ speaker: !cur().speaker })),
      pause: mkSwitch(s.paused, () => this.setState({ paused: !cur().paused })),
      redact: mkSwitch(s.redactCopy, () => choose(this.props, 'masked_copy', !cur().redactCopy, (x) => this.setState({ redactCopy: x }))),
      cvd: mkSwitch(s.cvd, () => choose(this.props, 'colour_vision', cur().cvd ? 'standard' : 'proven_teal', (x) => this.setState({ cvd: x === 'proven_teal' }))),
      digest: mkSwitch(s.digest, () => choose(this.props, 'weekly_summary', !cur().digest, (x) => this.setState({ digest: x })))
    };

    const EXCL = D.exclusions;
    const excl = EXCL.map((e) => {
      const on = !!s.excl[e.key];
      return {
        path: e.path, note: e.note,
        checked: on ? 'true' : 'false',
        mark: on ? '✓' : '',
        boxBg: on ? (stopped ? T2 : T1) : 'transparent',
        boxBd: on ? '1px solid ' + (stopped ? T2 : T1) : (stopped ? '1px dashed ' + CTRL : '1px solid ' + CTRL),
        toggle: act(() => {
          const next = Object.assign({}, cur().excl);
          next[e.key] = !next[e.key];
          this.setState({ excl: next });
        })
      };
    });

    // ---- checks & permissions (run lane) ----
    const P0 = D.checkPermissions[0];
    const perm = s.perm
      ? { word: 'Allowed by you · ' + (P0 && P0.at ? P0.at.time + ' ' + P0.at.day : ''), color: T1, sub: 'ThruLine asks once per project. A check runs only when someone presses Check it again.', btn: 'Take back permission' }
      : { word: 'Taken back at ' + NOW, color: T2, sub: 'The next check here asks you first. Nothing that already ran is changed.', btn: 'Allow again' };
    perm.toggle = act(() => this.setState({ perm: !cur().perm }));
    perm.project = P0 ? P0.project : '';
    // one row per project: the first row's markup when someone decided, the second's when nobody was asked
    const permRows = D.checkPermissions.map((p, i) => (i === 0 && p.allowed !== null
      ? Object.assign({ decided: true, look: uiK.perm }, perm)
      : {
        decided: false,
        project: p.project,
        word: p.allowed === null ? 'Not asked yet' : (p.allowed ? 'Allowed by you' : 'Taken back'),
        note: p.hasRecords ? 'asks the first time you check something here' : 'nothing recorded here yet',
        divider: i === 0 ? '0' : '1px solid ' + HAIR,
      }));
    const where = {
      project: D.checkWhere.project,
      folder: D.checkWhere.folder,
      found: 'The project folder, with the tools on your PATH. Found here: ' + D.checkWhere.toolsFound.join(', ') + '.',
    };

    // ---- rules ----
    const TPL = D.criteriaTemplates;
    const tpls = TPL.map((t, i) => {
      const used = !!s.tpl[t.key];
      return {
        text: t.text,
        divider: i === 0 ? '0' : '1px solid ' + HAIR,
        word: used ? 'Offered to each new Change, never added by itself' : 'Suggestion',
        btn: used ? 'Stop offering' : 'Offer to new Changes',
        toggle: act(() => {
          const next = Object.assign({}, cur().tpl);
          next[t.key] = !next[t.key];
          this.setState({ tpl: next });
        })
      };
    });

    // ---- notifications ----
    const NOTIF = [
      { key: 'ready', label: 'Your move: ready to approve', sub: 'A tracked Change became ready for your approval.' },
      { key: 'not_backed', label: 'Not backed', sub: 'An agent said the work was done, but the record does not back it.' },
      { key: 'agent_waiting', label: 'Agent waiting', sub: 'An agent is waiting for your answer.' },
      { key: 'recording_stopped', label: 'Recording stopped', sub: 'ThruLine stopped recording a project you were recording.' }
    ];
    const notifs = NOTIF.map((n, i) => ({
      label: n.label,
      sub: n.sub,
      labelColor: n.blocked ? T2 : T1,
      divider: i === 0 ? '0' : '1px solid ' + HAIR,
      sw: mkSwitch(n.blocked ? false : !!s.notif[n.key], () => {
        const next = !cur().notif[n.key];
        const guarded = this.props.actions?.setPreference
        const source = guarded ? { ...this.props, setPreference: guarded } : this.props
        choose(source, 'notifications.' + n.key, next, (x) => this.setState({ notif: Object.assign({}, cur().notif, { [n.key]: x }) }));
      }, n.blocked)
    }));

    // ---- appearance ----
    const cvd = s.cvd
      ? { c: '#2DD4BF', bd: 'rgba(45,212,191,.6)', bg: 'rgba(45,212,191,.08)',
          sentence: 'Proven is teal now, so it stays clearly apart from “the record shows the opposite” for red-green colour blindness. Shapes are unchanged, so nothing else moves.',
          tech: 'proven #2DD4BF · 11.28:1 on black · vs coral ΔE deutan 40.8 (was 15.4)' }
      : { c: '#3DD68C', bd: 'rgba(61,214,140,.6)', bg: 'rgba(61,214,140,.08)',
          sentence: 'Turn this on and Proven becomes teal, so it stays clearly apart from “the record shows the opposite” for red-green colour blindness. Shapes stay the same.',
          tech: 'proven #3DD68C · 11.20:1 on black · vs coral ΔE deutan 15.4' };
    const segFace = mkSeg([{ key: 'plain', label: 'Plain' }, { key: 'technical', label: 'Technical' }], s.face, (k) => switchFace(this.props, k, (x) => this.setState({ face: x })));
    const segDensity = mkSeg([{ key: 'comfortable', label: 'Comfortable' }, { key: 'compact', label: 'Compact' }], s.density, (k) => choose(this.props, 'density', k, (x) => this.setState({ density: x })));
    const segMotion = mkSeg([{ key: 'system', label: 'Follow this Mac' }, { key: 'reduce', label: 'Always reduce' }], s.motion, (k) => choose(this.props, 'motion', k, (x) => this.setState({ motion: x })));
    const densityText = s.density === 'compact' ? 'Compact: Inbox and table rows on one line.' : 'Comfortable: Inbox and table rows on two lines. The default.';

    // ---- keyboard (WCAG 2.1.4) ----
    const KEYMAP = [
      { keys: ['j', 'k'], label: 'Next / previous in a list', single: true },
      { keys: ['↵'], label: 'Open', single: false },
      { keys: ['c'], label: 'Check it again', single: true },
      { keys: ['y'], label: 'Copy for the agent', single: true },
      { keys: ['h'], label: 'Hide until the record changes', single: true },
      { keys: ['x'], label: 'Select', single: true },
      { keys: ['t'], label: 'Plain / Technical', single: true },
      { keys: ['?'], label: 'This map', single: true },
      { keys: ['g i'], label: 'Inbox', single: true },
      { keys: ['g c'], label: 'Changes', single: true },
      { keys: ['g d'], label: 'What counts as done', single: true },
      { keys: ['g w'], label: 'The whole story', single: true },
      { keys: ['g s'], label: 'Settings', single: true },
      { keys: ['⌘K'], label: 'Find, ask, act', single: false }
    ];
    const keymap = KEYMAP.map((k) => {
      const offNow = k.single && s.keys === 'off';
      const txt = k.keys.map((x) => (k.single && s.keys === 'mod') ? '⌥' + x : x).join(' / ');
      return {
        keyText: txt,
        label: k.label,
        keyColor: offNow ? T3 : T1,
        kbdBd: offNow ? LINE : CTRL,
        labelColor: offNow ? T2 : T1,
        state: offNow ? 'off' : ''
      };
    });
    const segKeys = mkSeg([{ key: 'on', label: 'On' }, { key: 'mod', label: 'Only with ⌥' }, { key: 'off', label: 'Off' }], s.keys, (k) => this.setState({ keys: k }));
    const keysText = s.keys === 'off' ? 'Off: only ⌘K and ↵ work; everything else is in the palette.' : (s.keys === 'mod' ? 'Each single key now needs ⌥ held, so typing can’t trigger one.' : 'On: one key per action. Turn off or add ⌥ if you use speech input.');

    // ---- system state + top bar + status bar ----
    const SYS = {
      dead: { glyph: '◌', lead: 'Nothing recorded' + (CC.lastStep ? ' since ' + CC.lastStep : '') + '.', text: CC.name + '’s hooks point at a ThruLine program that is no longer on this machine. Run the repair in each project.', cmd: 'thruline init', label: 'Copy repair command', slot: false },
      upgrade: { glyph: '↑', lead: 'ThruLine was updated and needs a one-time upgrade.', text: 'Other ThruLine commands wait until it runs, and change nothing meanwhile.', cmd: 'thruline upgrade', label: 'Copy command', slot: true },
      stopped: { glyph: '‖', lead: 'HQ stopped.', text: 'This page shows what it read at ' + NOW + ', and every action on it is off. Recording carries on without it.', cmd: 'thruline ui', label: 'Copy restart command', slot: false }
    };
    const sd = SYS[sysState];
    // the upgrade's time estimate: from the record; the canvas drew a placeholder for it
    const estimate = (D.upgrade && D.upgrade.estimate) || (isDemo ? 'time estimate · filled in at runtime' : '');
    const sys = sd
      ? { show: true, glyph: sd.glyph, lead: sd.lead, text: sd.text, cmd: sd.cmd, slot: sd.slot && !!estimate, estimate: estimate,
          label: s.copied === 'sys-' + sysState ? 'Copied' : sd.label,
          copy: () => this.copyText(sd.cmd, 'sys-' + sysState) }
      : { show: false, glyph: '', lead: '', text: '', cmd: '', slot: false, estimate: '', label: '', copy: () => {} };

    let pill;
    if (sysState === 'dead') pill = { word: 'Not recording', color: T2, dotBg: 'transparent', dotBd: '1px dashed ' + T3, anim: 'none' };
    else if (sysState === 'upgrade') pill = { word: 'Upgrade pending', color: T2, dotBg: 'transparent', dotBd: '1px dashed ' + T3, anim: 'none' };
    else if (stopped) pill = { word: 'HQ stopped', color: T2, dotBg: T3, dotBd: '0', anim: 'none' };
    else if (s.paused) pill = { word: 'Paused by you', color: T2, dotBg: 'transparent', dotBd: '1px dashed ' + T3, anim: 'none' };
    else pill = { word: 'Recording', color: BLUE, dotBg: BLUE, dotBd: '0', anim: ui.pulse };

    let statusLeft;
    if (sysState === 'dead') statusLeft = 'Nothing recorded' + (CC.lastStep ? ' since ' + CC.lastStep : '') + ' · ' + CC.name + '’s hooks aren’t firing';
    else if (sysState === 'upgrade') statusLeft = 'Upgrade pending · other ThruLine commands wait for it';
    else if (stopped) statusLeft = 'HQ stopped · showing what was read at ' + NOW;
    else statusLeft = (s.paused ? 'Recording paused by you in ' + D.project.name : CC.name + ' recording') + ' · ' + CX.name + (CX.status === 'not_answering' ? ' not answering' : ' observing') + ' · run capture ' + (RUN.on ? 'on' : 'off');

    const DEMOS = [['normal', 'Normal'], ['dead', 'Hooks not firing'], ['upgrade', 'Upgrade pending'], ['stopped', 'HQ stopped']];
    const demos = DEMOS.map((d) => ({
      label: d[1],
      pressed: s.demo === d[0] ? 'true' : 'false',
      bg: s.demo === d[0] ? LINE : 'transparent',
      color: s.demo === d[0] ? T1 : T2,
      border: '1px solid ' + (s.demo === d[0] ? CTRL : LINE),
      pick: () => this.setState(d[0] === 'normal'
        ? { demo: 'normal', copied: '', removed: {}, removing: '', paused: false, codexTried: false }
        : { demo: d[0], copied: '', removing: '' })
    }));

    const content = {
      speakerLabel: 'Add up to 3 lines when a ' + CC.name + ' session starts',
      speakerOn: 'On for ' + D.project.name + ' from its next session · you agreed at ' + NOW,
      pauseLabel: 'Pause recording in ' + D.project.name,
      pausedGap: 'Not recorded — paused by you, ' + NOW + ' – now',
      recordTech: D.storage.root + ' · ledger.db holds steps, exec.db holds runs · append-only · hash-chained',
      unsupported: D.unsupportedAgents.map((a) => a.name).join(', '),
      unsupportedWhy: ' · Not supported — ' + D.unsupportedAgents.map((a) => a.reason).join('; ') + '.',
      policyPath: D.policy.path,
      policyText: D.policy.text,
      policyPreview: 'Under this policy, ' + fmtCount(D.policy.wouldNotBeReady.changes) + ' of your last ' + fmtCount(D.policy.wouldNotBeReady.ofLast) + ' Changes would not be ready.',
      policyExample: 'What counts as done for ' + D.policy.example.name + ' ',
      masking: 'detector ' + D.masking.detector + ' · ' + fmtCount(D.masking.masked) + ' masked · best-effort',
      localModel: D.egress.localModel ? 'On' : 'Off',
      speakerEgress: 'Up to 3 a session in ' + D.project.name + '. They travel with the session to your agent’s provider.',
      previewMeta: '· version ' + D.statePreview.version + ' · ' + D.statePreview.scope.join(', '),
      asideNote: 'Changes here are saved on this machine as you make them. Nothing on this page is sent anywhere.',
      planNav: 'Plan & licence',
      speakerConsent: 'Those lines become part of what your agent sends to its own provider. Off until you agree, one project at a time.',
      redactSub: 'A second, labelled file that names the file it came from. The original is never edited.',
      digestSub: 'Kept on this machine, next to the record. Nothing is sent.',
      motionSub: 'Every animation shows a change of state. Reduced, the finished state appears at once.',
      lookSub: 'Yours alone, remembered on this machine. States never depend on colour alone.',
      tplNote: 'A line here is only offered. Each new Change still asks you to accept its lines, and accepting them approves nothing.',
      mcpWord: 'Installed with the hooks',
      triageNote: 'Triage keys (j k c y h x) work only while a list has focus, so typing elsewhere never triggers them.',
      faceTail: ' switches it anywhere.',
    };
    // the canvas's own notes to its reader: drawn in the gallery only
    const notes = isDemo ? Object.assign({}, DEMO.designNotes) : {};

    const out = {
      isDemo: isDemo,
      notes: notes,
      uiK: uiK,
      planLink: null,
      planPane: planPaneOf(D),
      openCommand: this.props.openCommand ? (e) => { if (e && e.preventDefault) e.preventDefault(); this.props.openCommand() } : undefined,
      content: content,
      runCap: runCap,
      permRows: permRows,
      where: where,
      crumb: TITLES[s.nav],
      tech: tech,
      s_speaker: s.speaker,
      s_paused: s.paused,
      ui: ui,
      nv: nv,
      pn: pn,
      seeWhat: seeWhat,
      totalText: totalText,
      store: store,
      bus: bus,
      busCaption: busCaption,
      src: src,
      run: run,
      caps: caps,
      why: why,
      sw: sw,
      excl: excl,
      pathNote: s.pathNote,
      addPath: act(() => this.setState({ pathNote: !this.state.pathNote })),
      perm: perm,
      lookNote: s.lookNote,
      lookElsewhere: act(() => this.setState({ lookNote: !this.state.lookNote })),
      tpls: tpls,
      notifs: notifs,
      cvd: cvd,
      segFace: segFace,
      segDensity: segDensity,
      segMotion: segMotion,
      densityModified: s.density !== 'comfortable',
      densityText: densityText,
      keymap: keymap,
      segKeys: segKeys,
      keysText: keysText,
      sys: sys,
      pill: pill,
      readStamp: stopped ? 'read ' + NOW + ' · not refreshing' : 'read ' + NOW + ' ↻',
      faceTop: {
        plain: tech ? T2 : T1,
        tech: tech ? T1 : T2,
        aria: tech ? 'Face: Technical. Switch to Plain' : 'Face: Plain. Switch to Technical'
      },
      toggleFace: () => switchFace(this.props, cur().face === 'plain' ? 'technical' : 'plain', (x) => this.setState({ face: x })),
      statusLeft: statusLeft,
      demos: demos
    };
    if (!isDemo) {
      liveOverrides(out, {
        D, s, tech, stopped, NOW, OFF, ui, mkSwitch, mkSeg, colors: { T1, T2, T3, OR, LINE, CTRL, BLUE, HAIR },
        setState: (p) => this.setState(p), cur, actions: this.props.actions || {}, prefs: this.props.prefs || null,
        notificationPermission: this.props.notificationPermission || null,
      });
    }
    return out;
  }
}
