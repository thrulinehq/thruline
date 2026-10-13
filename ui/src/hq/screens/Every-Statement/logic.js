/* Every-Statement.dc.html: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { recordingPill, storageSentence } from '../../data/project.js'
import { middle } from '../../data/paths.js'
import { fmtCount } from '../../data/count.js'
import { altitudes } from '../Change-Brief/altitude.js'
import { href as routeHref } from '../../routes.js'
import { isStep, stepLinker } from '../Step-View/link.js'
import { acceptedAfterWork } from '../../data/after-work.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

export default class Component extends DCLogic {
  state = { sel: '', view: 'table', groupBy: 'turn', menu: false, alsoOpen: false, sheet: '', checking: false, stopped: false,
    /* C30: the four filters, each one of the snapshot's own fields ('' = all), and which filter's menu is open */
    filter: { state: '', kind: '', agent: '', session: '' }, fmenu: '' };

  renderVals() {
    const s = this.state;
    /* every act on the screen closes an open filter menu, as it closes the Group menu */
    const go = (patch) => () => this.setState(Object.assign({ fmenu: '' }, patch));
    const Y = '#F2C94C';
    const T2 = '#A1A1AA';
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    const V = D.version.id;

    /* ---------- facts, indexed ---------- */
    const runById = (id) => D.runs.find((x) => x.id === id) || null;
    const editById = (id) => D.edits.find((x) => x.id === id) || null;
    const versionIdx = (id) => D.versions.findIndex((x) => x.id === id);
    const versionsOlder = (id) => (D.versions.length - 1) - versionIdx(id);
    const sesIdx = (id) => D.sessions.findIndex((x) => x.id === id);
    const sessionN = (id) => 'session ' + (sesIdx(id) + 1);
    const andList = (xs) => (xs.length < 2 ? xs.join('') : xs.slice(0, -1).join(', ') + ' and ' + xs[xs.length - 1]);
    const agents = [];
    D.sessions.forEach((x) => { if (agents.indexOf(x.agent) < 0) agents.push(x.agent); });
    const CAPTURED = { thruline: 'captured by ThruLine', transcript: 'read from the agent’s own transcript' };
    /* verdict (store vocabulary) → the state words this artboard draws */
    const STATE = { not_proven: 'notproven', needs_recheck: 'recheck', recorded: 'recorded', in_progress: 'progress', proven: 'proven', contradicted: 'contradicted' };
    /* how far the chain gets: rests on a run that the code moved past, on nothing, or on what counts as done */
    const reasonOf = (p) => (!p.restsOn ? 'noRun' : p.restsOn.kind === 'criteria' ? 'decidedByCriteria'
      : (p.since && editById(p.since.editId) && runById(p.restsOn.runId)) ? 'changedSince' : runById(p.restsOn.runId) ? 'ranOn' : 'noRun');
    /* a step or time the record did not number reads as absent, never as a made-up one */
    const stepTxt = (n) => (n == null ? 'step not recorded' : 'step ' + n);
    /* C30 R-ROUTES: a receipt chip that names a step opens that step; one that names none (a check ThruLine ran,
     * what counts as done) is text, and carries no arrow */
    const stepTo = isDemo ? () => undefined : stepLinker(this.props.at, D.project && D.project.id);
    const arrow = (n) => (isDemo || isStep(n) ? ' ›' : '');
    const crits = D.criteria.lines;
    const notProvenCrits = crits.filter((c) => c.proof.verdict !== 'proven');
    const provenCrits = crits.length - notProvenCrits.length;
    const rc = D.recheck;
    const hasCheck = !!rc && !!rc.runner;
    const check = hasCheck ? {
      command: rc.runner + ' ' + rc.scope.join(' '),
      paths: rc.scope,
      cwd: D.project.worktree,
      reaches: rc.reaches,
      askOncePerProject: rc.permission === 'ask_once_per_project' || (!isDemo && !!(this.props.actions && this.props.actions.needsPermission)),
      runningForS: rc.elapsedS,
    } : { command: '', paths: [], cwd: D.project.worktree, reaches: { lines: [], statements: [] }, askOncePerProject: false, runningForS: null };
    /* live (C30 R-RUN): "Run it" opens the check sheet already running — the press
       is the consent — unless the server says it will not start one, and then
       the control is off with the server's own reason (UX rule 6) */
    const acts = (!isDemo && this.props.actions) || {};
    const canRun = isDemo || (!!acts.check && !acts.checkBlocked);
    const RUN_WHY = acts.checkBlocked ? acts.checkBlocked.reason + ' ' + acts.checkBlocked.enabler : 'This check cannot start from this screen.';
    /* a check of this Change going now: the record's, never this screen's own timer */
    const checking = isDemo ? s.checking : !!D.running;
    const stopped = isDemo ? s.stopped : !!D.stopped;
    const stoppedForS = isDemo ? check.runningForS : D.stopped && D.stopped.elapsedS;
    const openCheck = () => {
      if (isDemo) this.setState({ sheet: 'tests', menu: false });
      else if (canRun) acts.check();
      return canRun;
    };
    const reachesAll = crits.length > 0 && check.reaches.lines.length === crits.length && crits.every((c) => check.reaches.lines.indexOf(c.num) >= 0);
    const reachPhrase = reachesAll ? 'reaches all ' + fmtCount(crits.length) + ' lines' : check.reaches.lines.length ? 'reaches lines ' + check.reaches.lines.join(', ')
      : check.reaches.statements.length ? 'reaches ' + fmtCount(check.reaches.statements.length) + ' statement' + (check.reaches.statements.length === 1 ? '' : 's') : '';
    /* a command too long for one caption line is cut where a person can see it is cut; the sheet shows it whole */
    const brief = (t) => (t.length > 90 ? t.slice(0, 89) + '…' : t);
    const lookedAtAll = () => {
      const fin = D.runs.filter((x) => x.status === 'finished');
      const open = D.runs.filter((x) => x.status === 'no_result');
      let t = fmtCount(fin.length) + ' finished run' + (fin.length === 1 ? '' : 's') + (fin.length ? ' (' + fin.map((x) => stepTxt(x.step) + ' on ' + x.version).join(', ') + ')' : '');
      if (open.length) t += ' and ' + andList(open.map((x) => stepTxt(x.step))) + ', which ha' + (open.length === 1 ? 's' : 've') + ' no result yet';
      return t + '.';
    };
    const SO_NOTPROVEN = 'Not proven on ' + V + ' — claimed, and no proof found in the record for this version. The claim may be entirely true.';
    const whenText = (ver) => 'The last version ThruLine recorded before this was said is ' + ver + '. ThruLine doesn’t record the code at the moment it spoke.';
    const LINK_PHRASE = { person: 'You linked this yourself', accepted_checks: 'covered by the checks you accepted' };
    const BASIS_PHRASE = { linked_by_person: 'You linked this yourself.', accepted_checks: 'looked at because the checks you accepted cover this kind of thing', cited_by_agent: 'cited by the agent',
      latest_before: 'the latest test run before it was said', later_run: 'a later test run on this version', matched: 'matched by ThruLine' };
    const CHECK_KIND = { tests_pass: 'tests', lint_clean: 'lint' };
    const CHECK_NOUN = { tests: 'tests', lint: 'lint' };
    const CHECKER = { lint: 'linter' };
    const CHAIN = { changedSince: 'moved', noRun: 'nothing2', decidedByCriteria: 'ran2', ranOn: 'held', assessment: 'assessment' };
    const STOP_AT = { 2: '② RESTS ON', 3: '③ RAN ON', 4: '④ SINCE', 5: '⑤ RE-CHECKED', 6: '⑥ SO' };
    const tally = (run) => (run.passed == null ? (run.status !== 'finished' ? 'no result yet' : run.outcome ? run.outcome + (run.exitCode != null ? ', exit ' + run.exitCode : '') + ', tally not recorded' : run.exitCode == null ? 'finished, tally not recorded' : 'exit ' + run.exitCode)
      : fmtCount(run.passed) + ' passed, ' + fmtCount(run.failed) + ' failed');
    /* a run with no edit after it: how far its chain holds, and the words where it stops */
    const fails = (run) => run.status !== 'finished' || run.outcome === 'failed' || run.outcome === 'unknown' || (run.exitCode != null && run.exitCode !== 0) || (run.failed != null && run.failed > 0);
    const ranOnStop = (run) => (fails(run) ? 2 : run.version === V ? null : 5);
    /* a later run that did not hold (the verdict "contradicted"): the chain stops at ⑤ on it */
    const failRunOf = (p, run) => { const f = p.recheckRunId ? runById(p.recheckRunId) : null; return f && f !== run && fails(f) ? f : null; };
    const heldStop = (p, run) => (failRunOf(p, run) ? 5 : ranOnStop(run));
    const decidedLine = fmtCount(provenCrits) + ' of ' + fmtCount(crits.length);
    const restsRun = (p) => (p.restsOn && p.restsOn.kind === 'run' ? runById(p.restsOn.runId) : null);
    const basisOf = (run, p) => [run.exitCode == null ? null : 'exit ' + run.exitCode, BASIS_PHRASE[p.restsOn.basis] || null,
      run.capturedBy ? (CAPTURED[run.capturedBy] || 'captured by ' + run.capturedBy) : null].filter(Boolean).join(' · ');

    /* Every row, crit or claim, as the rack and the inspector read it. */
    const sharesWith = (run) => crits.filter((c) => restsRun(c.proof) === run).map((c) => String(c.num));
    const critRow = (c, i) => {
      const p = c.proof;
      const reason = c.recordedLine ? 'assessment' : reasonOf(p);
      const prev = i > 0 ? crits[i - 1] : null;
      const run = restsRun(p);
      const ed = p.since ? editById(p.since.editId) : null;
      let stopText = '';
      let l2 = '';
      let looked = '';
      if (reason === 'changedSince') {
        stopText = prev && run && restsRun(prev.proof) === run
          ? 'same run (step ' + run.step + ')'
          : 'nothing on ' + V + ' since step ' + ed.step;
        l2 = 'Step ' + run.step + ' passed on ' + run.version + (c.link === 'person' ? ', and you linked it to this line' : '') + '. ' + middle(ed.file, 40) + ' changed after that, and nothing has run on ' + V + '.';
      } else if (reason === 'assessment') {
        stopText = c.recordedLine;
        l2 = c.recordedLine;
      } else if (reason === 'noRun') {
        const miss = (p.misses || []).join(', ');
        stopText = miss ? 'no run has included ' + miss : 'no run bears on this line yet';
        l2 = miss ? 'No run has included ' + miss + ' for this line.' : 'No run bears on this line yet.';
        const seen = (p.lookedAt || []).map(runById).filter(Boolean);
        looked = seen.length ? seen.map((x) => stepTxt(x.step) + ' ran ' + x.scope.join(', ') + ' on ' + x.version).join('; ') + (miss ? '; it did not include ' + miss + '.' : '.') : '';
      } else if (reason === 'ranOn') {
        const at = heldStop(p, run);
        const f = failRunOf(p, run);
        stopText = f ? stepTxt(f.step) + ' did not hold' : at == null ? 'holds on ' + V : at === 2 ? stepTxt(run.step) + ' did not hold' : 'nothing on ' + V + ' since ' + stepTxt(run.step);
        l2 = stepTxt(run.step).replace(/^s/, 'S') + ' ' + (at === 2 ? 'did not hold: ' + tally(run) : 'held on ' + run.version) + (c.link === 'person' ? ', and you linked it to this line' : '') + '.'
          + (f ? ' ' + stepTxt(f.step).replace(/^s/, 'S') + ' ran later and did not hold: ' + tally(f) + '.' : '');
      }
      return {
        id: c.id, kind: 'crit', num: String(c.num), gate: !!c.gate, st: STATE[p.verdict] || 'notproven', chain: CHAIN[reason], act: hasCheck ? 'tests' : '',
        reach: check.reaches.lines.indexOf(c.num) >= 0,
        text: c.text,
        meta: c.recordedLine ? 'recorded assessment' : c.link ? LINK_PHRASE[c.link] : 'nothing linked to this line yet',
        stopAt: reason === 'assessment' && p.stopsAt == null ? STOP_AT[6] : reason === 'ranOn' ? STOP_AT[heldStop(p, run) || 6] : (STOP_AT[p.stopsAt] || STOP_AT[2]), stopText: stopText, stopN: reason === 'ranOn' ? heldStop(p, run) : p.stopsAt, failRun: reason === 'ranOn' ? failRunOf(p, run) : null,
        basis: run ? basisOf(run, p) : '',
        run: run, edit: ed,
        l2: l2,
        l2Title: ed && l2.includes(middle(ed.file, 40) + ' changed after that') && middle(ed.file, 40) !== ed.file ? ed.file : undefined, // the whole display path
        restsNone: 'No run bears on this line yet.',
        looked: looked,
        l4: p.verdict === 'proven' ? 'This line is satisfied on ' + V + '.' : 'Approval of ' + V + ' waits on this.',
        so: '',
      };
    };
    const claimRow = (q) => {
      const p = q.proof;
      const reason = reasonOf(p);
      const kind = CHECK_KIND[q.claimKind] || '';
      const run = restsRun(p);
      const ed = p.since ? editById(p.since.editId) : null;
      let stopText = '';
      let l2 = '';
      let l4 = 'Approval rests on what counts as done, not on this statement.';
      let looked = '';
      if (reason === 'changedSince') {
        const sh = sharesWith(run);
        stopText = sh.length ? 'shares step ' + run.step + ' with lines ' + sh.join(', ') : 'nothing on ' + V + ' since step ' + ed.step;
        l2 = 'The ' + (CHECK_NOUN[kind] || 'check') + ' passed on ' + run.version + '. ' + middle(ed.file, 40) + ' changed after that, and nothing has run on ' + V + '.';
        if (sh.length) l4 += ' It shares step ' + run.step + ' with lines ' + andList(sh) + '.';
      } else if (reason === 'noRun') {
        stopText = 'no run in the record backs this';
        l2 = 'No run in the record backs this. The only source is ' + q.agent + '’s own message (' + stepTxt(q.step) + ').';
        const ses = (p.lookedAtSessions || []).map((id) => String(sesIdx(id) + 1));
        looked = ses.length ? 'every command recorded in session' + (ses.length === 1 ? ' ' : 's ') + andList(ses) + '; none ran a' + (/^[aeiou]/.test(CHECKER[kind] || '') ? 'n ' : ' ') + (CHECKER[kind] || 'check') + '.' : '';
      } else if (reason === 'ranOn') {
        const at = heldStop(p, run);
        const f = failRunOf(p, run);
        stopText = f ? stepTxt(f.step) + ' did not hold' : at == null ? 'holds on ' + V : at === 2 ? stepTxt(run.step) + ' did not hold' : 'nothing on ' + V + ' since ' + stepTxt(run.step);
        l2 = stepTxt(run.step).replace(/^s/, 'S') + ' ' + (at === 2 ? 'did not hold: ' + tally(run) : 'held on ' + run.version) + '.'
          + (f ? ' ' + stepTxt(f.step).replace(/^s/, 'S') + ' ran later and did not hold: ' + tally(f) + '.' : '');
      } else {
        stopText = 'decided by what counts as done';
        l2 = 'The record can’t check “' + p.verb + '” by itself. What you accepted as done decides it: ' + decidedLine + ' are proven on ' + V + '.';
        l4 = 'Approval never rests on this sentence; it rests on what counts as done.';
      }
      return {
        id: q.id, kind: 'claim', agent: q.agent, ses: sessionN(q.sessionId), sesId: q.sessionId, step: stepTxt(q.step), stepN: q.step, time: q.time || '', q: q.text,
        st: STATE[p.verdict] || 'notproven', chain: CHAIN[reason], act: kind, verb: p.verb,
        reach: check.reaches.statements.indexOf(q.id) >= 0,
        stopAt: reason === 'ranOn' ? STOP_AT[heldStop(p, run) || 6] : (STOP_AT[p.stopsAt] || STOP_AT[2]), stopText: stopText, stopN: reason === 'ranOn' ? heldStop(p, run) : p.stopsAt, failRun: reason === 'ranOn' ? failRunOf(p, run) : null,
        basis: run ? basisOf(run, p) : '',
        run: run, edit: ed,
        l2: l2,
        l2Title: ed && l2.includes(middle(ed.file, 40) + ' changed after that') && middle(ed.file, 40) !== ed.file ? ed.file : undefined, // the whole display path
        when: q.versionBefore ? whenText(q.versionBefore) : 'ThruLine recorded no version before this was said.',
        restsNone: 'No ' + (CHECK_NOUN[kind] || 'check') + ' run in the record.',
        looked: looked,
        l4: l4,
        so: '',
      };
    };
    const ROWS = crits.map(critRow).concat(D.statements.map(claimRow));
    const olderOf = (r) => (r.run ? r.run.version : '');
    ROWS.forEach((r) => {
      if (r.st === 'recheck') r.so = 'Needs a re-check on ' + V + '. Its proof is for ' + olderOf(r) + ', an older version.';
      else if (r.st === 'progress') r.so = 'In progress on ' + V + ' — nothing has been checked against this line yet.';
      else if (r.st === 'notproven') r.so = SO_NOTPROVEN;
      else if (r.st === 'recorded') r.so = 'Recorded — what is here doesn’t decide it either way.';
      else if (r.st === 'proven') r.so = 'Proven on ' + V + '.';
    });
    /* Also said: a message no checkable statement came from. Listed, never counted. */
    const ALSO = D.messages.filter((m) => !D.statements.some((q) => q.messageId === m.id));
    const alsoId = ALSO.length ? ALSO[0].id : '';
    const isAlso = (id) => ALSO.some((m) => m.id === id);
    /* ---------- filters: State / Kind / Agent / Session, each by the row's own field ---------- */
    const F = Object.assign({ state: '', kind: '', agent: '', session: '' }, s.filter || {});
    const FIELD = { state: (r) => r.st, kind: (r) => r.kind, agent: (r) => (r.kind === 'claim' ? r.agent : null), session: (r) => (r.kind === 'claim' ? r.sesId : null) };
    const FKEYS = ['state', 'kind', 'agent', 'session'];
    const passes = (r, except) => FKEYS.every((k) => k === except || !F[k] || FIELD[k](r) === F[k]);
    const SHOWN = ROWS.filter((r) => passes(r));
    const filtered = FKEYS.some((k) => !!F[k]);
    const shown = (id) => SHOWN.some((x) => x.id === id);
    const selId = (s.sel && (shown(s.sel) || isAlso(s.sel))) ? s.sel : (shown(D.selected) ? D.selected : (SHOWN.length ? SHOWN[0].id : alsoId));

    /* ---------- state words: word + glyph + line/fill style ---------- */
    const ST = {
      notproven: { word: 'Not proven', glyph: '○', gc: '#A1A1AA', wc: '#EDEDEF', deco: 'dashed', decoC: '#A1A1AA',
        pill: 'border: 1px dashed #82828C; color: #EDEDEF;', pv: 'color: #A1A1AA' },
      recheck: { word: 'Needs a re-check', glyph: '◭', gc: Y, wc: Y, deco: 'solid', decoC: Y,
        pill: 'border: 1px solid rgba(242,201,76,.6); color: #F2C94C; background-image: repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px);', pv: 'color: #F2C94C' },
      recorded: { word: 'Recorded', glyph: '▪', gc: '#A1A1AA', wc: '#EDEDEF', deco: 'dotted', decoC: '#A1A1AA',
        pill: 'border: 1px solid #66666F; color: #EDEDEF;', pv: 'color: #A1A1AA' },
      progress: { word: 'In progress', glyph: '◐', gc: '#52A8FF', wc: '#52A8FF', deco: 'solid', decoC: '#52A8FF',
        pill: 'border: 1px solid rgba(82,168,255,.6); color: #52A8FF; background: rgba(82,168,255,.08);', pv: 'color: #52A8FF' },
      /* not drawn on this artboard: the spec's coral ✕ (DESIGN-SPEC §1.8) */
      contradicted: { word: 'Contradicted', glyph: '✕', gc: '#FF6369', wc: '#FF6369', deco: 'solid', decoC: '#FF6369',
        pill: 'border: 1px solid rgba(255,99,105,.6); color: #FF6369;', pv: 'color: #FF6369' },
      /* not drawn on this artboard; styled as What-Changed draws Proven, so live data never meets an undefined state */
      proven: { word: 'Proven', glyph: '●', gc: 'var(--tl-proven, #3DD68C)', wc: 'var(--tl-proven, #3DD68C)', deco: 'solid', decoC: 'var(--tl-proven, #3DD68C)',
        pill: 'border: 1px solid rgba(var(--tl-proven-rgb, 61, 214, 140), .6); color: var(--tl-proven, #3DD68C); background: rgba(var(--tl-proven-rgb, 61, 214, 140), .08);', pv: 'color: var(--tl-proven, #3DD68C)' },
    };
    const PILL = "display: inline-flex; align-items: center; gap: 7px; height: 28px; box-sizing: border-box; padding: 0 12px; border-radius: 999px; font-size: 14px; font-weight: 550; white-space: nowrap; align-self: flex-start; ";

    const byId = {};
    ROWS.forEach((r) => { byId[r.id] = r; });
    const reached = (r) => checking && r.reach;

    /* ---------- mini chain: six slots, 12 px, one drawing everywhere ---------- */
    const MS = {
      hold: { ch: '', s: 'display: block; width: 10px; height: 10px; border-radius: 50%; background: #EDEDEF' },
      moved: { ch: '◭', s: 'display: block; font-size: 13px; line-height: 14px; color: #F2C94C' },
      nothing: { ch: '', s: 'display: block; width: 10px; height: 10px; box-sizing: border-box; border-radius: 50%; border: 1px dashed #A1A1AA' },
      ran: { ch: '', s: 'display: block; width: 7px; height: 7px; background: #A1A1AA' },
      nr: { ch: '', s: 'display: block; width: 10px; height: 0; border-top: 1px dashed #66666F' },
      checking: { ch: '', s: 'display: block; width: 10px; height: 10px; box-sizing: border-box; border-radius: 50%; border: 2px solid #2A2A30; border-top-color: #A1A1AA; animation: tl-spin 1s linear infinite' },
      progress: { ch: '◐', s: 'display: block; font-size: 13px; line-height: 14px; color: #52A8FF' },
    };
    const MV = { recheck: MS.moved, notproven: MS.nothing, recorded: MS.ran, progress: MS.progress, proven: MS.hold, contradicted: { ch: '✕', s: 'display: block; font-size: 12px; line-height: 14px; color: #FF6369' } };
    const SLOTBOX = 'flex: 0 0 14px; width: 14px; height: 14px; display: inline-flex; align-items: center; justify-content: center';
    const CONNBOX = 'flex: 0 0 16px; width: 16px; height: 14px; display: inline-flex; align-items: center';
    const holdConn = (lit) => lit
      ? 'display: block; width: 100%; height: 2px; background: #FF7A29; box-shadow: 0 0 5px rgba(255,154,82,.8)'
      : 'display: block; width: 100%; height: 1px; background: #66666F';
    const HATCH = 'display: block; width: 100%; height: 3px; background-image: repeating-linear-gradient(45deg, #F2C94C 0 2px, transparent 2px 4px)';
    const caps = (c) => 'display: block; width: 16px; height: 10px; background: linear-gradient(90deg, transparent 0 2px, ' + c + ' 2px 4px, transparent 4px 12px, ' + c + ' 12px 14px, transparent 14px)';
    const NONE = 'display: block; width: 0; height: 0';

    const chainCells = (r, lit, chk) => {
      const stopLink = chk ? MS.checking : MS.nothing;
      let slots;
      let conns;
      if (r.chain === 'moved') {
        slots = [MS.hold, MS.hold, MS.hold, MS.moved, stopLink, MV[r.st]];
        conns = [holdConn(lit), holdConn(lit), HATCH, caps(Y), NONE];
      } else if (r.chain === 'nothing2') {
        slots = [MS.hold, stopLink, MS.nr, MS.nr, MS.nr, MV[r.st]];
        conns = [caps(T2), NONE, NONE, NONE, NONE];
      } else if (r.chain === 'held') {
        /* rests on a run with no edit after it: links before the stop hold, the stop is open, the rest not reached */
        const stop = r.stopN || 6;
        slots = [1, 2, 3, 4, 5].map((k) => (k < stop ? MS.hold : k === stop ? stopLink : MS.nr)).concat([MV[r.st]]);
        conns = [1, 2, 3, 4, 5].map((k) => (k + 1 < stop ? holdConn(lit) : k + 1 === stop ? caps(T2) : NONE));
      } else if (r.chain === 'assessment') {
        slots = [MS.hold, MS.ran, MS.nr, MS.nr, MS.nr, MV[r.st]];
        conns = [holdConn(lit), NONE, NONE, NONE, NONE];
      } else {
        slots = [MS.hold, MS.ran, MS.nr, MS.nr, MS.nr, MV[r.st]];
        conns = [caps(T2), NONE, NONE, NONE, NONE];
      }
      const out = [];
      slots.forEach((sl, i) => {
        out.push({ box: SLOTBOX, s: sl.s, ch: sl.ch });
        if (i < 5) out.push({ box: CONNBOX, s: conns[i], ch: '' });
      });
      return out;
    };
    const chainLabel = (r, chk) => {
      const first = r.kind === 'crit' ? 'accepted holds' : 'said holds';
      const v = ST[r.st].word;
      const found = chk ? 'checking now' : 'nothing found';
      if (r.chain === 'moved') return 'Proof chain: ' + first + ', rests on holds, ran on holds, since: moved at step ' + (r.edit ? r.edit.step : '') + ', proof stops at re-checked: ' + found + ', so: ' + v;
      if (r.chain === 'nothing2') return 'Proof chain: ' + first + ', proof stops at rests on: ' + found + ', links 3 to 5 not reached, so: ' + v;
      if (r.chain === 'held') return 'Proof chain: ' + first + ', rests on ' + stepTxt(r.run.step) + ', ' + (r.stopN ? 'proof stops at ' + r.stopAt.slice(2).toLowerCase() : 'every link holds') + ', so: ' + v;
      if (r.chain === 'assessment') return 'Recorded assessment: ' + first + ', ThruLine assessed this accepted line, so: ' + v;
      return 'Proof chain: ' + first + ', proof stops at rests on: decided by what counts as done, links 3 to 5 not reached, so: ' + v;
    };

    /* ---------- rack rows ---------- */
    const rowVM = (r) => {
      const sel = selId === r.id;
      const stt = ST[r.st];
      const chk = reached(r);
      const crit = r.kind === 'crit';
      return {
        rowStyle: 'display: grid; grid-template-columns: minmax(0, 1fr) 56px; column-gap: 16px; align-items: center; min-height: 52px; padding: 0 24px; box-sizing: border-box;'
          + (sel ? ' background: rgba(255,122,41,.07); box-shadow: inset 2px 0 0 #FF7A29;' : ''),
        pressed: sel ? 'true' : 'false',
        pick: go({ sel: r.id, menu: false }),
        glyph: stt.glyph,
        glyphStyle: 'font-size: 14px; line-height: 1; text-align: center; color: ' + stt.gc,
        num: crit ? r.num : '',
        numStyle: crit ? "flex: 0 0 auto; font: 500 12.5px 'Geist Mono', monospace; color: #82828C; font-variant-numeric: tabular-nums" : 'display: none',
        gate: !!r.gate,
        qOpen: crit ? '' : '❝',
        qClose: crit ? '' : '❞',
        text: crit ? r.text : r.q,
        isClaim: !crit,
        agentChip: crit ? '' : r.agent + ' · ' + r.ses,
        word: stt.word,
        wordStyle: 'flex: 0 0 auto; font-weight: 500; color: ' + stt.wc,
        meta: crit ? r.meta : r.step + ' · ' + r.time,
        metaStyle: crit
          ? 'min-width: 0; overflow: hidden; text-overflow: ellipsis; color: #A1A1AA'
          : "font-family: 'Geist Mono', monospace; font-size: 12px; color: #82828C",
        cells: chainCells(r, sel, chk),
        chainLabel: chainLabel(r, chk),
        stopAt: r.stopAt,
        stopText: chk ? 'checking now · nothing settled yet' : r.stopText,
        stopTextStyle: 'font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: ' + (chk ? '#A1A1AA' : '#EDEDEF'),
        hasAct: !!r.act && !chk,
        actLabel: r.act === 'lint' ? 'Check it again: ' + r.q : 'Check it again on ' + V,
        act: () => {
          this.setState({ sel: r.id, menu: false });
          if (r.act === 'lint') this.setState({ sheet: 'lint' });
          else openCheck();
        },
        actStyle: 'height: 26px; padding: 0 8px; display: inline-flex; align-items: center; gap: 7px; background: #111113; border: 1px solid #66666F; border-radius: 6px; color: #EDEDEF; font: inherit; font-size: 12px; cursor: pointer; opacity: ' + (sel ? '1' : '0'),
      };
    };

    const RANK = { contradicted: -1, notproven: 0, recheck: 1, recorded: 2, progress: 3, proven: 4 };
    const byRank = (xs) => xs.map((x, i) => [x, i]).sort((p, q) => (RANK[p[0].st] - RANK[q[0].st]) || (p[1] - q[1])).map((p) => p[0].id);
    const critRows = SHOWN.filter((x) => x.kind === 'crit');
    const claimRows = SHOWN.filter((x) => x.kind === 'claim');
    const perAgent = agents.map((ag) => ag + ' ' + fmtCount(claimRows.filter((x) => x.agent === ag).length)).join(' · ');
    const recheckOlder = (() => { const x = ROWS.find((y) => y.st === 'recheck' && y.run); return x ? x.run.version : ''; })();
    const TURN = [
      { title: 'Needs a check', ids: byRank(critRows), glyph: '', gs: 'display: none',
        caption: 'Approval of ' + V + ' waits on these' + (reachesAll ? ' · one check reaches all ' + fmtCount(crits.length) : '') },
      { title: 'What the agents said', ids: byRank(claimRows), glyph: '', gs: 'display: none',
        caption: 'Approval rests on what counts as done, not on these · ' + perAgent },
    ].filter((g) => g.ids.length);
    const BYSTATE = [
      { st: 'contradicted', title: 'Contradicted', glyph: '✕', gs: 'font-size: 13px; color: #FF6369',
        caption: 'a later run in the record did not hold' },
      { st: 'notproven', title: 'Not proven', glyph: '○', gs: 'font-size: 13px; color: #A1A1AA',
        caption: 'nothing in the record shows them on ' + V },
      { st: 'recheck', title: 'Needs a re-check', glyph: '◭', gs: 'font-size: 13px; color: #F2C94C',
        caption: 'their proof is for ' + recheckOlder + ', an older version' },
      { st: 'recorded', title: 'Recorded', glyph: '▪', gs: 'font-size: 13px; color: #A1A1AA',
        caption: 'what is here doesn’t decide them either way' },
      { st: 'progress', title: 'In progress', glyph: '◐', gs: 'font-size: 13px; color: #52A8FF',
        caption: 'nothing has been checked against it yet' },
      { st: 'proven', title: 'Proven', glyph: '●', gs: 'font-size: 13px; color: var(--tl-proven, #3DD68C)',
        caption: 'proven on ' + V },
    ].map((g) => Object.assign(g, { ids: SHOWN.filter((x) => x.st === g.st).map((x) => x.id) })).filter((g) => g.ids.length);
    const byState = s.groupBy === 'state';
    const visible = [];
    /* the filters' words, and what is said when nothing matches them (never an empty list that says nothing) */
    const KIND_WORD = { crit: 'What counts as done', claim: 'What the agents said' };
    const sesLabel = (id) => { const i = sesIdx(id); const ag = i >= 0 ? D.sessions[i].agent : ''; return 'Session ' + (i + 1) + (ag ? ' · ' + ag : ''); };
    const valueWord = { state: (v) => (ST[v] ? ST[v].word : v), kind: (v) => KIND_WORD[v] || v, agent: (v) => v, session: (v) => sesLabel(v) };
    const FNAME = { state: 'State', kind: 'Kind', agent: 'Agent', session: 'Session' };
    const activeWords = FKEYS.filter((k) => F[k]).map((k) => FNAME[k] + ': ' + valueWord[k](F[k]));
    const NONE_MATCH = filtered && !SHOWN.length ? [{ title: 'Nothing matches these filters', ids: [], glyph: '', gs: 'display: none',
      caption: activeWords.join(' · ') + ' · pick All in a filter to see every statement' }] : [];
    const groups = (byState ? BYSTATE : TURN).concat(NONE_MATCH).map((g) => {
      g.ids.forEach((id) => visible.push(id));
      return {
        title: g.title, count: fmtCount(g.ids.length), caption: g.caption, glyph: g.glyph, gs: g.gs,
        aria: g.title + ', ' + fmtCount(g.ids.length),
        rows: g.ids.map((id) => rowVM(byId[id])),
      };
    });
    if (s.alsoOpen) ALSO.forEach((m) => visible.push(m.id));

    /* ---------- proof strip (accepted order) ---------- */
    const segs = crits.map((c) => c.id).map((id) => {
      const r = byId[id];
      const stt = ST[r.st];
      const sel = selId === id;
      const skin = r.st === 'recheck'
        ? 'border: 1px solid rgba(242,201,76,.6); background-color: #111113; background-image: repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px);'
        : 'border: 1px solid rgba(82,168,255,.6); background-color: #111113;';
      return {
        num: r.num, glyph: stt.glyph, gs: 'color: ' + stt.gc,
        label: 'Line ' + r.num + ': ' + stt.word,
        pressed: sel ? 'true' : 'false',
        pick: go({ sel: id, menu: false }),
        style: "height: 28px; min-width: 48px; box-sizing: border-box; padding: 0 10px; display: inline-flex; align-items: center; justify-content: center; gap: 7px; border-radius: 5px; font: 500 12.5px 'Geist Mono', monospace; color: #EDEDEF; cursor: pointer; " + skin
          + (sel ? ' box-shadow: 0 0 0 1px #FF7A29, 0 0 12px rgba(255,122,41,.35);' : ''),
      };
    });

    /* ---------- inspector: the Note and the six links ---------- */
    const EBI = "font-size: 11px; font-weight: 550; letter-spacing: .06em; text-transform: uppercase; color: #A1A1AA; white-space: nowrap";
    const EBI_NR = "font-size: 11px; font-weight: 550; letter-spacing: .06em; text-transform: uppercase; color: #A1A1AA; white-space: nowrap";
    const LINE = 'font-size: 13.5px; line-height: 1.45; color: #EDEDEF; text-wrap: pretty';
    const LINE_MONO = "font-family: 'Geist Mono', monospace; font-size: 12.5px; line-height: 1.5; color: #EDEDEF";
    const SUB = 'font-size: 12.5px; line-height: 1.45; color: #A1A1AA; text-wrap: pretty';
    const PAD = 'display: flex; flex-direction: column; gap: 4px; min-width: 0; padding-bottom: 14px';
    const PAD_NR = 'display: flex; flex-direction: column; gap: 0; min-width: 0; padding-bottom: 8px';
    const CAP = (c) => c ? 'display: block; flex: 0 0 2px; width: 12px; height: 2px; background: ' + c : 'display: none';
    const CAPTOP = (c) => c ? 'display: block; flex: 0 0 6px; width: 12px; height: 6px; box-sizing: border-box; border-top: 2px solid ' + c : 'display: none';
    const RAIL = {
      lit: 'display: block; flex: 1 1 auto; width: 2px; min-height: 8px; background: #FF7A29; box-shadow: 0 0 6px rgba(255,154,82,.6)',
      hatch: 'display: block; flex: 1 1 auto; width: 3px; min-height: 8px; background-image: repeating-linear-gradient(45deg, #F2C94C 0 2px, transparent 2px 4px)',
      none: 'display: block; flex: 1 1 auto; width: 0',
    };
    const NB = 'flex: 0 0 18px; width: 18px; height: 18px; box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; line-height: 1; ';
    const N = {
      hold: (n) => ({ ch: n, s: NB + "border-radius: 50%; background: #EDEDEF; color: #000000; font: 600 11px 'Geist Mono', monospace" }),
      moved: { ch: '◭', s: NB + 'color: #F2C94C; font-size: 15px' },
      nothing: { ch: '', s: NB + 'border-radius: 50%; border: 1.5px dashed #82828C' },
      ran: { ch: '▪', s: NB + 'color: #A1A1AA; font-size: 16px' },
      nr: (n) => ({ ch: n, s: NB + "border-radius: 50%; border: 1px solid #2A2A30; color: #A1A1AA; font: 500 11px 'Geist Mono', monospace" }),
      checking: { ch: '', s: NB + 'border-radius: 50%; border: 2px solid #2A2A30; border-top-color: #A1A1AA; animation: tl-spin 1s linear infinite' },
      progress: { ch: '◐', s: NB + 'color: #52A8FF; font-size: 15px' },
    };
    const VN = { recheck: N.moved, notproven: N.nothing, recorded: N.ran, progress: N.progress,
      proven: { ch: '●', s: NB + 'color: var(--tl-proven, #3DD68C); font-size: 15px' }, contradicted: { ch: '✕', s: NB + 'color: #FF6369; font-size: 14px' } };

    const P = (o) => Object.assign({
      isPlate: true, isStop: false,
      capTop: CAPTOP(''), node: { ch: '', s: '' }, rail: RAIL.none, cap: CAP(''),
      eyebrow: '', ebStyle: EBI, padStyle: PAD,
      hasChip: false, chipKind: '', chip: '', href: undefined, hasLink: false, hasRight: false, right: '',
      hasLine: true, lineStyle: LINE, pre: '', qo: '', q: '', qc: '', post: '',
      hasSub: false, sub: '', subStyle: SUB,
      panelStyle: '', hasText: false, text: '', hasLooked: false, looked: '',
      hasAct: false, act: () => {}, actNote: '',
    }, o);
    const openSel = () => {
      if (byId[selId] && byId[selId].act === 'lint') this.setState({ sheet: 'lint', menu: false });
      else openCheck();
    };
    const STOP = (color, text, looked, actNote) => P({
      isPlate: false, isStop: true,
      panelStyle: 'display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border: 1px dashed ' + color + '; border-radius: 8px',
      hasText: !!text, text: text || '', hasLooked: !!looked, looked: looked || '',
      hasAct: !!actNote, act: openSel, actNote: actNote || '',
    });
    const notReached = (n, label) => P({ node: N.nr(n), eyebrow: label, ebStyle: EBI_NR, padStyle: PAD_NR, hasLine: false, hasRight: true, right: 'not reached' });
    const firstLink = (r, rail, cap) => (r.kind === 'crit'
      ? P({ node: N.hold('1'), rail: rail, cap: cap, eyebrow: '① Accepted', pre: 'You accepted this at ' + D.criteria.acceptedAt + (acceptedAfterWork(D.criteria.acceptedTs, D.lastEditTs, D.criteria.acceptedAt, D.lastEditAt) ? ', after the work' : '') + ' (the agent’s last edit was ' + D.lastEditAt + ').', hasChip: true, chipKind: 'Accepted', chip: D.criteria.version + (isDemo ? ' ›' : '') })
      : P({ node: N.hold('1'), rail: rail, cap: cap, eyebrow: '① Said', pre: r.agent + ' wrote ', qo: '❝', q: r.q, qc: '❞', post: (r.time ? ' · ' + r.time : '') + (sesIdx(r.sesId) > 0 ? ' (' + r.ses + ')' : ''), hasSub: true, sub: 'The agent’s words, as recorded. Not proof by themselves.', hasChip: true, chipKind: 'Said', chip: r.step + arrow(r.stepN), href: stepTo(r.stepN) }));

    const ranOnPlate = (run, rail) => {
      const n = versionsOlder(run.version);
      const vr = D.versions[versionIdx(run.version)];
      const rel = versionIdx(run.version) < 0 ? 'a version ThruLine did not record' : n === 0 ? 'same version' : fmtCount(n) + ' version' + (n === 1 ? '' : 's') + ' older than ' + V;
      return P({ node: N.hold('3'), rail: rail, eyebrow: '③ Ran on', pre: 'version ' + run.version + ' · ' + rel,
        hasChip: !!vr && vr.recordedAtStep != null, chipKind: 'Version', chip: vr ? stepTxt(vr.recordedAtStep) + arrow(vr.recordedAtStep) : '', href: vr ? stepTo(vr.recordedAtStep) : undefined });
    };
    /* The record a verdict turns on (C30 R-ROUTES "How decided"): the edit that moved the proof; else the run that
     * did not hold, or the run it rests on. A verdict decided by what counts as done turns on no single step. */
    const decidedAt = (r) => (r.chain === 'moved' ? (r.edit || {}).step : r.chain === 'held' ? (r.stopN === 5 && r.failRun ? r.failRun : r.run || {}).step : undefined);
    const itemsFor = (r) => {
      const chk = reached(r);
      const items = [];
      if (r.chain === 'moved') {
        items.push(firstLink(r, RAIL.lit, CAP('')));
        items.push(P({ node: N.hold('2'), rail: RAIL.lit, eyebrow: '② Rests on', lineStyle: LINE_MONO, pre: r.run.command + ': ' + tally(r.run), hasSub: !!r.basis, sub: r.basis, hasChip: true, chipKind: 'Run', chip: stepTxt(r.run.step) + arrow(r.run.step), href: stepTo(r.run.step) }));
        items.push(ranOnPlate(r.run, RAIL.hatch));
        items.push(P({ node: N.moved, rail: RAIL.hatch, cap: CAP(Y), eyebrow: '④ Since',
          pre: r.edit.file + ' changed at ' + stepTxt(r.edit.step) + (r.edit.time ? ' (' + r.edit.time + ')' : '') + ', after the run' + (r.kind === 'crit' ? '' : ' and after the claim') + '. The code is now ' + V + '.',
          hasSub: r.kind === 'crit' && !r.run.declaresPaths, sub: 'This check doesn’t say which files it depends on, so ThruLine treats it as depending on everything this Change covers.',
          hasChip: true, chipKind: 'Edit', chip: stepTxt(r.edit.step) + arrow(r.edit.step), href: stepTo(r.edit.step) }));
        items.push(STOP(Y, 'Nothing on ' + V + ' has run with ' + r.edit.file + ' in scope.', lookedAtAll(), chk ? '' : 'supplies the missing link ⑤ on ' + V));
        items.push(chk
          ? P({ capTop: CAPTOP(Y), node: N.checking, eyebrow: '⑤ Re-checked', pre: 'This check is running now. Nothing is settled until it finishes.', hasSub: true, sub: check.command + ' on ' + V })
          : P({ capTop: CAPTOP(Y), node: N.nothing, eyebrow: '⑤ Re-checked', pre: 'Nothing on ' + V + ' yet.' }));
      } else if (r.chain === 'assessment') {
        items.push(firstLink(r, RAIL.lit, CAP('')));
        items.push(P({ node: N.ran, eyebrow: '② Assessed', pre: r.l2 }));
        items.push(notReached('3', '③ Ran on'));
        items.push(notReached('4', '④ Since'));
        items.push(notReached('5', '⑤ Re-checked'));
      } else if (r.chain === 'nothing2') {
        items.push(firstLink(r, RAIL.lit, CAP(T2)));
        items.push(STOP(T2, '', r.looked, chk ? '' : (r.act === 'lint' ? 'no lint command is in the record yet' : 'supplies the missing link ② on ' + V)));
        items.push(chk
          ? P({ capTop: CAPTOP(T2), node: N.checking, eyebrow: '② Rests on', pre: 'This check is running now. Nothing is settled until it finishes.' })
          : P({ capTop: CAPTOP(T2), node: N.nothing, eyebrow: '② Rests on', pre: r.restsNone }));
        items.push(notReached('3', '③ Ran on'));
        items.push(notReached('4', '④ Since'));
        items.push(notReached('5', '⑤ Re-checked'));
      } else if (r.chain === 'held') {
        const stop = r.stopN;
        items.push(firstLink(r, RAIL.lit, CAP('')));
        if (stop === 2) {
          items.push(STOP(T2, stepTxt(r.run.step).replace(/^s/, 'S') + ' did not hold: ' + tally(r.run) + '.', '', ''));
          items.push(P({ capTop: CAPTOP(T2), node: N.nothing, eyebrow: '② Rests on', lineStyle: LINE_MONO, pre: r.run.command + ': ' + tally(r.run), hasSub: !!r.basis, sub: r.basis, hasChip: true, chipKind: 'Run', chip: stepTxt(r.run.step) + arrow(r.run.step), href: stepTo(r.run.step) }));
          items.push(notReached('3', '③ Ran on'));
          items.push(notReached('4', '④ Since'));
          items.push(notReached('5', '⑤ Re-checked'));
        } else {
          items.push(P({ node: N.hold('2'), rail: RAIL.lit, eyebrow: '② Rests on', lineStyle: LINE_MONO, pre: r.run.command + ': ' + tally(r.run), hasSub: !!r.basis, sub: r.basis, hasChip: true, chipKind: 'Run', chip: stepTxt(r.run.step) + arrow(r.run.step), href: stepTo(r.run.step) }));
          items.push(ranOnPlate(r.run, RAIL.lit));
          items.push(P({ node: N.hold('4'), rail: RAIL.lit, eyebrow: '④ Since', pre: 'Nothing in the record changed after this run.' }));
          items.push(stop === 5
            ? P({ node: N.nothing, eyebrow: '⑤ Re-checked', pre: r.failRun ? stepTxt(r.failRun.step).replace(/^s/, 'S') + ' ran again and did not hold: ' + tally(r.failRun) + '.' : 'Nothing on ' + V + ' yet.',
              hasChip: !!r.failRun, chipKind: 'Run', chip: r.failRun ? stepTxt(r.failRun.step) + arrow(r.failRun.step) : '', href: r.failRun ? stepTo(r.failRun.step) : undefined })
            : P({ node: N.hold('5'), rail: RAIL.lit, eyebrow: '⑤ Re-checked', pre: 'Not needed: the run is on ' + V + '.' }));
        }
      } else {
        items.push(firstLink(r, RAIL.lit, CAP(T2)));
        items.push(STOP(T2, 'Nothing in the record can check “' + r.verb + '” on its own.', '', ''));
        items.push(P({ capTop: CAPTOP(T2), node: N.ran, eyebrow: '② Rests on', pre: 'Decided by what counts as done: ' + decidedLine + ' proven on ' + V + '.', hasLink: true }));
        items.push(notReached('3', '③ Ran on'));
        items.push(notReached('4', '④ Since'));
        items.push(notReached('5', '⑤ Re-checked'));
      }
      items.push(P({ node: VN[r.st], eyebrow: '⑥ So', pre: r.so, padStyle: 'display: flex; flex-direction: column; gap: 4px; min-width: 0; padding-bottom: 0', hasChip: isDemo || !!stepTo(decidedAt(r)), chipKind: 'How this was decided', chip: '›', href: stepTo(decidedAt(r)) }));
      return items;
    };

    /* The agent's message each statement came from, as quoted runs: statements in
     * the same sentence ("…; …") share one pair of quote marks, separate sentences
     * get their own. Read from the message text, in the order it was said. */
    const fragsFor = (id) => {
      const q0 = D.statements.find((x) => x.id === id);
      const m = q0 && D.messages.find((x) => x.id === q0.messageId);
      if (!m) return [{ id: id, qo: '❝', qc: '❞', tail: '' }];
      const mine = D.statements.filter((x) => x.messageId === m.id && byId[x.id])
        .map((x) => ({ x: x, at: m.text.indexOf(x.text) })).sort((p, q) => p.at - q.at);
      const all = mine.map((e, k) => {
        const prev = mine[k - 1];
        const next = mine[k + 1];
        const joinPrev = prev && /^\s*;\s*$/.test(m.text.slice(prev.at + prev.x.text.length, e.at));
        const joinNext = next && /^\s*;\s*$/.test(m.text.slice(e.at + e.x.text.length, next.at));
        return { id: e.x.id, qo: joinPrev ? '' : '❝', qc: joinNext ? '' : '❞', tail: joinNext ? ';' : '' };
      });
      /* a long message: the statement and one on each side, with … where the message goes on (never cut silently) */
      if (all.length <= 4) return all;
      const at = all.findIndex((f) => f.id === id);
      const lo = Math.max(0, at - 1), hi = Math.min(all.length, at + 2);
      return (lo > 0 ? [{ more: lo }] : []).concat(all.slice(lo, hi)).concat(hi < all.length ? [{ more: all.length - hi }] : []);
    };
    const ASK = check.askOncePerProject ? 'asks once for this project' : '';
    const runCaption = (withReach) => [brief(check.command), 'on ' + V].concat(withReach && reachPhrase ? [reachPhrase] : []).concat(ASK ? [ASK] : []).join(' · ');

    const base = {
      eyebrow: '', num: '', subject: '', subMeta: '', saidBy: '', frags: [],
      isCrit: false, isQuote: false, isAlso: false, hasNote: false, hasChain: false,
      pillStyle: '', pillGs: '', pillGlyph: '', pillWord: '', pillVs: '',
      l2: '', hasWhen: false, when: '', l3Run: false, l3Stop: false, l3None: false, l3caption: '', l4: '',
      chainNote: '', chainAria: '', items: [],
    };
    let insp;
    const r = byId[selId];
    const also = ALSO.find((x) => x.id === selId) || ALSO[0];
    if (!r && also) {
      insp = Object.assign({}, base, {
        eyebrow: 'Also said · not checked',
        isQuote: true, isAlso: true,
        saidBy: also.agent + ' said',
        frags: [{ qo: '❝', qc: '❞', tail: '', text: also.text, inside: isDemo ? undefined : true, pressed: 'true', hint: 'No rule applies to this sentence', pick: go({ sel: also.id }),
          style: 'display: inline; background: transparent; border: 0; padding: 0; font: inherit; color: #EDEDEF; text-align: left; cursor: default' }],
        subMeta: also.agent + ' · ' + sessionN(also.sessionId) + ' · step ' + also.step,
      });
    } else if (!r) {
      insp = Object.assign({}, base);
    } else {
      const stt = ST[r.st];
      const crit = r.kind === 'crit';
      const chk = reached(r);
      const frags = crit ? [] : fragsFor(r.id).map((f) => {
        if (f.more) return { qo: '', qc: '', tail: '', text: '… ' + f.more + ' more in this message', pressed: 'false', hint: '', pick: () => {},
          style: 'display: inline; background: transparent; border: 0; padding: 0; font: inherit; color: #82828C; text-align: left; cursor: default' };
        const fr = byId[f.id];
        const fst = ST[fr.st];
        const on = f.id === r.id;
        return {
          qo: f.qo, qc: f.qc, tail: f.tail, text: fr.q,
          /* live: a button wraps as one box, so a long quote pushed ❝ and ❞ onto lines of their own;
             the marks go inside it, beside the first and last words (C26) */
          inside: isDemo ? undefined : true,
          pressed: on ? 'true' : 'false',
          hint: fst.word + ' · version ' + V,
          pick: go({ sel: f.id, menu: false }),
          style: 'display: inline; background: transparent; border: 0; padding: 0; font: inherit; text-align: left; cursor: pointer; color: ' + (on ? '#EDEDEF' : '#A1A1AA')
            + '; text-decoration-line: underline; text-decoration-style: ' + fst.deco + '; text-decoration-color: ' + (on ? fst.decoC : '#66666F')
            + '; text-decoration-thickness: ' + (on ? '2px' : '1px') + '; text-underline-offset: 5px',
        };
      });
      insp = Object.assign({}, base, {
        eyebrow: crit ? ('What counts as done · line ' + r.num) : 'Statement · said by the agent',
        num: crit ? r.num : '',
        subject: crit ? r.text : '',
        subMeta: crit ? ((r.gate ? 'gate · ' : '') + 'accepted by ' + D.criteria.acceptedBy + ' · ' + D.criteria.acceptedAt + ' · what counts as done ' + D.criteria.version) : (r.agent + ' · ' + r.ses + ' · ' + r.step + ' · ' + r.time),
        saidBy: crit ? '' : r.agent + ' said',
        frags: frags,
        isCrit: crit, isQuote: !crit, hasNote: true, hasChain: true,
        pillStyle: PILL + stt.pill, pillGs: 'color: ' + stt.gc, pillGlyph: stt.glyph, pillWord: stt.word,
        pillVs: "font-family: 'Geist Mono', monospace; font-size: 12.5px; font-weight: 400; " + stt.pv,
        l2: r.l2, l2Title: r.l2Title,
        hasWhen: !crit, when: r.when || '',
        l3Run: !!r.act && !chk, l3Stop: chk, l3None: !r.act,
        l3caption: r.act === 'lint'
          ? 'a lint run on ' + V + ' · no lint command is in the record yet'
          : runCaption(crit),
        l4: r.l4,
        chainNote: r.chain === 'assessment' ? 'recorded assessment · open what counts as done for evidence' : r.chain === 'moved' ? 'link by link · stops at ⑤' : 'link by link · stops at ②',
        chainAria: r.chain === 'assessment' ? 'Recorded assessment for line ' + r.num : 'Proof chain, 6 links, for ' + (crit ? 'line ' + r.num : r.q),
        items: itemsFor(r),
      });
    }

    /* ---------- pager (rack order, j / k) ---------- */
    const n = visible.length;
    const at = visible.indexOf(selId);
    const pagerText = at >= 0 ? fmtCount(at + 1) + ' of ' + fmtCount(n) : '— of ' + fmtCount(n);
    const step = (d) => () => {
      const i = visible.indexOf(selId);
      const j = i < 0 ? 0 : (i + d + n) % n;
      this.setState({ sel: visible[j], menu: false });
    };

    const alsoSel = !!alsoId && selId === alsoId;
    const ALSO_ROW = 'display: grid; grid-template-columns: minmax(0, 1fr) 56px; column-gap: 16px; align-items: center; min-height: 52px; padding: 0 24px; box-sizing: border-box;';
    const alsoRows = ALSO.map((m) => ({
      text: m.text,
      meta: [m.agent, sesIdx(m.sessionId) >= 0 ? sessionN(m.sessionId) : null, m.step == null ? null : 'step ' + m.step].filter(Boolean).join(' · '),
      pressed: selId === m.id ? 'true' : 'false',
      pick: go({ sel: m.id, menu: false }),
      rowStyle: ALSO_ROW + (selId === m.id ? ' background: rgba(255,122,41,.07); box-shadow: inset 2px 0 0 #FF7A29;' : ''),
    }));
    /* where the proof stops, one group per link, most advanced first; one bar per line */
    const stops = [6, 5, 4, 3, 2].map((n) => ({ n, k: crits.filter((x) => x.proof.stopsAt === n).length }))
      .filter((g) => g.k > 0).map((g) => ({ label: STOP_AT[g.n], bars: Array.from({ length: g.k }, () => 1), count: '×' + fmtCount(g.k) }));

    /* ---------- content for the frame, the verdict strip and the sheets ---------- */
    const hasLines = crits.length > 0;
    const needLine = !hasLines ? 'What counts as done is not accepted yet' : fmtCount(notProvenCrits.length) + ' thing' + (notProvenCrits.length === 1 ? '' : 's') + ' still need' + (notProvenCrits.length === 1 ? 's' : '') + ' evidence';
    const movedCrits = crits.filter((x) => reasonOf(x.proof) === 'changedSince');
    const noRunCrits = crits.filter((x) => reasonOf(x.proof) === 'noRun');
    const why = movedCrits.length ? (() => {
      const run = restsRun(movedCrits[0].proof);
      const ed = editById(movedCrits[0].proof.since.editId);
      return { n: movedCrits.length, step: run.step, ranOn: run.version, path: ed.file, changedStep: ed.step };
    })() : { n: 0, step: '', ranOn: '', path: '', changedStep: '' };
    const countState = (st) => crits.filter((x) => x.proof.verdict === st).length;
    const countStop = (n) => crits.filter((x) => x.proof.stopsAt === n).length;
    const reachedQuotes = check.reaches.statements.map((id) => (D.statements.find((x) => x.id === id) || {}).text).filter(Boolean);
    const prevRun = movedCrits.length ? restsRun(movedCrits[0].proof) : null;
    const lintStatement = D.statements.find((x) => x.claimKind === 'lint_clean');
    const lines = check.reaches.lines.slice().sort((p, q) => p - q);
    const contiguous = lines.every((x, i) => i === 0 || x === lines[i - 1] + 1);
    const lineRange = lines.length > 1 && contiguous ? lines[0] + '–' + lines[lines.length - 1] : lines.join(', ');
    const fmtBytes = (b) => (b >= 1e9 ? (b / 1e9).toFixed(1) + ' GB' : b >= 1e6 ? (b / 1e6).toFixed(1) + ' MB' : Math.round(b / 1e3) + ' KB');
    const changeState = ST[STATE[D.change.state]] || ST.notproven;
    const content = {
      project: D.project.name,
      title: D.change.title,
      version: V,
      commit: D.version.commit ? '= commit ' + D.version.commit : '',
      recording: isDemo ? 'Recording' : recordingPill({ change: D.change }).text,
      readTitle: 'Read at ' + D.view.readAt + (D.view.readDate ? ' on ' + D.view.readDate : '') + (D.view.utcOffset ? ', UTC' + D.view.utcOffset : ''),
      readText: 'read ' + D.view.readAt + ' ↻',
      agentCount: F.agent ? '' : fmtCount(agents.length),
      sessionCount: F.session ? '' : fmtCount(D.sessions.length),
      foldNote: 'At 40 rows or more, chains that hold on ' + V + ' fold into one line, and where the proof stops leads.',
      stateGlyph: changeState.glyph,
      /* the canvas drew this pill for Needs a re-check only; the live product draws the state it has, in that state's own colours */
      statePill: isDemo ? undefined : 'flex: 0 0 auto; display: inline-flex; align-items: center; gap: 7px; height: 28px; box-sizing: border-box; padding: 0 12px; border-radius: 999px; font-size: 14px; font-weight: 550; white-space: nowrap; ' + changeState.pill,
      stateGlyphStyle: 'color: ' + changeState.gc,
      stateWord: changeState.word,
      onVersion: '· version ' + V,
      changeMeta: 'Change · ' + fmtCount(D.sessions.length) + ' session' + (D.sessions.length === 1 ? '' : 's') + ' · ' + agents.join(', '),
      headline: hasLines ? needLine + ' before ' + V + ' can be approved.' : 'What counts as done is not accepted yet, so nothing decides whether ' + V + ' can be approved.',
      hasWhy: why.n > 0,
      hasNoRun: noRunCrits.length > 0,
      noRunLead: (why.n > 0 ? '. ' : '') + 'No run has included ',
      whyPlain: hasLines ? '' : fmtCount(D.statements.length) + ' statement' + (D.statements.length === 1 ? '' : 's') + ' by the agents · ' + fmtCount(D.statements.filter((q) => q.proof.verdict === 'proven').length) + ' proven on ' + V + '.',
      hasLines: hasLines,
      noLines: '',
      hasBar: hasLines && !!D.criteria.acceptedAt && !!D.lastEditAt && acceptedAfterWork(D.criteria.acceptedTs, D.lastEditTs, D.criteria.acceptedAt, D.lastEditAt),
      runDisabled: canRun ? undefined : true,
      runOpacity: canRun ? undefined : '0.5',
      runWhy: canRun ? undefined : RUN_WHY,
      whyRest: fmtCount(why.n) + ' rest on ',
      whyStep: stepTxt(why.step),
      whyRanOn: why.ranOn,
      whyPath: why.path,
      whyChangedStep: stepTxt(why.changedStep),
      whyNoRun: noRunCrits.length ? (noRunCrits[0].proof.misses || []).join(', ') : '',
      whyNoRunLines: ' for line' + (noRunCrits.length === 1 ? ' ' : 's ') + andList(noRunCrits.map((x) => String(x.num))) + '.',
      tallyRecheck: fmtCount(countState('needs_recheck')) + ' need a re-check',
      tallyProgress: fmtCount(countState('in_progress')) + ' in progress',
      tallyProven: ' · ' + fmtCount(provenCrits) + ' proven on ',
      stopsRechecked: '×' + fmtCount(countStop(5)),
      stopsRestsOn: '×' + fmtCount(countStop(2)),
      waitsOn: hasLines ? ' waits on all ' + fmtCount(crits.length) + '.' : ' waits on what counts as done, which is not accepted yet.',
      barTitle: 'Bar set after the work: accepted ' + D.criteria.acceptedAt + '; the agent’s last edit was ' + D.lastEditAt + '.',
      barTimes: 'accepted ' + D.criteria.acceptedAt + '; last edit ' + D.lastEditAt,
      checkOn: brief(check.command) + ' on ' + V,
      hasStops: stops.length > 0,
      runningFor: check.runningForS == null ? '' : '· ' + check.runningForS + ' s',
      checkAria: 'Check it again on ' + V,
      checkTitle: 'Check it again · on ' + V,
      command: check.command,
      where: (check.cwd ? 'in ' + check.cwd + ' · ' : '') + 'on ' + V,
      pathsHead: check.paths.slice(0, -1).join(', '),
      pathsLast: check.paths.length ? check.paths[check.paths.length - 1] : '',
      reachLines: (lines.length ? 'Reaches lines ' + lineRange + (reachedQuotes.length ? ' and ' : '') : reachedQuotes.length ? 'Reaches ' : ''),
      reachQuote: reachedQuotes.join(' '),
      reachTail: (prevRun ? (prevRun.durationMs == null ? ' Step ' + prevRun.step + '’s duration was not recorded.' : ' Step ' + prevRun.step + ' took ' + Math.round(prevRun.durationMs / 1000) + ' s.') : '')
        + (ASK ? ' ThruLine asks once for this project before it runs anything here.' : '') + (canRun ? '' : ' ' + RUN_WHY),
      runOn: 'Run it on ' + V,
      lintAria: 'Check it again: ' + (lintStatement ? lintStatement.text.replace(/\.$/, '') : 'lint'),
      lintTitle: 'Check it again · lint on ' + V,
      rowVersion: 'version ' + V,
      alsoAria: 'Also said, not checked, ' + fmtCount(ALSO.length),
      alsoLabel: 'Also said — not checked · ' + fmtCount(ALSO.length),
      alsoText: ALSO.length ? ALSO[0].text : '',
      alsoMeta: ALSO.length ? ALSO[0].agent + ' · ' + sessionN(ALSO[0].sessionId) + ' · step ' + ALSO[0].step : '',
      alsoChip: (also && also.step != null ? 'step ' + also.step : 'said') + ' ›',
      storage: storageSentence(D.machine.recordsBytes),
    };
    const plural = (n, one, many) => fmtCount(n) + ' ' + (n === 1 ? one : many);
    const RANK_ORDER = ['contradicted', 'notproven', 'recheck', 'recorded', 'progress', 'proven'];
    const present = (k) => ROWS.map(FIELD[k]).filter((x) => x != null);
    const VALUES = {
      state: RANK_ORDER.filter((x) => present('state').indexOf(x) >= 0),
      kind: ['crit', 'claim'].filter((x) => present('kind').indexOf(x) >= 0),
      agent: agents.filter((x) => present('agent').indexOf(x) >= 0),
      session: D.sessions.map((x) => x.id).filter((x) => present('session').indexOf(x) >= 0),
    };
    const MENU_LABEL = { state: 'Filter by state', kind: 'Filter by kind', agent: 'Filter by agent', session: 'Filter by session' };
    const OPT = (on) => 'display: flex; align-items: flex-start; gap: 10px; width: 100%; box-sizing: border-box; padding: 8px 10px; background: ' + (on ? '#16161A' : 'transparent') + '; border: 0; border-radius: 5px; color: #EDEDEF; font: inherit; text-align: left; cursor: pointer';
    /* each option's count is the rows it would show, with the other filters as they are */
    const optionsOf = (k) => [''].concat(VALUES[k]).map((val) => {
      const n = ROWS.filter((r) => passes(r, k) && (!val || FIELD[k](r) === val)).length;
      const on = F[k] === val;
      return {
        label: val ? valueWord[k](val) : 'All', hint: plural(n, 'statement', 'statements'),
        checked: on ? 'true' : 'false', tick: on ? '✓' : '', style: OPT(on),
        pick: () => this.setState({ filter: Object.assign({}, F, { [k]: val }), fmenu: '', menu: false }),
      };
    });
    const flt = {};
    FKEYS.forEach((k) => {
      flt[k] = {
        label: F[k] ? FNAME[k] + ': ' + valueWord[k](F[k]) : FNAME[k],
        color: F[k] ? '#EDEDEF' : '#A1A1AA',
        open: s.fmenu === k,
        /* the menu hangs from its button; the wrapper is positioned only while the menu is open, so a
           closed filter paints exactly as the canvas drew it (a positioned box changes paint order) */
        anchor: s.fmenu === k ? 'relative' : undefined,
        expanded: s.fmenu === k ? 'true' : 'false',
        toggle: () => this.setState({ fmenu: s.fmenu === k ? '' : k, menu: false }),
        menuLabel: MENU_LABEL[k],
        opts: s.fmenu === k ? optionsOf(k) : [],
      };
    });
    /* "Open this in the full statement view": the Brief at this statement (`?statement=`); a line of
     * what counts as done is shown in full on What counts as done */
    const atNow = this.props.at || {};
    const whereNow = { change: atNow.change || D.change.id, project: atNow.project || D.project.id || undefined };
    const selRow = byId[selId] || null;
    const fullView = isDemo || !whereNow.change || !selRow ? undefined
      : selRow.kind === 'claim' ? routeHref('brief', Object.assign({ statement: selRow.id }, whereNow))
        : routeHref('done', whereNow);
    const currentView = ['table', 'timeline', 'transcript'].includes(s.view) ? s.view : 'table';
    const timelineRows = D.statements.slice().sort((a, b) => (a.step ?? 0) - (b.step ?? 0)).map((q) => ({
      id: q.id, lead: [q.time, q.agent, q.step == null ? null : stepTxt(q.step)].filter(Boolean).join(' · '), text: q.text,
    }));
    const transcriptRows = D.messages.slice().sort((a, b) => (a.step ?? 0) - (b.step ?? 0)).map((m) => ({
      id: m.id, lead: [m.time, m.agent, m.step == null ? null : stepTxt(m.step)].filter(Boolean).join(' · '), text: m.text,
    }));

    return {
      keys: {
        next: step(1), prev: step(-1), open: openSel, primary: openCheck,
        copy: () => !isDemo && whereNow.change ? routeHref('check', whereNow) : false,
      },
      alsoHref: !isDemo && also ? stepTo(also.step) : undefined,
      view: currentView,
      viewLabel: currentView === 'timeline' ? 'Timeline' : currentView === 'transcript' ? 'Transcript' : 'Every statement',
      viewRows: currentView === 'timeline' ? timelineRows : transcriptRows,
      showTimeline: go({ view: 'timeline' }), showTable: go({ view: 'table' }), showTranscript: go({ view: 'transcript' }),
      flt: flt,
      fullView: fullView,
      /* with no statement selected (nothing matches, or a sentence no rule applies to), the link says where it goes */
      fullViewLabel: isDemo ? undefined : !selRow ? 'Open the Brief ›' : selRow.kind === 'crit' ? 'Open this line in What counts as done ›' : undefined,
      /* C30 R-PREFS: the altitudes are links (Brief → brief, Workbench → changed); this screen is Explore */
      alt: altitudes('explore'),
      /* toolbar */
      groupLabel: byState ? 'State' : 'Whose turn',
      menuOpen: s.menu,
      menuExpanded: s.menu ? 'true' : 'false',
      toggleMenu: go({ menu: !s.menu }),
      groupOpts: [
        { key: 'turn', label: 'Whose turn', hint: 'Needs a check first, then what the agents said. Within a group: Not proven, Needs a re-check, Recorded, In progress, then age.' },
        { key: 'state', label: 'State', hint: 'Leading order: Not proven, Needs a re-check, Recorded, In progress, then age.' },
      ].map((o) => ({
        label: o.label, hint: o.hint,
        checked: s.groupBy === o.key ? 'true' : 'false',
        tick: s.groupBy === o.key ? '✓' : '',
        pick: go({ groupBy: o.key, menu: false }),
        style: 'display: flex; align-items: flex-start; gap: 10px; width: 100%; box-sizing: border-box; padding: 8px 10px; background: ' + (s.groupBy === o.key ? '#16161A' : 'transparent') + '; border: 0; border-radius: 5px; color: #EDEDEF; font: inherit; text-align: left; cursor: pointer',
      })),
      /* verdict strip */
      segs: segs,
      vIdle: !checking,
      checking: checking,
      captionTwo: stopped ? 'You stopped this check after ' + (stoppedForS == null ? 'an unrecorded time' : stoppedForS + ' s') + '. What it printed is kept, and it settles nothing.' : [reachPhrase].filter(Boolean).concat(ASK ? [ASK] : []).join(' · '),
      captionTwoStyle: stopped ? "font-family: 'Geist', sans-serif; font-size: 12.5px; color: #A1A1AA" : '',
      openTests: openCheck,
      openSel: openSel,
      runCheck: isDemo ? go({ sheet: '', checking: true, stopped: false }) : () => { if (canRun) { this.setState({ sheet: '' }); acts.check(); } },
      stopCheck: isDemo ? go({ checking: false, stopped: true }) : () => { if (acts.stopCheck) acts.stopCheck(); },
      closeSheet: go({ sheet: '' }),
      sheetTests: s.sheet === 'tests',
      sheetLint: s.sheet === 'lint',
      /* rack */
      rackSummary: filtered
        ? 'Showing ' + fmtCount(SHOWN.length) + ' of ' + fmtCount(ROWS.length) + ' · ' + activeWords.join(' · ')
        : 'What counts as done (' + fmtCount(crits.length) + ') · What the agents said (' + fmtCount(D.statements.length) + ') · Also said (' + fmtCount(ALSO.length) + ')',
      groups: groups,
      alsoOpen: s.alsoOpen,
      alsoExpanded: s.alsoOpen ? 'true' : 'false',
      alsoChevron: s.alsoOpen ? '▾' : '▸',
      alsoCaption: s.alsoOpen ? 'never counted · no rule applies, so ThruLine stays silent' : 'collapsed · never counted · no rule applies, so ThruLine stays silent',
      toggleAlso: go({ alsoOpen: !s.alsoOpen, menu: false }),
      pickAlso: go({ sel: alsoId, menu: false }),
      alsoRows: alsoRows,
      stops: stops,
      alsoPressed: alsoSel ? 'true' : 'false',
      alsoRowStyle: 'display: grid; grid-template-columns: minmax(0, 1fr) 56px; column-gap: 16px; align-items: center; min-height: 52px; padding: 0 24px; box-sizing: border-box;'
        + (alsoSel ? ' background: rgba(255,122,41,.07); box-shadow: inset 2px 0 0 #FF7A29;' : ''),
      /* inspector */
      insp: insp,
      pagerText: pagerText,
      prev: step(-1),
      next: step(1),
      /* status bar: same count as the headline */
      statusLine: [hasLines ? needLine + ' on ' + V : needLine].concat(checking ? ['1 check running'] : [])
        .concat([fmtCount(crits.length + D.statements.length) + ' statements', fmtCount(D.sessions.length) + ' session' + (D.sessions.length === 1 ? '' : 's')])
        .concat(!checking && D.change.recording ? ['recording'] : []).join(' · '),
      /* content the markup used to carry as literals */
      isDemo: isDemo,
      c: content,
    };
  }
}
