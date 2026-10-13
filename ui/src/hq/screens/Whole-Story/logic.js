/* Whole-Story.dc.html: the artboard's logic, from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from
 * `this.props.data` (live) or DEMO (the design gallery) — see DATA.md;
 * `this.state` holds UI state only. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { recordingPill, storageSentence } from '../../data/project.js'
import { fmtCount, countOf } from '../../data/count.js'
import { homeless, isExclusionMarker } from '../../data/paths.js'
import { inProject } from './from-change.js'
import { storyFile, classCounts, CLASSES, sha256Hex, EXPORT_FORMAT, EXPORT_SCHEMA_VERSION } from './export.js'
import { IO } from './export-io.js'
import { href as routeHref } from '../../routes.js'
import { WholeStoryNotes } from './notes.jsx'
import { WholeStoryObjections } from './objection.jsx'
import { WholeStorySeconds } from './seconds.jsx'
import React from 'react'
import { isStep, stepLinker } from '../Step-View/link.js'
import { faceOf as storedFaceOf, switchFace, storedOf, isLive } from '../../data/face.js'

export const PROPS = {"$preview":{"width":1512,"height":2800}}

export default class Component extends DCLogic {
  state = { fmt: 'md', paths: false, words: false, saved: false, copied: false, failed: null, face: 'plain' };

  renderVals() {
    const s = this.state;
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    if (D.live) return liveVals(this, D);
    const V = D.version.id;
    const pl = (n, one, many) => (n === 1 ? one : many);
    const step = (n) => 'step ' + n;
    const listAnd = (xs) => (xs.length <= 1 ? xs.join('') : xs.slice(0, -1).join(', ') + ' and ' + xs[xs.length - 1]);
    const steps = (ns) => ns.length === 1 ? step(ns[0]) : 'steps ' + ns.join(', ');
    const result = (r) => (r.passed === null || r.passed === undefined ? 'no result yet'
      : r.failed > 0 ? r.failed + ' failed, ' + r.passed + ' passed' : r.passed + ' passed, ' + r.failed + ' failed');
    const stamp = D.story.date + ' ' + D.approval.at + ' ' + D.story.utcOffset;
    const runByStep = Object.fromEntries(D.runs.map((r) => [r.step, r]));
    const gb = (bytes) => (bytes / 1e9).toFixed(1) + ' GB';
    const hash = (h) => (/^[0-9a-f]{64}$/.test(h) ? h.slice(0, 8) + '…' + h.slice(-4) : h);
    const nCrit = D.criteria.length;
    const nProven = D.criteria.filter((c) => c.proof.state === 'proven').length;
    const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', OR = '#FF7A29', CTRL = '#66666F', LINE = '#2A2A30', HAIR = '#1F1F23';
    const VIOLET = '#B78CFF', GREEN = 'var(--tl-proven, #3DD68C)', YELLOW = '#F2C94C';
    const redacted = !!(s.paths || s.words);

    // ---------- the story in one line (bands = recorded versions; only "now" carries the lit thread) ----------
    const PANEL = '#0A0A0B', NOWBG = '#140E0B';
    const item = function (glyph, gc, gs, title, tc, sub) { return { glyph: glyph, gc: gc, gs: gs, title: title, tc: tc, sub: sub }; };
    const band = function (label, labelColor, bg, border, line, lit, items) {
      return { label: label, labelColor: labelColor, bg: bg, border: border, line: line, lit: lit,
        items: items.map(function (i) { return Object.assign({}, i, { bg: bg, aria: label + ': ' + i.title + ', ' + i.sub }); }) };
    };
    const evItem = (e) => {
      switch (e.kind) {
        case 'gap': return item('◌', T3, '15px', 'not recorded', T2, 'before ' + e.before);
        case 'prompt': return item('•', T2, '20px', (e.by === 'you' ? 'you' : e.by) + ' asked', T1, step(e.step));
        case 'statement': return item('❝', VIOLET, '15px', 'the statement', T1, step(e.step));
        case 'edit': return item('•', T2, '20px', 'edit', T1, step(e.step));
        case 'accept': return item('•', e.restsOn ? OR : T2, '20px', (e.by === 'you' ? 'you' : e.by) + ' accepted ' + e.criteriaVersion, T1, e.at);
        case 'approve': return item('◆', T1, '14px', 'approved', T1, e.at);
        default: {
          if (e.passed === null || e.passed === undefined) return item('⋯', T2, '15px', 'no result yet', T1, step(e.step));
          if (e.failed > 0) return item('✕', T2, '13px', result(e), T1, step(e.step));
          return item('•', e.restsOn ? OR : T1, '20px', result(e), T1, step(e.step));
        }
      }
    };
    const bands = D.timeline.map((bd) => {
      if (bd.kind === 'unrecorded') return band('earlier', T3, PANEL, 'none', '1px dashed ' + T3, false, bd.events.map(evItem));
      const now = bd.kind === 'version' && !!bd.current;
      const label = bd.kind === 'session' ? 'session' : bd.version + (now ? ' · now' : '');
      return band(label, now ? T1 : T3, now ? NOWBG : PANEL, '1px solid ' + HAIR, '1px solid ' + LINE, now, bd.events.map(evItem));
    });

    // ---------- what counts as done (all four rest on step 62) ----------
    const WHY = {
      person: 'You linked this yourself.',
      run_names_subject: 'Looked at because what ran names what this line is about.',
      accepted_checks: 'Looked at because the checks you accepted cover this kind of thing.'
    };
    const acc = D.criteriaAccepted;
    const crit = D.criteria.map(function (c, i) {
      const pf = c.proof;
      const run = runByStep[pf.restsOn.step] || {};
      const scope = pf.scope.join(', ');
      return {
        n: String(c.n), gate: !!c.gate, text: c.text, why: WHY[c.linkBasis] || '',
        border: i === 0 ? 'none' : '1px solid ' + HAIR,
        chainLabel: 'Proof chain for line ' + c.n + ': every link holds, so Proven · version ' + pf.ranOn + ' · ' + scope,
        acceptedAria: 'Accepted by ' + acc.by + ' at ' + acc.at + (acc.afterWork ? ', after the work' : ''),
        accepted: acc.at + ' · ' + acc.by,
        restsAria: 'Rests on the run at ' + step(pf.restsOn.step) + ': ' + run.command + ', ' + result(run) + ', on ' + run.version,
        restsStep: step(pf.restsOn.step),
        ranAria: 'Ran on ' + pf.ranOn + (pf.ranOn === V ? ', the version being approved' : ''),
        ranOn: pf.ranOn,
        sinceAria: pf.changedSince ? 'Since: something it depends on changed after it' : 'Since: nothing it depends on changed after it',
        since: pf.changedSince ? 'changed' : 'unchanged',
        recheckAria: pf.ranOn === V ? 'Re-checked: not needed, it ran on ' + pf.ranOn : 'Re-checked: needed, it ran on ' + pf.ranOn,
        recheck: pf.ranOn === V ? 'not needed' : 'needed',
        ...lineVerdict(pf.state, pf.ranOn, pf.scope)
      };
    });
    const restSteps = [...new Set(D.criteria.map((c) => c.proof.restsOn.step))];
    const restStep = restSteps.length === 1 ? restSteps[0] : null;

    // ---------- runs (chronological; counts failures first) ----------
    const runs = D.runs.map(function (r, i) {
      const none = r.passed === null || r.passed === undefined;
      const cur = r.version === V && D.version.current;
      return {
        time: r.at || '—', step: step(r.step), cmd: r.command, destructive: !!r.destructive,
        glyph: none ? '⋯' : (r.failed > 0 ? '✕' : ''), gc: T2,
        result: result(r), rc: none ? T2 : T1,
        ver: r.version ? r.version + (cur ? ' · now' : '') : '—',
        vc: cur ? T1 : T3, vbg: cur ? 'rgba(255,122,41,.10)' : 'transparent',
        border: i === 0 ? 'none' : '1px solid ' + HAIR
      };
    });
    const captured = D.runs.filter((r) => r.capturedBy === 'thruline').map((r) => r.step).sort((x, y) => x - y);
    const asked = D.runs.filter((r) => r.permissionAskedAt && (r.passed === null || r.passed === undefined));
    const ranCount = D.runs.filter((r) => !(r.passed === null || r.passed === undefined)).length;

    // ---------- words (only the state words this page uses) ----------
    const words = [
      { glyph: '●', gc: GREEN, word: 'Proven', def: 'A run on this version shows it.' },
      { glyph: '○', gc: T2, word: 'Not proven', def: 'Nothing shows it on this version.' },
      { glyph: '◭', gc: YELLOW, word: 'Needs a re-check', def: 'It was shown on an earlier version.' },
      { glyph: '▪', gc: T2, word: 'Recorded', def: 'In the record; it settles nothing.' },
      { glyph: '⋯', gc: T2, word: 'No result yet', def: 'A step with no result recorded.' },
      { glyph: '◌', gc: T3, word: 'Not recorded', def: 'Nothing was captured here.' },
      { glyph: '◇', gc: T1, word: 'Ready for approval', def: 'Every line holds; not yet decided.' },
      { glyph: '◆', gc: T1, word: 'Approved', def: 'Approved by name, for one version.' }
    ];

    // ---------- design notes (rail only; never part of the exported file) ----------
    const designNotes = [
      { tag: 'ILLUSTRATIVE', where: 'Demo record', text: 'step 62 is the next number the record would assign. Accepting at 16:06 (the prototype says 16:10) keeps accept → re-check → approve in order. The redaction counts are illustrative.' },
      { tag: 'NEEDS ENGINE', where: 'What counts as done', text: 'pytest counts per run, and per-test links. Until they land, each line stops at ② and reads ▪ Recorded, and lines on one run share its outcome.' },
      { tag: 'ENGINE', where: 'Runs and ❝Tests pass.❞', text: '“captured by ThruLine” needs run capture (thruline shim install, then its PATH line), off by default; Proven on the statement needs “matched by ThruLine”. Without them the criteria carry the proof.' },
      { tag: 'NEEDS ENGINE', where: 'Step 60, lines per edit, integrity', text: 'the permission wait, a diff route, and the chain check on this screen (IR #4, #19). Until then: “was requested at 16:04”, “lines not recorded”, and thruline verify.' }
    ];

    // ---------- export: format ----------
    const fmtDefs = [
      { key: 'md', name: 'Markdown', ext: '.md' },
      { key: 'html', name: 'HTML', ext: '.html' },
      { key: 'json', name: 'JSON', ext: '.json' }
    ];
    const fmts = fmtDefs.map((f) => {
      const on = s.fmt === f.key;
      return {
        name: f.name, ext: f.ext, checked: on ? 'true' : 'false',
        bg: on ? 'rgba(255,122,41,.10)' : 'transparent',
        border: on ? OR : CTRL, color: on ? T1 : T2,
        ring: on ? OR : CTRL, dot: on ? OR : 'transparent',
        pick: () => this.setState({ fmt: f.key, saved: false, copied: false })
      };
    });
    const notes = {
      md: 'Plain text (Markdown). The same text the Copy as Markdown button copies.',
      html: 'A page anyone can open in a browser. One file. It opens without the internet, and it runs nothing.',
      json: 'For other tools (JSON). The same record, as named fields a program can read.'
    };
    const engine = {
      md: null,
      html: { tag: 'NEEDS ENGINE', text: 'An HTML copy that re-checks its own fingerprints in the browser, and a light theme for printing.' },
      json: { tag: 'ENGINE', text: 'The verification-summary shape of this file is command-line only today.' }
    };
    const eng = engine[s.fmt];

    // ---------- export: redacted copy ----------
    const classDefs = [
      { key: 'paths', label: 'File and folder paths', count: String(D.redaction.paths), enabled: D.redaction.paths > 0 },
      { key: 'words', label: 'The agent’s own words', count: String(D.redaction.agentWords), enabled: D.redaction.agentWords > 0 },
      { key: 'output', label: 'What a check printed when it broke', count: String(D.redaction.failureOutput), enabled: D.redaction.failureOutput > 0 }
    ];
    const classes = classDefs.map((k) => {
      const on = !!s[k.key];
      const patch = {};
      patch[k.key] = !on;
      patch.saved = false;
      patch.copied = false;
      return {
        label: k.label, count: k.count, enabled: k.enabled, disabled: !k.enabled,
        checked: on ? 'true' : 'false', mark: on ? '✓' : '',
        boxBg: on ? OR : 'transparent', boxBorder: on ? OR : CTRL,
        toggle: () => this.setState(patch)
      };
    });
    const toggleCut = () => {
      if (redacted) this.setState({ paths: false, words: false, saved: false, copied: false });
      else this.setState({ paths: true, saved: false, copied: false });
    };
    const leftOut = [];
    if (s.paths) leftOut.push('File and folder paths: ' + D.redaction.paths + ' left out');
    if (s.words) leftOut.push('The agent’s own words: ' + D.redaction.agentWords + ' left out');

    // ---------- export: file name (thruline-packet-<change-slug>-<version7>[-redacted].<ext>) ----------
    const ext = s.fmt;
    const fileName = 'thruline-packet-' + D.change.id + '-' + V + (redacted ? '-redacted' : '') + '.' + ext;

    // ---------- export: word-for-word preview ----------
    const PM = '[left out: a file or folder path]';
    const c1 = D.criteria[0];
    const scopeTxt = s.paths ? c1.proof.scope.map(() => PM).join(', ') : c1.proof.scope.join(', ');
    const said = D.statements[0];
    const pathsLimit = 'Left out: the paths the record holds as paths, and the project’s own folder wherever it appears. Any other path written inside a sentence or a command stays.';
    const quoteTxt = s.words ? '[left out: the agent’s own words]' : '“' + said.quote + '”';
    const T_ = D.change.title;
    const approvedLine = '◆ Approved on ' + D.approval.version + ' by ' + D.approval.by + ' on ' + stamp + ', against what counts as done, version ' + D.approval.criteriaVersion + '.';
    const restsLine = 'It rests on ' + nProven + ' of ' + nCrit + ' things that count as done, each with a run on ' + V + '.';
    const c1Line = c1.text + ' ● Proven · version ' + c1.proof.ranOn + ' · ' + scopeTxt;
    const saidLine = said.agent + ' said ' + quoteTxt + ' · ' + step(said.step) + ' · ' + said.at;
    const lead = 'This copy leaves things out. Each one is marked where it was. The full copy is unchanged.';
    let lines = [];
    if (s.fmt === 'md') {
      lines = (redacted ? [lead].concat(leftOut.map(function (x) { return '- ' + x; }), s.paths ? [pathsLimit] : [], ['']) : []).concat([
        '# ' + T_,
        '',
        approvedLine,
        restsLine,
        '',
        '## What counts as done, and what proves it',
        c1.n + '. ' + c1Line + ' · ' + step(c1.proof.restsOn.step),
        '',
        '## What the agent said, and what the record shows',
        saidLine,
        '…'
      ]);
    } else if (s.fmt === 'html') {
      lines = ['<!doctype html>', '<title>' + T_ + '</title>']
        .concat(redacted ? ['<section class="label">', '<p><strong>' + lead + '</strong></p>', '<ul>'].concat(leftOut.map(function (x) { return '<li>' + x + '</li>'; }), ['</ul>'], s.paths ? ['<p class="muted">' + pathsLimit + '</p>'] : [], ['</section>']) : [])
        .concat([
          '<h1>' + T_ + '</h1>',
          '<p>' + approvedLine + '</p>',
          '<h2>What counts as done, and what proves it</h2>',
          '<li>' + c1Line + '</li>',
          '<h2>What the agent said, and what the record shows</h2>',
          '<p>' + saidLine + '</p>',
          '…'
        ]);
    } else {
      lines = [
        '{',
        '  "format": "thruline-packet",',
        '  "schema_version": 1,',
        '  "title": ' + JSON.stringify(T_) + ',',
        '  "version": { "short": ' + JSON.stringify(V) + ', … },',
        '  "accepted_count": ' + nCrit + ',',
        '  "readiness": { "ready": true, … },',
        '  …',
        '  "redaction": {',
        '    "redacted": ' + (redacted ? 'true' : 'false') + ',',
        '    "removed": [' + (redacted ? ' … ' : '') + '],',
        '    "derived_from_sha256": ' + (redacted ? '"…"' : 'null'),
        '  }',
        '}'
      ];
    }
    const preview = lines.map(function (t) {
      const marked = t.indexOf('[left out') !== -1 || t.indexOf('leaves things out') !== -1 || t.indexOf('left out') !== -1 || (redacted && t.indexOf('"redacted"') !== -1);
      return { text: t === '' ? ' ' : t, color: marked ? T1 : T2 };
    });

    // ---------- export: actions ----------
    let statusText = 'Made in this browser from exactly the text above. Nothing is sent anywhere.';
    let statusColor = T2;
    if (s.saved) {
      statusText = 'Your browser has the file now, and saves it wherever it keeps your downloads on this Mac. Nothing was sent anywhere.';
      statusColor = T1;
    } else if (s.copied) {
      statusText = 'Copied: the same text the Markdown file holds. Nothing is written to a file, and nothing leaves this Mac.';
      statusColor = T1;
    }

    // ---------- readable now ----------
    const readable = D.readable.records.map(function (r, i) {
      let what = '', detail = '';
      if (r.kind === 'decision_file') { what = 'The approval, as a file'; detail = r.path; }
      else if (r.kind === 'run') { what = 'Run · ' + step(r.step); detail = r.capturedBy === 'thruline' ? 'captured by ThruLine' : 'reported by the agent'; }
      else { what = 'The recorded conversation'; detail = r.agent + ' transcript · ' + r.files + ' ' + pl(r.files, 'file', 'files'); }
      return { what: what, detail: detail, border: i === 0 ? 'none' : '1px solid ' + HAIR };
    });

    // ---------- the page's sentences, from the record ----------
    const saidSteps = steps([...new Set(D.statements.map((x) => x.step))].sort((x, y) => x - y));
    const agents = [...new Set(D.sessions.map((x) => x.agent))];
    const allF = D.findings.groups.flatMap((g) => g.findings);
    const nFindings = allF.length;
    const settledN = D.findings.groups.filter((g) => g.settlement.kind === 'settled').reduce((a, g) => a + g.findings.length, 0);
    const nothingN = D.findings.groups.filter((g) => g.settlement.kind === 'nothing').reduce((a, g) => a + g.findings.length, 0);
    const openN = D.findings.groups.filter((g) => g.settlement.kind === 'open').reduce((a, g) => a + g.findings.length, 0);
    const st0 = D.statements.find((x) => x.kind === 'checkable') || D.statements[0];
    const chk0 = (st0.checks || [])[0] || {};
    const chk1 = (st0.checks || [])[1] || {};
    const r1 = runByStep[(chk1.restsOn || {}).step] || {};
    const done = D.statements.find((x) => x.kind === 'done_claim') || {};
    const unchecked = D.statements.find((x) => x.kind === 'unchecked') || {};
    const CIRCLED = ['', '①', '②', '③', '④', '⑤', '⑥'];
    const STATE = { needs_recheck: ['◭', ' Needs a re-check · version '], ready: ['◇', ' Ready for approval · '] };
    const dec = D.decisions.map((d) => {
      if (d.kind === 'accept') return { at: d.at, text: 'You accepted what counts as done, ' + d.criteriaVersion + ': ' + d.statements + ' ' + pl(d.statements, 'statement', 'statements') + (d.afterWork ? ', after the work.' : '.'), state: STATE[d.stateAfter.state][1] + d.stateAfter.version };
      if (d.kind === 'run') return { at: d.at, text: 'Step ' + d.step + ' ran on ' + d.version + (d.scopeAdded ? ', ' + d.scopeAdded + ' in scope.' : '.'), state: STATE[d.stateAfter.state][1] + d.stateAfter.version };
      return { at: d.at, text: ' Approved on ' + d.version + ' by ' + d.by + ', against ' + d.criteriaVersion + '. It names this version only.', file: d.file };
    });
    const gapText = (g) => {
      if (g.kind === 'before_capture') return ['Before ' + g.at + '.', ' The hook was installed at ' + g.at + '; nothing earlier is in the record.'];
      if (g.kind === 'no_result') return ['Step ' + g.step + '’s result.', ' It asked for permission at ' + g.askedAt + '; no result is recorded.'];
      if (g.kind === 'code_at_statement') return ['The code at the moment the agent spoke.', ' The last version recorded before step ' + g.step + ' is ' + g.lastVersion + '.'];
      return ['How many lines each edit changed.', ' Lines not recorded; the edits and their files are.'];
    };
    const gaps = D.gaps.map(gapText);
    const fg = D.findings.groups;
    const page = {
      project: D.project.name,
      root: D.project.root,
      title: D.change.title,
      version: V,
      versionMeta: (D.version.current ? '(current) · ' : '') + '= commit ' + D.version.commit + ' · ' + (D.version.unsavedChanges ? 'unsaved changes' : 'no unsaved changes'),
      recording: D.recording.following ? 'Recording · following live' : 'Not recording',
      readAt: 'read ' + D.readAt + ' ↻',
      describes: 'describes ' + V + ' as of ' + D.story.date + ' ' + D.story.asOf + ' ' + D.story.utcOffset,
      sessions: D.sessions.length + ' ' + pl(D.sessions.length, 'session', 'sessions') + ' · ' + agents.join(', ') + ' · ' + D.recordedSteps + ' recorded steps',
      approvedOn: D.approval.version,
      approvedBy: 'by ' + D.approval.by + ' on ',
      approvedAt: stamp,
      approvedAgainst: ', against what counts as done, version ' + D.approval.criteriaVersion,
      restsOn: restsLine,
      barSet: 'Bar set after the work: accepted ' + acc.at + '; the agent’s last edit was ' + acc.lastAgentEditAt + '.',
      howItWent: 'How it went · ' + D.story.date + ' · ' + D.story.from + ' → ' + D.story.to + ' · UTC' + D.story.utcOffset,
      doneMeta: acc.version + ' · accepted ' + acc.at + ' by ' + acc.by + (acc.afterWork ? ', after the work' : ''),
      doneNote: restStep !== null
        ? ' Until ThruLine can link each test to a line, lines linked to one run share its outcome: all ' + nCrit + ' rest on ' + step(restStep) + '.'
        : ' Until ThruLine can link each test to a line, lines linked to one run share its outcome.',
      saidMeta: agents.join(', ') + ' · ' + saidSteps,
      s1: {
        said: st0.agent + ' said ',
        quote: st0.quote,
        where: step(st0.step) + ' · ' + st0.at,
        at0: chk0.at,
        read0: ' Not proven · version ' + chk0.version,
        why0: ': it rested on ' + step((chk0.restedOn || {}).step) + ' (on ' + (chk0.restedOn || {}).version + '), and ' + (chk0.changed || {}).file + ' changed at ' + step((chk0.changed || {}).step) + ', so the proof stopped at ' + CIRCLED[chk0.stoppedAt || 0] + '.',
        at1: chk1.at,
        why1: 'It rests on ' + step((chk1.restsOn || {}).step) + ((chk1.restsOn || {}).matchedBy === 'thruline' ? ', matched by ThruLine' : '') + ': the same command as ' + step((chk1.restsOn || {}).sameCommandAs) + ', with ' + (chk1.restsOn || {}).scopeAdded + ' in scope, on ' + chk1.version + '. Every link holds.',
        proven: 'Proven: ' + st0.claim + ' ',
        provenMeta: '· version ' + chk1.version + ' · ' + (r1.scope || []).join(', '),
        run: r1.command + ' · ' + result(r1),
        runVersion: r1.version + (r1.version === st0.checks[1].version ? ' · same version' : ''),
        runStep: step(r1.step) + ' ›',
        note: 'Approval rests on what counts as done; lines 1–' + nCrit + ' rest on this same run. The last version ThruLine recorded before this was said is ' + st0.versionBefore + '; ThruLine doesn’t record the code at the moment it spoke.'
      },
      s2: {
        said: done.agent + ' said ',
        quote: done.quote,
        where: step(done.step) + ' · ' + done.at,
        why: 'The record can’t check “' + done.word + '” by itself. It is decided by ',
        tail: ': ' + nProven + ' of ' + nCrit + ' lines rest on ' + (restStep !== null ? step(restStep) : 'their runs') + ', on ' + V + '.'
      },
      s3: { quote: unchecked.quote, where: step(unchecked.step) },
      runsRoot: D.project.root,
      runsNote: ', and a passing run speaks only for the version it ran on. ' + (captured.length ? (captured.length === 1 ? 'Step ' : 'Steps ') + listAnd(captured.map(String)) + ' ' + pl(captured.length, 'was', 'were') + ' captured by ThruLine.' : '') + asked.map((r) => ' Step ' + r.step + ' asked for permission at ' + r.permissionAskedAt + '; ThruLine can’t answer that for you.').join(''),
      findingsMeta: nFindings + ' found at ' + D.findings.foundAt + ' · ' + settledN + ' settled · ' + nothingN + ' nothing to settle · ' + openN + ' open',
      f: allF.map((x) => ({ n: String(x.n), text: x.text + ' ', where: x.destroys ? x.where + ' · ' : x.where, destroys: x.destroys ? ' ' + x.destroys : '' })),
      settle: fg.map((g) => g.settlement.kind === 'settled'
        ? { head: 'Settled at ' + g.settlement.at + ':', text: ' ' + g.settlement.text + ' ', ref: g.settlement.ref }
        : { text: ' ' + g.settlement.text }),
      dec: dec,
      gapsMeta: D.gaps.length + ' ' + pl(D.gaps.length, 'gap', 'gaps'),
      gaps: gaps.map((g) => ({ head: g[0], text: g[1] })),
      integrityMeta: 'checked on this Mac · ' + D.integrity.checkedAt,
      integritySteps: D.integrity.steps + ' steps · each hash covers the one before',
      chain: 'chain sha256:' + hash(D.integrity.chainHead),
      footTimes: 'All times on this page are on ' + D.story.date + ', UTC' + D.story.utcOffset + ' · masked before export: detector ' + D.masking.detector + ' · ' + D.masking.masked + ' masked · best-effort',
      maskNote: (D.redaction.failureOutput === 0 ? 'The runs these lines rest on all passed, so there is no failure output to leave out. ' : '') + 'Secrets were masked before this page was shown: detector ' + D.masking.detector + ' · ' + D.masking.masked + ' masked · best-effort.',
      readableAt: 'looked at ' + D.readable.checkedAt,
      readableSum: D.readable.records.every((r) => r.readable)
        ? 'Every one of the ' + D.readable.records.length + ' records this rests on could be read.'
        : D.readable.records.filter((r) => r.readable).length + ' of the ' + D.readable.records.length + ' records this rests on could be read.',
      storage: 'ThruLine is keeping ' + gb(D.storage.bytes) + ' of records on this machine · '
    };

    // ---------- contents ----------
    const toc = [
      { href: '#ws-outcome', label: 'The outcome', meta: D.approval.version },
      { href: '#ws-done', label: 'What counts as done', meta: nProven + ' of ' + nCrit },
      { href: '#ws-said', label: 'What the agent said', meta: saidSteps },
      { href: '#ws-runs', label: 'Runs', meta: ranCount + ' ran · ' + (D.runs.length - ranCount) + ' no result' },
      { href: '#ws-findings', label: 'Findings', meta: String(nFindings) },
      { href: '#ws-decisions', label: 'Decision history', meta: String(D.decisions.length) },
      { href: '#ws-not-recorded', label: 'What was not recorded', meta: String(D.gaps.length) },
      { href: '#ws-integrity', label: 'Record integrity', meta: D.integrity.steps + ' steps' },
      { href: '#ws-words', label: 'Words', meta: String(words.length) }
    ].map(function (t, i) {
      const on = i === 0;
      return Object.assign(t, {
        color: on ? T1 : T2, current: on ? 'location' : 'false',
        bar: on ? '2px solid ' + OR : '2px solid transparent',
        glow: on ? '-4px 0 8px -4px rgba(255,154,82,.7)' : 'none'
      });
    });

    return {
      isDemo: isDemo,
      page: page,
      bands: bands,
      crit: crit,
      runs: runs,
      words: words,
      fmts: fmts,
      fmtNote: notes[s.fmt],
      fmtEngineShow: isDemo && !!eng,
      fmtEngineTag: eng ? eng.tag : '',
      fmtEngine: eng ? eng.text : '',
      classes: classes,
      cutOn: redacted ? 'true' : 'false',
      cutSub: redacted ? 'On · a second file, leaving out what you tick' : 'Off · the file holds everything on this page',
      toggleCut: toggleCut,
      swBg: redacted ? OR : LINE,
      swBorder: redacted ? OR : CTRL,
      swKnob: redacted ? '18px' : '2px',
      swKnobBg: redacted ? '#000000' : T2,
      fileName: fileName,
      previewMeta: 'first lines · ' + (redacted ? 'redacted copy' : 'full copy'),
      preview: preview,
      downloadLabel: 'Download .' + ext,
      download: () => this.setState({ saved: true, copied: false }),
      copyMd: () => this.setState({ copied: true, saved: false }),
      statusText: statusText,
      statusColor: statusColor,
      readable: readable,
      toc: toc,
      designNotes: designNotes
    };
  }
}

/* ---------- the live path: one Change's story, from the snapshot ----------
 * `D` is `wholeStoryFrom(snapshot, tracked)` (from-change.js). Every sentence
 * here is made from its typed fields; a fact it does not hold is said in the
 * design's words for absence ("not recorded", "not accepted yet"), never
 * drawn as a value. Times on the page are this machine's local HH:MM; the
 * exported file says the same, and gives its moments in UTC, with Z, in its
 * technical part. */
export const LIVE_TEXT = Object.freeze({
  notAccepted: 'What counts as done is not accepted yet',
  integrityOff: 'Not checked on this page',
  noDecision: 'No decision has been recorded for this work.',
  noFinding: 'Nothing was found that needs settling.',
});

/** A saved export is never verified by this read. The server checks only the
 * links in sessions a person confirmed for this Change. */
export function integrityOf(read) {
  const off = (head, meta, detail) => ({
    toc: 'not checked',
    page: { integrityHead: head, integrityMeta: meta, integritySteps: detail,
      chain: 'This page has not checked the recorded chain. It cannot verify a saved copy.' },
  });
  if (!read || read.status === 'reading') {
    return off('Checking the recorded chain…', 'checking now', 'The result is not known yet.');
  }
  if (read.status === 'untracked') {
    return off('Not checked on this page', 'no tracked Change', 'No confirmed Change sessions were given to the chain check.');
  }
  if (read.status !== 'ready' || !read.doc || read.doc.schema_version !== 1) {
    return off('Could not check the recorded chain', 'check unavailable', 'The result is unknown.');
  }
  const d = read.doc;
  const links = Number.isSafeInteger(d.links_checked) && d.links_checked >= 0 ? d.links_checked : null;
  const sessions = Number.isSafeInteger(d.sessions_checked) && d.sessions_checked >= 0 ? d.sessions_checked : null;
  if (links == null || sessions == null || typeof d.checked_at !== 'string') {
    return off('Could not check the recorded chain', 'invalid answer', 'The result is unknown.');
  }
  const counted = countOf(links, 'recorded link', 'recorded links') + ' in ' + countOf(sessions, 'confirmed session', 'confirmed sessions');
  const scope = 'The check re-folded recorded event hashes and session heads. It did not re-read the agent’s source files or verify this saved export.';
  if (d.state === 'INTACT' && links > 0 && sessions > 0) {
    return { toc: 'chain checked', page: { integrityHead: 'The recorded chain adds up.',
      integrityMeta: 'checked ' + d.checked_at, integritySteps: counted, chain: scope } };
  }
  if (d.state === 'MISMATCH') {
    return { toc: 'chain mismatch', page: { integrityHead: 'The recorded chain does not add up.',
      integrityMeta: 'checked ' + d.checked_at, integritySteps: counted, chain: scope } };
  }
  return { toc: 'could not check', page: { integrityHead: 'Could not check the recorded chain.',
    integrityMeta: 'checked ' + d.checked_at, integritySteps: counted, chain: scope } };
}

/* C30 P0-1 — ⑥ So on one line of what counts as done: that line's own state,
 * drawn with the DESIGN-SPEC §1.4 tokens, or no verdict at all. "Proven" is
 * drawn only with the version the record says the line is proven on; a
 * Proven state with no version, or a line with no state, draws nothing.
 * The five chain links are lit only on a line that is Proven on its version. */
export const SO_WORDS = Object.freeze({
  proven: 'Proven',
  not_proven: 'Not proven',
  needs_recheck: 'Needs a re-check',
  in_progress: 'In progress',
  recorded: 'Recorded',
  none: 'no verdict recorded',
});
const SO_STYLE = {
  proven: { glyph: '●', gc: 'var(--tl-proven, #3DD68C)', color: 'var(--tl-proven, #3DD68C)', border: '1px solid rgba(var(--tl-proven-rgb, 61, 214, 140), .6)', bg: 'rgba(var(--tl-proven-rgb, 61, 214, 140), .08)' },
  not_proven: { glyph: '○', gc: '#A1A1AA', color: '#EDEDEF', border: '1px dashed #82828C', bg: 'transparent' },
  needs_recheck: { glyph: '◭', gc: '#F2C94C', color: '#F2C94C', border: '1px solid rgba(242,201,76,.6)', bg: 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)' },
  in_progress: { glyph: '◐', gc: '#52A8FF', color: '#52A8FF', border: '1px solid rgba(82,168,255,.6)', bg: 'rgba(82,168,255,.08)' },
  recorded: { glyph: '▪', gc: '#A1A1AA', color: '#EDEDEF', border: '1px solid #66666F', bg: 'transparent' },
};
const LINK_LIT = { link: '#FF7A29', linkGlow: '0 0 6px rgba(255,154,82,.6)' };
const LINK_UNLIT = { link: '#2A2A30', linkGlow: 'none' };
export function lineVerdict(state, version, scope) {
  const ver = typeof version === 'string' && version.trim() ? version : null;
  const st = SO_STYLE[state] ? state : null;
  if (!st || (st === 'proven' && !ver)) return { so: null, said: SO_WORDS.none, ...LINK_UNLIT };
  const sc = Array.isArray(scope) ? scope.join(', ') : '';
  return {
    so: { ...SO_STYLE[st], word: SO_WORDS[st] + (ver ? ' · version ' : ''), version: ver || '', scope: st === 'proven' ? sc : '' },
    said: SO_WORDS[st] + (ver ? ' · version ' + ver : ''),
    ...(st === 'proven' ? LINK_LIT : LINK_UNLIT),
  };
}

function liveVals(self, D) {
  const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', OR = '#FF7A29', LINE = '#2A2A30', HAIR = '#1F1F23';
  const VIOLET = '#B78CFF', GREEN = 'var(--tl-proven, #3DD68C)', YELLOW = '#F2C94C';
  const PANEL = '#0A0A0B', NOWBG = '#140E0B';
  const pl = (n, one, many) => (n === 1 ? one : many);
  const step = (n) => 'step ' + n;
  /* C30 R-ROUTES: "Run step N ›" opens that step; a criteria line has no run of its own to open yet */
  const stepTo = stepLinker(self.props.at, D.project && D.project.id);
  const result = (r) => (r.passed == null
    ? (r.outcome === 'succeeded' ? 'ended, no count' : r.outcome === 'failed' ? 'failed, no count' : 'no result yet')
    : r.failed > 0 ? fmtCount(r.failed) + ' failed, ' + fmtCount(r.passed) + ' passed' : fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed');
  const V = D.version.id || 'not recorded';
  const title = D.change.title || 'Untitled work';
  const agents = D.agents.length ? D.agents.join(', ') : 'No agent recorded';
  const crit = D.criteria;

  // ---- how it went ----
  const item = (glyph, gc, gs, t, tc, sub) => ({ glyph, gc, gs, title: t, tc, sub });
  const evItem = (e) => {
    if (e.kind === 'gap') return item('◌', T3, '15px', 'not recorded', T2, 'before ' + e.before);
    if (e.kind === 'prompt') return item('•', T2, '20px', 'you asked', T1, step(e.step));
    if (e.kind === 'edits') return item('•', T2, '20px', countOf(e.count, 'edit', 'edits'), T1, e.from == null ? '' : e.to == null || e.to === e.from ? step(e.from) : 'steps ' + e.from + '–' + e.to);
    if (e.kind === 'statements') return item('❝', VIOLET, '15px', countOf(e.count, 'statement', 'statements'), T1, step(e.step));
    if (e.passed == null && e.outcome == null) return item('⋯', T2, '15px', 'no result yet', T1, step(e.step));
    if ((e.failed || 0) > 0 || e.outcome === 'failed') return item('✕', T2, '13px', result(e), T1, step(e.step));
    return item('•', T1, '20px', result(e), T1, step(e.step));
  };
  const bands = D.timeline.map((bd) => {
    const now = bd.kind === 'version' && !!bd.current;
    const label = bd.kind === 'unrecorded' ? 'earlier' : bd.kind === 'session' ? 'session' : bd.kind === 'earlier' ? fmtCount(bd.versions) + ' earlier' : bd.version + (now ? ' · now' : '');
    const bg = now ? NOWBG : PANEL;
    const items = bd.events.map(evItem).map((i) => ({ ...i, bg, aria: label + ': ' + i.title + (i.sub ? ', ' + i.sub : '') }));
    return { label, labelColor: now ? T1 : T3, bg, border: bd.kind === 'unrecorded' ? 'none' : '1px solid ' + HAIR, line: bd.kind === 'unrecorded' ? '1px dashed ' + T3 : '1px solid ' + LINE, lit: now && !!D.approval, items };
  });

  // ---- what counts as done ----
  const critRows = crit.lines.map((c, i) => ({
    n: String(c.n), gate: false, text: c.text, why: '',
    border: i === 0 ? 'none' : '1px solid ' + HAIR,
    chainLabel: 'Line ' + c.n + ': ' + lineVerdict(c.state, c.state === 'proven' ? (c.proven ? c.proven.version : null) : crit.on, []).said,
    acceptedAria: 'Accepted at ' + (crit.acceptedAt || 'a time not recorded'), accepted: crit.acceptedAt || '—',
    restsAria: '', restsStep: '—', ranAria: '', ranOn: '—', sinceAria: '', since: '—', recheckAria: '', recheck: '—',
    ...lineVerdict(c.state, c.state === 'proven' ? (c.proven ? c.proven.version : null) : crit.on, c.proven ? c.proven.scope : [])
  }));

  // ---- runs ----
  const runs = D.runs.map((r, i) => {
    const none = r.passed == null && r.outcome == null;
    return {
      time: r.time || '—', step: step(r.step), cmd: clip(r.command, CMD_MAX), full: r.command || '', destructive: false,
      glyph: none ? '⋯' : ((r.failed || 0) > 0 || r.outcome === 'failed' ? '✕' : ''), gc: T2,
      result: result(r), rc: none ? T2 : T1,
      ver: r.version ? r.version + (r.now ? ' · now' : '') : '—',
      vc: r.now ? T1 : T3, vbg: r.now ? 'rgba(255,122,41,.10)' : 'transparent',
      border: i === 0 ? 'none' : '1px solid ' + HAIR
    };
  });
  const runsNote = ', and a passing run speaks only for the version it ran on. '
    + (D.testsTotal > D.runs.length ? 'These are the last ' + fmtCount(D.runs.length) + ' of ' + fmtCount(D.testsTotal) + ' test runs.' : D.testsTotal === 0 ? 'No test run is recorded for this work.' : 'Every test run recorded for this work is listed.');

  // ---- what the agent said ----
  let s1 = null;
  if (D.s1) {
    const x = D.s1;
    const proven = x.verdict === 'proven';
    let why0 = '';
    if (x.failedRun) why0 = ': a test run after it failed at ' + step(x.failedRun.step) + '.';
    else if (x.stopsAt === 2 || !x.rested) why0 = ': no test run before it is in the record, so the proof stopped at ②.';
    else if (x.since) why0 = ': it rested on ' + step(x.rested.step) + ' (on ' + x.rested.version + '), and ' + x.since.file + ' changed at ' + step(x.since.step) + ', so the proof stopped at ⑤.';
    else why0 = ': it rested on ' + step(x.rested.step) + ', which did not pass, so the proof stopped at ③.';
    s1 = {
      said: (x.agent || 'The agent') + ' said ', quote: x.quote, where: step(x.step) + (x.time ? ' · ' + x.time : ''),
      show0: !proven, at0: x.time || '—', read0: ' Not proven · version ' + x.version, why0,
      show1: proven, at1: x.rested ? x.rested.time || '—' : '—',
      why1: x.rested ? 'It rests on ' + step(x.rested.step) + (x.match && x.match.sameCommandAs ? ': the same command as ' + step(x.match.sameCommandAs) : ': a passing test run') + ', on ' + x.rested.version + ', after the last edit. Every link holds.' : '',
      showProven: proven,
      proven: 'Proven: the tests pass ', provenMeta: '· version ' + x.version,
      run: x.rested ? x.rested.command + ' · ' + result(x.rested) : '',
      runVersion: x.rested ? x.rested.version + (x.rested.version === x.version ? ' · same version' : '') : '',
      runStep: x.rested ? step(x.rested.step) + (isStep(x.rested.step) ? ' ›' : '') : '',
      runHref: x.rested ? stepTo(x.rested.step) : undefined,
      note: 'The record holds what ThruLine saw before the agent spoke; the last version it recorded before this was said is ' + (x.versionBefore || 'not recorded') + '.'
    };
  }
  const s2 = D.s2 ? {
    said: (D.s2.agent || 'The agent') + ' said ', quote: D.s2.quote, where: step(D.s2.step) + (D.s2.time ? ' · ' + D.s2.time : ''),
    why: 'The record can’t check “' + D.s2.word + '” by itself. It is decided by ',
    tail: crit.accepted ? ': ' + (crit.proven == null ? 'how many lines hold is not recorded' : fmtCount(crit.proven) + ' of ' + fmtCount(crit.lineCount) + ' lines hold') + ', on ' + V + '.' : ', which is not accepted yet.'
  } : null;
  const s3 = D.s3 ? { quote: D.s3.quote, where: step(D.s3.step) } : null;

  // ---- findings ----
  const groups = D.findings.map((f, i) => ({
    findings: [{ n: String(i + 1), text: f.text + ' ', where: f.where || '' }],
    settlement: f.settled ? { head: 'Settled.', text: '', ref: '' } : f.nothing ? { head: 'Nothing to settle.', text: ' A run speaks only for the version it ran on.', ref: '' } : f.resolution ? { head: 'Answered:', text: ' ' + f.resolution, ref: '' } : null
  }));
  const settled = D.findings.filter((f) => f.settled).length;

  // ---- decisions ----
  const dec = D.decisions.map((d) => {
    const at = d.ts ? hm(d.ts) : '—';
    if (d.kind === 'accept') return { at, text: 'You accepted what counts as done, ' + (d.criteriaVersion || 'a version not recorded') + (d.statements == null ? '.' : ': ' + countOf(d.statements, 'line', 'lines') + '.'), state: null, glyph: null, file: null };
    if (d.kind === 'withdraw') return { at, text: 'Approval of ' + short(d.version) + ' withdrawn by ' + d.by + '.', state: null, glyph: null, file: null };
    return { at, text: ' Approved on ' + short(d.version) + ' by ' + d.by + (d.criteriaVersion ? ', against ' + d.criteriaVersion : '') + '. It names this version only.', state: null, glyph: null, file: d.file };
  });
  if (!dec.length) dec.push({ at: '—', text: LIVE_TEXT.noDecision, state: null, glyph: null, file: null });

  // ---- not recorded ----
  const gaps = D.gaps.map((g) => {
    if (g.kind === 'before_capture') return { glyph: '◌', glyphColor: T3, head: 'Before ' + g.at + '.', text: ' Nothing earlier is in the record.' };
    if (g.kind === 'no_result') return { glyph: '⋯', glyphColor: T2, head: fmtCount(g.count) + ' test ' + pl(g.count, 'run', 'runs') + ' with no result.', text: ' The record holds the command, not how it ended; the latest is ' + step(g.step) + '.' };
    if (g.kind === 'code_at_statement') return { glyph: '◌', glyphColor: T3, head: 'The code at the moment the agent spoke.', text: ' The last version recorded before ' + step(g.step) + ' is ' + (g.lastVersion || 'not recorded') + '.' };
    if (g.kind === 'edit_line_counts') return { glyph: '◌', glyphColor: T3, head: 'How many lines ' + countOf(g.count, 'edit', 'edits') + ' changed.', text: ' Lines not recorded; the edits and their files are.' };
    return { glyph: '◌', glyphColor: T3, head: LIVE_TEXT.notAccepted + '.', text: ' Nothing on this page is measured against it.' };
  });

  // ---- page sentences ----
  const masked = D.masking && typeof D.masking.masked === 'number' ? D.masking.masked : null;
  const saidSteps = [D.s1, D.s2].filter(Boolean).map((x) => x.step).sort((a, b) => a - b);
  const page = {
    project: D.project.name || 'No project',
    root: D.project.root || 'folder not recorded',
    title,
    version: V,
    versionMeta: (D.version.commit ? '(current) · = commit ' + D.version.commit : '(current)') + (D.version.unsaved === true ? ' · unsaved changes' : D.version.unsaved === false ? ' · no unsaved changes' : ''),
    recording: recordingPill({ change: { recording: D.change.following ? true : null } }).text,
    readAt: 'read ' + D.readAt + ' ↻',
    describes: 'describes ' + V + ' as of ' + (D.readDate ? D.readDate + ' ' : '') + D.readAt,
    sessions: countOf(D.sessions, 'session', 'sessions') + ' · ' + agents + ' · ' + (D.steps == null ? 'steps not counted' : fmtCount(D.steps) + ' recorded steps'),
    chipGlyph: D.approval ? '◆' : '◇',
    chipWord: D.approval ? 'Approved on ' : 'Not approved · ',
    approvedOn: D.approval ? short(D.approval.version) : V,
    approvedBy: D.approval ? 'by ' + D.approval.by + ' at ' : '',
    approvedAt: D.approval ? hm(D.approval.ts) : '',
    approvedAgainst: D.approval ? (D.approval.criteriaVersion ? ', against what counts as done, version ' + D.approval.criteriaVersion : '') : (crit.accepted ? 'what counts as done is accepted, and no version is approved' : LIVE_TEXT.notAccepted),
    restsOn: crit.accepted
      ? (crit.proven == null ? 'How many of the ' + fmtCount(crit.lineCount) + ' things that count as done hold on ' + V + ' is not recorded.' : 'It rests on ' + fmtCount(crit.proven) + ' of ' + fmtCount(crit.lineCount) + ' things that count as done, on ' + V + '.')
      : 'Nothing is measured yet: ' + LIVE_TEXT.notAccepted.toLowerCase() + '.',
    barSet: null,
    howItWent: 'How it went' + (D.story.date ? ' · ' + D.story.date + ' · ' + D.story.from + ' → ' + D.story.to + ' · this machine’s time' : ' · nothing recorded'),
    doneMeta: crit.accepted ? (crit.version || '') + (crit.acceptedAt ? ' · accepted ' + crit.acceptedAt : '') : 'not accepted yet',
    doneNote: crit.accepted ? ' Each line rests on what the record holds for it.' : ' ' + LIVE_TEXT.notAccepted + ', so nothing on this page is measured against it.',
    saidMeta: agents + ' · ' + (saidSteps.length ? (saidSteps.length === 1 ? step(saidSteps[0]) : 'steps ' + saidSteps.join(', ')) : 'nothing said') + ' · ' + countOf(D.statementsCount, 'statement', 'statements') + ' in all',
    s1, s2, s3,
    runsRoot: D.project.root || 'a folder not recorded',
    runsNote,
    findingsMeta: fmtCount(D.findings.length) + ' found · ' + fmtCount(settled) + ' settled · ' + fmtCount(D.findings.length - settled) + ' open',
    groups,
    groupsNone: groups.length ? null : LIVE_TEXT.noFinding,
    dec,
    gapsMeta: countOf(gaps.length, 'gap', 'gaps'),
    gaps,
    integrityHead: LIVE_TEXT.integrityOff,
    integrityMeta: 'not checked here',
    integritySteps: (D.steps == null ? 'steps not counted' : fmtCount(D.steps) + ' steps in this record'),
    chain: 'check the whole record with thruline verify',
    /* The canvas says the file is internally consistent and names `thruline verify <file>`. No
     * command checks an exported file (`thruline verify` takes no file), so the live page says
     * what the file does not prove and names no check (C30: no claim without the act behind it). */
    fileTells: 'Nothing in this file proves it is unchanged since it was saved. It does not show who made it, or that the code is correct.',
    fileCheckOff: true,
    footTimes: 'Times on this page are this machine’s local time' + (masked == null ? '' : ' · masked before this page was shown: ' + fmtCount(masked) + ' masked · best-effort'),
    maskNote: 'The file holds no check output, so there is none of that to leave out. ' + (masked == null ? 'Whether anything was masked before this page was shown is not recorded.' : 'Secrets were masked before this page was shown: ' + fmtCount(masked) + ' masked · best-effort.'),
    readableAt: null,
    readableSum: null,
    storage: storageSentence(D.storageBytes)
  };

  const integrity = integrityOf(self.props.integrity);
  Object.assign(page, integrity.page);

  const words = [
    { glyph: '●', gc: GREEN, word: 'Proven', def: 'A run on this version shows it.' },
    { glyph: '○', gc: T2, word: 'Not proven', def: 'Nothing shows it on this version.' },
    { glyph: '◭', gc: YELLOW, word: 'Needs a re-check', def: 'It was shown on an earlier version.' },
    { glyph: '▪', gc: T2, word: 'Recorded', def: 'In the record; it settles nothing.' },
    { glyph: '⋯', gc: T2, word: 'No result yet', def: 'A step with no result recorded.' },
    { glyph: '◌', gc: T3, word: 'Not recorded', def: 'Nothing was captured here.' },
    { glyph: '◇', gc: T1, word: 'Ready for approval', def: 'Every line holds; not yet decided.' },
    { glyph: '◆', gc: T1, word: 'Approved', def: 'Approved by name, for one version.' }
  ];
  const toc = [
    { href: '#ws-outcome', label: 'The outcome', meta: V },
    { href: '#ws-done', label: 'What counts as done', meta: crit.accepted ? (crit.proven == null ? '—' : fmtCount(crit.proven) + ' of ' + fmtCount(crit.lineCount)) : 'not accepted' },
    { href: '#ws-said', label: 'What the agent said', meta: fmtCount(D.statementsCount) },
    { href: '#ws-runs', label: 'Runs', meta: countOf(D.testsTotal, 'test run', 'test runs') },
    { href: '#ws-findings', label: 'Findings', meta: fmtCount(D.findings.length) },
    { href: '#ws-decisions', label: 'Decision history', meta: fmtCount(D.decisions.length) },
    { href: '#ws-not-recorded', label: 'What was not recorded', meta: fmtCount(gaps.length) },
    { href: '#ws-integrity', label: 'Record integrity', meta: integrity.toc },
    { href: '#ws-words', label: 'Words', meta: String(words.length) }
  ].map((t, i) => ({ ...t, color: i === 0 ? T1 : T2, current: i === 0 ? 'location' : 'false', bar: i === 0 ? '2px solid ' + OR : '2px solid transparent', glow: i === 0 ? '-4px 0 8px -4px rgba(255,154,82,.7)' : 'none' }));

  /* C30 R-PR: the link in to this Change's pull-request preview */
  const at = self.props.at || {};
  return {
    isDemo: false,
    live: true,
    prHref: at.change ? routeHref('pr', { change: at.change, project: at.project }) : null,
    page,
    bands,
    crit: critRows,
    runs,
    words,
    toc,
    notes: D.notes,
    seconds: D.seconds,
    objected: D.objected,
    /* C30 R-LEDGER: the notes people kept on this Change, in one slot of the template */
    noteRows: Array.isArray(D.notes) && D.notes.length ? React.createElement(WholeStoryNotes, { notes: D.notes }) : null,
    secondRows: Array.isArray(D.seconds) && D.seconds.length ? React.createElement(WholeStorySeconds, { seconds: D.seconds }) : null,
    /* C30 R-LEDGER: the statements people said were wrong, beside the verdict each already had, in one more slot */
    objectionRows: Array.isArray(D.objected) && D.objected.length ? React.createElement(WholeStoryObjections, { objected: D.objected }) : null,
    ...exportVals(self, D, { page, bands, crit: critRows, runs, words, notes: D.notes, seconds: D.seconds, objected: D.objected }, title),
    designNotes: []
  };
}

/* ---------- the export rail, live (C30 R-ROUTES "Export and Download") ----------
 * The file is made by export.js from the page's own values above and nothing
 * else; the preview shows its every line, Download saves exactly that text,
 * and Copy as Markdown copies the Markdown file's. Readable now is the
 * served answer of GET /api/changes/<id>/export (`props.readable`, read by
 * index.jsx), for a tracked Change only. */
export const READ_TEXT = Object.freeze({
  untracked: 'Readable now applies to a tracked Change only. This work is not tracked as a Change, so no record was looked at.',
  reading: 'Looking at each record this rests on.',
  failed: 'Whether each record can be read now could not be checked, so read it as unknown, not as readable.',
  none: 'Nothing this rests on is kept outside the record, so there was nothing to look at.',
  all: (n) => 'Every one of the ' + fmtCount(n) + ' records this rests on could be read.',
  some: (k, n) => fmtCount(k) + ' of the ' + fmtCount(n) + ' records this rests on could be read.',
});
/** The route's own words for whether a record can be read now (change_export.rs `Standing`). */
export const STANDING_WORDS = Object.freeze({
  readable: 'readable',
  missing: 'missing',
  permission_denied: 'permission denied',
  content_changed: 'content changed',
  dangling_reference: 'dangling reference',
  not_written: 'never written',
  unreadable: 'could not be read',
});
const EVIDENCE_WHAT = { check_run: 'A check run', finding: 'A finding', observation: 'An observation' };
const STATUS_TEXT = {
  ready: 'Made in this browser from exactly the text above. Nothing is sent anywhere.',
  saved: 'Your browser has the file now, and saves it wherever it keeps your downloads on this Mac. Nothing was sent anywhere.',
  copied: 'Copied: the same text the Markdown file holds. Nothing is written to a file, and nothing leaves this Mac.',
  save: 'This browser would not save the file. Nothing was saved and nothing was sent.',
  copy: 'This browser would not copy the text. Nothing was copied, and nothing was sent.',
};
const RESET = { saved: false, copied: false, failed: null };

/* The face (Plain / Technical) lives here and only here. A `face` prop — the
 * app-wide preference, when one is served (C30 R-PREFS) — wins over the
 * screen's own state, and `onFace` hears of every change. */
export function faceOf(self) {
  const f = self.props && self.props.face;
  if (f === 'plain' || f === 'technical') return f;
  /* live, the stored face (data/face.js); in the gallery the screen's own */
  return storedFaceOf(self.props, self.state.face === 'technical' ? 'technical' : 'plain');
}
function setFace(self, face) {
  switchFace(self.props, face, (f) => self.setState({ face: f }));
  if (self.props && typeof self.props.onFace === 'function') self.props.onFace(face);
}

/* C30 R-PREFS "When you export, also make a masked copy": while the person has not touched
 * the redacted copy on this page, a stored `masked_copy: true` starts it ticked — every kind
 * of thing it can leave out. Touching any switch or box here keeps what is on screen from then
 * on, and a change of the setting does not reach into a page already being used. The gallery
 * hands the screen no preferences, so it starts as the canvas drew it. */
function maskedByDefault(self) {
  return isLive(self.props) && !self.state.cutTouched && storedOf(self.props, 'masked_copy', false) === true;
}

/** Readable now, from the served /export document: the rail's rows and sentences, and the file's. */
function readableOf(read, D, tech) {
  if (!D.tracked || !D.change.id) return { status: 'untracked', at: 'not looked at', sum: READ_TEXT.untracked, rows: [], doc: null };
  const r = read || { status: 'reading' };
  if (r.status !== 'ready' || !r.doc) {
    return r.status === 'failed'
      ? { status: 'failed', at: 'not looked at', sum: READ_TEXT.failed, rows: [], doc: null }
      : { status: 'reading', at: 'looking now', sum: READ_TEXT.reading, rows: [], doc: null };
  }
  const doc = r.doc;
  const root = D.project.root;
  const rows = [];
  const add = (code, what, detail, section, ref, paths) => {
    const ok = code === 'readable';
    rows.push({
      code, readable: ok, what, detail, section, ref, paths,
      word: STANDING_WORDS[code] || 'not known',
      glyph: ok ? '✓' : '◌', glyphColor: ok ? '#A1A1AA' : '#82828C', wordColor: ok ? '#A1A1AA' : '#EDEDEF',
    });
  };
  for (const d of Array.isArray(doc.decisions) ? doc.decisions : []) {
    if (!d || d.standing == null) continue; // a withdrawal writes no copy, so there is none to read
    const path = inProject(d.path, root);
    add(d.standing, 'The approval, as a file', path || 'the approval of ' + short(d.version), 'decisions', 'decision ' + d.decision_id, path ? [path] : []);
  }
  for (const e of Array.isArray(doc.evidence) ? doc.evidence : []) {
    if (!e) continue;
    add(e.standing, EVIDENCE_WHAT[e.kind] || 'A record', e.name ? homeless(e.name) : 'its name is not in the record', 'done', e.kind + ' ' + e.evidence_id + ' · session ' + e.session_id, []);
  }
  for (const t of Array.isArray(doc.transcripts) ? doc.transcripts : []) {
    if (!t) continue;
    const path = inProject(t.file, root);
    const lost = (t.lines_changed || 0) + (t.lines_gone || 0) > 0 ? ', ' + fmtCount(t.lines_changed || 0) + ' changed, ' + fmtCount(t.lines_gone || 0) + ' gone' : '';
    add(t.standing, 'The recorded conversation', (path ? path + ' · ' : '') + countOf(t.lines || 0, 'recorded line', 'recorded lines') + lost, 'said', 'transcript · session ' + t.session_id + ' · ' + (t.lines || 0) + ' lines, ' + (t.lines_gone || 0) + ' gone, ' + (t.lines_changed || 0) + ' changed', path ? [path] : []);
  }
  rows.forEach((x, i) => {
    x.border = i === 0 ? 'none' : '1px solid #1F1F23';
    if (tech) x.tech = x.ref + ' · ' + x.code;
  });
  /* the served totals, never counted here */
  const T = doc.totals || {};
  const total = typeof T.records === 'number' ? T.records : rows.length;
  const ok = typeof T.readable === 'number' ? T.readable : rows.filter((x) => x.readable).length;
  const sum = total === 0 ? READ_TEXT.none : ok === total ? READ_TEXT.all(total) : READ_TEXT.some(ok, total);
  return { status: 'ready', at: 'looked at ' + hm(doc.checked_at), sum, rows, doc };
}

/** The file's own technical part: ids, the version, the moments in UTC, the masking, each record by its code. */
function technicalOf(D, R, V) {
  const out = ['Format: ' + EXPORT_FORMAT + ' ' + EXPORT_SCHEMA_VERSION];
  out.push(D.tracked ? 'Change id: ' + D.change.id : 'Work id: ' + (D.change.threadId || D.change.id || 'not recorded') + ' · not tracked as a Change');
  out.push('Version: ' + V + (D.version.commit ? ' = commit ' + D.version.commit : ''));
  if (D.approval) out.push('Approved at (UTC): ' + utcOf(D.approval.ts));
  if (R.doc && R.doc.checked_at) out.push('Records looked at (UTC): ' + R.doc.checked_at);
  const served = R.doc && R.doc.masking && typeof R.doc.masking.statement === 'string' ? R.doc.masking.statement : null;
  if (served) out.push('Secret masking before this was served: ' + served);
  else if (D.masking && typeof D.masking.masked === 'number') out.push('Secret masking before this was shown: ' + fmtCount(D.masking.masked) + ' masked');
  if (D.leftOutByExclusion > 0) out.push('Left out by your exclusion: ' + fmtCount(D.leftOutByExclusion) + ' entries');
  for (const x of R.rows) out.push('Record: ' + x.ref + ' · ' + x.code);
  return out;
}

/** What a redacted copy can leave out, from the typed fields the page draws them from. */
function removableOf(D, R) {
  const list = (x) => (Array.isArray(x) ? x : []);
  const ok = (x) => typeof x === 'string' && x.trim() !== '' && !isExclusionMarker(x);
  const paths = [
    D.project.root,
    D.s1 && D.s1.since ? D.s1.since.file : null,
    ...list(D.findings).flatMap((f) => list(f.files)),
    ...list(D.decisions).map((d) => d.file),
    ...list(D.criteria && D.criteria.lines).flatMap((c) => (c.proven ? list(c.proven.scope) : [])),
    ...R.rows.flatMap((r) => list(r.paths)),
  ];
  const words = [D.s1 && D.s1.quote, D.s2 && D.s2.quote, D.s3 && D.s3.quote];
  return { paths: paths.filter(ok), words: words.filter(ok), output: [] };
}

function exportVals(self, D, shown, title) {
  const s = self.state;
  const T1 = '#EDEDEF', T2 = '#A1A1AA', OR = '#FF7A29', CTRL = '#66666F', LINE = '#2A2A30';
  const io = self.props.io || IO;
  const tech = faceOf(self) === 'technical';
  const V = D.version.id || 'not recorded';
  const R = readableOf(self.props.readable, D, tech);
  shown.page.readableAt = R.at;
  shown.page.readableSum = R.sum;
  const P = { ...shown, readable: R, technical: technicalOf(D, R, V) };
  const removable = removableOf(D, R);
  const facts = { title, version: D.version.id, commit: D.version.commit, changeId: D.tracked ? D.change.id : null, tracked: D.tracked, leftOutByExclusion: D.leftOutByExclusion };

  // ---- the redacted copy: which classes are ticked, and what each removes ----
  const fromSettings = maskedByDefault(self);
  const isTicked = (k) => (fromSettings ? true : !!s[k]);
  const ticked = CLASSES.map((c) => c.key).filter(isTicked);
  const counts = classCounts(P, ticked, removable);
  const keys = ticked.filter((k) => counts[k] > 0);
  const classes = ['paths', 'words', 'output'].map((k) => CLASSES.find((c) => c.key === k)).map((c) => {
    const on = keys.includes(c.key);
    const enabled = counts[c.key] > 0;
    return {
      key: c.key, label: c.label, count: fmtCount(counts[c.key]), enabled, disabled: !enabled,
      checked: on ? 'true' : 'false', mark: on ? '✓' : '', boxBg: on ? OR : 'transparent', boxBorder: on ? OR : CTRL,
      toggle: () => self.setState({ ...Object.fromEntries(CLASSES.map((x) => [x.key, isTicked(x.key)])), [c.key]: !isTicked(c.key), cutTouched: true, ...RESET })
    };
  });
  const first = classes.find((k) => k.enabled);

  // ---- the file: what the preview shows is what Download saves ----
  const file = storyFile(P, { fmt: s.fmt, keys, removable, facts });
  const md = file.fmt === 'md' ? file : storyFile(P, { fmt: 'md', keys, removable, facts });
  const redacted = file.redaction.redacted;
  const lines = file.text.split('\n');
  if (lines[lines.length - 1] === '') lines.pop();
  const MARKS = CLASSES.flatMap((c) => [c.marker, c.marker.replace(/[[\]]/g, (b) => '\\' + b)]);
  const marked = (t) => MARKS.some((m) => t.includes(m)) || (redacted && /left out\b|derived_from_sha256|leaves things out/.test(t));
  const preview = lines.map((t) => ({ text: t === '' ? ' ' : t, color: marked(t) ? T1 : T2 }));

  // ---- the actions: nothing is sent anywhere ----
  const download = () => {
    try {
      io.save(file);
      self.setState({ saved: true, copied: false, failed: null });
    } catch (e) {
      self.setState({ saved: false, copied: false, failed: 'save' });
    }
  };
  const copyMd = () => {
    let done;
    try { done = Promise.resolve(io.copy(md.text)); } catch (e) { done = Promise.reject(e); }
    done.then(() => self.setState({ copied: true, saved: false, failed: null }), () => self.setState({ copied: false, saved: false, failed: 'copy' }));
  };
  const status = s.failed ? s.failed : s.saved ? 'saved' : s.copied ? 'copied' : 'ready';

  // ---- the format ----
  const fmtDefs = [{ key: 'md', name: 'Markdown', ext: '.md' }, { key: 'html', name: 'HTML', ext: '.html' }, { key: 'json', name: 'JSON', ext: '.json' }];
  const fmts = fmtDefs.map((f) => {
    const on = file.fmt === f.key;
    return { name: f.name, ext: f.ext, checked: on ? 'true' : 'false', bg: on ? 'rgba(255,122,41,.10)' : 'transparent', border: on ? OR : CTRL, color: on ? T1 : T2, ring: on ? OR : CTRL, dot: on ? OR : 'transparent', pick: () => self.setState({ fmt: f.key, ...RESET }) };
  });
  const notes = {
    md: 'Plain text (Markdown). The same text the Copy as Markdown button copies.',
    html: 'A page anyone can open in a browser. One file. It opens without the internet, and it runs nothing.',
    json: 'For other tools (JSON). The same record, as named fields a program can read.'
  };

  // ---- the technical face: the file's own technical part, and this exact file's fingerprint ----
  const techLines = tech
    ? [...file.technical, 'Fingerprint of this exact file (sha256): ' + sha256Hex(file.text), fmtCount(new TextEncoder().encode(file.text).length) + ' bytes · ' + file.mime]
    : null;

  return {
    fmts,
    fmtNote: notes[file.fmt],
    fmtEngineShow: false, fmtEngineTag: '', fmtEngine: '',
    classes,
    cutOn: redacted ? 'true' : 'false',
    cutNote: redacted,
    cutOff: !first && !redacted,
    cutSub: redacted ? 'On · a second file, leaving out what you tick' + (fromSettings ? ' · started on by Settings' : '') : first ? 'Off · the file holds everything on this page' : 'Off · nothing on this page is of a kind it leaves out',
    toggleCut: () => {
      if (redacted) self.setState({ ...Object.fromEntries(CLASSES.map((x) => [x.key, false])), cutTouched: true, ...RESET });
      else if (first) self.setState({ ...Object.fromEntries(CLASSES.map((x) => [x.key, false])), [first.key]: true, cutTouched: true, ...RESET });
    },
    swBg: redacted ? OR : LINE, swBorder: redacted ? OR : CTRL, swKnob: redacted ? '18px' : '2px', swKnobBg: redacted ? '#000000' : T2,
    fileName: file.filename,
    previewMeta: 'the whole file · ' + (redacted ? 'redacted copy' : 'full copy'),
    preview,
    downloadLabel: 'Download .' + file.fmt,
    download,
    copyMd,
    exportOff: false,
    statusText: STATUS_TEXT[status],
    statusColor: status === 'ready' ? T2 : T1,
    readable: R.rows,
    readableShow: R.rows.length > 0,
    techPressed: tech ? 'true' : 'false',
    faceAria: tech ? 'Face: Technical (switch to Plain)' : 'Face: Plain (switch to Technical)',
    plainColor: tech ? T2 : T1,
    techColor: tech ? T1 : undefined,
    toggleFace: () => setFace(self, tech ? 'plain' : 'technical'),
    techLines
  };
}

const pad2 = (n) => String(n).padStart(2, '0');
/** The runs table's command column holds this many characters; a longer command ends in "…". */
export const CMD_MAX = 52;
function clip(t, n) {
  const x = String(t || '');
  return x.length > n ? x.slice(0, n - 1) + '…' : x;
}
function hm(ts) {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? '—' : pad2(d.getHours()) + ':' + pad2(d.getMinutes());
}
function utcOf(ts) {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? 'a time not recorded' : d.toISOString().replace(/\.\d{3}Z$/, 'Z');
}
function short(rev) {
  return typeof rev === 'string' && /^[0-9a-f]{8,}$/i.test(rev) ? rev.slice(0, 7) : rev;
}
