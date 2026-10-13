import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { API, readProjection } from './data.js'
import { readChangeDocuments } from './change-workspace.jsx'
import { deriveChangeModel, versionSentence } from './change-model.js'
import { inboxItems, inboxSections, reasonLine, storageReading, changeHrefAt } from './inbox-view.js'
import { START_HASH } from '../routes.js'
import './workspace.css'

/* The Inbox — the app's landing (T-2-6; `B8.1`–`B8.6`, `03-USER-LENS.md`
 * rules 7, 8, 12, 13, `L1.1`).
 *
 * WHAT THIS SCREEN IS. The first thing a person meets. One item per Change
 * that needs something, the reason in everyday English, and one next action
 * that opens the Change at the stage which would settle it. The baseline's two
 * worst moments are what it removes: landing on the dense technical view
 * (H2T-2), and a control labelled Inbox that opened a session picker over it
 * (H2T-3). Sessions, Trace, receipts, checks and settings keep their routes
 * and are one level down, never gone.
 *
 * WHAT IT MAY DO. Read. `readProjection` and `readChangeDocuments` are the
 * only I/O and every address they are given is a GET. There is no POST in this
 * file; the Inbox cannot write a record.
 *
 * WHAT IT MAY SAY. Every user-visible string comes from
 * `HQ_COPY.change.inbox`, with two disclosed exceptions, both of which are the
 * RECORD speaking rather than this product: the Change's own title, and the
 * project folder the record names (BUILD-14: its last part on the card, and
 * the repository identity and the whole folder one level down). The version line is `change.version.bound`,
 * shared with the Change screen.
 *
 * WHAT IT REFUSES TO SAY. No percentage. No count of events, of sessions, or of
 * anything a person did not ask about (`B8.6`). Nothing green.
 *
 * WHAT CHANGED AT T-S-3, AND WHY IT IS WRITTEN DOWN RATHER THAN DONE QUIETLY.
 * This paragraph said "No state word" and that was the right rule while each
 * screen wrote its own strings: a word the Inbox computed could disagree with
 * the Change screen's, and a queue that contradicts the screen it opens is
 * worse than a queue that says less. The design record's §3.8 and §10 (binding
 * by `R-2026-09-22-3`) close that differently — every state word comes from ONE
 * derived view-model — so the row now shows the state word, and shows it by
 * rendering `item.hq.inboxRow.line`, which this file does not compute and
 * cannot alter. The old rule is kept in the half that still holds: the Inbox
 * still derives no state word of its own.
 * Every reason is a sentence about the record; the Inbox never concludes that
 * the work is good, and with nothing waiting it says only that nothing is
 * waiting.
 */

const C = HQ_COPY.change.inbox
const A = HQ_COPY.change.attention

/**
 * How many Changes this screen reads in full.
 *
 * A reason can need the confirmed work's findings, which is two reads per
 * piece of linked work. Reading an unbounded number of Changes on a landing
 * would make the first screen the slowest one. The ones beyond the bound are
 * SAID — `unread_one` / `unread_many` — never silently dropped (Rule 12).
 */
export const READ_LIMIT = 25

/* ── reading ──────────────────────────────────────────────────────────── */

/**
 * Everything the Inbox needs, read once.
 *
 * The list first; then the newest `READ_LIMIT` Changes in full, through the
 * same reader the Change screen uses, so no second reader can decide a
 * Change's facts differently. A Change that could not be read is SAID in
 * `notes` and contributes no item — never a silent omission from a queue whose
 * whole job is to be complete about what needs attention.
 *
 * @returns {Promise<object>}
 */
export async function readInbox() {
  const notes = []
  const head = await readProjection(API.changes)
  if (head.status === 'refused') return { status: 'refused', refusal: head.refusal, notes }
  const all = Array.isArray(head.doc && head.doc.changes) ? head.doc.changes : []
  const read = all
    .filter((c) => c && typeof c.id === 'string' && c.id)
    .slice()
    .sort((a, b) => String(b.opened_at || '').localeCompare(String(a.opened_at || '')))
    .slice(0, READ_LIMIT)

  const docsById = new Map()
  /* What the reader said about each Change, kept beside it, so a Change that
   * could not be read in full carries its own words one level down on its own
   * line (SEAM-INBOX-READ-VERIFY item 7). `notes` still holds every one. */
  const notesById = new Map()
  for (const row of read) {
    const docs = await readChangeDocuments(row.id)
    const own = [...(docs.notes || [])]
    if (docs.status !== 'ready') {
      if (docs.refusal) own.push(docs.refusal)
      else {
        const title = row.request && typeof row.request.title === 'string' ? row.request.title.trim() : ''
        own.push(title ? fill(C.detail_missing, { title }) : C.detail_missing_unnamed)
      }
    } else docsById.set(row.id, docs)
    notes.push(...own)
    notesById.set(row.id, own)
  }

  /* The store's own size, `L1.1`. An optional local capability: if the server
   * this page is served by does not answer, the sentence says it could not be
   * measured rather than guessing a number. */
  const size = await readProjection(API.storage)
  if (size.status === 'refused') notes.push(size.refusal)

  return { ...assembleInbox(all, docsById, notesById), storage: storageReading(size.doc || null), notes }
}

/**
 * The Inbox, from documents already read (A3-F2). PURE: no fetch, no clock.
 *
 * `readInbox` is this plus the reads, so a test can hand it a generated store
 * and assert what every surface built from the result says. Which Changes are
 * read in full is decided HERE — newest first, `READ_LIMIT` of them — and a
 * Change whose documents are missing or incomplete contributes no item
 * (the reader already said why in its notes).
 *
 * @param {Array<object>} all the `changes` array of `/api/changes`
 * @param {Map<string, object>} docsById what `readChangeDocuments` returned,
 *   per Change id, for the Changes that were read
 * @param {Map<string, string[]>} [notesById] what the reader said about each
 *   Change; an incomplete Change without an entry falls back to its own notes
 * @returns {object}
 */
export function assembleInbox(all, docsById, notesById) {
  const sorted = (Array.isArray(all) ? all : [])
    .filter((c) => c && typeof c.id === 'string' && c.id)
    .slice()
    .sort((a, b) => String(b.opened_at || '').localeCompare(String(a.opened_at || '')))
  const read = sorted.slice(0, READ_LIMIT)
  const inputs = []
  const models = new Map()
  /* SEAM-INBOX-READ-VERIFY item 7 — each Change not read in full, named, with
   * what the reader said about it. Its count is `unreadable`. */
  const unreadableChanges = []
  for (const row of read) {
    const docs = docsById && docsById.get(row.id)
    if (!docs || docs.incomplete) {
      const own = notesById && notesById.get(row.id)
      unreadableChanges.push({
        id: String(row.id),
        title: (row.request && typeof row.request.title === 'string' ? row.request.title : '').trim(),
        notes: Array.isArray(own) ? own : (docs && Array.isArray(docs.notes) ? docs.notes : []),
      })
      continue
    }
    const hq = deriveChangeModel(docs.change, docs.coverage, docs.applicability, docs.proposals, docs.work)
    models.set(row.id, hq)
    inputs.push({
      change: docs.change,
      proposals: docs.proposals,
      work: docs.work,
      /* P6-INBOX-REASON: the hard-coded empty list is gone. What each accepted
       * line has behind it is derived once, in `change-model.js`, and read
       * from `hq` by `reasonsFor`. */
      applicability: [],
      /* T-S-3 — derived once, here, from the documents the Change screen and
       * the packet read, so the row and the screen cannot say two things. */
      hq,
    })
  }
  const { items, settled } = inboxItems(inputs)
  return {
    status: 'ready',
    items,
    settled,
    unreadable: unreadableChanges.length,
    unreadableChanges,
    /* A3-F14 — every open Change, newest first, with the ONE derived model
     * for each one that was read (null for the rest). The list of Changes on
     * the landing renders from this, so its sentence per Change is the
     * Change's own headline and its count is this Inbox's. */
    changes: sorted.map((row) => ({
      id: String(row.id),
      title: (row.request && typeof row.request.title === 'string' ? row.request.title : '').trim(),
      hq: models.get(row.id) || null,
    })),
    total: sorted.length,
    unread: sorted.length - read.length,
    newest: sorted.length ? String(sorted[0].id) : null,
  }
}

/* ── the pieces ───────────────────────────────────────────────────────── */

/** Where the work is, as one line. The version line is the Change screen's own. */
export function Scope({ scope }) {
  const version = scope.version
  /* T-S-3: the Inbox used to build this sentence itself, from the same typed
   * fields the Change screen used, with its own words for the two edge cases —
   * which is exactly how two screens end up naming one version differently.
   * `versionSentence` is the one derivation; the Inbox's own short forms stay
   * only for the two shapes it has always said differently on a list row. */
  const versionText = version.bound
    ? version.named
      ? versionSentence(version)
      : C.scope.version_unnamed
    : C.scope.no_version
  /* BUILD-14 (PW-F3) — the card names the project by its folder
   * (`projectNameOf`, the one rule the crumb bar uses too). The repository
   * identity is the engine's digest, not a word a person reads: it moved one
   * level down, into the disclosure below, and is never dropped. With no
   * folder recorded the project segment is left out rather than filled with
   * the digest; with nothing bound at all the sentence says so, as before. */
  const where = scope.project || (scope.repository ? null : C.scope.no_repository)
  return <>
    <p className="ws-inbox-scope">
      {where ? <span className="ws-inbox-where">{where}</span> : null}
      <span className="ws-inbox-version">{where ? ' · ' : ''}{versionText}</span>
    </p>
    {scope.repository ? <details className="ws-inbox-where-technical">
      <summary>{C.scope.technical_open}</summary>
      <dl>
        <div><dt>{C.scope.technical_repository}</dt><dd>{scope.repository}</dd></div>
        {scope.folder ? <div><dt>{C.scope.technical_folder}</dt><dd>{scope.folder}</dd></div> : null}
      </dl>
    </details> : null}
  </>
}

/**
 * One item: the Change's name, why it is here, where it is, and the one thing
 * to do about it.
 *
 * Exactly one primary action, which is a link to the Change opened at the
 * stage that settles the reason. The other reasons are listed under it as
 * text, not as controls — six things of equal weight is the confusion this
 * screen exists to end (`03-USER-LENS.md` §2).
 */
export function InboxItem({ item }) {
  return <li className="ws-inbox-item" data-change={item.id}>
    <h3 className="ws-inbox-title">{item.title}</h3>
    {/* P6-INBOX-REASON / T-3-4 verifier finding 2 — the reason line is the
      * MODEL's. It used to be `reasonsFor`'s own template, which is why a
      * Change with two interrupted runs read "recording has gaps" while its
      * own Change screen said what actually happened. One derivation, three
      * surfaces (design §3.8). */}
    <p className="ws-inbox-reason">{item.reasonText}</p>
    {/* Where this Change stands, in the one state word the model decided, with
      * the version beside it — the same string the Change screen's line and the
      * packet's line carry for this Change (§3.8). The Inbox constructs no
      * state word of its own; with nothing accepted yet there is no line and
      * the row says only why it is here. */}
    {item.hq && item.hq.inboxRow.line ? <p className="ws-inbox-state ws-cov-line-state">{item.hq.inboxRow.line}</p> : null}
    {/* STEP-7-FIRST-READ (path step 7, PW-F12; founder ruling #21 (a)) — the
      * first place a person reads after the agent has committed. ONE cautious
      * clause, the lead of the moved sentence the Change screen prints
      * (`hq.codeMoved`, from the served `code_moved` this row's own read of
      * `GET /api/changes/<id>` already carries — no further read, no new
      * route). Only when the served field says two different commits; with no
      * field, a null one or equal commits the card says nothing, never
      * "unchanged". The receipt is one click away, on the Change. */}
    {item.hq && item.hq.codeMoved && item.hq.codeMoved.clause ? <p className="ws-inbox-code-moved">{item.hq.codeMoved.clause}</p> : null}
    {/* T-S-4 — whether this has been approved, and on which version, from the
      * same field the Change screen's headline reads. A row that said nothing
      * about a standing approval would send a person into the Change to learn
      * something the Inbox already knows. */}
    {item.hq && item.hq.inboxRow.decision ? <p className="ws-inbox-decision ws-decision-headline">{item.hq.inboxRow.decision}</p> : null}
    {/* A3-F2 — the Change-level count, in its own noun, from the one
      * attention derivation. It is never the Inbox heading's noun: the heading
      * counts Changes, this counts what is on one. */}
    {item.attention ? <p className="ws-inbox-attention">{item.attention.items === 1 ? A.items_one : fill(A.items_many, { n: item.attention.items })}</p> : null}
    <Scope scope={item.scope} />
    {item.also.length ? <>
      <p className="ws-inbox-also-label">{C.also}</p>
      <ul className="ws-inbox-also">{item.also.map((a) => <li key={a.reason.id}>{fill(a.line.template, a.line.values)}</li>)}</ul>
    </> : null}
    {/* A suggestion is a secondary line and never a reason (`B8.8`). It is
      * below the action, so it cannot be read as part of why this item is
      * here. */}
    <a className="ws-inbox-primary" href={item.href}>{item.line.action}</a>
    {item.suggested ? <p className="ws-inbox-suggested">{item.suggested === 1 ? C.suggested_one : fill(C.suggested_many, { n: item.suggested })}</p> : null}
  </li>
}

/**
 * Nothing is tracked. Rule 12: what this place is, why it is empty, and the
 * one thing that fills it (`HQ_COPY.empty.inbox`, T-6-6).
 *
 * T-6-6 — the one primary action is now PICKING A PROJECT (`#/start`), not
 * copying a command. The body already said "Start by picking what you are
 * working on" above a control that copied a terminal command, which is a
 * sentence and a button pointing two different ways. The pick screen is the
 * in-product door (`first-use.jsx`): it proposes a Change from what the record
 * already holds, and writes it only on a click that says so. `POST
 * /api/changes` still answers 405 (`crates/cli/src/uiserve.rs`), so nothing
 * here creates a Change itself.
 *
 * The command stays, as the second way and styled as secondary: it is the
 * route when the project is not one this machine has recorded, and it is
 * `change.wayin.command`, written down in one place.
 */
export function InboxEmpty({ onCopy }) {
  const W = HQ_COPY.change.wayin
  const E = HQ_COPY.empty.inbox
  return <section className="ws-inbox-empty" data-empty="inbox">
    <h2 className="ws-inbox-empty-heading">{C.empty.heading}</h2>
    <p className="ws-inbox-muted">{E.what}</p>
    {/* `C.empty.body` is `E.why` + `E.fill`, composed in the copy file. */}
    <p className="ws-inbox-empty-body">{C.empty.body}</p>
    <a className="ws-inbox-primary" href={START_HASH}>{E.action}</a>
    <p className="ws-inbox-muted">{C.empty.how}</p>
    <p className="ws-inbox-command"><code>{W.command}</code></p>
    <button type="button" className="ws-first-secondary" onClick={() => onCopy && onCopy(W.command)}>{W.copy_command}</button>
  </section>
}

/** Changes are open and none of them is waiting on anything. A different fact. */
export function InboxSettled({ settled, newest }) {
  return <section className="ws-inbox-empty">
    <h2 className="ws-inbox-empty-heading">{C.settled.heading}</h2>
    <p className="ws-inbox-empty-body">{settled === 1 ? C.settled.body_one : fill(C.settled.body_many, { n: settled })}</p>
    {newest ? <a className="ws-inbox-primary" href={changeHrefAt(newest, 'define')}>{C.settled.action}</a> : null}
  </section>
}

/**
 * What this product is keeping on the machine (`L1.1`, rule 13).
 *
 * One sentence, one real number, from the field the server measured. The path
 * and the exact byte count are the technical layer, one click down, never on
 * top (rule 1). It says plainly that nothing here removes any of it: offering
 * a tidy-up that does not exist would be worse than the silence it replaces.
 */
export function Storage({ storage }) {
  const s = storage || { known: false }
  return <section className="ws-inbox-storage">
    <p className="ws-inbox-storage-line">{s.known ? fill(C.storage.sentence, { size: s.size }) : C.storage.unknown}</p>
    <p className="ws-inbox-muted">{C.storage.note}</p>
    {s.known && s.path ? <details className="ws-inbox-technical">
      <summary>{C.storage.technical_open}</summary>
      <dl>
        <div><dt>{C.storage.technical_path}</dt><dd>{s.path}</dd></div>
        {s.also ? <div><dt>{C.storage.technical_also_path}</dt><dd>{s.also}</dd></div> : null}
        {s.alsoCommandLog ? <div><dt>{C.storage.technical_command_log_path}</dt><dd>{s.alsoCommandLog}</dd></div> : null}
        <div><dt>{C.storage.technical_bytes}</dt><dd>{String(s.bytes)}</dd></div>
      </dl>
      {/* What the number is made of (T-2-10, finding 1). The half a person
        * needs when the total grows is which part grew, and on the store the
        * exit check measured that part was the work kept word for word. */}
      {(s.parts || []).length ? <>
        <p className="ws-inbox-storage-parts-label">{C.storage.technical_parts}</p>
        <ul className="ws-inbox-storage-parts">{s.parts.map((p) => <li key={p.key}>{fill(C.storage.part_line, { noun: p.noun, size: p.size })}</li>)}</ul>
      </> : null}
      <p className="ws-inbox-muted">{s.also ? C.storage.technical_note_two : s.alsoCommandLog ? C.storage.technical_note_command_log : C.storage.technical_note}</p>
      <p className="ws-inbox-muted">{C.storage.technical_rounding}</p>
    </details> : null}
  </section>
}

/**
 * One whose-turn section (A3-F14). A section a person can act from states its
 * count and lists its Changes; a section no field of the record can fill yet
 * is drawn disabled, with why and what would turn it on, and holds nothing.
 * "Nothing needed" is collapsed and only counts.
 */
export function InboxSection({ section }) {
  const S = C.sections
  const words = S[section.id]
  const count = section.count == null ? null : section.count === 1 ? S.count_one : fill(S.count_many, { n: section.count })
  if (section.id === 'nothing') {
    return <section className="ws-inbox-section" data-section="nothing">
      <details className="ws-inbox-section-nothing">
        <summary><h2 className="ws-inbox-section-heading">{words.heading}</h2> <span className="ws-inbox-section-count">{count}</span></summary>
        <p className="ws-inbox-muted">{section.count === 1 ? C.settled.body_one : fill(C.settled.body_many, { n: section.count })}</p>
      </details>
    </section>
  }
  if (section.disabled) {
    /* Off, and it says why and what would turn it on — wired by
     * `aria-describedby`, the same rule every off control keeps (A3-F10). */
    const why = 'ws-inbox-section-' + section.id + '-why'
    const enabler = 'ws-inbox-section-' + section.id + '-enabler'
    return <section className="ws-inbox-section ws-inbox-section-off" data-section={section.id} aria-disabled="true" aria-describedby={why + ' ' + enabler}>
      <h2 className="ws-inbox-section-heading">{words.heading}</h2>
      <p className="ws-inbox-section-reason" id={why}>{words.reason}</p>
      <p className="ws-inbox-section-enabler" id={enabler}>{words.enabler}</p>
    </section>
  }
  return <section className="ws-inbox-section ws-inbox-section-items" data-section={section.id}>
    <h2 className="ws-inbox-section-heading">{words.heading}</h2>
    <p className="ws-inbox-section-count">{count}</p>
    <ul className="ws-inbox-list">{section.items.map((item) => <InboxItem key={item.id} item={item} />)}</ul>
  </section>
}

/**
 * The Changes this screen tried to read and could not read in full
 * (SEAM-INBOX-READ-VERIFY item 7; INBOX-READ-FAILURE-NOT-SETTLED).
 *
 * Its own labelled group, never among the settled ones. Each Change is its own
 * line, named by its title — or, when the title itself could not be read, by
 * the first part of its id — with the plain fact that its details could not be
 * read and nothing about it is settled. What the server said is one level
 * down, on the Change it belongs to, never on top (rule 1).
 *
 * Read it again re-reads the whole Inbox through `readInbox`: GET only, no
 * write address. It is the screen's one filled action only when nothing else
 * needs attention (`primary`); beside a Change that does, that Change's own
 * next action stays the one primary (rule 7) — it settles something, and
 * reading again only finds out — and this is drawn secondary.
 */
export function UnreadableGroup({ changes, count, primary, onRetry }) {
  const U = C.unreadable_group
  return <section className="ws-inbox-section ws-inbox-unreadable" data-section="unreadable">
    <h2 className="ws-inbox-section-heading">{U.heading}</h2>
    <p className="ws-inbox-section-count" role="alert">{count === 1 ? C.unreadable_one : fill(C.unreadable_many, { n: count })}</p>
    <ul className="ws-inbox-list">{changes.map((c) => <li key={c.id} className="ws-inbox-item ws-inbox-unreadable-item" data-change={c.id}>
      <h3 className="ws-inbox-title">{c.title || fill(U.untitled, { change: c.id.slice(0, 8) })}</h3>
      <p className="ws-inbox-reason">{U.line}</p>
      {c.notes.length ? <details className="ws-inbox-technical">
        <summary>{U.technical_open}</summary>
        {c.notes.map((n, i) => <p key={i} className="ws-inbox-muted">{n}</p>)}
      </details> : null}
    </li>)}</ul>
    <button type="button" className={primary ? 'ws-inbox-primary ws-inbox-retry' : 'ws-btn-secondary ws-inbox-retry'} onClick={() => onRetry && onRetry()}>{U.action}</button>
  </section>
}

/* ── the screen ───────────────────────────────────────────────────────── */

/**
 * @param {object} props
 * @param {object} props.state what `readInbox` returned
 * @param {(text: string) => void} [props.onCopy]
 * @param {() => void} [props.onRetry] read the Inbox again (GET only)
 */
export function InboxScreen({ state, onCopy, onRetry }) {
  if (state.status === 'loading') {
    return <main className="ws-inbox"><div className="ws-inbox-inner"><p className="ws-inbox-muted" role="status">{C.reading}</p></div></main>
  }
  if (state.status === 'refused') {
    /* Rule 12: a read that failed is said, in the reader's own words, and
     * nothing is claimed about what is or is not waiting. */
    return <main className="ws-inbox"><div className="ws-inbox-inner">
      <h1 className="ws-inbox-heading">{C.title}</h1>
      <p className="ws-inbox-muted">{fill(C.failed, { reason: state.refusal || '' })}</p>
    </div></main>
  }
  const items = state.items || []
  const unread = state.unread || 0
  const unreadable = state.unreadable || 0
  const unreadableChanges = state.unreadableChanges || []
  /* The reader's words about an unreadable Change are on that Change's own
   * line; the list at the foot keeps everything else it said. */
  const placed = new Set(unreadableChanges.flatMap((c) => c.notes))
  const footNotes = (state.notes || []).filter((n) => !placed.has(n))
  return <main className="ws-inbox"><div className="ws-inbox-inner">
    <h1 className="ws-inbox-heading">{C.title}</h1>
    {unreadable ? <UnreadableGroup changes={unreadableChanges} count={unreadable} primary={!items.length} onRetry={onRetry} /> : null}
    {items.length ? <>
      {/* Beside the unreadable group, the Changes read in full get their own
        * label, so two counts of different sets never sit one under the other. */}
      {unreadable ? <h2 className="ws-inbox-section-heading">{C.unreadable_group.attention_heading}</h2> : null}
      <p className="ws-inbox-count">{items.length === 1 ? C.one : fill(C.many, { n: items.length })}</p>
      {/* A3-F14 — by whose turn it is, in the report's fixed order. The
        * sections a person can act from appear when they hold something; the
        * two the record cannot fill yet always appear, disabled; "Nothing
        * needed" appears, collapsed, when something was read and needs nothing. */}
      {inboxSections(items, state.settled).filter((s) => s.disabled || s.count > 0).map((s) => <InboxSection key={s.id} section={s} />)}
    </> : state.total && !unreadable ? <>
      <InboxSettled settled={state.settled} newest={state.newest} />
      {inboxSections([], 0).filter((s) => s.disabled).map((s) => <InboxSection key={s.id} section={s} />)}
    </> : state.total ? <>
      {inboxSections([], state.settled).filter((s) => s.disabled || s.count > 0).map((s) => <InboxSection key={s.id} section={s} />)}
    </> : <InboxEmpty onCopy={onCopy} />}
    {unread ? <p className="ws-inbox-muted">{unread === 1 ? C.unread_one : fill(C.unread_many, { n: unread })}</p> : null}
    <Storage storage={state.storage} />
    {footNotes.length ? <ul className="ws-inbox-notes">{footNotes.map((n, i) => <li key={i}>{n}</li>)}</ul> : null}
  </div></main>
}

/** The same screen, reading for itself. What `main.jsx` mounts. */
export default function Inbox() {
  const [state, setState] = React.useState({ status: 'loading' })
  const live = React.useRef(true)
  /* Read it again (SEAM-INBOX-READ-VERIFY item 7): the same GET-only read the
   * landing makes, again. Nothing is written. */
  const reread = React.useCallback(() => {
    setState({ status: 'loading' })
    readInbox().then((r) => { if (live.current) setState(r) })
  }, [])
  React.useEffect(() => {
    live.current = true
    readInbox().then((r) => { if (live.current) setState(r) })
    return () => { live.current = false }
  }, [])
  /* An item's action is a link to an address this root does not render. The
   * address moving is the click; booting the screen it names is what makes the
   * click do what it looks like it does — and that is `watchAddress`, installed
   * once in `main.jsx` for every screen, not once per screen for one pair of
   * them (T-2-8-F4). This screen no longer watches anything itself. */
  const copy = (text) => { try { navigator.clipboard.writeText(text) } catch { /* nothing else to do */ } }
  return <InboxScreen state={state} onCopy={copy} onRetry={reread} />
}
