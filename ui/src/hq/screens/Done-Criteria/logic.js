/* Done-Criteria.dc.html: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from `this.props.data`
 * (live) or DEMO (the design gallery, where the demo switcher picks a variant);
 * this.state holds UI state only: each line's decision while it is being made,
 * the open editor, the composer, the face, and (live) the lines a person adds
 * and what they mark out of scope — a draft, written only by the accept act. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { recordingPill, storageSentence } from '../../data/project.js'
import { fmtCount } from '../../data/count.js'
import { altitudes } from '../Change-Brief/altitude.js'
import { faceOf, switchFace } from '../../data/face.js'
import { isStep, stepLinker } from '../Step-View/link.js'
import { acceptedAfterWork } from '../../data/after-work.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

const dataOf = (props) => (props && props.data) || DEMO;
/* the longest line one acceptance takes (change_propose::MAX_CRITERION_CHARS) */
const MAX_LINE_CHARS = 240;
/* the canvas's stage glyphs, and the one Change model's (data/change.js) */
const STAGE_GLYPH = { done: '✓', current: '●', not_started: '○', not_reached: '—' };
const sameWords = (a, b) => String(a || '').trim().toLowerCase().replace(/\s+/g, ' ').replace(/[.!?;]+$/, '') === String(b || '').trim().toLowerCase().replace(/\s+/g, ' ').replace(/[.!?;]+$/, '');
const foldersOf = (D) => {
  const out = {};
  D.covers.forEach(function (f) { out[f.path] = !!f.included; });
  return out;
};

export default class Component extends DCLogic {
  state = {
    lines: dataOf(this.props).criteria.lines.map(function (l) { return l.state; }),
    links: dataOf(this.props).criteria.lines.map(function (l) { return !!l.linked; }),
    texts: dataOf(this.props).criteria.lines.map(function () { return ''; }),
    editing: this.props && this.props.data ? -1 : 1,
    draft: this.props && this.props.data ? '' : DEMO.criteria.lines[1].text,
    recorded: !!dataOf(this.props).criteria.recorded,
    ver: dataOf(this.props).criteria.version,
    folders: foldersOf(dataOf(this.props)),
    composer: '',
    composerText: '',
    tplOpen: false,
    /* live: lines this person added (each {text, state, template?}) and what they mark out of scope */
    added: [],
    outOfScope: [],
    demo: 'reviewing',
    face: 'plain'
  };

  baseText(i) {
    const L = dataOf(this.props).criteria.lines;
    if (i < L.length) return L[i].text;
    const a = (this.state.added || [])[i - L.length];
    return a ? a.text : '';
  }

  lineText(i) {
    if (i >= dataOf(this.props).criteria.lines.length) return this.baseText(i);
    const t = this.state.texts[i];
    return t ? t : this.baseText(i);
  }

  /* a line this person added: its decision or wording changes in place */
  setAdded(i, patch, extra) {
    const j = i - dataOf(this.props).criteria.lines.length;
    const added = (this.state.added || []).slice();
    if (!added[j]) return;
    added[j] = Object.assign({}, added[j], patch);
    this.setState(Object.assign({ added: added, demo: '' }, extra || {}));
  }

  setLine(i, v) {
    if (i >= dataOf(this.props).criteria.lines.length) {
      this.setAdded(i, { state: v }, v === 'dropped' && this.state.editing === i ? { editing: -1 } : null);
      return;
    }
    const L = dataOf(this.props).criteria.lines;
    const lines = (this.props.data && this.state.lines.length !== L.length ? L.map(function (l) { return l.state; }) : this.state.lines).slice();
    lines[i] = v;
    const patch = { lines: lines, demo: '' };
    if (v === 'dropped' && this.state.editing === i) patch.editing = -1;
    this.setState(patch);
  }

  toggleLink(i) {
    const links = this.state.links.slice();
    links[i] = !links[i];
    this.setState({ links: links, demo: '' });
  }

  startEdit(i) {
    if (this.state.editing === i) { this.setState({ editing: -1, demo: '' }); return; }
    this.setState({ editing: i, draft: this.lineText(i), demo: '' });
  }

  saveEdit(i) {
    const d = String(this.state.draft || '').trim();
    if (i >= dataOf(this.props).criteria.lines.length) {
      if (d) this.setAdded(i, { text: d }, { editing: -1 });
      else this.setState({ editing: -1, demo: '' });
      return;
    }
    const texts = this.state.texts.slice();
    if (d && d !== this.baseText(i)) texts[i] = d;
    if (d === this.baseText(i)) texts[i] = '';
    this.setState({ texts: texts, editing: -1, demo: '' });
  }

  /* the design gallery's demo switcher: DEMO only */
  applyDemo(name) {
    const v = DEMO.states[name];
    const base = { demo: name, composer: '', composerText: '', tplOpen: false, ver: DEMO.criteria.version, texts: DEMO.criteria.lines.map(function () { return ''; }), folders: foldersOf(DEMO) };
    base.lines = v.lines.slice();
    base.links = v.links.slice();
    base.editing = name === 'reviewing' ? 1 : -1;
    base.draft = name === 'reviewing' ? this.baseText(1) : '';
    base.recorded = !!v.recorded;
    this.setState(base);
  }

  renderVals() {
    const s = this.state;
    const isDemo = !this.props.data;
    const D = dataOf(this.props);
    const V = D.version.id;
    const Ldata = D.criteria.lines;
    const rec = isDemo ? s.recorded : !!D.criteria.recorded && !s.reopen;
    /* live, before a version is recorded: the lines this person added follow the record's */
    const addedLines = isDemo || rec ? [] : (s.added || []).map(function (a, j) {
      return {
        n: Ldata.length + j + 1, text: a.text, kind: 'behaviour',
        source: a.template ? { kind: 'template', quote: a.template } : { kind: 'yours' },
        check: D.recheck ? D.recheck.command : null, namedTest: null,
        evidence: 'none', state: a.state, linked: false, proposal: null, added: true
      };
    });
    const L = Ldata.concat(addedLines);
    const nL = L.length;
    const run = D.lastRun, early = D.earlierRun, moved = D.movedEdit;
    const A = (!isDemo && this.props.actions) || {};
    const act = D.act || {};
    const stepTo = isDemo ? () => undefined : stepLinker(this.props.at, act.project);
    const runStep = run && isStep(run.step) ? run.step : null;
    const runName = runStep ? 'step ' + runStep : 'a check';
    const linkRunName = runStep ? 'step ' + runStep : 'check run';
    const runHref = stepTo(runStep);
    const stepOr = (r) => (r && r.step != null ? 'step ' + r.step : 'nothing yet');
    const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', OR = '#FF7A29', LINE = '#2A2A30';
    const YEL = '#F2C94C', BLUE = '#52A8FF';
    const GLOW = '0 0 6px rgba(255,154,82,.6)';
    const HATCH = 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)';
    /* live: what is recorded comes from the record on every read; this screen's
       own draft (decisions, wording, folders) lives in state until it is accepted */
    const dataStates = Ldata.map(function (l) { return l.state; });
    const lines = isDemo ? s.lines : (rec || s.lines.length !== Ldata.length ? dataStates : s.lines).concat(addedLines.map(function (l) { return l.state; }));
    const ver = isDemo ? s.ver : D.criteria.version + (s.reopen && D.criteria.recorded ? 1 : 0);
    const baseVer = D.criteria.version;
    /* C30 R-PREFS: the face is the stored one on a live screen; the gallery keeps the canvas's own toggle */
    const tech = faceOf(this.props, s.face) === 'technical';
    const whenFirst = D.criteria.acceptedAt || (D.demoTimes && D.demoTimes.accept) || 'just now';
    const when = ver === baseVer ? whenFirst : 'just now';
    const by = D.criteria.acceptedBy;
    const lastEditAt = D.agent.lastEdit.at || 'an unrecorded time';
    const acc = lines.filter(function (x) { return x === 'accepted'; }).length;
    const prop = lines.filter(function (x) { return x === 'proposed'; }).length;
    const drp = lines.filter(function (x) { return x === 'dropped'; }).length;
    const provenOnV = L.filter(function (l) { return l.provenOn === V || l.modelState === 'proven'; }).length;
    const things = function (n) { return fmtCount(n) + (n === 1 ? ' thing still needs' : ' things still need'); };
    const isLinked = function (i) { return L[i].evidence === 'candidate' && !!s.links[i]; };
    const tally = function (r) { return !r || (r.passed == null && r.failed == null) ? 'no test count recorded' : r.failed ? fmtCount(r.failed) + ' failed, ' + fmtCount(r.passed) + ' passed' : fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed'; };
    const WORDS = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten'];
    const word = function (n) { return n >= 0 && n < WORDS.length ? WORDS[n] : fmtCount(n); };
    const andList = function (a) { return a.length <= 1 ? String(a[0] === undefined ? '' : a[0]) : a.slice(0, -1).join(', ') + ' and ' + a[a.length - 1]; };
    const gb = function (bytes) { return (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GB'; };
    const stepRec = runStep ? 'step ' + runStep + ' ›' : '';

    // ---- what the Change covers (drives "Depends on" and the conservative rule) ----
    const folderDefs = D.covers.map(function (f) {
      const n = f.editSteps.length;
      const note = n
        ? fmtCount(n) + (n === 1 ? ' edit · step ' : ' edits · steps ') + f.editSteps.join(', ')
        : 'no edits' + (f.holdsCheck ? ' · the check lives here' : '');
      return { key: f.path, path: f.path, note: note, included: !!f.included };
    });
    /* a folder is on when the person said so; one they have not touched
       follows the record (live, the folders can change under the screen: a
       Change tracked here arrives with its own proposed folders), and a
       recorded version covers what it covers */
    const folderOn = function (f) {
      if (!isDemo && rec) return true;
      return Object.prototype.hasOwnProperty.call(s.folders, f.key) ? !!s.folders[f.key] : f.included;
    };
    const onPaths = folderDefs.filter(folderOn).map(function (f) { return f.path; });
    const joinOr = function (a) { return a.length <= 1 ? (a[0] || '') : a.slice(0, -1).join(', ') + ' or ' + a[a.length - 1]; };
    const ruleSentence = onPaths.length
      ? 'No line names its own files yet, so each one depends on everything this Change covers: a later change anywhere in ' + joinOr(onPaths) + ' reopens every line.'
      : 'No folders are chosen, so any later change anywhere reopens every line.';
    const deps = onPaths.length ? onPaths.map(function (p) { return { path: p }; }) : [{ path: 'the whole repository' }];
    const depsNote = onPaths.length
      ? 'names no files of its own, so a later change anywhere in these reopens it'
      : 'no folders are chosen, so any later change reopens it';
    /* live, changing a recorded version: the folders it covers stay covered.
       The accept act only adds to them (change_propose: "narrowing is a
       re-scope, and it is a different act"), so no ✕ is offered that would
       be silently undone. */
    const keepsFolders = !isDemo && !rec && !!D.criteria.recorded;
    const folders = folderDefs.map((f) => {
      const on = folderOn(f);
      return {
        path: f.path,
        note: f.note,
        onEdit: on && !rec && !keepsFolders,
        onLocked: on && (rec || keepsFolders),
        offEdit: !on && !rec,
        removeLabel: 'Remove ' + f.path + ' from what this Change covers',
        addLabel: 'Put ' + f.path + ' back in what this Change covers',
        toggle: () => {
          const next = Object.assign({}, this.state.folders);
          next[f.key] = !on;
          this.setState({ folders: next, demo: '' });
        }
      };
    });

    // ---- the proposed lines ----
    const span = function (r) { return isStep(r.step) ? (isStep(r.startStep) ? 'step ' + r.startStep + ' → ' + r.step : 'step ' + r.step) : 'check run'; };
    const runFull = !run ? 'no run in the record yet' : [span(run), run.command, run.exit != null ? 'exit ' + run.exit : null, tally(run), run.version, run.capturedBy ? 'captured by ' + run.capturedBy : null].filter(Boolean).join(' · ');
    const SOURCE = {
      words: function (src) { return 'from your words “' + src.quote + '”'; },
      name: function (src) { return 'from the Change’s name “' + src.quote + '”'; },
      check: function () { return 'a check that must pass, not a behaviour'; },
      proposal: function () { return 'proposed from your request'; },
      accepted: function () { return 'accepted in v' + D.criteria.version; },
      yours: function () { return 'your words · added here'; },
      template: function (src) { return 'from the template “' + src.quote + '”'; }
    };
    const DEF = L.map(function (l) {
      const gate = l.kind === 'gate';
      return {
        kind: gate ? 'Gate' : 'Behaviour',
        source: SOURCE[l.source.kind](l.source),
        provenLabel: gate ? 'Checked by' : 'Would be proven by', cmd: l.check || 'no check named yet', cmdTail: l.check ? 'on the version you approve' : '',
        extraText: gate ? 'the command this Change ran at' : l.namedTest ? 'named test' : '',
        extraMono: gate ? 'steps ' + (l.ranAtSteps || []).join(', ') : l.namedTest || '',
        rests: gate ? 'a pass of the declared check' : 'a passing run of that check'
      };
    });

    // what the record holds for each line, before the version is recorded
    const recNow = function (i) {
      const ev = L[i].evidence;
      if (ev === 'none' || ev === 'engine') {
        if (ev === 'engine' && L[i].line) return { label: 'In the record now:', text: L[i].line, receipt: '' };
        return run
          ? { label: 'In the record now:', text: runName + ' ran ' + (run.command || 'a test') + ' on ' + run.version + '. It isn’t linked to this line.', receipt: stepRec }
          : { label: 'In the record now:', text: 'no test run in the record yet.', receipt: '' };
      }
      if (ev === 'candidate') {
        return isLinked(i)
          ? { label: 'In the record now:', text: 'you linked ' + runName + ' to this line. It ran on ' + run.version + ', before ' + moved.file + ' changed at step ' + moved.step + ', so this line will need a re-check on ' + V + '.', receipt: stepRec }
          : { label: 'In the record now:', text: runName + ' ran ' + run.file + ' on ' + run.version + '. It isn’t linked to this line.', receipt: stepRec };
      }
      if (ev === 'looked_at' && run && early) return { label: 'Looked at:', text: '2 finished runs, steps ' + early.step + ' and ' + run.step + ', both of ' + run.file + ' only; step ' + D.pendingRun.step + ' has no result yet. No finished run has included ' + D.uncovered + '.', receipt: '' };
      return { label: 'In the record now:', text: runName + ' ran it on ' + run.version + ': ' + tally(run) + ', before ' + moved.file + ' changed at step ' + moved.step + '.', receipt: stepRec };
    };

    // the state each accepted line reads once the version is recorded (spec §6.2 K1)
    const nowOf = function (i) {
      const ev = L[i].evidence;
      /* live, once accepted: the engine's state for the line, in the design's marks */
      if (ev === 'engine' || (!isDemo && ev === 'candidate')) {
        const ms = L[i].modelState;
        const text = L[i].line || '';
        if (ms === 'proven') return { kind: 'proven', glyph: '●', word: 'Proven', color: 'var(--tl-proven, #3DD68C)', border: '1px solid rgba(var(--tl-proven-rgb, 61, 214, 140), .5)', hatch: 'none', edge: '2px solid var(--tl-proven, #3DD68C)', text: text, basis: '', receipt: !!runHref, canLink: false };
        if (ms === 'needs_recheck') return { kind: 'recheck', glyph: '◭', word: 'Needs a re-check', color: YEL, border: '1px solid rgba(242,201,76,.6)', hatch: HATCH, edge: '2px solid ' + YEL, text: text, basis: '', receipt: !!runHref, canLink: false };
        if (ms === 'in_progress' || ev === 'candidate') return { kind: 'progress', glyph: '◐', word: 'In progress', color: BLUE, border: '1px solid rgba(82,168,255,.6)', hatch: 'none', edge: '2px solid ' + BLUE, text: text, basis: '', receipt: !!runHref, canLink: ev === 'candidate' && !!act.unlinkedRun };
        return { kind: 'open', glyph: '○', word: L[i].word || 'Not proven', color: T2, border: '1px solid #2A2A30', hatch: 'none', edge: '2px solid #2A2A30', text: text, basis: '', receipt: !!runHref, canLink: false };
      }
      const recheck = ev === 'check' || isLinked(i);
      if (!moved || !run) return { kind: 'open', glyph: '○', word: 'Not proven', color: T2, border: '1px solid #2A2A30', hatch: 'none', edge: '2px solid #2A2A30', text: '', basis: '', receipt: false, canLink: false };
      if (recheck) {
        return {
          kind: 'recheck', glyph: '◭', word: 'Needs a re-check', color: YEL,
          border: '1px solid rgba(242,201,76,.6)', hatch: HATCH, edge: '2px solid ' + YEL,
          text: 'Proven on the previous version — needs a re-check. The affected code changed afterward: ' + moved.file + ', step ' + moved.step + '.',
          basis: ev === 'check' ? 'Looked at because the checks you accepted cover this kind of thing.' : 'You linked this yourself.',
          receipt: true, canLink: false
        };
      }
      return {
        kind: 'progress', glyph: '◐', word: 'In progress', color: BLUE,
        border: '1px solid rgba(82,168,255,.6)', hatch: 'none', edge: '2px solid ' + BLUE,
        text: 'A test ran for this Change but isn’t linked to this line — link it.',
        basis: ev === 'looked_at'
          ? 'Looked at: ' + runName + ' ran ' + run.file + ' on ' + run.version + '; it did not include ' + D.uncovered + '.'
          : 'Looked at: ' + runName + ' ran ' + run.file + ' on ' + run.version + '; nobody has linked it to this line.',
        receipt: false, canLink: ev === 'candidate'
      };
    };

    const techOf = function (i) {
      const ev = L[i].evidence;
      if (!isDemo && ev !== 'check') return runFull;
      if (ev === 'looked_at') {
        return 'step ' + early.startStep + ' → ' + early.step + ' · exit ' + early.exit + ' · ' + tally(early) + ' · ' + early.version
          + ' · step ' + run.startStep + ' → ' + run.step + ' · exit ' + run.exit + ' · ' + run.version
          + ' · both: ' + run.file + ' · step ' + D.pendingRun.step + ' · no result yet';
      }
      if (ev === 'check') return 'check: ' + L[i].check + ' · last run step ' + run.startStep + ' → ' + run.step + ' · exit ' + run.exit + ' · ' + tally(run) + ' · ' + run.version + ' · captured by ' + run.capturedBy;
      return runFull + ' · link: ' + (isLinked(i) ? 'person (you)' : 'none');
    };

    const litAt = function (i) { return lines[i] === 'accepted'; };
    const litBelow = function (i) { for (let j = i + 1; j < nL; j++) { if (litAt(j)) return true; } return false; };
    const kindCount = function (k) { return L.filter(function (l) { return l.kind === k; }).length; };
    const firstOfKind = function (i) { return L.findIndex(function (l) { return l.kind === L[i].kind; }) === i; };

    let nRe = 0, nIn = 0;
    const cards = DEF.map((d, i) => {
      const st = lines[i];
      const isAcc = st === 'accepted';
      const isDrop = st === 'dropped';
      const isEditing = !rec && !isDrop && s.editing === i;
      const hasLabel = firstOfKind(i);
      const brY = hasLabel ? 53 : 27;
      const topLit = isAcc || litBelow(i);
      const botLit = litBelow(i);
      const n = String(L[i].n);
      const edited = !!s.texts[i];

      let stateWord;
      if (rec) stateWord = isAcc ? 'Accepted in v' + ver : (isDrop ? 'Dropped · recorded in v' + ver : 'Not in v' + ver + ' · still proposed');
      /* live, a drop is not recorded: the record keeps no row that says a line
         was dropped, so the line simply stays out of the version */
      else stateWord = isAcc ? 'Accepted' : (isDrop ? (isDemo ? 'Dropped · recorded with v' + ver : 'Dropped · stays out of v' + ver) : 'Proposed');

      let firstText, disc1;
      if (isAcc && rec) { firstText = by + ' · ' + when + ' · after the work'; disc1 = { b: '1px solid #EDEDEF', bg: '#EDEDEF', c: '#000000' }; }
      else if (isAcc) { firstText = by + ' · when v' + ver + ' is recorded'; disc1 = { b: '1px solid #EDEDEF', bg: 'transparent', c: T1 }; }
      else { firstText = 'by you, once you accept it'; disc1 = { b: '1px dashed #66666F', bg: 'transparent', c: T2 }; }

      const planned = { b: '1px dashed #66666F', bg: 'transparent', c: T2 };
      const linkDefs = [
        { n: '1', word: 'ACCEPTED', text: firstText, mono: '', disc: disc1, wc: isAcc ? T1 : T2 },
        { n: '2', word: 'RESTS ON', text: d.rests, mono: '', disc: planned, wc: T2 },
        { n: '3', word: 'RAN ON', text: 'the version you approve · now', mono: V, disc: planned, wc: T2 },
        { n: '4', word: 'SINCE', text: 'nothing it depends on changed after', mono: '', disc: planned, wc: T2 },
        { n: '5', word: 'RE-CHECKED', text: 'only needed if ④ moved', mono: '', disc: planned, wc: T2 },
        { n: '6', word: 'SO', text: 'the state on that version', mono: '', disc: planned, wc: T2 }
      ];
      const chain = linkDefs.map(function (l, k) {
        return {
          n: l.n, word: l.word, text: l.text, mono: l.mono, wordColor: l.wc,
          discBorder: l.disc.b, discBg: l.disc.bg, discColor: l.disc.c,
          connDisplay: k < 5 ? 'block' : 'none'
        };
      });

      let lockedNote = '';
      if (rec) lockedNote = isAcc ? 'v' + ver + ' · ' + when : (isDrop ? 'dropped · v' + ver : 'not in v' + ver);

      const rn = recNow(i);
      const now = nowOf(i);
      const showNow = rec && isAcc;
      if (showNow) { if (now.kind === 'recheck') nRe++; else if (now.kind === 'progress') nIn++; }
      const draftChanged = isEditing && String(s.draft || '').trim() !== this.lineText(i);
      const candidate = L[i].evidence === 'candidate';

      return {
        num: n,
        text: this.lineText(i),
        kind: d.kind,
        isGate: d.kind === 'Gate',
        isBehaviour: d.kind !== 'Gate',
        ariaLabel: 'Line ' + n + ', ' + stateWord,
        hasLabel: hasLabel,
        label: d.kind + ' · ' + fmtCount(kindCount(L[i].kind)),
        labelNote: d.kind === 'Gate' ? 'a command that must pass; kept apart from behaviour' : 'what the code should do',
        // thread
        brY: brY + 'px',
        brY1: (brY - 1) + 'px',
        nodeY: (brY - 4) + 'px',
        topBg: topLit ? OR : LINE,
        topGlow: topLit ? GLOW : 'none',
        botBg: botLit ? OR : LINE,
        botGlow: botLit ? GLOW : 'none',
        botDisplay: i === nL - 1 ? 'none' : 'block',
        nodeBg: isAcc ? OR : '#000000',
        nodeBorder: isAcc ? '0' : (isDrop ? '1px dashed #66666F' : '1px solid #66666F'),
        brBg: isAcc ? OR : (isDrop ? 'transparent' : LINE),
        brBorder: isDrop ? '2px dotted #66666F' : '0',
        brGlow: isAcc ? GLOW : 'none',
        // card
        cardBg: isDrop ? 'transparent' : '#111113',
        cardBorder: isDrop ? '1px solid #1F1F23' : (isAcc || isEditing ? '1px solid #66666F' : '1px solid #2A2A30'),
        glyph: isAcc ? '●' : (isDrop ? '—' : '○'),
        glyphColor: isAcc ? T1 : T2,
        textColor: isDrop ? T2 : T1,
        textDeco: isDrop ? 'line-through' : 'none',
        stateWord: stateWord,
        stateColor: isAcc ? T1 : T2,
        source: edited ? 'your wording · ' + d.source : d.source,
        notEditing: !isEditing,
        isEditing: isEditing,
        draft: isEditing ? s.draft : '',
        onDraft: (e) => this.setState({ draft: e && e.target ? String(e.target.value) : '', demo: '' }),
        editLabel: 'Line ' + n + ' · say one thing a check could observe',
        editNote: 'Your wording is recorded with v' + ver + ', with who and when. The plan for this line shows again when you save.',
        showDecide: !rec && !isDrop && !isEditing,
        showRestore: !rec && isDrop,
        showLocked: rec,
        lockedNote: lockedNote,
        accLabel: isAcc ? 'Accepted' : 'Accept',
        accAria: (isAcc ? 'Accepted' : 'Accept') + ', line ' + n,
        editAria: 'Edit line ' + n,
        dropAria: 'Drop line ' + n,
        saveAria: 'Save line ' + n,
        cancelAria: 'Cancel editing line ' + n,
        restoreAria: 'Restore line ' + n,
        accPressed: isAcc ? 'true' : 'false',
        accBg: isAcc ? '#1F1F23' : 'transparent',
        accBorder: isAcc ? '1px solid #EDEDEF' : '1px solid #66666F',
        showPlan: !isDrop && !isEditing,
        provenLabel: d.provenLabel,
        cmd: d.cmd,
        cmdTail: d.cmdTail,
        extraText: d.extraText,
        extraMono: d.extraMono,
        deps: deps,
        depsNote: depsNote,
        chainLabel: 'How line ' + n + ' would be proven',
        chain: chain,
        // the record, before the version is recorded
        showRecNow: !showNow,
        recLabel: rn.label,
        recText: rn.text,
        receipt: rn.receipt,
        hasReceipt: rn.receipt !== '' && (isDemo || !!runHref),
        receiptHref: runHref,
        canLink: isDemo && !rec && candidate,
        linkLabel: isLinked(i) ? 'Unlink' : runStep ? 'Link step ' + runStep : 'Link check run',
        linkColor: isLinked(i) ? T2 : T1,
        linkAria: isLinked(i) ? 'Unlink ' + linkRunName + ' from line ' + n : 'Link ' + linkRunName + ' to line ' + n,
        toggleLink: isDemo ? () => this.toggleLink(i) : () => { if (A.link && act.unlinkedRun) A.link(L[i].n); },
        // the state once the version is recorded
        showNow: showNow,
        nowGlyph: now.glyph,
        nowWord: now.word,
        nowColor: now.color,
        nowBorder: now.border,
        nowHatch: now.hatch,
        nowEdge: now.edge,
        nowText: now.text,
        nowBasis: now.basis,
        hasNowReceipt: now.receipt && (isDemo || !!runHref),
        nowReceiptHref: runHref,
        canLinkNow: showNow && now.canLink,
        // technical face
        showTech: tech,
        tech: techOf(i),
        draftChanged: draftChanged,
        accept: () => this.setLine(i, isAcc ? 'proposed' : 'accepted'),
        edit: () => this.startEdit(i),
        drop: () => this.setLine(i, 'dropped'),
        restore: () => this.setLine(i, 'proposed'),
        save: () => this.saveEdit(i),
        cancel: () => this.setState({ editing: -1, demo: '' })
      };
    });

    // ---- counters and the accept bar ----
    let counter;
    if (rec) {
      const cp = [];
      if (nRe) cp.push(fmtCount(nRe) + (nRe === 1 ? ' needs' : ' need') + ' a re-check');
      if (nIn) cp.push(fmtCount(nIn) + ' in progress');
      cp.push(fmtCount(provenOnV) + ' proven on ' + V);
      counter = cp.join(' · ');
    } else {
      const counterParts = [nL ? fmtCount(acc) + ' of ' + fmtCount(nL) + ' accepted' : 'nothing proposed yet'];
      if (prop) counterParts.push(fmtCount(prop) + ' proposed');
      if (drp) counterParts.push(fmtCount(drp) + ' dropped');
      counter = counterParts.join(' · ');
    }

    let barLine1;
    if (rec) {
      barLine1 = 'What counts as done v' + ver + ' · accepted ' + when + ' by ' + by + ' · ' + fmtCount(acc) + (acc === 1 ? ' line' : ' lines');
    } else if (nL === 0) {
      barLine1 = act.changeId ? 'Nothing is proposed yet. Add a line, or start from a template, and accept what counts as done.' : 'Nothing is proposed yet: this work is not tracked as a Change, so there is nothing to accept.';
    } else if (acc === 0) {
      barLine1 = '0 of ' + fmtCount(nL) + ' accepted. Nothing is accepted for you: accept or drop each line.';
    } else {
      const parts = [fmtCount(acc) + ' of ' + fmtCount(nL) + ' accepted'];
      if (prop) parts.push(fmtCount(prop) + ' proposed ' + (prop === 1 ? 'stays' : 'stay') + ' out of v' + ver);
      if (drp) parts.push(fmtCount(drp) + (isDemo ? ' dropped, recorded with v' + ver : ' dropped, ' + (drp === 1 ? 'stays' : 'stay') + ' out of v' + ver));
      if (s.editing >= 0 && lines[s.editing] !== 'dropped') {
        const changed = String(s.draft || '').trim() !== this.lineText(s.editing);
        parts.push('line ' + (s.editing + 1) + (changed ? ' has an unsaved edit' : ' is open for editing'));
      }
      barLine1 = parts.join(' · ');
    }

    /* live: untracked work is tracked first (C30 R-UNTRACKED), and the accept
       act's own answer — the version it wrote, or the refusal in the server's
       words — is said here until the record is read back */
    const TR = (!isDemo && D.track) || {};
    const trackOffered = !isDemo && !act.changeId && !!TR.untracked;
    const ans = (!isDemo && act.accepting) || {};
    const readBack = !!D.criteria.recorded && D.criteria.version >= (ans.version || 0);
    if (!isDemo && !act.changeId && TR.loading) barLine1 = 'Reading whether this work is tracked as a Change…';
    else if (trackOffered) barLine1 = TR.said || (TR.busy ? 'Tracking this work as a Change…' : 'This work is not tracked as a Change yet, so there is nothing to accept.');
    else if (!rec && ans.busy) barLine1 = 'Recording what counts as done (v' + ver + ')…';
    else if (!rec && ans.said) barLine1 = ans.said;
    else if (ans.version && !readBack) barLine1 = 'Recorded as v' + ans.version + '. Reading it back…';
    const barLine2 = isDemo ? 'recorded with who and when · accepted after the work is shown as such'
      : trackOffered && !TR.said ? 'It records a Change named after your request, with this work as yours. It accepts and approves nothing.'
        : TR.note ? TR.note : 'recorded with who and when · accepted after the work is shown as such';

    let titleMeta, headline, headSub, defineSub;
    if (rec) {
      titleMeta = 'v' + ver + ' · accepted ' + when + ' by ' + by + ' · after the work';
      headline = things(acc) + ' evidence before ' + V + ' can be approved.';
      headSub = (acc === 1 ? 'Approval of ' + V + ' waits on that one line.' : 'Approval of ' + V + ' waits on all ' + fmtCount(acc) + '.')
        + (D.recheck && D.recheck.command ? ' One re-check reaches every line: ' + D.recheck.command + ' on ' + V + '.' : ' The record holds no command that re-checks these lines yet.');
      defineSub = 'v' + ver + ' accepted';
    } else if (ver === baseVer) {
      titleMeta = 'v' + ver + ' · proposed from your words · not accepted';
      headline = 'Nothing counts as done yet, so nothing is ready. Say what counts as done for this Change first.';
      headSub = 'The lines you accept below become what counts as done. Nothing is accepted for you, and accepting is not approving.';
      defineSub = 'not accepted yet';
    } else {
      titleMeta = 'v' + ver + ' · changes v' + (ver - 1) + ' · not accepted';
      headline = 'v' + (ver - 1) + ' still counts until you accept v' + ver + '.';
      headSub = 'v' + (ver - 1) + ' and anything measured against it stay in the record.';
      defineSub = 'v' + ver + ' not accepted';
    }

    const stepsHead = D.steps != null ? fmtCount(D.steps) + ' steps · ' : '';
    const recTail = isDemo || D.recording ? ' · recording' : '';
    const statusLeft = rec
      ? stepsHead + 'what counts as done v' + ver + ': ' + things(acc) + ' evidence on ' + V + recTail
      : !nL ? stepsHead + 'what counts as done: nothing proposed yet' + recTail
        : stepsHead + 'what counts as done: ' + fmtCount(acc) + ' of ' + fmtCount(nL) + ' accepted, not recorded yet' + recTail;

    const whyPoints = 'the ' + fmtCount(acc) + (acc === 1 ? ' line' : ' lines') + ' in v' + ver;

    const barAfterText = rec
      ? 'Recorded: accepted ' + when + ', after the agent’s last edit at ' + lastEditAt + '. The note travels with v' + ver + ': on every line’s ① ACCEPTED, on the approval and in the whole story.'
      : 'The agent’s last edit was at ' + lastEditAt + '. Accepting now records that the bar was set after the work. The note travels with v' + ver + ': on every line’s ① ACCEPTED, on the approval and in the whole story, so whoever reads it can see when the bar was set.';

    /* C30 R-PREFS: the altitudes are links to the open Change's three views (Brief → brief,
     * Explore → statements, Workbench → changed). This screen is none of them; the canvas marks
     * Brief, and so does the product (the group's current item, not this page). */
    const alts = altitudes('brief', { page: false }).list.map((a, i) => Object.assign({}, a, { borderLeft: i === 0 ? '0' : '1px solid #66666F' }));

    const demoDefs = [
      { id: 'fresh', label: 'Fresh' },
      { id: 'reviewing', label: 'Reviewing' },
      { id: 'dropped', label: 'One dropped' },
      { id: 'recorded', label: 'v1 recorded' }
    ];
    const demos = demoDefs.map((d) => ({
      label: d.label,
      pressed: s.demo === d.id ? 'true' : 'false',
      bg: s.demo === d.id ? '#2A2A30' : 'transparent',
      color: s.demo === d.id ? T1 : T2,
      border: s.demo === d.id ? '1px solid #66666F' : '1px solid #2A2A30',
      pick: () => this.applyDemo(d.id)
    }));

    const TN = D.templatesNote || {};
    const templates = D.templates.map((t) => {
      const row = { name: t.name, note: 'from ' + t.from };
      if (isDemo || !Array.isArray(t.lines) || !t.lines.length) return row;
      row.note = 'from ' + t.from + ' · ' + fmtCount(t.lines.length) + (t.lines.length === 1 ? ' line' : ' lines');
      row.pickLabel = 'Add the lines of the template “' + t.name + '” as proposals';
      row.pick = () => {
        const have = L.map((l, i) => this.lineText(i));
        const add = [];
        t.lines.forEach(function (text) {
          if (String(text).length <= MAX_LINE_CHARS && !have.concat(add.map(function (a) { return a.text; })).some(function (h) { return sameWords(h, text); })) add.push({ text: String(text), state: 'proposed', template: t.name });
        });
        this.setState({ added: (this.state.added || []).concat(add), tplOpen: false, demo: '' });
      };
      return row;
    });

    const scope = s.composer === 'scope';
    const recordedOut = isDemo ? [] : (D.outOfScope || []);
    const draftOut = isDemo || rec ? [] : (s.outOfScope || []);
    const outItems = recordedOut.map(function (t) { return { text: t, note: 'v' + baseVer, removable: false }; })
      .concat(draftOut.map((t, k) => ({
        text: t, note: 'with v' + ver, removable: true, removeLabel: 'Remove ' + t + ' from out of scope',
        remove: () => { const next = (this.state.outOfScope || []).slice(); next.splice(k, 1); this.setState({ outOfScope: next, demo: '' }); }
      })));

    /* the composer, live: what it would add, and whether it can */
    const typed = String(s.composerText || '').trim();
    const tooLong = typed.length > MAX_LINE_CHARS;
    const dupLine = !scope && L.some((l, i) => sameWords(this.lineText(i), typed));
    const dupOut = scope && recordedOut.concat(draftOut).some(function (t) { return sameWords(t, typed); });
    const composerCanAdd = !isDemo && !!act.changeId && !rec && !!typed && !tooLong && !dupLine && !dupOut;
    const composerWhyLive = !typed ? (scope ? 'Say one thing this Change is not about, then add it.' : 'Say one thing a check could observe, then add it.')
      : tooLong ? 'That is longer than ' + MAX_LINE_CHARS + ' characters. A paragraph is not one thing to check: split it, or shorten it.'
        : dupLine ? 'That line is already in the list.'
          : dupOut ? 'That is already out of scope.'
            : scope ? 'Recorded as out of scope with v' + ver + ', with who and when, when you accept it.'
              : 'It arrives below as a proposal. It counts only once you accept it, and it is recorded only with the version you accept.';
    const composerSubmit = () => {
      if (!composerCanAdd) return;
      if (scope) this.setState({ outOfScope: (this.state.outOfScope || []).concat([typed]), composer: '', composerText: '', demo: '' });
      else this.setState({ added: (this.state.added || []).concat([{ text: typed, state: 'proposed' }]), composer: '', composerText: '', demo: '' });
    };
    const untrackedWhy = 'Track this work as a Change first. Then anything you add arrives as a proposal.';
    const tplWhyLive = !act.changeId ? untrackedWhy : (TN.why || '') + (tech && TN.detail ? ' ' + TN.detail : '');

    /* the six stages: Define is this screen's; the rest are the Change's own */
    const stageOf = function (id) {
      const st = (D.stages || []).find(function (x) { return x.id === id; });
      return st ? (STAGE_GLYPH[st.status] || '—') : '—';
    };
    const stg = isDemo
      ? { g1: '✓', g2: '●', g3: '○', g4: '—', g5: '—' }
      : { g1: stageOf('prepare'), g2: stageOf('build'), g3: stageOf('verify'), g4: stageOf('review'), g5: stageOf('approve') };

    /* where the proposed lines came from, by source */
    const bySource = function (k) { return L.filter(function (l) { return l.source.kind === k; }).map(function (l) { return l.n; }); };
    const fromWords = bySource('words'), fromName = bySource('name'), fromCheck = L.filter(function (l) { return l.source.kind === 'check'; });
    const lineRef = function (ns, cap) { return (ns.length === 1 ? (cap ? 'Line ' : 'line ') : (cap ? 'Lines ' : 'lines ')) + andList(ns); };
    const originParts = [];
    if (fromWords.length) originParts.push(lineRef(fromWords, true) + (fromWords.length === 1 ? ' is' : ' are') + ' proposed from these words');
    if (fromName.length) originParts.push(lineRef(fromName, !originParts.length) + ' from the Change’s name');
    fromCheck.forEach(function (l) { originParts.push(lineRef([l.n], !originParts.length) + ' from the check this Change ran at steps ' + andList(l.source.steps)); });
    const proposedN = bySource('proposal');
    if (proposedN.length) originParts.push(lineRef(proposedN, true) + (proposedN.length === 1 ? ' is' : ' are') + ' proposed from these words');
    const originSentence = [originParts.length ? (originParts.length > 2
      ? originParts.slice(0, -1).join(', ') + ', and ' + originParts[originParts.length - 1]
      : originParts.join(' and ')) + '.' : '',
      D.recordingSince ? 'ThruLine began recording at ' + D.recordingSince + '; nothing before that was recorded.' : ''].filter(Boolean).join(' ');
    const nc = D.notCheckable;
    const ncShown = !nc || !nc.shownBy.length ? '' : nc.shownBy.length > 1 ? 'Lines ' + nc.shownBy[0] + '–' + nc.shownBy[nc.shownBy.length - 1] + ' are' : 'Line ' + nc.shownBy[0] + ' is';
    const claim = D.claim || {};
    const cw = claim.word;
    const req = D.request || {};

    return {
      isDemo: isDemo,

      /* top bar, stages, request, aside, status bar */
      projectName: D.project.name,
      changeTitle: D.change.title,
      version: V,
      recordingLabel: isDemo ? 'Recording · following live' : recordingPill({ change: { recording: D.recording ? true : null } }).text,
      versionNote: D.version.commit ? ['· = commit ' + D.version.commit].concat(D.version.clean ? ['no unsaved changes'] : []).join(' · ') : '',
      stepsLabel: D.steps != null ? fmtCount(D.steps) + ' steps' : '',
      /* the request is quoted on one line in the design; a long one is cut here and read whole in the story */
      requestText: req.text ? '“' + (isDemo || req.text.length <= 180 ? req.text : req.text.slice(0, 179).replace(/\s+\S*$/, '') + '…') + '”' : 'Not recorded',
      requestBy: [req.by, req.at].filter(Boolean).join(' · ') + ' · ',
      requestStepLink: isStep(req.step) ? 'step ' + req.step + ' ›' : '',
      requestStepHref: stepTo(req.step),
      requestWho: [req.by, req.at].filter(Boolean).join(' · '),
      requestAt: [req.at, req.step != null ? 'step ' + req.step : null].filter(Boolean).join(' · ') || 'Not recorded',
      lastEditAt: D.agent.lastEdit.at ? D.agent.lastEdit.at + (D.agent.lastEdit.step != null ? ' · step ' + D.agent.lastEdit.step : '') : 'No edit recorded',
      nowVersion: '· version ' + V,
      nowReceipt: stepRec,
      linkNowLabel: runStep ? 'Link step ' + runStep : run ? 'Link check run' : 'Link',
      addNote: !isDemo && !act.changeId ? untrackedWhy
        : nL ? 'Anything you add arrives as a proposal, like the ' + word(nL) + ' above.' : 'Anything you add arrives as a proposal.',
      hasNote: !!nc,
      hasClaim: isDemo || !!D.claim,
      notCheckableText: nc ? '“' + nc.quote + '” — no command can observe it. ' + (ncShown ? ncShown + ' what would show it. ' : '') : '',
      agentSaid: (D.agent.name || 'The agent') + ' said ',
      claimText: claim.text || '',
      claimAt: [claim.at, claim.step != null ? 'step ' + claim.step : null].filter(Boolean).join(' · '),
      claimCannot: cw ? 'The record can’t check “' + cw + '” by itself. What counts as done isn’t accepted yet.'
        : 'The record can’t check a claim like this by itself.' + (rec ? '' : ' What counts as done isn’t accepted yet.'),
      claimPoints: cw ? '“' + cw.charAt(0).toUpperCase() + cw.slice(1) + '” now points at ' : 'The claim now points at ',
      originSentence: originSentence,
      timingNote: 'A run is never linked by timing alone. ' + (runStep ? 'Step ' + runStep + ' ran in this session' : run ? 'A check ran for this Change' : 'A run in this session') + ', and that by itself says nothing about any line. A link needs a reason: you link it, a check you accept covers it, or what ran names what the line is about.',
      hollowNote: 'Each line starts hollow and counts only once you accept it.' + (nc ? ' “' + nc.quote + '” stays a note: no command can observe it.' : ''),
      nextVersion: 'Changing a line later starts v' + (baseVer + 1) + '. v' + baseVer + ' stays in the record.',
      storageLine: storageSentence(D.storage ? D.storage.bytes : null),
      composerWhy: isDemo ? 'Adding here isn’t built yet (needs engine). Until then, edit one of the proposed lines.' : !act.changeId ? untrackedWhy : composerWhyLive,
      composerCanAdd: composerCanAdd,
      composerSubmit: composerSubmit,
      tplWhy: isDemo ? 'A template’s lines arrive as proposals, never accepted for you.' : tplWhyLive,
      tplRail: isDemo ? 'Your repository’s templates for what counts as done live in Settings › Rules. Picked here, they arrive as proposals.'
        : D.templates.length ? 'This project’s templates for what counts as done, from .thruline/policy.yml. Picked here, they arrive as proposals.' : tplWhyLive,

      alts: alts,
      toggleFace: () => switchFace(this.props, tech ? 'plain' : 'technical', (f) => this.setState({ face: f })),
      faceLabel: !tech ? 'Face: Plain. Switch to Technical' : 'Face: Technical. Switch to Plain',
      plainColor: !tech ? T1 : T3,
      techColor: !tech ? T3 : T1,

      defineGlyph: rec ? '✓' : '○',
      defineSub: defineSub,
      stg: stg,

      titleMeta: titleMeta,
      headline: headline,
      headSub: headSub,
      counter: counter,
      originColor: acc > 0 ? OR : LINE,
      originGlow: acc > 0 ? GLOW : 'none',
      trunkBg: acc > 0 ? OR : LINE,
      trunkGlow: acc > 0 ? GLOW : 'none',

      cards: cards,

      canChange: !rec,
      // live: adding needs a Change to add to; untracked work is tracked first
      addOff: isDemo || act.changeId ? undefined : true,
      addExpanded: s.composer === 'line' ? 'true' : 'false',
      scopeExpanded: scope ? 'true' : 'false',
      openAdd: () => this.setState({ composer: this.state.composer === 'line' ? '' : 'line', composerText: '', tplOpen: false }),
      openScope: () => this.setState({ composer: this.state.composer === 'scope' ? '' : 'scope', composerText: '', tplOpen: false }),
      closeComposer: () => this.setState({ composer: '', composerText: '' }),
      composerOpen: s.composer !== '' && !rec,
      composerText: s.composerText,
      onComposer: (e) => this.setState({ composerText: e && e.target ? String(e.target.value) : '' }),
      composerLabel: scope ? 'Out of scope · something this Change is not about' : 'New line · say one thing a check could observe',
      composerPlaceholder: scope ? 'For example: refunds' : 'One thing a check could observe',
      composerAdd: scope ? 'Add to out of scope' : 'Add as proposed',
      tplOpen: s.tplOpen && !rec,
      tplExpanded: s.tplOpen ? 'true' : 'false',
      tplChevron: s.tplOpen ? 'M3.5 8.5 7 5l3.5 3.5' : 'M3.5 5.5 7 9l3.5-3.5',
      toggleTpl: () => this.setState({ tplOpen: !this.state.tplOpen, composer: '' }),
      templates: templates,

      folders: folders,
      ruleSentence: ruleSentence,
      coversNote: isDemo ? 'Suggested from the recorded edits, with their steps.'
        : keepsFolders ? 'v' + baseVer + ' covers these folders, and v' + ver + ' keeps every one of them.'
          : rec ? 'What v' + ver + ' covers, with the steps that edited each.' : 'Suggested from the recorded edits, with their steps.',
      outItems: outItems,
      outEmpty: isDemo ? 'Nothing yet. For example: refunds, currency rounding.' : outItems.length ? '' : 'Nothing yet.',
      track: { offered: trackOffered, busy: TR.busy ? 'true' : undefined, label: TR.busy ? 'Tracking this work…' : 'Track this work as a Change', act: () => { if (!TR.busy && A.track) A.track(); } },

      showAcceptBtn: !rec && acc > 0,
      showAcceptDisabled: !rec && acc === 0 && !trackOffered && !(TR.loading && !act.changeId),
      acceptLabel: 'Accept what counts as done (v' + ver + ')',
      recordV: isDemo ? () => this.setState({ recorded: true, editing: -1, composer: '', composerText: '', tplOpen: false, demo: '' }) : () => {
        if (!A.accept || ans.busy) return;
        /* a proposal goes by its number (and the person's wording, if they
           changed it); every other accepted line — one they added, or one of
           the standing version they keep — goes by its words */
        const accepted = [], own = [], edits = {};
        L.forEach((l, i) => {
          if (lines[i] !== 'accepted') return;
          if (l.proposal != null) { accepted.push(l.proposal); if (s.texts[i]) edits[l.proposal] = s.texts[i]; }
          else own.push(this.lineText(i));
        });
        const draft = { accepted: accepted, edits: edits, folders: onPaths, replacing: ver !== baseVer };
        if (own.length) draft.lines = own;
        if (draftOut.length) draft.exclusions = draftOut.slice();
        const answer = A.accept(draft);
        if (answer && typeof answer.then === 'function') {
          answer.then((r) => {
            if (r && r.ok) this.setState({ reopen: false, added: [], outOfScope: [], texts: [], editing: -1, composer: '', composerText: '', tplOpen: false });
          }, function () {});
        }
      },
      recorded: rec,
      notRecorded: !rec,
      barLine1: barLine1,
      /* live: the bar is one line; its whole sentence is the line's title */
      barTitle: isDemo ? undefined : barLine1,
      barLine2: barLine2,
      barChip: ver === baseVer ? 'Bar set after the work: accepted ' + whenFirst + '; the agent’s last edit was ' + lastEditAt + '.' : 'Bar set after the work: the agent’s last edit was ' + lastEditAt + '.',
      showBarChip: isDemo || (!!D.agent.lastEdit.at && !!D.criteria.acceptedAt && acceptedAfterWork(D.criteria.acceptedTs, D.agent.lastEdit.ts, D.criteria.acceptedAt, D.agent.lastEdit.at)),
      changeItLabel: 'Change it · starts v' + (ver + 1),
      changeIt: isDemo ? () => this.setState({ recorded: false, ver: this.state.ver + 1, demo: '' }) : () => this.setState({ reopen: true, lines: dataStates, added: [], outOfScope: [], texts: [] }),

      whyPoints: whyPoints,
      barAfterText: barAfterText,
      tlYouBg: rec ? T1 : 'transparent',
      tlYouBorder: rec ? '0' : '2px solid #FF7A29',
      tlYouGlow: rec ? 'none' : GLOW,
      tlYouWhen: rec ? when : 'now',

      listEyebrow: rec ? 'What counts as done v' + ver + ' · where each line stands on ' + V : 'Proposed, checkable statements · accept, edit or drop each',
      readAt: 'read ' + (ver === baseVer ? (rec ? whenFirst : D.readAt || '') : 'just now') + ' ↻',

      statusLeft: statusLeft,
      demos: demos
    };
  }
}
