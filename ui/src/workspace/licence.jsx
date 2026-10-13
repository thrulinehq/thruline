import React from 'react'
import { HQ_COPY } from './hq-copy.js'
import { API, readProjection } from './data.js'
import { licenceView } from './licence-view.js'
import { goToInbox } from '../routes.js'
import './workspace.css'

/* The licence page, at `#/licence` (T-DIST-4).
 *
 * WHAT IT SAYS, in this order: the status in one sentence (and, when the
 * licence can't be used, what would fix it); what needs a licence and why the
 * rest never will; the exact command that installs one, with a copy control —
 * except in a build that checks no licence, where that command is always
 * refused and so is not offered (ADV-dist-copy-3).
 * The technical layer — signature, key id, fingerprint — is one click down.
 *
 * WHAT IT MAY DO. Read `GET /api/licence`, and nothing else. There is no
 * upload and no write address on this page: a licence is installed from a
 * terminal, where ThruLine checks the file before it keeps it.
 *
 * Quiet by rule: no banner and no colour carrying meaning.
 */

const L = HQ_COPY.licence

export function LicencePanel({ view, onCopy, copied, onBack }) {
  const P = L.page
  return <main className="ws-change ws-licence"><div className="ws-change-inner">
    <p className="ws-change-kicker"><a className="ws-change-secondary" href="#/inbox" onClick={onBack}>{P.back}</a></p>
    <h1 className="ws-change-title">{P.title}</h1>
    <p className="ws-change-conclusion" data-licence-status={view.status}>{view.sentence}</p>
    {view.fix ? <p className="ws-change-muted ws-licence-fix">{view.fix}</p> : null}

    <section className="ws-licence-needs" aria-labelledby="ws-licence-h-needs">
      <h2 id="ws-licence-h-needs">{P.needs_heading}</h2>
      {/* R-2026-09-24-25: with the paid gate off nothing needs a licence, and
        * the page says so rather than naming a surface that is open anyway. */}
      <p>{view.gateOff ? P.needs_off : P.needs}</p>
      <p className="ws-change-muted">{P.free}</p>
      <p className="ws-change-muted">{P.why}</p>
    </section>

    {/* ADV-dist-copy-3: not in a build that checks no licence — there the
      * command is refused every time, so offering it would only mislead. */}
    {view.offersInstall ? <section className="ws-licence-install" aria-labelledby="ws-licence-h-install">
      <h2 id="ws-licence-h-install">{P.install_heading}</h2>
      <p className="ws-change-muted">{P.install_intro}</p>
      <p className="ws-inbox-command"><code>{P.install_command}</code></p>
      {/* The one primary on this page: copying the command. */}
      <button type="button" className="ws-change-primary" onClick={() => onCopy && onCopy(P.install_command)}>{P.copy}</button>
      {copied ? <p className="ws-change-muted" role="status">{P.copied}</p> : null}
      <p className="ws-change-muted">{P.offline}</p>
    </section> : null}

    <details className="ws-change-technical">
      <summary>{L.technical.open}</summary>
      <dl>{view.technical.map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}</dl>
      <p className="ws-change-muted">{L.technical.signature}</p>
      <p className="ws-change-muted">{L.technical.fingerprint}</p>
    </details>
  </div></main>
}

export default function LicenceScreen() {
  const [doc, setDoc] = React.useState(undefined)
  const [copied, setCopied] = React.useState(false)
  React.useEffect(() => {
    let live = true
    readProjection(API.licence).then((r) => { if (live) setDoc(r.status === 'ready' ? r.doc : null) })
    return () => { live = false }
  }, [])
  if (doc === undefined) return <main className="ws-change"><div className="ws-change-inner"><p className="ws-change-conclusion">{L.page.loading}</p></div></main>
  const copy = (text) => {
    try { navigator.clipboard.writeText(text); setCopied(true) } catch { /* nothing else to do */ }
  }
  return <LicencePanel
    view={licenceView(doc)}
    onCopy={copy}
    copied={copied}
    onBack={(e) => { if (e) e.preventDefault(); goToInbox(typeof window === 'undefined' ? null : window) }}
  />
}
