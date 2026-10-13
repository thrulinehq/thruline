/* Approve-Sheet.dc.html: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from `this.props.data`
 * (live) or DEMO (the design gallery, where the demo switcher lays a variant over
 * it); this.state holds UI state only. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { recordingPill, storageSentence } from '../../data/project.js'
import { PLANS_URL } from '../../../workspace/licence-view.js'
import { fmtCount } from '../../data/count.js'
import { altitudes } from '../Change-Brief/altitude.js'
import { PLAN_WORDS } from '../../data/plan.js'
import { HQ_COPY } from '../../../workspace/hq-copy.js'
import { answerOf } from './actions.js'
import { decisionRows } from './from-change.js'
import { acceptedAfterWork } from '../../data/after-work.js'
import React from 'react'
import { DecisionRefusal, sheetDecisions } from './decisions.jsx'
import { href as routeHref } from '../../routes.js'

/* The licence could not be read: said in the licence page's own words, never as Free or Pro. */
const LICENCE_UNKNOWN = HQ_COPY.licence.status.unknown
const PLAN_NOT_READ = 'Plan not read'
export const PROPS = {"$preview":{"width":1512,"height":920}}

export default class Component extends DCLogic {
  state = {
    open: true,
    plan: 'pro',
    moves: false,
    name: ((this.props && this.props.data) || DEMO).reviewer.name,
    note: '',
    history: (((this.props && this.props.data) || DEMO).decisions || []).slice(),
    movedRefused: false,
    copied: '',
    runOpen: false,
    /* live only: the server's answer to the last act, as `answerOf` reads it, and whether one is on its way */
    answer: null,
    sending: false
  };

  renderVals() {
    const s = this.state || {};
    /* ---- content: live data, or the canvas's demo record with the switcher's variants laid over it ---- */
    const isDemo = !this.props.data;
    const D0 = this.props.data || DEMO;
    const D = isDemo
      ? Object.assign({}, D0, s.plan === 'free' ? D0.states.free : {}, s.moves ? D0.states.moves : {})
      : D0;
    const V = D.version.id, C = D.version.commit;
    /* live: the server's answer to the last act (C30 P0-2). A "moved" answer
       carries the version the code is now and the files the server named. */
    const ans = !isDemo && s.answer ? s.answer : null;
    const ansMoved = ans && ans.kind === 'moved' ? ans.moved : null;
    const M = ansMoved || D.moved || {};
    const movedFiles = (M.files || []).join(', ');
    const cr = D.criteria, cv = cr.version;
    const nLines = cr.lines.length;
    const nProven = cr.lines.filter((l) => l.state === 'proven').length;
    const run = D.run || { scope: [] };
    const hasRun = !!D.run;
    /* live: what the two acts carry, and whether the engine says this version is ready */
    const act = D.act || {};
    const A = (!isDemo && this.props.actions) || {};
    const tracked = isDemo || !!act.changeId;
    const ready = isDemo || (tracked && !!act.ready);
    /* C29: an accepted decision that blocks approval, broken or with no proof on
       exactly this version (decisions.jsx); the gallery has none. The server stays the gate. */
    const dec = sheetDecisions(isDemo ? null : this.props.decisions, D.version.fullId || V);
    const decisionRefused = dec.refusals.length > 0;
    const LINE = { proven: { glyph: '●', color: 'var(--tl-proven, #3DD68C)', word: 'Proven' }, needs_recheck: { glyph: '◭', color: '#F2C94C', word: 'Needs a re-check' }, recorded: { glyph: '▪', color: '#A1A1AA', word: 'Recorded' } };
    const lineMark = (state) => LINE[state] || { glyph: '○', color: '#A1A1AA', word: 'Not proven' };
    const lic = D.licence;
    const st = D.statements;
    const saidRecorded = st.find((x) => x.proof === 'recorded') || {};
    const saidProven = st.find((x) => x.proof === 'proven') || {};
    const tally = (r) => (!r || (r.passed == null && r.failed == null) ? 'no test count recorded'
      : r.failed ? fmtCount(r.failed) + ' failed, ' + fmtCount(r.passed) + ' passed' : fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed');
    const WORDS = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten'];
    const gb = (bytes) => (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
    const scopeList = run.scope.join(', ');
    /* live: first line only, whitespace collapsed, cut with "…"; the gallery draws the canvas as it is */
    const clip1 = (t, n) => {
      if (isDemo) return t;
      const lines = String(t).split('\n').map((x) => x.trim()).filter(Boolean);
      const one = (lines[0] || '').replace(/\s+/g, ' ');
      if (one.length > n) return one.slice(0, n - 1).replace(/\s+\S*$/, '') + '…';
      return lines.length > 1 ? one + '…' : one;
    };
    const demoTime = (k) => (D.demoTimes && D.demoTimes[k]) || 'just now';

    /* live: the record's decisions on every read; the gallery keeps the acts pressed in it */
    const recordedRows = Array.isArray(D.decisions) ? D.decisions : [];
    /* live: an act the server just recorded is drawn from the server's own rows,
       until a re-read of the record holds at least as many */
    const answeredRows = ans && ans.kind === 'recorded'
      ? (ans.decisions && Array.isArray(ans.decisions.history) ? decisionRows(ans.decisions.history) : recordedRows.concat(decisionRows([ans.decision])))
      : null;
    const history = isDemo ? (Array.isArray(s.history) ? s.history : [])
      : (answeredRows && answeredRows.length > recordedRows.length ? answeredRows : recordedRows);
    const last = history.length ? history[history.length - 1] : null;
    const approvedFull = last && last.kind === 'approved' ? last.targetRevision || act.approvedFull || null : null;
    const firstTime = (kind) => { const h = history.find((x) => x.kind === kind); return h ? h.time : ''; };
    const approved = !!(last && last.kind === 'approved');
    /* The screen knows the code moved once an Approve was refused for it,
       or once it moved after an approval. Until then it is the stale screen K6 describes. */
    const movesNow = !!D.moved || !!ansMoved;
    const codeNow = movesNow && (!!s.movedRefused || history.some((x) => x.kind === 'approved'));
    const refused = !!s.movedRefused && !approved;
    /* K7: approved on V, then the code moved. The approval stays on V. */
    const approvedMoved = approved && codeNow;
    const moved = codeNow && !approved;
    const lastWithdrawn = !!(last && last.kind === 'withdrawn');
    /* Free or Pro only: approving a version needs Pro (CTO C21-b). Live, the one
       plan rule (data/plan.js) says whether approving is open; a 403
       licence_required answer closes it whatever the last read said. */
    const licRefused = !!(ans && ans.kind === 'licence');
    const planUnknown = !isDemo && !licRefused && lic.plan == null;
    const ended = isDemo ? lic.plan !== 'pro' : licRefused || !lic.approving;
    const refusedAct = ans && ans.kind === 'refused' ? ans : null;
    const nameVal = typeof s.name === 'string' ? s.name : '';
    const nameOk = nameVal.trim().length > 0;
    const noteVal = typeof s.note === 'string' ? s.note : '';
    const when = (t) => (t === 'just now' ? ' just now' : ' on ' + t);
    const set = (patch) => this.setState(patch);
    const cur = () => this.state || {};

    /* ---- actions ---- */
    /* live: send one act, then draw whatever the server answers (C30 P0-2).
       Nothing is reloaded instead of drawn: the record is re-read only after
       an act was recorded, and the drawn answer stays until the next act. */
    const send = (kind, nm, words) => {
      const fn = kind === 'approve' ? A.approve : A.withdraw;
      if (!fn || cur().sending) return;
      set({ sending: true, answer: null, copied: '' });
      const unreachable = () => answerOf({ ok: false, status: 0, doc: { error: 'unreachable' } });
      let sent;
      try { sent = Promise.resolve(fn(nm, words)); } catch (err) { sent = Promise.reject(err); }
      sent.then(answerOf, unreachable).then((a0) => {
        let a = a0;
        if (kind === 'withdraw' && a.kind === 'moved') {
          a = { kind: 'refused', detail: 'The approval standing now is on ' + (a.moved.version || 'another version') + ', not the version on this screen. Reload this page, then try again.' };
        } else if (kind === 'withdraw' && a.kind === 'licence') {
          a = { kind: 'refused', detail: 'ThruLine refused this. Reload this page to see what the record holds.' };
        }
        a = Object.assign({ act: kind }, a);
        /* a recorded approval kept its note with it, so the box is emptied; anything else
           leaves the words where the person typed them */
        const kept = a.kind === 'recorded' && kind === 'approve';
        set(a.kind === 'moved' ? { sending: false, answer: a, movedRefused: true } : kept ? { sending: false, answer: a, note: '' } : { sending: false, answer: a });
        if (a.kind === 'recorded' && A.reload) A.reload();
      });
    };
    const approve = () => {
      const st0 = cur();
      const h = !isDemo ? history : Array.isArray(st0.history) ? st0.history : [];
      const lh = h.length ? h[h.length - 1] : null;
      const nm = typeof st0.name === 'string' ? st0.name.trim() : '';
      if (ended || st0.movedRefused || !nm || !ready || decisionRefused) return;
      if (lh && lh.kind === 'approved') return;
      if (!isDemo) { send('approve', nm, typeof st0.note === 'string' ? st0.note : ''); return; }
      if (isDemo ? st0.moves : movesNow) { set({ movedRefused: true, copied: '' }); return; }
      const again = h.some((x) => x.kind === 'approved');
      const nt = typeof st0.note === 'string' ? st0.note.trim() : '';
      set({ history: h.concat([{ kind: 'approved', time: again ? 'just now' : demoTime('approve'), name: nm, note: nt }]), note: '', copied: '' });
    };
    const withdraw = () => {
      const st0 = cur();
      const h = !isDemo ? history : Array.isArray(st0.history) ? st0.history : [];
      const lh = h.length ? h[h.length - 1] : null;
      const nm = typeof st0.name === 'string' ? st0.name.trim() : '';
      if (!lh || lh.kind !== 'approved' || !nm) return;
      if (!isDemo) {
        if (!approvedFull) return;
        send('withdraw', nm, approvedFull);
        return;
      }
      const again = h.some((x) => x.kind === 'withdrawn');
      set({ history: h.concat([{ kind: 'withdrawn', time: again ? 'just now' : demoTime('withdraw'), name: nm, note: '' }]), copied: '' });
    };
    const cancel = () => set({ open: false });
    const openSheet = () => set({ open: true });
    const onName = (e) => set({ name: e && e.target ? String(e.target.value) : '' });
    const onNote = (e) => set({ note: e && e.target ? String(e.target.value) : '' });
    const toggleRun = () => set({ runOpen: !cur().runOpen });
    const trailer = 'ThruLine-Approved: ' + V;
    const copyTrailer = () => {
      const text = trailer;
      try {
        const nav = typeof navigator !== 'undefined' ? navigator : null;
        if (nav && nav.clipboard && typeof nav.clipboard.writeText === 'function') {
          nav.clipboard.writeText(text).then(() => set({ copied: 'ok' }), () => set({ copied: 'fail' }));
        } else {
          set({ copied: 'fail' });
        }
      } catch (err) {
        set({ copied: 'fail' });
      }
    };
    const startOver = () => set({ open: true, plan: 'pro', moves: false, name: D0.reviewer.name, note: '', history: (D0.decisions || []).slice(), movedRefused: false, copied: '', runOpen: false });

    const tA = firstTime('approved'), tW = firstTime('withdrawn');

    /* ---- the state pill (sheet and screen share one derivation) ---- */
    const HATCH = 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)';
    const pill = approvedMoved
      ? { glyph: '◆', word: 'Approved on ' + V + ' — code changed since', meta: '', color: '#EDEDEF', border: '1px solid #EDEDEF', bg: 'transparent' }
      : approved
      ? { glyph: '◆', word: 'Approved on ' + V, meta: '', color: '#EDEDEF', border: '1px solid #EDEDEF', bg: 'transparent' }
      : moved
        ? { glyph: '◭', word: 'Needs a re-check', meta: '· version ' + M.version, color: '#F2C94C', border: '1px solid rgba(242,201,76,.6)', bg: HATCH }
        : ready
          ? { glyph: '◇', word: 'Ready for approval', meta: '· ' + V, color: '#EDEDEF', border: '1px solid #EDEDEF', bg: 'transparent' }
          : { glyph: '○', word: 'Not ready for approval', meta: '· ' + V, color: '#A1A1AA', border: '1px solid #2A2A30', bg: 'transparent' };
    pill.hasMeta = pill.meta.length > 0;

    /* ---- the Change screen behind ---- */
    const bgHeadline = approvedMoved
      ? 'Your approval stays on ' + V + '. The code is now ' + M.version + ', and nothing about ' + M.version + ' is approved.'
      : approved
      ? 'Approved on ' + V + ' by you' + when(last.time) + ', against what counts as done, version ' + cv + '.'
      : moved
        ? (movedFiles ? 'The code moved while you were looking: ' + movedFiles + ' changed' + (M.step != null ? ' at step ' + M.step : '') + '.' : 'The code moved while you were looking.')
        : !nLines ? 'Nothing counts as done yet, so nothing is ready.'
          : nProven === nLines ? 'All ' + fmtCount(nProven) + ' things that count as done are proven on ' + V + '.'
            : fmtCount(nProven) + ' of ' + fmtCount(nLines) + ' things that count as done are proven on ' + V + '.';
    const stillNeeded = fmtCount(nLines) + ' things still needed evidence.';
    const at = (t, rest) => (t ? 'At ' + t + rest : '');
    const bgEarlier0 = approvedMoved
      ? 'At ' + tA + ' this read: Approved on ' + V + '. At ' + D.readyAt + ': Ready for approval on ' + V + '.'
      : moved
      ? (lastWithdrawn ? 'At ' + tA + ' this read: Approved on ' + V + '.' : 'At ' + D.readyAt + ' this read: Ready for approval on ' + V + '.')
      : approved
        ? 'At ' + D.readyAt + ' this read: Ready for approval on ' + V + '. At ' + cr.acceptedAt + ': ' + stillNeeded
        : lastWithdrawn
          ? 'At ' + tA + ' this read: Approved on ' + V + '. At ' + cr.acceptedAt + ': ' + stillNeeded
          : 'At ' + cr.acceptedAt + ' this read: ' + stillNeeded;
    /* live: only the earlier readings the record holds a time for */
    const bgEarlier = isDemo ? bgEarlier0 : [
      approved || lastWithdrawn ? at(tA, ' this read: Approved on ' + V + '.') : '',
      cr.acceptedAt && nLines ? at(cr.acceptedAt, ': what counts as done (' + cv + ') was accepted.') : '',
    ].filter(Boolean).join(' ');
    const bgL4 = codeNow
      ? 'Approval of ' + M.version + ' waits on all ' + fmtCount(nLines) + '.'
      : approved
        ? 'It does not merge, deploy or tell the agent anything.'
        : 'Approving records your decision on ' + V + ' only. It does not merge, deploy or tell the agent anything.';
    const strip = cr.lines.map((l) => {
      const n = String(l.n);
      if (codeNow) return { n: n, glyph: '◭', color: '#F2C94C', border: '1px solid rgba(242,201,76,.6)', bg: HATCH, label: 'Line ' + n + ': Needs a re-check on ' + M.version };
      if (l.state === 'proven') return { n: n, glyph: '●', color: 'var(--tl-proven, #3DD68C)', border: '1px solid rgba(var(--tl-proven-rgb, 61, 214, 140), .5)', bg: 'rgba(var(--tl-proven-rgb, 61, 214, 140), .08)', label: 'Line ' + n + ': Proven on ' + V + (run.scope.length ? ', ' + run.scope.join(' and ') : '') };
      const mk = lineMark(l.state);
      return { n: n, glyph: mk.glyph, color: mk.color, border: l.state === 'needs_recheck' ? '1px solid rgba(242,201,76,.6)' : '1px solid #2A2A30', bg: l.state === 'needs_recheck' ? HATCH : 'transparent', label: 'Line ' + n + ': ' + mk.word + ' on ' + V };
    });
    const nRecheck = cr.lines.filter((l) => l.state === 'needs_recheck').length;
    const stripText = codeNow
      ? fmtCount(nLines) + ' of ' + fmtCount(nLines) + ' need a re-check on ' + M.version + ' · ' + (movedFiles || 'the code') + (M.step != null ? ' changed at step ' + M.step : ' changed') + ' · 0 proven on ' + M.version
      : !nLines ? 'Nothing counts as done yet.'
        : [fmtCount(nProven) + ' of ' + fmtCount(nLines) + ' proven on ' + V, scopeList, fmtCount(isDemo ? nLines - nProven : nRecheck) + ' need a re-check'].filter(Boolean).join(' · ');
    const mapDecision = approved
      ? { hasGlyph: true, text: 'Approved on ' + V + ' · ' + last.time, border: '1px solid rgba(255,122,41,.6)' }
      : lastWithdrawn
        ? { hasGlyph: false, text: 'The approval on ' + V + ' was withdrawn.', border: '1px solid #2A2A30' }
        : { hasGlyph: false, text: 'Nothing has been approved for this version yet.', border: '1px solid #2A2A30' };
    const band = codeNow
      ? { label: V, left: '574px', color: '#A1A1AA', rule: '0', bg: 'rgba(255,255,255,.025)' }
      : { label: V + ' · now', left: '640px', color: '#EDEDEF', rule: '1px solid #FF7A29', bg: 'rgba(255,122,41,.05)' };
    const aState = codeNow
      ? { glyph: '◭', color: '#F2C94C', word: 'Needs a re-check', meta: ' · version ' + M.version + (movedFiles ? ' · ' + movedFiles + ' changed after' : ' · the code changed after'), step: M.step != null ? ' · step ' + M.step : '' }
      : { glyph: '●', color: 'var(--tl-proven, #3DD68C)', word: 'Proven', meta: ' · version ' + V + (scopeList ? ' · ' + scopeList : ''), step: hasRun ? ' · step ' + run.step : '' };
    const aOf = (l) => (codeNow || l.state === 'proven' ? aState : Object.assign({}, lineMark(l.state), { meta: ' · version ' + V, step: '' }));
    const dThread = approved
      ? { stroke: '#FF7A29', w: '2', dash: 'none', filter: 'drop-shadow(0 0 4px rgba(255,154,82,.7))' }
      : { stroke: '#2A2A30', w: '1', dash: '4 4', filter: 'none' };
    const bgDecisionRows = history.map((h) => (h.kind === 'approved'
      ? { glyph: '◆', gc: '#EDEDEF', text: 'Approved on ' + V + ' by you' + when(h.time) + ', against what counts as done, version ' + cv + '.' }
      : { glyph: '◇', gc: '#A1A1AA', text: 'The approval on ' + V + ' was withdrawn by you' + when(h.time) + '.' }));

    const ORDER = { proven: 0, needs_recheck: 1, not_proven: 1, recorded: 2 };
    const saidRows = st.slice().sort((a, b) => (ORDER[a.proof] ?? 1) - (ORDER[b.proof] ?? 1)).map((x, i) => {
      const mk = lineMark(x.proof);
      return {
        text: x.text, step: x.step != null ? 'step ' + x.step : '', first: i === 0,
        isRecorded: x.proof === 'recorded', isState: x.proof !== 'recorded',
        glyph: mk.glyph, color: mk.color, word: mk.word,
        meta: x.proof === 'proven' ? '· version ' + V + (scopeList ? ' · ' + scopeList : '') : '· version ' + V
      };
    });
    /* what each line rests on, by its link's basis (from-change.js `linkOf`); a line with none says so */
    const BASIS = { person: 'You linked this yourself.', accepted_check: 'The check you accepted.', named: 'What ran names what this line is about.' };
    const rests = cr.lines.map((l, i) => ({
      n: String(l.n), text: l.text, basis: BASIS[l.link] || (isDemo ? '' : 'Nothing is linked to this line yet.'), gate: l.kind === 'gate',
      border: i === 0 ? 'none' : '1px solid #1F1F23',
      chainLabel: l.state === 'proven' ? 'Chain for line ' + l.n + ': links 1 to 5 hold, and it ends Proven on ' + V : 'Chain for line ' + l.n + ': it ends ' + aOf(l).word + ' on ' + V,
      a: aOf(l),
      /* the sheet rows are about the version being approved, as it stood when the sheet opened */
      sa: lineMark(l.state),
      mark: lineMark(l.state).glyph + ' ',
      meta: ' · version ' + V + (l.state === 'proven' && scopeList ? ' · ' + scopeList : ''),
      linkBg: l.state === 'proven' ? '#FF7A29' : '#2A2A30',
      linkGlow: l.state === 'proven' ? '0 0 5px rgba(255,154,82,.55)' : 'none',
      endBg: lineMark(l.state).color,
      /* a check ThruLine ran is in its own record and has no step: none is printed (never "step null") */
      step: hasRun && l.state === 'proven' && run.step != null ? 'step ' + run.step : ''
    }));

    /* ---- decision history in the sheet ---- */
    const decisionFile = '.thruline/decisions/' + V + '.json';
    const historyRows = history.map((h, i) => {
      const latest = i === history.length - 1;
      const isA = h.kind === 'approved';
      return {
        isApproved: isA,
        isWithdrawn: !isA,
        title: isA ? 'Approved ' + V + ' · ' + h.time + ' · by you' : 'The approval on ' + V + ' was withdrawn by you' + when(h.time) + '.',
        titleColor: isA ? '#EDEDEF' : '#A1A1AA',
        sub: isA
          ? 'Against what counts as done ' + cv + ' · rests on ' + fmtCount(nProven) + ' of ' + fmtCount(nLines) + ' lines, each with a run on ' + V + (C ? ' (= commit ' + C + ')' : '') + '.'
          : 'A row of its own, pointing at the approval it withdraws. Nothing was edited or deleted.',
        hasWho: isA,
        who: 'Name on the record: ' + (h.name || '') + ' · with the OS user and git user.email from this machine, not verified',
        hasNote: isA && !!h.note,
        noteText: 'Note: “' + (h.note || '') + '”',
        file: isA ? decisionFile : '',
        border: i === 0 ? 'none' : '1px solid #1F1F23',
        bg: latest && isA ? '#16161A' : 'transparent',
        anim: latest && isA ? 'tl-stamp 400ms cubic-bezier(0.2, 0, 0.38, 0.9) both' : 'none'
      };
    });

    /* ---- the footer: the one act, and why it is off when it is off ---- */
    const sending = !isDemo && !!s.sending;
    const approveEnabled = !approved && !moved && !ended && nameOk && ready && !sending && !decisionRefused;
    const NEEDS_PRO = 'Approving a version needs Pro.';
    let why;
    if (sending) {
      why = { l1: 'Recording…', c1: '#A1A1AA', l2: 'Waiting for ThruLine to answer.' };
    } else if (refusedAct) {
      /* live: the server refused the act; its reason, in its words or plain ones (actions.js answerOf) */
      why = { l1: refusedAct.act === 'withdraw' ? 'Not withdrawn.' : 'Not approved.', c1: '#EDEDEF', l2: refusedAct.detail };
    } else if (approved && !nameOk) {
      why = { l1: 'Withdrawing an approval has to have a name on it, too.', c1: '#EDEDEF', l2: 'Put your name in the box, and the button turns on.' };
    } else if (approved && !isDemo && !approvedFull) {
      why = { l1: 'ThruLine has not read the exact approved version yet.', c1: '#EDEDEF', l2: 'Reload this page, then try withdrawing it again.' };
    } else if (approvedMoved) {
      why = { l1: 'Your approval stays on ' + V + '.', c1: '#EDEDEF', l2: 'The code is now ' + M.version + ', and nothing about ' + M.version + ' is approved.' };
    } else if (approved) {
      why = { l1: 'It does not merge, deploy or tell the agent anything.', c1: '#EDEDEF', l2: 'Your approval names ' + V + ' only.' };
    } else if (moved) {
      why = { l1: 'The code is now ' + M.version + '.', c1: '#EDEDEF', l2: 'Nothing has run on ' + M.version + ' yet. See what changed, and check it again before approving.' };
    } else if (decisionRefused) {
      /* C29: the decision's own words and its sentence (decisions.jsx `sheetDecisions`) */
      why = { l1: dec.why.l1, c1: '#EDEDEF', l2: dec.why.l2 };
    } else if (planUnknown) {
      /* the licence was not read: neither Free nor Pro, in the licence page's own words */
      why = { l1: LICENCE_UNKNOWN.problem, c1: '#EDEDEF', l2: LICENCE_UNKNOWN.fix };
    } else if (ended) {
      /* C20: the refusal is two sentences at the control */
      why = { l1: NEEDS_PRO, c1: '#EDEDEF', l2: 'Everything else is free. Everything you recorded is still here.' };
    } else if (!tracked || !nLines) {
      why = { l1: 'Nothing counts as done yet, so nothing is ready.', c1: '#EDEDEF', l2: 'Say what counts as done for this Change first.' };
    } else if (!ready && act.readinessBlocker) {
      /* RUN-LINK-TRUTH-R5: the one blocker (`blockerOf`'s pair), word for word, then its act */
      why = { l1: act.readinessBlocker.reason, c1: '#EDEDEF', l2: act.readinessBlocker.enabler };
    } else if (!ready) {
      why = { l1: 'This version is not ready for approval.', c1: '#EDEDEF', l2: act.readiness || 'Every line of what counts as done has to be proven on it first.' };
    } else if (!nameOk) {
      why = { l1: 'An approval has to have a name on it.', c1: '#EDEDEF', l2: 'Put your name in the box, and the button turns on.' };
    } else {
      why = { l1: 'Nothing is recorded until you press Approve.', c1: '#A1A1AA', l2: 'Also recorded from this machine: OS user · git user.email. Neither is verified.' };
    }
    const planLine = ended
      ? (approved
        ? NEEDS_PRO + ' Every approval you recorded stays valid, and withdrawing one needs no licence.'
        : NEEDS_PRO + ' Everything else is free.')
      : approved
        ? 'Every approval you record stays valid, on every plan. Everything else is free.'
        : NEEDS_PRO + ' Everything else is free.';
    const copiedMsg = s.copied === 'ok'
      ? 'Copied, not sent. Paste it into your commit message.'
      : s.copied === 'fail'
        ? 'This browser didn’t allow copying here. Select the line above and copy it.'
        : '';

    /* ---- design-review switcher (design gallery only) ---- */
    const seg = (on) => ({
      pressed: on ? 'true' : 'false',
      bg: on ? '#2A2A30' : 'transparent',
      border: on ? '#66666F' : '#2A2A30',
      color: on ? '#EDEDEF' : '#A1A1AA'
    });
    const switchHint = (movesNow && !codeNow)
      ? 'The screen still shows ' + V + '. Press Approve to see the refusal.'
      : approvedMoved
        ? 'Approved first, then the code moved (step ' + M.step + ').'
        : '';
    const demo = {
      pro: Object.assign(seg(!ended), { pick: () => set({ plan: 'pro' }) }),
      free: Object.assign(seg(ended), { pick: () => set({ plan: 'free' }) }),
      hold: Object.assign(seg(!s.moves), { pick: () => set({ moves: false, movedRefused: false }) }),
      moves: Object.assign(seg(!!s.moves), { pick: () => set({ moves: true }) }),
      typed: Object.assign(seg(nameOk), { pick: () => set({ name: D0.reviewer.name }) }),
      empty: Object.assign(seg(!nameOk), { pick: () => set({ name: '' }) })
    };

    const tr = D.trace;
    const fileCount = (M.files || []).length;
    const afterWork = acceptedAfterWork(cr.acceptedTs, D.agent.lastEditTs, cr.acceptedAt, D.agent.lastEditAt);

    return {
      keys: { cancel: () => { if (!s.open) return false; cancel(); } },
      isDemo: isDemo,
      /* C30 R-PREFS: the altitudes are links to the Change's three views; the sheet sits over the
       * Change at Brief, which the canvas marks, and so does the product */
      alt: altitudes('brief', { page: false }),

      /* top bar + status bar */
      recordingLabel: isDemo ? 'Recording · following live' : recordingPill({ change: { recording: D.recording ? true : null } }).text,
      projectName: D.project.name,
      changeTitle: D.change.title,
      mainLabel: 'Change: ' + D.change.title,
      crumbVersion: codeNow ? M.version : V,
      crumbNote: codeNow ? (M.step != null ? 'changed at step ' + M.step : 'changed') : C ? '= commit ' + C : '',
      readLabel: 'read ' + ((lastWithdrawn ? tW : codeNow ? M.at : approved ? tA : D.readAt) || D.readAt || '') + ' ↻',
      statusLeft: approvedMoved
        ? 'Approved on ' + V + ' · ' + fmtCount(nLines) + ' of ' + fmtCount(nLines) + ' need a re-check on ' + M.version + ' · recording'
        : moved
        ? fmtCount(nLines) + ' of ' + fmtCount(nLines) + ' need a re-check on ' + M.version + ' · recording'
        : approved
          ? 'Approved on ' + V + ' · rests on ' + fmtCount(nProven) + ' of ' + fmtCount(nLines) + ' things that count as done · recording'
          : !nLines ? 'Nothing counts as done yet' + (isDemo || D.recording ? ' · recording' : '')
            : 'Rests on ' + fmtCount(nProven) + ' of ' + fmtCount(nLines) + ' things that count as done, each with a run on ' + V + ' · recording',
      /* the gallery's switcher, or the one plan rule's word; a licence not read states no plan */
      planStatus: isDemo ? (ended ? 'Free' : 'Pro') : licRefused ? PLAN_WORDS.free : PLAN_WORDS[lic.plan] || PLAN_NOT_READ,
      pricingHref: PLANS_URL, // the one named pricing address (PRICING-TRUTHS)
      storageLine: storageSentence(D.storage ? D.storage.bytes : null),

      /* the Change screen */
      changeMeta: ['Change · ' + fmtCount(D.change.sessions) + ' session' + (D.change.sessions === 1 ? '' : 's'), D.agent.name].filter(Boolean).join(' · '),
      /* T10 (t10.4): one recorded instant, when the session started — never a
       * composed clock-time range "a–b" (the canvas's "16:01–16:09" was the
       * invented range T10 removed; live serves no end time at all). */
      behindMeta: [D.project.name, [D.session.day, D.session.start].filter(Boolean).join(' ')].filter(Boolean).join(' · '),
      agentSaid: (D.agent.name || 'The agent') + ' said ',
      /* the dimmed Change behind the sheet: one line each, cut like everywhere else (C26.6); the whole text is the title */
      saidRecorded: clip1(saidRecorded.text || '', 60),
      saidProven: clip1(saidProven.text || '', 60),
      saidAt: [saidProven.at || saidRecorded.at, (saidProven.step != null ? saidProven.step : saidRecorded.step) != null ? 'step ' + (saidProven.step != null ? saidProven.step : saidRecorded.step) : null].filter(Boolean).join(' · '),
      saidRows: saidRows,
      version: V,
      approveLabel: 'Approve ' + V,
      gatedShort: NEEDS_PRO,
      bgPill: pill,
      bgHeadline: bgHeadline,
      bgEarlier: bgEarlier,
      bgL4: bgL4,
      strip: strip,
      stripText: stripText,
      bgShowApprove: !approved && !moved && !ended,
      bgShowGated: !approved && !moved && ended,
      bgShowMoved: codeNow,
      bgShowSecondary: approved || moved,
      bgSecondaryHasGlyph: approved,
      bgSecondaryLabel: approved ? 'See the decision' : 'Back to the approval sheet',
      openSheet: openSheet,
      codeNow: codeNow,
      band: band,
      movedNow: M.version + ' · now',
      lanes: {
        request: 'Request · ' + fmtCount(tr.lanes.request),
        work: 'Work · ' + fmtCount(tr.lanes.work),
        execution: 'Execution · ' + fmtCount(tr.lanes.execution),
        claims: 'Claims & evidence · ' + fmtCount(tr.lanes.claims)
      },
      traceVersions: tr.versions,
      requestStep: D.request.step != null ? 'step ' + D.request.step : '',
      requestText: D.request.text ? '“' + clip1(D.request.text, 70) + '”' : 'Not recorded',
      requestTitle: isDemo || !D.request.text ? undefined : D.request.text,
      edits: tr.edits.map((e) => ({ step: 'step ' + e.step, file: e.file })),
      runs: tr.runs.map((r) => ({ step: 'step ' + r.step, result: tally(r) + (r.exit != null ? ' · exit ' + r.exit : '') })),
      pendingStep: D.pending ? 'step ' + D.pending.step : '',
      pendingCommand: D.pending ? clip1(D.pending.command, 60) : '',
      pendingTitle: isDemo || !D.pending ? undefined : D.pending.command,
      pendingLine: D.pending ? 'Step ' + D.pending.step + ' has no result yet: ' : '',
      pendingVoid: D.pending ? 'If step ' + D.pending.step + ' runs and changes a file.' : '',
      criteriaVersion: cv || 'not accepted',
      criteriaMapNote: nLines ? fmtCount(nLines) + ' lines · accepted ' + cr.acceptedAt : 'Nothing counts as done yet',
      recordedStep: saidRecorded.step != null ? 'step ' + saidRecorded.step : '',
      provenStep: saidProven.step != null ? 'step ' + saidProven.step : '',
      wcadTitle: cv ? 'What counts as done · ' + cv : 'What counts as done',
      wcadNote: cv ? 'accepted ' + cr.acceptedAt + ' by ' + cr.acceptedBy + (afterWork ? ', after the work' : '') : 'not accepted yet',
      answerIn: 'Only you can answer it, in ' + (D.agent.name || 'the agent') + '.',
      mapDecision: mapDecision,
      dThread: dThread,
      bgNoDecision: history.length === 0,
      bgDecisionRows: bgDecisionRows,
      rests: rests,

      /* the sheet */
      open: !!s.open,
      /* C29: which decisions refuse, in their words, and which are not checked on this version; never in the gallery */
      decisionRefusal: isDemo || approved ? null : React.createElement(DecisionRefusal, {
        view: dec,
        seeWhere: this.props.at && this.props.at.change ? routeHref('brief', { change: this.props.at.change, project: this.props.at.project }) : undefined
      }),
      versionNote: (C ? ['= commit ' + C] : []).concat(D.version.clean ? ['no unsaved changes'] : []).join(' · '),
      pasteTitle: C ? 'Paste ' + C + ' into git show to see exactly this code.' : undefined,
      /* live: a move the record cannot name the files of says so, never "No files changed" */
      movedFilesLead: !isDemo && fileCount === 0 ? 'ThruLine could not name the files that changed since this screen loaded.'
        : (WORDS[fileCount] || fmtCount(fileCount)) + ' file' + (fileCount === 1 ? '' : 's') + ' changed since this screen loaded: ',
      movedFiles: movedFiles,
      movedMeta: [M.step != null ? 'step ' + M.step : null, M.at, 'now ' + M.version].filter(Boolean).join(' · '),
      trailer: trailer,
      /* C30 R-PR: the link in to this Change's pull-request preview, beside the trailer an approval gives */
      prHref: !isDemo && this.props.at && this.props.at.change ? routeHref('pr', { change: this.props.at.change, project: this.props.at.project }) : null,
      decisionFile: decisionFile,
      alsoWritten: ' in ' + (D.project.worktree || 'the project folder') + ', yours to commit, and an attestation signed with this machine’s key. It isn’t tied to an identity; the Technical face shows it, and checking it is free.',
      restsListLabel: 'What counts as done, version ' + String(cv).replace(/^v/, ''),
      restsMeta: !nLines ? 'Nothing counts as done yet: there is nothing for an approval to rest on.'
        : fmtCount(nLines) + ' lines of what counts as done · ' + cv + ', accepted ' + [cr.acceptedAt, cr.acceptedBy ? 'by ' + cr.acceptedBy : null].filter(Boolean).join(' ') + (cr.setAside != null ? ' · ' + fmtCount(cr.setAside) + ' set aside' : ''),
      barTitle: cr.acceptedAt && D.agent.lastEditAt ? 'Accepted ' + cr.acceptedAt + '; the agent’s last edit was ' + D.agent.lastEditAt + '.' : undefined,
      showBar: isDemo || (!!cr.acceptedAt && !!D.agent.lastEditAt && afterWork),
      provenMeta: ' · version ' + V + (scopeList ? ' · ' + scopeList : ''),
      saidProvenMeta: '· version ' + V + (scopeList ? ' · ' + scopeList : ''),
      runStep: hasRun && run.step != null ? 'step ' + run.step : '',
      // with nothing accepted, nothing rests on a run: the row is not drawn (never "All 0 rest on")
      hasRun: isDemo || (hasRun && nLines > 0),
      allRestOn: 'All ' + fmtCount(nLines) + ' rest on',
      runSummary: hasRun ? (run.command || '') + ' · ' + tally(run) : '',
      runStepCaret: hasRun && run.step != null ? 'step ' + run.step + ' ' : '',
      runDetail: !hasRun ? '' : [run.exit != null ? 'exit ' + run.exit : null, tally(run), run.durationS != null ? run.durationS + ' s' : null, run.at,
        run.cwd ? 'cwd ' + run.cwd : null, 'ran on ' + run.version + (C ? ' = commit ' + C : ''), run.capturedBy ? 'captured by ' + run.capturedBy : null].filter(Boolean).join(' · '),
      saidMeta: [D.agent.name, saidProven.at || saidRecorded.at, 'approval rests on what counts as done, not on these statements'].filter(Boolean).join(' · '),
      namesOnly: cv ? ' and ' + cv + ' only.' : ' only.',
      recordsIn: cv ? ' and ' + cv + ' in ' : ' in ',
      /* the note is kept with the approval, in the same record write (C30 R-LEDGER) */
      noteDisabled: isDemo ? undefined : sending,
      notePlaceholder: 'What you checked by hand, or why you are approving',
      noteSub: 'optional · kept with the decision',
      titleWords: approved ? 'Your decision on' : 'Approve',
      pill: pill,
      cancel: cancel,
      refused: refused,
      hasHistory: history.length > 0,
      historyRows: historyRows,
      showTrailer: approved,
      copyTrailer: copyTrailer,
      hasCopiedMsg: copiedMsg.length > 0,
      copiedMsg: copiedMsg,
      runOpen: !!s.runOpen,
      runExpanded: s.runOpen ? 'true' : 'false',
      runCaret: s.runOpen ? '▾' : '›',
      toggleRun: toggleRun,
      note: noteVal,
      showNote: !approved,
      onNote: onNote,
      name: nameVal,
      onName: onName,
      nameBorder: nameOk ? '#66666F' : '#FF7A29',
      nameOutline: !isDemo ? undefined : nameOk ? 'none' : '2px solid #FF7A29',
      nameShadow: !isDemo ? undefined : nameOk ? 'none' : '0 0 0 1px #FFFFFF inset',
      why: why,
      showCancel: !approved,
      showClose: approved,
      approveEnabled: approveEnabled,
      approveDisabled: !approved && !approveEnabled,
      planGated: ended && !planUnknown,
      approve: approve,
      withdrawEnabled: approved && nameOk && !sending && (isDemo || !!approvedFull),
      withdrawDisabled: approved && (!nameOk || sending || (!isDemo && !approvedFull)),
      withdraw: withdraw,
      planLine: planLine,
      planEnded: ended,
      /* a licence is installed from the terminal; the live screen offers See plans only (C20) */
      showInstall: isDemo && ended,

      /* review switcher */
      demo: demo,
      startOver: startOver,
      hasSwitchHint: switchHint.length > 0,
      switchHint: switchHint
    };
  }
}
