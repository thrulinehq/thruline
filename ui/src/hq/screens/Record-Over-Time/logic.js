/* Record-Over-Time.dc.html: the artboard's logic, from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from
 * `this.props.data` (live) or DEMO (the design gallery) — see DATA.md;
 * `this.state` holds UI state only. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { fmtClock, recordingPill, storageSentence } from '../../data/project.js'
import { href } from '../../routes.js'
import { fmtCount as fc } from '../../data/count.js'
import { IO } from '../Whole-Story/export-io.js'
import { REASONS, minutesText, recordCsv, sliceMetrics, sliceRows } from './metrics.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
const dayMs = 86400000
const isoMs = (iso) => Date.parse(iso + 'T00:00:00Z')
const fmtDay = (iso) => { const d = new Date(isoMs(iso)); return d.getUTCDate() + ' ' + MONTHS[d.getUTCMonth()]; }
const daysBetween = (fromIso, toIso) => Math.round((isoMs(toIso) - isoMs(fromIso)) / dayMs)
const gb = (bytes) => (bytes / 1e9).toFixed(1) + ' GB'

export default class Component extends DCLogic {
  state = { range: '30', agent: 'all', project: 'all', stop: null, menu: false, toast: false, banner: true, codexTried: false, focus: 'r30' };

  renderVals() {
    const S = this.state;
    const set = (u) => this.setState(u);
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    if (D.live && !D.tallies) return liveVals(this, D);

    /* ---------- the record, as tallies ----------
       per window, per project, per agent (D.tallies), flattened to the canvas's fields:
       n statements · p proven on their version · r needs a re-check now · fa not proven, nothing found
       fx not proven, a re-run on that version failed · r3 recorded, stops at RAN ON · r6 recorded, stops at SO
       re reopened by a later edit (= reP + fx + r) · reP re-proven after re-running · pass/fail/stop your re-runs
       b claims before any run · bP of them later proven · bN of them still nothing (bN <= fa) */
    const F = ['n', 'p', 'r', 'fa', 'fx', 'r3', 'r6', 're', 'reP', 'pass', 'fail', 'stop', 'b', 'bP', 'bN'];
    const flat = (t) => (t ? {
      n: t.statements, p: t.proven, r: t.needsRecheck, fa: t.noProofFound, fx: t.rerunFailed,
      r3: t.recordedStopsAtRanOn, r6: t.recordedStopsAtSo, re: t.reopened, reP: t.reproven,
      pass: t.reruns.passed, fail: t.reruns.failed, stop: t.reruns.stopped,
      b: t.beforeAnyRun.total, bP: t.beforeAnyRun.laterProven, bN: t.beforeAnyRun.stillNothing
    } : null);
    const zero = {}; F.forEach((f) => { zero[f] = 0; });
    const add = (x, y) => { const o = {}; F.forEach((f) => { o[f] = x[f] + y[f]; }); return o; };
    const projById = Object.fromEntries(D.projects.map((p) => [p.id, p]));
    const one = (r, proj, a) => flat(((D.tallies[r] || {})[proj] || {})[a]) || zero;
    const slice = (r, a, proj) => {
      const ps = proj === 'all' ? D.projects.map((p) => p.id) : [proj];
      const as = a === 'all' ? ['cc', 'cx'] : [a];
      let o = zero;
      ps.forEach((pid) => as.forEach((aid) => { o = add(o, one(r, pid, aid)); }));
      return o;
    };
    const median = (r, a, proj) => {
      const m = (((D.medianMinutesToProof[r] || {})[proj] || {})[a]);
      return m === undefined ? null : m;
    };
    /* statements per day, index 0 = (days - 1) days ago, last = today. null = not recorded */
    const DAYS = Math.max(...D.windows);
    const series = (proj, a, key) => (((D.daily[proj] || {})[a] || {})[key]) || null;

    const R = S.range, A = S.agent, P = S.project;
    const cur = slice(R, A, P);
    const np = cur.fa + cur.fx;
    const rec = cur.r3 + cur.r6;
    const med = median(R, A, P);
    const pl = (n, one, many) => (n === 1 ? one : many);
    const agentName = { all: 'both agents', cc: 'Claude Code', cx: 'Codex' };
    const projName = { all: fc(D.projects.length) + ' ' + pl(D.projects.length, 'project', 'projects') };
    D.projects.forEach((p) => { projName[p.id] = p.name; });
    const nothingYet = (pid) => pid !== 'all' && projById[pid].lifetime.statements === 0;
    const rangeWord = 'last ' + R + ' days';

    /* ---------- focus ring: drawn on the control you last used ---------- */
    const RING_OL = '2px solid #FF7A29';
    const RING_IN = 'inset 0 0 0 1px #FFFFFF';
    /* the canvas draws the ring on the control last used; live, only real :focus-visible draws it (styles.css) */
    const ol = (key) => (!isDemo ? undefined : S.focus === key ? RING_OL : 'none');
    const inner = (key) => (!isDemo ? undefined : S.focus === key ? RING_IN : 'none');

    /* ---------- header ---------- */
    const winKeys = D.windows.map(String);
    const ranges = winKeys.map((v, i) => ({
      label: v + ' days',
      pressed: R === v ? 'true' : 'false',
      bg: R === v ? '#2A2A30' : 'transparent',
      color: R === v ? '#EDEDEF' : '#A1A1AA',
      divider: i === 0 ? '0' : '1px solid #66666F',
      radius: i === 0 ? '5px 0 0 5px' : (i === winKeys.length - 1 ? '0 5px 5px 0' : '0'),
      ol: ol('r' + v),
      sh: inner('r' + v),
      pick: () => set({ range: v, focus: 'r' + v, menu: false, toast: false, stop: null })
    }));

    /* the agent chip beside the sentence already names the agent, so the sentence stays one line */
    const whoLead = 'statements';
    /* the second sentence names only what is there, so the four parts always add up to the headline count */
    const leadTail = [cur.r ? fc(cur.r) + ' ' + pl(cur.r, 'needs', 'need') + ' a re-check' : '',
      np ? fc(np) + ' ' + pl(np, 'has', 'have') + ' no proof in the record' : '',
      rec ? fc(rec) + ' ' + pl(rec, 'is recorded and settles', 'are recorded and settle') + ' nothing' : ''].filter(Boolean).join(', ');
    const lead = cur.n === 0
      ? 'Nothing to count in this selection.'
      : fc(cur.p) + ' of ' + fc(cur.n) + ' ' + (cur.n === 1 ? whoLead.replace(/s$/, '') : whoLead) + ' ' + pl(cur.n, 'is', 'are') + ' proven on the version ' + pl(cur.n, 'it was', 'they were') + ' about.'
        + (leadTail ? ' ' + leadTail + '.' : '');

    const hdr = {
      meta: rangeWord + ' · ' + projName[P] + ' · computed on this machine',
      projLabel: P === 'all' ? 'All ' + fc(D.projects.length) : projName[P],
      menuExpanded: S.menu ? 'true' : 'false',
      toggleMenu: () => set({ menu: !S.menu, toast: false, focus: 'proj' }),
      projOl: ol('proj'), projSh: inner('proj'),
      exportCsv: () => set({ toast: true, menu: false, focus: 'export' }),
      expOl: ol('export'), expSh: inner('export'),
      chipShow: A !== 'all',
      chipLabel: agentName[A] + ' only',
      chipAria: 'Showing ' + agentName[A] + ' only. Clear the agent filter',
      clearAgent: () => set({ agent: 'all', focus: 'agent-all', stop: null }),
      lead: lead
    };

    /* ---------- project menu ---------- */
    const projSub = (p) => {
      if (nothingYet(p)) return (projById[p].recording ? 'recording' : 'not recording') + ' · nothing recorded yet';
      const n = slice(R, A, p).n;
      return n === 0 ? 'no statements in this range' : fc(n) + ' ' + pl(n, 'statement', 'statements') + ' · ' + rangeWord;
    };
    const menu = {
      show: S.menu,
      items: ['all'].concat(D.projects.map((q) => q.id)).map((p) => ({
        label: p === 'all' ? 'All ' + projName.all : projName[p],
        sub: projSub(p),
        checked: P === p ? 'true' : 'false',
        tick: P === p ? '✓' : '',
        bg: P === p ? '#16161A' : 'transparent',
        pick: () => set({ project: p, menu: false, focus: 'proj', stop: null })
      }))
    };

    const csvName = 'record-over-time-' + R + 'd-' + (P === 'all' ? 'all-projects' : projName[P]) + (A === 'all' ? '' : '-' + (A === 'cc' ? 'claude-code' : 'codex')) + '.csv';
    const toast = {
      show: S.toast,
      file: 'Saved ' + csvName,
      rows: fc(cur.n) + ' ' + pl(cur.n, 'row', 'rows') + ', one per statement: agent, project, version, state, where its proof stops, and the step it was said at.',
      done: () => set({ toast: false })
    };

    /* ---------- KPI row ---------- */
    const ccS = slice(R, 'cc', P), cxS = slice(R, 'cx', P), bothS = slice(R, 'all', P);
    const k = {
      nLabel: A === 'all' ? 'Statements the agents made' : 'Statements ' + agentName[A] + ' made',
      n: fc(cur.n),
      nUnit: 'in ' + projName[P],
      nSub: A === 'all' ? fc(ccS.n) + ' Claude Code · ' + fc(cxS.n) + ' Codex' : 'of ' + fc(bothS.n) + ' made by both agents',
      nAria: (A === 'all' ? 'Statements the agents made: ' : 'Statements ' + agentName[A] + ' made: ') + fc(cur.n) + '. List them',
      p: fc(cur.p),
      of: 'of ' + fc(cur.n),
      pSub: fc(cur.reP) + ' re-proven after a later edit',
      pAria: 'Proven on the version they were about: ' + fc(cur.p) + ' of ' + fc(cur.n) + '. List them',
      re: fc(cur.re),
      reSub: fc(cur.r) + ' open · ' + fc(cur.reP) + ' re-proven' + (cur.fx ? ' · ' + fc(cur.fx) + ' failed' : ''),
      reAria: 'Needed a re-check after a later edit: ' + fc(cur.re) + ' of ' + fc(cur.n) + '. ' + fc(cur.r) + ' still need one, ' + fc(cur.reP) + ' were re-proven, ' + fc(cur.fx) + ' failed when re-run. List them',
      np: fc(np),
      npSub: fc(cur.fa) + ' nothing found · ' + (cur.fx ? fc(cur.fx) + ' ' + pl(cur.fx, 'run', 'runs') + ' failed' : 'no failed runs'),
      npAria: 'Claimed, and no proof found in the record: ' + fc(np) + ' of ' + fc(cur.n) + '. Open them in the Inbox',
      med: med === null ? '—' : fc(med),
      medUnit: med === null ? '' : 'min',
      medSub: med === null ? 'no statement proven in this range' : 'across the ' + fc(cur.p) + ' proven'
    };

    /* ---------- by agent ---------- */
    const HATCH = 'repeating-linear-gradient(45deg, rgba(242,201,76,.85) 0 2px, transparent 2px 5px)';
    const SEG = [
      { f: 'p',   bg: 'var(--tl-proven, #3DD68C)', img: 'none', bd: '0', one: 'proven on its version', many: 'proven on their version' },
      { f: 'r',   bg: 'rgba(242,201,76,.10)', img: HATCH, bd: '1px solid rgba(242,201,76,.75)', one: 'needs a re-check', many: 'need a re-check' },
      { f: 'fx',  bg: 'rgba(255,99,105,.14)', img: 'none', bd: '1px solid #FF6369', one: 'not proven: a re-run on its version failed', many: 'not proven: a re-run on their version failed' },
      { f: 'fa',  bg: 'transparent', img: 'none', bd: '1px dashed #82828C', one: 'not proven: nothing found', many: 'not proven: nothing found' },
      { f: 'rec', bg: '#2A2A30', img: 'none', bd: '1px solid #A1A1AA', one: 'recorded, which settles nothing', many: 'recorded, which settles nothing' }
    ];
    const maxN = Math.max(ccS.n, cxS.n, 1);
    const scale = 280 / maxN;
    const agentRow = (key, s, m) => {
      const selected = A === key;
      const vals = { p: s.p, r: s.r, fx: s.fx, fa: s.fa, rec: s.r3 + s.r6 };
      const segs = SEG.filter((g) => vals[g.f] > 0).map((g) => ({
        w: Math.max(3, vals[g.f] * scale).toFixed(1) + 'px',
        bg: g.bg, img: g.img, bd: g.bd,
        title: fc(vals[g.f]) + ' ' + pl(vals[g.f], g.one, g.many)
      }));
      const has = s.n > 0;
      const nm = key === 'cc' ? 'Claude Code' : 'Codex';
      return {
        pressed: selected ? 'true' : 'false',
        pick: () => set({ agent: selected ? 'all' : key, focus: 'agent-' + key, stop: null }),
        bd: selected ? '1px solid #FF7A29' : '1px solid #1F1F23',
        sh: selected
          ? (isDemo && S.focus === 'agent-' + key ? RING_IN + ', ' : '') + '0 0 0 1px rgba(255,122,41,.25), 0 0 16px rgba(255,122,41,.16)'
          : inner('agent-' + key),
        ol: ol('agent-' + key),
        nText: has ? fc(s.n) + ' ' + pl(s.n, 'statement', 'statements') : 'no statements here',
        segs: segs,
        total: has ? fc(s.n) : '',
        p: has ? fc(s.p) : '—', r: has ? fc(s.r) : '—', np: has ? fc(s.fa + s.fx) : '—',
        rec: has ? fc(s.r3 + s.r6) : '—', re: has ? fc(s.re) : '—',
        of: has ? ' of ' + fc(s.n) : '',
        fxShow: s.fx > 0,
        fxText: fc(s.fx) + ' ' + pl(s.fx, 're-run', 're-runs') + ' failed',
        med: m === null ? '—' : m + ' min',
        aria: nm + ': ' + fc(s.n) + ' ' + pl(s.n, 'statement', 'statements') + ', ' + fc(s.p) + ' proven on their version, ' + fc(s.r) + ' ' + pl(s.r, 'needs', 'need') + ' a re-check, '
          + fc(s.fa + s.fx) + ' not proven, ' + fc(s.r3 + s.r6) + ' recorded. ' + (selected ? 'This page shows ' + nm + ' only; press to show both agents.' : 'Press to show ' + nm + ' only.')
      };
    };
    const ag = {
      allPressed: A === 'all' ? 'true' : 'false',
      allBg: A === 'all' ? '#2A2A30' : 'transparent',
      allColor: A === 'all' ? '#EDEDEF' : '#A1A1AA',
      allOl: ol('agent-all'), allSh: inner('agent-all'),
      allPick: () => set({ agent: 'all', focus: 'agent-all', stop: null }),
      cc: agentRow('cc', ccS, median(R, 'cc', P)),
      cx: agentRow('cx', cxS, median(R, 'cx', P))
    };

    /* ---------- statements per day ---------- */
    const nDays = +R, start = DAYS - nDays;
    const useCC = A !== 'cx', useCX = A !== 'cc';
    const projs = P === 'all' ? D.projects.map((q) => q.id) : [P];
    const agents = [useCC ? 'cc' : null, useCX ? 'cx' : null].filter(Boolean);
    const pts = [];
    let fog = false;
    const goneToday = [];
    for (let i = start; i < DAYS; i++) {
      let c = 0, p = 0, gone = 0;
      agents.forEach((a) => {
        /* an agent's day is not recorded when any chosen project has no record of it that day */
        let missing = false, ac = 0, ap = 0;
        projs.forEach((pid) => {
          const cs = series(pid, a, 'statements'), ps = series(pid, a, 'proven');
          if (!cs) return;
          if (cs[i] === null) { missing = true; return; }
          ac += cs[i]; ap += ps ? (ps[i] || 0) : 0;
        });
        if (missing) { fog = true; gone += 1; if (i === DAYS - 1) goneToday.push(agentName[a]); } else { c += ac; p += ap; }
      });
      pts.push({ i: i - start, c: c, p: p, nil: gone === agents.length });
    }
    const daysWith = pts.filter((q) => !q.nil && q.c > 0).length;
    const L = 30, PW = 354, T = 10, PH = 138, BASE = T + PH;
    const step = PW / nDays;
    const maxC = Math.max(2, ...pts.map((q) => q.c));
    const yMax = Math.ceil(maxC / 2) * 2;
    const X = (q) => (L + (q.i + 0.5) * step).toFixed(1);
    const Y = (v) => (T + PH * (1 - v / yMax)).toFixed(1);
    const live = pts.filter((q) => !q.nil);
    const lineD = (key) => live.map((q, j) => (j === 0 ? 'M' : 'L') + X(q) + ' ' + Y(q[key])).join(' ');
    const areaD = live.length
      ? 'M' + X(live[0]) + ' ' + BASE + ' ' + live.map((q) => 'L' + X(q) + ' ' + Y(q.p)).join(' ') + ' L' + X(live[live.length - 1]) + ' ' + BASE + ' Z'
      : '';
    const dots = (key) => (nDays > 7 ? '' : live.map((q) => {
      const x = +X(q), y = +Y(q[key]);
      return 'M' + (x - 2.5).toFixed(1) + ' ' + y + ' a2.5 2.5 0 1 0 5 0 a2.5 2.5 0 1 0 -5 0';
    }).join(' '));
    const fogW = Math.max(step, 6);
    const tsHas = !nothingYet(P) && daysWith >= 2;
    const ts = {
      has: tsHas,
      none: !tsHas,
      msg: cur.n === 1 ? 'One statement, on one day.' : 'Too few days with statements to draw.',
      range: rangeWord + (A === 'all' ? '' : ' · ' + agentName[A]),
      grid: 'M30 ' + Y(yMax) + ' H384 M30 ' + Y(yMax / 2) + ' H384',
      yTop: fc(yMax), yTopY: (+Y(yMax) + 4).toFixed(1),
      yMid: fc(yMax / 2), yMidY: (+Y(yMax / 2) + 4).toFixed(1),
      xLeft: nDays + ' days ago',
      claimsD: lineD('c'), provenD: lineD('p'), areaD: areaD,
      dotsC: dots('c'), dotsP: dots('p'),
      fogDisp: fog ? 'inline' : 'none',
      fogLegend: fog ? 'inline-flex' : 'none',
      fogX: (L + PW - fogW).toFixed(1), fogW: fogW.toFixed(1),
      aria: 'Statements per day over the ' + rangeWord + ': ' + fc(cur.n) + ' made, ' + fc(cur.p) + ' proven on the version they were about. The busiest day had ' + fc(maxC) + '.' + (goneToday.length ? ' Today has no ' + goneToday.join(' or ') + ' record.' : fog ? ' On some days an agent has no record; those days are not counted as zero.' : '')
    };

    /* ---------- where the proof stops ---------- */
    const LABELS = ['SAID', 'RESTS ON', 'RAN ON', 'SINCE', 'RE-CHECKED', 'SO'];
    const counts = [null, cur.fa, cur.r3, null, cur.r + cur.fx, cur.r6];
    const stopTotal = cur.fa + cur.r3 + cur.r + cur.fx + cur.r6;
    let auto = null, best = 0;
    [5, 2, 3, 6].forEach((kk) => { if (counts[kk - 1] > best) { best = counts[kk - 1]; auto = kk; } });
    const sel = S.stop === null ? auto : S.stop;
    const lit = (sel !== null && counts[sel - 1] > 0) ? sel : auto;
    const maxStop = Math.max(1, ...counts.filter((c) => c !== null));
    const BH = 32 / maxStop;
    const hpx = (n) => (n > 0 ? Math.max(3, n * BH).toFixed(1) + 'px' : '0px');
    const ORANGE = '#FF7A29', GLOW = '0 0 6px rgba(255,154,82,.6)', UNLIT = '#2A2A30';
    const REC = { bd: '1px solid #A1A1AA', bg: '#2A2A30', img: 'none' };
    const ABS = { bd: '1px dashed #82828C', bg: 'transparent', img: 'none' };
    const RCK = { bd: '1px solid rgba(242,201,76,.75)', bg: 'rgba(242,201,76,.10)', img: HATCH };
    const FXS = { bd: '1px solid #FF6369', bg: 'rgba(255,99,105,.14)', img: 'none' };
    const partA = { 2: [cur.fa, ABS], 3: [cur.r3, REC], 5: [cur.r, RCK], 6: [cur.r6, REC] };
    const moved = lit === 5;
    const stList = LABELS.map((label, idx) => {
      const kk = idx + 1;
      const c = counts[idx];
      const a = partA[kk] || [0, ABS];
      const b = kk === 5 ? cur.fx : 0;
      const isSel = sel === kk;
      /* thread: holds before the lit stop, gap + end caps at the stop, not reached after */
      const holds = lit === null ? true : kk < lit;
      const isStop = lit === kk;
      const yellowSeg = moved && kk === 4;
      const leftLit = kk > 1 && (lit === null || kk <= lit - 1);
      const rightLit = kk < 6 && (lit === null || kk <= lit - 1);
      let dBg = 'transparent', dColor = '#A1A1AA', dBd = '1px solid ' + UNLIT;
      if (holds) { dBg = '#EDEDEF'; dColor = '#000000'; dBd = '1px solid #EDEDEF'; }
      if (yellowSeg) { dBg = 'transparent'; dColor = '#F2C94C'; dBd = '1px solid #F2C94C'; }
      if (isStop) { dBg = 'transparent'; dColor = '#EDEDEF'; dBd = '1px dashed #82828C'; }
      /* end caps take the colour of the cause: yellow where a later edit moved the proof, t2 for an absence or a record that settles nothing */
      const capColor = moved ? '#F2C94C' : '#A1A1AA';
      return {
        k: yellowSeg ? '◭' : String(kk),
        label: label,
        countText: c === null ? '—' : fc(c),
        countColor: c === null || c === 0 ? '#82828C' : '#EDEDEF',
        aDisp: a[0] > 0 ? 'block' : 'none', ah: hpx(a[0]), abd: a[1].bd, abg: a[1].bg, aimg: a[1].img,
        arad: b > 0 ? '0' : '2px 2px 0 0',
        bDisp: b > 0 ? 'block' : 'none', bh: hpx(b), bbd: FXS.bd, bbg: FXS.bg, bimg: FXS.img,
        lH: leftLit ? '2px' : (kk > 1 && !isStop ? '1px' : '0px'),
        lBg: leftLit ? ORANGE : (kk > 1 && !isStop ? UNLIT : 'transparent'),
        lImg: 'none',
        lGlow: leftLit ? GLOW : 'none',
        capL: isStop && kk > 1 ? 'block' : 'none', capLc: capColor,
        rH: rightLit ? (yellowSeg ? '4px' : '2px') : (kk < 6 && !(lit !== null && kk === lit - 1) ? '1px' : '0px'),
        rBg: rightLit ? (yellowSeg ? 'transparent' : ORANGE) : (kk < 6 && !(lit !== null && kk === lit - 1) ? UNLIT : 'transparent'),
        rImg: rightLit && yellowSeg ? 'repeating-linear-gradient(45deg, #F2C94C 0 2px, transparent 2px 5px)' : 'none',
        rGlow: rightLit && !yellowSeg ? GLOW : 'none',
        capR: lit !== null && kk === lit - 1 ? 'block' : 'none', capRc: capColor,
        dBg: dBg, dColor: dColor, dBd: dBd,
        lblColor: isSel ? '#EDEDEF' : '#A1A1AA',
        pressed: isSel ? 'true' : 'false',
        bg: isSel ? '#111113' : 'transparent',
        sh: [isSel ? 'inset 0 0 0 1px rgba(255,122,41,.55)' : '', isDemo && S.focus === 'stop-' + kk ? RING_IN : ''].filter(Boolean).join(', ') || 'none',
        ol: ol('stop-' + kk),
        aria: 'Link ' + kk + ', ' + label.toLowerCase() + ': ' + (c === null ? 'proof never stops here' : fc(c) + ' ' + pl(c, 'statement stops', 'statements stop') + ' here'),
        pick: () => set({ stop: kk, focus: 'stop-' + kk })
      };
    });

    const cSel = sel === null ? 0 : counts[sel - 1];
    /* the panel is about statements, so the sentences lead with the count alone and stay within two lines */
    let expl = '', actHref = '', actLabel = '', actHint = '';
    if (sel === null) {
      expl = 'Every statement in this selection is proven on the version it was about, so no proof stops anywhere.';
    } else if (sel === 1) {
      expl = 'Proof never stops at ① SAID: every statement was said at a recorded step, so this link always holds.';
    } else if (sel === 4) {
      expl = '④ SINCE is where a later edit shows, as moved. The proof then stops one link on, at ⑤, unless something re-ran.';
    } else if (!cSel) {
      expl = 'No statement stops at ' + ['', '①', '②', '③', '④', '⑤', '⑥'][sel] + ' ' + LABELS[sel - 1] + ' in this selection.';
    } else if (sel === 2) {
      expl = fc(cSel) + ' ' + pl(cSel, 'stops', 'stop') + ' at ② RESTS ON: nothing in the record matches what ' + pl(cSel, 'it says', 'they say') + '.'
        + (cur.bN ? ' ' + fc(cur.bN) + ' ' + pl(cur.bN, 'was', 'were') + ' said before any run was recorded.' : '');
      actHref = 'Inbox.dc.html'; actLabel = 'Open ' + pl(cSel, 'it', 'these ' + fc(cSel)) + ' in the Inbox'; actHint = 'Each shows what was looked at, and what would settle it.';
    } else if (sel === 3) {
      expl = fc(cSel) + ' ' + pl(cSel, 'stops', 'stop') + ' at ③ RAN ON: ThruLine didn’t watch ' + pl(cSel, 'its run', 'their runs') + ', so the code wasn’t captured. ' + pl(cSel, 'It settles', 'They settle') + ' as Recorded.';
      actHref = 'Every-Statement.dc.html'; actLabel = 'See ' + pl(cSel, 'it', 'these ' + fc(cSel)); actHint = 'Each opens its own proof chain.';
    } else if (sel === 5) {
      expl = fc(cSel) + ' ' + pl(cSel, 'stops', 'stop') + ' at ⑤ RE-CHECKED, after a later edit: '
        + (cur.fx
          ? fc(cur.r) + ' ' + pl(cur.r, 'has', 'have') + ' no finished run on the new version, and ' + fc(cur.fx) + ' ' + pl(cur.fx, 'was', 'were') + ' re-run and failed.'
          : pl(cSel, 'it has no', 'none has a') + ' finished run on the new version.');
      actHref = 'Inbox.dc.html'; actLabel = 'Open ' + pl(cSel, 'it', 'these ' + fc(cSel)) + ' in the Inbox'; actHint = 'Each has one next step there: Check it again.';
    } else if (sel === 6) {
      expl = fc(cSel) + ' “done” ' + pl(cSel, 'claim settles', 'claims settle') + ' as Recorded at ⑥ SO, by rule: a claim that a whole task is done is never promoted to Proven.';
      actHref = 'Every-Statement.dc.html'; actLabel = 'See ' + pl(cSel, 'it', 'these ' + fc(cSel)); actHint = 'What counts as done carries their proof.';
    }
    const st = {
      list: stList,
      total: fc(stopTotal) + ' of ' + fc(cur.n) + ' ' + pl(stopTotal, 'stops', 'stop') + ' short',
      expl: expl,
      actShow: !!actHref,
      actHref: actHref || 'Inbox.dc.html',
      actLabel: actLabel,
      actHint: actHref ? actHint : 'Nothing to open from this link.'
    };

    /* ---------- small counts ---------- */
    const br = {
      b: fc(cur.b),
      of: 'of ' + fc(cur.n),
      bPText: fc(cur.bP) + ' proven by a run made afterwards',
      bNText: fc(cur.bN) + ' still ' + pl(cur.bN, 'has', 'have') + ' nothing to rest on'
    };
    const rrTotal = cur.pass + cur.fail + cur.stop;
    /* a zero count carries no state mark: coral and green only where something happened */
    const rowFail = cur.fail
      ? { glyph: '✕', gc: '#FF6369', text: fc(cur.fail) + ' failed — the record shows the opposite' }
      : { glyph: '·', gc: '#82828C', text: '0 failed' };
    const rowPass = cur.pass
      ? { glyph: '●', gc: 'var(--tl-proven, #3DD68C)', text: fc(cur.pass) + ' passed, so ' + fc(cur.pass) + ' ' + pl(cur.pass, 'statement is', 'statements are') + ' proven' }
      : { glyph: '·', gc: '#82828C', text: '0 passed' };
    const rowStop = cur.stop
      ? { glyph: '⋯', gc: '#A1A1AA', text: fc(cur.stop) + ' stopped before a result' }
      : { glyph: '·', gc: '#82828C', text: '0 stopped before a result' };
    const rr = {
      total: fc(rrTotal),
      rows: rrTotal === 0
        ? [{ glyph: '·', gc: '#82828C', text: 'No re-runs in this selection.' }]
        : (cur.fail > 0 ? [rowFail, rowPass, rowStop] : [rowPass, rowFail, rowStop])
    };

    /* ---------- empty slice ---------- */
    let empty = { show: false, title: '', body: '', actLabel: '', act: () => set({ project: 'all', agent: 'all', stop: null }) };
    if (cur.n === 0) {
      const pr = P === 'all' ? null : projById[P];
      const life = pr ? pr.lifetime : null;
      const ago = life && life.lastStatementOn ? daysBetween(life.lastStatementOn, D.today) : null;
      const agoText = ago === null ? '' : fc(ago) + ' ' + pl(ago, 'day', 'days') + ' ago';
      const reach = ago === null ? null : winKeys.find((w) => +w > ago && +w > +R);
      const other = A === 'cc' ? 'cx' : 'cc';
      if (pr && life.statements === 0) {
        empty = { show: true, title: 'Nothing recorded yet in ' + pr.name,
          body: (pr.recording ? 'ThruLine is recording this project, and no agent has made a statement in it yet.' : 'No agent has made a statement in this project yet.') + ' Nothing here is counted — an empty record is not a zero.',
          actLabel: 'Show all ' + projName.all, act: () => set({ project: 'all', focus: 'proj', stop: null }) };
      } else if (pr && A !== 'all' && life.byAgent[A] === 0) {
        empty = { show: true, title: agentName[A] + ' made no statements in ' + pr.name,
          body: life.statements === 1
            ? 'The one statement in ' + pr.name + ' was made by ' + agentName[other] + ', ' + agoText + '.'
            : 'The ' + fc(life.statements) + ' statements in ' + pr.name + ' were made by ' + agentName[other] + '.',
          actLabel: 'Show both agents', act: () => set({ agent: 'all', focus: 'agent-all', stop: null }) };
      } else if (pr && reach) {
        empty = { show: true, title: 'No statements in ' + pr.name + ' in the ' + rangeWord,
          body: (life.statements === 1 ? 'Its one statement' : 'Its last statement') + ' was recorded on ' + fmtDay(life.lastStatementOn) + ', ' + agoText + '. Nothing after it was recorded for this project.',
          actLabel: 'Show the last ' + reach + ' days', act: () => set({ range: reach, focus: 'r' + reach, stop: null }) };
      } else {
        empty = { show: true, title: 'No statements in this selection',
          body: 'Nothing was recorded for this agent and range.',
          actLabel: 'Show everything', act: () => set({ project: 'all', agent: 'all', stop: null }) };
      }
    }

    const out = {
      isDemo: isDemo,
      banner: { show: isDemo && S.banner, dismiss: () => set({ banner: false }) },
      codex: { notTried: !S.codexTried, tried: S.codexTried, retry: () => set({ codexTried: true }),
        triedText: 'Tried again at ' + D.readAt + '. Still not answering, so nothing from Codex is being recorded.' },
      side: {
        recording: 'Recording · ' + fc(D.projects.filter((q) => q.recording).length) + ' ' + pl(D.projects.filter((q) => q.recording).length, 'project', 'projects'),
        readAt: 'read ' + D.readAt + ' ↻',
        yourMove: (D.inbox.yourMove == null ? null : fc(D.inbox.yourMove)),
        yourMoveAria: (D.inbox.yourMove == null ? null : fc(D.inbox.yourMove) + ' ' + pl(D.inbox.yourMove, 'Change is', 'Changes are') + ' your move'),
        projects: D.projects.map((q) => ({
          name: q.name,
          sub: q.recording
            ? (q.lifetime.statements === 0 ? 'recording · nothing yet' : 'recording · ' + fc(q.changes) + ' ' + pl(q.changes, 'Change', 'Changes'))
            : 'recorded ' + (q.lastRecordedOn ? fmtDay(q.lastRecordedOn) : 'never') + ' · ' + fc(q.dismissedChanges) + ' dismissed'
        })),
        hook: D.capture ? ' · hook ' + D.capture.claudeCode.hookVersion : '',
        runCapture: D.capture ? (D.capture.runCapture ? ' on' : ' off') : '',
        storage: 'ThruLine is keeping ' + gb(D.storage.bytes) + ' of records on this machine · '
      },
      hdr: hdr,
      ranges: ranges,
      menu: menu,
      toast: toast,
      has: cur.n > 0,
      empty: empty,
      k: k,
      ag: ag,
      ts: ts,
      st: st,
      br: br,
      rr: rr,
      foot: { text: fc(cur.n) + ' ' + pl(cur.n, 'statement', 'statements') + ' · ' + agentName[A] + ' · ' + rangeWord + ' · ' + projName[P] + ' · counted on this machine' }
    };
    return D.live ? liveCounted(out, D, { cur, ccS, cxS, S, set, R, A, P, save: this.props.save || IO.save, at: this.props.at, codex: liveCodex(this.props, S, set) }) : out;
  }
}

/* ---------- the live path ----------
 * The project documents are read; the per-statement counts are not served in
 * this version (from-projects.js says why and what it would cost), so the
 * page says so in plain words and its counting controls are disabled. */
const pl = (n, one, many) => (n === 1 ? one : many);
const AGENT = { claude_code: 'Claude Code', codex: 'Codex' };
const DAY = (iso) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d.getDate() + ' ' + MONTHS[d.getMonth()];
};
export const LIVE_TEXT = Object.freeze({
  title: 'The statements could not be read just now.',
  off: 'Counting by day needs the statements, and they could not be read just now.'
});

/* The Capture card says what is true of the folder in front of the person, from the one capture read
 * (data/capture.js): a read that failed is said as one (never a card that vanishes, never a guess about
 * Codex); capture that is off, an observer that is not answering and an observer watching another folder
 * are three different faults and are named as three. A fact the route could not read (null) is no fault. */
const CODEX_NOTE = 'Nothing from Codex is being recorded until it answers, so today has no Codex record.';

function codexFault(props) {
  const error = props.captureError;
  if (error) {
    return { label: 'Capture could not be checked just now', note: String(error) + ' Whether Codex is being recorded is not known until it can be.', still: 'Capture still could not be checked.' };
  }
  const codex = props.captureHealth && props.captureHealth.codex;
  if (!codex) return null;
  if (!codex.configured) {
    return { label: 'Codex capture is off', note: 'Nothing from Codex is being recorded, so today has no Codex record.', still: 'Codex capture is still off, so nothing from Codex is being recorded.' };
  }
  if (codex.state === 'disconnected' || codex.state === 'stopped') {
    return { label: 'Codex app server not answering', note: CODEX_NOTE, still: 'Still not answering, so nothing from Codex is being recorded.' };
  }
  if (codex.watchesThisProject === false) {
    return { label: 'Codex is watching a different folder', note: 'Nothing from Codex in this project is being recorded, so today has no Codex record here.', still: 'Codex is still watching a different folder, so nothing from Codex in this project is being recorded.' };
  }
  return null;
}

function liveCodex(props, state, set) {
  const fault = codexFault(props);
  /* Try again was asked and its read is out: what is on screen is the answer BEFORE it, so no result is stated
   * ("Still …") until the read returns. The card keeps what it said at the click and says only "Checking…"
   * (the word First run uses for a read that is out). */
  const checking = !!state.codexTried && !!props.captureReading;
  const shown = fault || (checking ? state.codexWas : null);
  const down = !!shown;
  return {
    down,
    label: shown ? shown.label : undefined,
    note: shown ? shown.note : undefined,
    notTried: down && !state.codexTried,
    tried: down && state.codexTried,
    retry: () => {
      if (props.retryCapture) props.retryCapture();
      set({ codexTried: true, codexTriedAt: fmtClock(new Date()), codexWas: fault });
    },
    triedText: !shown ? '' : checking ? 'Checking…' : 'Tried again at ' + state.codexTriedAt + '. ' + shown.still
  };
}

function liveSide(D) {
  const n = D.projects.length;
  const projWord = fc(n) + ' ' + pl(n, 'project', 'projects');
  const src = D.sources.map((x) => AGENT[x]).filter(Boolean);
  return {
      recording: recordingPill({}).text,
      readAt: 'read ' + D.readAt + ' ↻',
      yourMove: (D.inbox.yourMove == null ? null : fc(D.inbox.yourMove)),
      yourMoveAria: (D.inbox.yourMove == null ? null : fc(D.inbox.yourMove) + ' ' + pl(D.inbox.yourMove, 'Change is', 'Changes are') + ' your move'),
      // the current project first, marked current; then the others by last activity, newest first (as the Inbox lists them)
      projects: D.projects.filter((p) => p.id === D.currentProject)
        .concat(D.projects.filter((p) => p.id !== D.currentProject).sort((a, b) => String(b.lastActivity || '').localeCompare(String(a.lastActivity || ''))))
        .map((p) => {
          const sub = (p.lastActivity ? 'recorded ' + DAY(p.lastActivity) + ' · ' : '') + (p.threads === null ? 'Changes not counted' : fc(p.threads) + ' ' + pl(p.threads, 'Change', 'Changes'));
          // C30 R-NAV: a sidebar project row is a link to that project's Inbox
          return { name: p.name, sub, title: sub, cur: p.id === D.currentProject ? 'true' : undefined, bg: p.id === D.currentProject ? '#16161A' : undefined, edge: p.id === D.currentProject, href: href('inbox', { project: p.id }) };
        }),
      cap1: src[0] || 'Nothing',
      hook: src[0] ? ' · in the record' : ' recorded yet',
      cap2: src[1] || (src[0] ? 'One agent' : 'No agent'),
      runCapture: src[1] ? ' · in the record' : ' recorded so far',
      storage: storageSentence(D.storage.bytes)
  };
}

/* What the record does not hold is said instead of a number. */
export const NOT_COUNTED = 'not counted';
const NO_EXPORT = 'Nothing to save: no statements in this selection.';
const EXPORT_TIP = 'Save the counts on this page as a CSV file, one row per project and agent.';

/* The canvas's page, counted from the record: the design's own values for
 * everything the verdicts give, and the counts that need more (minutes to proof,
 * proofs moved by a later edit, statements made before any run) from the rows
 * the statements route serves (metrics.js). A count the record cannot give is
 * drawn as "Not counted" with its reason, never 0. */
function liveCounted(out, D, { cur, ccS, cxS, S, set, R, A, P, save, at, codex }) {
  const rows = D.rows || [];
  const mine = sliceRows(rows, { range: R, agent: A, project: P });
  const m = sliceMetrics(mine);
  const mm = m.median, mv = m.moved, mb = m.before;

  /* ---- minutes from a statement to its proof ---- */
  const minText = (x) => { const t = minutesText(x); return +t >= 1000 ? fc(+t) : t; };
  let med = '—', medUnit = '', medSub, medTitle;
  if (!mm.served) {
    medSub = 'Not counted: proof times not recorded';
    medTitle = REASONS.median;
  } else {
    const how = 'Median of the minutes from each statement to the recorded start of the run that proves it, over the statements in this selection that were proven by a run made after they were said.';
    const already = mm.already ? ' ' + fc(mm.already) + ' more ' + pl(mm.already, 'was', 'were') + ' already backed by a run made before ' + pl(mm.already, 'it was', 'they were') + ' said, so ' + pl(mm.already, 'it waited', 'they waited') + ' for nothing and ' + pl(mm.already, 'is', 'are') + ' not in this number.' : '';
    const untimed = mm.unmeasured ? ' ' + fc(mm.unmeasured) + ' could not be timed, because the record holds no time for one of the two steps, and ' + pl(mm.unmeasured, 'is', 'are') + ' left out.' : '';
    if (mm.minutes !== null) {
      med = minText(mm.minutes); medUnit = 'min';
      medSub = 'across the ' + fc(mm.waited) + ' proven by a later run';
      medTitle = how + ' Here that is ' + fc(mm.waited) + ' ' + pl(mm.waited, 'statement', 'statements') + '.' + already + untimed;
    } else if (mm.proven === 0) {
      medSub = 'no statement proven in this selection';
      medTitle = how + ' No statement in this selection is proven.';
    } else {
      medSub = mm.already ? 'all ' + fc(mm.already) + ' proven were already backed' : 'none could be timed';
      medTitle = how + ' None waited for a later run.' + already + untimed;
    }
  }
  const forAgent = (a) => sliceMetrics(sliceRows(rows, { range: R, agent: a, project: P }));

  /* ---- Export CSV: the rows of this selection, built here and handed to the browser ---- */
  const exp = recordCsv({ rows, projects: D.projects, range: R, agent: A, project: P, through: D.today });
  const none = exp.count === 0;

  const agentLive = (row, s, key) => {
    const am = forAgent(key);
    const mins = am.median.served && am.median.minutes !== null ? minText(am.median.minutes) + ' min' : '—';
    return Object.assign({}, row, {
      re: s.n > 0 ? (am.moved.served ? fc(am.moved.total) : '—') : row.re,
      of: s.n > 0 && am.moved.served ? row.of : '',
      med: s.n > 0 ? mins : '—',
      fxText: fc(s.fx) + ' ' + pl(s.fx, 'run', 'runs') + ' failed',
    });
  };

  /* ---- the small cards ---- */
  const br = mb.served
    ? {
      b: fc(mb.total), of: 'of ' + fc(cur.n),
      bPText: fc(mb.laterProven) + ' proven by a run made afterwards',
      bNText: fc(mb.stillNothing) + ' still ' + pl(mb.stillNothing, 'has', 'have') + ' nothing to rest on'
        + (mb.failedAfter ? ' · ' + fc(mb.failedAfter) + ' had a run afterwards that failed' : ''),
      title: 'Statements a run could settle, with no test run before them in their piece of work. A statement that only says the work is done is not counted here.',
    }
    : { b: '—', of: '', bPText: REASONS.before, bNText: '', glyphs: 'none', second: 'none', textColor: '#A1A1AA' };
  const rr = { total: '—', rows: [{ glyph: '·', gc: '#82828C', text: REASONS.reruns }], title: REASONS.reruns };

  const listAt = (subset) => href('recordStatements', { project: at && at.project, range: R, agent: A, scope: P, subset });

  /* ---- where the proof stops: a link with no page behind it is not drawn ---- */
  const noList = out.st.actHref === 'Every-Statement.dc.html';
  const st = noList
    ? Object.assign({}, out.st, { actShow: false, actHint: 'Each of these is in its own piece of work. This page has no list of them across pieces of work.' })
    : out.st;

  return Object.assign({}, out, {
    live: true,
    banner: { show: false, dismiss: () => {} },
    codex,
    side: liveSide(D),
    hdr: Object.assign({}, out.hdr, {
      meta: out.hdr.meta.replace(' · computed on this machine', ' · counted on this machine'),
      exportCsv: () => {
        if (none) return;
        save({ text: exp.text, filename: exp.filename, mime: 'text/csv;charset=utf-8' });
        set({ toast: true, menu: false, focus: 'export' });
      },
      expOff: none,
      expColor: none ? '#82828C' : undefined,
      expWhy: none ? NO_EXPORT : EXPORT_TIP,
    }),
    toast: {
      show: S.toast && !none,
      file: 'Saved ' + exp.filename,
      wrap: true, /* the whole file name, never cut at the card's edge */
      rows: fc(exp.count) + ' ' + pl(exp.count, 'row', 'rows') + ', one per project and agent: the statements made, how many are proven, need a re-check, have no proof or are recorded, and the other counts on this page. An empty cell means not counted.',
      done: () => set({ toast: false }),
    },
    k: Object.assign({}, out.k, {
      /* C54 #7: each count opens the statements it counts, in the range, agent and project in view */
      nHref: listAt('all'),
      pHref: listAt('proven'),
      reHref: listAt(mv.served ? 'moved' : 'open'),
      pSub: mv.served ? fc(mv.reproven) + ' re-proven after a later edit' : 'Re-proving is not counted',
      /* C26 one question, one answer: "open" below is the statements the headline says need a re-check,
       * by construction (the engine's proof_moved is true for every one of them) */
      re: mv.served ? fc(mv.total) : fc(cur.r),
      reLabel: mv.served ? undefined : 'Need a re-check now',
      reSub: mv.served
        ? fc(mv.open) + ' open · ' + fc(mv.reproven) + ' re-proven' + (mv.failed ? ' · ' + fc(mv.failed) + ' failed' : '')
        : 'Earlier re-checks are not counted',
      reAria: mv.served
        ? 'Needed a re-check after a later edit: ' + fc(mv.total) + ' of ' + fc(cur.n) + '. ' + fc(mv.open) + ' still need one, ' + fc(mv.reproven) + ' were re-proven, ' + fc(mv.failed) + ' failed when re-run. List them'
        : 'Need a re-check now: ' + fc(cur.r) + ' of ' + fc(cur.n) + '. Earlier re-checks are ' + NOT_COUNTED + '. List them',
      med, medUnit, medSub, medTitle,
    }),
    ag: Object.assign({}, out.ag, { cc: agentLive(out.ag.cc, ccS, 'cc'), cx: agentLive(out.ag.cx, cxS, 'cx') }),
    st,
    br,
    rr,
  });
}

function liveVals(self, D) {
  const n = D.projects.length;
  const T = D.totals;
  const projWord = fc(n) + ' ' + pl(n, 'project', 'projects');
  const counted = fc(T.claims) + ' ' + pl(T.claims, 'statement', 'statements') + ' across ' + fc(T.threads) + ' ' + pl(T.threads, 'piece of work', 'pieces of work');
  return {
    isDemo: false,
    live: true,
    banner: { show: false, dismiss: () => {} },
    codex: liveCodex(self.props, self.state, (u) => self.setState(u)),
    side: liveSide(D),
    hdr: {
      meta: 'every project on this machine · ' + projWord,
      projLabel: n === 1 ? D.projects[0].name : 'All ' + fc(n),
      menuExpanded: 'false',
      toggleMenu: () => {},
      off: true,
      offColor: '#82828C',
      offWhy: LIVE_TEXT.off,
      projOl: 'none', projSh: 'none', expOl: 'none', expSh: 'none',
      exportCsv: () => {},
      chipShow: false, chipLabel: '', chipAria: '', clearAgent: () => {},
      lead: 'The record holds ' + counted + '. ' + LIVE_TEXT.off
    },
    ranges: D.windows.map((w, i) => ({
      label: w + ' days',
      pressed: 'false',
      disabled: true,
      bg: 'transparent',
      color: '#82828C',
      divider: i === 0 ? '0' : '1px solid #66666F',
      radius: i === 0 ? '5px 0 0 5px' : (i === D.windows.length - 1 ? '0 5px 5px 0' : '0'),
      ol: 'none', sh: 'none',
      pick: () => {}
    })),
    menu: { show: false, items: [] },
    toast: { show: false, file: '', rows: '', done: () => {} },
    has: false,
    empty: {
      show: true,
      title: LIVE_TEXT.title,
      body: 'This page counts, day by day and agent by agent, what was said and how far its proof reaches. The record holds ' + counted + ', but the read that counts them did not finish, so nothing is counted here and nothing is shown as zero. Reload the page to read it again.',
      actLabel: 'Reload this page',
      act: () => { if (typeof window !== 'undefined') window.location.reload(); }
    },
    foot: { text: counted + ' · ' + projWord + ' · not counted: the statements could not be read' }
  };
}
