import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { readProjection } from './data.js'
import { CLASSES, FORMATS, renderExport, sha256Hex } from './packet-export.js'

/* The whole story, as a file — the sheet (T-5-4; `B10.14`, `B12.2`, lens
 * rules 6, 7, 11).
 *
 * WHAT IT SHOWS. The exact text the file will hold, in a `<pre>`, for every
 * format; for the page format, the same string rendered in a sandboxed frame
 * (`sandbox=""`: no script, no navigation, no network from inside it). The
 * text shown and the text saved are ONE value — `renderExport`'s `text` — so
 * the preview cannot drift from the file (`B10.14`: "preview what leaves").
 *
 * WHAT IT DOES. One primary act, "Save this file", which is separate from and
 * after the preview (`B12.2`). It hands the previewed string to the browser as
 * a download. Nothing is POSTed; there is no write address here, and the
 * server writes nothing. The one read is `GET /api/changes/<id>/export`,
 * declared below by name.
 *
 * WHAT IT SAYS. Every word is `HQ_COPY.change.export`. The engine's noun for
 * the file is in its NAME, which sits in the technical details, and nowhere
 * a person reads it first.
 */

const X = HQ_COPY.change.export
/* One choice per line. The sheet's layout lives here, inline, because the
 * stylesheet is not this task's to change; nothing here carries meaning by
 * colour. */
const ROW = { display: 'block', margin: '0.35em 0' }

/** The one read this sheet adds: whether the records the file rests on can be read now. */
export const CHANGE_EXPORT = (id) => '/api/changes/' + encodeURIComponent(id) + '/export'

/**
 * @param {string} id
 * @returns {Promise<{records: object|null, note: string|null}>} a failed read
 *   is `records: null` and the file then says the records were not checked —
 *   never that they were readable.
 */
export async function readExportRecords(id) {
  const read = await readProjection(CHANGE_EXPORT(id))
  if (read.status === 'ready' && read.doc) return { records: read.doc, note: null }
  return { records: null, note: read.status === 'refused' ? read.refusal : null }
}

/**
 * Hand exactly `text` to the browser as a file named `filename`. The Blob is
 * made from the same string the sheet shows; nothing else is written.
 */
export function saveText({ text, filename, mime }) {
  const blob = new Blob([text], { type: mime })
  const href = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = href
  a.download = filename
  a.rel = 'noopener'
  document.body.appendChild(a)
  a.click()
  a.remove()
  window.setTimeout(() => URL.revokeObjectURL(href), 1000)
}

/** The door, on the whole-story page. Secondary: the page's one primary is the way back. */
export function ExportDoor({ onOpen }) {
  return <div className="ws-export-door">
    <button type="button" className="ws-change-secondary ws-export-open" onClick={onOpen}>{X.door}</button>
    <p className="ws-change-muted">{X.door_note}</p>
  </div>
}

/**
 * @param {object} props
 * @param {object} props.packet `hq.packet`
 * @param {object|null} props.records the served `/export` document
 * @param {() => void} props.onClose
 * @param {string} [props.initialFormat]
 * @param {string[]} [props.initialLeaveOut]
 * @param {(file: object) => void} [props.save] for tests; the browser's download otherwise
 */
export function ExportSheet({ packet, records, onClose, initialFormat, initialLeaveOut, save }) {
  const [format, setFormat] = React.useState(FORMATS.includes(initialFormat) ? initialFormat : 'html')
  const [leaveOut, setLeaveOut] = React.useState(Array.isArray(initialLeaveOut) ? initialLeaveOut : [])
  const [status, setStatus] = React.useState(null)
  const heading = React.useRef(null)
  React.useEffect(() => { if (heading.current) heading.current.focus() }, [])

  const file = renderExport(packet, records, { format, redact: leaveOut })
  const toggle = (c) => {
    setStatus(null)
    setLeaveOut((now) => (now.includes(c) ? now.filter((x) => x !== c) : CLASSES.filter((k) => k === c || now.includes(k))))
  }
  const doSave = () => {
    try {
      ;(save || saveText)(file)
      setStatus({ ok: true, text: X.saved })
    } catch {
      setStatus({ ok: false, text: X.save_failed })
    }
  }

  return <section className="ws-export-sheet" aria-labelledby="ws-export-heading">
    <h2 id="ws-export-heading" tabIndex={-1} ref={heading}>{X.heading}</h2>
    <p className="ws-export-lead">{X.lead}</p>

    <fieldset className="ws-export-formats">
      <legend>{X.format_legend}</legend>
      {FORMATS.map((f) => <label key={f} className="ws-export-format" style={ROW}>
        <input type="radio" name="ws-export-format" value={f} checked={format === f} onChange={() => { setStatus(null); setFormat(f) }} />
        {' '}{X.formats[f].label}
        <span className="ws-change-muted"> — {X.formats[f].note}</span>
      </label>)}
    </fieldset>

    <fieldset className="ws-export-leave-out">
      <legend>{X.leave_out_legend}</legend>
      <p className="ws-change-muted">{X.leave_out_note}</p>
      {CLASSES.map((c) => <label key={c} className="ws-export-class" style={ROW}>
        <input type="checkbox" name={'ws-export-leave-out-' + c} value={c} checked={leaveOut.includes(c)} onChange={() => toggle(c)} />
        {' '}{X.classes[c]}
      </label>)}
    </fieldset>

    <h3>{X.preview_heading}</h3>
    {format === 'html' ? <>
      <p className="ws-change-muted">{X.preview_rendered}</p>
      <iframe
        className="ws-export-frame"
        title={X.preview_rendered}
        sandbox=""
        srcDoc={file.text}
        style={{ width: '100%', height: '22em', border: '1px solid currentColor' }}
      />
    </> : null}
    <p className="ws-change-muted">{X.preview_text}</p>
    <pre
      className="ws-export-preview"
      data-export-format={format}
      data-export-filename={file.filename}
      tabIndex={0}
      style={{ maxHeight: '22em', overflow: 'auto', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}
    >{file.text}</pre>

    <p className="ws-export-actions">
      <button type="button" className="ws-change-primary ws-export-save" onClick={doSave}>{X.save}</button>
    </p>
    {status ? <p className={status.ok ? 'ws-export-saved' : 'ws-export-failed'} role="status">{status.text}</p> : null}
    <p><button type="button" className="ws-change-secondary ws-export-close" onClick={onClose}>{X.close}</button></p>

    <details className="ws-change-technical ws-export-technical">
      <summary>{X.technical.open}</summary>
      <dl className="ws-change-facts">
        <div><dt>{X.technical.filename}</dt><dd className="ws-export-filename">{file.filename}</dd></div>
        <div><dt>{X.technical.format}</dt><dd>{file.mime}</dd></div>
        <div><dt>{X.technical.fingerprint}</dt><dd className="ws-export-sha256">{sha256Hex(file.text)}</dd></div>
        {file.redaction.redacted ? <div><dt>{X.technical.derived_from}</dt><dd>{file.redaction.derived_from_sha256}</dd></div> : null}
        {records && records.masking && records.masking.statement
          ? <div><dt>{X.technical.masking}</dt><dd>{records.masking.statement}</dd></div>
          : null}
      </dl>
    </details>
  </section>
}

export default ExportSheet
