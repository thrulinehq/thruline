import React from 'react'
import { CAST, PAL, GRAY, sprite } from './lib/arcade.js'

/* THE CAST — a design exhibit, in-product and deep-linkable (`?cast`).
 *
 * RA6 amendment B: all four members with their bindings, on made-up data and
 * marked as an exhibit — including HOLLOW, whose appearance HERE is the
 * design being shown, not the class being asserted: the bench notice renders
 * verbatim, and every product data surface keeps the earned-appearance rules
 * unchanged (the containment pin walks them; this file may never be imported
 * by a real-data render).
 *
 * The exhibit renders sprites through its own frame painter rather than the
 * product's `GhostAt`, which refuses a benched member by construction — that
 * refusal is a product rule, and this page is a poster on the workshop wall. */

/* The bindings, quoted from the committed design sheet (docs/design/ThruLine
 * Arcade.dc.html, the cast block). The design speaks here; nothing below is a
 * claim about any real project. */
const EXHIBIT = [
  {
    key: 'nib',
    role: 'the worker-spirit',
    color: '#6fe0ff',
    body: 'Traces the path of work actually done, in the order it was recorded. ThruLine reads and marks; it never plays the game for you.',
    bound: 'the ordered event chain of a session',
  },
  {
    key: 'dusk',
    role: 'the ghost of old proof',
    color: '#c58bff',
    body: 'A moth, not a spectre: drawn to proof nobody has read lately. Appears beside a claim that once checked out and no longer matches the project. Re-observation banishes it; the visit stays in history forever.',
    bound: 'a passing check whose files changed after it passed',
  },
  {
    key: 'snag',
    role: 'the contradiction ghost',
    color: '#ff5f4d',
    body: 'Hard-edged, spiked, unignorable. The only cast member allowed to break the fun: when SNAG is on screen the game is meant to feel wrong. Never comic, never animated for charm — it holds still.',
    bound: 'a completion claim the record disagrees with',
  },
  {
    key: 'hollow',
    role: 'the spirit with nothing inside',
    color: '#8a93b0',
    body: 'Outline only — nothing inside, because there is no receipt inside. Designed in full, and shipped as a design, not as an appearance.',
    bound: 'nothing yet · the class is not graduated',
    /* The design sheet's bench notice, verbatim. */
    bench: 'Designed this round, allowed on no screen until graduation rules it in.',
  },
]

function Frame({ rows, scale, gray }) {
  const s = sprite(rows, scale, gray ? GRAY : PAL)
  return (
    <span
      aria-hidden="true"
      style={{
        display: 'inline-block',
        width: s.w,
        height: s.h,
        position: 'relative',
      }}
    >
      <span style={{ position: 'absolute', top: 0, left: 0, width: s.px, height: s.px, boxShadow: s.shadow }} />
    </span>
  )
}

export default function CastExhibit() {
  return (
    <div className="os-root" style={{ display: 'block', overflow: 'auto' }}>
      {/* The permanent demo-data banner (C7 Part 4). Sticky, unconditional,
          and carrying no dismiss control: a region that renders made-up data
          says so for as long as it is on screen, or it is lying by the time a
          reader scrolls. */}
      <div
        data-demo-banner="cast"
        style={{
          position: 'sticky',
          top: 0,
          zIndex: 10,
          padding: '9px 16px',
          background: '#ff7ad9',
          color: '#1a0716',
          font: '500 11px/1.5 var(--mono)',
        }}
      >
        DESIGN EXHIBIT — the cast, on made-up data. Nothing here describes a real project.
      </div>
      <div style={{ maxWidth: '980px', margin: '0 auto', padding: '30px 22px 60px' }}>
        <h1 style={{ margin: '0 0 4px', font: '600 22px/1.2 var(--sans)' }}>The cast</h1>
        <p style={{ margin: '0 0 22px', font: '400 12.5px/1.6 var(--sans)', color: 'var(--t2)' }}>
          Distinct silhouette, glyph and name, recognisable in grayscale; colour reinforces identity, never
          carries it. On product surfaces every appearance is earned by a record — this page only shows the
          designs.
        </p>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '14px' }}>
          {EXHIBIT.map((m) => {
            const member = CAST[m.key]
            return (
              <article
                key={m.key}
                style={{
                  display: 'flex',
                  gap: '18px',
                  padding: '18px 20px',
                  borderRadius: 'var(--rad)',
                  background: 'var(--s1)',
                  border: '1px solid var(--line)',
                }}
              >
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '10px', flex: 'none' }}>
                  <Frame rows={member.frames[0]} scale={5} />
                  <Frame rows={member.frames[0]} scale={2} gray />
                  <span style={{ font: '400 8px/1 var(--mono)', color: 'var(--t3)', letterSpacing: '.08em' }}>GRAYSCALE</span>
                </div>
                <div style={{ minWidth: 0 }}>
                  <h2 style={{ margin: 0, font: '600 15px/1.3 var(--mono)', color: m.color }}>
                    {member.name}{' '}
                    <span style={{ font: '400 11px/1.3 var(--mono)', color: 'var(--t3)' }}>
                      {member.glyph} {m.role}
                    </span>
                  </h2>
                  <p style={{ margin: '8px 0', font: '400 12px/1.6 var(--sans)', color: 'var(--t2)' }}>{m.body}</p>
                  <p style={{ margin: 0, font: '400 10.5px/1.6 var(--mono)', color: 'var(--t3)' }}>
                    Bound to: {m.bound}
                  </p>
                  {m.bench ? (
                    <p
                      style={{
                        margin: '10px 0 0',
                        padding: '7px 10px',
                        border: '1px dashed #ff7ad980',
                        borderRadius: '7px',
                        font: '400 10.5px/1.5 var(--mono)',
                        color: '#ff7ad9',
                      }}
                    >
                      BENCHED — {m.bench}
                    </p>
                  ) : null}
                </div>
              </article>
            )
          })}
        </div>
      </div>
    </div>
  )
}
