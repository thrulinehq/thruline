/* Browser notifications from this open HQ page, and the first-open weekly
 * card. No background worker, network delivery, or record write. */
import React, { useEffect, useMemo, useRef, useState } from 'react'
import { usePreferences } from './data/preferences.js'
import { readJson } from './data/api.js'
import { API } from './data/api.js'
import { currentProjectId } from './data/project.js'
import { NOTIFICATION_POLL_MS, readNotificationFacts, WEEKLY_SUMMARY } from './data/notification-events.js'
import { createNotificationDelivery } from './data/notification-delivery.js'
import { localWeekKey, summaryMarkdown, WEEKLY_SHOWN_KEY } from './data/weekly-summary.js'

const card = { position: 'fixed', right: 20, bottom: 20, zIndex: 80, width: 'min(360px, calc(100vw - 40px))', boxSizing: 'border-box', background: '#16161A', color: '#EDEDEF', border: '1px solid #66666F', borderRadius: 8, padding: 18, boxShadow: '0 10px 28px #0009', font: "14px 'Geist', sans-serif" }
const button = { background: '#24242A', color: '#EDEDEF', border: '1px solid #66666F', borderRadius: 5, padding: '7px 10px', cursor: 'pointer' }

export function NotificationCenter({ at }) {
  const prefs = usePreferences()
  const choices = prefs.status === 'read' ? prefs.prefs.notifications : null
  const switches = useMemo(() => ({ ready: !!choices?.ready, not_backed: !!choices?.not_backed,
    agent_waiting: !!choices?.agent_waiting, recording_stopped: !!choices?.recording_stopped }),
  [choices?.ready, choices?.not_backed, choices?.agent_waiting, choices?.recording_stopped])
  const current = useRef(switches)
  current.current = switches
  const findingAfter = useRef(null)
  const delivery = useMemo(() => createNotificationDelivery(), [])
  const [readError, setReadError] = useState(null)
  const [weekly, setWeekly] = useState(null)
  const [weeklyError, setWeeklyError] = useState(null)
  const [fallbackProject, setFallbackProject] = useState(null)
  const wantProject = switches.ready || switches.agent_waiting || switches.recording_stopped
  useEffect(() => {
    if (prefs.status !== 'read' || !wantProject || at?.project) return
    let live = true
    readJson(API.projects).then((doc) => { if (live) setFallbackProject(currentProjectId(doc.projects, null) || null) }, () => {})
    return () => { live = false }
  }, [prefs.status, wantProject, at?.project])
  const projectId = at?.project || fallbackProject

  useEffect(() => {
    if (prefs.status !== 'read') return
    delivery.disableOff(switches)
    if (!switches.not_backed) findingAfter.current = null
    if (!Object.values(switches).some(Boolean)) { setReadError(null); return }
    let live = true
    let busy = false
    const poll = async () => {
      if (!live || busy) return
      busy = true
      try {
        const facts = await readNotificationFacts(current.current, readJson, { findingAfter: findingAfter.current, projectId })
        if (!live) return
        const outcome = delivery.deliver(facts, current.current)
        if (current.current.not_backed && !outcome.reason && facts.findingSeq != null) findingAfter.current = facts.findingSeq
        setReadError(outcome.reason || (facts.errors.length ? 'Some notification facts could not be read. ThruLine will try again.' : null))
      } catch {
        if (live) setReadError('Notifications could not read the record. ThruLine will try again.')
      } finally { busy = false }
    }
    poll()
    const timer = window.setInterval(poll, NOTIFICATION_POLL_MS)
    document.addEventListener('visibilitychange', poll)
    return () => { live = false; window.clearInterval(timer); document.removeEventListener('visibilitychange', poll) }
  }, [prefs.status, switches, delivery, projectId])

  const weeklyOn = prefs.status === 'read' && prefs.prefs.weekly_summary === true
  useEffect(() => {
    if (!weeklyOn) { setWeekly(null); setWeeklyError(null); return }
    const week = localWeekKey()
    try { if (localStorage.getItem(WEEKLY_SHOWN_KEY) === week) return }
    catch { setWeeklyError('This browser could not remember whether this week’s card was shown.'); return }
    let live = true
    readJson(WEEKLY_SUMMARY).then((summary) => {
      if (!live) return
      if (!summaryMarkdown(summary)) { setWeeklyError('ThruLine could not read this week’s counts.'); return }
      try { localStorage.setItem(WEEKLY_SHOWN_KEY, week) }
      catch { setWeeklyError('This browser could not remember whether this week’s card was shown.'); return }
      setWeekly(summary)
      setWeeklyError(null)
    }, () => { if (live) setWeeklyError('ThruLine could not read this week’s counts. Reload to try again.') })
    return () => { live = false }
  }, [weeklyOn])

  const save = async () => {
    const text = summaryMarkdown(weekly)
    if (!text) return
    const { saveText } = await import('../workspace/export-sheet.jsx')
    saveText({ text, filename: `thruline-weekly-${localWeekKey()}.md`, mime: 'text/markdown; charset=utf-8' })
  }
  return <>
    {readError ? <div role="status" style={{ position: 'fixed', left: 12, bottom: 12, zIndex: 79, background: '#16161A', color: '#EDEDEF', border: '1px solid #66666F', padding: '7px 10px', fontSize: 12 }}>{readError}</div> : null}
    {weekly ? <section aria-label="Weekly summary" style={card}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'start', gap: 10 }}>
        <h2 style={{ fontSize: 17, margin: 0 }}>Your last seven days</h2>
        <button type="button" aria-label="Dismiss weekly summary" onClick={() => setWeekly(null)} style={button}>Close</button>
      </div>
      <p style={{ color: '#A1A1AA', margin: '8px 0 12px' }}>From the record, computed when this page opened. Nothing was sent.</p>
      <ul style={{ lineHeight: 1.75, margin: '0 0 14px', paddingLeft: 22 }}>
        <li>Changes approved: {weekly.approvals}</li>
        <li>Not backed findings: {weekly.not_backed}</li>
        <li>Re-checks run: {weekly.rechecks}</li>
        <li>Projects recorded: {weekly.projects_recorded}</li>
      </ul>
      <button type="button" onClick={save} style={button}>Save as Markdown</button>
    </section> : weeklyError ? <div role="status" style={card}>{weeklyError}</div> : null}
  </>
}
