/* Inbox.dc.html: the artboard's logic, from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from
 * `this.props.data` (the store, through ui/src/hq/data/project.js) or, in the
 * design gallery, from DEMO (./demo.js, the contract in ./DATA.md);
 * `this.state` holds UI state only.
 *
 * Every group is a list. The canvas drew one row of each kind; each kind's
 * row markup is now the template of its group's loop, so the gallery draws
 * exactly what the canvas drew and the product draws every row it has. */
import React from 'react'
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { densityOf, choose } from '../../data/face.js'
import { inboxText, rowsUnder } from './text.js'
import { fmtCount } from '../../data/count.js'
import { fmtClock } from '../../data/project.js'
import { DecisionGroup, DecisionPreview, decisionText } from './decisions.jsx'
import { rowKeys } from '../../keys/list.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

/** Which preview block draws a row of each kind. */
export const PREVIEW_OF = { approve: 'post', prompt: 'prompt', check: 'disc', agent: 'stripe', live: 'live', decision: 'decision' };

const ORANGE = '#FF7A29';

/** One row's look, selected (`on`) or not, as the canvas draws it. The Changes screen draws the same rows. */
export function rowUi(on, comfy, pick) {
  return {
    bg: on ? '#16161A' : '#0A0A0B',
    edge: on ? ORANGE : 'transparent',
    glow: on ? '0 0 8px rgba(255,154,82,.55)' : 'none',
    cur: on ? 'true' : 'false',
    pick: pick,
    btnBg: on ? ORANGE : 'transparent',
    btnFg: on ? '#000000' : '#EDEDEF',
    btnBd: on ? ORANGE : '#66666F',
    btnW: on ? '600' : '500',
    kbdFg: on ? '#000000' : '#A1A1AA',
    kbdBd: on ? 'rgba(0,0,0,.4)' : '#66666F',
    keys: on && comfy ? 'inline-flex' : 'none'
  };
}

/** The hint bar's word for the Settings › Keyboard mode (the canvas drew "on"). */
export const KEYS_HINT = Object.freeze({ on: 'Single keys on · ', mod: 'Single keys need ⌥ · ', off: 'Single keys off · ' });

/** One segment of a segmented control (Density, Show), pressed or not. */
export function segUi(on) {
  return {
    bg: on ? '#2A2A30' : 'transparent',
    color: on ? '#EDEDEF' : '#A1A1AA',
    pressed: on ? 'true' : 'false'
  };
}

export default class Component extends DCLogic {
  state = { sel: 'disc', density: 'comfortable', filter: 'all', nothingOpen: false, banner: true, codexTried: false, copied: false };

  renderVals() {
    const s = this.state;
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    const t = inboxText(D, { copied: s.copied, codexTried: s.codexTried, retryAt: s.codexTriedAt });
    /* C30 R-PREFS: the density is the stored one on a live screen; the gallery keeps the canvas's own toggle */
    const density = densityOf(this.props, s.density);
    const comfy = density === 'comfortable';

    // C29: decisions the record shows broken come first (docs/decisions/API.md, "UI and gate use");
    // the gallery's DEMO has none, so the artboard is unchanged
    const model = !isDemo && !t.reading ? this.props.decisions : null;
    const decisions = ((model && model.broken) || []).map((r) => ({ id: 'decision:' + r.id, kind: 'decision', t: decisionText(r), row: r }));
    const decisionById = {};
    for (const r of decisions) decisionById[r.id] = r;

    // the selected row: the one picked, else the first row the person has to act on
    const order = [].concat(decisions, t.groups.you, t.groups.check, t.groups.agent, t.groups.live);
    const selected = decisionById[s.sel] || t.byId[s.sel] || order[0] || null;
    const selId = selected ? selected.id : null;

    const row = (id) => rowUi(selId === id, comfy, () => this.setState({ sel: id }));
    const withUi = (g) => g.map((r) => Object.assign({}, r.t, { id: r.id, kind: r.kind, ui: row(r.id), copyAct: copyActOf(r) }));

    const seg = segUi;

    const f = s.filter;
    const show = (ok) => (ok ? 'flex' : 'none');
    const filterDefs = [
      { key: 'all', label: 'All', count: '' },
      { key: 'you', label: 'Your move', count: t.n.yourMove },
      { key: 'agent', label: 'The agent’s', count: t.n.agentMove },
      { key: 'live', label: 'Recording', count: t.n.live }
    ];
    const filters = filterDefs.map((x, i) => Object.assign({}, x, seg(f === x.key), {
      divider: i === 0 ? '0' : '1px solid #66666F',
      countShow: x.count ? 'inline' : 'none',
      pick: () => this.setState({ filter: x.key })
    }));

    const pvKind = selected ? PREVIEW_OF[selected.kind] : null;
    // C30 R-LEDGER: on a row from the record "Copy it again" opens the sheet where the copy is real
    // and kept (Repair-Loop's Copy message); a toggle that said "copied again" would claim a copy
    // that did not happen. The gallery's canvas row keeps its toggle.
    const copyActOf = (r) => ((r.t && r.t.recorded && r.t.tellHref) || (r.kind === 'check' && r.t && r.t.checkHref)
      ? () => { this.setState({ sel: r.id }); if (typeof window !== 'undefined') window.location.hash = r.t.tellHref || r.t.checkHref; }
      : () => this.setState({ sel: r.id, copied: r.id }));
    const sp = selected ? Object.assign({}, selected.t, { copyAct: copyActOf(selected) }) : {};
    const any = (g) => t.groups[g].length > 0;
    const d = {
      rowPad: comfy ? '10px 16px 10px 20px' : '8px 16px 8px 20px',
      rowGap: comfy ? '4px' : '0px',
      line2: comfy ? 'flex' : 'none',
      compactOnly: comfy ? 'none' : 'inline',
      modified: comfy ? 'none' : 'inline-block',
      comfy: seg(comfy),
      compact: seg(!comfy),
      setComfy: () => choose(this.props, 'density', 'comfortable', (v) => this.setState({ density: v })),
      setCompact: () => choose(this.props, 'density', 'compact', (v) => this.setState({ density: v }))
    };
    // C30 P0-5: a filter's count is the rows it shows. Decisions are not Changes and no filter
    // counts them, so their group is drawn under All only.
    const showDecisions = !t.reading && f === 'all' && decisions.length > 0;
    // "The agent's" counts the threads that are the agent's move. On the live record those are
    // recording, so they are filed under "Recording now", not "Waiting on the agent" (no note was
    // copied): that filter shows them there. In the gallery the recording row is the person's
    // thread, so the canvas's "The agent's" view is unchanged.
    // C30 R-LEDGER: "Waiting on the agent" is filed by the record (a note was copied, no work since)
    // while every row keeps the engine's move. A filter shows the waiting rows of its own move only,
    // and its count equals the rows it shows: the one rule Changes draws through too (C46 §1.1).
    const under = rowsUnder(t, D, f);
    const liveRows = f === 'agent' ? under.live : t.groups.live;
    const agentRows = under.agent;

    // a group with no rows is not drawn in the product; the gallery draws every group the canvas drew
    // while the project is being read (C29 R1) no group is drawn, not even "Nothing needed"
    const g = t.reading ? { you: 'none', check: 'none', agent: 'none', live: 'none', nothing: 'none' } : {
      you: show((f === 'all' || f === 'you') && (isDemo || any('you'))),
      check: show((f === 'all' || f === 'you') && (isDemo || any('check'))),
      agent: show(isDemo ? (f === 'all' || f === 'agent' || (f === 'you' && agentRows.length > 0)) : agentRows.length > 0),
      live: show((f === 'all' || f === 'live' || (f === 'agent' && liveRows.length > 0)) && (isDemo || liveRows.length > 0)),
      nothing: show(f === 'all')
    };
    const nn = {
      open: s.nothingOpen,
      caret: s.nothingOpen ? '▾' : '▸',
      expanded: s.nothingOpen ? 'true' : 'false',
      verb: s.nothingOpen ? 'collapse' : 'expand',
      toggle: () => this.setState({ nothingOpen: !s.nothingOpen })
    };
    // C30 R-KEYS: the list keys walk the rows in the order they are drawn, and do what a click on them does
    const drawn = [].concat(showDecisions ? decisions : [], g.you === 'flex' ? t.groups.you : [], g.check === 'flex' ? t.groups.check : [],
      g.agent === 'flex' ? agentRows : [], g.live === 'flex' ? liveRows : []);
    const keys = rowKeys({
      rows: drawn.map((r) => {
        const address = D.hideRows && D.hideRows[r.threadId]
        return { id: r.id, kind: r.kind, t: r.t, select: () => this.setState({ sel: r.id }), copy: copyActOf(r),
          hide: address && !address.hidden && this.props.hideRow ? () => this.props.hideRow(address) : null }
      }),
      selId: selId,
      expand: g.nothing === 'flex' ? nn.toggle : null
    });

    return {
      isDemo: isDemo,
      keys: keys,
      keysHint: KEYS_HINT[this.props.keysMode] || KEYS_HINT.on,
      readAgain: !isDemo ? this.props.reloadInbox : undefined,
      reading: !!t.reading,
      // under "The agent's", the Recording group's header counts the rows it shows
      t: t.reading ? t : Object.assign({}, t, { n: Object.assign({}, t.n, f === 'agent' ? { live: fmtCount(liveRows.length), agent: fmtCount(agentRows.length) } : { agent: fmtCount(agentRows.length) }) }),
      sp: sp,
      groups: { you: withUi(t.groups.you), check: withUi(t.groups.check), agent: withUi(agentRows), live: withUi(liveRows) },
      d: d,
      // C29: the two slots the ported template keeps for decisions; empty in the gallery
      slot: {
        decisionGroup: showDecisions ? React.createElement(DecisionGroup, { rows: decisions.map((r) => Object.assign({}, r, { ui: row(r.id) })), d: d }) : null,
        decisionPreview: pvKind === 'decision' ? React.createElement(DecisionPreview, { row: selected.row }) : null
      },
      g: g,
      filters: filters,
      nn: nn,
      pv: {
        decision: pvKind === 'decision',
        disc: pvKind === 'disc',
        post: pvKind === 'post',
        prompt: pvKind === 'prompt',
        stripe: pvKind === 'stripe',
        live: pvKind === 'live'
      },
      banner: {
        show: isDemo && s.banner,
        dismiss: () => this.setState({ banner: false })
      },
      codex: {
        tried: s.codexTried,
        notTried: !s.codexTried,
        retry: () => {
          if (this.props.retryCapture) this.props.retryCapture()
          this.setState({ codexTried: true, codexTriedAt: fmtClock(new Date()) })
        }
      }
    };
  }
}
