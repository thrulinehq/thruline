/* Team-View.dc.html: the artboard's logic, from the canvas. renderVals()
 * returns everything template.jsx reads. Content comes only from
 * `this.props.data` (live) or DEMO (the design gallery) — see DATA.md;
 * `this.state` holds UI state only. */
import React from 'react'
import { DCLogic } from '../../dc/runtime.jsx'
import { DEMO } from './demo.js'
import { SecondControl } from './seconds.jsx'
import { recordingPill, storageSentence } from '../../data/project.js'
import { PLANS_URL } from '../../../workspace/licence-view.js'
import { fmtCount, countOf } from '../../data/count.js'
import { href } from '../../routes.js'
import { faceOf, switchFace } from '../../data/face.js'

export const PROPS = {"$preview":{"width":1512,"height":920}}

const plural = countOf
const gb = (bytes) => (bytes / 1e9).toFixed(1) + ' GB'

export default class Component extends DCLogic {
  // UI state only: the face, the filter, the copied note, and local edits not
  // yet on the base branch. `seat` and `rule` hold overrides keyed by id; an
  // absent key means "as recorded".
  state = { face: 'plain', seat: {}, rule: {}, filter: 'all', copied: false };

  renderVals() {
    const s = this.state;
    const D = this.props.data || DEMO;
    const isDemo = !this.props.data;
    /* C30 R-PREFS: the face is the stored one on a live screen; the gallery keeps the canvas's own toggle */
    const tech = faceOf(this.props, s.face) === 'technical';
    if (D.live) return liveVals(this, D, tech);
    const base = D.project.baseBranch;
    const T1 = '#EDEDEF';
    const T2 = '#A1A1AA';
    const T3 = '#82828C';
    const LINE = '#2A2A30';
    const CTRL = '#66666F';
    const HAIR = '1px solid #1F1F23';
    const MONO = "'Geist Mono', monospace";
    const SANS = "'Geist', sans-serif";

    const byId = Object.fromEntries(D.people.map((p) => [p.id, p]));
    const you = D.people.find((p) => p.isYou) || null;
    const nameOf = (id) => (byId[id] ? byId[id].name : id);
    const shortName = (id) => (byId[id] && byId[id].isYou ? 'you' : nameOf(id));
    const initial = (id) => nameOf(id).slice(0, 1).toUpperCase();
    const groupOf = (id) => D.groups.find((g) => g.members.includes(id)) || null;
    const ruleGroups = new Set(D.rules.map((r) => r.group).filter(Boolean));
    const namedInRule = (id) => D.groups.some((g) => ruleGroups.has(g.name) && g.members.includes(id));

    // ---- face (Plain / Technical) ----
    const face = {
      plain: tech ? T2 : T1,
      technical: tech ? T1 : T2,
      label: tech ? 'Face: Technical. Switch to Plain' : 'Face: Plain. Switch to Technical',
      toggle: () => switchFace(this.props, tech ? 'plain' : 'technical', (f) => this.setState({ face: f }))
    };

    // ---- seats ----
    const roster = D.people.map((p) => ({
      id: p.id,
      initials: initial(p.id),
      name: p.name,
      you: !!p.isYou,
      email: p.email,
      key: p.key || 'no key yet',
      group: groupOf(p.id) ? groupOf(p.id).name : '',
      fixed: namedInRule(p.id),
      seated: Object.hasOwn(s.seat, p.id) ? s.seat[p.id] : !!p.seated
    }));
    const seatedP = roster.filter((p) => p.fixed || p.seated);
    const readersP = roster.filter((p) => !(p.fixed || p.seated));
    const used = seatedP.length;
    const segments = Array.from({ length: D.plan.seats }, (_, i) => ({
      bg: i < used ? T2 : 'transparent',
      bd: i < used ? T2 : LINE
    }));
    const seated = seatedP.map((p, i) => ({
      initials: p.initials,
      name: p.name,
      youShow: p.you ? 'inline' : 'none',
      ident: tech ? p.key : p.email,
      group: p.group || '—',
      groupColor: p.group ? T2 : T3,
      ruleShow: p.fixed ? 'inline' : 'none',
      releaseShow: p.fixed ? 'none' : 'inline-flex',
      releaseLabel: 'Release ' + p.name + '’s seat',
      release: () => this.setState({ seat: { ...s.seat, [p.id]: false } }),
      sep: i === 0 ? '0' : HAIR
    }));
    const readers = readersP.map((p, i) => ({
      initials: p.initials,
      name: p.name,
      ident: tech ? p.key : p.email,
      assignLabel: 'Assign a seat to ' + p.name,
      assign: () => this.setState({ seat: { ...s.seat, [p.id]: true } }),
      sep: i === 0 ? '0' : HAIR
    }));
    const seats = {
      used: used,
      usedText: fmtCount(used) + ' of ' + fmtCount(D.plan.seats) + ' in use',
      people: plural(used, 'person', 'people'),
      segments: segments,
      seated: seated,
      readers: readers,
      hasReaders: readers.length > 0,
      noReaders: readers.length === 0
    };

    // ---- approval rules (edits are local until committed and on the base branch) ----
    const ruleOn = (r) => (Object.hasOwn(s.rule, r.id) ? s.rule[r.id] : !!r.twoApprovals);
    const twoCount = D.rules.filter(ruleOn).length;
    const edits = D.rules.filter((r) => ruleOn(r) !== !!r.twoApprovals).length;
    const sw = (r) => {
      const on = ruleOn(r);
      return {
        checked: on ? 'true' : 'false',
        trackBg: on ? T1 : 'transparent',
        trackBd: on ? T1 : CTRL,
        knobLeft: on ? '16px' : '2px',
        knobBg: on ? '#000000' : T3,
        word: on ? 'Two approvals' : 'One approval',
        toggle: () => this.setState({ rule: { ...s.rule, [r.id]: !on } })
      };
    };
    const ruleRow = (r) => {
      const g = D.groups.find((x) => x.name === r.group);
      return {
        ...sw(r),
        paths: r.paths,
        ariaLabel: (r.paths || 'Everything else') + ' asks for two approvals',
        group: r.group,
        approvers: g ? g.members.map((id) => (byId[id] && byId[id].isYou ? nameOf(id) + ' (you)' : nameOf(id))).join(', ') : ''
      };
    };
    const rules = {
      count: fmtCount(twoCount) + ' of ' + fmtCount(D.rules.length) + ' rules ' + (twoCount === 1 ? 'asks' : 'ask') + ' for two approvals',
      r1: ruleRow(D.rules[0]),
      r2: ruleRow(D.rules[1]),
      clean: edits === 0,
      edited: edits > 0,
      editText: edits === 1 ? '1 edit, not on ' + base + ' yet' : fmtCount(edits) + ' edits, not on ' + base + ' yet',
      undo: () => this.setState({ rule: {} }),
      note: tech
        ? 'Read from the base branch (' + base + '), like .thruline/policy.yml, so a Change cannot loosen its own rule. The strictest match across every changed path wins.'
        : 'Read from ' + base + ', the way .thruline/policy.yml is, so a Change cannot loosen its own rule. A Change follows the strictest rule among the files it changed.'
    };
    const groups = D.groups.map((g) => ({
      name: g.name,
      people: plural(g.members.length, 'person', 'people') + ' · ',
      paths: D.rules.filter((r) => r.group === g.name).map((r) => r.paths).join(', ')
    }));

    // ---- waiting on a second person ----
    // A decision needs a second approval when any rule its changed paths match asks for two.
    const ruleById = Object.fromEntries(D.rules.map((r) => [r.id, r]));
    const needsTwo = (w) => w.rulesMatched.some((id) => ruleById[id] && ruleOn(ruleById[id]));
    const needsTwoOnMain = (w) => w.rulesMatched.some((id) => ruleById[id] && ruleById[id].twoApprovals);
    const preview = D.waiting.filter(needsTwo).length;
    const today = D.waiting.filter(needsTwoOnMain).length;
    const waitRow = (w) => {
      const two = needsTwo(w);
      const first = w.approvals[0] || {};
      const nextName = nameOf(w.next);
      return {
        title: w.change.title,
        version: w.version,
        approvalsAria: 'Approvals of ' + w.version + ', ' + fmtCount(w.approvals.length) + ' of 2 recorded',
        firstBy: 'Approved · ' + shortName(first.by) + ' · ',
        firstAt: first.at,
        ref: tech
          ? w.decisionFile + ' · commit ' + w.commit
          : w.version + ' = commit ' + w.commit + (w.cleanTree ? ' · clean tree' : ''),
        second: two ? 'Waiting for ' + shortName(w.next) : 'Not needed with your edit',
        copyLabel: s.copied ? 'Copied — not sent' : 'Copy a note',
        copyAria: s.copied ? 'Copied a note for ' + nextName + '. Nothing was sent' : 'Copy a note for ' + nextName,
        copyIcon: s.copied ? 'none' : 'inline',
        doneIcon: s.copied ? 'inline' : 'none',
        copy: () => this.setState({ copied: true })
      };
    };
    const wait = {
      count: fmtCount(D.waiting.length),
      headText: edits > 0 ? 'With your edit: ' + fmtCount(preview) + ' would wait · ' + base + ' today: ' + fmtCount(today) : 'Each approval names one commit on a clean tree',
      headColor: edits > 0 ? T2 : T3,
      second: {
        discBd: preview > 0 ? '1px solid ' + LINE : '1px dashed ' + CTRL,
        color: preview > 0 ? T1 : T3
      },
      // The artboard draws two rows: b first, then a.
      b: waitRow(D.waiting[0]),
      a: waitRow(D.waiting[1]),
      changedAfter: D.changedAfterApproval.length === 0 ? 'none' : D.changedAfterApproval.map((c) => c.title).join(', ')
    };

    // ---- shared "What counts as done" ----
    const tpl = (t) => ({
      name: t.name,
      version: 'v' + t.version,
      lines: plural(t.lines, 'line', 'lines'),
      initial: initial(t.adoptedBy),
      by: shortName(t.adoptedBy),
      on: ' · ' + t.adoptedOn,
      previewAria: 'Preview ' + t.name + ', version ' + t.version
    });
    const templates = {
      count: fmtCount(D.templates.length),
      t0: tpl(D.templates[0]),
      t1: tpl(D.templates[1]),
      t2: tpl(D.templates[2])
    };

    // ---- set aside under team authority ----
    const keyOf = (r) => (r.kind === 'waived' ? 'waived' : 'na');
    const shown = D.setAside.filter((r) => s.filter === 'all' || keyOf(r) === s.filter);
    const asideRows = shown.map((r, i) => ({
      kind: keyOf(r) === 'waived' ? (tech ? 'waived' : 'Waived') : (tech ? 'not_applicable' : 'Does not apply'),
      kindFont: tech ? MONO : SANS,
      line: r.line,
      version: r.version,
      change: r.change,
      who: shortName(r.by),
      authority: r.authority,
      when: r.at,
      why: r.why,
      sep: i === 0 ? '0' : HAIR
    }));
    const filterDefs = [
      { key: 'all', label: 'All', count: fmtCount(D.setAside.length) },
      { key: 'waived', label: 'Waived', count: fmtCount(D.setAside.filter((r) => keyOf(r) === 'waived').length) },
      { key: 'na', label: 'Does not apply', count: fmtCount(D.setAside.filter((r) => keyOf(r) === 'na').length) }
    ];
    const filters = filterDefs.map((f, i) => {
      const on = s.filter === f.key;
      return {
        label: f.label,
        count: f.count,
        bg: on ? LINE : 'transparent',
        color: on ? T1 : T2,
        pressed: on ? 'true' : 'false',
        divider: i === 0 ? '0' : '1px solid ' + CTRL,
        pick: () => this.setState({ filter: f.key })
      };
    });
    const aside = {
      count: fmtCount(D.setAside.length),
      rows: asideRows,
      filters: filters,
      note: tech
        ? 'Refused outright: candidate_identity · decision_authority · packet_consistency'
        : 'Never set aside: which version, who may decide, or the whole story’s consistency.'
    };

    // ---- who is waiting on whom ----
    const onYou = D.waiting.filter((w) => you && w.next === you.id).length;
    const others = D.waiting.filter((w) => !(you && w.next === you.id));
    const otherNames = [...new Set(others.map((w) => nameOf(w.next)))];
    const lines = plural(D.setAside.length, 'line', 'lines') + ' set aside';
    const head = {
      onYou: plural(onYou, 'decision is', 'decisions are') + ' waiting on you',
      rest: otherNames.map((n) => ' · ' + fmtCount(others.filter((w) => nameOf(w.next) === n).length) + ' is waiting on ' + n).join('') + ' · ' + lines
    };
    const statusWho = otherNames.map((n) => ' · ' + fmtCount(others.filter((w) => nameOf(w.next) === n).length) + ' on ' + n).join('');

    return {
      isDemo: isDemo,
      banner: {
        show: isDemo,
        head: 'Roadmap.',
        text: ' Team features below are designed, not yet built. Today Team sells the organisation licence, one Paddle invoice and priority support. People and records here are illustrative.',
        link: 'What Team includes today',
        href: null
      },
      plan: {
        label: D.plan.name + ' · ' + fmtCount(D.plan.seats) + ' seats',
        aria: 'Plan: ' + D.plan.name + ', ' + fmtCount(D.plan.seats) + ' seats. Opens plan and licence',
        lead: 'Licensed to ',
        licensee: D.plan.licensee,
        tail: '. One Paddle invoice, priority support. Seat changes: email support.'
      },
      text: {
        crumb: 'rules and decisions read from ' + base,
        travel: 'Decision files travel through ' + base + '. ThruLine never commits, pushes or merges.',
        byFile: 'by file, read from ' + base,
        applies: ' Applies once you commit it and it reaches ' + base + '.'
      },
      project: D.project.name,
      recording: 'Recording · ' + plural(D.recording.projects, 'project', 'projects'),
      readAt: 'read ' + D.readAt + ' ↻',
      yourMove: (D.inbox.yourMove == null ? null : fmtCount(D.inbox.yourMove)),
      yourMoveAria: (D.inbox.yourMove == null ? null : plural(D.inbox.yourMove, 'Change is', 'Changes are') + ' your move'),
      head: head,
      groups: groups,
      face: face,
      seats: seats,
      rules: rules,
      wait: wait,
      templates: templates,
      aside: aside,
      monthDeciders: 'This month, ' + plural(D.activity.monthDeciders, 'person', 'people') + ' approved or set lines aside here.',
      storage: 'ThruLine is keeping ' + gb(D.storage.bytes) + ' of records on this machine · ',
      status: plural(onYou, 'decision', 'decisions') + ' waiting on you' + statusWho + ' · ' + lines + ' · ' + fmtCount(used) + ' of ' + fmtCount(D.plan.seats) + ' seats in use'
    };
  }
}

/* ---------- the live path: what the store holds, and why the rest is not here ----------
 * ThruLine v0.2 has no team model. Seats, groups, approval rules and shared
 * lists of what counts as done are drawn as their section headings with the
 * reason in plain words; approvals and lines set aside are the record's own. */
const NOT_YET = 'Not available in this version.'
export const LIVE_REASONS = Object.freeze({
  groups: NOT_YET + ' Groups would name who may approve which files.',
  rules: NOT_YET + ' Approval rules would say, file by file, how many people must approve a version, read from your repository so a Change cannot loosen its own rule.',
  templates: NOT_YET + ' Shared lists of what counts as done would be agreed once and used by every Change in this repository.',
  seats: NOT_YET + ' A seat would be a person who approves. Recording, reading and checking stay free for everyone.'
})

/* The second-approval slot of one approved version, in words (C30 R-LEDGER).
 * Only what the record holds: who put a name beside the approval and when. No
 * required count is invented — approval rules are not built, so this never says
 * a version is "waiting" on anyone; it says whether a second name is there. */
export function secondSlot(d) {
  const seconds = Array.isArray(d.seconds) ? d.seconds : []
  if (d.withdrawn) return 'Approval taken back, so no second counts'
  if (d.replaced) return 'A later approval replaced this one'
  if (!seconds.length) return 'No second approval yet'
  return 'Seconded · ' + seconds.map((s) => s.who + (s.at ? ' ' + s.at : '')).join(' · ')
}

function liveVals(self, D, tech) {
  const s = self.state;
  const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', LINE = '#2A2A30', CTRL = '#66666F', HAIR = '1px solid #1F1F23';
  const MONO = "'Geist Mono', monospace", SANS = "'Geist', sans-serif";
  const plural = countOf;
  const n = D.projects;
  const standing = D.decisions.filter((d) => !d.withdrawn);
  const moved = standing.filter((d) => d.codeMovedSince);
  const plan = D.plan;

  /* C30 R-LEDGER: the control that puts a second name beside the approval that
   * stands. It is given its writes by the screen (`actions`); without them (the
   * gallery, a test of the words) nothing is drawn, rather than a button that
   * goes nowhere. Only the approval that stands can be seconded. */
  const acts = (self.props && self.props.actions) || {};
  const control = (d) => (d.standing && acts.second && acts.withdraw
    ? React.createElement(SecondControl, {
      key: d.changeId + ':' + (d.full || d.version),
      row: { changeId: d.changeId, full: d.full || '', version: d.version, approver: d.actor || '', seconds: d.seconds || [] },
      second: acts.second, withdraw: acts.withdraw, reload: acts.reload
    })
    : null);
  const decisions = D.decisions.map((d) => {
    const seconded = d.standing && Array.isArray(d.seconds) && d.seconds.length > 0;
    return {
      title: d.title || 'A Change with no title recorded',
      href: href('approve', { change: d.changeId }),
      ref: tech ? (d.recordPath || 'decision file not written') + ' · ' + d.version : d.version + (d.withdrawn ? ' · withdrawn' : ''),
      approvalsAria: 'Approvals of ' + d.version + ', ' + fmtCount(1 + (d.standing && Array.isArray(d.seconds) ? d.seconds.length : 0)) + ' recorded',
      firstBy: 'Approved · ' + (d.actor || 'someone') + ' · ',
      firstAt: d.at || '',
      second: secondSlot(d),
      discBd: seconded ? '1px solid ' + T1 : '1px dashed ' + CTRL,
      secondColor: seconded ? T1 : T3,
      control: control(d)
    };
  });

  const keyOf = (r) => (r.kind === 'waived' ? 'waived' : 'na');
  const shown = D.setAside.filter((r) => s.filter === 'all' || keyOf(r) === s.filter);
  const filterDefs = [
    { key: 'all', label: 'All', count: fmtCount(D.setAside.length) },
    { key: 'waived', label: 'Waived', count: fmtCount(D.setAside.filter((r) => keyOf(r) === 'waived').length) },
    { key: 'na', label: 'Does not apply', count: fmtCount(D.setAside.filter((r) => keyOf(r) === 'na').length) }
  ];
  const aside = {
    count: fmtCount(D.setAside.length),
    rows: shown.map((r, i) => ({
      kind: keyOf(r) === 'waived' ? (tech ? 'waived' : 'Waived') : (tech ? 'not_applicable' : 'Does not apply'),
      kindFont: tech ? MONO : SANS,
      line: r.line || 'A line of what counts as done',
      version: r.version,
      change: r.change || 'A Change with no title recorded',
      // C30 R-NAV: the Change a line was set aside on is known, so its name opens it
      href: r.changeId ? href('brief', { change: r.changeId }) : undefined,
      who: r.by,
      authority: r.authority,
      when: r.when || r.at,
      why: r.why,
      sep: i === 0 ? '0' : HAIR
    })),
    filters: filterDefs.map((f, i) => {
      const on = s.filter === f.key;
      return {
        label: f.label, count: f.count,
        bg: on ? LINE : 'transparent', color: on ? T1 : T2, pressed: on ? 'true' : 'false',
        divider: i === 0 ? '0' : '1px solid ' + CTRL,
        pick: () => self.setState({ filter: f.key })
      };
    }),
    none: D.setAside.length === 0 ? 'Nothing has been set aside on this machine.' : null,
    note: tech
      ? 'Refused outright: candidate_identity · decision_authority · packet_consistency'
      : 'Never set aside: which version, who may decide, or the whole story’s consistency.'
  };

  const lines = plural(D.setAside.length, 'line', 'lines') + ' set aside';
  return {
    isDemo: false,
    live: true,
    avail: { groups: false, rules: false, templates: false, seats: false },
    reason: LIVE_REASONS,
    banner: {
      show: true,
      head: 'Team arrives in a later version.',
      text: ' Seats, approval rules and shared lists of what counts as done are designed, not built. This page shows what this machine has recorded: approvals, second approvals and lines set aside.',
      link: 'See plans',
      href: PLANS_URL // the one named pricing address (PRICING-TRUTHS)
    },
    text: {
      crumb: 'decisions recorded on this machine',
      waitHead: 'Approved versions',
      tagline: 'Approvals and lines set aside, as this machine recorded them',
      travel: 'Each approval is written to .thruline/decisions in its project. ThruLine never commits, pushes or merges.',
      byFile: '',
      applies: ''
    },
    project: D.project.name || 'No project',
    recording: recordingPill({}).text,
    readAt: 'read ' + D.readAt + ' ↻',
    yourMove: (D.inbox.yourMove == null ? null : fmtCount(D.inbox.yourMove)),
    yourMoveAria: (D.inbox.yourMove == null ? null : plural(D.inbox.yourMove, 'Change is', 'Changes are') + ' your move'),
    plan: plan
      ? {
        label: plan.name,
        aria: 'Plan: ' + plan.name + '. Opens plans',
        lead: plan.name === 'Pro' && plan.licensee ? 'Licensed to ' : 'Approving a version needs Pro. Everything else is free.',
        licensee: plan.name === 'Pro' && plan.licensee ? plan.licensee : '',
        tail: plan.name === 'Pro' && plan.licensee ? '. One Stripe invoice, priority support.' : ''
      }
      : { label: 'Plan not read', aria: 'The plan could not be read', lead: 'The licence could not be read.', licensee: '', tail: '' },
    head: {
      onYou: plural(standing.length, 'version is', 'versions are') + ' approved here',
      rest: ' · ' + lines
    },
    groups: [],
    face: {
      plain: tech ? T2 : T1, technical: tech ? T1 : T2,
      label: tech ? 'Face: Technical. Switch to Plain' : 'Face: Plain. Switch to Technical',
      toggle: () => switchFace(self.props, tech ? 'plain' : 'technical', (f) => self.setState({ face: f }))
    },
    seats: { used: null, usedText: '', people: '', segments: [], seated: [], readers: [], hasReaders: false, noReaders: false },
    rules: { count: '', clean: false, edited: false },
    wait: {
      count: fmtCount(decisions.length),
      headText: 'Each approval names one version',
      headColor: T3,
      second: { discBd: '1px dashed ' + CTRL, color: T3 },
      changedAfter: moved.length === 0 ? 'none' : moved.map((d) => d.title || d.version).join(', ')
    },
    decisions: decisions,
    decisionsNone: decisions.length === 0 ? 'No version has been approved on this machine yet.' : null,
    templates: { count: '—' },
    aside: aside,
    monthDeciders: '',
    storage: storageSentence(D.storage.bytes),
    // the storage sentence is said once, on the right (DESIGN-SPEC §2.4)
    status: plural(standing.length, 'version', 'versions') + ' approved · ' + lines
  };
}
