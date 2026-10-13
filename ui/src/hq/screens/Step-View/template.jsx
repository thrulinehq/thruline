/* Step-View: the markup of one step of the record (C30 R-ROUTES "Step view").
 *
 * Built only from the canvas's own pieces, as the Whole-Story page draws them:
 * its top bar and second bar, its 860 + 340 page grid, its section headings,
 * bordered panels, violet-barred quotes, mono rows, the contents list in the
 * aside and the privacy panel. Nothing here is a new visual language; edit it
 * only to make the product true, never to restyle it. */
import React from 'react'
import { PRIVACY } from '../../data/project.js'
import './styles.css'

const MONO = "'Geist Mono', monospace"
const eyebrow = { fontSize: '11px', fontWeight: '550', letterSpacing: '.06em', textTransform: 'uppercase', color: '#A1A1AA' }
const h2 = { margin: '0', fontSize: '18px', fontWeight: '500', lineHeight: '1.25', color: '#EDEDEF' }
const meta = { font: "12.5px 'Geist Mono', monospace", color: '#82828C' }
const panel = { border: '1px solid #1F1F23', borderRadius: '8px', background: '#0A0A0B', overflow: 'hidden' }
const ghost = { height: '28px', padding: '0 10px', display: 'inline-flex', alignItems: 'center', gap: '8px', borderRadius: '6px', color: '#A1A1AA', fontSize: '13px' }
const quoteBar = { width: '2px', background: '#B78CFF', borderRadius: '1px' }
const codeBox = { margin: '0', padding: '10px 12px', border: '1px solid #1F1F23', borderRadius: '6px', background: '#000000', font: "13px 'Geist Mono', monospace", lineHeight: '1.5', color: '#EDEDEF', overflowWrap: 'anywhere', whiteSpace: 'pre-wrap' }

function Quote({ children, mono }) {
  return (
    <div style={{ display: 'flex', gap: '12px', alignItems: 'stretch' }}>
      <span aria-hidden="true" style={quoteBar}></span>
      <div style={{ fontSize: mono ? '13px' : '16px', fontFamily: mono ? MONO : undefined, lineHeight: '1.5', color: mono ? '#A1A1AA' : '#EDEDEF', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', minWidth: '0' }}>{children}</div>
    </div>
  )
}

function Fact({ label, children }) {
  return (
    <>
      <span style={{ ...eyebrow, lineHeight: '20px' }}>{label}</span>
      <span style={{ font: "12.5px 'Geist Mono', monospace", lineHeight: '20px', color: '#EDEDEF', overflowWrap: 'anywhere', minWidth: '0' }}>{children}</span>
    </>
  )
}

export default function Template({ v, link = (h) => h }) {
  const s = v?.step
  const r = s?.record
  return (<>
    <div style={{ width: '100%', minHeight: '100vh', boxSizing: 'border-box', background: '#000000', color: '#EDEDEF', fontFamily: "'Geist', -apple-system, 'Helvetica Neue', sans-serif", fontSize: '14px', display: 'flex', flexDirection: 'column', overflow: 'hidden', position: 'relative' }}>
      <header className="tl-topbar" style={{ height: '44px', flex: '0 0 44px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '10px', padding: '0 12px', background: '#0A0A0B', borderBottom: '1px solid #1F1F23' }}>
        <a href={link('Inbox.dc.html')} aria-label="Inbox" style={{ display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: '28px', height: '28px', borderRadius: '5px' }}>
          <span style={{ width: '20px', height: '20px', borderRadius: '4px', background: '#FF7A29', color: '#000000', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', font: "600 12px 'Geist Mono', monospace" }}>{'t'}</span>
        </a>
        <nav aria-label="Where you are" style={{ display: 'flex', alignItems: 'center', gap: '6px', minWidth: '0', flex: '1 1 auto', fontSize: '13px', color: '#A1A1AA', whiteSpace: 'nowrap', overflow: 'hidden' }}>
          <a href={link('Inbox.dc.html')} style={{ flex: '0 0 auto', color: '#EDEDEF', fontWeight: '500' }}>{v?.page?.project}</a>
          <span aria-hidden="true" style={{ flex: '0 0 auto', color: '#82828C' }}>{'›'}</span>
          <a href={v?.page?.briefHref} className="tl-crumb-title" title={v?.page?.title} style={{ flex: '0 1 auto', minWidth: '120px', color: '#EDEDEF', overflow: 'hidden', textOverflow: 'ellipsis' }}>{v?.page?.title}</a>
          <span aria-hidden="true" style={{ flex: '0 0 auto', color: '#82828C' }}>{'›'}</span>
          <span style={{ flex: '0 0 auto', fontFamily: MONO, fontSize: '12px', color: '#EDEDEF' }}>{v?.page?.stepMono}</span>
        </nav>
        <span title={v?.rec?.title} style={{ display: 'inline-flex', alignItems: 'center', gap: '7px', height: '24px', padding: '0 10px 0 8px', border: '1px solid #1F1F23', borderRadius: '12px', fontSize: '12px', color: v?.rec?.color ?? '#52A8FF', whiteSpace: 'nowrap', flex: '0 0 auto' }}>
          <span aria-hidden="true" style={{ width: '7px', height: '7px', borderRadius: '50%', background: v?.rec?.dot ?? '#52A8FF', boxShadow: v?.rec?.ring, animation: v?.rec?.anim ?? 'tl-pulse 1.6s ease-in-out infinite' }}></span>
          {v?.rec?.text}
        </span>
        <div role="group" aria-label="Altitude — back to the Change" style={{ display: 'inline-flex', border: '1px solid #66666F', borderRadius: '6px', overflow: 'hidden' }}>
          {(v?.alt?.list || []).map((a) => (
            <a key={a.id} href={link(a.page)} aria-current={a.current} className="tl-ghost" style={{ height: '26px', padding: '0 10px', display: 'inline-flex', alignItems: 'center', background: a.bg, borderLeft: a.bl, color: a.color, fontSize: '12.5px' }}>{a.label}</a>
          ))}
        </div>
        <a href={link('Command-Ask.dc.html')} onClick={v?.openCommand} className="tl-ghost" style={{ height: '26px', padding: '0 10px', display: 'inline-flex', alignItems: 'center', gap: '8px', border: '1px solid #66666F', borderRadius: '6px', color: '#EDEDEF', fontSize: '12.5px' }}>
          {'Find, ask, act'}<kbd style={{ font: "11px 'Geist Mono', monospace", color: '#A1A1AA', border: '1px solid #66666F', borderRadius: '3px', padding: '0 4px' }}>{'⌘K'}</kbd>
        </a>
        <span style={{ font: "12px 'Geist Mono', monospace", color: '#82828C' }}>{v?.page?.readAt}</span>
      </header>
      <div style={{ height: '44px', flex: '0 0 44px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '12px', padding: '0 16px', background: '#000000', borderBottom: '1px solid #1F1F23', whiteSpace: 'nowrap' }}>
        <a href={v?.page?.briefHref} className="tl-ghost" style={{ ...ghost, padding: '0 10px 0 8px' }}>
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"><path d="M10 3 5 8l5 5" /></svg>{' Back to the Change'}
        </a>
        <span aria-hidden="true" style={{ width: '1px', height: '16px', background: '#2A2A30' }}></span>
        <span style={{ fontSize: '13px', fontWeight: '500', color: '#EDEDEF' }}>{v?.page?.stepWord}</span>
        <span style={{ fontSize: '13px', color: '#82828C', overflow: 'hidden', textOverflow: 'ellipsis' }}>{v?.page?.ofTotal}</span>
        <span style={{ flex: '1 1 auto' }}></span>
        {s ? (<>
          {s.previous ? (<a href={s.previous.href} className="tl-ghost" style={ghost}><span aria-hidden="true">{'‹'}</span>{'Previous step'}</a>) : (<span style={{ ...ghost, color: '#82828C' }}>{'First step'}</span>)}
          {s.next ? (<a href={s.next.href} className="tl-ghost" style={ghost}>{'Next step'}<span aria-hidden="true">{'›'}</span></a>) : (<span style={{ ...ghost, color: '#82828C' }}>{'Last step'}</span>)}
        </>) : null}
      </div>
      <div style={{ flex: '1 1 auto', minHeight: '0', overflow: 'auto', boxSizing: 'border-box', padding: '16px 0 16px', display: 'grid', gridTemplateColumns: '860px 340px', columnGap: '48px', justifyContent: 'center', alignItems: 'start' }}>
        {v?.missing ? (
          <article aria-label="No such step" style={{ minWidth: '0', gridColumn: '1 / span 2', maxWidth: '860px', justifySelf: 'center', width: '100%', display: 'flex', flexDirection: 'column', gap: '12px', padding: '14px 16px', border: '1px dashed #66666F', borderRadius: '8px', boxSizing: 'border-box' }}>
            <h2 role="status" style={h2}>{v.missing.title}</h2>
            <p style={{ margin: '0', fontSize: '13.5px', lineHeight: '1.5', color: '#A1A1AA', textWrap: 'pretty' }}>{v.missing.body}</p>
            <div style={{ display: 'flex', gap: '8px' }}>
              <a href={v.missing.briefHref} className="tl-ghost" style={{ ...ghost, border: '1px solid #66666F', color: '#EDEDEF' }}>{'Back to the Brief'}</a>
              <a href={v.missing.statementsHref} className="tl-ghost" style={{ ...ghost, border: '1px solid #66666F', color: '#EDEDEF' }}>{'Every statement'}<span aria-hidden="true">{'›'}</span></a>
            </div>
          </article>
        ) : (<>
          <article aria-label={'Step ' + s?.n} style={{ minWidth: '0', display: 'flex', flexDirection: 'column', gap: '20px' }}>
            <section aria-labelledby="sv-h-step" style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: '16px' }}>
                <h2 id="sv-h-step" style={h2}>{s?.who}</h2>
                <span style={{ ...meta, whiteSpace: 'nowrap' }}>{s?.time}{' · '}{s?.versionNote}{' · '}{s?.session}</span>
              </div>
              <div style={panel}>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', padding: '14px 16px' }}>
                  {r?.prompt ? (<>
                    {r.prompt.segments.map((x, i) => (
                      <React.Fragment key={i}>
                        {x.pasted ? <span style={eyebrow}>{x.label}</span> : null}
                        <Quote mono={x.pasted}>{x.text}</Quote>
                      </React.Fragment>
                    ))}
                    {r.prompt.conversation ? (<p style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#82828C' }}>{'This was conversation, not a request for work.'}</p>) : null}
                  </>) : null}
                  {r?.claim ? (<>
                    <Quote><span style={{ color: '#B78CFF' }}>{'❝'}</span><span style={{ textDecoration: 'underline dashed #A1A1AA', textUnderlineOffset: '4px' }}>{r.claim.text}</span><span style={{ color: '#B78CFF' }}>{'❞'}</span></Quote>
                    <p style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#A1A1AA' }}>{r.claim.says}{' '}{r.claim.checkable}</p>
                    {r.claim.statementHref ? (<div><a href={r.claim.statementHref} className="tl-ghost" style={{ ...ghost, border: '1px solid #66666F', color: '#EDEDEF', height: '26px', fontSize: '12.5px' }}>{'Open this in the full statement view'}<span aria-hidden="true">{'›'}</span></a></div>) : null}
                  </>) : null}
                  {r?.message ? (<>
                    <Quote>{r.message.text}</Quote>
                    <p style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#A1A1AA' }}>{r.message.note}</p>
                  </>) : null}
                  {r?.edit ? (<>
                    <span style={{ font: "14px 'Geist Mono', monospace", color: '#EDEDEF', overflowWrap: 'anywhere' }}>{r.edit.file}</span>
                    <span style={{ fontSize: '13px', lineHeight: '1.5', color: '#A1A1AA' }}>{r.edit.lines}{r.edit.makes ? ' · ' + r.edit.makes : ''}</span>
                    {r.edit.notApplied ? (<span style={{ fontSize: '13px', lineHeight: '1.5', color: '#F2C94C' }}>{r.edit.notApplied}</span>) : null}
                    {r.edit.hasRows ? (
                      <div role="table" aria-label="The lines of this edit" style={{ border: '1px solid #1F1F23', borderRadius: '6px', background: '#000000', overflow: 'auto', maxHeight: '420px' }}>
                        {r.edit.rows.map((x, i) => (
                          <div key={i} role="row" style={{ display: 'grid', gridTemplateColumns: '18px minmax(0, 1fr)', gap: '6px', padding: '0 10px', background: x.bg, font: "12.5px 'Geist Mono', monospace", lineHeight: '20px', color: x.color }}>
                            <span role="cell" aria-hidden="true">{x.sign}</span>
                            <span role="cell" style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{x.text}</span>
                          </div>
                        ))}
                      </div>
                    ) : (<span style={{ fontSize: '13px', lineHeight: '1.5', color: '#82828C' }}>{r.edit.noLines}</span>)}
                    {r.edit.source ? (<span style={{ ...meta, fontSize: '12px' }}>{r.edit.source}</span>) : null}
                    {r.edit.cut ? (<span style={{ fontSize: '12.5px', lineHeight: '1.5', color: '#82828C' }}>{r.edit.cut}</span>) : null}
                  </>) : null}
                  {r?.command ? (<>
                    <pre style={codeBox}>{r.command.command}</pre>
                    <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'baseline', gap: '4px 14px', fontSize: '13.5px' }}>
                      <span style={{ color: '#A1A1AA' }}>{r.command.kind}</span>
                      <span style={{ color: r.command.outcomeColor }}>{r.command.outcome}</span>
                      {r.command.exit ? (<span style={{ font: "12.5px 'Geist Mono', monospace", color: '#A1A1AA' }}>{r.command.exit}</span>) : null}
                      {r.command.tally ? (<span style={{ color: '#EDEDEF' }}>{r.command.tally}</span>) : null}
                    </div>
                    {r.command.tallyLine ? (<span style={{ font: "12px 'Geist Mono', monospace", color: '#82828C', overflowWrap: 'anywhere' }}>{r.command.tallyLine}</span>) : null}
                    {r.command.noTally ? (<span style={{ fontSize: '13px', lineHeight: '1.5', color: '#82828C' }}>{r.command.noTally}</span>) : null}
                    {r.command.hasOutput ? (<>
                      <span style={eyebrow}>{r.command.outputNote}</span>
                      <pre style={{ ...codeBox, font: "12px 'Geist Mono', monospace", color: '#A1A1AA', maxHeight: '320px', overflow: 'auto' }}>{r.command.output.join('\n')}</pre>
                    </>) : null}
                  </>) : null}
                </div>
              </div>
            </section>
            <section aria-labelledby="sv-h-receipt" style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: '16px' }}>
                <h2 id="sv-h-receipt" style={h2}>{'Where this came from'}</h2>
              </div>
              <div style={panel}>
                <div style={{ display: 'grid', gridTemplateColumns: '90px minmax(0, 1fr)', gap: '6px 12px', padding: '12px 16px', alignItems: 'start' }}>
                  {s?.receipt?.has ? (<>
                    <Fact label="Recording">{s.receipt.file}</Fact>
                    <Fact label="Line">{s.receipt.line}</Fact>
                  </>) : (<Fact label="Recording">{s?.receipt?.none}</Fact>)}
                  <Fact label="Recorded">{s?.time}</Fact>
                  <Fact label="Version">{s?.version || 'not recorded'}</Fact>
                  <Fact label="Session">{s?.session}</Fact>
                </div>
              </div>
              <p style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#A1A1AA', textWrap: 'pretty' }}>{s?.receipt?.note}</p>
            </section>
            <section aria-labelledby="sv-h-lines" style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: '16px' }}>
                <h2 id="sv-h-lines" style={h2}>{'The session around this step'}</h2>
                <span style={{ ...meta, whiteSpace: 'nowrap' }}>{'the session’s own lines, as recorded'}</span>
              </div>
              {s?.linesError ? (<p role="status" style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#A1A1AA' }}>{s.linesError}</p>) : null}
              {s?.lines ? (
                s.lines.found ? (
                  <div style={panel}>
                    {s.lines.before ? (<div style={{ padding: '8px 16px', fontSize: '12.5px', color: '#82828C', borderBottom: '1px solid #1F1F23' }}>{s.lines.before}</div>) : null}
                    <ol style={{ margin: '0', padding: '0', listStyle: 'none' }}>
                      {s.lines.items.map((l, i) => (
                        <li key={i} aria-current={l.aria} style={{ display: 'grid', gridTemplateColumns: '52px minmax(0, 1fr)', gap: '12px', padding: '8px 16px', borderTop: i === 0 ? 'none' : '1px solid #1F1F23', borderLeft: l.bar, background: l.bg }}>
                          <span style={{ font: "12.5px 'Geist Mono', monospace", color: '#82828C', fontVariantNumeric: 'tabular-nums' }}>{l.time}</span>
                          <span style={{ display: 'flex', flexDirection: 'column', gap: '2px', minWidth: '0' }}>
                            <span style={{ fontSize: '12.5px', color: '#82828C' }}>{l.label}{l.outcome ? ' · ' + l.outcome : ''}</span>
                            <span style={{ fontFamily: l.mono ? MONO : undefined, fontSize: l.mono ? '12.5px' : '13.5px', lineHeight: '1.5', color: l.color, whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{l.text}{l.more ? '… (' + l.more + ' more characters)' : ''}</span>
                          </span>
                        </li>
                      ))}
                    </ol>
                    {s.lines.after ? (<div style={{ padding: '8px 16px', fontSize: '12.5px', color: '#82828C', borderTop: '1px solid #1F1F23' }}>{s.lines.after}</div>) : null}
                  </div>
                ) : (<p style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#A1A1AA' }}>{s.lines.absent}</p>)
              ) : (!s?.linesError ? (<p role="status" style={{ margin: '0', fontSize: '13px', lineHeight: '1.5', color: '#82828C' }}>{'Reading the session…'}</p>) : null)}
            </section>
          </article>
          <aside aria-label="Steps around this one" style={{ minWidth: '0', display: 'flex', flexDirection: 'column', gap: '14px' }}>
            <nav aria-label="Steps around this one" style={{ display: 'flex', flexDirection: 'column', gap: '8px', padding: '12px 14px', border: '1px solid #1F1F23', borderRadius: '8px' }}>
              <span style={eyebrow}>{'Around this step'}</span>
              <div style={{ display: 'flex', flexDirection: 'column', borderLeft: '1px solid #2A2A30' }}>
                {(s?.around || []).map((t) => (
                  <a key={t.label} href={t.href} aria-current={t.current} aria-label={t.aria} className="tl-toc" style={{ height: '26px', boxSizing: 'border-box', display: 'flex', alignItems: 'center', gap: '8px', marginLeft: '-1px', padding: '0 8px 0 12px', borderLeft: t.bar, boxShadow: t.glow, color: t.color, fontSize: '13px' }}>
                    <span style={{ flex: '0 0 auto', font: "12px 'Geist Mono', monospace", color: '#82828C' }}>{t.label}</span>
                    <span style={{ flex: '1 1 auto', minWidth: '0', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{t.what}</span>
                    <span style={{ font: "12px 'Geist Mono', monospace", color: '#82828C', whiteSpace: 'nowrap' }}>{t.time}</span>
                  </a>
                ))}
              </div>
            </nav>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', padding: '12px 14px', border: '1px solid #1F1F23', borderRadius: '8px', fontSize: '12.5px', lineHeight: '1.5', color: '#82828C' }}>
              {v?.loadLine ? (<span role="status" style={{ color: '#A1A1AA' }}>{v.loadLine}</span>) : null}
              <span>{PRIVACY}</span>
              <span>{v?.page?.storage}<a href={link('Settings.dc.html#storage')}>{'see what'}</a></span>
            </div>
          </aside>
        </>)}
      </div>
    </div>
  </>)
}
