import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { Card } from './verdict-panel.jsx'
import { ChangeNotice, CodeLimit, Technical, readChangeDocuments, versionLine } from './change-workspace.jsx'
import { ACCEPTED_AFTER_WORK_SECTION, UNLINKED_RUN_SLUG, deriveChangeModel, linkedWorkLine } from './change-model.js'
import { renderExport } from './packet-export.js'
import { ExportDoor, ExportSheet, readExportRecords } from './export-sheet.jsx'
import { buildPacketView } from './packet-view.js'
import { doorHref } from './door-request.js'
import './workspace.css'

/* The whole story on one page (T-2-5; `B10.1`, `B10.3`–`B10.6`, `B8.10`–`B8.12`,
 * `B7.1`–`B7.4`, `03-USER-LENS.md` rules 1–5 and 12–13, `L1.1`).
 *
 * WHAT THIS PAGE IS. One Change's record, read top to bottom, at
 * `#/changes/<id>/packet`. It answers Sam's SECOND question — "what still
 * needs evidence, and what's actually wrong?" — from this page alone, with
 * the technical layer closed. So the order is: the one conclusion sentence,
 * the one named version, the still-needs-evidence list, and only then the
 * twelve sections of the record in the brief's own order.
 *
 * WHAT IT MAY DO. Read. It calls `readChangeDocuments`, which is the Change
 * screen's own read loop over the same GET routes, and `readExportRecords`
 * (T-5-4), one more GET; there is no POST here and no write route in the map
 * it reads from. Nothing on it sends anything anywhere. Its one door out is
 * T-5-4's: "Save the whole story as a file" opens the sheet
 * (`export-sheet.jsx`), which shows the exact text first and saves it only
 * when asked — on this Mac, through the browser, never through the server.
 *
 * WHAT IT MAY SAY. Every string comes from `HQ_COPY.change.packet`, with the
 * same two disclosed exceptions the Change screen has: the Change's own title,
 * request and criteria, which are the RECORD speaking, and the finding cards,
 * which render through `verdict-model.js` and its own copy families unchanged.
 *
 * WHAT IT REFUSES TO SAY. No state word in any heading. No percentage
 * anywhere. Nothing green and no colour carrying meaning alone. A section
 * with nothing in it says so in words, and is never a tick or a blank. A
 * criterion with nothing behind it says "No test ran for this." and is never
 * read as a pass. Freshness says unknown, never yes.
 */

const C = HQ_COPY.change
const P = C.packet

/* ── reading ──────────────────────────────────────────────────────────── */

/**
 * Everything this page needs, read once, through the Change screen's reader.
 *
 * @param {string} id
 * @returns {Promise<{status: string, model?: object, refusal?: string, notes: string[]}>}
 */
export async function readPacket(id) {
  /* T6 — with the run sheet's typed counts, so this page and the exported
   * file name a failing run's counts exactly as the Change screen does. */
  const read = await readChangeDocuments(id, { runs: true })
  if (read.status !== 'ready') return read
  const hq = deriveChangeModel(read.change, read.coverage, read.applicability, read.proposals, read.work)
  /* T-5-4 — whether the records the file rests on can be read now. Read ONCE,
   * here, so Copy as Markdown and the export sheet render the same records and
   * so the same Markdown text (§A.2). A failed read is said aloud, and the
   * file then says the records were not checked. */
  const { records, note } = await readExportRecords(id)
  return {
    status: 'ready',
    records,
    model: {
      ...buildPacketView({ change: read.change, proposals: read.proposals, work: read.work, coverage: read.coverage, evidence: [], hq }),
      /* T-S-3 — the ONE derived view-model, the same one the Change screen and
       * the Inbox row read, from the same documents. The packet may not decide
       * a state word or a version of its own. */
      hq,
    },
    notes: note ? [...read.notes, note] : read.notes,
  }
}

/* ── the pieces ───────────────────────────────────────────────────────── */

/**
 * One piece of evidence, with the two things it may never be without: the
 * word for where it came from, and the address of the record it came from.
 *
 * There is no branch here that renders an item lacking either. An item that
 * cannot carry both never becomes an item — `evidenceItem` refuses it in the
 * model, and `<Refused>` below says how many were refused. A label that could
 * be absent is a label a reader cannot rely on.
 */
export function EvidenceItem({ item, children }) {
  return <div className="ws-packet-item">
    {children}
    <p className="ws-packet-provenance">
      <span className="ws-packet-provenance-label">{P.provenance.label}:</span>{' '}
      <span className="ws-packet-provenance-word">{item.provenanceWord}</span>
      {' · '}
      <a className="ws-packet-record" href={item.record.href}>{item.record.link}</a>
      {' — '}
      <span className="ws-packet-where">{item.record.where}</span>
    </p>
  </div>
}

/** What could not be shown, and why. Never silent (Rule 12). */
export function Refused({ n }) {
  if (!n) return null
  return <p className="ws-packet-refused">{n === 1 ? P.record.refused_one : fill(P.record.refused_many, { n })}</p>
}

/** A section with nothing in it says what that means. Never a blank, never a tick. */
function Empty({ text }) {
  return <p className="ws-change-empty ws-packet-empty">{text}</p>
}

/* ── the twelve sections ──────────────────────────────────────────────── */

function Purpose({ model }) {
  return <>
    <p className="ws-change-request"><q className="ws-packet-quote">{model.request.purpose || model.request.title}</q></p>
    {model.request.requester ? <p className="ws-change-muted">{fill(C.define.requester, { who: model.request.requester })}</p> : null}
  </>
}

/** The rows of the "accepted after the work" section of the one derived array, or none. */
function acceptedAfterWorkRows(model) {
  const sections = (model.hq && model.hq.packet && model.hq.packet.sections) || []
  const section = sections.find((s) => s && s.id === ACCEPTED_AFTER_WORK_SECTION)
  return section && Array.isArray(section.rows) ? section.rows : []
}

/**
 * What counts as done, and what is only suggested.
 *
 * The two blocks are about DIFFERENT lines, never the same line twice. The
 * suggestions route re-derives its list on every read, so on an accepted
 * Change it hands back the accepted lines as well; `model.unaccepted` is the
 * part of that list nobody has accepted word for word. Rendering the raw list
 * here put "Accepted at version 1 by sam" and "Suggested, and not accepted —
 * nothing counts until somebody accepts it" on the same sentence, four lines
 * apart (T-2-5 verifier round 0, found on a real Change).
 */
function Accepted({ model }) {
  const a = model.accepted
  const acceptance = model.acceptance
  const unaccepted = model.unaccepted || []
  return <>
    {a.criteria.length ? <>
      <ol className="ws-change-criteria">{a.criteria.map((t, i) => <li key={i}>{t}</li>)}</ol>
      <p className="ws-change-muted">{
        acceptance && acceptance.actor && acceptance.date
          ? fill(P.accepted.note_full, { list: a.version == null ? 0 : a.version, who: acceptance.actor, date: acceptance.date })
          : acceptance && acceptance.actor
            ? fill(P.accepted.note_undated, { list: a.version == null ? 0 : a.version, who: acceptance.actor })
            : fill(P.accepted.note_unattributed, { list: a.version == null ? 0 : a.version })
      }</p>
      {/* QUAL-F2-R3 (`Q7`): accepted after the work was done — the row of the
        * ONE section of `hq.packet.sections` that says so, the very row every
        * exported file prints (`change-model.js` `sectionsOf`). This component
        * words nothing of its own; a page with no derived view-model has no
        * section and says nothing. `#ws-packet-accepted` is where the
        * qualification case reads it. */}
      {acceptedAfterWorkRows(model).map((r, i) => <p key={i} className="ws-change-muted">{r.text}</p>)}
      <p className="ws-change-muted">{acceptance && acceptance.why ? fill(P.accepted.why, { why: acceptance.why }) : P.accepted.why_none}</p>
      <h3>{P.accepted.exclusions_heading}</h3>
      {model.exclusions.length
        ? <ul className="ws-change-criteria">{model.exclusions.map((t, i) => <li key={i}>{t}</li>)}</ul>
        : <p className="ws-change-muted">{P.accepted.exclusions_none}</p>}
    </> : null}
    {unaccepted.length ? <>
      <h3>{P.accepted.proposed_heading}</h3>
      <p className="ws-change-muted">{a.criteria.length ? P.accepted.proposed_note : P.accepted.unaccepted}</p>
      <ol className="ws-change-criteria">{unaccepted.map((p, i) => <li key={i}>{p.text}</li>)}</ol>
    </> : null}
  </>
}

function Target({ model }) {
  const p = model.prepare || {}
  const rows = [
    [C.prepare.repository, p.repository],
    [C.prepare.base, p.base],
    [C.prepare.snapshot, model.snapshot],
    [C.prepare.workspace, p.workspace],
  ]
  return <>
    <p className="ws-change-request">{versionLine(model.version)}</p>
    <dl className="ws-change-facts">{rows.map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v || C.prepare.none_recorded}</dd></div>)}</dl>
    <p className="ws-change-muted">{P.target.note}</p>
  </>
}

function Changed({ model }) {
  return <ul className="ws-change-diff">{model.build.map((d, i) => <li key={i}>{d}</li>)}</ul>
}

/**
 * One line per thing that counts as done, and under it what was looked at.
 *
 * A row with nothing under it renders "No test ran for this." — the sentence
 * `03-USER-LENS.md` rule 5 asks for by name. It is not a failure and not a
 * pass; it is the record being empty, said in Sam's words.
 */
/**
 * RUN-LINK-INSPECTOR (PW-F10) — on the whole-story page, on screen only, a
 * sentence that says a test ran and nothing links it is followed by one door
 * to that line's link control (the same words the run sheet's door carries).
 * The exported Markdown, HTML and JSON are made from `hq.packet` and never pass
 * through this component: they keep the plain sentence, byte for byte.
 *
 * PATH-UI-2-R2 — the door names the line it opens. A plain link to the Verify
 * stage loaded the Change screen on Brief, which draws no stage, so the person
 * arrived with no line open and no name box and needed a second press. This
 * page is its own root, so the request rides in the address (`door-request.js`)
 * and the Change screen lands on the line and on its name box in the one act.
 */
function LinkDoor({ id, n, text }) {
  if (!id) return null
  const search = typeof window === 'undefined' || !window.location ? '' : window.location.search
  return <>{' '}<a className="ws-change-secondary ws-packet-link-door" href={doorHref(id, n, search)}>{fill(HQ_COPY.run.outcomes.passed.unlinked_door, { line: text || '' })}</a></>
}

function Mapping({ model }) {
  return <>
    <p className="ws-change-muted">{P.mapping.note}</p>
    <ol className="ws-packet-mapping">{model.mapping.rows.map((row) => <li key={row.n}>
      <p className="ws-packet-criterion">{row.text}</p>
      {/* The line's own sentence, from the served state. A line with nothing
        * behind it says "No test ran for this."; a line whose evidence settled
        * nothing says so and still shows the run. Neither is a pass and
        * neither is a failure. */}
      <p className="ws-packet-nothing">{row.sentence || (row.nothing ? P.mapping.nothing_for_it : '')}{row.state === 'unlinked_run' ? <LinkDoor id={model.id} n={row.n} text={row.text} /> : null}</p>
      {row.items.map((item, i) => <EvidenceItem key={i} item={item}>
        {item.summary ? <p className="ws-packet-summary">{item.summary}</p> : null}
        {item.why ? <p className="ws-packet-why">{fill(P.mapping.why, { why: item.why })}</p> : null}
        {item.note ? <p className="ws-packet-note">{item.note}</p> : null}
        {/* T-3-2 — which version this was examined on. Printed either way: a
          * packet that quietly dropped the results from another version would
          * be hiding exactly what a reader needs to see. */}
        {item.appliesText ? <p className="ws-packet-applies">{item.appliesText}</p> : null}
        {item.state && HQ_COPY.states[item.state] ? <p className="ws-packet-state">{HQ_COPY.states[item.state].word} — {HQ_COPY.states[item.state].subtitle}</p> : null}
      </EvidenceItem>)}
      {(row.removed || []).map((r, i) => <p key={'r' + i} className="ws-packet-removed">{fill(P.mapping.removed, { who: r.who })} {r.summary || ''}</p>)}
    </li>)}</ol>
    <p className="ws-change-muted">{P.limits.no_link_between}</p>
    <Refused n={model.mapping.refused} />
  </>
}

/* `linkedWorkLine` is `change-model.js`'s, the same function the Change
 * screen's Verify view calls — one Change reads the same way on both surfaces
 * (T-2-8-F5): the unrecorded-work sentence where the record does not hold what
 * the agent said (UNRECORDED-WORK-NOT-SILENT), otherwise `change-view.js`
 * `nothingWrongLine`, counting the pieces of work the footer counts (PW-F13). */

function Examination({ model, onCopy }) {
  const left = model.scope.suggested.length
  return <>
    {/* PACKET-CHECKED-TRUE — the linked runs first, as "Every run" prints
      * them, word for word (`examinedRuns`). */}
    {(model.checks || []).length
      ? <>
        <p className="ws-change-muted">{P.checks.lead}</p>
        <ul className="ws-packet-rows ws-packet-checks">{model.checks.map((r, n) => <li key={n}>
          <span className="ws-packet-row-text">{r.text}</span>
          {r.lines.length ? <ul className="ws-packet-row-lines">{r.lines.map((l, i) => <li key={i}>{l}</li>)}</ul> : null}
        </li>)}</ul>
      </>
      : (model.recorded || []).length
        /* No run, and the mapping shows recorded evidence: say what it is,
         * in the mapping section's own words, never "nothing was looked at". */
        ? <>
          <p className="ws-change-muted">{P.checks.recorded_lead}</p>
          <ul className="ws-packet-rows ws-packet-recorded">{model.recorded.map((r) => <li key={r.n}>
            <span className="ws-packet-row-text">{r.text}</span>
            <ul className="ws-packet-row-lines">
              {r.sentence ? <li>{r.sentence}</li> : null}
              {r.names.map((name, i) => <li key={i}>{name}</li>)}
            </ul>
          </li>)}</ul>
        </>
        : null}
    {/* A run nobody linked is on record and is not a check of this Change:
      * it is said, under words that say so, and never in the list above. */}
    {(model.unlinkedRuns || []).length
      ? <>
        <p className="ws-change-muted">{P.checks.unlinked_lead}</p>
        <ul className="ws-packet-rows ws-packet-unlinked">{model.unlinkedRuns.map((r, n) => <li key={n}>
          <span className="ws-packet-row-text">{r.text}</span>
          {r.lines.length ? <ul className="ws-packet-row-lines">{r.lines.map((l, i) => <li key={i}>{l}</li>)}</ul> : null}
        </li>)}</ul>
      </>
      : null}
    <p className="ws-change-muted">{C.verify.scope_note}</p>
    {model.evidence.length
      ? <div className="ws-change-cards">{model.evidence.map((item, i) => <EvidenceItem key={i} item={item}>
        <Card
          card={item.card}
          primary={false}
          register="manager"
          /* Same reason as the Change screen: the card's own chips are record
           * ids, which are Trace's words, and there is no proof viewer on this
           * page for them to open. The record address is on the item below. */
          receipts="off"
          onCopy={onCopy}
        />
      </EvidenceItem>)}</div>
      : <p className="ws-change-muted">{linkedWorkLine(model)}</p>}
    <h3>{P.limits.heading}</h3>
    <p className="ws-change-muted">{left
      ? (left === 1 ? P.limits.suggested_one : fill(P.limits.suggested_many, { n: left }))
      : P.limits.none}</p>
    <Refused n={model.refused - model.mapping.refused} />
  </>
}

/**
 * The approval history (T-S-4-F2): every act, in the order it was recorded,
 * from the one derived view-model. The packet states no decision of its own and
 * derives no state word — `model.hq.decision` is the same field the Change
 * screen's headline and the Inbox row read, so the three cannot disagree about
 * what was approved or on which version.
 */
export function Decisions({ model }) {
  const decision = model.hq && model.hq.decision ? model.hq.decision : null
  if (!decision) return null
  return <>
    {decision.approved ? <p className="ws-decision-headline">{decision.headline}</p> : null}
    {/* T7 — the list the approval was given against, beside the one accepted
      * now: the same sentence the Change screen prints. */}
    {decision.listMovedLine ? <p className="ws-change-changed-since">{decision.listMovedLine}</p> : null}
    {/* T-5-3 (`B7.20`, `B10.9`) — the packet a person takes away carries the
      * exceptions the standing approval was given under, in the screen's own
      * words and from the approval's own rows. An approval with something set
      * aside is never rendered here as an ordinary one. */}
    {(decision.exceptions || []).length
      ? <>
        <p className="ws-packet-exceptions-heading">{decision.exceptionsHeading}</p>
        <ul className="ws-packet-exceptions">
          {(decision.exceptions || []).map((e) => <li key={e.obligationId}>{e.line}</li>)}
        </ul>
      </>
      : null}
    <p className="ws-change-muted">{decision.sentence}</p>
    <ul className="ws-packet-decisions">
      {decision.history.map((h, n) => <li key={n}>{h.line}</li>)}
    </ul>
  </>
}

/**
 * T6 — what happened after each line was raised: the SAME rows the Change
 * screen shows and the exported file carries (`hq.packet.repairs`, built once
 * by `change-model.js` `repairHistoryOf`). This section only renders them.
 */
function Repairs({ model }) {
  const rows = (model.hq && model.hq.packet && model.hq.packet.repairs) || []
  return <ul className="ws-packet-rows ws-packet-repairs">{rows.map((r) => <li key={r.n}>
    <span className="ws-packet-row-text">{r.text}</span>
    <ul className="ws-packet-row-lines"><li data-kind={r.kind}>{r.sentence}</li></ul>
  </li>)}</ul>
}

const BODIES = {
  purpose: Purpose,
  accepted: Accepted,
  target: Target,
  changed: Changed,
  mapping: Mapping,
  examination: Examination,
  repairs: Repairs,
  decisions: Decisions,
}

/* ── the page ─────────────────────────────────────────────────────────── */

/** What to read first, at the top, before any of the record. */
export function StillNeedsEvidence({ model }) {
  /* P6B-HTT-4 — the heading is the body's own: an empty case wears the empty
   * case's heading, never the question its first sentence says does not arise. */
  /* BRIEF-COUNT-AGREES-R2 — with lines set aside and nothing waiting, "has
   * something behind it" would be said of lines nobody checked. The count is
   * the served answer's (`model.hq.readiness.tally`), the same one the top, the
   * Change screen's list and Ask read. BRIEF-COUNT-AGREES-R3 — a review a person
   * set aside is one of them: "has something behind it" is not said over it. */
  const tally = model.hq && model.hq.readiness ? model.hq.readiness.tally : null
  const aside = tally ? tally.setAside.length + (tally.setAsideApart || []).length : 0
  const none = aside ? { heading: P.needs.none_set_aside_heading, body: P.needs.none_set_aside } : { heading: P.needs.none_heading, body: P.needs.none }
  const heading = model.accepted.criteria.length === 0
    ? P.needs.no_criteria_heading
    : model.needs.length === 0 ? none.heading : P.needs.heading
  return <section className="ws-packet-needs">
    <h2>{heading}</h2>
    {model.accepted.criteria.length === 0
      ? <Empty text={P.needs.no_criteria} />
      : model.needs.length === 0
        ? <p className="ws-change-muted">{none.body}</p>
        : <>
          <p className="ws-change-muted">{P.needs.note}</p>
          <ul className="ws-packet-needs-list">{model.needs.map((n, at) => <li key={n.n ?? 'at' + at}>
            <span className="ws-packet-criterion">{n.text}</span>{' — '}<span className="ws-packet-nothing">{n.why}</span>
            {/* The door to link a run belongs to a line the answer says has one nobody linked; with no answer, to a line the coverage says the same of. */}
            {n.n != null && (n.slug ? n.slug === UNLINKED_RUN_SLUG : (model.mapping.rows || []).some((r) => r.n === n.n && r.state === 'unlinked_run')) ? <LinkDoor id={model.id} n={n.n} text={n.text} /> : null}
          </li>)}</ul>
        </>}
    {/* The other half of the question, in the same breath. The walk found this
      * forty lines below, where ten seconds of reading never reaches it. */}
    {model.wrong.length ? <>
      <h3>{P.needs.wrong_heading}</h3>
      <p className="ws-change-muted">{P.needs.wrong_note}</p>
      {model.wrong.map((item, i) => <EvidenceItem key={i} item={item}>
        <p className="ws-packet-wrong">{item.card && item.card.record ? item.card.record.text : ''}</p>
      </EvidenceItem>)}
    </> : null}
  </section>
}

/**
 * The §3.6 record, rendered from the one derived array (T-S-5).
 *
 * This component decides NOTHING: not a heading, not a row, not an order. It
 * maps `hq.packet.sections`, which `packetMarkdown` walks in the same order,
 * so what a person reads on the page and what they paste elsewhere cannot be
 * two different stories. A section with no rows says what the record's silence
 * means, in words — never a blank and never a tick.
 */
export function RecordSections({ sections }) {
  /* `decisions` is in the array — `packetMarkdown` needs it — but it is
   * RENDERED by the section of the same heading in the twelve below, which was
   * built for it in T-S-4 and carries the standing decision as well as the
   * acts. Two headings reading "Decision history" would be one story told
   * twice; the array stays single, and the renderer names its owner.
   *
   * The same holds for the "accepted after the work" caution (QUAL-F2-R3): it
   * is in the array, so every file carries it, and it is RENDERED by the
   * accepted section below, under the list it is about — where the
   * qualification case reads it — and not a second time here. */
  return <>{(sections || []).filter((s) => s.id !== 'decisions' && s.id !== ACCEPTED_AFTER_WORK_SECTION).map((s) => <section className="ws-packet-section ws-packet-36" key={s.id} id={'ws-packet-36-' + s.id}>
    <h2>{s.title}</h2>
    {s.rows.length
      ? <ul className="ws-packet-rows">{s.rows.map((r, i) => <li key={i}>
        <span className="ws-packet-row-text">{r.text}</span>
        {r.lines.length ? <ul className="ws-packet-row-lines">{r.lines.map((l, j) => <li key={j}>{l}</li>)}</ul> : null}
        {/* UNRECORDED-WORK-NO-QUOTE — where the agent said it (log file and
          * line), one level down: folded away, never on the plain face. */}
        {Array.isArray(r.receipts) && r.receipts.length
          ? <details className="ws-log-receipt">
            <summary>{HQ_COPY.change.unrecorded.receipt_summary}</summary>
            <ul>{r.receipts.map((x, j) => <li key={j}><code>{x}</code></li>)}</ul>
          </details>
          : null}
      </li>)}</ul>
      : <Empty text={s.none} />}
  </section>)}</>
}

/**
 * Take this away (T-S-5; design §3.6).
 *
 * T-5-4 §A.2: the text is the EXPORT's Markdown — `renderExport(hq.packet,
 * records, {format: 'md'}).text` — the same string the export sheet previews
 * and saves for that format, so one Change has one Markdown text wherever a
 * person takes it from. `packetMarkdown` no longer reaches a person; its
 * retirement is `docs/loop/export/INTERFACE-NEEDS.md` item 1. This component
 * still never assembles a sentence. There is no library and no file:
 * `navigator.clipboard` where it exists, and where it does not (headless, or
 * any page not served over a secure origin) the text itself in a textarea,
 * said out loud rather than a button that reports success it did not have.
 */
export function copiedMarkdown(hq, records) {
  return hq ? renderExport(hq.packet, records || null, { format: 'md' }).text : ''
}

export function CopyAsMarkdown({ hq, records }) {
  const [state, setState] = React.useState('idle')
  const C2 = P.copy
  if (!hq) return null
  const text = copiedMarkdown(hq, records)
  const copy = () => {
    try {
      const clip = typeof navigator === 'undefined' ? null : navigator.clipboard
      if (!clip || typeof clip.writeText !== 'function') { setState('fallback'); return }
      Promise.resolve(clip.writeText(text)).then(() => setState('done'), () => setState('fallback'))
    } catch { setState('fallback') }
  }
  return <div className="ws-packet-copy">
    <button type="button" className="ws-change-secondary" onClick={copy}>{C2.label}</button>
    <p className="ws-change-muted">{C.export.copy_note}</p>
    {state === 'done' ? <p className="ws-packet-copied" role="status">{C2.done}</p> : null}
    {state === 'fallback' ? <>
      <p className="ws-packet-copy-fallback" role="status">{C2.fallback}</p>
      <textarea className="ws-packet-copy-text" readOnly rows={12} value={text} />
    </> : null}
  </div>
}

/**
 * @param {object} props
 * @param {object} props.model what `buildPacketView` decided
 * @param {string[]} [props.notes] anything that could not be read, said aloud
 */
export function PacketPage({ model, notes, onCopy, records, sheetOpen }) {
  const [sheet, setSheet] = React.useState(Boolean(sheetOpen))
  /* Closing the sheet puts focus back on the door that opened it. */
  const returning = React.useRef(false)
  React.useEffect(() => {
    if (sheet || !returning.current || typeof document === 'undefined') return
    returning.current = false
    const back = document.querySelector('.ws-export-open')
    if (back) back.focus()
  }, [sheet])
  /* T-5-4 — the sheet is its own screen: while it is open it is the only
   * thing here, so it carries the one primary ("Save this file") and the
   * page's own primary is not beside it (lens rule 7). */
  if (sheet && model.hq) {
    return <main className="ws-change ws-packet ws-export"><div className="ws-change-inner">
      <ExportSheet packet={model.hq.packet} records={records || null} onClose={() => { returning.current = true; setSheet(false) }} />
    </div></main>
  }
  return <main className="ws-change ws-packet"><div className="ws-change-inner">
    <p className="ws-change-kicker">{C.kicker}</p>
    <h1 className="ws-change-title">{model.request.title || ''}</h1>
    <p className="ws-change-conclusion">{fill(C.conclusion[model.conclusion.key], model.conclusion.values)}</p>
    <p className="ws-change-version" aria-label={C.version.label}>{versionLine(model.version)}</p>
    <p className="ws-change-muted ws-packet-lead">{P.lead}</p>
    {/* Every accepted line and where it stands, from the one derived model —
      * the same state word and the same version the Change screen and the
      * Inbox row show for it (§3.8). The packet constructs none of them. */}
    {model.hq ? <p className="ws-change-muted ws-packet-covers">{model.hq.packet.coversLine}</p> : null}
    {/* T-3-5 — is this ready, and what does that rest on: the same sentence
      * and the same list the Change screen shows, from the same field. The
      * list is in the same block as the sentence, always, so the claim and
      * what it stands on cannot be read apart. */}
    {model.hq && model.hq.packet.readinessLine
      ? <section className="ws-packet-readiness">
        {/* RUN-LINK-TRUTH-R5 — the one blocker first, as every surface on the Change screen says it. */}
        {model.hq.packet.readinessBlocker ? <p className="ws-packet-readiness-blocker">{model.hq.packet.readinessBlocker}</p> : null}
        <p className="ws-packet-readiness-line">{model.hq.packet.readinessLine}</p>
        {(model.hq.packet.readinessRests || []).length
          ? <ul className="ws-packet-readiness-rests">
            {(model.hq.packet.readinessRests || []).map((r, n) => <li key={n}>{r}</li>)}
          </ul>
          : null}
        {/* BRIEF-COUNT-AGREES-R2 — what is missing that is not one of the lines, on its own; and the lines set aside, under their own heading and never in the list above. */}
        {(model.hq.packet.readinessAlso || []).map((r, n) => <p key={'also' + n} className="ws-packet-readiness-also">{r}</p>)}
        {model.hq.packet.readinessSetAsideHeading ? <p className="ws-packet-readiness-set-aside-heading">{model.hq.packet.readinessSetAsideHeading}</p> : null}
        {(model.hq.packet.readinessSetAside || []).length
          ? <ul className="ws-packet-readiness-set-aside">
            {(model.hq.packet.readinessSetAside || []).map((r, n) => <li key={n}>{r}</li>)}
          </ul>
          : null}
      </section>
      : null}
    <StillNeedsEvidence model={model} />
    {/* The §3.6 record, from the ONE `hq.packet.sections` array that
      * `packetMarkdown` walks (T-S-5 verifier finding 1). Before this the page
      * rendered twelve sections of its own and the copied text seven of the
      * model's, so "every run with its version" and "what was not recorded"
      * were in the clipboard and nowhere on the page. There is one array now. */}
    {model.hq ? <RecordSections sections={model.hq.packet.sections} /> : null}
    {model.sections.map((s) => {
      const Body = BODIES[s.id]
      return <section className="ws-packet-section" key={s.id} id={'ws-packet-' + s.id}>
        <h2>{s.heading}</h2>
        {s.filled && Body ? <Body model={model} onCopy={onCopy} /> : <Empty text={s.empty} />}
        {/* STEP-7-LIMITS — "What changed" says what the record cannot see. */}
        {s.id === 'changed' && model.hq ? <CodeLimit limit={model.hq.codeLimit} /> : null}
      </section>
    })}
    {(notes || []).length ? <ul className="ws-change-notes">{(notes || []).map((n, i) => <li key={i}>{n}</li>)}</ul> : null}
    <footer className="ws-packet-footer">
      <p>{P.footer.lead}</p>
      <p className="ws-change-muted">{model.footer.text}</p>
    </footer>
    {/* The one next action of this page, and it is the way back to where the
      * actions are. Below it, T-5-4's door: the control that used to be off
      * ("Send this to somebody else", Phase 5) is now the way to the file,
      * secondary, and it saves nothing until the text has been shown. */}
    <p className="ws-packet-actions">
      <a className="ws-change-primary ws-packet-back" href={'#/changes/' + encodeURIComponent(model.id || '')}>{P.back}</a>
    </p>
    <CopyAsMarkdown hq={model.hq} records={records} />
    {model.hq ? <ExportDoor onOpen={() => setSheet(true)} /> : null}
    <Technical model={model} />
  </div></main>
}

/** The id in `#/changes/<id>/packet`, or null. Nothing else on this page reads the address, but for the query a door carries forward (`LinkDoor`). */
export function packetIdFromHash(hash) {
  const m = /^#\/changes\/([A-Za-z0-9_-]{1,128})\/packet$/.exec(hash || '')
  return m ? { id: m[1] } : null
}

export default function PacketView({ id }) {
  const [state, setState] = React.useState({ status: 'loading' })
  React.useEffect(() => {
    let live = true
    readPacket(id).then((r) => { if (live) setState(r) })
    return () => { live = false }
  }, [id])
  /* The way back is a link to a different root (`main.jsx`), so the address
   * moving is not enough — the screen it names has to be booted, or the click
   * reads as having done nothing. That is `watchAddress`, installed once in
   * `main.jsx` for every screen (T-2-8-F4); this page watches nothing itself. */
  const copy = (text) => { try { navigator.clipboard.writeText(text) } catch { /* nothing else to do */ } }
  if (state.status !== 'ready') return <ChangeNotice status={state.status} refusal={state.refusal} />
  return <PacketPage model={state.model} notes={state.notes} onCopy={copy} records={state.records} />
}
