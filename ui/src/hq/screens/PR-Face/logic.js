/* PR-Face.dc.html: the artboard's logic. renderVals() returns everything
 * template.jsx reads. Content comes only from `this.props.data` (a live
 * preview, from-change.js) or DEMO (the design gallery) — see DATA.md;
 * `this.state` holds UI state only (which folds are open, whether the copy
 * worked).
 *
 * The page is a PREVIEW of the note for this Change's pull request, and of
 * what the ThruLine Action would post for its folder (C30 R-PR). ThruLine
 * posts nothing: its one way out is Copy as Markdown, which puts exactly
 * `data.copy` — the note the preview draws, then the setup line drawn under it
 * — on the clipboard through `props.actions.copy`. Work nobody tracked has no
 * note; its one act is "Track this work as a Change" (`props.actions.track`).
 * Nothing here fetches or posts.
 */
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { TRAILER_KEY } from './from-change.js'

export const PROPS = {"$preview":{"width":1512,"height":2400}}

/** What the page says under the comment, beside the setup line. */
export const PASTE_NOTE = 'Copy as Markdown copies the note and the line above it. Paste them yourself: ThruLine posts nothing and makes no network call.'
export const PASTE_NOTE_UNTRACKED = 'ThruLine posts nothing to a pull request and makes no network call.'

const HAIR = '1px solid #1F1F23'

export default class Component extends DCLogic {
  state = { toggled: {}, copy: null };

  copy() {
    const D = this.props.data;
    const act = this.props.actions;
    if (!D || typeof D.copy !== 'string' || !act || typeof act.copy !== 'function') {
      this.setState({ copy: 'failed' });
      return;
    }
    Promise.resolve()
      .then(() => act.copy(D.copy))
      .then(() => this.setState({ copy: 'done' }), () => this.setState({ copy: 'failed' }));
  }

  renderVals() {
    const s = this.state;
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    const host = D.host || {};
    const note = D.note || { lines: [], layers: [] };
    const act = this.props.actions || {};

    const layers = (note.layers || []).map((ly, i) => {
      const open = !!ly.open !== !!s.toggled[i];
      const toggle = () => this.setState({ toggled: { ...this.state.toggled, [i]: !this.state.toggled[i] } });
      return {
        open, summary: ly.summary, items: ly.items || [], toggle,
        key: (e) => { if (e && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); toggle(); } },
      };
    });

    const copyStatus = s.copy === 'done'
      ? 'Copied. Paste it into the pull request yourself; ThruLine posted nothing.'
      : s.copy === 'failed'
        ? 'This browser did not let ThruLine copy. Select the note in the preview and copy it instead.'
        : null;

    const untracked = !isDemo && D.untracked ? D.untracked : null;
    const cr = !isDemo && D.checkRun ? D.checkRun : null;
    const rows = cr ? (cr.conclusions || []) : [];
    const tr = !isDemo && D.trailer ? D.trailer : null;

    return {
      isDemo,
      wrap: !isDemo,
      crumbKind: (D.board && D.board.crumbKind) || 'preview',
      crumb: (D.board && D.board.crumb) || 'this project',
      figureLabel: isDemo
        ? 'Pull request 482, Conversation tab: ThruLine’s note as a code host renders its Markdown'
        : untracked
          ? 'Preview: there is no note for work that is not tracked as a Change.'
          : 'Preview: ThruLine’s note as a code host would render its Markdown. Nothing is posted.',
      thesisA: untracked ? 'This work is not tracked as a Change yet.' : 'This is the note for this Change’s pull request, read from the record.',
      thesisB: untracked ? 'There is no note until it is.' : 'Nothing is posted until you paste it.',
      host: {
        titleText: (host.title || 'A Change with no recorded title') + (host.number ? ' ' : ''),
        number: host.number ? '#' + host.number : null,
        pill: host.pill,
        author: host.author || null,
        base: host.base, head: host.head,
        project: host.project || 'this project',
        version: host.version || null,
        meta: host.meta || [host.project || 'this project'],
      },
      avatar: host.author ? host.author.initial : 'you',
      articleLabel: isDemo ? 'ThruLine note, as posted at ' + ((D.comment && D.comment.postedAt) || '') : 'ThruLine note, as you would paste it',
      postedAt: D.comment && D.comment.postedAt,
      readAt: (D.comment && D.comment.readAt) || 'an unrecorded time',
      note,
      layers,
      /* the approval it waits on, and Copy as Markdown: a tracked Change only */
      approval: !isDemo && D.approval ? D.approval : null,
      copyLabel: s.copy === 'done' ? 'Copied' : 'Copy as Markdown',
      onCopy: () => this.copy(),
      copyStatus,
      setup: !isDemo && D.setup ? D.setup : null,
      pasteNote: isDemo ? null : untracked ? PASTE_NOTE_UNTRACKED : PASTE_NOTE,
      untracked,
      onTrack: () => { if (typeof act.track === 'function' && !(untracked && untracked.busy)) act.track(); },
      edited: !isDemo && D.edited ? D.edited : null,
      live3: cr ? {
        name: cr.name || 'The check run',
        sub: ['What the ThruLine Action would post for this Change’s folder, from the newest receipt in ', { code: '.thruline/receipts/' }, '.'],
        figureLabel: 'The check run' + (cr.name ? ' ' + cr.name : '') + ', as this Change’s folder stands now',
        tag: cr.tag,
        readAt: cr.readAt ? 'read ' + cr.readAt : '',
        title: cr.title,
        lines: cr.lines || [],
        command: cr.command,
        check: cr.canCheck ? {
          label: cr.checking ? 'Checking this receipt…' : 'Check this receipt',
          disabled: !!cr.checking,
          onClick: () => { if (typeof act.check === 'function') act.check() },
        } : null,
        checkError: cr.checkError,
        rows: rows.map((c, i) => ({ conclusion: c.conclusion, text: 'The badge reads “' + c.label + '”.', border: i === rows.length - 1 ? 'none' : HAIR })),
        note: rows.length
          ? cr.tag !== 'not checked here' && cr.tag !== 'no receipt'
            ? 'This is the CLI verifier’s result for the receipt you checked. Re-check after the folder or head changes.'
            : 'Which one a pull request gets is decided by checking its receipt on the pull request’s head. ThruLine does not decide it while you view this page.'
          : 'What the check run can conclude could not be read from this machine.',
      } : null,
      live4: tr ? {
        subject: 'your commit message',
        trailerKey: tr.text ? TRAILER_KEY.trim() : null,
        trailerValue: tr.text ? ' ' + tr.text.slice(TRAILER_KEY.length) : null,
        trailerNote: tr.note,
        badge: (D.badge && D.badge.lines) || [],
        badgeImage: D.badge && D.badge.image ? D.badge.image : null,
      } : null,
      aside: isDemo ? {
        a: 'Invisible once rendered; the next render edits this comment in place.',
        b: 'The same bytes as HQ’s Note and the check run’s summary. Markdown can’t colour text, so each state travels as glyph and word.',
        c: 'One row per line that counts as done; each chain ends where the proof stops.',
        e: 'A version can hold unsaved work, so the note names the commit it equals, in one of three forms.',
        f: 'How to check it yourself, what it is not, what was masked, and the UTC offset.',
      } : {
        a: 'Invisible once rendered; it names the Change and the version the note describes.',
        b: 'The state, why, how to settle it, and approval, from the Change’s own reading. Markdown can’t colour text, so each state travels as glyph and word.',
        c: 'One row per line of what counts as done, where it stands, and where its proof stops.',
        e: 'A version can hold unsaved work, so the note says whether a commit holds it.',
        f: 'How to check it yourself, what it is not, what was masked, and that every time is UTC.',
      },
    };
  }
}
