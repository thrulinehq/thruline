/* Changes (C30 R-NAV): renderVals() for the Inbox's own template, drawn once
 * per project. There is no canvas artboard for this page and no demo: it
 * reads only `this.props.data` (from-projects.js `changesData`).
 *
 * Every row is the Inbox's row (Inbox/text.js, Inbox/logic.js `rowUi`), in
 * the Inbox's groups, under a heading that is the sidebar's own project row
 * (a link to that project's Inbox). The Inbox's filters (All, Your move, The
 * agent's, Recording) apply as they do on the Inbox; the state filter keeps
 * only the rows in one state. Every count is the rows it stands for. */
import { DCLogic } from '../../dc/runtime.jsx'
import { inboxText, projectLine, rowsUnder } from '../Inbox/text.js'
import { KEYS_HINT, PREVIEW_OF, rowUi, segUi } from '../Inbox/logic.js'
import { rowKeys } from '../../keys/list.js'
import { fmtCount } from '../../data/count.js'
import { densityOf, choose } from '../../data/face.js'
import { readingLine } from '../../data/project.js'

const plural = (n, one, many) => (n === 1 ? one : many)

/** The whose-move filters, as the Inbox draws them. */
export const FILTERS = Object.freeze([
  { key: 'all', label: 'All' },
  { key: 'you', label: 'Your move' },
  { key: 'agent', label: 'The agent’s' },
  { key: 'live', label: 'Recording' },
])

/** The states a row can be in, in the words its pill says. */
export const STATES = Object.freeze([
  { key: 'all', label: 'Any state' },
  { key: 'asked', label: 'Your turn' },
  { key: 'recheck', label: 'Needs a re-check' },
  { key: 'notProven', label: 'Not proven' },
  { key: 'done', label: 'Nothing open' },
])

/** Whether a row (an Inbox row: {kind, t}) is in state `st`. */
export function inState(row, st) {
  if (st === 'all') return true
  if (st === 'asked') return row.kind === 'prompt'
  if (st === 'recheck') return row.kind === 'check' && !!row.t.recheck
  if (st === 'notProven') return row.kind === 'check' && !row.t.recheck
  return false
}

/** The rows one project shows under a filter and a state, by group: the Inbox's own filter rule
 *  (Inbox/text.js `rowsUnder`, C46 §1.1), then the state. */
export function blockRows(t, D, f, st) {
  const u = rowsUnder(t, D, f)
  const keep = (rows) => rows.filter((r) => inState(r, st))
  return {
    you: keep(u.you),
    check: keep(u.check),
    agent: keep(u.agent),
    live: st === 'all' ? u.live : [],
    nothing: f === 'all' && (st === 'all' || st === 'done') ? t.nn.items : [],
  }
}

const count = (r) => r.you.length + r.check.length + r.agent.length + r.live.length

export default class Changes extends DCLogic {
  state = { sel: null, density: 'comfortable', filter: 'all', st: 'all', open: {}, copied: false };

  renderVals() {
    const s = this.state
    const D = this.props.data
    /* C30 R-PREFS: the density is the stored one; Settings and the Inbox change the same value */
    const comfy = densityOf(this.props, s.density) === 'comfortable'
    const show = (ok) => (ok ? 'flex' : 'none')

    // every read project's Inbox text, and the rows each shows under the filters
    const read = D.blocks.filter((b) => b.data)
    const texts = read.map((b) => ({ b, t: inboxText(b.data, { copied: s.copied }) }))
    const rowsOf = (f, st) => texts.map(({ b, t }) => ({ b, t, r: blockRows(t, b.data, f, st) }))
    const shown = rowsOf(s.filter, s.st)

    // one selection across every project: the row picked, else the first row
    const key = (b, r) => b.id + ':' + r.id
    const order = []
    for (const { b, r } of shown) for (const g of ['you', 'check', 'agent', 'live']) for (const row of r[g]) order.push({ k: key(b, row), b, row })
    const selected = order.find((o) => o.k === s.sel) || order[0] || null
    const selKey = selected ? selected.k : null
    const withUi = (b, g) => g.map((row) => Object.assign({}, row.t, {
      id: key(b, row), kind: row.kind,
      ui: rowUi(selKey === key(b, row), comfy, () => this.setState({ sel: key(b, row) })),
      copyAct: () => this.setState({ sel: key(b, row), copied: row.id }),
    }))

    const blocks = []
    for (const blk of D.blocks) {
      const side = blk.side || {}
      const head = (line, dot) => ({ name: side.name || blk.id, href: blk.href, line, dot })
      if (blk.reading) { blocks.push({ key: blk.id, head: head(readingLine(side.name)), g: { you: 'none', check: 'none', agent: 'none', live: 'none', nothing: 'none' } }); continue }
      if (blk.failed) { blocks.push({ key: blk.id, head: head('ThruLine could not read this project’s record just now.'), g: { you: 'none', check: 'none', agent: 'none', live: 'none', nothing: 'none' } }); continue }
      const it = shown.find((x) => x.b === blk)
      const r = it.r
      if (count(r) === 0 && r.nothing.length === 0) continue
      const open = s.st === 'done' || !!s.open[blk.id]
      const t = it.t
      blocks.push({
        key: blk.id,
        head: head(projectLine(side), side.recording ? (side.changeCount ? 'live' : 'hollow') : null),
        groups: { you: withUi(blk, r.you), check: withUi(blk, r.check), agent: withUi(blk, r.agent), live: withUi(blk, r.live) },
        g: { you: show(r.you.length), check: show(r.check.length), agent: show(r.agent.length), live: show(r.live.length), nothing: show(r.nothing.length) },
        t: {
          n: { you: fmtCount(r.you.length), check: fmtCount(r.check.length), agent: fmtCount(r.agent.length), live: fmtCount(r.live.length), nothing: fmtCount(r.nothing.length) },
          nn: Object.assign({}, t.nn, { items: r.nothing }),
        },
        nn: {
          open,
          caret: open ? '▾' : '▸',
          expanded: open ? 'true' : 'false',
          verb: open ? 'collapse' : 'expand',
          toggle: () => this.setState({ open: Object.assign({}, s.open, { [blk.id]: !open }) }),
        },
      })
    }

    // the page's sentences: the Inbox's, over every project read
    const merged = { changes: {}, prompts: [], sessions: [], note: D.note, nothingNeeded: { items: [] } }
    for (const { b } of texts) {
      Object.assign(merged.changes, b.data.changes)
      merged.prompts.push(...b.data.prompts)
      merged.sessions.push(...b.data.sessions)
    }
    const page = inboxText(Object.assign(merged, { projects: D.projects, inboxBadge: D.inboxBadge, inboxBadgeAria: D.inboxBadgeAria, readAt: D.readAt, storage: D.storage, capture: {} }))
    const total = D.blocks.length
    const reading = D.blocks.filter((b) => b.reading).length
    const threads = read.reduce((a, b) => a + (b.threads || 0), 0)
    const visible = shown.reduce((a, x) => a + count(x.r) + x.r.nothing.length, 0)
    const countNote = reading
      ? 'Counted by Change: every thread of work in ' + fmtCount(total - reading) + ' of ' + fmtCount(total) + ' ' + plural(total, 'project', 'projects') + ' read so far.'
      : !visible && (s.filter !== 'all' || s.st !== 'all')
        ? 'No thread of work in ' + fmtCount(total) + ' ' + plural(total, 'project', 'projects') + ' matches what is chosen here.'
        : 'Counted by Change: every thread of work in ' + fmtCount(total) + ' ' + plural(total, 'project', 'projects') + ', ' + fmtCount(threads) + ' in all.'
    const t = Object.assign({}, page, {
      // while any project is still being read, no count is said: the headline would undercount
      headline: reading ? readingLine() : total === 0 ? 'Nothing is recorded on this machine yet.' : page.headline,
      footer: reading ? '' : page.footer,
      countNote,
    })

    // the filters' counts are the rows each would show, under the state chosen; the states' under the filter chosen
    const fRows = (f) => rowsOf(f, s.st).reduce((a, x) => a + count(x.r), 0)
    const filters = FILTERS.map((x, i) => Object.assign({}, x, segUi(s.filter === x.key), {
      count: x.key === 'all' ? '' : fmtCount(fRows(x.key)),
      countShow: x.key === 'all' ? 'none' : 'inline',
      divider: i === 0 ? '0' : '1px solid #66666F',
      pick: () => this.setState({ filter: x.key }),
    }))
    const stRows = (st) => rowsOf(s.filter, st).reduce((a, x) => a + count(x.r) + (st === 'done' ? x.r.nothing.length : 0), 0)
    const states = STATES.map((x, i) => Object.assign({}, x, segUi(s.st === x.key), {
      count: x.key === 'all' ? '' : fmtCount(stRows(x.key)),
      countShow: x.key === 'all' ? 'none' : 'inline',
      divider: i === 0 ? '0' : '1px solid #66666F',
      pick: () => this.setState({ st: x.key }),
    }))

    // C30 R-KEYS: the list keys walk every block's rows in the order they are drawn, as a click would;
    // `e` opens or closes "Nothing needed" in the selected row's project
    const selBlock = selected ? blocks.find((b) => b.groups && ['you', 'check', 'agent', 'live'].some((g) => b.groups[g].some((r) => r.id === selKey))) : null
    const nnBlock = selBlock && selBlock.g.nothing === 'flex' ? selBlock : blocks.find((b) => b.groups && b.g.nothing === 'flex') || null
    const keys = rowKeys({
      rows: order.map((o) => {
        const address = o.b.data.hideRows && o.b.data.hideRows[o.row.threadId]
        return { id: o.k, kind: o.row.kind, t: o.row.t,
          select: () => this.setState({ sel: o.k }), copy: () => this.setState({ sel: o.k, copied: o.row.id }),
          hide: address && !address.hidden && this.props.hideRow ? () => this.props.hideRow(o.b.id, address) : null }
      }),
      selId: selKey,
      expand: nnBlock ? nnBlock.nn.toggle : null,
    })

    const pvKind = selected ? PREVIEW_OF[selected.row.kind] : null
    const sp = selected ? Object.assign({}, selected.row.t, { copyAct: () => this.setState({ sel: selected.k, copied: selected.row.id }) }) : {}
    return {
      isDemo: false,
      keys,
      keysHint: KEYS_HINT[this.props.keysMode] || KEYS_HINT.on,
      reading: false,
      page: {
        name: 'Changes',
        inbox: { cur: undefined, cls: 'tl-ghost', bg: undefined, color: '#A1A1AA', weight: undefined },
        changes: { cur: 'page', bg: '#16161A', color: '#EDEDEF', weight: '500' },
      },
      t,
      sp,
      blocks,
      d: {
        rowPad: comfy ? '10px 16px 10px 20px' : '8px 16px 8px 20px',
        rowGap: comfy ? '4px' : '0px',
        line2: comfy ? 'flex' : 'none',
        compactOnly: comfy ? 'none' : 'inline',
        modified: comfy ? 'none' : 'inline-block',
        comfy: segUi(comfy),
        compact: segUi(!comfy),
        setComfy: () => choose(this.props, 'density', 'comfortable', (v) => this.setState({ density: v })),
        setCompact: () => choose(this.props, 'density', 'compact', (v) => this.setState({ density: v })),
      },
      slot: { decisionGroup: null, decisionPreview: null },
      filters,
      states,
      pv: { decision: false, disc: pvKind === 'disc', post: pvKind === 'post', prompt: pvKind === 'prompt', stripe: pvKind === 'stripe', live: pvKind === 'live' },
      banner: { show: false, dismiss: () => {} },
      codex: { tried: false, notTried: false, retry: () => {} },
    }
  }
}
