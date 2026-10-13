/* Change-Brief.dc.html: the artboard's logic, from the canvas. renderVals()
 * returns everything template.jsx reads.
 *
 * Content comes from one object only: `const D = this.props.data || DEMO`
 * (demo.js; the contract is DATA.md; the live snapshot is built by
 * `ui/src/hq/data/change.js`). In the design gallery (`props.data` absent) the
 * snapshot drawn is `DEMO.states[state.demo]`, so the canvas's demo switcher
 * keeps working on DEMO and on nothing else; with live data the snapshot is `D`
 * itself and every design annotation (`isDemo`) is hidden.
 *
 * The canvas drew four situations (s1, checking, ready, moved). Here each link
 * of a statement's chain is read from that statement's own proof facts — the
 * run it rests on, the version that run ran on, the edit after it, the
 * re-check, the verdict — so the same drawing serves any statement in the
 * record. The four canvas situations are four combinations of those facts and
 * render exactly as the canvas drew them (pinned by the old-vs-new markup
 * comparison and the pixel diff). Presentation — colours, glyphs, geometry,
 * sentence skeletons in the product's language — stays here. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { storageSentence } from '../../data/project.js'
import { fmtCount } from '../../data/count.js'
import React from 'react'
import { BriefDecisions, briefDecisions } from './decisions.jsx'
import { BriefNotes } from './notes.jsx'
import { BriefObjectionLines, BriefObjectionForm } from './objection.jsx'
import { VERDICT_WORDS } from '../../data/objections.js'
import { terminalCommand } from '../Repair-Loop/terminal.js'
import { liveOutputOf } from '../../data/run.js'
import { altitudes } from './altitude.js'
import { faceOf, switchFace } from '../../data/face.js'
import { isStep, stepLinker } from '../Step-View/link.js'
import { isExclusionMarker } from '../../data/paths.js'
import { href } from '../../routes.js'
import { editorHref } from './editor.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

/* ---------- presentation constants of the lane graph ---------- */
/* Lanes: x, width and the y of each row a node can take. Work and execution
 * nodes sit in their version's column (x = COL_X0 + COL_DX × column). */
const LANES = {
  request: { x: 18, w: 116, rows: [42, 90, 154] },
  work: { rows: [42, 108] },
  execution: { rows: [200, 294] },
  claims: { x: 840, w: 192, rows: [42, 90, 155] },
}
const COL_X0 = 160, COL_DX = 168, BAND_X0 = 156, BAND_W = 164, BAND_END = 816, NODE_W = 156
/* The most version columns the map draws, and nodes per lane per column. */
const MAX_COLS = 3, SLOTS = 2
/* Where an edge leaves or meets a node, measured from the node's top, and the
 * height of the nodes vertical edges reach up to — the canvas's drawn boxes. */
const PORT_Y = { request: 56, agent_said: 28, edit: 30, statement: 29, criteria: 21 }
const BOTTOM = { edit: 59, agent_said: 57, more: 59 }
/* Two anchors the canvas placed by hand for its own demo records. Any other
 * edge is routed by the rule (a vertical edge at the node's centre; a
 * statement's edge lands 112 px into the run). */
const HAND = { 'run-26>said-32': { x: 120 }, 'run-62': { inX: 124 } }
const EDGE_STYLE = {
  recorded_order: { stroke: '#2A2A30', sw: '1', dash: undefined },
  documented_reference: { stroke: '#66666F', sw: '1', dash: undefined },
  inferred: { stroke: '#66666F', sw: '1', dash: '4 3' },
}
/* The version ruler's band widths, as the canvas weighted them. */
const RULER_GROW = { moved: [0.9, 1.1, 1.5, 1.5], ready: [0.9, 1.1, 2.4], early: [0.9, 1.1, 2] }

/* How each claim kind is spoken of. */
const CLAIM = {
  tests_pass: { past: 'The tests passed', noun: 'a test claim', proven: 'tests pass', establish: 'these tests pass' },
  builds: { past: 'The run it rests on passed', noun: 'a build claim', proven: 'it builds', establish: 'this run passes' },
  task_complete: { noun: 'a completion claim' },
  done: { noun: 'a completion claim' },
  fixed: { noun: 'a claim that something is fixed' },
  other: { noun: 'a claim' },
}
const STAGE_NAMES = { define: 'Define', prepare: 'Prepare', build: 'Build', verify: 'Verify', review: 'Review', approve: 'Approve' }
const STAGE_GLYPH = { done: '✓', current: '●', not_started: '○', not_reached: '—' }
const COUNT_WORD = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine']
const CORAL = '#FF6369', CORAL_TEXT = '#FF8A8F'

/* The Change's state pill (L1) and its six stages, as the header and the status bar draw them.
 * Module-level so the Change with nothing said draws the same frame (`nothingSaid`). */
function pillLook(state, ver) {
  const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', YEL = '#F2C94C', GRN = 'var(--tl-proven, #3DD68C)', CTRL = '#66666F';
  const HATCH = 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)';
  const PILLS = {
    ready: { glyph: '◇', glyphColor: T1, word: 'Ready for approval', verTxt: '· ' + ver, border: '1px solid ' + T1, color: T1, bgImage: 'none' },
    needs_recheck: { glyph: '◭', glyphColor: YEL, word: 'Needs a re-check', verTxt: '· version ' + ver, border: '1px solid rgba(242,201,76,.6)', color: YEL, bgImage: HATCH },
    proven: { glyph: '●', glyphColor: GRN, word: 'Proven', verTxt: '· version ' + ver, border: '1px solid rgba(var(--tl-proven-rgb, 61, 214, 140), .6)', color: T1, bgImage: 'none' },
    contradicted: { glyph: '✕', glyphColor: CORAL, word: 'Not proven', verTxt: '· version ' + ver, border: '1px solid rgba(255,99,105,.7)', color: CORAL_TEXT, bgImage: 'none' },
    recorded: { glyph: '▪', glyphColor: T2, word: 'Recorded', verTxt: '· version ' + ver, border: '1px solid ' + CTRL, color: T1, bgImage: 'none' },
    not_proven: { glyph: '○', glyphColor: T2, word: 'Not proven', verTxt: '· version ' + ver, border: '1px dashed ' + T3, color: T1, bgImage: 'none' },
  };
  return PILLS[state] || PILLS.not_proven;
}

/* The Brief of a Change with no statement: the top bar, the state pill and stages, a sentence
 * saying what the record holds, the links to read it, and the status bar. Every value is the
 * snapshot's own; none is the canvas's. */
function nothingSaid(R, face) {
  const T1 = '#EDEDEF', T2 = '#A1A1AA';
  const ver = R.currentVersion;
  const curV = (R.versions || []).find((x) => x.id === ver) || {};
  const pill = pillLook(R.change.state, ver);
  const plural = (k, one, many) => fmtCount(k) + ' ' + (k === 1 ? one : many);
  const count = (k) => (Array.isArray(R[k]) ? R[k].length : 0);
  const agents = Array.from(new Set((R.sessions || []).map((x) => x.agent).filter(Boolean)));
  const title = String(R.change.title == null ? '' : R.change.title).replace(/\s+/g, ' ').trim();
  const steps = R.change.stepCount != null ? ' · ' + plural(R.change.stepCount, 'step', 'steps') : '';
  return {
    isDemo: false,
    empty: {
      headline: 'Nothing the agent said in this work claims it is finished, so there is nothing to check yet.',
      sub: 'The record holds ' + plural(count('edits'), 'edit', 'edits') + ' and ' + plural(count('runs'), 'command', 'commands') + '. Read the whole story to see them.',
    },
    projectName: R.project.name,
    changeTitleFull: R.change.title,
    changeTitle: title.length <= 46 ? title : title.slice(0, 45).trimEnd() + '…',
    changeMeta: 'Change · ' + plural((R.sessions || []).length, 'session', 'sessions') + (agents.length ? ' · ' + agents.join(', ') : ''),
    ver,
    verNote: curV.unsavedChanges ? '· includes unsaved changes' : (curV.commit ? '= commit ' + curV.commit : ''),
    verNoteLong: curV.unsavedChanges ? ver + ' includes unsaved changes'
      : ver + (curV.commit ? ' = commit ' + curV.commit : '') + (curV.unsavedChanges === false ? ' · no unsaved changes' : ''),
    readTime: R.view.readAt,
    readDate: R.view.readDate + ' · local time',
    alts: altitudes('brief').list,
    toggleTech: face.toggleTech,
    techPressed: face.tech ? 'true' : 'false',
    plainColor: face.tech ? T2 : T1,
    techColor: face.tech ? T1 : T2,
    pill,
    stages: stageMarks(R.change.stages),
    storageLine: storageSentence(R.machine && R.machine.recordsBytes),
    statusGlyph: pill.glyph,
    statusGlyphColor: pill.glyphColor,
    statusLeft: pill.word + ' · version ' + ver + ' · ' + plural(0, 'statement', 'statements') + steps + (R.change.recording ? ' · recording' : ''),
  };
}

function stageMarks(list) {
  const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C';
  return (list || []).map((sg, i, all) => {
    const n = STAGE_NAMES[sg.id] || sg.id; const g = STAGE_GLYPH[sg.status] || '○'; const cur = g === '●';
    const tip = n + ': ' + (g === '✓' ? 'done' : (cur ? 'where this Change is now' : (g === '○' ? 'not started' : 'not reached yet')));
    return { label: n, glyph: g, tip: tip, glyphColor: cur ? T1 : (g === '—' ? T3 : T2), labelColor: cur ? T1 : T2, weight: cur ? '500' : '400', cur: cur ? 'step' : 'false', conn: i < all.length - 1 ? 'inline-block' : 'none' };
  });
}

export default class Component extends DCLogic {
  state = {
    demo: 's1', frag: null, tech: false,
    open: { why: false, then: false, kind: false, after: false, tech: false },
    told: false, copied: false, more: false, noteOpen: false, objectOpen: false, walk: 0, zoom: 1, panX: 0,
    showHeld: false, pinned: '', sheet: false
  };

  renderVals() {
    const s = this.state;
    const set = (p) => this.setState(p);
    const noop = () => {};
    /* C30 R-PREFS: the face is the stored one on a live screen; the gallery keeps the canvas's own toggle */
    const techOn = faceOf(this.props, s.tech ? 'technical' : 'plain') === 'technical';
    const toggleTech = () => switchFace(this.props, techOn ? 'plain' : 'technical', (f) => set({ tech: f === 'technical' }));
    const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', OR = '#FF7A29', YEL = '#F2C94C', GRN = 'var(--tl-proven, #3DD68C)', LINE = '#2A2A30', CTRL = '#66666F';
    const GLOW = '0 0 6px rgba(255,154,82,.6)';
    const MONO = "'Geist Mono', monospace", SANS = "'Geist', sans-serif";

    /* ---------- the data ---------- */
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    const R = isDemo && D.states ? (D.states[s.demo] || D) : D;
    /* C30 R-ROUTES: a receipt chip that names a step opens that step; one that names none (a check ThruLine ran) is text */
    const stepTo = isDemo ? () => undefined : stepLinker(this.props.at, R.project && R.project.id);
    const stepKey = (key) => (key == null || key === 'null' ? null : Number(key));
    const acts = this.props.actions || {};
    /* "Told the agent" (C30 R-LEDGER): live, the record's own last act on this thread (a copy, a pasted
       mark, or a mark taken back; data/told.js) and what a refused write said. The gallery keeps the
       canvas's local toggle. Nothing is sent. */
    const toldRec = !isDemo && R.change && R.change.told ? R.change.told : null;
    const toldW = (!isDemo && acts.told) || {};
    const toldShown = isDemo ? s.told : !!(toldRec && toldRec.standing);
    /* A Change whose agent said nothing that claims it is finished: the frame, and an honest empty
     * state with its links; no statement is made up to fill the chain (census Change-Brief defect) */
    if (!isDemo && !(R.statements || []).length) return nothingSaid(R, { tech: techOn, toggleTech });

    const byId = (list, id) => (list || []).find((x) => x.id === id) || null;
    const runOf = (id) => byId(R.runs, id);
    const editOf = (id) => byId(R.edits, id);
    const versionOf = (id) => byId(R.versions, id);
    /* The edit that made a version: the last edit in it (before a step, when
     * one is given). The canvas's versions each have exactly one. */
    const editMaking = (vid, before) => {
      let hit = null;
      for (const e of R.edits || []) if (e.makesVersion === vid && (before == null || e.step < before)) hit = e;
      return hit;
    };
    const verIdx = (id) => (R.versions || []).findIndex((x) => x.id === id);
    const plural = (n, one, many) => fmtCount(n) + ' ' + (n === 1 ? one : many);
    const failing = (r) => !!r && (r.failed > 0 || r.outcome === 'failed');
    const tally = (r) => {
      if (!r) return '';
      if (r.passed == null && r.failed == null) {
        if (r.status === 'no_result' || r.outcome === 'unknown') return 'no result yet';
        if (r.outcome === 'failed') return 'failed' + (r.exitCode != null ? ' · exit ' + r.exitCode : ' · no count');
        if (r.outcome === 'succeeded') return 'succeeded · no count';
        return 'no result yet';
      }
      return r.failed > 0 ? fmtCount(r.failed) + ' failed, ' + fmtCount(r.passed) + ' passed' : fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed';
    };
    const secs = (ms) => (ms / 1000).toFixed(2) + ' s';
    /* C30 R-RUN: a check ThruLine ran from HQ is in its own record, not the agent's, so it has no
     * step (data/change.js sheetRunOf); it is named as what it is, never "step null" */
    const rstep = (r) => (r && r.step != null ? 'step ' + r.step : 'the check ThruLine ran');
    const Rstep = (r) => (r && r.step != null ? 'Step ' + r.step : 'The check ThruLine ran');
    const durNote = (r) => (!r ? '' : r.durationMs != null ? Rstep(r) + ' took ' + secs(r.durationMs) + '.' : Rstep(r) + '’s duration was not recorded.');
    const lastWord = (t) => { const w = String(t).replace(/[.!?…*`]+$/, '').split(/\s+/); return w[w.length - 1]; };
    const firstSentence = (t) => { const m = String(t).match(/^.*?[.!?](?=\s|$)/); return m && m[0].length < String(t).length ? m[0] + ' …' : String(t); };
    const q = (t) => '❝' + t + '❞';
    /* Long record text is cut for a node or a button on the live path only
     * (the whole text is in the inspector); the canvas's own text never is. */
    const clip = (t, n) => { const x = String(t == null ? '' : t).replace(/\s+/g, ' ').trim(); return isDemo || isExclusionMarker(x) || x.length <= n ? x : x.slice(0, n - 1).trimEnd() + '…'; };
    const tail = (p, n) => { const x = String(p == null ? '' : p); if (isDemo || isExclusionMarker(x) || x.length <= n) return x; const parts = x.split('/'); let out = parts.pop(); while (parts.length && (parts[parts.length - 1] + '/' + out).length <= n - 2) out = parts.pop() + '/' + out; return out.length + 2 > n ? '…' + x.slice(-(n - 1)) : '…/' + out; };
    /* Long unbroken tokens (paths, hashes) in a narrow node are cut from the left. */
    const soft = (t, n) => (isDemo || isExclusionMarker(t) ? String(t) : String(t).split(/(\s+)/).map((w) => (w.length > n ? '…' + w.slice(-(n - 1)) : w)).join(''));
    const base = (p) => { const x = String(p || ''); return x.slice(x.lastIndexOf('/') + 1); };

    const ver = R.currentVersion;
    const curV = versionOf(ver) || {};
    const rel = (vid) => { const n = verIdx(ver) - verIdx(vid); return n === 0 ? 'same version' : (n === 1 ? '1 version older' : fmtCount(n) + ' versions older'); };
    const crit = R.criteria || {};
    const proven = crit.provenOn || null;
    const provenRun = proven && proven.runId ? runOf(proven.runId) : null;
    const runningRow = (R.runs || []).find((r) => r.status === 'running' && r.version === ver) || null;
    /* live (C30 R-RUN): the run sheet serves what this Change's check has printed so far and its
     * elapsed time from the run's own recorded start (data/run.js liveOutputOf); they stand in for
     * the snapshot's only on that same run, matched by its run key */
    const served = !isDemo && runningRow ? liveOutputOf(R.runSheet) : null;
    const liveRun = served && served.run && runningRow.runKey === served.run ? served : null;
    const running = liveRun && liveRun.elapsedS != null ? Object.assign({}, runningRow, { elapsedS: liveRun.elapsedS }) : runningRow;
    const noResult = (R.runs || []).filter((r) => r.status === 'no_result');
    const firstNoResult = noResult[0] || {};
    const rc = R.recheck || { runner: '', scope: [] };
    const rcScopeList = rc.scope || [];
    const rcCmd = (rc.runner || '') + (rcScopeList.length ? ' ' + rcScopeList.join(' ') : '');
    /* The canvas's technical line names `--scope`, which `thruline check` does not have; the live
     * line is the command Repair-Loop copies, `--for-change <id> -- <runner>` (C30 P0-4). */
    const rcTech = isDemo
      ? (rcScopeList.length ? 'thruline check --scope ' + rcScopeList.join(' ') + ' -- ' + rc.runner : '')
      : (terminalCommand({ changeId: acts.changeId || null, runner: rc.runner, scope: rcScopeList }) || '');
    const rcScope = rcScopeList.join(' and ');
    const rcCmdShow = clip(rcCmd, 40);
    /* No control here may promise an act this build cannot do: on the live path
     * a check runs only when the screen was handed the act, and where the
     * server says it will not start one, its own reason is drawn (C30 R-RUN). */
    const checkOff = !isDemo && (!acts.check || !!acts.checkBlocked);
    const offWhy = !isDemo && acts.checkBlocked ? acts.checkBlocked.reason + ' ' + acts.checkBlocked.enabler : 'This check cannot start from this screen.';
    const askNote = !isDemo && acts.refusal ? ' · ' + acts.refusal
      : checkOff ? ' · ' + offWhy
        : (isDemo ? rc.permission === 'ask_once_per_project' : !!acts.needsPermission) ? ' · asks once for this project' : '';
    const lastEdit = (R.edits || []).filter((e) => e.time).slice(-1)[0] || {};

    /* The statement the brief is about (the header's chain strip), and the one
     * selected in the inspector. Statements of one message keep the order they
     * were said in. */
    const stmts = R.statements || [];
    const B = byId(stmts, R.brief && R.brief.statementId) || stmts[0];
    /* the statement the address names (`?statement=`, Every statement's "Open this in the full
     * statement view"), until the person picks another here; one the record does not hold is not
     * guessed at, and the brief's own statement is shown */
    const asked = !isDemo && this.props.at && this.props.at.statement ? String(this.props.at.statement) : null;
    const want = s.frag || asked;
    const selId = want && byId(stmts, want) ? want : B.id;
    const S = byId(stmts, selId);
    const msg = byId(R.messages, B.messageId) || { text: '' };
    const inMsgAll = stmts.filter((x) => x.messageId === B.messageId).sort((a, b) => msg.text.indexOf(a.text) - msg.text.indexOf(b.text));
    const inMsg = isDemo ? inMsgAll : capKeeping(inMsgAll, 3, [B.id, selId]);
    /* A statement's words for display inside ❝ ❞: markdown markers stripped (`plain`), never altered otherwise. */
    const pl = (st) => (st.plain != null ? st.plain : st.text);
    const word = (st) => lastWord(pl(st));

    /* ---------- one statement's chain, as facts ---------- */
    const CIRC = ['①', '②', '③', '④', '⑤', '⑥'];
    const factsOf = (st) => {
      const P = st.proof || {};
      const run = P.restsOn && P.restsOn.runId ? runOf(P.restsOn.runId) : null;
      const ranOn = P.ranOn ? P.ranOn.version : null;
      const maker = ranOn ? editMaking(ranOn, run ? run.step : null) : null;
      const since = P.since ? editOf(P.since.editId) : null;
      const rs = (P.recheck && P.recheck.status) || 'none';
      const recheckRun = P.recheck && P.recheck.runId ? runOf(P.recheck.runId) : null;
      const failedRun = rs === 'failed' ? recheckRun : null;
      const verdict = P.verdict;
      const L = [];
      if (!st.verifier) {
        L.push('hold', crit.accepted ? 'hold' : 'nothing', 'nr', 'nr', 'nr', 'recorded');
      } else {
        /* A run with no result is dotted and unlit and never counts as a link. */
        const held = run && run.status === 'finished';
        L.push('hold');
        L.push(held ? 'hold' : 'nothing');
        L.push(held && ranOn ? 'hold' : 'nr');
        L.push(!held ? 'nr' : since ? 'moved' : 'hold');
        L.push(!held ? 'nr' : failedRun ? 'failed' : (verdict !== 'proven' && (since || ranOn !== ver)) ? (rs === 'running' ? 'checking' : 'nothing') : 'hold');
        L.push(verdict === 'proven' ? 'proven' : verdict === 'contradicted' ? 'contradicted' : 'notproven');
      }
      const stopAt = L.findIndex((m) => m === 'nothing' || m === 'checking');
      return { st, P, run, ranOn, maker, since, sinceFile: since ? tail(since.file, 40) : '', sinceFileShort: since ? (isDemo ? since.file : clip(base(since.file), 18)) : '', rs, failedRun, verdict, L, stopAt, basis: P.restsOn ? P.restsOn.basis : null };
    };
    const FB = factsOf(B);
    const FS = factsOf(S);
    const P = B.proof;
    const restRun = FB.run;
    const ranOn = FB.ranOn;
    const ranMaker = FB.maker;
    const sinceEdit = FB.since;
    const sinceStep = sinceEdit ? sinceEdit.step : null;
    const sinceFile = FB.sinceFile;

    /* Which situation the Change is in: the canvas's four demo states, read
     * from the record instead of from the switcher. */
    const ST = R.change.state;
    const isReady = ST === 'ready';
    const isChk = !isReady && !!running;
    const isMoved = ST === 'needs_recheck' && !isChk;
    const isS1 = !isReady && !isMoved && !isChk;
    const demo = s.demo;
    const early = isS1 || isChk;
    const fixed = !S.verifier;
    const sProven = !fixed && FS.verdict === 'proven';
    const sContra = !fixed && FS.verdict === 'contradicted';
    const readTime = R.view.readAt;
    const readDate = R.view.readDate + ' · local time';
    const verNote = curV.unsavedChanges ? '· includes unsaved changes' : (curV.commit ? '= commit ' + curV.commit : '');
    const verNoteLong = curV.unsavedChanges
      ? ver + ' includes unsaved changes' + (isDemo && curV.illustrative ? ' (illustrative)' : '')
      : ver + (curV.commit ? ' = commit ' + curV.commit : '') + (curV.unsavedChanges === false ? ' · no unsaved changes' : '');

    /* ---------- names in the frame ---------- */
    const projectName = R.project.name;
    const changeTitleFull = R.change.title;
    const changeTitle = clip(R.change.title, 46);
    const agents = Array.from(new Set((R.sessions || []).map((x) => x.agent).filter(Boolean)));
    const changeMeta = 'Change · ' + plural((R.sessions || []).length, 'session', 'sessions') + (agents.length ? ' · ' + agents.join(', ') : '');
    const saidBy = (B.agent || 'The agent') + ' said';
    const saidAt = 'at ' + B.time + ' · step ' + B.step;
    const priorNote = (st) => 'The last version ThruLine recorded before this was said is ' + st.versionBefore + '. ThruLine doesn’t record the code at the moment it spoke.';
    const briefPrior = priorNote(B);
    const selPrior = priorNote(S);
    const chainAria = 'Proof chain for the statement ' + pl(B).replace(/[.!?]+$/, '') + ', 6 links';
    const sourceLine = (B.agent || 'The agent') + ' · step ' + B.step + ' · ' + B.time + ' · one message, ' + (COUNT_WORD[inMsgAll.length] || fmtCount(inMsgAll.length)) + (inMsgAll.length === 1 ? ' statement' : ' statements');
    const bytes = R.machine && R.machine.recordsBytes;
    const gb = bytes != null ? bytes / 1e9 : null;
    const storage = gb == null ? '' : (gb >= 1 ? gb.toFixed(1) + ' GB' : Math.round(bytes / 1e6) + ' MB');
    const storageLine = storageSentence(bytes);

    /* ---------- top bar ---------- */
    /* C30 R-PREFS: the altitudes are links to the open Change's three views (Brief → brief,
     * Explore → statements, Workbench → changed); this screen is the Brief, so Brief is marked */
    const alts = altitudes('brief').list;
    const techPressed = techOn ? 'true' : 'false';
    const plainColor = techOn ? T2 : T1;
    const techColor = techOn ? T1 : T2;

    /* ---------- actions ---------- */
    const pa = R.primaryAction;
    const runCheck = isDemo
      ? () => { if (isMoved) { set({ sheet: true }); } else { set({ demo: 'checking', walk: 0, sheet: false }); } }
      : () => (acts.check ? acts.check() : undefined);
    const stopCheck = isDemo ? () => set({ demo: 's1', walk: 0 }) : () => (acts.stopCheck ? acts.stopCheck() : undefined);
    const tell = () => {
      if (isDemo) { set({ told: !s.told }); return; }
      /* live: every press copies the message and keeps that it was copied (POST …/told-agent, free,
         nothing sent); the label and the "Told the agent" line then read the record, not this page */
      const message = tellMessage();
      if (typeof navigator !== 'undefined' && navigator.clipboard) {
        try { navigator.clipboard.writeText(message); } catch (e) { /* copying is best effort; the line below says what was kept */ }
      }
      if (acts.tellAgent) acts.tellAgent(message);
    };
    const tellLabel = toldShown ? 'Copied — not sent' : 'Tell the agent';
    const checkLabel = pa === 'check_what_changed' ? 'Check what changed' : 'Check it again';
    /* The orange check control, drawn as the design draws a control that is
     * off (dashed, t2) when no check can run from here, with its reason. */
    const chk = checkOff
      ? { bg: 'transparent', color: T2, border: '1px dashed ' + CTRL, cursor: 'not-allowed', off: 'true', why: offWhy }
      : { bg: OR, color: '#000000', border: '0', cursor: 'pointer', off: undefined, why: undefined };
    const chkGhost = checkOff
      ? { color: T2, border: '1px dashed ' + CTRL, cursor: 'not-allowed', off: 'true', why: chk.why }
      : { color: T1, border: '1px solid ' + CTRL, cursor: 'pointer', off: undefined, why: undefined };
    const checkTitle = checkOff ? chk.why
      : isDemo ? 'Opens the check sheet · asks once for this project'
        : acts.inPlace ? 'Runs this check here, on ' + ver + (acts.needsPermission ? ' · asks once for this project' : '')
          : 'Opens the check sheet';

    /* ---------- header: L1 pill, stages, headline ---------- */
    const pill = pillLook(ST, ver);
    const stages = stageMarks(R.change.stages);

    const claimOf = (st) => CLAIM[st.claimKind] || {};
    const claimB = claimOf(B);
    const lineCount = crit.lineCount || 0;
    const provenCount = proven ? proven.count : 0;
    /* The one sentence about a statement's chain: the headline for the brief's
     * statement and the inspector's line for the selected one. */
    const verdictSentence = (F) => {
      const st = F.st; const c = claimOf(st);
      if (!st.verifier) return (st.agent || 'The agent') + ' said “' + pl(st) + '”. The record can’t check that by itself; what counts as done decides it.';
      if (F.verdict === 'contradicted') {
        const r = F.failedRun;
        return r ? 'A later test run failed: ' + rstep(r) + ', ' + tally(r) + ', on ' + r.version + '.' : 'A later test run failed.';
      }
      if (!F.run) return 'Claimed, and no proof found in the record: no test run came before it.';
      if (F.verdict === 'proven') return (c.past || 'The run it rests on passed') + ' on ' + F.ranOn + ', and nothing changed after that.';
      if (F.run.status !== 'finished') return 'The run it rests on, ' + rstep(F.run) + ', has no result in the record.';
      if (F.since) return (c.past || 'The run it rests on passed') + ' on ' + F.ranOn + '. ' + F.sinceFile + ' changed after that, and nothing has run on ' + ver + (F.ranOn === ver ? ' since.' : '.');
      return (c.past || 'The run it rests on passed') + ' on ' + F.ranOn + '. Nothing has run on ' + ver + ' since.';
    };
    const notProvenSentence = verdictSentence(FB);
    const headline = isReady ? 'Ready for approval for version ' + ver + '. It rests on:'
      : isMoved ? (sinceEdit ? 'The code moved while you were looking: ' + sinceFile + ' changed at step ' + sinceStep + '.' : 'The code moved while you were looking: it is now ' + ver + '.')
        : notProvenSentence;
    const hasHeadSub = isReady || (isMoved && crit.accepted);
    const headSub = isReady
      ? fmtCount(provenCount) + ' of ' + fmtCount(lineCount) + ' things that count as done, each with a run on ' + ver + (provenRun ? ' — ' + rstep(provenRun) + ', ' + provenRun.time + ', ' + tally(provenRun) : '') + '.'
      : fmtCount(lineCount - provenCount) + ' of ' + fmtCount(lineCount) + ' lines now stop at ⑤ on ' + ver + '.';
    const headSubLive = isMoved ? 'assertive' : 'polite';

    /* The statements of the message, in the order they were said. */
    /* Live, the statements of the message share ONE line, as the canvas draws them, so the stand-still
     * region (424px) holds everything to its last line: the line's room is the main pane's width less
     * "<agent> said" and "at HH:MM · step N" (15px text ≈ 7.4px a character); the statement in front of
     * the person gets what the others leave, never under 24 characters. Each is whole in the inspector. */
    const paneW = isDemo ? null : Math.max(700, ((typeof window !== 'undefined' && window.innerWidth) || 1512) - 460 - 48);
    const lineChars = paneW ? Math.max(40, Math.floor((paneW - 280) / 7.4) - 2 * inMsg.length) : null;
    const otherChars = paneW && inMsg.length > 1 ? Math.max(12, Math.min(40, Math.floor(lineChars / (inMsg.length + 1)))) : null;
    const selChars = paneW ? Math.max(24, Math.min(90, lineChars - (otherChars || 0) * (inMsg.length - 1))) : 90;
    const hBtn = (st) => {
      const sel = st.id === selId;
      const offDeco = st.verifier ? (st.proof.verdict === 'proven' ? 'underline solid ' + T1 : 'underline dashed ' + T2) : 'underline dotted ' + T2;
      return { text: clip(pl(st), sel || !otherChars ? selChars : otherChars), pressed: sel ? 'true' : 'false', deco: sel ? 'underline solid ' + OR : offDeco, th: sel ? '2px' : '1px', pick: () => set({ frag: st.id, walk: 0 }) };
    };
    const saidStmts = inMsg.map(hBtn);

    /* ---------- header: the chain strip ---------- */
    const EYE = ['① SAID', '② RESTS ON', '③ RAN ON', '④ SINCE', '⑤ RE-CHECKED', '⑥ SO'];
    const NO_B = '0px solid transparent';
    const DISC = {
      hold: (i) => ({ discText: String(i + 1), discBg: T1, discColor: '#000000', discBorder: NO_B, discTop: 'transparent', discFs: '11px', discAnim: 'none' }),
      moved: () => ({ discText: '◭', discBg: 'transparent', discColor: YEL, discBorder: NO_B, discTop: 'transparent', discFs: '15px', discAnim: 'none' }),
      nothing: () => ({ discText: '', discBg: 'transparent', discColor: T2, discBorder: '1.5px dashed ' + T3, discTop: T3, discFs: '11px', discAnim: 'none' }),
      checking: () => ({ discText: '', discBg: 'transparent', discColor: T2, discBorder: '2px solid ' + LINE, discTop: T2, discFs: '11px', discAnim: 'tl-spin 1.1s linear infinite' }),
      notproven: () => ({ discText: '○', discBg: 'transparent', discColor: T2, discBorder: NO_B, discTop: 'transparent', discFs: '16px', discAnim: 'none' }),
      proven: () => ({ discText: '●', discBg: 'transparent', discColor: GRN, discBorder: NO_B, discTop: 'transparent', discFs: '16px', discAnim: 'none' }),
      nr: (i) => ({ discText: String(i + 1), discBg: 'transparent', discColor: T2, discBorder: '1px solid ' + LINE, discTop: LINE, discFs: '11px', discAnim: 'none' }),
      failed: () => ({ discText: '✕', discBg: 'transparent', discColor: T2, discBorder: NO_B, discTop: 'transparent', discFs: '13px', discAnim: 'none' }),
      contradicted: () => ({ discText: '✕', discBg: 'transparent', discColor: CORAL, discBorder: NO_B, discTop: 'transparent', discFs: '15px', discAnim: 'none' }),
      recorded: () => ({ discText: '▪', discBg: 'transparent', discColor: T2, discBorder: NO_B, discTop: 'transparent', discFs: '15px', discAnim: 'none' })
    };
    const SEG = {
      lit: { segBg: OR, segH: '2px', segShadow: GLOW },
      moved: { segBg: 'repeating-linear-gradient(45deg, #F2C94C 0 2px, transparent 2px 5px)', segH: '4px', segShadow: 'none' },
      dotted: { segBg: 'repeating-linear-gradient(90deg, #66666F 0 2px, transparent 2px 6px)', segH: '1px', segShadow: 'none' },
      none: { segBg: 'transparent', segH: '1px', segShadow: 'none' }
    };
    const col = (i, d, sg, f1, f2, words, extra) => Object.assign({
      eyebrow: EYE[i], capStartDisp: 'none', capEndDisp: 'none', capColor: YEL, padL: '0px',
      f1: f1, f2: f2, f1Font: MONO, f1Weight: '400', f1Color: T1,
      aria: 'Link ' + (i + 1) + ', ' + EYE[i].slice(2).toLowerCase() + ': ' + f1 + (f2 ? ', ' + f2 : '') + '. ' + words,
      hasQ: false, q: '', f2Color: T2,
      walkBg: 'transparent', walkRing: 'none'
    }, DISC[d](i), SEG[sg], extra || {});
    const stopA = { capEndDisp: 'inline-block' };
    const stopB = { capStartDisp: 'inline-block', padL: '12px' };
    const verdictFont = { f1Font: SANS, f1Weight: '550' };
    /* ① names the statement this strip is for, so the header never leaves the reader guessing which fragment the chain belongs to. */
    // live, the ① column's quote stays on one line: a sixth of the pane less its padding (12px ≈ 6.2px a character)
    const SAID_Q = { hasQ: true, q: clip(pl(B), paneW ? Math.max(12, Math.min(34, Math.floor((paneW / 6 - 10) / 6.2) - 2)) : 34), f2Color: T1 };
    const saidF1 = 'step ' + B.step + ' · ' + B.time;
    const saidWords = (B.agent || 'The agent') + ' wrote “' + pl(B) + '” Holds.';
    const BASIS_WORDS = { matched: 'Holds. Matched by ThruLine.', cited_by_agent: 'Holds. Cited by the agent.', latest_before: 'Holds. The last test run before it.', later_run: 'Holds. A later run on this version.' };
    const scopeList = (P.scope || []).join(', ');

    /* Each link of the brief's chain, from its own mark. */
    const stripCol = (i, m) => {
      const F = FB; const cap = i === F.stopAt - 1 ? stopA : (i === F.stopAt ? stopB : {});
      if (i === 0) return col(0, 'hold', 'lit', saidF1, '', saidWords, Object.assign({}, SAID_Q, cap));
      if (i === 1) {
        if (!B.verifier) return m === 'hold'
          ? col(1, 'hold', 'lit', 'what counts as done', fmtCount(provenCount) + ' of ' + fmtCount(lineCount) + ' proven', 'Holds.', cap)
          : col(1, 'nothing', 'dotted', 'what counts as done', 'not accepted yet', 'Proof stops here: what counts as done is not accepted yet.', cap);
        if (m === 'hold') return col(1, 'hold', 'lit', rstep(F.run), tally(F.run), BASIS_WORDS[F.basis] || 'Holds.', cap);
        if (F.run) return col(1, 'nothing', 'dotted', rstep(F.run), 'No result yet ⋯', 'Proof stops here: the run it rests on has no result.', cap);
        return col(1, 'nothing', 'dotted', 'nothing recorded', 'no test run before it', 'Proof stops here: no test run came before this statement.', cap);
      }
      if (m === 'nr') return col(i, 'nr', 'dotted', 'not reached', '', 'Not reached.', cap);
      if (i === 2) return col(2, 'hold', 'lit', F.ranOn, rel(F.ranOn) === 'same version' ? 'same version as now' : rel(F.ranOn), 'Holds.', cap);
      if (i === 3) {
        if (m === 'moved') return col(3, 'moved', 'moved', F.sinceFileShort, 'step ' + F.since.step + ', after the run', 'Moved at ' + F.since.time + '. Proof stops after this link.', Object.assign({}, stopA, cap));
        return col(3, 'hold', 'lit', 'still applies', 'nothing changed since', 'Holds.', cap);
      }
      if (i === 4) {
        if (m === 'hold') return col(4, 'hold', 'lit', 'not needed', 'ran on ' + F.ranOn, 'Holds.', cap);
        if (m === 'checking') return col(4, 'checking', 'dotted', '◔ checking · ' + running.elapsedS + ' s', 'on ' + ver, 'Checking now. Nothing is settled until it finishes.', stopB);
        if (m === 'failed') return col(4, 'failed', 'none', rstep(F.failedRun) + ' failed', tally(F.failedRun), 'A later run failed.');
        return col(4, 'nothing', 'dotted', 'nothing yet', 'on ' + ver, F.since ? 'Proof stops here: nothing on ' + ver + ' has run with ' + F.sinceFile + ' in scope.' : 'Proof stops here: nothing has run on ' + ver + '.', stopB);
      }
      if (m === 'proven') return col(5, 'proven', 'none', 'Proven · ' + P.version, scopeList, 'The verdict: Proven on this version.', Object.assign({ f1Color: GRN }, verdictFont));
      if (m === 'contradicted') return col(5, 'contradicted', 'none', 'Not proven', 'on ' + ver, 'The verdict: a later run failed.', Object.assign({ f1Color: CORAL_TEXT }, verdictFont));
      if (m === 'recorded') return col(5, 'recorded', 'none', 'Recorded', 'on ' + ver, 'The verdict: recorded, settles nothing.', verdictFont);
      return col(5, 'notproven', 'none', 'Not proven', 'on ' + ver, 'The verdict.', verdictFont);
    };
    const chainCols = FB.L.map((m, i) => stripCol(i, m));
    const stopIdx = FB.verdict === 'proven' || FB.stopAt < 0 ? 6 : FB.stopAt + 1;
    if (s.walk > 0) {
      const w = Math.min(s.walk, stopIdx) - 1;
      chainCols[w].walkBg = 'rgba(255,122,41,.07)';
      chainCols[w].walkRing = 'inset 0 0 0 1px rgba(255,122,41,.55)';
    }
    const walkLabel = s.walk === 0 ? 'Walk me through it'
      : (s.walk >= stopIdx ? 'Last one: ' + CIRC[stopIdx - 1] + ' · the action' : 'Link ' + CIRC[s.walk - 1] + ' of ' + CIRC[stopIdx - 1]);
    const walkNext = () => { if (s.walk >= stopIdx) { set({ walk: 0 }); } else { set({ walk: s.walk + 1 }); } };

    /* The proof panel shows the Change's one primary action. */
    const pStop = pa === 'check_again', pChecking = pa === 'stop_check', pReady = pa === 'approve', pMoved = pa === 'check_what_changed';
    const stopEyebrow = FB.verdict === 'proven' && !isReady ? 'Holds on ' + ver + ' · nothing to settle'
      : FB.verdict === 'contradicted' ? 'A later run failed · ⑤'
        : 'Proof stops here · ' + (FB.stopAt >= 0 ? CIRC[FB.stopAt] : '⑤');
    /* The pointer drops from the middle of the ┤ ├ gap (between ④ and ⑤) straight down to the panel it docks.
       Strip geometry: 3 pad + 14 eyebrow + 4 + 18 disc row + 4 + 34 text + 3 pad = 80, then the 6px section gap;
       the caps end 37px down, so the line starts at 41 (45px above the panel). Any other chain: a short stub under ⑥. */
    const gapAt5 = FB.stopAt === 4 && !isReady;
    const pointerLeft = gapAt5 ? '257px' : '446px';
    const pointerTop = gapAt5 ? '-45px' : '-8px';
    const pointerH = gapAt5 ? '45px' : '8px';
    const pointerBg = !gapAt5 ? 'linear-gradient(#66666F, #66666F)'
      : (isChk ? 'repeating-linear-gradient(180deg, #82828C 0 3px, transparent 3px 6px)' : 'repeating-linear-gradient(180deg, rgba(242,201,76,.75) 0 3px, transparent 3px 6px)');
    /* "Looked at": the runs the verdict considered. */
    const lookedOf = (F) => {
      const looked = (F.P.lookedAt || []).map(runOf).filter(Boolean);
      const done = looked.filter((r) => r.status === 'finished');
      const openR = looked.filter((r) => r.status === 'no_result');
      const parts = [];
      if (done.length === 1) parts.push(rstep(done[0]) + ' on ' + done[0].version + ',');
      else if (done.length > 1) parts.push(fmtCount(done.length) + ' finished runs (' + done.map((r) => rstep(r) + ' on ' + r.version).join(', ') + ')');
      if (openR.length) parts.push(openR.map((r) => 'step ' + r.step).join(', ') + (openR.length === 1 ? ', which has no result yet.' : ', which have no result yet.'));
      if (!parts.length) return 'Looked at: no test run before it in the record.';
      let out = 'Looked at: ' + parts.join(' and ');
      if (!openR.length) out = out.replace(/,$/, '.');
      return out + (isMoved ? ' Nothing has run on ' + ver + '.' : '');
    };
    const lookedAt = lookedOf(FB);
    const lzReady = isReady, lzMoved = isMoved;
    const lzDefault = !isReady && !isMoved;
    const lzLead = crit.accepted ? 'Approval rests on what counts as done: ' + fmtCount(provenCount) + ' of ' + fmtCount(lineCount) + ' proven on ' + ver + '.' : 'Approval rests on what counts as done, which isn’t accepted yet.';
    const lzLink = crit.accepted ? 'See what counts as done' : 'Say what counts as done';
    const lzNote = crit.accepted ? 'Approval waits on ' + fmtCount(lineCount - provenCount) + ' more.' : 'Nothing counts as done yet, so nothing is ready.';

    /* Landing column and proof-panel sentences. */
    const approveNote = 'Approving records your decision on ' + ver + ' only. It does not merge, deploy or tell the agent anything.';
    const barSet = 'Bar set after the work: accepted ' + crit.acceptedAt + (lastEdit.time ? '; the agent’s last edit was ' + lastEdit.time + '.' : '.');
    const sl = R.view.sinceLook || { runs: 0, filesChanged: 0 };
    const sinceLookLine = R.view.previousLookAt ? 'Since you looked at ' + R.view.previousLookAt + ': ' + (sl.criteriaAccepted ? 'what counts as done accepted · ' : '') + plural(sl.runs, 'run', 'runs') + ' · ' + plural(sl.filesChanged, 'file', 'files') + ' changed' : '';
    const waitsOn = crit.accepted ? 'Approval of ' + ver + ' waits on all ' + fmtCount(lineCount) + '.' : 'Approval of ' + ver + ' waits on what counts as done.';
    const approveLabel = 'Approve ' + ver;
    const approveReadLabel = 'Approve ' + R.view.readVersion;
    const changedFiles = R.view.changedSinceLoad || [];
    const movedNote = 'Not approved. The code moved while this screen was open, so approving now would have approved something other than what is on your screen. '
      + (changedFiles.length ? (changedFiles.length === 1 ? 'One file changed' : (COUNT_WORD[changedFiles.length] || fmtCount(changedFiles.length)) + ' files changed') + ' since this screen loaded: '
        : 'ThruLine could not name the changed file from this record.');
    const changedList = changedFiles.join(', ');
    const checkingEyebrow = 'Checking now · ⑤ · ' + (running ? running.elapsedS : 0) + ' s';
    /* live (C30 R-RUN): the newest two lines the run sheet serves of what the check has printed,
     * under its command; served but empty says so; and where the sheet serves no output (a check
     * started from a terminal, or a server from before R-RUN) none is drawn as if it were
     * streaming, and the first line says what is shown */
    const liveOut = running && running.output && running.output.length ? running.output
      : liveRun && liveRun.lines ? ['$ ' + (running.command || 'the check')].concat(liveRun.lines.length ? liveRun.lines.slice(-2) : ['Nothing printed yet.'])
      : !isDemo && running ? ['ThruLine records what this check prints. It is not shown while the check runs.'] : [];
    const outLines = [0, 1, 2].map((i) => liveOut[i] || '');
    const outCursorLine = outLines[2] + ' ';
    const liveAria = isDemo ? 'Live output, illustrative' : 'Live output';
    const hasOpen = firstNoResult.step != null;
    const openLead = hasOpen ? 'Still open, and it doesn’t hold this up: step ' + firstNoResult.step + ' (' : 'Nothing else is waiting on a result.';
    const openCmd = hasOpen ? clip(firstNoResult.command, 40) : '';
    const openTail = hasOpen ? ') has no result yet. If it runs and changes a file, the version moves.' : '';

    /* ---------- map ---------- */
    const zoom = s.zoom;
    const mapTransform = 'translate(' + s.panX + 'px, 0px) scale(' + zoom + ')';
    const zoomLabel = zoom.toFixed(1) + '×';
    const zoomIn = () => set({ zoom: Math.min(1.4, Math.round((zoom + 0.1) * 10) / 10) });
    const zoomOut = () => set({ zoom: Math.max(0.7, Math.round((zoom - 0.1) * 10) / 10) });
    const zoomFit = () => set({ zoom: 1, panX: 0 });
    const panLeft = () => set({ panX: Math.min(320, s.panX + 80) });
    const panRight = () => set({ panX: Math.max(-320, s.panX - 80) });

    /* What the selected statement's proof lights. */
    const tl = !fixed;
    const SP = S.proof;
    /* The lit thread is orange only across links that hold: a run with no result lights nothing. */
    const selRun = !fixed && FS.run && FS.L[1] === 'hold' ? FS.run.id : null;
    const selMaker = selRun && FS.ranOn ? editMaking(FS.ranOn, FS.run.step) : null;
    const selSince = !fixed && FS.since ? FS.since.id : null;
    const litIds = new Set([selRun, selMaker && selMaker.id].filter(Boolean));
    const sinceIds = new Set(stmts.filter((x) => x.verifier && x.proof.since).map((x) => x.proof.since.editId));

    /* Which versions get a column. The canvas draws every version; a live
     * record can hold dozens, so the map draws the current one and the ones the
     * selected statement's thread passes through, filled with the latest, at
     * most MAX_COLS — never crammed. */
    let shownVersions = R.versions || [];
    if (!isDemo && shownVersions.length > MAX_COLS) {
      const need = new Set([ver, FS.ranOn, selMaker && selMaker.makesVersion, FS.since && FS.since.makesVersion].filter(Boolean));
      const pick = shownVersions.filter((v) => need.has(v.id));
      for (let i = shownVersions.length - 1; i >= 0 && pick.length < MAX_COLS; i--) if (!pick.includes(shownVersions[i])) pick.push(shownVersions[i]);
      shownVersions = shownVersions.filter((v) => pick.includes(v));
    }
    const colIdx = (id) => shownVersions.findIndex((x) => x.id === id);
    const hiddenVersions = (R.versions || []).length - shownVersions.length;

    /* Version columns: every shown version is a band; the current one runs to
     * the claims lane. */
    const band = (x, w, label, now) => ({ x: x + 'px', w: w + 'px', label: label, bg: now ? 'rgba(255,122,41,.05)' : 'rgba(255,255,255,.025)', color: now ? T1 : T3, ul: now ? '1px solid ' + OR : '0px solid transparent' });
    const colW = {};
    const bands = shownVersions.map((v, i) => {
      const x = BAND_X0 + COL_DX * i; const now = v.id === ver; const w = now ? BAND_END - x : BAND_W;
      colW[v.id] = Math.min(NODE_W, w - 8);
      const label = (i === 0 && hiddenVersions > 0 ? '+' + hiddenVersions + ' more · ' : '') + v.id;
      return band(x, w, now ? label + ' · now' : label, now);
    });

    /* Nodes: the record's graph, plus the two absence markers. */
    let gnodes = ((R.graph && R.graph.nodes) || []).slice();
    const laneTotal = (lane) => gnodes.filter((n) => n.lane === lane && n.kind !== 'nothing_recorded').length;
    const laneReq = fmtCount(laneTotal('request') + (R.change.recordedFrom ? 1 : 0));
    const laneWork = fmtCount(laneTotal('work'));
    const laneExec = fmtCount(laneTotal('execution'));
    const laneClaims = fmtCount(laneTotal('claims') + 1);
    if (!isDemo) gnodes = capGraph(gnodes);
    if (R.change.recordedFrom) gnodes.unshift({ id: 'before', lane: 'request', kind: 'before', step: null, version: null, label: null });
    const hasRunOnCur = gnodes.some((n) => n.lane === 'execution' && n.version === ver && n.kind === 'run');
    const settledOnCur = gnodes.some((n) => n.lane === 'execution' && n.version === ver && n.kind === 'run' && (runOf(n.ref) || {}).status === 'finished');
    if (!hasRunOnCur && colIdx(ver) >= 0) {
      const hole = { id: 'nothing-' + ver, lane: 'execution', kind: 'nothing_recorded', step: null, version: ver, label: null };
      const at = gnodes.findIndex((n) => n.lane === 'execution' && n.version === ver);
      if (at >= 0) gnodes.splice(at, 0, hole);
      else { let last = -1; gnodes.forEach((n, i) => { if (n.version === ver) last = i; }); gnodes.splice(last >= 0 ? last + 1 : gnodes.length, 0, hole); }
    }
    const box = {};
    const used = {};
    gnodes.forEach((n) => {
      const L = LANES[n.lane]; if (!L) return;
      const inCols = n.lane === 'work' || n.lane === 'execution';
      if (inCols && colIdx(n.version) < 0) return;
      const key = n.lane + '|' + (inCols ? n.version : '');
      const row0 = n.lane === 'request' && !R.change.recordedFrom ? 1 : 0;
      const row = (used[key] || row0); used[key] = row + 1;
      if (row >= L.rows.length) return;
      const c = colIdx(n.version);
      const x = L.x != null ? L.x : COL_X0 + COL_DX * c;
      const w = L.w != null ? L.w : (colW[n.version] || NODE_W);
      box[n.id] = { id: n.id, lane: n.lane, kind: n.kind, x: x, y: L.rows[row], w: w };
    });
    const laneReqLabel = 'Request · ' + laneReq;
    const laneClaimsLabel = 'Claims & evidence · ' + laneClaims;

    const node = (b, kind, step, title, o) => Object.assign({
      x: b.x + 'px', y: b.y + 'px', w: b.w + 'px', kind: kind, step: step, title: title,
      tf: SANS, ls: '0em', tc: T1, hasSub: false, sub: '', sf: SANS, sc: T2,
      bg: '#111113', border: '1px solid ' + LINE, shadow: 'none'
    }, o || {});
    const cmd = { tf: MONO, ls: '-0.03em' };
    const litB = { border: '1px solid rgba(255,122,41,.55)' };
    const movB = { border: '1px solid rgba(242,201,76,.6)' };
    const dash = { bg: 'transparent', border: '1px dashed ' + T3, tc: T2 };
    const stepTxt = (n) => (n.step != null ? 'step ' + n.step : '');
    const thread = (n) => (litIds.has(n.id) ? litB : (n.id === selSince ? movB : {}));

    const nodes = [];
    gnodes.forEach((n) => {
      const b = box[n.id]; if (!b) return;
      if (n.kind === 'before') nodes.push(node(b, 'Before ' + R.change.recordedFrom, '', 'Not recorded', { bg: 'transparent', border: '1px dashed ' + T3, tc: T2 }));
      else if (n.kind === 'session') nodes.push(node(b, 'Session', stepTxt(n), n.label + (n.time ? ' · ' + n.time : '')));
      else if (n.kind === 'request') nodes.push(node(b, 'Request', stepTxt(n), q(soft(clip(n.label, 70), 14))));
      else if (n.kind === 'agent_said') nodes.push(node(b, 'Agent said', stepTxt(n), q(firstSentence(n.label))));
      else if (n.kind === 'more') nodes.push(node(b, '+' + fmtCount(n.count) + ' more', '', n.label, { bg: '#0A0A0B', border: '1px dashed ' + T3, tc: T2 }));
      else if (n.kind === 'edit') {
        const e = editOf(n.ref) || {};
        nodes.push(node(b, (sinceIds.has(n.id) ? '◭ ' : '') + 'Edit', stepTxt(n), tail(n.label, 20), Object.assign({ hasSub: true, sub: 'makes ' + e.makesVersion, sf: MONO, sc: T3 }, cmd, tl ? thread(n) : {})));
      } else if (n.kind === 'run') {
        const r = runOf(n.ref) || {};
        if (r.status === 'running') nodes.push(node(b, '◔ Checking now', r.elapsedS + ' s', n.label, Object.assign({ hasSub: true, sub: 'nothing settled yet', sc: T2 }, cmd, { bg: 'transparent', border: '1px dashed ' + CTRL })));
        else nodes.push(node(b, 'Run', stepTxt(n), clip(n.label, 30), Object.assign({ hasSub: true, sub: failing(r) ? '✕ ' + tally(r) : tally(r), sc: failing(r) || r.passed == null ? T2 : T1 }, cmd, tl ? thread(n) : {})));
      } else if (n.kind === 'nothing_recorded') nodes.push(node(b, 'Nothing recorded', '', 'A run on ' + n.version + ' would appear here.', dash));
      else if (n.kind === 'command') nodes.push(node(b, '⋯ Command', stepTxt(n), clip(n.label, 30), Object.assign({ border: '1px dotted ' + T3 }, cmd, tl ? thread(n) : {})));
      else if (n.kind === 'criteria') nodes.push(node(b, 'What counts as done', '', crit.accepted ? 'Accepted ' + crit.acceptedAt + ' · ' + fmtCount(crit.lineCount) + ' lines · ' + crit.version : 'Not accepted yet', Object.assign({ tc: crit.accepted ? T1 : T2 }, (fixed && crit.accepted) ? litB : {})));
    });

    const selB = { border: '1px solid ' + OR, shadow: '0 0 0 1px rgba(255,122,41,.25), 0 0 18px rgba(255,122,41,.18)' };
    const plainB = { border: '1px solid ' + LINE, shadow: 'none' };
    const stmtSub = (st) => {
      if (!st.verifier) return '▪ Recorded';
      if (st.proof.verdict === 'proven') return '6 links hold · ' + st.proof.version;
      if (st.proof.verdict === 'contradicted') return '✕ Not proven · ' + st.proof.version;
      if (st.proof.recheck && st.proof.recheck.status === 'running') return '○ Not proven · checking now';
      return '○ Not proven · ' + st.proof.version;
    };
    const btnNodes = gnodes.filter((n) => n.kind === 'statement' && box[n.id]).map((n) => {
      const st = byId(stmts, n.ref) || {}; const b = box[n.id]; const sel = st.id === selId;
      return Object.assign({ x: b.x + 'px', y: b.y + 'px', w: b.w + 'px', kind: st.verifier ? 'Statement' : 'Agent said', step: stepTxt(n), title: q(soft(clip(pl(st), 30), 22)), sub: stmtSub(st), hint: 'Show this statement in the inspector', pressed: sel ? 'true' : 'false', pick: () => set({ frag: st.id, walk: 0 }) }, sel ? selB : plainB);
    });

    /* Edges of the record, routed by the lanes they join. */
    const route = (a, b, hand) => {
      if (a.lane === 'claims' && b.lane === 'execution') {
        const sy = a.y + PORT_Y.statement; const ex = b.x + ((HAND[b.id] && HAND[b.id].inX) || 112); const ly = b.y - 25;
        return 'M ' + a.x + ' ' + sy + ' C ' + (a.x - 40) + ' ' + sy + ', ' + (a.x - 40) + ' ' + ly + ', ' + (a.x - 70) + ' ' + ly + ' L ' + (ex + 30) + ' ' + ly + ' C ' + (ex + 10) + ' ' + ly + ', ' + ex + ' ' + (b.y - 15) + ', ' + ex + ' ' + b.y;
      }
      if (a.lane === 'execution' && b.lane === 'work') {
        const x = a.x + ((hand && hand.x != null) ? hand.x : a.w / 2);
        return 'M ' + x + ' ' + a.y + ' L ' + x + ' ' + (b.y + (BOTTOM[b.kind] || 0));
      }
      if (a.lane === b.lane && a.y === b.y) {
        const x1 = a.x + a.w - 16, x2 = b.x + 16, top = a.y - 12;
        return 'M ' + x1 + ' ' + a.y + ' C ' + x1 + ' ' + top + ', ' + x2 + ' ' + top + ', ' + x2 + ' ' + b.y;
      }
      const sx = a.x + a.w, sy = a.y + (PORT_Y[a.kind] || 0), ex = b.x, ey = b.y + (PORT_Y[b.kind] || 0), c = (ex - sx) / 2 + 1;
      return 'M ' + sx + ' ' + sy + ' C ' + (sx + c) + ' ' + sy + ', ' + (ex - c) + ' ' + ey + ', ' + ex + ' ' + ey;
    };
    /* A route is drawn only where its geometry reads: a vertical edge within
     * one column, a side or arc edge running left to right. */
    const routable = (a, b) => {
      if (a.lane === 'claims' && b.lane === 'execution') return true;
      if (a.lane === 'execution' && b.lane === 'work') return a.x === b.x;
      return b.x > a.x + a.w - 1;
    };
    const edgeList = ((R.graph && R.graph.edges) || []).slice();
    if (!isDemo) {
      /* recorded order between what the map draws: a run and the edit before it
       * in its column; the request and the first edit drawn after it. */
      const shownEdits = gnodes.filter((n) => n.kind === 'edit' && box[n.id]);
      for (const n of gnodes) {
        if (n.lane !== 'execution' || !box[n.id] || n.step == null) continue;
        const prev = shownEdits.filter((e) => e.version === n.version && e.step < n.step).pop();
        if (prev && !edgeList.some((x) => x.from === n.id && x.to === prev.id)) edgeList.push({ from: n.id, to: prev.id, kind: 'recorded_order' });
      }
      const reqN = gnodes.find((n) => n.kind === 'request' && box[n.id]);
      const firstE = shownEdits.find((e) => box[e.id].x === COL_X0);
      if (reqN && firstE) edgeList.push({ from: reqN.id, to: firstE.id, kind: 'recorded_order' });
    }
    const edges = edgeList.map((e) => {
      const a = box[e.from], b = box[e.to]; if (!a || !b || !routable(a, b)) return null;
      const st = EDGE_STYLE[e.kind] || EDGE_STYLE.inferred;
      return { d: route(a, b, HAND[e.from + '>' + e.to]), stroke: st.stroke, sw: st.sw, dash: st.dash };
    }).filter(Boolean);

    /* The lit thread: the selected statement's chain drawn over the map. */
    const P_ = (d, stroke, sw, dash) => ({ d: d, stroke: stroke, sw: sw, dash: dash, glow: stroke === OR });
    const threadPaths = [];
    const selBox = box[S.id];
    if (fixed && selBox && box.criteria) {
      const a = selBox, c = box.criteria, sy = a.y + PORT_Y.statement, ey = c.y + PORT_Y.criteria;
      if (!crit.accepted) {
        const x0 = a.x - 16;
        threadPaths.push(P_('M ' + a.x + ' ' + sy + ' C ' + (a.x - 12) + ' ' + sy + ', ' + x0 + ' ' + (sy - 5) + ', ' + x0 + ' ' + (sy - 11), OR, '2'));
        threadPaths.push(P_('M ' + (x0 - 7) + ' ' + (sy - 11) + ' L ' + (x0 + 7) + ' ' + (sy - 11), T2, '2'));
        threadPaths.push(P_('M ' + (x0 - 7) + ' ' + (ey + 21) + ' L ' + (x0 + 7) + ' ' + (ey + 21), T2, '2'));
        threadPaths.push(P_('M ' + x0 + ' ' + (ey + 21) + ' C ' + x0 + ' ' + (ey + 9) + ', ' + (a.x - 12) + ' ' + ey + ', ' + a.x + ' ' + ey, T3, '1.5', '3 3'));
      } else {
        threadPaths.push(P_('M ' + a.x + ' ' + sy + ' C ' + (a.x - 18) + ' ' + sy + ', ' + (a.x - 18) + ' ' + ey + ', ' + a.x + ' ' + ey, OR, '2'));
      }
    } else if (!fixed && selBox) {
      const sinceB = selSince ? box[selSince] : null;
      const makerB = selMaker ? box[selMaker.id] : null;
      const runB = selRun ? box[selRun] : null;
      if (sinceB && SP.stopsAt && !settledOnCur && colIdx(ver) === colIdx((FS.since || {}).makesVersion)) {
        const x = sinceB.x + sinceB.w / 2, y0 = sinceB.y + (BOTTOM[sinceB.kind] || 0), y1 = LANES.execution.rows[0];
        threadPaths.push(P_('M ' + x + ' ' + y0 + ' L ' + x + ' ' + (y0 + 27), T3, '1.5', '3 3'));
        threadPaths.push(P_('M ' + (x - 7) + ' ' + (y0 + 27) + ' L ' + (x + 7) + ' ' + (y0 + 27), YEL, '2'));
        threadPaths.push(P_('M ' + (x - 7) + ' ' + (y0 + 51) + ' L ' + (x + 7) + ' ' + (y0 + 51), YEL, '2'));
        threadPaths.push(P_('M ' + x + ' ' + (y0 + 51) + ' L ' + x + ' ' + y1, T3, '1.5', '3 3'));
      }
      if (sinceB && makerB && routable(makerB, sinceB)) threadPaths.push(P_(route(makerB, sinceB), YEL, '2', '4 3'));
      if (runB && makerB && routable(runB, makerB)) threadPaths.push(P_(route(runB, makerB, HAND[selRun + '>' + selMaker.id]), OR, '2'));
      if (runB) threadPaths.push(P_(route(selBox, runB), OR, '2'));
    }
    const hasThread = threadPaths.length > 0;

    /* Findings: what ThruLine noticed in the record of this Change. */
    const findText = (f) => {
      if (f.kind === 'edit_after_run') return 'Edit after the run · step ' + f.editStep;
      if (f.kind === 'no_check') return tail(f.file, 24) + ': no check';
      if (f.kind === 'agent_reported') return 'Agent-reported · ' + f.claimStep + ' vs ' + f.runStep;
      if (f.kind === 'run_outcomes') {
        if (!f.failedSteps.length) return 'Step ' + f.passedSteps.join(', ') + ' passed';
        if (!f.passedSteps.length) return 'Step ' + f.failedSteps.join(', ') + ' failed';
        return 'Step ' + f.failedSteps.join(', ') + ' failed, ' + f.passedSteps.join(', ') + ' passed';
      }
      if (f.kind === 'no_result') return 'Step ' + f.step + ': no result yet';
      if (f.kind === 'edit_covered') return 'Edit at ' + f.editStep + ' · covered by ' + f.runStep;
      if (f.kind === 'in_scope') return f.path + ' in scope at ' + f.runStep;
      return f.kind;
    };
    const findings = (R.findings || []).map((f) => ({ t: '· ' + findText(f), c: f.resolved ? T2 : T1 }));
    const findCount = fmtCount(findings.length);

    const demoDefs = [['s1', 'S1 · 16:04 Not proven'], ['checking', 'Checking now'], ['ready', 'K-ready · 16:09 proven'], ['moved', 'Code changed · a1b2c3d']];
    const demos = isDemo ? demoDefs.map((d) => ({
      label: d[1], pressed: demo === d[0] ? 'true' : 'false',
      bg: demo === d[0] ? '#1F1F23' : 'transparent', border: demo === d[0] ? CTRL : 'transparent', color: demo === d[0] ? T1 : T2,
      pick: () => set({ demo: d[0], walk: 0, told: false, copied: false, sheet: false, showHeld: false, pinned: '' })
    })) : [];

    /* ---------- inspector: header, fragments, the Note ---------- */
    const inspKind = 'Statement · said by the agent';
    const selPos = stmts.findIndex((x) => x.id === selId);
    const pagerLabel = fmtCount(selPos + 1) + ' of ' + fmtCount(stmts.length);
    const prevStmt = () => set({ frag: stmts[Math.max(0, selPos - 1)].id, walk: 0 });
    const nextStmt = () => set({ frag: stmts[Math.min(stmts.length - 1, selPos + 1)].id, walk: 0 });

    const selCard = { bg: '#111113', border: '1px solid ' + OR, shadow: '0 0 0 1px rgba(255,122,41,.2)' };
    const offCard = { bg: 'transparent', border: '1px solid ' + LINE, shadow: 'none' };
    const cardsOf = isDemo ? inMsgAll : capKeeping(inMsgAll, 4, [B.id, selId]);
    const fragCards = cardsOf.map((st) => {
      const sel = st.id === selId; const pv = st.verifier && st.proof.verdict === 'proven'; const cx = st.verifier && st.proof.verdict === 'contradicted';
      const look = !st.verifier ? { glyph: '▪', gc: T2, caption: 'Recorded · decided by what counts as done' }
        : cx ? { glyph: '✕', gc: CORAL, caption: 'Not proven · version ' + st.proof.version }
          : { glyph: pv ? '●' : '○', gc: pv ? T1 : T2, caption: pv ? '6 links hold · version ' + st.proof.version : 'Not proven · version ' + st.proof.version };
      return Object.assign({ quote: clip(pl(st), 70), glyph: look.glyph, gc: look.gc, caption: look.caption, pressed: sel ? 'true' : 'false', pick: () => set({ frag: st.id, walk: 0 }) }, sel ? selCard : offCard);
    });

    const sWord = fixed ? word(S) : '';
    const proposedLine = crit.proposedCount != null
      ? fmtCount(crit.proposedCount) + ' statements are proposed from your request' + (crit.proposedFromStep != null ? ' (step ' + crit.proposedFromStep + ').' : '.')
      : 'Nothing is proposed yet; you write what counts as done.';
    const tnS = SP && SP.thenAndNow;
    const addedScope = (tnS && tnS.added && tnS.added[0] && tnS.added[0].newScope || []).join(', ');
    const sRun = FS.run;
    const sCmd = !rcScopeList.length && sRun ? sRun.command : rcCmd;
    const sMaker = FS.maker;
    const acceptedRestsHere = crit.accepted && proven && sRun && proven.runId === sRun.id;
    const NOTE0 = { hasExtra: false, extra: '', hasEarlier: false, earlier: '', l3Btn: false, l3Act: noop, l3Glyph: '', l3Label: '', l3KeyDisp: 'none', l3Caption: '', l3Link: false, l3Text: false, l3TextVal: '', hasEngine: false, engine: '' };
    let note;
    if (fixed) {
      note = Object.assign({}, NOTE0, {
        glyph: '▪', gc: T2, word: 'Recorded', wc: T1, rest: '· version ' + ver,
        l2: !crit.accepted ? 'The record can’t check “' + sWord + '” by itself. What counts as done isn’t accepted yet.'
          : (provenCount === lineCount ? 'The record can’t check “' + sWord + '” by itself. All ' + fmtCount(lineCount) + ' things you accepted as done are proven on ' + ver + '; your approval rests on those, not on this sentence.'
            : 'The record can’t check “' + sWord + '” by itself. What you accepted as done decides it: ' + fmtCount(provenCount) + ' of ' + fmtCount(lineCount) + ' are proven on ' + ver + '.'),
        l3Link: !crit.accepted,
        l4: 'Approval never rests on this sentence; it rests on what counts as done.'
      });
    } else if (sProven) {
      const e = SP.earlier;
      note = Object.assign({}, NOTE0, {
        glyph: '●', gc: GRN, word: 'Proven: ' + (claimOf(S).proven || ''), wc: GRN, rest: '· version ' + ver + ((SP.scope || []).length ? ' · ' + SP.scope.join(', ') : ''),
        l2: tally(sRun) + (sRun.time ? ' at ' + sRun.time : '') + ' on ' + ver + (addedScope ? ', with ' + addedScope + ' in scope' : '') + (sMaker ? ' — after the edit at step ' + sMaker.step : '') + '.',
        hasEarlier: !!e, earlier: e ? 'At ' + e.at + ' this read: ○ Not proven on ' + e.version + '.' : '',
        l3Text: true, l3TextVal: 'Nothing to settle on ' + ver + '.' + (sRun.time ? ' Any change after ' + sRun.time + ' reopens it.' : ' Any later change reopens it.'),
        l4: acceptedRestsHere ? 'Approval rests on what counts as done; lines 1–' + lineCount + ' rest on this same run.' : (crit.accepted ? 'Approval rests on what counts as done, not on this statement.' : 'Approval rests on what counts as done, which isn’t accepted yet.'),
        hasEngine: isDemo, engine: 'Needs engine: “matched by ThruLine” (a command match). Until it exists this statement stays ○ Not proven, with ⑤ reading “A run on ' + ver + ' exists (step ' + sRun.step + ') and isn’t linked to this statement.” The lines of what counts as done carry the proof.'
      });
    } else if (isMoved) {
      const e = SP.earlier;
      note = Object.assign({}, NOTE0, {
        glyph: sContra ? '✕' : '○', gc: sContra ? CORAL : T2, word: 'Not proven', wc: sContra ? CORAL_TEXT : T1, rest: '· version ' + ver,
        l2: verdictSentence(FS),
        hasEarlier: !!e, earlier: e ? 'At ' + e.at + ' this read: Proven · version ' + e.version + ' · ' + (e.scope || []).join(', ') + '.' : '',
        l3Btn: true, l3Act: runCheck, l3Glyph: '▶', l3Label: 'Check what changed', l3KeyDisp: 'inline-block', l3Caption: clip(sCmd, 60) + ' on ' + ver + (checkOff ? askNote : ''),
        l4: 'Approval rests on what counts as done, not on this statement.'
      });
    } else {
      note = Object.assign({}, NOTE0, {
        glyph: sContra ? '✕' : '○', gc: sContra ? CORAL : T2, word: 'Not proven', wc: sContra ? CORAL_TEXT : T1, rest: '· version ' + ver,
        l2: verdictSentence(FS),
        hasExtra: isChk, extra: 'This check is running now. Nothing is settled until it finishes. Stopping it keeps whatever it printed so far; that still settles nothing.',
        l3Btn: true, l3Act: isChk ? stopCheck : runCheck, l3Glyph: isChk ? '' : '▶', l3Label: isChk ? 'Stop this check' : 'Check it again', l3KeyDisp: isChk ? 'none' : 'inline-block',
        l3Caption: isChk ? sCmd + ' · ' + running.elapsedS + ' s' : clip(sCmd, 60) + ' on ' + ver + askNote,
        l4: crit.accepted ? 'Approval rests on what counts as done, not on this statement.' : 'Approval rests on what counts as done, which isn’t accepted yet.'
      });
    }
    const l3Off = !isChk && checkOff;
    const l3c = l3Off ? chkGhost : { color: T1, border: '1px solid ' + CTRL, cursor: 'pointer', off: undefined, why: undefined };

    /* ---------- inspector: version ruler ---------- */
    const mk = (g, t, c, title) => ({ g: g, t: t, c: c || T2, title: title || '' });
    const rb = (label, marks, now, grow, last) => ({ label: label, marks: marks, lc: now ? T1 : T3, bg: now ? 'rgba(255,122,41,.06)' : 'transparent', ul: now ? 'underline solid ' + OR : 'none', grow: String(grow), br: last ? '0px solid transparent' : '1px solid #1F1F23' });
    const growOf = RULER_GROW[isMoved ? 'moved' : (isReady ? 'ready' : 'early')];
    const saidMsgs = (R.messages || []).filter((m) => stmts.some((x) => x.messageId === m.id));
    const MAX_MARKS = 5;
    const ruler = shownVersions.map((v, i, all) => {
      const now = v.id === ver; const at = [];
      const maker = editMaking(v.id);
      const seen = new Set();
      const pushEdit = (e) => { if (!e || seen.has(e.id) || e.step <= B.step) return; seen.add(e.id); at.push({ k: e.step, m: sinceIds.has(e.id) ? mk('◭', String(e.step), YEL, 'edit after the proof') : mk('', 'edit ' + e.step, T2) }); };
      pushEdit(maker);
      (R.edits || []).filter((e) => e.makesVersion === v.id && sinceIds.has(e.id)).forEach(pushEdit);
      (R.runs || []).filter((r) => r.version === v.id && (isDemo || r.kind === 'test')).forEach((r) => {
        if (r.status === 'finished') at.push({ k: r.step != null ? r.step : 1e9, m: mk(failing(r) ? '✕' : '●', r.step != null ? String(r.step) : '', now ? T1 : T2, rstep(r) + ': ' + tally(r)) });
        else if (r.status === 'no_result') at.push({ k: r.step, m: mk('⋯', String(r.step), T2, 'no result yet') });
        else if (r.status === 'running') at.push({ k: Infinity, m: mk('◔', 'run', T2, 'checking now') });
      });
      saidMsgs.filter((m) => { const st = stmts.find((x) => x.messageId === m.id); return st && st.versionBefore === v.id && (isDemo || m.id === B.messageId); }).forEach((m) => at.push({ k: m.step, m: mk('❝', String(m.step), T2, 'step ' + m.step + ': said') }));
      if (crit.accepted && crit.acceptedOnVersion === v.id) at.push({ k: crit.acceptedAfterStep + 0.5, m: mk('┃', crit.acceptedAt, now ? T1 : T2, 'what counts as done accepted') });
      at.sort((a, b) => a.k - b.k);
      let marks = at.map((x) => x.m);
      if (!isDemo && marks.length > MAX_MARKS) marks = marks.slice(-MAX_MARKS);
      if (now) marks.push(mk('◆', 'now', T1, 'now'));
      return rb(now ? v.id + ' · now' : v.id, marks, now, growOf[i] != null ? growOf[i] : 1, i === all.length - 1);
    });
    const rulerAria = 'Version ruler: ' + shownVersions.filter((v) => v.id !== ver).map((v) => v.id).join(', ') + ', then the current version';

    /* ---------- inspector: the chain, link by link ---------- */
    const DP = {
      hold: (n) => ({ mText: n, mBg: T1, mc: '#000000', mBorder: NO_B, mFs: '11px' }),
      moved: () => ({ mText: '◭', mBg: 'transparent', mc: YEL, mBorder: NO_B, mFs: '15px' }),
      nothing: () => ({ mText: '', mBg: 'transparent', mc: T2, mBorder: '1.5px dashed ' + T3, mFs: '11px' }),
      checking: () => ({ mText: '◔', mBg: 'transparent', mc: T2, mBorder: '1px solid ' + CTRL, mFs: '12px' }),
      notproven: () => ({ mText: '○', mBg: 'transparent', mc: T2, mBorder: NO_B, mFs: '16px' }),
      proven: () => ({ mText: '●', mBg: 'transparent', mc: GRN, mBorder: NO_B, mFs: '16px' }),
      recorded: () => ({ mText: '▪', mBg: 'transparent', mc: T2, mBorder: NO_B, mFs: '15px' }),
      failed: () => ({ mText: '✕', mBg: 'transparent', mc: T2, mBorder: NO_B, mFs: '13px' }),
      contradicted: () => ({ mText: '✕', mBg: 'transparent', mc: CORAL, mBorder: NO_B, mFs: '15px' })
    };
    const RAIL = {
      lit: { railW: '2px', railBg: OR, railShadow: GLOW },
      moved: { railW: '3px', railBg: 'repeating-linear-gradient(45deg, #F2C94C 0 2px, transparent 2px 5px)', railShadow: 'none' },
      dotted: { railW: '1px', railBg: 'repeating-linear-gradient(180deg, #66666F 0 2px, transparent 2px 6px)', railShadow: 'none' },
      none: { railW: '2px', railBg: 'transparent', railShadow: 'none' }
    };
    const ITEM0 = {
      isPlate: false, isStop: false, isNR: false, aria: '', eyebrow: '', sentence: '', sc: T1,
      hasSub: false, sub: '', hasSub2: false, sub2: '', hasTech: false, tech: '', hasChips: false, chips: [], hasLink: false, linkHref: 'What-Changed.dc.html', linkLabel: '',
      cardBg: '#111113', cardBorder: '1px solid ' + LINE, cardLeft: '1px solid ' + LINE,
      discDisp: 'inline-flex', mText: '', mBg: 'transparent', mc: T2, mBorder: NO_B, mFs: '11px', capDisp: 'none', capColor: YEL,
      railW: '2px', railBg: 'transparent', railShadow: 'none',
      border: CTRL, lc: OR, label: '', hasLooked: false, looked: '', hasOut: false, hasBtn: false, act: noop, btnGlyph: '▶', btnLabel: '', keyDisp: 'inline-block', btnCaption: '', hasDefine: false,
      btnColor: T1, btnBorder: '1px solid ' + CTRL, btnCursor: 'pointer', btnOff: undefined, btnWhy: undefined
    };
    const chip = (kind, v, rel, key) => ({
      kind: kind, ver: v ? v + (rel ? ' · ' + rel : '') : '', verDisp: v ? 'inline' : 'none',
      verBorder: rel === '1 version older' ? 'rgba(242,201,76,.6)' : CTRL, verColor: rel === '1 version older' ? YEL : T1,
      step: (key != null && key !== 'null' ? 'step ' + key : 'the check ThruLine ran') + (isDemo || isStep(stepKey(key)) ? ' ›' : ''), border: s.pinned === key ? OR : CTRL, pressed: s.pinned === key ? 'true' : 'false',
      href: stepTo(stepKey(key)),
      hint: isDemo ? 'Pin this record in the Technical drawer' : stepTo(stepKey(key)) ? 'Open this step' : undefined,
      pin: () => set({ pinned: key, open: Object.assign({}, s.open, { tech: true }) })
    });
    const plate = (n, eyebrow, mark, rail, sentence, o) => Object.assign({}, ITEM0, {
      isPlate: true, eyebrow: eyebrow, sentence: sentence,
      aria: 'Link ' + n + ', ' + eyebrow.slice(2).toLowerCase() + ': ' + sentence
    }, DP[mark](String(n)), RAIL[rail], o || {});
    const stop = (o) => Object.assign({}, ITEM0, { isStop: true, label: 'Proof stops here', discDisp: 'none', capDisp: 'block' }, o, { aria: 'Proof stops here: ' + (o.sentence || '') });
    const nr = (eyebrow) => Object.assign({}, ITEM0, { isNR: true, eyebrow: eyebrow, aria: eyebrow + ': not reached', discDisp: 'none' });
    const MOVEDCARD = { cardBg: 'repeating-linear-gradient(45deg, rgba(242,201,76,.07) 0 2px, transparent 2px 7px), #111113', cardLeft: '3px solid ' + YEL };
    const NOTHINGCARD = { cardBg: 'transparent', cardBorder: '1px dashed ' + T3, cardLeft: '1px dashed ' + T3 };
    const techSo = { hasTech: techOn && !!SP.rule, tech: 'from the recording · rule ' + SP.rule };
    const saidSentence = (st) => (st.agent || 'The agent') + ' wrote ❝' + pl(st) + '❞ · ' + st.time;
    const saidSub = 'The agent’s words, as recorded. Not proof by themselves.';
    const outCheck = checkOff ? { btnColor: chkGhost.color, btnBorder: chkGhost.border, btnCursor: chkGhost.cursor, btnOff: chkGhost.off, btnWhy: chkGhost.why } : {};
    const notProvenSo = 'Not proven on ' + ver + ' — claimed, and no proof found in the record for this version. The claim may be entirely true.';
    const match = SP && SP.restsOn && SP.restsOn.match;
    const restSub = (F) => {
      const r = F.run; const parts = [];
      const dur = r.durationMs != null ? secs(r.durationMs) : '';
      if (F.basis === 'matched') {
        if (rel(F.ranOn) === 'same version') parts.push('matched by ThruLine ⓘ (same command as step ' + (match ? match.sameCommandAs : '') + (match && match.widerScope ? ', wider scope' : '') + ', current version)');
        else { parts.push('matched by ThruLine ⓘ'); if (r.capturedBy === 'thruline') parts.push('captured by ThruLine'); }
      } else {
        if (r.exitCode != null) parts.push('exit ' + r.exitCode);
        parts.push(F.basis === 'cited_by_agent' ? 'cited by the agent' : F.basis === 'later_run' ? 'a later run on this version' : 'the last test run before it was said');
        parts.push(r.capturedBy === 'thruline' ? 'captured by ThruLine' : 'from the agent’s transcript');
      }
      if (dur) parts.push(dur);
      return parts.join(' · ');
    };

    let plates;
    if (fixed) {
      const saidF = plate(1, '① SAID', 'hold', 'lit', saidSentence(S), { hasSub: true, sub: saidSub, hasChips: true, chips: [chip('Said', '', '', String(S.step))] });
      const bundle = plate(2, '② RESTS ON', 'hold', 'lit', 'Decided by what counts as done: ' + fmtCount(provenCount) + ' of ' + fmtCount(lineCount) + ' proven on ' + (proven ? proven.version : ver) + '.', { hasChips: true, chips: [], hasLink: true, linkHref: 'Done-Criteria.dc.html', linkLabel: 'What counts as done ›' });
      const stopF = stop({ border: CTRL, capColor: T2, sentence: 'Nothing in the record can check “' + sWord + '” on its own.', hasDefine: !crit.accepted, hasLooked: !!crit.accepted, looked: 'Approval never rests on this sentence; it rests on what counts as done.' });
      const so6F = plate(6, '⑥ SO', 'recorded', 'none', 'Recorded — what is here doesn’t decide it either way.', { hasTech: techOn, tech: 'claim kind ' + S.claimKind + ' · no verifier for ' + S.claimKind });
      plates = !crit.accepted ? [saidF, stopF, nr('② ③ ④ ⑤'), so6F] : [saidF, bundle, stopF, nr('③ ④ ⑤'), so6F];
    } else {
      const F = FS; const r = F.run; const L = F.L;
      const said = plate(1, '① SAID', 'hold', 'lit', saidSentence(S), { hasSub: true, sub: saidSub, hasChips: true, chips: [chip('Said', '', '', String(S.step))] });
      plates = [said];
      if (!r) {
        plates.push(stop({ border: 'rgba(242,201,76,.55)', sentence: 'Claimed, and no proof found in the record: no test run came before it.', hasLooked: true, looked: lookedOf(F), hasBtn: false }));
        plates.push(nr('② ③ ④ ⑤'));
      } else if (L[1] === 'nothing') {
        plates.push(stop({ border: 'rgba(242,201,76,.55)', sentence: 'The run it rests on, ' + rstep(r) + ', has no result in the record.', hasLooked: true, looked: lookedOf(F), hasBtn: false }));
        plates.push(plate(2, '② RESTS ON', 'nothing', 'dotted', clip(r.command, 80) + ': no result yet', Object.assign({ hasChips: true, chips: [chip('Run', F.ranOn, rel(F.ranOn), String(r.step))] }, NOTHINGCARD)));
        plates.push(nr('③ ④ ⑤'));
      } else {
        const ranV = versionOf(F.ranOn) || {};
        const ca = F.P.restsOn.checkedAgainst;
        plates.push(plate(2, '② RESTS ON', 'hold', 'lit', clip(r.command, 80) + ': ' + tally(r), { hasSub: true, sub: restSub(F), hasSub2: !!ca, sub2: ca ? 'checked against: which tests → ' + ca.covers.join(', ') + ' only; not ' + ca.misses.join(', ') : '', hasChips: true, chips: [chip('Run', F.ranOn, rel(F.ranOn), String(r.step))] }));
        plates.push(plate(3, '③ RAN ON', 'hold', 'lit', 'version ' + F.ranOn + (R.project.worktree ? ' · ' + R.project.worktree : '') + ' · ' + rel(F.ranOn), { hasChips: ranV.recordedAtStep != null, chips: ranV.recordedAtStep != null ? [chip('Version', F.ranOn, '', String(ranV.recordedAtStep))] : [] }));
        if (L[3] === 'moved') {
          const e = F.since;
          plates.push(plate(4, '④ SINCE', 'moved', 'moved', F.sinceFile + ' changed at step ' + e.step + (e.time ? ' (' + e.time + ')' : '') + (e.step > S.step ? ', after the run and after the claim.' : ', after the run.') + (F.ranOn !== ver ? ' The code is now ' + ver + '.' : ''),
            Object.assign({ hasSub: isDemo ? (isMoved && !!F.P.since.dependsOnWholeChange) : !!F.P.since.dependsOnWholeChange, sub: 'This check doesn’t say which files it depends on, so ThruLine treats it as depending on everything this Change covers.', hasChips: true, chips: [chip('Edit', '', '', String(e.step))], hasLink: true, linkHref: 'What-Changed.dc.html', linkLabel: 'What changed ›' }, MOVEDCARD)));
        } else {
          plates.push(plate(4, '④ SINCE', 'hold', 'lit', F.maker && F.maker.step < r.step ? 'Still applies — ' + tail(F.maker.file, 40) + ' unchanged since ' + F.ranOn + '.' : 'Still applies — nothing changed after the run.'));
        }
        if (L[4] === 'checking') {
          plates.push(stop({ label: 'Checking now · ⑤', lc: T2, border: CTRL, capColor: T2, sentence: 'This check is running now. Nothing is settled until it finishes.', hasOut: true, hasBtn: true, act: stopCheck, btnGlyph: '', btnLabel: 'Stop this check', keyDisp: 'none', btnCaption: !isDemo && !acts.stopCheck && acts.stopWhy ? acts.stopWhy : 'Stopping it keeps whatever it printed so far.' }));
          plates.push(plate(5, '⑤ RE-CHECKED', 'checking', 'dotted', 'Checking now on ' + ver + ' · ' + running.elapsedS + ' s · no result yet.', NOTHINGCARD));
        } else if (L[4] === 'nothing') {
          plates.push(stop(Object.assign({ border: 'rgba(242,201,76,.55)', sentence: F.since ? 'Nothing on ' + ver + ' has run with ' + F.sinceFile + ' in scope.' : 'Nothing has run on ' + ver + ' since.', hasLooked: true, looked: lookedOf(F), hasBtn: true, act: runCheck, btnLabel: checkLabel, btnCaption: 'supplies the missing link ⑤ on ' + ver + (checkOff ? askNote : '') }, outCheck)));
          plates.push(plate(5, '⑤ RE-CHECKED', 'nothing', 'dotted', 'Nothing on ' + ver + ' yet.', NOTHINGCARD));
        } else if (L[4] === 'failed') {
          const fr = F.failedRun;
          plates.push(plate(5, '⑤ RE-CHECKED', 'failed', 'none', 'A later run failed: ' + rstep(fr) + ' · ' + tally(fr) + ' · on ' + fr.version + '.', { hasChips: true, chips: [chip('Run', fr.version, rel(fr.version), String(fr.step))] }));
        } else {
          plates.push(plate(5, '⑤ RE-CHECKED', 'hold', 'lit', 'Not needed — it ran on ' + F.ranOn + (addedScope ? ' with ' + addedScope + ' in scope' : '') + '.'));
        }
      }
      if (F.verdict === 'proven') plates.push(plate(6, '⑥ SO', 'proven', 'none', 'Proven · version ' + SP.version + ((SP.scope || []).length ? ' · ' + SP.scope.join(', ') : ''), Object.assign({ sc: GRN }, techSo)));
      else if (F.verdict === 'contradicted') plates.push(plate(6, '⑥ SO', 'contradicted', 'none', 'Not proven on ' + ver + ' — a later test run failed, so the record shows the opposite of what was claimed.', Object.assign({ sc: CORAL_TEXT }, techSo)));
      else plates.push(plate(6, '⑥ SO', 'notproven', 'none', notProvenSo, techSo));
    }
    const heldBar = sProven;
    if (heldBar && !s.showHeld) plates = [];
    const heldText = 'All 6 links hold on ' + (SP ? SP.version : ver) + ' · rests on ' + (sRun ? rstep(sRun) : '');
    const heldLabel = s.showHeld ? 'Hide the links' : 'Show all 6';
    const heldExp = s.showHeld ? 'true' : 'false';
    const toggleHeld = () => set({ showHeld: !s.showHeld });
    const chainNote = fixed ? (crit.accepted ? 'stops after ②' : 'stops at ②') : (sProven ? 'holds end to end' : sContra ? 'a later run failed' : 'stops at ' + (FS.stopAt >= 0 ? CIRC[FS.stopAt] : '⑤'));

    /* ---------- inspector: what would settle it ---------- */
    let settle;
    const scopeOut = rcScope ? ' It says nothing about files outside ' + rcScope + '.' : '';
    if (fixed) {
      settle = { sentence: !crit.accepted ? 'What counts as done decides this sentence. Nothing in the record can check “' + sWord + '” on its own.' : (provenCount === lineCount && provenRun ? 'What counts as done decides this sentence: all ' + fmtCount(lineCount) + ' lines rest on step ' + provenRun.step + ', on ' + ver + '.' : 'What counts as done decides this sentence: its ' + fmtCount(lineCount) + ' lines need a run on ' + ver + '.'), hasCmd: false, on: ver, showTech: false, scope: !crit.accepted ? proposedLine : 'Approval never rests on this sentence.' };
    } else if (sProven) {
      settle = { sentence: 'Settled for ' + ver + '.' + (sRun.time ? ' Any change after ' + sRun.time + ' reopens it.' : ' Any later change reopens it.'), hasCmd: true, on: rc.on || ver, showTech: techOn && !!rcTech, scope: 'It established that ' + (claimOf(S).establish || 'this run passes') + ' on ' + ver + '.' + scopeOut + ' ' + durNote(sRun) };
    } else {
      settle = { sentence: 'The same check, run again on the version this Change is about now.', hasCmd: !!sCmd, on: rc.on || ver, showTech: techOn && !!rcTech, scope: 'It can establish whether ' + (claimOf(S).establish || 'this run passes') + ' on ' + ver + '.' + scopeOut + (sRun ? ' ' + durNote(sRun) : '') };
    }
    const copyLabel = s.copied ? 'Copied' : 'Copy';
    const copyCmd = () => {
      if (!isDemo && typeof navigator !== 'undefined' && navigator.clipboard) { try { navigator.clipboard.writeText(sCmd); } catch (e) { /* best effort */ } }
      set({ copied: true });
    };

    /* ---------- inspector: collapsed rows ---------- */
    const tog = (k) => () => { const o = Object.assign({}, s.open); o[k] = !s.open[k]; set({ open: o }); };
    const row = (k, meta) => ({ exp: s.open[k] ? 'true' : 'false', open: !!s.open[k], chev: s.open[k] ? '▾' : '▸', toggle: tog(k), meta: meta });
    const ws = (n, t, c, stepN) => {
      const href = stepTo(stepN);
      return { n: String(n), t: t, chip: c ? (isDemo || isStep(stepN) ? c : c.replace(/ ›$/, '')) : '', chipDisp: c ? 'inline' : 'none', href: href };
    };
    const foundStmt = (st) => 'Found the statement “' + pl(st) + '” at step ' + st.step + ' — ' + (claimOf(st).noun || 'a claim') + '.';
    const runFacts = (r) => rstep(r) + ' · ' + (r.exitCode != null ? 'exit ' + r.exitCode + ' · ' : '') + tally(r) + ' · on ' + r.version;
    let whySteps, whyVerdict;
    if (fixed) {
      whySteps = [ws(1, foundStmt(S), 'step ' + S.step + ' ›', S.step), ws(2, 'No rule can check “' + sWord + '” by itself: ' + (claimOf(S).noun || 'a claim') + ' has no verifier.'), ws(3, !crit.accepted ? 'Looked for what counts as done: not accepted yet.' : 'Looked for what counts as done: accepted at ' + crit.acceptedAt + ', version ' + crit.version + ', ' + fmtCount(lineCount) + ' lines.')];
      whyVerdict = 'Recorded. What counts as done decides it, not this sentence.';
    } else if (!sRun) {
      whySteps = [ws(1, foundStmt(S), 'step ' + S.step + ' ›', S.step), ws(2, 'Looked for a test run before it: none in the record.')];
      whyVerdict = 'Not proven on ' + ver + '. Claimed, and no proof found in the record.';
    } else if (sProven) {
      whySteps = [ws(1, foundStmt(S), 'step ' + S.step + ' ›', S.step), ws(2, 'Found a run on the current version: ' + runFacts(sRun) + '.', rstep(sRun) + ' ›', sRun.step), ws(3, 'Compared versions: ' + rstep(sRun) + ' ran on ' + FS.ranOn + ', the current version.'), ws(4, 'Looked for an edit after that run: none.'), ws(5, (sRun.scope && sRun.scope.length ? 'Checked its scope: ' + sRun.scope.join(' and ') + ' were in scope.' : 'Checked its scope: the run does not state one.'), rstep(sRun) + ' ›')];
      whyVerdict = 'Proven · version ' + SP.version + ((SP.scope || []).length ? ' · ' + SP.scope.join(', ') : '') + '. A run proves a statement only for the version it ran on.';
    } else {
      const cmp = FS.ranOn === ver ? 'Compared versions: that run ran on ' + ver + ', the current version.' : 'Compared versions: that run’s ' + FS.ranOn + ' is not the current ' + ver + '.';
      const fifth = sContra ? ws(5, 'Found a later run that failed: ' + rstep(FS.failedRun) + '.', rstep(FS.failedRun) + ' ›', FS.failedRun.step)
        : ws(5, isChk ? 'Looked for a run on ' + ver + ': one is running, with no result yet.' : 'Looked for a run on ' + ver + ': none.', 'looked at ›');
      whySteps = [ws(1, foundStmt(S), 'step ' + S.step + ' ›', S.step), ws(2, (FS.basis === 'cited_by_agent' ? 'Found the run it cites: ' : 'Found the run it rests on: ') + runFacts(sRun) + '.', rstep(sRun) + ' ›', sRun.step), ws(3, cmp), FS.since ? ws(4, 'Found an edit after that run: ' + FS.sinceFile + ' (step ' + FS.since.step + ').', 'step ' + FS.since.step + ' ›', FS.since.step) : ws(4, 'Looked for an edit after that run: none.'), fifth];
      whyVerdict = 'Not proven on ' + ver + '. A run proves a statement only for the version it ran on.';
    }
    const ln = (t, c) => ({ t: t, c: c || T1 });
    const tn = (SP && SP.thenAndNow) || null;
    const lostLine = (x) => { const r = runOf(x.runId) || {}; const e = editOf(x.editId) || {}; return 'No longer counts: step ' + r.step + ' (' + tally(r) + ', on ' + r.version + ') — ' + tail(e.file, 40) + ' changed at step ' + e.step + '.'; };
    let thenMeta, thenLines;
    if (fixed || !tn) { thenMeta = 'nothing to compare'; thenLines = [ln(fixed ? 'This sentence has no proof of its own, so there is nothing to compare between versions.' : 'Its proof has not moved between versions.', T2)]; }
    else {
      thenMeta = tn.from + ' → ' + tn.to + ': ' + fmtCount(tn.noLongerCounts.length) + ' no longer counts · ' + fmtCount(tn.added.length) + ' new';
      thenLines = tn.noLongerCounts.map((x) => ln(lostLine(x)))
        .concat(tn.added.length ? tn.added.map((x) => { const r = runOf(x.runId) || {}; return ln('New: ' + rstep(r) + ' (' + tally(r) + ', on ' + r.version + ((x.newScope || []).length ? ', ' + x.newScope.join(', ') + ' in scope' : '') + ').'); }) : [ln('Kept: none · New: none.', T2)])
        .concat(isDemo && early ? [ln('The changed lines need the diff route; until then the edit step is the record.', T2)] : []);
    }
    const captured = (R.runs || []).filter((r) => r.capturedBy === 'thruline' && r.kind === 'test' && r.status === 'finished');
    const fromTranscript = (R.runs || []).filter((r) => r.capturedBy !== 'thruline' && r.kind === 'test' && r.status === 'finished');
    const stepList = (xs) => (xs.length > 1 ? xs.slice(0, -1).join(', ') + ' and ' + xs[xs.length - 1] : String(xs[0]));
    const saidNever = 'Said by the agent: step ' + S.step + ' — never counted.';
    let kindMeta, kindLines;
    if (fixed) { kindMeta = 'said by the agent 1 (never counted)'; kindLines = [ln(saidNever), ln('Nothing else can speak for “' + sWord + '”: it has no verifier.', T2)]; }
    else if (!captured.length) {
      const last = fromTranscript.slice(-5).map((r) => r.step);
      kindMeta = 'from the transcript ' + fmtCount(fromTranscript.length) + ' · said by the agent 1 (never counted)';
      kindLines = [ln(fromTranscript.length ? 'From the agent’s own transcript: ' + (fromTranscript.length > 5 ? 'the last 5, steps ' : (last.length > 1 ? 'steps ' : 'step ')) + stepList(last) + '.' : 'From the agent’s own transcript: no finished test run.'), ln(saidNever), ln('Captured by ThruLine · linked by you · checked by a person: none.', T2)];
    }
    else if (early) { kindMeta = 'captured ' + fmtCount(captured.length) + ' · said by the agent 1 (never counted)'; kindLines = [ln('Captured by ThruLine: ' + captured.map((r) => rstep(r) + ' (on ' + r.version + ')').join(', ') + '.'), ln(saidNever), ln('The agent’s own tool output (transcript): none used.', T2), ln('Linked by you · matched by ThruLine · brought in · checked by a person: none.', T2)]; }
    else { kindMeta = 'captured ' + fmtCount(captured.length) + ' · matched 1 · said 1 (never counted)'; kindLines = [ln('Captured by ThruLine: ' + (captured.every((r) => r.step != null) ? 'steps ' + stepList(captured.map((r) => r.step)) : captured.map(rstep).join(', ')) + '.'), ln('Matched by ThruLine: step ' + sRun.step + ' (same command as step ' + (match ? match.sameCommandAs : '') + (match && match.widerScope ? ', wider scope' : '') + ').'), ln(saidNever), ln('Linked by you · brought in · checked by a person: none.', T2)]; }
    const al = (n, t, st) => ({ n: String(n), t: t, s: st });
    const fu = (SP && SP.followUp) || {};
    const again = fu.checkedAgain ? runOf(fu.checkedAgain.runId) : null;
    const later = fu.laterWork ? editOf(fu.laterWork.editId) : null;
    const atT = (x) => (x && x.time ? ' at ' + x.time : '');
    let afterMeta, afterLines;
    if (fixed) { afterMeta = 'nothing to repair'; afterLines = [al(1, 'Nothing to repair', '— this sentence is recorded, not judged')]; }
    else {
      afterMeta = sProven && again ? 'checked again' + atT(again) : later ? 'reopened' + atT(later) : isChk ? 'a check is running' : 'nothing yet';
      afterLines = [
        al(1, 'Told the agent', toldW.busy ? '— keeping that in the record…'
          : toldW.said ? '— ' + toldW.said
          : toldRec ? (toldRec.act === 'marked_pasted' ? '— you said you pasted it' : toldRec.act === 'copied' ? '— copied, not sent' : '— you took back “pasted”') + (toldRec.when ? ' · ' + toldRec.when : '') + ' · nothing was sent'
          : toldShown ? '— copied, not sent' : (sProven ? '— not needed' : '— not yet')),
        al(2, 'Later work seen', later ? '— step ' + later.step + atT(later) : (fu.laterWorkSince ? '— none after ' + fu.laterWorkSince : '— none')),
        al(3, 'Checked again', isChk ? '— running now' : again ? '— ' + rstep(again) + atT(again) : '— not yet'),
        al(4, 'Still stands', sProven ? '— no: settled on ' + ver : '— yes, on ' + ver)];
    }
    const techLines = [
      ln('statement · claim kind ' + S.claimKind + ' · step ' + S.step + (S.verifier ? '' : ' · no verifier for ' + S.claimKind)),
    ].concat(R.record && R.record.chainHead ? [ln('record chain ' + R.record.chainHead + ' · tree ' + R.record.tree, T2)] : [])
      .concat(R.record && (R.record.feeds || []).length ? [ln('fed by the ' + R.record.feeds.join(' and '), T2)] : [])
      .concat(S.source && !isDemo ? [ln('said at ' + S.source.file + ':' + S.source.line, T3)] : [])
      .concat(isDemo ? [ln('each record’s file:line shows here once it is served (needs engine)', T3)] : []);
    const rows = {
      why: row('why', fmtCount(whySteps.length) + ' steps'),
      then: row('then', thenMeta),
      kind: row('kind', kindMeta),
      after: row('after', afterMeta),
      tech: row('tech', 'ids, hashes, chain position')
    };
    /* The record behind each receipt chip, by step. */
    const PIN = {};
    stmts.forEach((st) => { PIN[String(st.step)] = 'Said · step ' + st.step + ' · ' + st.time + ' · ' + st.agent + ' · ' + (st.source && !isDemo ? st.source.file + ':' + st.source.line : 'the transcript line'); });
    (R.runs || []).filter((r) => r.step != null && r.status === 'finished').forEach((r) => { PIN[String(r.step)] = 'Run · step ' + r.step + ' · ' + r.command + (r.exitCode != null ? ' · exit ' + r.exitCode : '') + ' · ' + tally(r) + ' · on ' + r.version + ' · ' + (r.durationMs != null ? secs(r.durationMs) : 'duration not recorded'); });
    (R.versions || []).filter((v) => v.recordedAtStep != null).forEach((v) => { PIN[String(v.recordedAtStep)] = 'Version · step ' + v.recordedAtStep + ' · ' + v.id + ', recorded after the edit at step ' + v.madeByStep; });
    (R.edits || []).forEach((e) => { const v = versionOf(e.makesVersion) || {}; PIN[String(e.step)] = 'Edit · step ' + e.step + ' · ' + e.file + ' · ' + e.makesVersion + (v.recordedAtStep != null ? ' recorded at step ' + v.recordedAtStep : ''); });
    const hasPinned = !!(s.pinned && PIN[s.pinned]);
    const pinnedText = hasPinned ? PIN[s.pinned] : '';

    /* ---------- inspector: sticky footer ---------- */
    const fCheck = !fixed && !sProven && !isChk && (pa === 'check_again' || pa === 'check_what_changed');
    const fDefine = fixed && !crit.accepted;
    const fStop = !fixed && isChk;
    const fNone = (!fixed && sProven) || (fixed && !!crit.accepted) || (!fixed && !sProven && !isChk && !fCheck);
    const fNoneText = fixed ? 'Approval never rests on this sentence.' : 'Nothing to settle on ' + ver + '.' + (crit.accepted ? ' Approving is the Change’s action now.' : '');
    const fTell = fCheck;
    const moreOpen = !!s.more;
    const moreExp = s.more ? 'true' : 'false';
    const toggleMore = () => set({ more: !s.more });
    const closeMore = () => set({ more: false });
    /* C30 R-LEDGER: "Add a note" opens the notes form (notes.jsx); in the gallery it only closes the menu, as drawn */
    const openNote = isDemo ? closeMore : () => set({ more: false, noteOpen: true });
    const closeNote = () => set({ noteOpen: false });
    /* C30 R-LEDGER: "This is wrong…" opens the objection form (objection.jsx); in the gallery it only closes the menu, as drawn */
    const openObjection = isDemo ? closeMore : () => set({ more: false, objectOpen: true });
    const closeObjection = () => set({ objectOpen: false });
    /* Only an absolute file path from a recorded edit can become an OS link.
     * A hunk's recorded line is used when present; no source line is invented. */
    const beforeStatement = (R.edits || []).filter((e) => e && e.sessionId === S.sessionId && e.step <= S.step && typeof e.file === 'string' && e.file && !e.file.startsWith('/') && !e.file.startsWith('~/') && !e.file.split('/').includes('..') && !isExclusionMarker(e.file) && e.editorPath);
    const editorEdit = beforeStatement.filter((e) => pl(S).includes(e.file)).at(-1) || beforeStatement.at(-1) || null;
    const editorLine = editorEdit && (editorEdit.hunks || []).flatMap((h) => [h.new_start, h.old_start]).find((n) => Number.isInteger(n) && n > 0) || null;
    const editorLink = editorEdit && editorHref(editorEdit.editorPath, editorLine);
    /* The absolute path stays in JS: the menu draws a button, not an href, and the OS link
     * is made only at the click (the home folder is never in rendered markup). */
    const openInEditor = editorLink ? () => {
      if (typeof document === 'undefined' || !document.body) return;
      const a = document.createElement('a');
      a.href = editorLink;
      a.hidden = true;
      document.body.appendChild(a);
      a.click();
      a.remove();
    } : null;
    /* What "Tell the agent" copies: where the proof stops, in the record's words. */
    const tellMessage = () => 'About “' + pl(S) + '” (step ' + S.step + '): ' + verdictSentence(FS) + (sCmd ? ' Please run ' + sCmd + ' on the current code and report the result.' : '');

    /* ---------- status bar (reads L1; its count equals the headline's) ---------- */
    const recWord = R.change.recording ? ' · recording' : '';
    const statusGlyph = s.sheet ? '▶' : pill.glyph;
    const statusGlyphColor = s.sheet ? T2 : pill.glyphColor;
    const steps = R.change.stepCount != null ? ' · ' + plural(R.change.stepCount, 'step', 'steps') : '';
    const statusLeft = s.sheet ? 'Check sheet open: ' + rcCmd + ' on ' + ver + askNote
      : isReady ? 'Ready for approval · ' + ver + ' · rests on ' + fmtCount(provenCount) + ' of ' + fmtCount(lineCount) + recWord
        : isMoved ? 'Needs a re-check · version ' + ver + (crit.accepted ? ' · ' + fmtCount(lineCount - provenCount) + ' of ' + fmtCount(lineCount) + ' lines stop at ⑤' : '') + recWord
          : isChk ? 'Not proven · version ' + ver + ' · 1 check running, ' + running.elapsedS + ' s' + recWord
            : pill.word + ' · version ' + ver + ' · ' + plural(stmts.length, 'statement', 'statements') + steps + recWord;

    /* C29: the decisions the person set, as the record shows them on the version
       in front of them (decisions.jsx); one slot of the template, empty in the gallery */
    const decisionRows = isDemo ? null : React.createElement(BriefDecisions, { view: briefDecisions(this.props.decisions, curV.fullId || ver) });

    /* C30 R-LEDGER: the words people kept on this Change, and the form "Add a note" opens (notes.jsx);
       one more slot of the template, empty in the gallery */
    const noteRows = isDemo ? null : React.createElement(BriefNotes, {
      notes: Array.isArray(D.notes) ? D.notes : [], changeId: acts.noteChangeId || null, open: !!s.noteOpen, onClose: closeNote,
      add: acts.addNote || (() => Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })), reload: acts.reload
    });

    /* C30 R-LEDGER: what people said was wrong with the statement in the inspector, beside the verdict it already
       has (the verdict is read from the statement and never changed here), and the form "This is wrong…" opens;
       two more slots of the template, empty in the gallery. The form is keyed on the statement, so words typed
       for one are never kept against another. */
    const objectionLines = isDemo ? null : React.createElement(BriefObjectionLines, {
      objections: Array.isArray(S.objections) ? S.objections : [], verdictWord: (S.proof && VERDICT_WORDS[S.proof.verdict]) || ''
    });
    const objectionForm = isDemo ? null : React.createElement(BriefObjectionForm, {
      key: S.id, open: !!s.objectOpen, sentence: S.plain || S.text || '', onClose: closeObjection,
      keep: (reason) => (acts.keepObjection ? acts.keepObjection(S.id, reason) : Promise.resolve({ ok: false, status: 0, doc: { error: 'not_available' } })),
      reload: acts.reload
    });

    return {
      keys: {
        next: nextStmt,
        prev: prevStmt,
        /* `.` and `y` press the Check it again and Tell the agent beside them: the panel under the proof chain
           draws both for the Change (pStop, pMoved), the inspector footer only for a statement a rule can check
           (fCheck). A key follows whichever of the two is drawn, never only the footer. */
        primary: () => (fCheck || pStop || pMoved) && !checkOff ? runCheck() : false,
        approve: () => pReady && this.props.at && this.props.at.change ? href('approve', this.props.at) : false,
        copy: () => (fTell || pStop || pMoved) ? tell() : false,
        expand: rows.why.toggle,
        then: rows.then.toggle,
        walk: walkNext,
        tech: rows.tech.toggle,
        cancel: () => {
          if (!s.sheet && !s.noteOpen && !s.objectOpen && !s.more) return false;
          set({ sheet: false, noteOpen: false, objectOpen: false, more: false });
        }
      },
      isDemo, decisionRows, noteRows, openNote, objectionLines, objectionForm, openObjection, editorHref: editorLink, openInEditor, editorAvailable: isDemo || !!editorLink, projectName, changeTitle, changeTitleFull, changeMeta, readDate, saidBy, saidAt, briefPrior, selPrior, chainAria, sourceLine, storage,
      approveNote, barSet, sinceLookLine, waitsOn, approveLabel, approveReadLabel, movedNote, changedList, checkingEyebrow,
      outLines, outCursorLine, liveAria, openLead, openCmd, openTail, rcCmd: rcCmdShow, rcTech, askNote, laneReqLabel, laneClaimsLabel, storageLine, edges, threadPaths, hasThread,
      proposedLine, rulerAria, heldText, saidStmts, stopEyebrow, lzLead, lzLink, lzNote, chk, chkGhost, checkTitle, l3Off, l3c, settleCmd: clip(sCmd, 90),
      ver, verNote, verNoteLong, readTime, alts, toggleTech, techPressed, plainColor, techColor,
      pill, stages, headline, hasHeadSub, headSub, headSubLive,
      chainCols, walkLabel, walkNext, tell, tellLabel, runCheck, stopCheck, checkLabel,
      pStop, pChecking, pReady, pMoved, pointerLeft, pointerTop, pointerH, pointerBg, lookedAt, lzDefault, lzReady, lzMoved,
      mapTransform, zoomLabel, zoomIn, zoomOut, zoomFit, panLeft, panRight,
      bands, laneWork, laneExec, nodes, btnNodes, findings, findCount,
      demos,
      inspKind, pagerLabel, prevStmt, nextStmt, fragCards, note, ruler,
      plates, heldBar, heldLabel, heldExp, toggleHeld, chainNote,
      settle, copyLabel, copyCmd, rows, whySteps, whyVerdict, thenLines, kindLines, afterLines, techLines,
      hasPinned, pinnedText,
      fCheck, fDefine, fStop, fNone, fNoneText, fTell, moreOpen, moreExp, toggleMore, closeMore,
      statusGlyph, statusGlyphColor, statusLeft
    };

    /* The live record's lanes, capped to what the map can draw without
     * crowding: per shown version column at most SLOTS nodes a lane, the nodes
     * the selected statement's thread passes through first, the rest folded
     * into one "+N more" node; the claims lane keeps the brief's statement and
     * the selected one. */
    function capGraph(all) {
      const keep = new Set([selRun, selMaker && selMaker.id, selSince].filter(Boolean));
      const out = [];
      out.push(...all.filter((n) => n.lane === 'request').slice(0, 2));
      for (const lane of ['work', 'execution']) {
        for (const v of shownVersions) {
          const inCol = all.filter((n) => n.lane === lane && n.version === v.id && (lane === 'work' || n.kind === 'run' || n.kind === 'command'));
          const reserve = lane === 'execution' && v.id === ver && !inCol.some((n) => n.kind === 'run') ? 1 : 0;
          const room = SLOTS - reserve;
          if (inCol.length <= room) { out.push(...inCol); continue; }
          const must = inCol.filter((n) => keep.has(n.id));
          const rest = inCol.filter((n) => !keep.has(n.id));
          const show = must.slice(0, room - 1);
          while (show.length < room - 1 && rest.length) show.push(rest.pop());
          show.sort((a, b) => a.step - b.step);
          out.push(...show);
          out.push({ id: 'more-' + lane + '-' + v.id, lane, kind: 'more', step: Infinity, version: v.id, count: inCol.length - show.length, label: lane === 'work' ? 'more edits on ' + v.id : 'more runs on ' + v.id });
        }
      }
      out.push(...all.filter((n) => n.kind === 'criteria'));
      const claimsN = all.filter((n) => n.kind === 'statement');
      const pickS = [B.id, selId].filter((id, i, a) => a.indexOf(id) === i);
      if (pickS.length < 2) { const other = claimsN.filter((n) => n.ref !== B.id).pop(); if (other) pickS.push(other.ref); }
      out.push(...claimsN.filter((n) => pickS.includes(n.ref)));
      return out;
    }
  }
}

/** Up to `n` items, always keeping the ones named, in their original order. */
function capKeeping(list, n, ids) {
  if (list.length <= n) return list;
  const keep = list.filter((x) => ids.includes(x.id));
  const rest = list.filter((x) => !ids.includes(x.id));
  const out = keep.slice(0, n);
  for (const x of rest) { if (out.length >= n) break; out.push(x); }
  return list.filter((x) => out.includes(x));
}
