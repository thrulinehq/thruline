/* Names second people added to the standing approval. A second is an act in
 * the record, never a new verdict, and the server decides which still count. */
import React from 'react'
import { NAME_NOT_CHECKED } from '../../data/seconds.js'

export function WholeStorySeconds({ seconds }) {
  if (!Array.isArray(seconds) || !seconds.length) return null
  return (
    <>
      <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: '12px' }}>
        <h2 id="ws-h-seconds" style={{ margin: '0', fontSize: '18px', fontWeight: '500', lineHeight: '1.25', color: '#EDEDEF' }}>Second approvals</h2>
        <span style={{ font: "12.5px 'Geist Mono', monospace", color: '#82828C' }}>names on the standing approval</span>
      </div>
      <ol aria-labelledby="ws-h-seconds" style={{ margin: '0', padding: '14px 16px', listStyle: 'none', display: 'flex', flexDirection: 'column', gap: '10px', border: '1px solid #1F1F23', borderRadius: '8px', background: '#0A0A0B' }}>
        {seconds.map((second) => (
          <li key={second.id} style={{ display: 'flex', gap: '10px', fontSize: '13.5px', color: '#EDEDEF' }}>
            <span>{second.who}</span><span style={{ color: '#A1A1AA' }}>{second.when}</span>
          </li>
        ))}
      </ol>
      <p style={{ margin: '0', fontSize: '12.5px', lineHeight: '18px', color: '#A1A1AA' }}>{NAME_NOT_CHECKED}</p>
    </>
  )
}
