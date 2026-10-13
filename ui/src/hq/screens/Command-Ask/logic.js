/* Command-Ask.dc.html: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { liveRecRow, liveEv, livePal, liveTop, decisionFindRows, searchFind } from './live.js'
import { href as routeHref } from '../../routes.js'
import { fmtCount } from '../../data/count.js'
import { keyOf, singleAllowed } from '../../keys/dispatch.js'

// the gallery's prototype links, as the live address's screen
const SCREEN_OF = { 'Change-Brief.dc.html': 'brief', 'What-Changed.dc.html': 'changed', 'Every-Statement.dc.html': 'statements' };

export const PROPS = {"$preview":{"width":1512,"height":920}}

export default class Component extends DCLogic {
  state = { query: '', asked: null, askedText: '', active: 'act-check', tab: 'answer', pinned: null, copied: null };

  componentDidMount() {
    this.__paletteKey = (e) => {
      if (!this.props.overlay || e.defaultPrevented || e.target?.closest?.('#tl-cmdk-input')) return;
      this.renderVals().onPaletteKey(e);
    };
    document.addEventListener('keydown', this.__paletteKey);
  }

  componentWillUnmount() {
    document.removeEventListener('keydown', this.__paletteKey);
  }

  renderVals() {
    const s = this.state;
    const set = (p) => this.setState(p);
    // content arrives only through D: the record in the product, the canvas's demo in the gallery
    const isDemo = !this.props.data;
    const D = this.props.data || DEMO;
    // overlay: the palette over whatever screen is showing (app.jsx, ⌘K); onClose closes it
    const overlay = !!this.props.overlay;
    const onClose = typeof this.props.onClose === 'function' ? this.props.onClose : null;
    const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', VIOLET = '#B78CFF';
    const MONO = "12.5px 'Geist Mono', monospace";
    const SANS = "13.5px 'Geist', -apple-system, sans-serif";

    // ---------- the typed text ----------
    const STOP = ['what', 'which', 'when', 'where', 'were', 'there', 'that', 'this', 'with', 'from', 'have', 'does', 'about', 'should', 'would', 'could', 'anything', 'something', 'after', 'before', 'into', 'your', 'they', 'them', 'then', 'than', 'been', 'being', 'will', 'just', 'some', 'ever'];
    const kw = (t) => String(t || '').toLowerCase().replace(/[^a-z0-9/._-]+/g, ' ').split(' ').filter((w) => w.length >= 4 && STOP.indexOf(w) < 0);
    const query = String(s.query == null ? '' : s.query);
    const qt = query.trim();
    const ql = qt.toLowerCase();
    const words = kw(ql);
    const hit = (hay) => {
      if (!ql) return true;
      const h = String(hay).toLowerCase();
      return h.indexOf(ql) >= 0 || words.some((w) => h.indexOf(w) >= 0);
    };
    const QRE = /^(what|which|when|where|who|why|how|did|does|do|is|was|were|has|have|can|could|should|will|are)\b/;
    const isQuestion = !!ql && (/\?$/.test(ql) || QRE.test(ql));
    const quoted = (ws, fb) => (ws.length ? '“' + ws.join('” or “') + '”' : '“' + fb + '”');

    // ---------- the record, as Find sees it (demo record, statement spec §4) ----------
    const STATE = { not_proven: { glyph: '○', word: 'Not proven', gcol: T2 } };
    const stepsText = (ns) => (ns.length === 1 ? 'step ' + ns[0] : 'steps ' + ns.join(' and '));
    const versionsOlder = (n) => fmtCount(n) + (n === 1 ? ' version older' : ' versions older');
    const runCounts = (r) => (r.failed > 0 ? fmtCount(r.failed) + ' failed, ' + fmtCount(r.passed) + ' passed' : fmtCount(r.passed) + ' passed, ' + fmtCount(r.failed) + ' failed');
    const recRow = (r) => {
      const step = r.step != null ? 'step ' + r.step : '';
      switch (r.kind) {
        case 'Request':
          return { kind: r.kind, qo: '“', text: r.text, qc: '”', qcol: T1, mono: false, meta: (r.by === 'you' ? 'your request' : r.by) + ' · ' + r.at, step: step, href: 'Change-Brief.dc.html' };
        case 'Read':
          return { kind: r.kind, text: r.file, mono: true, meta: 'read by ' + r.agent, step: step, href: 'Change-Brief.dc.html' };
        case 'Edit':
          return r.afterLastCheck
            ? { kind: r.kind, text: r.op + ' ' + r.file, mono: true, glyph: '◭ ', gcol: '#F2C94C', meta: 'changed after the last check · ' + r.at, step: step, href: 'What-Changed.dc.html' }
            : { kind: r.kind, text: r.op + ' ' + r.file, mono: true, meta: 'then version ' + r.versionAfter, step: step, href: 'What-Changed.dc.html' };
        case 'Run':
          return { kind: r.kind, text: r.command, mono: true, meta: runCounts(r) + ' · ' + r.version + ' · ' + (r.exit ? 'exit ' + r.exit : versionsOlder(r.versionsBehind)), step: step, href: 'Change-Brief.dc.html' };
        case 'Said': {
          const st = r.statement && STATE[r.statement.state];
          return st
            ? { kind: r.kind, qo: '❝', text: r.text, qc: '❞', qcol: VIOLET, mono: false, glyph: st.glyph + ' ', gcol: st.gcol, meta: st.word + ' · version ' + r.statement.version + ' · ' + r.at, step: step, href: 'Every-Statement.dc.html' }
            : { kind: r.kind, qo: '❝', text: r.text, qc: '❞', qcol: VIOLET, mono: false, meta: r.agent, step: step, href: 'Change-Brief.dc.html' };
        }
        case 'Version':
          return { kind: r.kind, text: r.version, mono: true, meta: (r.current ? 'current · ' : '') + '= commit ' + r.commit, step: step, href: 'Change-Brief.dc.html' };
        case 'Command':
          return r.result == null
            ? { kind: r.kind, text: r.command, mono: true, glyph: '⋯ ', gcol: T2, meta: 'no result yet' + (r.removes ? ' · removes ' + r.removes : ''), step: step, href: 'Change-Brief.dc.html' }
            : { kind: r.kind, text: r.command, mono: true, meta: r.result + (r.removes ? ' · removes ' + r.removes : ''), step: step, href: 'Change-Brief.dc.html' };
        case 'File':
          return { kind: r.kind, text: r.path, mono: true, meta: (r.changedAt.length ? 'changed at ' + stepsText(r.changedAt) + (r.uncovered ? ' · no check covers it' : '') : 'run at ' + stepsText(r.runAt)), step: r.lastVersion || '', href: 'What-Changed.dc.html' };
        default:
          return { kind: r.kind, text: r.text || '', mono: false, meta: '', step: step, href: 'Change-Brief.dc.html' };
      }
    };
    // on the record: the live row, and its address in the app
    const at0 = this.props.at || {};
    const whenOf = (ts) => { const d = new Date(ts); return Number.isNaN(d.getTime()) ? '' : String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0'); };
    const liveRow = (r) => {
      const x = liveRecRow(r, { when: whenOf });
      const change = x.change || D.change.threadId || D.change.id || null;
      // a row with no Change behind it is no link (C30 R-LINKS: never the Inbox in its place)
      x.href = change ? routeHref(x.screen || 'brief', { change, project: at0.project }) : undefined;
      return x;
    };
    /* C29: the project's decisions, findable by their words, kind and state; each links to
       where it is shown: the open Change's Brief, or the Inbox for a broken one (the only
       decisions the Inbox draws), else no link (C30 R-LINKS). None in the gallery. */
    const dm = isDemo ? null : this.props.decisions;
    const pid = at0.project || this.props.projectId || undefined;
    const decHref = (r) => (at0.change ? routeHref('brief', { change: at0.change, project: pid })
      : dm && dm.broken.some((b) => b.id === r.id) ? routeHref('inbox', { project: pid }) : undefined);
    const DEC = decisionFindRows(dm, decHref);
    const REC = DEC.concat(D.findable.map((r) => Object.assign({ id: r.id }, isDemo ? recRow(r) : liveRow(r))))      .map((r) => Object.assign({}, r, { search: (r.kind + ' ' + r.text + ' ' + r.meta + ' ' + r.step + (r.hay ? ' ' + r.hay : '')).toLowerCase() }));
    const recById = {};
    REC.forEach((r) => { recById[r.id] = r; });
    /* C30 R-ROUTES "Record search": the rest of the record, as the search route
       found it for exactly this text (data/search.js); each row links to its
       own Change. None in the gallery. */
    const SR = isDemo ? searchFind(null, '', null) : searchFind(this.props.found, qt, { when: whenOf });
    SR.rows.forEach((r) => { r.href = r.change ? routeHref(r.screen || 'brief', { change: r.change, project: pid }) : undefined; });

    // ---------- the five questions the record can answer (prototype ASK.chips) ----------
    const QS = D.questions.slice();
    const nQ = QS.length;
    const V = D.version.id;
    const reset = { tab: 'answer', pinned: null, copied: null };
    const ask = (key) => set(Object.assign({ asked: key, askedText: '', active: 'ask-' + key }, reset));
    const askTyped = () => { if (!qt) return; set(Object.assign({ asked: 'typed', askedText: qt, active: 'ask-typed' }, reset)); };

    // ---------- which rows show ----------
    /* live (C30 R-RUN): where the press opens the check sheet already running (the press is the consent) */
    const CHK = (!isDemo && this.props.actions) || {};
    const ACT = [
      { id: 'act-check', key: 'check', hay: 'check it again re-run ' + D.check.command + ' ' + V, enter: !isDemo && CHK.runs ? 'opens the check and runs it on ' + V : 'opens the check · it asks before running' },
      { id: 'act-tell', key: 'tell', hay: 'tell the agent message copy prompt', enter: 'opens the message · copying doesn’t send it' },
      { id: 'act-approve', key: 'approve', hay: 'approve ' + V + ' decision sign-off', enter: 'nothing yet · approval waits on ' + fmtCount(D.approval.waitsOn) + ' things' },
      { id: 'act-define', key: 'define', hay: 'say what counts as done criteria define accept', enter: 'opens what counts as done' },
    ];
    const GO = [
      { id: 'go-statements', key: 'statements', hay: 'every statement statements claims', enter: 'goes to Every statement' },
      { id: 'go-changed', key: 'changed', hay: 'what changed diff files edits', enter: 'goes to What changed' },
      { id: 'go-story', key: 'story', hay: 'the whole story review take away export', enter: 'goes to The whole story' },
      { id: 'go-inbox', key: 'inbox', hay: 'inbox changes', enter: 'goes to the Inbox' },
    ];
    const vis = {};
    ACT.concat(GO).forEach((r) => { vis[r.key] = hit(r.hay); });
    // with no Change open (the project's palette), only what needs no Change is offered
    if (!isDemo && !D.open) ['check', 'tell', 'approve', 'define', 'statements', 'changed', 'story'].forEach((k) => { vis[k] = false; });
    const actShow = ACT.some((r) => vis[r.key]);
    const goShow = GO.some((r) => vis[r.key]);

    const localFound = ql ? REC.filter((r) => hit(r.search)) : D.nearProofStop.map((id) => recById[id]).filter(Boolean);
    let findList = ql ? SR.rows.concat(localFound) : localFound;
    const findMoreN = Math.max(0, localFound.length + (ql ? SR.total : 0) - Math.min(4, findList.length));
    findList = findList.slice(0, 4);
    // nothing found is said only once the whole record has answered (or could not)
    const findEmpty = !!ql && findList.length === 0 && SR.state !== 'reading';
    const findFailed = !!ql && SR.state === 'failed';
    const findEarly = !isQuestion;
    const findLate = isQuestion;

    const askList = ql ? QS.filter((x) => hit(x.text)) : QS.slice();
    const typedShow = !!ql && askList.length === 0;
    const otherList = isQuestion ? QS.filter((x) => askList.indexOf(x) < 0) : [];
    const askShow = askList.length > 0 || typedShow;

    // ---------- geometry: every row is 32px, every section head 28px, so the thread knows where to start ----------
    const LIST_Y0 = 132, HEAD = 28, ROW = 32, INPUT_Y = 93, END_Y = 101;
    let y = LIST_Y0;
    const posY = {}, order = [], hints = {};
    const head = () => { y += HEAD; };
    const row = (id, hint) => { posY[id] = y + ROW / 2; y += ROW; order.push(id); hints[id] = hint; };
    const note = () => { y += ROW; };
    const findBlock = () => {
      head();
      findList.forEach((r) => row('find-' + r.id, !r.href ? 'no screen shows it' : r.change && r.change !== (D.change.threadId || D.change.id) ? 'opens it on its Change' : 'opens it on the Change'));
      if (findEmpty) note();
      if (findMoreN || findFailed) note();
    };
    if (actShow) { head(); ACT.forEach((r) => { if (vis[r.key]) row(r.id, r.enter); }); }
    if (goShow) { head(); GO.forEach((r) => { if (vis[r.key]) row(r.id, r.enter); }); }
    if (findEarly) findBlock();
    if (askShow) { head(); askList.forEach((x) => row('ask-' + x.key, 'asks · the answer comes from the record')); if (typedShow) row('ask-typed', 'asks the record, as typed'); }
    if (findLate) findBlock();
    if (otherList.length) { head(); otherList.forEach((x) => row('ask-' + x.key, 'asks · the answer comes from the record')); }

    // a typed question lands on the question itself, so ↵ asks it
    const askDefault = askList.length ? 'ask-' + askList[0].key : (typedShow ? 'ask-typed' : '');
    const askFirst = this.props.askFirst && !qt && !s.asked && s.active === 'act-check' && order.includes(askDefault);
    const activeId = askFirst ? askDefault : order.indexOf(s.active) >= 0 ? s.active : ((isQuestion && askDefault) ? askDefault : (order[0] || ''));
    const askedRow = s.asked === 'typed' ? 'ask-typed' : (s.asked ? 'ask-' + s.asked : '');
    const rs = (id) => {
      const isAsked = !!askedRow && id === askedRow;
      const isAct = id === activeId;
      return {
        domId: 'tl-opt-' + id,
        sel: String(isAct),
        bg: isAsked ? 'rgba(255,122,41,.10)' : (isAct ? '#1F1F23' : 'transparent'),
        edge: isAsked ? 'inset 2px 0 0 #FF7A29' : 'none',
        cls: (isAsked || isAct) ? '' : 'tl-opt',
      };
    };

    const o = {};
    ACT.concat(GO).forEach((r) => { o[r.key] = Object.assign(rs(r.id), { show: vis[r.key] }); });
    o.approve.pick = () => set({ active: 'act-approve' });
    if (CHK.checkHref) o.check.href = CHK.checkHref;

    const findRows = findList.map((r) => Object.assign(rs('find-' + r.id), {
      kind: r.kind, qo: r.qo || '', qc: r.qc || '', qcol: r.qcol || T1, text: r.text, title: r.title,
      fontCss: r.mono ? MONO : SANS, glyph: r.glyph || '', gcol: r.gcol || T3, meta: r.meta, step: r.step, href: r.href,
    }));

    const mkAsk = (x) => {
      const asked = s.asked === x.key;
      return Object.assign(rs('ask-' + x.key), {
        text: x.text,
        meta: asked ? 'asked ' + D.askedAt + ' · the answer is on the right' : x.reads,
        metaColor: asked ? T2 : T3,
        tail: (!asked && activeId === 'ask-' + x.key) ? '↵ asks' : '',
        pick: () => ask(x.key),
      });
    };
    const askRows = askList.map(mkAsk);
    const otherRows = otherList.map(mkAsk);
    const typedAsked = s.asked === 'typed' && s.askedText === qt;
    const typed = Object.assign(rs('ask-typed'), {
      show: typedShow,
      label: 'Ask “' + qt + '”',
      meta: typedAsked ? 'asked ' + D.askedAt + ' · the answer is on the right' : nQ === 0 ? 'the record is searched for it' : 'none of the ' + fmtCount(nQ) + ' questions matches · the record is searched for it',
      metaColor: typedAsked ? T2 : T3,
      tail: (!typedAsked && activeId === 'ask-typed') ? '↵ asks' : '',
      pick: askTyped,
    });

    const sec = {
      act: actShow, go: goShow, findEarly: findEarly, findLate: findLate, ask: askShow, other: otherList.length > 0,
      findHint: !isDemo && D.loading ? 'Reading the record…'
        : !isDemo && D.failed ? 'ThruLine did not answer, so there is nothing to search yet. Reload to try again.'
        : !isDemo && !D.open ? (ql ? 'in the ' + fmtCount(D.record.steps) + ' Changes of ' + (D.project.name || 'this project') + (SR.state === 'reading' ? ' · searching their records…' : '') : 'the latest Changes · type to search all ' + fmtCount(D.record.steps) + ' and what they hold')
        : ql ? 'in the ' + fmtCount(D.record.steps) + ' recorded steps' + (isDemo ? '' : SR.state === 'reading' ? ' · searching the rest of ' + (D.project.name || 'this project') + '…' : SR.state === 'served' ? ' and the rest of ' + (D.project.name || 'this project') : '') : 'near where the proof stops · type to search the ' + fmtCount(D.record.steps) + ' recorded steps',
      askHint: nQ === 0 ? 'asks the record, as typed' : ql ? (askList.length ? fmtCount(askList.length) + ' of the ' + fmtCount(nQ) + ' questions ' + (askList.length === 1 ? 'matches' : 'match') : 'none of the ' + fmtCount(nQ) + ' questions matches') : 'the ' + fmtCount(nQ) + ' questions the record can answer',
      otherTitle: askList.length ? 'Other questions the record can answer' : 'Questions the record can answer',
      findEmptyShow: findEmpty,
      findEmptyText: findFailed
        ? 'Nothing in this Change’s steps, files or commands contains ' + quoted(words, qt) + '.'
        : 'Nothing in the record’s steps, files or commands contains ' + quoted(words, qt) + '.',
      findMoreShow: findMoreN > 0 || findFailed,
      // a failed search of the rest of the record is said, never left to read as "nothing more"
      findMoreText: findFailed ? SR.message : fmtCount(findMoreN) + ' more in the record · keep typing to narrow it',
    };

    // ---------- the answer (built from record fields only; prototype ASK, refined vocabulary) ----------
    const evOf = (e) => {
      const step = 'step ' + e.step + ' ›';
      switch (e.kind) {
        case 'Said':
          return { kind: e.kind, qo: e.quote ? '❝' : '', text: e.text, qc: e.quote ? '❞' : '', mono: false, result: '', badge: '', badgeKind: '', note: e.agent + ' · ' + e.at + ' · the agent’s own words', step: step, detail: e.detail };
        case 'Run':
          return { kind: e.kind, text: e.text, mono: true, result: runCounts(e), badge: e.version + (e.versionsBehind ? ' · ' + versionsOlder(e.versionsBehind) : ''), badgeKind: e.versionsBehind ? 'older' : 'current', note: '', step: step, detail: e.detail };
        case 'Edit':
          return { kind: e.kind, text: e.text, mono: true, result: '', badgeGlyph: e.afterRun ? '◭' : '', badge: e.afterRun ? 'after the run' : '', badgeKind: e.afterRun ? 'after' : '', note: e.at + ' · then version ' + e.versionAfter, step: step, detail: e.detail };
        case 'Version':
          return { kind: e.kind, text: e.version + ' · = commit ' + e.commit, mono: true, result: '', badge: e.current ? 'current' : '', badgeKind: 'current', note: (e.unsavedChanges ? 'unsaved changes' : 'no unsaved changes') + ' · ' + e.at, step: step, detail: e.detail };
        case 'Command':
          return { kind: e.kind, text: e.text, mono: true, result: e.result == null ? '' : e.result, badgeGlyph: e.result == null ? '⋯' : '', badge: e.result == null ? 'no result yet' : '', badgeKind: 'noresult', note: (e.removes ? 'removes ' + e.removes + ' · ' : '') + 'recorded, not judged', step: step, detail: e.detail };
        default:
          return { kind: e.kind, text: e.text || '', mono: false, result: '', badge: '', badgeKind: '', note: '', step: step, detail: e.detail || '' };
      }
    };
    const EV = {};
    Object.keys(D.evidence).forEach((k) => { EV[k] = isDemo ? evOf(D.evidence[k]) : liveEv(D.evidence[k]); });
    const BADGE = {
      older: { b: '1px solid rgba(242,201,76,.6)', c: '#F2C94C', bg: 'transparent' },
      after: { b: '1px solid rgba(242,201,76,.6)', c: '#F2C94C', bg: 'repeating-linear-gradient(45deg, rgba(242,201,76,.14) 0 2px, transparent 2px 6px)' },
      current: { b: '1px solid #66666F', c: T1, bg: 'transparent' },
      noresult: { b: '1px dotted #A1A1AA', c: T2, bg: 'transparent' },
    };
    const PILL = {
      notProven: { border: '1px dashed #82828C', color: T1, bg: 'transparent', glyphColor: T2, metaColor: T2 },
      changed: { border: '1px solid rgba(242,201,76,.6)', color: '#F2C94C', bg: 'repeating-linear-gradient(45deg, rgba(242,201,76,.16) 0 2px, transparent 2px 6px)', glyphColor: '#F2C94C', metaColor: '#F2C94C' },
      noResult: { border: '1px dotted #A1A1AA', color: T1, bg: 'transparent', glyphColor: T2, metaColor: T2 },
      prepared: { border: '1px solid #66666F', color: T1, bg: 'transparent', glyphColor: T2, metaColor: T2 },
      fog: { border: '1px dashed #82828C', color: T1, bg: 'transparent', glyphColor: T3, metaColor: T2 },
      neutral: { border: '1px solid #66666F', color: T1, bg: 'transparent', glyphColor: T2, metaColor: T2 },
      proven: { border: '1px solid #66666F', color: T1, bg: 'transparent', glyphColor: 'var(--tl-proven, #3DD68C)', metaColor: T2 },
    };
    const L = (x) => Object.assign({ pre: '', qo: '', qt: '', qc: '', code: '', post: '', r: '' }, x);
    const R = (t) => (t ? '\u2002' + t : ''); // an en space, as the canvas wrote it
    // proven, contradicted and recorded: the live answers' other states (from-change.js), in the state words
    const PILL_OF = { not_proven: ['notProven', '○'], changed: ['changed', '◭'], no_result: ['noResult', '⋯'], prepared: ['prepared', ''], neutral: ['neutral', ''], proven: ['proven', '●'], contradicted: ['notProven', '✕'], recorded: ['notProven', '○'] };
    const aboutText = (a) => {
      const parts = [];
      if (a.statements && a.statements.notProven != null) parts.push(fmtCount(a.statements.notProven) + ' of ' + fmtCount(a.statements.of) + ' statements');
      if (a.statements && a.statements.proven != null) parts.push(fmtCount(a.statements.proven) + ' of ' + fmtCount(a.statements.of) + ' statements');
      if (a.opposite) parts.push('the record shows the opposite');
      if (a.version) parts.push('version ' + a.version);
      if (a.run != null) parts.push('the run at step ' + a.run);
      if (a.checkAt != null) parts.push('the check at ' + a.checkAt);
      if (a.step != null) parts.push('step ' + a.step);
      if (a.prepared) parts.push('prepared, not sent');
      return parts.length ? '· ' + parts.join(' · ') : '';
    };
    const notRecordedLine = D.record.hookInstalledAt
      ? 'Before ' + D.record.hookInstalledAt + ', when the hook was installed.'
      : (D.record.window && D.record.window.from ? 'Before ' + D.record.window.from + ', when this Change’s first step was recorded.' : 'Anything before the first step recorded here.');
    const ANS = {};
    QS.forEach((x) => {
      const a = D.answers[x.key];
      if (!a) return;
      if (a.unanswered) {
        // one of the five the record cannot answer: the same three parts every time
        ANS[x.key] = { q: x.text, pill: 'fog', glyph: '◌', word: 'The record doesn’t say', meta: '', silent: true,
          lookedFor: a.lookedFor, missing: a.missing, notRecorded: notRecordedLine, would: a.would, lines: [], ev: [], looked: false, action: 'silent' };
        return;
      }
      const pk = PILL_OF[a.state] || PILL_OF.not_proven;
      ANS[x.key] = {
        q: x.text, pill: pk[0], glyph: pk[1], word: a.word, meta: aboutText(a.about || {}),
        lines: a.lines.map((l) => L({ pre: l.pre || '', qo: l.quote ? '❝' : '', qt: l.quote || '', qc: l.quote ? '❞' : '', code: l.code || '', post: l.post || '', r: R(l.steps.map((n) => 'step ' + n).join(' · ')) })),
        ev: a.evidence.slice(), looked: !!a.looked, action: a.action, seeChanged: !!a.seeChanged,
        reason: a.reason || '', draft: a.draft || '',
      };
    });

    // the silent answer: the record doesn't say
    const at = String(s.askedText || '').trim();
    const aw = kw(at);
    const aq = quoted(aw, at);
    const aFind = at ? REC.filter((r) => r.search.indexOf(at.toLowerCase()) >= 0 || aw.some((w) => r.search.indexOf(w) >= 0)) : [];
    /* live: what the record search found for the asked text too (the rest of the record) */
    const aRoute = isDemo || !at ? null : searchFind(this.props.found, at, { when: whenOf });
    const aCount = aFind.length + (aRoute && aRoute.state === 'served' ? aRoute.total : 0);
    const aWaiting = !!aRoute && aRoute.state === 'reading';
    const aTitle = at ? at.charAt(0).toUpperCase() + at.slice(1) + (/\?$/.test(at) ? '' : '?') : '';
    const proj = D.project.name || 'this project';
    const SILENT = {
      q: aTitle, pill: 'fog', glyph: '◌', word: 'The record doesn’t say', meta: '', silent: true,
      lookedFor: isDemo || D.open
        ? aq + ' in the text, file paths and commands of all ' + fmtCount(D.record.steps) + ' recorded steps' + (D.record.window.from ? ', ' + D.record.window.from + '–' + D.record.window.to : '') + (aRoute && aRoute.state === 'served' ? ', and in the rest of ' + proj : '') + '.'
        : aq + ' in the titles and most-edited files of all ' + fmtCount(D.record.steps) + ' Changes in this project' + (aRoute && aRoute.state === 'served' ? ', and in what they hold' : '') + '.',
      missing: aWaiting
        ? (aFind.length
          ? fmtCount(aFind.length) + (aFind.length === 1 ? ' thing here mentions ' : ' things here mention ') + aq + ' (listed under Find).'
          : (D.open ? 'Nothing in this Change’s steps contains ' : 'No Change’s title or most-edited file contains ') + aq + '.') + ' The rest of ' + proj + ' is still being searched.'
        : aCount === 0
          ? 'None of them contains ' + aq + '. No edit touched a file by that name, and no command mentions it.' + (aRoute && aRoute.state === 'failed' ? ' ' + aRoute.message : '')
          : isDemo || !aRoute || aRoute.state !== 'served'
            ? fmtCount(aCount) + (aCount === 1 ? ' recorded step mentions ' : ' recorded steps mention ') + aq + ' (listed under Find), but none of the ' + fmtCount(nQ) + ' questions the record can answer covers this one.'
            : fmtCount(aCount) + (aCount === 1 ? ' thing in the record mentions ' : ' things in the record mention ') + aq + ' (listed under Find), but none of the ' + fmtCount(nQ) + ' questions the record can answer covers this one.',
      notRecorded: notRecordedLine,
      would: aCount === 0
        ? 'A command, edit or message naming ' + aq + ', reaching this Change’s record.'
        : 'Opening one of the matches under Find. Each one carries its own receipt; Ask doesn’t summarise them.',
      lines: [], ev: [], looked: false, action: 'silent',
    };

    const A = s.asked === 'typed' ? SILENT : (s.asked ? ANS[s.asked] : null);
    const pinnedOk = !!(A && s.pinned && A.ev.indexOf(s.pinned) >= 0);
    const evRows = A ? A.ev.map((k, i) => {
      const e = EV[k];
      const pinned = pinnedOk && s.pinned === k;
      const b = BADGE[e.badgeKind] || BADGE.current;
      return {
        kind: e.kind, qo: e.qo || '', qc: e.qc || '', text: e.text, fontCss: e.mono ? MONO : "13px 'Geist', -apple-system, sans-serif",
        hasNext: i < A.ev.length - 1,
        hasResult: !!e.result, result: e.result || '',
        hasBadge: !!e.badge, hasBadgeGlyph: !!e.badgeGlyph, badgeGlyph: e.badgeGlyph || '', badge: e.badge, badgeBorder: b.b, badgeColor: b.c, badgeBg: b.bg,
        hasNote: !!e.note, note: e.note || '', step: e.step,
        pressed: String(pinned),
        border: pinned ? '1px solid #FF7A29' : '1px solid #2A2A30',
        shadow: pinned ? '0 0 0 1px rgba(255,122,41,.25), 0 0 14px rgba(255,122,41,.16)' : 'none',
        pick: () => set({ pinned: pinned ? null : k }),
      };
    }) : [];
    const pe = pinnedOk ? EV[s.pinned] : null;

    const plainOf = (l) => l.pre + l.qo + l.qt + l.qc + l.code + l.post + l.r;
    const answerText = () => {
      if (!A) return '';
      const parts = ['Q: ' + A.q, 'A: ' + A.word + (A.meta ? ' ' + A.meta : '')];
      if (A.silent) parts.push('Looked for: ' + A.lookedFor, 'Missing: ' + A.missing, 'Not recorded: ' + A.notRecorded, 'Would answer it: ' + A.would);
      A.lines.forEach((l) => parts.push(plainOf(l)));
      A.ev.forEach((k) => { const e = EV[k]; parts.push('- ' + e.kind + ' · ' + (e.qo || '') + e.text + (e.qc || '') + (e.result ? ' · ' + e.result : '') + (e.badge ? ' · ' + (e.badgeGlyph ? e.badgeGlyph + ' ' : '') + e.badge : '') + ' · ' + e.step.replace(' ›', '')); });
      parts.push('From the ThruLine record of ' + (D.project.name || 'this project') + ', read ' + D.readAt + '.');
      return parts.join('\n');
    };
    /* A copy says "Copied" only once the clipboard took it (design note 12); a
       refusal says so. Nothing is sent anywhere: the clipboard is this machine's. */
    const toClipboard = (t, done) => {
      let wrote;
      try {
        wrote = typeof navigator !== 'undefined' && navigator.clipboard && typeof navigator.clipboard.writeText === 'function'
          ? navigator.clipboard.writeText(t) : Promise.reject(new Error('no clipboard'));
      } catch (err) { wrote = Promise.reject(err); }
      Promise.resolve(wrote).then(() => done(true), () => done(false));
    };
    const nEv = A ? A.ev.length : 0;

    const p = A ? (PILL[A.pill] || PILL.notProven) : PILL.notProven;
    // the two questions the "doesn't say" answer offers next: the design's five, in the gallery and on the record
    const TRY = ['changed', 'destructive'];
    const answer = {
      show: !!A,
      q: A ? A.q : '',
      pill: { border: p.border, bg: p.bg, color: p.color, glyphColor: p.glyphColor, metaColor: p.metaColor, glyph: A ? A.glyph : '', hasGlyph: !!(A && A.glyph), isPrepared: !!(A && A.pill === 'prepared'), word: A ? A.word : '', meta: A && A.meta ? A.meta : '' },
      hasLines: !!(A && A.lines.length),
      lines: A ? A.lines : [],
      silent: !!(A && A.silent),
      lookedFor: A && A.silent ? A.lookedFor : '',
      missing: A && A.silent ? A.missing : '',
      notRecorded: A && A.silent ? A.notRecorded : '',
      would: A && A.silent ? A.would : '',
      tryChanged: () => ask(TRY[0]),
      tryDestructive: () => ask(TRY[1]),
      tryChangedText: (QS.find((x) => x.key === TRY[0]) || {}).text || '',
      tryDestructiveText: (QS.find((x) => x.key === TRY[1]) || {}).text || '',
      hasEv: nEv > 0,
      ev: evRows,
      pin: { show: !!pe, kind: pe ? pe.kind : '', step: pe ? pe.step.replace(' ›', '') : '', detail: pe ? pe.detail : '' },
      looked: !!(A && A.looked),
      hasDraft: !!(A && A.draft),
      draft: A && A.draft ? A.draft : '',
      actCheck: !!(A && A.action === 'check'),
      checkHref: CHK.checkHref || undefined,
      actTell: !!(A && A.action === 'tell'),
      actNone: !!(A && A.action === 'none'),
      seeChanged: !!(A && A.seeChanged),
      reason: A && A.reason ? A.reason : '',
      copyLabel: s.copied === 'answer' ? (nEv ? 'Copied with ' + fmtCount(nEv) + (nEv === 1 ? ' receipt' : ' receipts') : 'Copied') + ' · not sent'
        : s.copied === 'answer-refused' ? 'Not copied · the browser refused'
        : (nEv === 0 ? 'Copy the answer' : (nEv === 1 ? 'Copy answer with its receipt' : 'Copy answer with receipts')),
      announce: A ? 'Answer: ' + A.word + (A.meta ? ' ' + A.meta : '') + '. ' + A.q : '',
      copy: () => { const asked = s.asked; toClipboard(answerText(), (ok) => { if (this.state.asked === asked) set({ copied: ok ? 'answer' : 'answer-refused' }); }); },
      tellLabel: s.copied === 'tell' ? 'Copied — not sent' : s.copied === 'tell-refused' ? 'Not copied · the browser refused' : 'Copy for the agent',
      tell: () => { const asked = s.asked; toClipboard(A && A.draft ? A.draft : '', (ok) => { if (this.state.asked === asked) set({ copied: ok ? 'tell' : 'tell-refused' }); }); },
    };

    const onStory = s.tab === 'story';
    const tabs = {
      answer: { on: !onStory, sel: String(!onStory), line: onStory ? 'transparent' : '#FF7A29', color: onStory ? T2 : T1, pick: () => set({ tab: 'answer' }) },
      story: { on: onStory, sel: String(onStory), line: onStory ? '#FF7A29' : 'transparent', color: onStory ? T1 : T2, pick: () => set({ tab: 'story' }) },
    };
    const closeAnswer = () => set({ asked: null, askedText: '', pinned: null, copied: null, tab: 'answer' });

    // ---------- the lit thread: question row → answer ----------
    const silent = s.asked === 'typed';
    const y0 = askedRow && posY[askedRow] ? posY[askedRow] : INPUT_Y;
    const thread = {
      show: !!A,
      d: 'M 886 ' + y0 + ' C 968 ' + y0 + ', 970 ' + END_Y + ', 1052 ' + END_Y,
      y0: String(y0),
      stroke: silent ? '#82828C' : '#FF7A29',
      width: silent ? '1.5' : '2',
      dash: silent ? '4 5' : 'none',
      filter: silent ? 'none' : 'drop-shadow(0 0 4px rgba(255,154,82,.7))',
      endFill: silent ? '#0A0A0B' : '#FF7A29',
    };

    // ---------- esc ladder: the answer, then the field, then back to the Change ----------
    const escClears = !!A || !!qt;
    // every change of the typed text is told to the screen, which asks the record for it (index.jsx, data/search.js)
    const onQuery = typeof this.props.onQuery === 'function' ? this.props.onQuery : null;
    const setQuery = (v, extra) => { const was = this.state.query; set(Object.assign({ query: v }, extra)); if (onQuery && v !== was) onQuery(v); };
    const escRun = () => { if (A) closeAnswer(); else setQuery('', { active: 'act-check' }); };
    const esc = {
      clears: escClears,
      closes: !escClears,
      label: A ? 'Close the answer' : 'Clear what you typed',
      hint: A ? 'closes the answer' : (qt ? 'clears the field' : (!isDemo && overlay ? 'closes the palette' : 'back to the Change')),
      run: escRun,
    };

    // ---------- the field ----------
    const onType = (e) => { const v = e && e.target ? String(e.target.value) : ''; setQuery(v, { active: '' }); };
    /* ↵ on an Act, Go to or Find row follows that row's own link (design note 2,
       census §3.08 row 2): the very element drawn for the row is clicked, so ↵
       goes where a click goes, the overlay closes as a click closes it, and
       Check it again opens the sheet already running exactly as its click does.
       Approve is drawn aria-disabled: ↵ on it does nothing. A row with no link
       does nothing. */
    const follow = (e) => {
      const doc = e && e.currentTarget && e.currentTarget.ownerDocument ? e.currentTarget.ownerDocument : (typeof document !== 'undefined' ? document : null);
      const el = doc && activeId ? doc.getElementById('tl-opt-' + activeId) : null;
      if (!el || el.getAttribute('aria-disabled') === 'true') return;
      if (String(el.tagName).toUpperCase() !== 'A' || !el.getAttribute('href')) return;
      if (e.preventDefault) e.preventDefault();
      el.click();
    };
    const onKey = (e) => {
      if (!e) return;
      const k = e.key;
      if (k === 'ArrowDown' || k === 'ArrowUp') {
        if (e.preventDefault) e.preventDefault();
        if (!order.length) return;
        const i = order.indexOf(activeId);
        const j = k === 'ArrowDown' ? Math.min(order.length - 1, i + 1) : Math.max(0, i - 1);
        set({ active: order[j] });
      } else if (k === 'Enter') {
        if (activeId === 'ask-typed') askTyped();
        else if (activeId.indexOf('ask-') === 0) ask(activeId.slice(4));
        else follow(e);
      } else if (k === 'Escape') {
        if (escClears) {
          escRun();
          // in the overlay, Esc clears first; the app's own Esc must not close the palette yet
          if (overlay && e.stopPropagation) e.stopPropagation();
        } else if (overlay && onClose) {
          onClose();
          // the palette is gone before the app's own keys look at this Esc, which on Check it again and
          // The whole story would then read it as "Back to the Change": one Esc is one act
          if (e.stopPropagation) e.stopPropagation();
        }
      }
    };
    /* The field keeps character keys for typing. When focus leaves it, the
       palette's own displayed shortcuts follow its rows; the Change behind
       remains inert while this modal is open. */
    const paletteKey = (e) => {
      const k = keyOf(e);
      if (!k || k.kind !== 'single' || !singleAllowed(k, this.props.keysMode || 'on') || e.repeat) return;
      const click = (id) => {
        const doc = e.target?.ownerDocument || document;
        const el = doc.getElementById('tl-opt-' + id);
        if (!el || el.getAttribute('aria-disabled') === 'true' || !el.getAttribute('href')) return;
        e.preventDefault();
        el.click();
      };
      if (k.token === 'g') {
        this.__gAt = Date.now();
        e.preventDefault();
        return;
      }
      const pending = this.__gAt && Date.now() - this.__gAt <= 1000;
      this.__gAt = null;
      if (pending) {
        const row = { d: 'act-define', l: 'go-statements', w: 'go-story', i: 'go-inbox' }[k.token];
        if (row) click(row);
        return;
      }
      if (k.token === '.') click('act-check');
      if (k.token === 'y') {
        if (A && A.action === 'tell' && A.draft) {
          e.preventDefault();
          answer.tell();
        } else click('act-tell');
      }
    };

    // ---------- design-review switcher ----------
    // the switcher's presets are design-only: they live in DEMO, never in the record
    const PRESETS = DEMO.presets;
    const isPreset = (k) => { const P = PRESETS[k]; return ql === P.query && s.asked === P.asked && (P.asked !== 'typed' || at.toLowerCase() === P.askedText); };
    const current = (!qt && !s.asked) ? 'empty' : isPreset('finish') ? 'finish' : isPreset('migration') ? 'migration' : '';
    const demo = {};
    Object.keys(PRESETS).forEach((k) => {
      const on = current === k;
      demo[k] = {
        pressed: String(on),
        bg: on ? '#1F1F23' : 'transparent',
        border: on ? '#66666F' : '#2A2A30',
        color: on ? T1 : T2,
        pick: () => set(Object.assign({}, PRESETS[k], reset)),
      };
    });

    // ---------- the header, the footer and the fixed lines, from the record ----------
    const R2 = D.record;
    const looked = D.proofStop;
    const lookedText = ' ' + fmtCount(looked.finishedRuns.length) + (looked.finishedRuns.length === 1 ? ' finished run (' : ' finished runs (')
      + looked.finishedRuns.map((r) => 'step ' + r.step + ' on ' + r.version).join(', ') + ')'
      + (looked.pendingSteps.length
        ? ' and ' + stepsText(looked.pendingSteps) + (looked.pendingSteps.length === 1 ? ', which has no result yet.' : ', which have no result yet.')
        : '.');
    const st = D.story || {};
    const storyState = STATE[st.state] || STATE.not_proven;
    const gb = D.storage ? D.storage.total : {};
    const top = isDemo ? {
      project: D.project.name,
      title: D.change.title,
      version: V,
      versionMeta: (D.version.current ? '(current)' : '') + (D.version.commit ? ' · = commit ' + D.version.commit : '') + (D.version.unsavedChanges ? ' · unsaved changes' : ' · no unsaved changes'),
      readStamp: 'read ' + D.readAt + ' ↻',
      status: fmtCount(R2.steps) + ' steps · ' + fmtCount(R2.statements) + (R2.statements === 1 ? ' statement · ' : ' statements · ') + fmtCount(R2.statementsNeedingCheck) + (R2.statementsNeedingCheck === 1 ? ' needs a check' : ' need a check') + (D.recording ? ' · recording' : ''),
      keeping: 'ThruLine is keeping ' + gb.value + ' ' + gb.unit + ' of records on this machine · ',
    } : liveTop(D);
    const pal = !isDemo ? livePal(D) : {
      version: V,
      actOn: 'on ' + V + ' · each one shows its key',
      checkSub: D.check.command + ' · on ' + V,
      checkAsks: D.check.command + ' · on ' + V + ' · asks once for this project',
      approveWaits: 'waits on ' + fmtCount(D.approval.waitsOn) + (D.approval.waitsOn === 1 ? ' thing' : ' things'),
      approveWhy: ' · ' + fmtCount(D.criteria.proposed) + ' proposed ' + (D.criteria.proposed === 1 ? 'line' : 'lines') + ' of what counts as done, ' + (D.criteria.accepted ? fmtCount(D.criteria.accepted) + ' accepted' : 'none accepted'),
      defineSub: fmtCount(D.criteria.proposed) + ' ' + (D.criteria.proposed === 1 ? 'line' : 'lines') + ' proposed from your request · written only when you accept',
      statementsSub: fmtCount(R2.statements) + (R2.statements === 1 ? ' statement · ' : ' statements · ') + fmtCount(R2.statementsNeedingCheck) + (R2.statementsNeedingCheck === 1 ? ' needs a check' : ' need a check'),
      changedSub: fmtCount(D.diff.edits) + (D.diff.edits === 1 ? ' edit to ' : ' edits to ') + fmtCount(D.diff.files) + (D.diff.files === 1 ? ' file' : ' files') + ' · ' + fmtCount(D.diff.filesChangedAfterLastCheck) + (D.diff.filesChangedAfterLastCheck === 1 ? ' file changed' : ' files changed') + ' after the last check',
      askedAbout: '· asked ' + D.askedAt + ' · about this Change',
      looked: lookedText,
      seeChanged: 'See what changed in ' + looked.changedSince.file,
      pasteInto: 'Nothing here reaches the agent unless a supported route says it did. You paste it into ' + D.change.agent + '.',
      storySaid: 'said by ' + st.agent + ' · step ' + st.step + ' · ' + st.at,
      storyText: st.text,
      storyGlyph: storyState.glyph,
      storyWord: storyState.word,
      storyMeta: '· version ' + st.version,
      storyWhy: st.why,
      storyLast: 'The last version ThruLine recorded before this was said is ' + st.lastVersionBefore + '. ThruLine doesn’t record the code at the moment it spoke.',
      scope: 'this Change',
      goHint: 'in this Change · moving between views settles nothing',
    };

    return {
      isDemo: isDemo,
      overlay: overlay,
      onClose: onClose || undefined,
      // Esc's last rung, and any link the palette follows, close the overlay
      escCloseClick: overlay && onClose ? (e) => { if (e && e.preventDefault) e.preventDefault(); onClose(); } : undefined,
      overlayClick: (e) => { const t = e && e.target; if (onClose && t && t.closest && t.closest('a[href]')) onClose(); },
      top: top,
      pal: pal,
      query: query,
      onType: onType,
      onKey: onKey,
      onPaletteKey: paletteKey,
      activeDomId: activeId ? 'tl-opt-' + activeId : '',
      enterHint: hints[activeId] || 'nothing to run',
      esc: esc,
      sec: sec,
      o: o,
      findRows: findRows,
      askRows: askRows,
      otherRows: otherRows,
      typed: typed,
      answer: answer,
      tabs: tabs,
      closeAnswer: closeAnswer,
      thread: thread,
      demo: demo,
    };
  }
}
