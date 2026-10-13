/* Onboarding.dc.html: the artboard's logic, from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from
 * `this.props.data` (the store, Wave 2) or, in the design gallery, from DEMO
 * (./demo.js, the contract in ./DATA.md); `this.state` holds UI state only —
 * except that in the gallery the DEMO STATE switcher also picks the capture
 * state, which in the product is the chosen folder's `capture`.
 *
 * Live (C30 R-ROUTES "Capture health"): the chosen folder's `capture` is the
 * capture route's answer, folded in by index.jsx (capture-view.js
 * `withCapture`), and every word step 3 draws from it comes from
 * capture-view.js `stepThree`. The screen tells index.jsx which folder and
 * step are in front (`actions.place`), so only that folder is read. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { middle } from '../../data/paths.js'
import { fmtCount } from '../../data/count.js'
import { href } from '../../routes.js'
import { stepThree } from './capture-view.js'
import { installView } from './install-view.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

/** The folder in front of the person: the one they chose, else the default the data names. */
const chosenOf = (D, s) => {
  const folders = (D && D.folders) || [];
  return folders.find((f) => f.id === s.folder) || folders.find((f) => f.id === (D && D.defaultFolder)) || {};
};

/** A path a shell reads back as itself: as it is when plain, else quoted ("~/" kept outside the quotes). */
const shellPath = (p) => {
  const q = (x) => "'" + String(x).replace(/'/g, "'\\''") + "'";
  if (typeof p !== 'string' || /^[\w./~@%+=:,-]+$/.test(p)) return p;
  return p.startsWith('~/') ? '~/' + q(p.slice(2)) : q(p);
};

export default class Component extends DCLogic {
  state = { step: 2, folder: 'checkout', mcp: true, speaker: false, runcap: false, installed: false, health: 'waiting', copied: '' };

  setState(partial) {
    super.setState(partial);
    this.tellPlace();
  }

  /* live: say which folder and step are in front, once per change (index.jsx reads that folder only) */
  tellPlace() {
    const a = this.props && this.props.actions;
    const D = this.props && this.props.data;
    if (!D || !a || typeof a.place !== 'function') return;
    const id = chosenOf(D, this.state).id || null;
    const key = id + '|' + this.state.step;
    if (key === this.__told) return;
    this.__told = key;
    a.place({ folder: id, step: this.state.step });
  }

  renderVals() {
    // the product opens on step 1 (choose the folder, read every session); the gallery keeps the canvas's step 2
    if (this.props.data && !this.__opened) { this.__opened = true; this.state = Object.assign({}, this.state, { step: 1 }); }
    const s = this.state;
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    const O = '#FF7A29', T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C';
    const LINE = '#2A2A30', HAIR = '#1F1F23', CTRL = '#66666F', BLUE = '#52A8FF';
    const GLOW = '0 0 6px rgba(255,154,82,.6)';
    const go = (o) => () => this.setState(o);
    const show = (b) => (b ? 'flex' : 'none');
    const showI = (b) => (b ? 'inline-flex' : 'none');
    const joinList = (a) => (a.length <= 1 ? a.join('') : a.slice(0, -1).join(', ') + ' and ' + a[a.length - 1]);

    // ---- the project folder (only folders ThruLine has seen an agent work in) ----
    const folders = D.folders || [];
    // live: the person's choice, else the default the data names (never the home folder; C26), else nothing chosen
    const proj = isDemo ? (folders.find((f) => f.id === s.folder) || folders[0] || {}) : chosenOf(D, s);
    const chosen = !!proj.id;
    const cap = proj.capture || {};
    /* live: the capture route's answer for this folder (capture-view.js) says every step-3 word.
       A capture without a `state` is the older contract (installed + health), read as before. */
    const legacy = !isDemo && !!proj.capture && !proj.capture.state;
    const S3 = !isDemo && !legacy ? stepThree(proj, s, cap.now) : null;
    // in the gallery the DEMO STATE switcher sets the capture state; in the product the record does
    const inst = isDemo ? !!s.installed : S3 ? S3.installed : !!cap.installed;
    const health = isDemo ? s.health : S3 ? S3.phase : cap.health;
    const h = inst ? health : 'none';
    /* a folder nothing has recorded in has no stated path (data/project.js PATH_UNKNOWN): its row says so, and a
       sentence or a command that would carry the path says "this folder" / starts without a `cd` instead */
    const knownPath = isDemo || proj.pathKnown !== false;
    const wherePath = knownPath ? proj.path : 'this folder';
    const cdLine = knownPath ? 'cd ' + (isDemo ? proj.path : shellPath(proj.path)) : '';
    const inFolder = (cmd) => (cdLine ? cdLine + ' && ' + cmd : cmd);
    /* live (C30 R-ROUTES "Install"): what Install would add, is in, or can't add, read from the
       install route's plan for this folder (install-view.js); the click is the consent */
    const IV = isDemo ? null : installView(proj, { mcp: s.mcp, speaker: s.speaker, runcap: s.runcap }, cdLine);
    // what step 2 says was installed: in the product, only what the install plan read in the files
    const planInst = isDemo ? inst : IV.planInst;
    const first = proj.firstSession || {};
    const firstAt = first.startedAt || '';
    const hasFirstAsk = !!first.firstAsk;
    const sessionsKept = isDemo ? ((D.states && D.states[h] && D.states[h].storage) || D.storage || {}).sessions : (D.storage || {}).sessions;
    // A fixed, clearly labelled example teaches the proof chain before this
    // project has a claim. It must never be presented as this folder's record.
    const sampleChain = !D.example;
    const fx = D.example || DEMO.example;
    const fxSaid = fx.said || {}, fxRests = fx.restsOn || {}, fxRan = fx.ranOn || {}, fxSince = fx.since || {}, fxStops = fx.stops || {}, fxSo = fx.so || {};

    // ---- copy buttons: copying never runs anything ----
    const copy = (key, text) => () => {
      try {
        if (typeof navigator !== 'undefined' && navigator.clipboard && navigator.clipboard.writeText) {
          const p = navigator.clipboard.writeText(text);
          if (p && p.catch) p.catch(() => {});
        }
      } catch (e) { /* clipboard can be blocked; the Copied state still shows */ }
      this.setState({ copied: key });
    };
    const cp = (key, text) => ({ label: s.copied === key ? 'Copied' : 'Copy', aria: (s.copied === key ? 'Copied: ' : 'Copy: ') + text, note: show(s.copied === key), act: copy(key, text) });

    // ---- step rail + the lit thread between steps ----
    const done = [s.step > 1 || inst, inst, inst && health === 'recording'];
    const onShort = ['hooks'].concat(s.mcp ? ['MCP'] : [], s.speaker ? ['speakers'] : [], s.runcap ? ['run capture'] : []);
    const sub3 = !inst ? 'Runs after you install'
      : health === 'waiting' ? 'Waiting for the first step'
      : health === 'recording' ? 'Recording · since ' + firstAt
      : 'The hooks can’t run';
    const subs = [
      { text: proj.path, font: "'Geist Mono', monospace", color: T3 },
      { text: S3 ? S3.sub2 : inst ? (onShort.length > 2 ? 'Installed · hooks + ' + (onShort.length - 1) + ' more' : 'Installed · ' + onShort.join(' + ')) : 'Read it, choose, then install', font: 'inherit', color: T3 },
      { text: S3 ? S3.sub3 : sub3, font: 'inherit', color: h === 'recording' ? BLUE : T3 }
    ];
    const titles = ['Choose a project folder', 'What ThruLine will do', 'Check it’s recording'];
    const mkStep = (i) => {
      const n = i + 1;
      const active = s.step === n;
      const dn = done[i];
      const disc = active
        ? { bg: '#000000', bd: '2px solid ' + O, color: O, glyph: String(n), glow: '0 0 10px rgba(255,154,82,.45)' }
        : dn
          ? { bg: T1, bd: '2px solid ' + T1, color: '#000000', glyph: '✓', glow: 'none' }
          : { bg: '#000000', bd: '1px solid ' + CTRL, color: T2, glyph: String(n), glow: 'none' };
      return {
        sub: subs[i].text, subFont: subs[i].font, subColor: subs[i].color,
        titleColor: active || dn ? T1 : T2,
        bg: active ? '#16161A' : 'transparent',
        edge: active ? O : 'transparent',
        current: active ? 'step' : 'false',
        label: 'Step ' + n + ' of 3: ' + titles[i] + '. ' + (dn ? 'Done. ' : '') + subs[i].text,
        discBg: disc.bg, discBd: disc.bd, discColor: disc.color, glyph: disc.glyph, glow: disc.glow,
        pick: go({ step: n })
      };
    };
    const seg = (lit) => ({ bg: lit ? O : LINE, glow: lit ? GLOW : 'none' });
    const bar = (i) => (done[i] || s.step === i + 1 ? O : LINE);

    // ---- step 1: folder rows ----
    const fRow = (f) => {
      const key = f && f.id;
      const on = !!f && proj.id === key;
      return {
        on: on ? 'true' : 'false',
        bg: on ? '#16161A' : '#111113',
        bd: on ? O : HAIR,
        ring: on ? O : CTRL,
        dot: on ? O : 'transparent',
        // live (C27): a folder that already has records goes straight to its Inbox
        pick: !isDemo && f && f.recorded && this.props.actions && this.props.actions.open
          ? () => this.props.actions.open(key)
          : () => this.setState({ folder: key, installed: key === this.state.folder ? this.state.installed : false, copied: '' })
      };
    };

    // ---- step 2: consents (each optional part is its own yes) ----
    const chk = (key, onTag, offTag) => {
      const on = !!s[key];
      return {
        aria: on ? 'true' : 'false',
        box: on ? O : 'transparent',
        boxBd: on ? O : CTRL,
        mark: on ? '✓' : '',
        tag: on ? onTag : offTag,
        tagColor: on ? T1 : T2,
        tagBd: on ? CTRL : LINE,
        toggle: () => this.setState({ [key]: !this.state[key] })
      };
    };
    const c = {
      mcp: chk('mcp', 'Optional · on', 'Optional · off'),
      speaker: chk('speaker', 'On · you said yes', 'Off unless you say yes'),
      runcap: chk('runcap', 'Optional · on', 'Optional · off')
    };
    const spk = {
      rule: s.speaker ? O : LINE,
      glow: s.speaker ? GLOW : 'none',
      bd: s.speaker ? CTRL : HAIR,
      head: s.speaker ? 'Will add up to 3 lines like these when a session starts here' : 'Would add up to 3 lines like these, only if you say yes'
    };
    const onNames = ['Claude Code hooks'].concat(s.mcp ? ['MCP server'] : [], s.speaker ? ['speaker hooks'] : [], s.runcap ? ['run capture'] : []);
    const offNames = [].concat(s.mcp ? [] : ['MCP server'], s.speaker ? [] : ['speaker hooks'], s.runcap ? [] : ['run capture']);
    const sum = {
      on: (planInst ? 'Installed: ' : 'Adds ') + joinList(onNames) + ' · ' + wherePath + ' only',
      off: offNames.length ? 'Left out: ' + joinList(offNames) + ' · add later in Settings' : 'Nothing left out',
      installLabel: planInst ? 'Installed · go to step 3' : 'Install for this project',
      install: () => this.setState({ installed: true, step: 3 })
    };
    // live: "Install for this project" installs exactly the ticked parts (the click is the consent);
    // it is on only when something chosen would be added, and its title names each file it writes
    const actions = this.props.actions;
    const canInstall = !isDemo && !!actions && typeof actions.install === 'function';
    // the install route's answer: on to step 3 when nothing chosen was refused or failed
    const settled = (r) => !!r && r.ok && Object.keys(r.parts || {}).every((k) => r.parts[k].outcome !== 'refused' && r.parts[k].outcome !== 'failed');
    const installNow = (choices) => () => {
      Promise.resolve(actions.install(choices)).then((r) => { if (settled(r)) this.setState({ step: 3 }); }, () => {});
    };
    if (!isDemo) {
      const S = IV.sum;
      const off = S.mode === 'off' || (S.mode === 'install' && !canInstall);
      Object.assign(sum, {
        on: S.on,
        off: S.off,
        installLabel: S.installLabel,
        why: S.why,
        install: off ? () => {} : S.mode === 'step3' ? go({ step: 3 }) : installNow({ mcp: s.mcp, speaker: s.speaker, runcap: s.runcap }),
        disabled: off ? true : undefined,
        opacity: off ? '.5' : undefined
      });
    }
    // "Turn on" (run capture): with the hooks already in, it installs run capture alone; otherwise it ticks the box
    const turnOn = !isDemo && IV.turnOn === 'install' && canInstall
      ? () => { Promise.resolve(actions.install({ mcp: false, speaker: false, runcap: true })).then(() => {}, () => {}); }
      : () => this.setState({ runcap: true });

    // ---- capture health: every state is word + glyph + line style ----
    const K = {
      ok:     { color: T1, g: T1, bd: CTRL, style: 'solid', glyph: '✓' },
      wait:   { color: T2, g: T2, bd: T3, style: 'dotted', glyph: '⋯' },
      fog:    { color: T2, g: T3, bd: T3, style: 'dashed', glyph: '◌' },
      off:    { color: T3, g: T3, bd: LINE, style: 'solid', glyph: '–' },
      chosen: { color: T1, g: T1, bd: CTRL, style: 'dashed', glyph: '+' }
    };
    const row = (label, kind, word, detail, extra) => Object.assign({
      label: label, word: word, detail: detail,
      glyph: K[kind].glyph, color: K[kind].color, gColor: K[kind].g, bd: K[kind].bd, bdStyle: K[kind].style,
      actShow: 'none', actLabel: '', act: () => {}
    }, extra || {});
    const mkHealth = (isI, hs) => {
      const hh = isI ? hs : 'none';
      const pre = hh === 'none';
      const rows = [
        pre ? row('Claude Code hooks', 'fog', 'Not installed', 'Added in step 2, for ' + wherePath + ' only')
          : hh === 'problem' ? row('Claude Code hooks', 'fog', 'Can’t run', 'Installed, but they point at a ThruLine program that isn’t there any more')
          : row('Claude Code hooks', 'ok', 'Installed', 'For ' + wherePath + ' only · 10 events'),
        pre ? row('First step received', 'off', 'After install', 'Needs the hooks first')
          : hh === 'waiting' ? row('First step received', 'wait', 'Waiting', 'Start Claude Code in this folder; the first step shows up here when it is recorded')
          : hh === 'recording' ? row('First step received', 'ok', 'Received · ' + firstAt, 'Session started · ' + (first.agent || '') + ' (hook ' + (first.hookVersion || '') + ')')
          : row('First step received', 'fog', 'Nothing yet', 'No step has reached ThruLine from this folder'),
        proj.git === true ? row('Git', 'ok', 'Found', 'A Git repository: each version is fingerprinted, unsaved changes included')
          : proj.git === false ? row('Git', 'fog', 'Not found', 'No Git repository, so versions can’t be told apart; what rests on one reads as Fog')
          : row('Git', 'fog', 'Not checked yet', 'ThruLine hasn’t checked this folder for a Git repository yet'),
        s.mcp ? row('MCP server', pre ? 'chosen' : 'ok', pre ? 'Chosen' : 'Installed', 'The agent can check its context and hand in a “done” to verify')
          : row('MCP server', 'off', 'Off · optional', 'Left out in step 2; add it any time from Settings'),
        s.speaker ? row('Speaker hooks', pre ? 'chosen' : 'ok', pre ? 'Chosen' : 'Installed', 'Up to 3 lines when a session starts, each ending at a step')
          : row('Speaker hooks', 'off', 'Off', 'You didn’t say yes in step 2'),
        s.runcap ? row('Run capture', pre ? 'chosen' : 'wait', pre ? 'Chosen' : 'One line to add', 'Test runs recorded as runs, with exit code and version; see below')
          : row('Run capture', 'off', 'Off · optional', 'Test runs are seen only as text in the transcript', { actShow: 'inline-flex', actLabel: 'Turn on', act: () => this.setState({ runcap: true }) }),
        row('Stays on this machine', 'ok', '127.0.0.1 only', 'No network in the core; stores readable only by you')
      ];
      return rows.map((r, i) => Object.assign(r, { sep: i === 0 ? 'transparent' : HAIR }));
    };
    // live: the seven rows capture-view.js words from the route's answer, in the canvas's row style
    const capRows = () => S3.rows.map((x, i) => Object.assign(row(x[0], x[1], x[2], x[3], x[4] && x[4].turnOn
      ? { actShow: 'inline-flex', actLabel: 'Turn on', act: turnOn } : undefined), { sep: i === 0 ? 'transparent' : HAIR }));
    const healthRows = S3 ? capRows() : mkHealth(inst, health);
    /* the preview is the brief's four checks: hooks · first step · Git · run capture (the full list is step 3).
       Live and not installed, it is the canvas's "once installed" list, with this folder's real Git row. */
    const asIf = () => {
      const r = mkHealth(true, 'waiting');
      if (S3) r[2] = Object.assign({}, capRows()[2]);
      return r;
    };
    const preview = (inst ? (S3 ? capRows() : mkHealth(true, health)) : asIf())
      .filter((r, i) => i === 0 || i === 1 || i === 2 || i === 5)
      .map((r, i) => Object.assign({}, r, { sep: i === 0 ? 'transparent' : HAIR }));
    const pv = inst
      ? { head: 'Capture health, now', bd: 'solid ' + HAIR, foot: 'Read from the record when you open step 3, and again each time you press Look again.' }
      : { head: 'Once installed, this list reads', bd: 'dashed ' + LINE, foot: !S3 || S3.previewKnown ? 'A preview of step 3. Nothing here is true yet: nothing is installed.' : 'A preview of step 3. Whether this folder’s hooks are installed is not known yet.' };

    // ---- step 3: headline, state card, footer ----
    const H = S3 ? S3.head : {
      none:      { title: 'Nothing is installed in ' + proj.name + ' yet', sub: 'Step 2 lists everything ThruLine would add. This check runs once you install.', kind: 'fog', word: 'Not installed' },
      waiting:   { title: 'Installed. Start Claude Code in ' + proj.name + '.', sub: 'Nothing recorded in this project yet. Start Claude Code in this folder; the first step shows up here when it is recorded.', kind: 'wait', word: 'No step yet' },
      recording: { title: proj.name + ' is recording.', sub: 'The first step arrived at ' + firstAt + '. Every step ' + (first.agent || '') + ' takes in this folder is now recorded on this machine, and nothing leaves it.', kind: 'live', word: 'Recording' },
      problem:   { title: 'The hooks can’t reach ThruLine.', sub: 'They were installed, but they point at a ThruLine program that isn’t there any more, so nothing is being recorded. One command finds what to fix.', kind: 'fog', word: 'Not recording' }
    }[h];
    const live = H.kind === 'live';
    const hk = live ? { color: BLUE, g: BLUE, bd: 'rgba(82,168,255,.55)', style: 'solid', glyph: '' } : K[H.kind];
    const h3 = {
      title: H.title, sub: H.sub, word: H.word,
      glyph: hk.glyph, color: hk.color, gColor: hk.g, bd: hk.bd, bdStyle: hk.style,
      tint: live ? 'rgba(82,168,255,.08)' : 'transparent',
      // live: the KIT's pulsing blue recording dot (never the ● of Proven) and a trailing ✓
      dotShow: live ? 'inline-block' : 'none', glyphShow: live ? 'none' : 'inline', tickShow: live ? 'inline' : 'none'
    };
    const k = S3 ? {
      none: show(S3.cards.none), wait: show(S3.cards.wait), rec: show(S3.cards.rec), recGen: show(S3.cards.recGen), prob: show(S3.cards.prob)
    } : {
      none: show(h === 'none'),
      wait: show(h === 'waiting'),
      rec: show(h === 'recording' && hasFirstAsk),
      recGen: show(h === 'recording' && !hasFirstAsk),
      prob: show(h === 'problem')
    };
    // live: Look again reads the record again (index.jsx); it never moves the state by itself
    const lookAgain = () => { const a = this.props.actions; if (a && typeof a.lookAgain === 'function') a.lookAgain(); };
    const F = S3 ? Object.assign({}, S3.footer, { act: S3.footer.act === 'lookAgain' ? lookAgain : S3.footer.act === 'step2' ? go({ step: 2 }) : () => {} }) : {
      none:      { btn: true, label: 'Go to step 2', act: go({ step: 2 }), link: false, anyway: false, note: 'Nothing is installed until you press Install for this project.' },
      waiting:   { btn: true, label: 'Look again', act: isDemo ? go({ health: 'recording' }) : lookAgain, link: false, anyway: true, note: 'The Inbox stays empty until the first step is recorded.' },
      recording: { btn: false, label: '', act: () => {}, link: true, anyway: false, note: 'It opens on what needs a look first.' },
      problem:   { btn: true, label: 'Look again', act: isDemo ? go({ health: 'recording' }) : lookAgain, link: false, anyway: false, note: 'Run the command above first, then look again.' }
    }[h];
    // live: both Inbox links open the chosen folder's Inbox (the gallery keeps the canvas's link)
    const inboxHref = !isDemo && proj.id ? href('inbox', { project: proj.id }) : undefined;
    const f3 = { btnShow: showI(F.btn), label: F.label, act: F.act, linkShow: showI(F.link), anywayShow: showI(F.anyway), note: F.note, inboxHref: inboxHref };

    const ch = IV ? {
      // live: each choice beside what the install plan read for it
      head: planInst ? 'What you said yes to' : 'What you’ve chosen so far',
      scope: 'for ' + wherePath + ' only',
      mcp: IV.ch.mcp.word, mcpC: IV.ch.mcp.on ? T1 : T3,
      speaker: IV.ch.speaker.word, speakerC: IV.ch.speaker.on ? T1 : T3,
      runcap: IV.ch.runcap.word, runcapC: IV.ch.runcap.on ? T1 : T3
    } : {
      head: planInst ? 'What you said yes to' : 'What you’ve chosen so far',
      scope: 'for ' + wherePath + ' only',
      mcp: s.mcp ? 'Yes' : 'No · left out', mcpC: s.mcp ? T1 : T3,
      speaker: s.speaker ? 'Yes · you said yes' : 'No · off', speakerC: s.speaker ? T1 : T3,
      runcap: s.runcap ? 'Yes' : 'No · optional', runcapC: s.runcap ? T1 : T3
    };

    // ---- design-review switcher (not product) ----
    const dm = (key) => {
      const on = inst && s.step === 3 && s.health === key;
      return { bg: on ? LINE : 'transparent', color: on ? T1 : T2, pressed: on ? 'true' : 'false', pick: go({ step: 3, installed: true, health: key }) };
    };
    const fresh = !inst && s.step === 2;
    const dmReset = {
      bg: fresh ? LINE : 'transparent', color: fresh ? T1 : T2, pressed: fresh ? 'true' : 'false',
      pick: go({ step: 2, folder: 'checkout', mcp: true, speaker: false, runcap: false, installed: false, health: 'waiting', copied: '' })
    };

    // ---- the product only: read every session of the chosen folder (design addition: load progress) ----
    const fmtN = (n) => (typeof n === 'number' && Number.isFinite(n) ? fmtCount(n) : null);
    const L = D.load || {};
    const mine = !!L.project && L.project === proj.id;
    const running = !!L.running && mine;
    const otherRunning = !!L.running && !mine;
    const finished = !!L.finished && mine;
    const fd = proj.found || {};
    const foundParts = [
      [fd.sessions, 'session', 'sessions'], [fd.prompts, 'prompt', 'prompts'], [fd.edits, 'edit', 'edits'],
      [fd.commands, 'command', 'commands'], [fd.tests, 'test run', 'test runs'], [fd.claims, 'claim', 'claims'], [fd.threads, 'thread', 'threads']
    ].filter((x) => typeof x[0] === 'number').map((x) => fmtN(x[0]) + ' ' + (x[0] === 1 ? x[1] : x[2]));
    const loadView = {
      show: true,
      line: running
        ? 'Reading ' + (fmtN(L.filesTotal) ?? '…') + ' recordings — ' + (fmtN(L.filesDone) ?? '0') + ' done · ' + (fmtN(L.eventsAdded) ?? '0') + ' rows'
        : otherRunning ? 'Another folder is being read. This one can be read when it finishes.'
        : finished ? 'Read ' + (fmtN(L.filesTotal) ?? '…') + ' recordings · ' + (fmtN(L.eventsAdded) ?? '0') + ' new rows' + (L.errors ? ' · ' + fmtN(L.errors) + ' could not be read whole' : '')
        : foundParts.length ? 'Recorded so far for ' + proj.name + ':' : 'Nothing from ' + proj.name + ' is recorded yet.',
      // a quick row's counts are null until its index is built: said, never drawn as 0
      found: foundParts.concat(proj.complete === false ? [foundParts.length ? 'the rest not yet counted' : 'not yet counted'] : []).join(' · '),
      problem: D.loadError || ''
    };
    const act = this.props.actions && this.props.actions.load;
    /* C27: never make the Inbox wait. A folder with records opens its Inbox; for one without,
       the load starts and the Inbox opens at once, its status bar following the load. */
    const open = this.props.actions && this.props.actions.open;
    const opens = chosen && !!proj.recorded && !!open;
    const off = !opens && (running || otherRunning || !act || !chosen);
    const primary = {
      label: opens ? 'Open its Inbox' : running ? 'Reading every session…' : !chosen ? 'Choose a folder to load' : 'Load every session',
      act: opens ? () => open(proj.id) : off ? () => {} : () => act(proj.id),
      disabled: off ? true : undefined,
      opacity: off ? '.6' : undefined,
      nowrap: 'nowrap',
      note: opens ? 'ThruLine already has this folder’s sessions. Its Inbox opens at once.'
        : !chosen && !running ? 'Choose a folder above first. Nothing is read until you do.'
        : running ? 'Reading every Claude Code and Codex session for this folder. Open the Inbox any time; it fills in as they are read.'
        : 'Reads every Claude Code and Codex session recorded for this folder, on this machine. Nothing leaves it. The Inbox opens at once and fills in as they are read.'
    };

    const ask = first.firstAsk || {};
    const f0 = folders[0], f1 = folders[1];
    // a sentence from facts: empty when any fact it needs is missing, never "undefined"
    const T = (strings, ...vals) => (vals.some((x) => x === undefined || x === null) ? '' : strings.reduce((a, str, i) => a + str + (i < vals.length ? vals[i] : ''), ''));
    // live, a long path is cut in its middle and a long name at its end, each whole in title=
    const liveRows = !!(this.props && this.props.data);
    const folderText = (f) => (f ? {
      name: f.name, path: liveRows ? middle(f.path, 52) : f.path, agent: f.agent,
      full: liveRows ? f.path : undefined, fullName: liveRows ? f.name : undefined,
      // live: the capture route's git answer once this folder was read (capture-view.js withCapture); never a guess
      git: f.git === true ? 'Git repository' : f.git === false ? 'No Git repository' : f.gitReason ? 'Git didn’t answer' : 'Git not checked yet',
      recorded: f.recorded ? 'Recorded' : 'Not recorded yet'
    } : {});
    const t = {
      home: D.home,
      foldersHead: T`Folders where an agent has worked · ${fmtCount(folders.length)}`,
      f0: folderText(f0), f1: folderText(f1),
      speaker: (D.speakerExample && D.speakerExample.length ? D.speakerExample : DEMO.speakerExample)
        .map((l) => ({ text: l.text, step: T`step ${l.step}` })),
      // live: the hooks' first delivery, and the first ask with the thread it opened (capture-view.js)
      started: S3 ? S3.started : T`Session started ${first.startedAt} · ${first.agent} (hook ${first.hookVersion})`,
      askStep: S3 ? (S3.ask ? S3.ask.step : '') : T`· step ${ask.step}`,
      askText: S3 ? (S3.ask ? S3.ask.text : '') : T`“${ask.text}”`,
      askChange: S3 ? (S3.ask ? S3.ask.change : '') : ask.proposedChange,
      ex: {
        present: !!fx,
        sample: !isDemo && sampleChain,
        chainAria: T`Example: the proof chain for ${fxSaid.quote == null ? null : String(fxSaid.quote).replace(/[.]$/, '')}`,
        saidStep: T`step ${fxSaid.step}`,
        saidBy: T`${fxSaid.agent} said `,
        quote: fxSaid.quote,
        saidAt: fxSaid.at,
        restsStep: T`step ${fxRests.step}`,
        restsCommand: fxRests.command,
        restsResult: T`${fmtN(fxRests.passed)} passed, ${fmtN(fxRests.failed)} failed · exit ${fxRests.exit}`,
        ranStep: T`step ${fxRan.step}`,
        ranVersion: T`version ${fxRan.version}`,
        ranOlder: T`· ${fmtN(fxRan.versionsOlder)} version${fxRan.versionsOlder === 1 ? '' : 's'} older`,
        sinceStep: T`step ${fxSince.step}`,
        sinceFile: fxSince.file,
        sinceAt: fxSince.at,
        stopsVersion: fxStops.version,
        stopsFile: fxStops.file,
        soVersion: T`· version ${fxSo.version}`,
        first: T`Check it again, with ${fxStops.file} in scope.`,
        why: T`Each link above ends at its step: ${[fxSaid.step, fxRests.step, fxRan.step, fxSince.step].some((x) => x == null) ? null : [fxSaid.step, fxRests.step, fxRan.step, fxSince.step].join(', ')}.`
      }
    };

    return {
      isDemo: isDemo,
      t: t,
      v: {
        stepOf: 'Step ' + s.step + ' of 3',
        bar1: bar(0), bar2: bar(1), bar3: bar(2),
        st1: mkStep(0), st2: mkStep(1), st3: mkStep(2),
        seg1: seg(done[0]), seg2: seg(done[1]),
        // live: the one sentence data/project.js sessionsKeptSentence makes, the number Settings states too (C29)
        storage: !isDemo && D.storage && D.storage.line ? D.storage.line : sessionsKept ? 'ThruLine is keeping the record of ' + fmtCount(sessionsKept) + ' session' + (sessionsKept === 1 ? '' : 's') + ' on this machine.' : 'ThruLine is keeping nothing on this machine yet.',
        storageLink: sessionsKept ? 'See what, and remove it' : 'Where records will go',
        d1: show(s.step === 1), d2: show(s.step === 2), d3: show(s.step === 3),
        proj: { name: proj.name, path: proj.path },
        cdLine: cdLine || '# in your project folder',
        folders: folders.map((f) => Object.assign(folderText(f), { id: f.id, gitOk: f.git === true, ui: fRow(f) })),
        continueLabel: isDemo ? 'Continue with ' + proj.name : primary.label,
        primary: isDemo ? null : primary,
        load: isDemo ? null : loadView,
        toStep1: go({ step: 1 }), toStep2: go({ step: 2 }), toStep3: go({ step: 3 }),
        t2: 'Everything ThruLine will add to ' + proj.name,
        onCount: fmtCount(onNames.length - 1) + ' of 3 optional chosen',
        c: c, spk: spk, sum: sum,
        health: healthRows, preview: preview, pv: pv,
        h3: h3, k: k, f3: f3,
        // "Review what counts as done →": the first ask's thread, live; no address at all without one
        doneHref: S3 ? S3.doneHref : undefined,
        /* the run-capture steps: in the gallery, the canvas's; live, once the install plan reads the shim in
           ThruLine's home and HQ's search path does not hold it yet, the one line still to add (rows 10, 11) */
        shimShow: show(IV && IV.facts ? IV.shim.show : !S3 && inst && !!s.runcap),
        shim: IV && IV.facts
          ? { cmd: IV.shim.cmd, step2: IV.shim.step2, shape: IV.shim.shape }
          : { cmd: 'thruline shim install', step2: 'Add the one line it prints to your shell profile, then restart the terminal Claude Code runs in. ThruLine shows that line here word for word, from its output.', shape: 'its shape: export PATH="<ThruLine home>/shim:$PATH"' },
        // "Changed your mind?": live, the terminal command that removes exactly what the install wrote
        undo: IV
          ? { cmd: IV.undo.cmd, text: IV.undo.text }
          : { cmd: 'thruline sensor uninstall', text: 'This removes the hooks from this folder. What was already recorded stays until you remove it in Settings, which says what would be lost first.' },
        // "Same install, from a terminal": live, the command that writes the same two files in this folder
        initCmd: IV ? IV.initCmd : 'thruline init',
        planCmd: IV ? IV.planCmd : 'thruline sensor plan',
        ch: ch,
        doctorCmd: S3 ? 'thruline doctor' : undefined,
        cp: {
          add: cp('add', 'thruline init'),
          demo: cp('demo', 'thruline demo'),
          init: cp('init', inFolder(IV ? IV.initCmd : 'thruline init')),
          start: cp('start', inFolder('claude')),
          // live: `thruline doctor` reads every settings level the capture route reads; `sensor doctor` reads one file
          doctor: cp('doctor', S3 ? 'thruline doctor' : 'thruline sensor doctor'),
          shim: cp('shim', IV && IV.facts ? IV.shim.copy : 'thruline shim install'),
          undo: cp('undo', IV ? IV.undo.cmd : 'thruline sensor uninstall')
        },
        dm: { reset: dmReset, waiting: dm('waiting'), recording: dm('recording'), problem: dm('problem') }
      }
    };
  }
}
