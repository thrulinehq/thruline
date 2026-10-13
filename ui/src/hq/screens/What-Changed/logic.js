/* What-Changed.dc.html: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { recordingPill, storageSentence } from '../../data/project.js'
import { fmtCount } from '../../data/count.js'
import { altitudes } from '../Change-Brief/altitude.js'
import { isStep, stepLinker } from '../Step-View/link.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

export default class Component extends DCLogic {
  state = { file: '', from: 'proof', run: 'idle', banner: true, stoppedAfter: 0 };

  componentWillUnmount() {
    clearTimeout(this._t);
  }

  renderVals() {
    const s = this.state;
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    const V = D.version.id;
    /* C30 R-RUN: a check ThruLine ran from HQ sits half a step after the last record before it
     * (from-change.js); it is named as what it is, never by a made-up step number */
    const stp = (n) => (n == null ? 'step not recorded' : Number.isInteger(n) ? 'step ' + n : 'the check ThruLine ran');
    const Stp = (n) => { const w = stp(n); return w.charAt(0).toUpperCase() + w.slice(1); };
    const mark = (n) => (Number.isInteger(n) ? ' ' + n : '');
    /* C30 R-ROUTES: a chip that names a step opens that step; a check ThruLine ran has no step and no link */
    const stepTo = isDemo ? () => undefined : stepLinker(this.props.at, D.project && D.project.id);
    const go = (n) => (isDemo || isStep(n) ? ' ›' : '');
    const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', YE = '#F2C94C', GR = 'var(--tl-proven, #3DD68C)';
    const HATCH = 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)';

    // ---------- the record, indexed ----------
    // The canvas's "Check it again" finishes with an illustrative run. Only the
    // design gallery plays that out; the live product shows the runs it is given.
    const WHO = { thruline: 'ThruLine' };
    // A finished run held when it exited 0, or, with no exit code recorded, when its tally shows no failure.
    const endedOf = (r) => (r.status !== 'finished' ? 'unknown'
      : r.outcome ? (r.outcome === 'succeeded' && !(r.failed > 0) ? 'succeeded' : r.outcome === 'unknown' ? 'unknown' : 'failed')
        : (r.exitCode === 0 || (r.exitCode == null && r.failed === 0)) ? 'succeeded' : 'failed');
    const norm = (r) => Object.assign({}, r, { ended: endedOf(r), paths: (r.scope || []).map((q) => disp(q)), matchedBy: r.matchedBy ? (WHO[r.matchedBy] || r.matchedBy) : '' });
    // Absolute paths (work done in another checkout of the project) are shown from the folder every file of this Change shares.
    const absAll = D.files.length > 1 && D.files.every((f) => String(f.path).startsWith('/'));
    const common = absAll ? D.files.map((f) => f.path.split('/').slice(0, -1)).reduce((a, b) => { let i = 0; while (i < a.length && a[i] === b[i]) i++; return a.slice(0, i); }).join('/') : '';
    /* live: a long path keeps its end (the file's name) and drops leading folders behind '…/'; the full path is its title */
    const tailPath = (p, n) => {
      const str = String(p || '');
      if (isDemo || str.length <= n) return undefined;
      const parts = str.split('/');
      let acc = parts.pop();
      if (acc.length > n - 2) return '…' + acc.slice(-(n - 1));
      while (parts.length && acc.length + parts[parts.length - 1].length + 1 <= n - 2) acc = parts.pop() + '/' + acc;
      return '…/' + acc;
    };
    const disp = (p) => (common && String(p).startsWith(common + '/') ? String(p).slice(common.length + 1) : p);
    const EDITS = D.edits.map((e) => Object.assign({}, e, { path: disp(e.file) }));
    // When the first recorded version was made by an edit, the code before that edit was never recorded.
    const VERS = D.versions.length && D.versions[0].madeByStep != null ? [{ id: null }].concat(D.versions) : D.versions;
    const FILES = D.files.map((f) => ({ id: f.path, path: disp(f.path), kind: f.kind }));
    const CHECK = D.recheck ? { command: D.recheck.runner + ' ' + D.recheck.scope.join(' '), paths: D.recheck.scope, askOncePerProject: D.recheck.permission === 'ask_once_per_project' }
      : { command: '', paths: [], askOncePerProject: false };
    /* live (C30 R-RUN): Check it again opens the check sheet already running — the
       press is the consent — unless the server says it will not start one, and
       then the control is off with the server's own reason (UX rule 6) */
    const acts = (!isDemo && this.props.actions) || {};
    const canRun = isDemo || (!!acts.check && !acts.checkBlocked);
    const RUN_WHY = acts.checkBlocked ? acts.checkBlocked.reason + ' ' + acts.checkBlocked.enabler : 'This check cannot start from this screen.';
    const after = isDemo && s.run === 'done' ? D.afterCheck : null;
    const runs = (after ? D.runs.concat([after.run]) : D.runs).map(norm);
    /* a check of this Change going now: the record's, never this screen's own timer */
    const checking = isDemo ? s.run === 'checking' : !!D.running;
    const vIdx = (id) => VERS.findIndex((x) => x.id === id);
    const focus = D.statements.find((x) => x.id === D.statementId) || D.statements[0] || null;
    const held = runs.filter((x) => x.ended === 'succeeded');
    const latest = held.length ? held[held.length - 1] : null;  // the newest run that held
    // "The proof": the run the statement rests on; failing that, the newest run that held; failing that, none.
    const restsId = focus && focus.proof && focus.proof.restsOn && focus.proof.restsOn.kind === 'run' ? focus.proof.restsOn.runId : null;
    const cited = runs.find((x) => x.id === restsId) || latest || null;
    const proofIdx = cited ? vIdx(cited.version) : -1;
    const done = !!latest && !!cited && latest.version === V && latest.step > cited.step;
    const idle = !checking && !done;
    const stopped = idle && (isDemo ? s.stoppedAfter > 0 : !!D.stopped);
    const editsOf = (path) => EDITS.filter((e) => e.path === path);
    const isCheckFile = (path, run) => !!run && run.paths.indexOf(path) >= 0;
    const scopeFor = (path, run) => run.paths.find((q) => q !== path && path.indexOf(q) === 0 && q.slice(-1) === '/') || '';
    const plural = (n, one, many) => fmtCount(n) + ' ' + (n === 1 ? one : many);
    const RULE = 'this check doesn’t say which files it depends on, so ThruLine treats it as depending on everything this Change covers';

    // Compare range: from one recorded version to now. 'proof' = the version the cited run ran on.
    const FROM_KEYS = VERS.slice(0, -1).map((x, i) => (i === proofIdx ? 'proof' : i === 0 ? 'base' : 'v' + i));
    const fiRaw = s.from === 'base' ? 0 : FROM_KEYS.indexOf(s.from);
    const fi = fiRaw >= 0 ? fiRaw : proofIdx >= 0 && proofIdx < VERS.length - 1 ? proofIdx : 0;
    // "Whole Change" is every edit, even when the proof ran on the first version
    const whole = s.from === 'base' || (fi === 0 && !(proofIdx === 0 && s.from === 'proof'));
    const since = !whole && proofIdx >= 0 && fi === proofIdx;
    const fromV = VERS[fi] || { id: null };
    const firstEdit = EDITS.length ? EDITS[0] : null;
    const verLabel = (i) => (VERS[i] && VERS[i].id) || ('before step ' + (firstEdit ? firstEdit.step : ''));
    // Until a check on this version exists, "the proof" is the cited run. After it, the page names that step outright.
    const SINCE = done ? 'after ' + stp(cited.step) : 'since the proof';

    // State marks: word + glyph + line/fill style. Green only on "Proven".
    const PILL = {
      none: { glyph: '○', g: T2, word: 'Not proven', color: T1, border: '1px dashed #82828C', bg: 'transparent' },
      older: { glyph: '◭', g: YE, word: 'Needs a re-check', color: YE, border: '1px solid rgba(242,201,76,.6)', bg: HATCH },
      proven: { glyph: '●', g: GR, word: 'Proven', color: GR, border: '1px solid rgba(var(--tl-proven-rgb, 61, 214, 140), .6)', bg: 'rgba(var(--tl-proven-rgb, 61, 214, 140), .08)' },
      /* not drawn on this artboard; the spec's coral ✕ (DESIGN-SPEC §1.8) */
      contradicted: { glyph: '✕', g: '#FF6369', word: 'Contradicted', color: '#FF6369', border: '1px solid rgba(255,99,105,.6)', bg: 'transparent' }
    };
    // Proof beside each added line ("proof blame"). Neutral solid once a check on this version covers it; never green.
    const MARK = {
      none: { b: '1px dashed #82828C', bg: 'transparent' },
      older: { b: '1px solid rgba(242,201,76,.8)', bg: 'repeating-linear-gradient(45deg, rgba(242,201,76,.7) 0 2px, transparent 2px 4px)' },
      now: { b: '1px solid #EDEDEF', bg: '#EDEDEF' },
      blank: { b: '1px solid transparent', bg: 'transparent' }
    };

    // ---------- handlers ----------
    const pick = (f) => () => this.setState({ file: f });
    const setFrom = (k) => () => this.setState({ from: k });
    const check = () => {
      if (!canRun) return false;
      if (!isDemo) { acts.check(); return; }
      clearTimeout(this._t);
      this._t0 = Date.now();
      this.setState({ run: 'checking', stoppedAfter: 0 });
      if (isDemo) this._t = setTimeout(() => this.setState({ run: 'done' }), 1800);
    };
    const stop = () => {
      if (!isDemo) { if (acts.stopCheck) acts.stopCheck(); return; }
      clearTimeout(this._t);
      const n = Math.max(1, Math.round((Date.now() - (this._t0 || Date.now())) / 1000));
      this.setState({ run: 'idle', stoppedAfter: n });
    };
    const replay = () => {
      clearTimeout(this._t);
      this.setState({ run: 'idle', stoppedAfter: 0, from: 'proof', file: D.selectedFile });
    };
    const hideBanner = () => this.setState({ banner: false });

    // ---------- coverage: which run a file's proof stands on ----------
    // now: the newest run that held is on this version and reads the file; older: it ran on an
    // older version and the file is unchanged since; none: the file changed after it.
    const lastEditStep = (path) => { const e = editsOf(path); return e.length ? e[e.length - 1].step : null; };
    const reads = (run, path) => !run.declaresPaths || run.paths.some((q) => path === q || (q.slice(-1) === '/' && path.indexOf(q) === 0));
    const coverWith = (path, run) => {
      if (!run || !reads(run, path)) return 'none';
      // the file must be unchanged since the run, by the record's own order of steps
      const last = lastEditStep(path);
      if (last != null && last > run.step) return 'none';
      return run.version === V ? 'now' : 'older';
    };
    const covOf = (path) => coverWith(path, latest);
    const covBefore = (path) => coverWith(path, cited);

    // ---------- record chips (kind first, then the fact, then the version relation, then the step) ----------
    const older = (ver) => (VERS.length - 1) - vIdx(ver);
    const runChip = (r) => (!r ? null : { kind: 'Run', fact: r.command + ' · ' + RES(r),
      badge: r.version + ' · ' + (older(r.version) === 0 ? 'same version' : fmtCount(older(r.version)) + ' version' + (older(r.version) === 1 ? '' : 's') + ' older'),
      badgeB: older(r.version) === 0 ? '1px solid #66666F' : '1px solid rgba(242,201,76,.6)', badgeC: older(r.version) === 0 ? T2 : YE,
      step: stp(r.step) + go(r.step), href: stepTo(r.step) });
    const RES = (r) => (r.passed == null ? (r.ended === 'failed' ? 'failed' : r.ended === 'succeeded' ? 'held' : 'no result yet') : fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed');
    // Counted from the hunks' own lines; an edit whose lines the record does not hold uses its stated totals.
    const countOf = (e, op, stated) => (e.hunks ? e.hunks.reduce((m, h) => m + h.lines.filter((l) => l.op === op).length, 0) : stated);
    const addsOf = (es) => es.reduce((n, e) => n + countOf(e, 'add', e.linesAdded), 0);
    const delsOf = (es) => es.reduce((n, e) => n + countOf(e, 'del', e.linesRemoved), 0);
    const known = (es) => es.every((e) => e.hunks || (e.linesAdded != null && e.linesRemoved != null));
    const pm = (es) => (known(es) ? '+' + fmtCount(addsOf(es)) + ' −' + fmtCount(delsOf(es)) : 'lines not recorded');
    const editChip = (e) => ({ kind: 'Edit', fact: e.path + ' · ' + pm([e]), badge: e.makesVersion + (e.makesVersion === V ? ' · now' : ''),
      badgeB: '1px solid #66666F', badgeC: T2, step: stp(e.step) + go(e.step), href: stepTo(e.step) });
    const ILLUS = 'Step ' + (D.afterCheck ? D.afterCheck.run.step : '') + ' is illustrative: the next step number the record would assign. Matching it to the statement by its command is engine work not built yet; until then the statement stays Not proven and what counts as done carries the proof.';

    // ---------- the files ----------
    // A range starts at a step: the proof's run, or the edit that made the chosen version (the record's order, across sessions).
    const madeStep = (i) => { const ver = VERS[i]; if (!ver || !ver.id) return -Infinity; if (ver.madeByStep != null) return ver.madeByStep;
      const own = EDITS.filter((e) => e.makesVersion === ver.id).map((e) => e.step); return own.length ? Math.min.apply(null, own) : -Infinity; };
    const startStep = since && cited ? cited.step : whole ? -Infinity : madeStep(fi);
    const shownEdits = (path) => editsOf(path).filter((e) => e.step > startStep);
    const FD = {};
    FILES.forEach((file) => {
      const all = editsOf(file.path);
      const shown = shownEdits(file.path);
      const cov = covOf(file.path);
      const run = cov === 'none' ? null : latest && cov === 'now' ? latest : cited;
      const last = all.length ? all[all.length - 1] : null;
      FD[file.id] = {
        id: file.id, path: file.path, kind: file.kind, all: all, shown: shown, cov: cov, run: run,
        lines: shown.length ? pm(shown) : all.length ? 'unchanged since ' + verLabel(fi) : 'not edited',
        linesC: shown.length ? T2 : T3,
        e1: !all.length ? 'none' : all.length === 1 && last.time ? last.step + ' · ' + last.time : all.map((e) => e.step).join(', '),
        e2: !all.length ? (isCheckFile(file.path, cited) ? 'the check' : '') : '→ ' + last.makesVersion,
        c1: run ? (isCheckFile(file.path, run) ? 'Ran' : 'Run') + ' · ' + stp(run.step) : 'Nothing',
        c2: run ? 'on ' + run.version + (run.time ? ' · ' + run.time : '') : 'in no run’s scope',
      };
    });
    const pillOf = (cov) => PILL[cov === 'now' ? 'proven' : cov === 'older' ? 'older' : 'none'];
    const subOf = (d) => d.cov === 'now' ? d.run.paths.join(', ') : d.cov === 'older' ? 'proof is on ' + d.run.version : 'no check covers it';

    const fileId = FD[s.file] ? s.file : (FD[D.selectedFile] ? D.selectedFile : (FILES.length ? FILES[0].id : ''));
    const mkRow = (id) => {
      const d = FD[id];
      const p = pillOf(d.cov);
      const sel = fileId === id;
      const sub = subOf(d);
      const stateAria = d.cov === 'now' ? 'Proven on ' + V + ', scope ' + sub : p.word + ', ' + sub;
      return {
        path: d.path, pathShort: tailPath(d.path, 54), pathFull: isDemo ? undefined : d.path, lines: d.lines, linesC: d.linesC, e1: d.e1, e2: d.e2, c1: d.c1, c2: d.c2,
        pWord: d.cov === 'now' ? 'Proven · ' + V : p.word, pGlyph: p.glyph, pG: p.g, pColor: p.color, pBorder: p.border, pBg: p.bg,
        sub: sub, subC: d.cov === 'now' ? T2 : T3,
        bg: sel ? '#16161A' : 'transparent',
        // selection, live: orange border and glow (DESIGN-SPEC §1.10); the canvas's focus ring stays in the gallery
        outline: isDemo ? (sel ? '2px solid #FF7A29' : 'none') : (sel ? '1px solid #FF7A29' : undefined),
        outlineOffset: isDemo ? undefined : '-1px',
        shadow: isDemo ? (sel ? '0 0 0 1px #FFFFFF inset' : 'none') : (sel ? '0 0 0 1px rgba(255,122,41,.25), 0 0 12px rgba(255,122,41,.15)' : undefined),
        pressed: sel ? 'true' : 'false',
        pick: pick(id),
        aria: d.path + '. ' + d.lines + '. ' + (d.e1 === 'none' ? 'Not edited' : 'Edited at steps ' + d.e1) + '. Covered by: ' + d.c1 + ', ' + d.c2 + '. ' + stateAria + '.'
      };
    };
    const KINDS = [['code', 'Code'], ['tests', 'Tests'], ['config', 'Config'], ['generated', 'Generated']];
    const shownKinds = KINDS.filter((k) => FILES.some((f) => f.kind === k[0]));
    const emptyKinds = KINDS.filter((k) => !FILES.some((f) => f.kind === k[0])).map((k) => k[1] + ' 0').join(' · ');
    const groups = shownKinds.map((k, i) => {
      const files = FILES.filter((f) => f.kind === k[0]);
      const lastGroup = i === shownKinds.length - 1;
      return {
        label: k[1], count: plural(files.length, 'file', 'files'),
        aside: lastGroup ? emptyKinds : (isDemo && k[0] === 'code' ? 'Line counts are illustrative; the record doesn’t hold them yet.' : ''),
        files: files.map((f) => mkRow(f.id)),
      };
    });

    // ---------- compare toggle + version ruler (the ruler is the selector: pick where to compare from) ----------
    const failedOn = (i) => runs.find((r) => r.version === VERS[i].id && r.ended === 'failed');
    const tg = {
      since: setFrom('proof'), whole: setFrom('base'),
      sinceP: since ? 'true' : 'false', wholeP: whole ? 'true' : 'false',
      sinceBg: since ? '#2A2A30' : 'transparent', wholeBg: whole ? '#2A2A30' : 'transparent',
      sinceC: since ? T1 : T2, wholeC: whole ? T1 : T2,
      from: verLabel(fi), to: V,
      cap: since ? (done ? stp(cited.step) + '’s version → now' : 'the proof’s version → now') : whole ? 'every edit → now' : failedOn(fi) ? 'the failed run’s version → now' : fromV.id + ' → now'
    };
    const mk = (t, c) => ({ t: t, c: c });
    const listAnd = (xs) => (xs.length < 2 ? xs.join('') : xs.slice(0, -1).join(', ') + ', and ' + xs[xs.length - 1]);
    // What happened while each version was the code: the edit that made it, runs on it, claims said on it.
    const eventsOn = (ver) => []
      .concat(EDITS.filter((e) => e.makesVersion === ver).map((e) => ({ step: e.step, kind: 'edit', e: e })))
      .concat(runs.filter((r) => r.version === ver).map((r) => ({ step: r.step, kind: 'run', r: r })))
      .concat(D.statements.filter((q) => q.versionBefore === ver).map((q) => ({ step: q.step, kind: 'said', q: q })))
      .sort((p, q) => p.step - q.step);
    const markOf = (ev) => ev.kind === 'edit' ? '±' + mark(ev.step)
      : ev.kind === 'said' ? '❝' + mark(ev.step)
        : ev.r.ended === 'failed' ? '✕' + mark(ev.step) : ev.r.ended === 'succeeded' ? '●' + mark(ev.step) : '⋯' + mark(ev.step);
    const srOf = (ev) => ev.kind === 'edit' ? 'the edit at ' + stp(ev.step)
      : ev.kind === 'said' ? 'the claim at ' + stp(ev.step)
        : ev.r.ended === 'failed' ? 'the run at ' + stp(ev.step) + ' on it, ' + fmtCount(ev.r.failed) + ' failed, ' + fmtCount(ev.r.passed) + ' passed'
          : ev.r.ended === 'succeeded' ? 'the run at ' + stp(ev.step) + ' that held, ' + fmtCount(ev.r.passed) + ' passed, ' + fmtCount(ev.r.failed) + ' failed'
            : stp(ev.step) + ', which has no result yet';
    // A version with many records shows its first marks and how many more; its spoken text still names every one.
    const cap = (ms) => (ms.length > 12 ? ms.slice(0, 11).concat([mk('+' + fmtCount(ms.length - 11) + ' more', T3)]) : ms);
    const band = (i, label, marks, sr, fog) => {
      const on = i >= fi;
      return {
        label: label, marks: marks, sr: sr,
        bg: on ? 'rgba(255,122,41,.06)' : 'transparent',
        line: on ? '#FF7A29' : 'transparent',
        glow: on ? '0 0 6px rgba(255,154,82,.55)' : 'none',
        labelC: fog ? T3 : (on ? T1 : T2),
        pressed: i === fi ? 'true' : 'false',
        pick: setFrom(FROM_KEYS[i])
      };
    };
    const bands = VERS.slice(0, -1).map((ver, i) => {
      if (!ver.id) {
        return band(i, verLabel(i), [mk('◌ not recorded', T3)], 'Compare from ' + verLabel(i) + ', the code before the first edit. ThruLine recorded no version for it.', true);
      }
      const evs = eventsOn(ver.id);
      return band(i, ver.id, cap(evs.map((ev) => mk(markOf(ev), T2))),
        'Compare from ' + ver.id + (i === proofIdx ? ', the version the proof ran on: ' : ': ') + listAnd(evs.map(srOf)) + '.', false);
    });
    const nowEvents = eventsOn(V);
    const afterProof = (ev) => ev.kind === 'edit' && (!latest || ev.step > latest.step);
    const nowMarks = nowEvents.filter((ev) => ev.kind !== 'run' || ev.r.ended !== 'succeeded').map((ev) => afterProof(ev) ? mk('◭' + mark(ev.step), YE) : mk(markOf(ev), T2));
    if (checking) nowMarks.push(mk('◔ checking', T2));
    nowEvents.filter((ev) => ev.kind === 'run' && ev.r.ended === 'succeeded').forEach((ev) => nowMarks.push(mk('●' + mark(ev.step), T1)));
    const nowSr = nowEvents.filter((ev) => !(ev.kind === 'run' && ev.r.ended === 'succeeded')).map((ev) => ev.kind === 'edit'
      ? 'the edit at ' + stp(ev.step) + (afterProof(ev) ? ', after the proof' : '')
      : ev.kind === 'run' ? stp(ev.step) + ' has no result yet' : 'the claim at ' + stp(ev.step));
    if (checking) nowSr.push('a check is running now');
    nowEvents.filter((ev) => ev.kind === 'run' && ev.r.ended === 'succeeded').forEach((ev) => nowSr.push('the check at ' + stp(ev.step) + ' held, ' + fmtCount(ev.r.passed) + ' passed, ' + fmtCount(ev.r.failed) + ' failed'));
    const nowBand = {
      marks: cap(nowMarks),
      sr: 'To ' + V + ', the version now: ' + nowSr.join('; ') + '.'
    };

    // ---------- diff hunks ----------
    // Line numbers come from each hunk's start and its lines. In a range that starts before an
    // earlier edit to the same file, the old side is renumbered against the range's start.
    const numbered = (e, h) => {
      let shift = 0;
      // A hunk whose start the record does not state (a Codex patch, or an edit read from the request) has no line numbers.
      const numberless = h.oldStart == null || h.newStart == null;
      if (!numberless) editsOf(e.path).filter((x) => x.step < e.step && x.step > startStep).forEach((x) => (x.hunks || []).forEach((hh) => {
        if (hh.newStart != null && hh.newStart < h.oldStart) shift -= hh.lines.filter((l) => l.op === 'add').length - hh.lines.filter((l) => l.op === 'del').length;
      }));
      let o = h.oldStart + shift;
      let n = h.newStart;
      const oc = h.lines.filter((l) => l.op !== 'add').length;
      const nc = h.lines.filter((l) => l.op !== 'del').length;
      return {
        head: numberless ? '@@ line numbers not recorded @@' : '@@ -' + (h.oldStart + shift) + ',' + oc + ' +' + h.newStart + ',' + nc + ' @@',
        fn: h.context || (numberless && e.source === 'request' ? 'from the request · no surrounding lines' : ''),
        chip: stp(e.step) + (e.time ? ' · ' + e.time : '') + go(e.step), href: stepTo(e.step),
        lines: h.lines.map((l) => {
          const t = l.op === 'add' ? 'a' : l.op === 'del' ? 'd' : 'c';
          const out = { o: t === 'a' || numberless ? '' : String(o), n: t === 'd' || numberless ? '' : String(n), t: t, code: l.text };
          if (t !== 'a') o += 1;
          if (t !== 'd') n += 1;
          return out;
        }),
      };
    };
    const deco = (h, d) => {
      const rk = d.cov;
      const m = MARK[rk];
      return {
        head: h.head, fn: h.fn, chip: h.chip, href: h.href,
        blame: rk === 'now' ? 'covered by ' + stp(d.run.step) + ' on ' + V : rk === 'none' ? 'no check has run this on ' + V : 'ran on ' + d.run.version + ' only · ' + stp(d.run.step),
        bB: m.b, bBg: m.bg,
        lines: h.lines.map((ln) => {
          const a = ln.t === 'a', dl = ln.t === 'd';
          const r = a ? m : MARK.blank;
          return {
            o: ln.o, n: ln.n, code: ln.code,
            sign: a ? '+' : dl ? '−' : '',
            signC: a ? T1 : T3,
            bg: a ? 'rgba(255,255,255,.05)' : dl ? 'rgba(255,255,255,.02)' : 'transparent',
            bar: a ? 'inset 2px 0 0 #A1A1AA' : dl ? 'inset 2px 0 0 #66666F' : 'none',
            color: a ? T1 : dl ? T3 : T2,
            rB: r.b, rBg: r.bg
          };
        })
      };
    };

    const fd = FD[fileId] || { id: '', path: '', kind: 'code', all: [], shown: [], cov: 'none', run: null };
    const citedChip = runChip(cited);
    const latestChip = latest ? runChip(latest) : citedChip;
    const claimBefore = (e) => !!focus && focus.step != null && e.step != null && focus.step < e.step;
    const changedAfter = (d) => d.all.length > 0 && (!cited || d.all[d.all.length - 1].step > cited.step);
    const firstChangeAfter = cited ? EDITS.find((e) => e.step > cited.step) || null : null;
    const diff = {
      eyebrow: since ? (done ? 'Since ' + stp(cited.step) + ' ran' : 'Since the proof ran') : whole ? 'The whole Change' : 'Since ' + fromV.id,
      path: fd.path, pathShort: tailPath(fd.path, 44), pathFull: isDemo ? undefined : fd.path, showWhole: setFrom('base'),
      counts: '', hunksN: '', sentence: '', extra: '', hunks: [],
      eTitle: '', eBody: '', eBtn: false, eChip: citedChip
    };
    const hunks = [];
    fd.shown.forEach((e) => (e.hunks || []).forEach((h) => hunks.push(deco(numbered(e, h), fd))));
    const linesMissing = fd.shown.some((e) => !e.hunks);
    diff.hunksN = plural(hunks.length, 'hunk', 'hunks');
    const lastE = fd.all.length ? fd.all[fd.all.length - 1] : null;
    if (fd.all.length && changedAfter(fd)) {
      diff.counts = pm(fd.shown);
      diff.sentence = 'Edited at ' + stp(lastE.step) + (lastE.time ? ' (' + lastE.time + ')' : '')
        + (cited ? ', after the run at ' + stp(cited.step) + (claimBefore(lastE) ? ' and after the claim at ' + stp(focus.step) : '') + '.' : '. No run in the record has checked it.')
        + ' The code is now ' + V + '.';
      const ex = [];
      if (!since && fd.all.length === 1) ex.push('Only one edit in this Change touched this file, so every range shows the same lines.');
      if (done) ex.push(Stp(latest.step) + ' ran on ' + V + ' at ' + latest.time + ', after this edit, with ' + scopeFor(fd.path, latest) + ' in scope.');
      diff.extra = ex.join(' ');
      diff.hunks = hunks;
    } else if (fd.all.length) {
      const sinceP = 'Unchanged since ' + cited.version + '.';
      if (fd.shown.length > 1) {
        diff.counts = pm(fd.shown);
        diff.sentence = 'Edited at ' + fd.shown.map((e) => stp(e.step)).join(' and ') + ', ' + (fd.shown.length === 2 ? 'both' : 'all') + ' before the run at ' + stp(cited.step) + '. ' + sinceP;
      } else if (fd.shown.length === 1) {
        const f = failedOn(fi);
        diff.counts = pm(fd.shown);
        diff.sentence = 'Edited at step ' + fd.shown[0].step + (f ? ', after the failed run at ' + stp(f.step) + ' on ' + fromV.id + ' and' : ',') + ' before the run at ' + stp(cited.step) + '. ' + sinceP;
      } else {
        diff.counts = 'no lines changed';
        diff.sentence = 'Unchanged since ' + cited.version + ', the version the run at ' + stp(cited.step) + ' saw. Last edited at ' + stp(lastE.step) + '.';
      }
      diff.hunks = hunks;
      diff.eTitle = done ? 'No lines changed after ' + stp(cited.step) + '.' : 'No lines changed since the proof ran.';
      diff.eBody = done
        ? fd.path + ' is the same on ' + cited.version + ' and ' + V + '. The check at ' + latest.time + ' ran on ' + V + ', and it counts for this file: ' + RULE + '.'
        : fd.path + ' is the same on ' + cited.version + ' and ' + V + '. It still needs a re-check: ' + RULE + '.';
      diff.eBtn = true;
      diff.eChip = done ? latestChip : citedChip;
    } else {
      const isChk = isCheckFile(fd.path, cited);
      diff.counts = 'not edited';
      diff.sentence = 'Not edited in this Change.' + (isChk ? ' This file is the check that ran at ' + stp(cited.step) + (done ? ' and again at ' + stp(latest.step) : '') + '.' : '');
      diff.eTitle = 'This file wasn’t edited in this Change.';
      diff.eBody = !isChk ? '' : done
        ? 'It is the check itself. ' + Stp(latest.step) + ' ran it again at ' + latest.time + ' on ' + V + ', with ' + latest.paths.filter((q) => cited.paths.indexOf(q) < 0).join(', ') + ' in scope: ' + fmtCount(latest.passed) + ' passed, ' + fmtCount(latest.failed) + ' failed.'
        : 'It is the check itself. ' + Stp(cited.step) + ' ran it on ' + cited.version + ': ' + fmtCount(cited.passed) + ' passed, ' + fmtCount(cited.failed) + ' failed. A passing run speaks only for the version it ran on.';
      diff.eChip = done ? latestChip : citedChip;
    }
    diff.hasExtra = diff.extra.length > 0;
    diff.hasHunks = diff.hunks.length > 0;
    diff.noHunks = !diff.hasHunks;

    // ---------- why the selected file reads as it does (the ④ / ⑤ link for this file) ----------
    let det;
    const res = RES;
    if (!done) {
      if (changedAfter(fd)) det = { eyebrow: '④ Since · ' + fd.path, glyph: '◭', gC: YE,
        text: fd.path + ' changed at ' + stp(lastE.step) + (lastE.time ? ' (' + lastE.time + ')' : '')
          + (cited ? ', after the run' + (claimBefore(lastE) ? ' and after the claim' : '') + '.' : '. No run in the record has checked it.') + ' The code is now ' + V + '.',
        note: checking ? 'A check with ' + scopeFor(fd.path, { paths: CHECK.paths }) + ' in scope is running now on ' + V + '. Nothing is settled until it finishes.' : '',
        was: '', hasStop: true, chip: editChip(lastE) };
      else if (fd.all.length) det = { eyebrow: '④ Since · ' + fd.path, glyph: '◭', gC: YE,
        text: fd.path + ' is unchanged since ' + cited.version + ', the version ' + stp(cited.step) + ' ran on. It still needs a re-check: ' + RULE + '.',
        note: 'Declare the paths this check reads, and proof will carry forward when they are unchanged. Declared paths are engine work, not built yet.',
        was: '', hasStop: false, chip: citedChip };
      else if (!cited) det = { eyebrow: '④ Since · ' + (fd.path || 'this Change'), glyph: '◭', gC: YE,
        text: fd.path ? 'Not edited in this Change, and no run in the record has checked it.' : 'No file was edited in this Change.',
        note: '', was: '', hasStop: false, chip: null };
      else det = { eyebrow: '④ Since · ' + fd.path, glyph: '◭', gC: YE,
        text: 'Not edited in this Change' + (isCheckFile(fd.path, cited) ? ': this file is the check' : '') + '. ' + Stp(cited.step) + ' ran it on ' + cited.version + ', ' + res(cited) + '.'
          + (firstChangeAfter ? ' ' + firstChangeAfter.path + ' changed at ' + stp(firstChangeAfter.step) + ', after that run.' : ''),
        note: 'A passing run speaks only for the version it ran on.', was: '', hasStop: false, chip: citedChip };
    } else {
      const was = pillOf(covBefore(fd.path) === 'older' ? 'older' : 'none');
      const base = { glyph: '❺', gC: T1, note: '', wasG: was.glyph, was: was.word + ' · version ' + V, hasStop: false, chip: latestChip, eyebrow: '⑤ Re-checked · ' + fd.path };
      if (changedAfter(fd)) det = Object.assign(base, {
        text: 'Checked again at ' + latest.time + ' on ' + V + ' with ' + scopeFor(fd.path, latest) + ' in scope: ' + res(latest) + '.' });
      else if (fd.all.length) det = Object.assign(base, {
        text: 'Checked again at ' + latest.time + ' on ' + V + ': ' + res(latest) + '. It counts for ' + fd.path + ' because ' + RULE + '.' });
      else det = Object.assign(base, {
        text: 'Ran again at ' + latest.time + ' on ' + V + ', with ' + latest.paths.filter((q) => cited.paths.indexOf(q) < 0).join(', ') + ' added to its scope: ' + res(latest) + '.' });
    }
    det.hasNote = !!det.note;
    det.hasWas = !!det.was;
    det.wasG = det.wasG || '';
    det.hasChipNote = done && isDemo;
    det.chipNote = ILLUS;

    // ---------- coverage on this version ----------
    const segs = (c) => [['none', 'none'], ['older', cited ? cited.version : 'older'], ['now', V]].map((pair) => {
      const k = pair[0], lab = pair[1];
      if (k !== c) return { text: lab, border: '1px solid #1F1F23', bg: 'transparent', color: T3 };
      if (k === 'none') return { text: '○ none', border: '1px dashed #82828C', bg: 'transparent', color: T1 };
      if (k === 'older') return { text: '◭ ' + lab, border: '1px solid rgba(242,201,76,.6)', bg: HATCH, color: YE };
      return { text: V, border: '1px solid #EDEDEF', bg: '#EDEDEF', color: '#000000' };
    });
    const covWord = (d) => d.cov === 'now' ? 'on this version, ' + V : d.cov === 'older' ? 'on an older version, ' + d.run.version : 'none';
    const cov = FILES.map((f) => {
      const d = FD[f.id];
      const sel = fileId === f.id;
      return {
        path: d.path, pathShort: tailPath(d.path, 28), pathFull: isDemo ? undefined : d.path,
        cap: d.cov === 'none' ? 'no check yet' : stp(d.run.step),
        segs: segs(d.cov),
        pick: pick(f.id),
        pressed: sel ? 'true' : 'false',
        border: sel ? '#FF7A29' : '#1F1F23',
        bg: sel ? '#111113' : '#0A0A0B',
        aria: d.path + ': proof coverage ' + covWord(d) + '.'
      };
    });

    // ---------- header (the headline never carries a state word; its count equals the status bar's) ----------
    const nChanged = FILES.filter((f) => changedAfter(FD[f.id])).length;
    const VERDICT = { not_proven: 'notproven', needs_recheck: 'recheck', proven: 'proven', contradicted: 'contradicted' };
    const stState = VERDICT[after ? after.statementVerdict : focus && focus.proof ? focus.proof.verdict : null] || 'notproven';
    const matched = !!latest && (!!latest.matchedBy || (!!focus && !!focus.proof && !!focus.proof.restsOn && focus.proof.restsOn.basis === 'matched' && focus.proof.restsOn.runId === latest.id));
    const PROOF_WORD = cited ? 'since the proof' : 'in this Change';
    const sp = PILL[stState === 'proven' ? 'proven' : stState === 'recheck' ? 'older' : stState === 'contradicted' ? 'contradicted' : 'none'];
    const head = done ? {
      title: nChanged === 0 ? 'No file changed after ' + stp(cited.step) + ', and a check on ' + V + ' covers this version.'
        : plural(nChanged, 'file', 'files') + ' changed after ' + stp(cited.step) + ', and a check on ' + V + ' now covers ' + (nChanged === 1 ? 'it' : 'them') + '.',
      pGlyph: sp.glyph, pG: sp.g, pWord: sp.word, pColor: sp.color, pBorder: sp.border, pBg: sp.bg,
      meta: '· version ' + V + ' · ' + latest.paths.join(', '), metaC: stState === 'proven' ? GR : T2,
      rest: '· rests on ' + stp(latest.step) + (matched ? ' (matched by ' + (latest.matchedBy || 'ThruLine') + ')' : '') + ', which ran on ' + latest.version + (latest.time ? ' at ' + latest.time : '')
    } : {
      title: cited && nChanged === 0 ? 'No file changed since the proof ran' + (cited.version === V ? ' on ' + V : '') + '.'
        : !cited ? plural(nChanged, 'file', 'files') + ' changed in this Change, and nothing in the record has checked ' + (nChanged === 1 ? 'it' : 'them') + '.'
        : checking ? plural(nChanged, 'file', 'files') + ' changed since the proof ran. A check on ' + V + ' is running.' : plural(nChanged, 'file', 'files') + ' changed since the proof ran, and nothing has checked ' + (nChanged === 1 ? 'it' : 'them') + '.',
      pGlyph: sp.glyph, pG: sp.g, pWord: sp.word, pColor: sp.color, pBorder: sp.border, pBg: sp.bg,
      meta: '· version ' + V, metaC: T2,
      rest: cited ? '· rests on ' + (cited.step != null ? stp(cited.step) : 'a check ThruLine ran') + ', which ran on ' + cited.version : '· no run in the record backs it'
    };
    const two = (n) => (n < 10 ? '0' : '') + n;
    const clock = () => { const t = new Date(); return two(t.getHours()) + ':' + two(t.getMinutes()); };
    const act = {
      /* "Ready for approval" is the Change's readiness (the shared model's state), never this screen's own reading */
      idle: isDemo ? idle : !checking && D.change.state !== 'ready', checking: checking, done: isDemo ? done && !checking : !checking && D.change.state === 'ready',
      check: check, stop: stop,
      stopped: stopped,
      plainSub: idle && !stopped,
      stoppedText: 'You stopped this check at ' + (isDemo ? D.afterCheck.run.time : D.stopped ? D.stopped.at : clock())
        + ' after ' + (isDemo ? s.stoppedAfter : D.stopped ? D.stopped.elapsedS : 0) + ' s. What it printed is kept, and it settles nothing.'
    };

    // ---------- the six-stage strip: progress glyphs (Verify is where you are) ----------
    const SG = { done: '✓', current: '●', not_started: '○', not_reached: '—' };
    const G = (after ? after.stages : D.change.stages).map((x) => SG[x.status]);
    const gc = (g) => g === '●' ? T1 : g === '—' ? T3 : T2;
    const stg = { g0: G[0], c0: gc(G[0]), g1: G[1], g2: G[2], c2: gc(G[2]), g3: G[3], c3: gc(G[3]), g4: G[4], c4: gc(G[4]), g5: G[5], c5: gc(G[5]) };

    const filesN = plural(FILES.length, 'file', 'files');
    const nShown = FILES.filter((f) => FD[f.id].shown.length).length;
    const fmtBytes = (b) => (b >= 1e9 ? (b / 1e9).toFixed(1) + ' GB' : b >= 1e6 ? (b / 1e6).toFixed(1) + ' MB' : Math.round(b / 1e3) + ' KB');
    const onV = runs.filter((r) => r.version === V && r.ended === 'succeeded');
    return {
      keys: {
        primary: check,
        copy: () => (!isDemo && acts.openTell ? acts.openTell() : false),
        approve: () => (!isDemo && act.done && acts.openApprove ? acts.openApprove() : false),
      },
      /* C30 R-PREFS: the altitudes are links (Brief → brief, Explore → statements). This screen is
       * the Workbench, so live it is Workbench that is marked; the canvas board marks Brief, and the
       * gallery keeps the board */
      alt: isDemo ? altitudes('brief', { page: false }) : altitudes('workbench'),
      top: { read: (idle && !stopped) || !isDemo ? D.view.readAt : D.afterCheck.run.time },
      stg: stg,
      banner: { show: isDemo && s.banner, canReplay: !idle || stopped, replay: replay, hide: hideBanner },
      head: head,
      act: act,
      left: { count: since ? filesN + ' · ' + fmtCount(nShown) + ' changed ' + SINCE : whole ? filesN + ' · ' + fmtCount(nShown) + ' edited in this Change' : filesN + ' · ' + fmtCount(nShown) + ' changed since ' + verLabel(fi) },
      tg: tg,
      bands: bands,
      nowBand: nowBand,
      groups: groups,
      det: det,
      diff: diff,
      cov: cov,
      status: {
        left: done
          ? filesN + ' · ' + fmtCount(nChanged) + ' changed after ' + stp(cited.step) + ' · ' + plural(onV.length, 'check', 'checks') + ' on ' + V + ': ' + res(latest) + (D.change.recording ? ' · recording' : '')
          : checking
            ? filesN + ' · ' + fmtCount(nChanged) + ' changed ' + PROOF_WORD + ' · a check is running on ' + V + (D.change.recording ? ' · recording' : '')
            : stopped
              ? filesN + ' · ' + fmtCount(nChanged) + ' changed ' + PROOF_WORD + ' · 1 check on ' + V + ' stopped, settles nothing' + (D.change.recording ? ' · recording' : '')
              : filesN + ' · ' + fmtCount(nChanged) + ' changed ' + PROOF_WORD + ' · ' + plural(onV.length, 'check', 'checks') + ' on ' + V + (D.change.recording ? ' · recording' : '')
      },
      /* content the markup used to carry as literals */
      isDemo: isDemo,
      c: {
        project: D.project.name,
        title: D.change.title,
        version: V,
        versionMeta: [VERS.length && VERS[VERS.length - 1].id === V ? '(current)' : null, D.version.commit ? '= commit ' + D.version.commit : null,
          D.version.unsavedChanges == null ? null : D.version.unsavedChanges ? 'unsaved changes' : 'no unsaved changes'].filter(Boolean).join(' · '),
        recording: isDemo ? 'Recording · following live' : recordingPill({ change: D.change }).text,
        replay: 'Replay from ' + D.view.readAt,
        hasFocus: !!focus,
        quote: focus ? focus.text : '',
        saidBy: focus ? focus.agent + (focus.step != null ? ' · ' + stp(focus.step) : '') : '',
        checkOn: (CHECK.command.length > 90 ? CHECK.command.slice(0, 89) + '…' : CHECK.command) + ' · on ' + V,
        askOnce: (CHECK.askOncePerProject || acts.needsPermission ? ' · asks once for this project' : '') + (canRun ? '' : ' · ' + RUN_WHY),
        runDisabled: canRun ? undefined : true,
        runOpacity: canRun ? undefined : '0.5',
        runWhy: canRun ? undefined : RUN_WHY,
        onVersion: '· ' + V,
        approveNote: 'Approving records; it does not merge or deploy. Any edit after ' + (latest ? latest.time : '') + ' puts this back to Needs a re-check.',
        nowLabel: V + ' · now',
        colOn: 'On ' + V,
        wasAt: 'At ' + D.view.readAt + ' this read: ',
        stopText: 'Nothing on ' + V + ' has run with ' + fd.path + ' in scope.',
        lookedAt: (() => {
          const rs = runs.filter((r) => !(after && r.id === after.run.id));
          const fin = rs.filter((r) => r.ended !== 'unknown');
          const open = rs.filter((r) => r.ended === 'unknown');
          return 'Looked at: ' + plural(fin.length, 'finished run', 'finished runs') + (fin.length ? ' (' + fin.map((r) => stp(r.step) + ' on ' + r.version).join(', ') + ')' : '')
            + (open.length ? ' and ' + open.map((r) => stp(r.step)).join(', ') + ', which ha' + (open.length === 1 ? 's' : 've') + ' no result yet' : '') + '.';
        })(),
        legendNone: 'no check has run it on ' + V,
        legendOlder: cited ? 'ran on ' + cited.version + ' only' : 'ran on an older version only',
        legendNow: 'covered by a check on ' + V,
        coverageOn: 'Coverage on ' + V,
        covMax: FILES.length > 6 ? '170px' : undefined,
        storage: storageSentence(D.machine.recordsBytes),
      },
    };
  }
}
