# ThruLine — what it is, and why you can believe what it tells you

This is the one explanation of ThruLine. Part one is in plain words and describes what you see. Part two, marked *(technical)*, names the files, commands, addresses and version numbers behind each part of part one. Every sentence that says what ThruLine does ends with where to check it: `code:` is a file and the name inside it, `test:` is a test that holds the behaviour, `served:` is an address the local server answers and a field in its answer, and `copy:` is the exact words on screen, with their key in the copy file. `scripts/qualification/docs-trace.mjs` rejects this file when one of those does not exist, or when the file and the product disagree (`code: scripts/qualification/docs-trace.mjs traceTokens`).

## Part one — in plain words

### What ThruLine is for

You ask an AI agent for some work, and it says "done". ThruLine reads what the agent actually did and shows what the record backs and what it doesn't. “ThruLine reads the records Claude Code and Codex already write while you work. There are none on this machine yet.” (`copy: empty.no_project.why`) is what it says before it has anything to read. It runs on your own computer, and you open it with one command, `thruline ui`, which opens it in your browser (`code: crates/cli/src/uiserve.rs resolve_bind_address`, `test: crates/cli/tests/ui_server_contract.rs::the_listener_is_on_the_loopback_interface_only`).

The first screen is a list headed “What needs your attention” (`copy: change.inbox.title`). When nothing is tracked yet, it says so and gives one next step: “Nothing needs your attention because nothing is being tracked yet. Start by picking what you are working on. ThruLine proposes a Change from the work it can already see, and writes nothing down until you say so.” (`copy: change.inbox.empty.body`, `test: ui/src/workspace/inbox.test.js::the empty state teaches, and carries exactly one primary action`).

### A Change

A Change is one piece of work you are being asked to accept: a feature, a fix or a tidy-up, possibly spread over several agent sessions. It holds “What was asked for” (`copy: change.define.request_heading`), “What counts as done” (`copy: change.define.accepted_heading`), and “The version this Change is about” (`copy: change.version.label`). A Change is about one version of the code at a time; when the code moves on, the old version stays readable beside the new one (`code: crates/core/src/change.rs read_change`, `test: crates/core/tests/change_records.rs::a_change_binds_one_version_at_a_time_and_keeps_the_old_one`).

The top of a Change answers "what am I being asked to accept?" with one title, one sentence and one named version, and nothing between them (`test: ui/src/workspace/change-workspace.test.js::the top is one title, one sentence and one named version, in that order and nothing between`). The sentence is one of a small fixed set, for example “{with_evidence} of {total} things that count as done have evidence. {without_evidence} still need it.” (`copy: change.conclusion.some_evidence`) or “The record shows a problem with {problems} of {total} things.” (`copy: change.conclusion.problem`). The numbers in it are counted, never written by hand (`code: ui/src/workspace/change-view.js conclusionOf`).

### The six stages

A Change has six stages. Each is a view with its own question, and “These six are views of the same Change. Moving between them settles nothing.” (`copy: change.stages.views_only`, `test: ui/src/workspace/change-workspace.test.js::the strip says in words that moving between stages settles nothing`).

- “Define” (`copy: change.stages.define.name`) — “What counts as done?” (`copy: change.stages.define.question`)
- “Prepare” (`copy: change.stages.prepare.name`) — “What exactly will be examined?” (`copy: change.stages.prepare.question`)
- “Build” (`copy: change.stages.build.name`) — “What changed?” (`copy: change.stages.build.question`)
- “Verify” (`copy: change.stages.verify.name`) — “What happened when we checked it?” (`copy: change.stages.verify.question`)
- “Review” (`copy: change.stages.review.name`) — “What did review find, and was it addressed?” (`copy: change.stages.review.question`)
- “Approve” (`copy: change.stages.approve.name`) — “What exactly am I accepting?” (`copy: change.stages.approve.question`)

A stage with nothing in it says what would fill it, rather than showing a blank (`test: ui/src/workspace/change-workspace.test.js::every one of the six, on an empty Change, renders the sentence that says what would fill it`).

### What counts as done, and what the Change covers

ThruLine suggests what should count as done, read out of what was asked for, and nothing counts until you accept it (`code: crates/core/src/change_propose.rs PROPOSAL_VIEW_SCHEMA_VERSION`, `test: crates/core/tests/change_propose.rs::every_proposal_names_where_it_came_from_and_why`). The button is “Accept what counts as done” (`copy: change.define.accept_action`), and afterwards the screen says “Accepted as what counts as done (list {list}). Accepting this is not approving the work.” (`copy: change.define.accepted_note`). Changing the list later makes a new list; the old one stays readable (`test: crates/core/tests/change_records.rs::a_new_scope_version_supersedes_and_the_old_one_stays_readable`).

A Change covers some folders of the project. “Work outside these folders is not counted for this Change.” (`copy: change.define.covers_accepted_note`). Work that ThruLine only suggested belongs to the Change is not counted either; only work a person confirmed is (`test: crates/core/tests/change_scope_gate.rs::a_suggested_link_is_not_counted`, `test: crates/core/tests/change_scope_gate.rs::a_confirmed_link_whose_work_is_out_of_scope_is_not_counted`).

A picture of a Change — the connected diagram of the plan, the work and the checks for one Change — arrives in a later version (`cite: docs/loop/tasks/P6-IR-4.md`, `cite: docs/loop/INTERFACE-REQUESTS.md`). Today the Change screen says “The outline lists the thread step by step, without the picture. This Change screen draws no picture of the record.” (`copy: shell.sidebar.outline_why.reason`). A single agent session does have its picture, the Trace (`test: ui/src/workspace/trace.test.js::Plain says "step N" where Technical says "seq N"`).

### The words on a line, and what each one means

Each thing that counts as done gets one word saying what its evidence showed, and there are no other words than these (`code: ui/src/workspace/change-model.js criterionState`):

- **Proven** — “The record backs it” (`copy: states.proven.subtitle`). It never appears without what was proven, the version and the folders: “Proven: {what} · version {version} · {scope}” (`copy: change.model.proven_line`, `test: ui/src/workspace/change-model.test.js::"Proven" is never rendered without its scope and its version`).
- **Not proven** — “The record does not back it” (`copy: states.not_proven.subtitle`).
- **In progress** — nothing has been checked against this line yet; what would settle it is “A check that covers this line, run on this version.” (`copy: change.model.inspector.settle_in_progress`).
- **Recorded** — “Something ran, and it settles nothing” (`copy: states.couldnt.subtitle`).
- **Needs a re-check** — it was checked, and the code it depends on changed afterwards (`code: ui/src/workspace/change-model.js criterionState`).
- **Loose thread** — “Promised, and the record shows no follow-through” (`copy: states.loose.subtitle`). This word is used for an agent session, not a line of a Change.
- **Fog** — “Recording gap” (`copy: states.fog.subtitle`): part of what happened was never written down. Also used for an agent session.
- **Ready for approval** — the strongest thing ThruLine says about a Change, and always followed by what it rests on: “Ready for approval for version {version}. It rests on:” (`copy: change.model.readiness.states.readiness_ready`, `test: crates/core/tests/change_readiness.rs::a_ready_answer_never_has_an_empty_rests_on_list`).
- **Approved** — only after a named person approves one version: “Approved on {version}” (`copy: change.model.decision.approved`).

Each line gets exactly one of the first five, worked out from what the record holds, never from what the agent wrote (`test: ui/src/workspace/change-model.test.js::each state the record can be in derives one of the five words, and no sixth`, `test: ui/src/workspace/change-model.test.js::criterionState reads typed fields and never the prose`). Missing evidence is shown as missing, never as a success: the most common reason is “No test ran for this.” (`copy: change.model.readiness.reasons.readiness_nothing_looked`).

### What still needs evidence

When a Change is not ready, it lists what is missing under “{n} things still need evidence:” (`copy: change.model.readiness.states.readiness_needs_evidence`), each line with its reason in plain words (`test: crates/core/tests/change_readiness.rs::every_unsatisfying_class_refuses_with_its_own_reason`). The reasons include:

- “No test ran for this.” (`copy: change.model.readiness.reasons.readiness_nothing_looked`)
- “The record holds the opposite of this.” (`copy: change.model.readiness.reasons.readiness_not_proven`)
- “Something ran, and it settled nothing about this.” (`copy: change.model.readiness.reasons.readiness_settled_nothing`)
- “What was checked was a different version of the code.” (`copy: change.model.readiness.reasons.readiness_other_version`)
- “This was checked, and the files it looked at have changed since.” (`copy: change.model.readiness.reasons.readiness_stale`)
- “The check that covers this was stopped before it finished.” (`copy: change.model.readiness.reasons.readiness_stopped`)

One unmet line holds back the whole answer (`test: crates/core/tests/change_readiness.rs::one_unsatisfied_line_among_many_holds_the_whole_answer`).

### Does the evidence still apply to this version?

Evidence is about the code as it was when the check ran. When files change afterwards, ThruLine works out whether the change touches what the check depended on (`code: crates/core/src/evidence_applicability.rs invalidating_paths`). If it doesn't, the line says “Still applies — {path} unchanged since {version}.” (`copy: change.coverage.freshness.applicability_still_applies.plain`, `test: crates/core/tests/evidence_applicability_rule.rs::an_untouched_criterion_still_applies`). If it does, the line says “Proven on the previous version — needs a re-check.” (`copy: change.coverage.freshness.applicability_needs_a_recheck.plain`, `test: crates/core/tests/evidence_applicability_rule.rs::editing_a_declared_scope_path_invalidates_the_criterion`), and gives the reason, such as “The affected code changed afterward.” (`copy: change.coverage.freshness.applicability_reason_source_changed.plain`) or “A dependency changed afterward.” (`copy: change.coverage.freshness.applicability_reason_dependencies_changed.plain`). If nobody has worked this out, the line does not claim the evidence still applies (`test: ui/src/workspace/change-model.test.js::an absent freshness assessment is not read as "still applies"`).

### What do I do now?

Every stage has exactly one main action, and every button that is off says why and what would turn it on (`test: ui/src/workspace/change-workspace.test.js::every view has exactly one primary action, and every disabled control says why and what would enable it`). On a line where the record shows something broke, the main action is “Look at what broke” (`copy: change.model.actions.look`). On a line that needs a re-check, the main action opens the run sheet (`code: ui/src/workspace/change-workspace.jsx runsFromHere`).

The Change screen offers “Check it again” to open the run sheet (`code: ui/src/workspace/change-workspace.jsx RunDoor`). The screen also offers a command you can copy to run a check in a terminal (`code: ui/src/workspace/change-workspace.jsx runCheckCommand`).

When the next step is to tell the agent something, the finding comes with a message written for you to paste, behind the button “Copy prompt for the agent” (`copy: chrome.copy_prompt`, `test: ui/src/workspace/verdict-panel.test.js::the first card carries the primary Copy prompt button and later cards do not`). For example: “Re-run the check that covers "{claim_quote}" on the current code and paste the full output, including the command you ran.” (`copy: families.verdict/*/not_proven.manager.prompt`). A prepared repair message for a whole Change arrives in a later version (`lane: thruline-review:docs/loop/review/tasks/T-4-3.md@e3b78f5e`).

### What review found, and what was set aside

The Review stage says, while nothing is written down, “No review has been written down for this Change yet, so there is nothing here to read.” (`copy: change.stages.review.empty`). Recorded local reviews of one version appear in the Review stage (`code: ui/src/workspace/change-review.jsx ReviewSection`, `test: ui/src/workspace/review-model.test.js::T-4-1 (rendered): the Review stage keeps one primary, off, with its reason and what would turn it on`). That stage also shows where each recorded finding stands (`code: ui/src/workspace/change-workspace.jsx FindingStandings`, `test: ui/src/workspace/review-model.test.js::T-4-2 (rendered, Review stage): the stage lists where each stands and keeps its one primary`). The review runtime is not part of this launch (`ruling: R-2026-09-28-15`). Starting a review from the Change screen is not built yet (`code: ui/src/workspace/hq-copy.js review_from_terminal`).

A named person can set one thing that counts as done aside, but only if what counts as done for the Change says who is allowed to (`test: crates/cli/tests/change_exceptions_cli.rs::with_no_exception_path_configured_nothing_can_be_set_aside_and_nothing_is_appended`). Otherwise the screen says “Nothing can be set aside on this Change: what counts as done for it does not say who is allowed to do that. Say so in what counts as done first, naming who decides.” (`copy: change.model.readiness.no_exception_path`). A set-aside line is never shown as evidence: it sits under “Set aside, with nobody checking:” (`copy: change.model.readiness.set_aside_heading`) and reads “Waived by {who}, under {authority}: {why}. Nobody checked this.” (`copy: change.model.readiness.waived`, `test: crates/core/tests/change_readiness.rs::a_waiver_clears_the_line_and_is_never_counted_as_evidence`). Setting something aside is done from a terminal with `thruline change waive`; there is no button for it on the screen yet (`code: crates/cli/src/change_cmd.rs waive`). Three things can never be set aside by anybody (`test: crates/cli/tests/change_exceptions_cli.rs::the_three_prerequisites_are_refused_even_with_an_exception_path_configured`).

### Not in this release

<!-- no-trace: a statement of what this release leaves out; its sources are the rulings and case files cited beside it -->
Not in this release: an AI reviewer (PR-Agent) and the review runtime that runs it; review findings from that reviewer; and a review verb, or a review profile on `thruline change accept`, that records a review as required (`ruling: R-2026-09-28-37`, `cite: spec/qualification/cases/Q01.json`). The review runtime and AI review are not part of the launch build (`ruling: R-2026-09-28-37`, `ruling: R-2026-09-28-20`).

### The whole story, and the file you can send

“The whole story” (`copy: change.packet.door`) is one page with everything about the Change, in a fixed order, top to bottom; a part with nothing in it says so in its own sentence (`test: ui/src/workspace/packet.test.js::every section with nothing in it renders its own sentence from the copy file`). From it, “Save the whole story as a file” (`copy: change.export.door`) shows exactly what the file will hold before anything is saved: “This is exactly what the file will hold, word for word. Saving keeps it on this Mac. Nothing is sent anywhere. If you then send the file to somebody, this is what they will read.” (`copy: change.export.lead`, `test: ui/src/workspace/export-sheet.test.js::(B.1, in a DOM) door → sheet; for each format and a redacted copy, the bytes saved are the bytes previewed; nothing is fetched`).

The file comes as a web page, plain text or JSON, and the same record gives the same file, byte for byte, every time (`test: ui/src/workspace/packet-export.test.js::three formats, each with its own name, type and text; the same inputs give the same bytes`). A copy with private details taken out is a second, labelled file that says what was removed and how many; the first file is never edited (`test: ui/src/workspace/packet-export.test.js::(B.4) the derivative opens with the label naming each class and how many; every removal is marked; nothing removed survives`).

### What approving does, and what it does not do

To approve, press “Approve this version” and confirm in the sheet that opens (`code: ui/src/workspace/change-workspace.jsx DecisionSheet`), and wherever it is offered the screen says “It does not merge, deploy or tell the agent anything.” (`copy: change.model.decision.not_an_action`). An approval names exactly one version, one person, one time and the list of what counted as done (`test: crates/core/tests/change_decision.rs::an_approval_names_one_version_one_actor_one_time_and_one_accepted_criteria_version`). It needs a name: “An approval has to have a name on it.” (`copy: change.model.decision.actor_missing`).

An approval never moves to a later version of the code (`test: crates/core/tests/change_decision.rs::an_approval_given_for_one_version_does_not_move_to_the_next_one`). If the code changes afterwards, the approval still shows, with the change said beside it: “Approved on {version} — code changed since” (`copy: change.model.decision.approved_moved`). If the code moves while you are looking at the screen, approving is refused: “Not approved. The code moved while this screen was open, so approving now would have approved something other than what is on your screen.” (`copy: change.model.decision.moved_refused`, `test: crates/core/tests/change_decision.rs::approving_a_version_the_record_has_moved_off_is_refused_and_names_what_changed`). An approval given with something set aside is labelled differently: “Approved with exceptions on {version}” (`copy: change.model.decision.approved_with_exceptions`).

You can take an approval back with “Withdraw approval” (`copy: change.model.decision.withdraw`). The withdrawal is written down next to the approval; neither is edited or deleted (`test: crates/core/tests/change_decision.rs::a_withdrawal_is_its_own_row_and_leaves_both_acts_readable`, `test: crates/core/tests/change_decision.rs::a_recorded_decision_cannot_be_edited_or_deleted`).

Everything in ThruLine is free except approving a version, which needs a Pro licence (`copy: licence.page.needs`, `code: crates/licence/src/lib.rs PAID`). With no licence installed, status says “Free. Approving a version needs a Pro licence.” (`copy: licence.status.absent.problem`, `test: crates/cli/tests/licence_flow_cli.rs::a_fresh_home_is_free_and_approving_is_off_without_pro`). The Approve door points to plans and to installing a licence file (`copy: licence.approve.plans`, `copy: licence.page.install_command`, `test: ui/src/workspace/licence.test.js::C20: an unlicensed Approve gives the Pro requirement, pricing link, and licence-install path`). Recording, reading, checking, linking evidence, stopping a check, withdrawing an approval and exporting your own records work without one (`copy: licence.page.free`).

### What ThruLine keeps on this machine

ThruLine keeps its records in one folder on your computer, and says how big it is (approving a version also writes one small file into your project's own folder, described under "What approving does"): “ThruLine is keeping {size} of records on this machine.” (`copy: change.inbox.storage.sentence`, `served: GET /api/storage bytes`, `test: crates/cli/src/uiserve.rs::the_storage_route_counts_every_file_in_the_folder_it_names`). It also says “Nothing in this version removes any of it, and nothing on this screen deletes anything.” (`copy: change.inbox.storage.note`).

How big is that? Measured on a fresh, empty folder: after reading one agent session of 23,593 bytes, the folder held 1,261,720 bytes (about 1.2 MB), of which 1,040,384 bytes were the records themselves, 184,320 the log of what ran, 32,305 a word-for-word copy of the session and 4,711 the copy the screen reads (`served: GET /api/storage parts`). It grows with every session: on the machine this file was written on, after months of one person's use, the folder was about 18 GB. The folder is readable only by your own user account (`test: crates/core/src/private_store.rs::a_newly_created_store_is_not_group_or_other_readable`).

Offering to remove stored records you no longer need — saying what would be lost before you agree — arrives in a later version (`cite: docs/INTERFACE-REQUESTS.md`, `cite: docs/loop/03-USER-LENS.md`).

### What leaves this machine

Nothing, in ordinary use. The screen is served only to your own computer and refuses any other address (`code: crates/cli/src/uiserve.rs resolve_bind_address`, `test: crates/cli/tests/ui_server_contract.rs::a_non_loopback_bind_address_is_refused_and_nothing_is_bound`). When ThruLine uses a language model, it only talks to one running on this computer and refuses any other address (`code: crates/core/src/mind.rs is_loopback`, `test: crates/core/tests/rb6a_mind.rs::the_ollama_provider_refuses_a_non_loopback_endpoint`). Saving the whole story as a file hands it to your browser and fetches or sends nothing (`test: ui/src/workspace/export-sheet.test.js::(B.1, in a DOM) door → sheet; for each format and a redacted copy, the bytes saved are the bytes previewed; nothing is fetched`). The exceptions are listed in part two, under "What leaves this machine (technical)".

## Part two — the parts behind it (technical)

### The Change record (technical)

A Change is stored in the local ledger and served as one `ChangeView`: the request; the standing accepted scope (criteria and folders covered) and every earlier scope, superseded and never deleted; the implementation binding (one target revision at a time) and its history; associations to sessions with provenance; `examination` and `decision` placeholders that read as placeholders, not absences; `decisions`, the approval rows; and the masking treatment applied to its text (`code: crates/core/src/change.rs ChangeView`, `test: crates/core/tests/change_records.rs::a_change_is_stored_and_read_back_with_every_component`, `test: crates/core/tests/change_records.rs::examination_and_decision_are_placeholders_not_absences`). It is served whole at `GET /api/changes/<id>` (`served: GET /api/changes/<id> decisions`). An association with a machine basis can only be a suggestion; a confirmation needs a named actor and is appended beside the suggestion (`test: crates/core/tests/change_records.rs::no_machine_basis_can_produce_a_confirmed_association`, `test: crates/core/tests/change_records.rs::a_suggestion_becomes_a_confirmation_only_by_appending_beside_it`). Served text is masked before it leaves the store (`code: crates/core/src/redact.rs mask`, `test: crates/core/tests/change_records.rs::served_text_is_masked_before_it_leaves_the_store`).

The scope gate counts a session's result for a Change only when the association is confirmed and the session's changed files are inside the accepted scope; it has exactly one caller (`code: crates/core/src/change_link.rs SCOPE_GATE`, `test: crates/core/tests/change_scope_gate.rs::a_confirmed_link_inside_scope_is_counted`, `test: crates/core/tests/change_scope_gate.rs::the_scope_gate_has_exactly_one_caller_and_it_is_named_here`).

Prepare writes an examined-inputs record — repository, base, snapshot, workspace — that is never edited or removed, and refuses the word `unknown` as a value (`code: crates/core/src/examined_inputs.rs INPUTS_VIEW_SCHEMA_VERSION`, `test: crates/core/tests/examined_inputs_record.rs::a_record_is_never_edited_or_removed`, `test: crates/core/tests/examined_inputs_record.rs::the_word_unknown_is_refused_as_a_value_by_the_module_and_by_the_database`).

### Evidence semantics (technical)

Coverage (`served: GET /api/changes/<id>/coverage rows`) links each accepted criterion to evidence items by a basis — declared by the check profile, named by the evidence's own typed subject, or linked manually by a person; timing is not a basis the store accepts (`test: crates/core/tests/change_evidence_rule.rs::the_profile_declares_a_check_for_the_criterions_class`, `test: crates/core/tests/change_evidence_rule.rs::timing_is_not_a_basis_the_store_will_accept`). A run from work that was only suggested never links (`test: crates/core/tests/change_evidence_rule.rs::a_run_from_work_that_was_only_suggested_never_links`).

The browser derives one state per criterion from typed fields in `criterionState`: a stale bearing item gives `needs_recheck`; any bearing `not_proven` gives `not_proven`; any bearing `proven` gives `proven`; items that bear nothing give `recorded`; no items give `in_progress` (`code: ui/src/workspace/change-model.js criterionState`). Execution is a second, separate field — not run, running, completed, cancelled, interrupted, blocked — so a killed run is never read as its last printed line (`code: crates/core/src/run_execution.rs RUN_EXECUTION_SCHEMA_VERSION`, `test: crates/cli/tests/change_cancel_run_cli.rs::a_cancelled_check_never_becomes_a_completed_one_when_the_wrapper_reaps_it`). The session Verdict tab's mapping from engine states to state words is the table at the head of `docs/HQ-COPY.md`, generated from the copy file (`code: ui/src/workspace/hq-copy.js HQ_COPY`).

Readiness (`served: GET /api/changes/<id>/readiness rests_on`) is computed from coverage, waivers and the recorded review policy; it is never ready on an empty criteria set or on a version it cannot name, and the same records give the same answer (`code: crates/core/src/change_readiness.rs compute`, `test: crates/core/tests/change_readiness.rs::an_empty_criteria_set_is_never_ready`, `test: crates/core/tests/change_readiness.rs::a_version_it_cannot_name_is_never_ready`, `test: crates/core/tests/change_readiness.rs::the_same_records_give_the_same_answer`).

### Applicability and freshness (technical)

An applicability record states whether one piece of linked evidence still applies after the tree moved or the scope changed, with one of five reason classes: source, tests, dependencies (a lockfile), configuration, criteria changed (`code: crates/core/src/evidence_applicability.rs classify_path`, `test: crates/core/tests/evidence_applicability_rule.rs::each_reason_class_is_produced_by_its_matching_change`). Paths declared for a criterion invalidate only that criterion; with none declared, the rule falls back to the whole accepted scope (`test: crates/core/tests/evidence_applicability_rule.rs::declared_paths_only_invalidate_their_own_paths`, `test: crates/core/tests/evidence_applicability_rule.rs::undeclared_falls_to_the_conservative_allowed_scope`). Records are append-only; nothing in the module issues an update or a delete (`test: crates/core/tests/evidence_applicability_rule.rs::no_writer_in_this_module_ever_issues_update_or_delete_against_the_table`). They are read at `GET /api/changes/<id>/applicability`, which derives nothing and writes nothing (`served: GET /api/changes/<id>/applicability records`).

### Dispositions: obligation waivers (technical)

A waiver is one `change_obligation_waivers` row, kind `waived` or `not_applicable`, naming an actor, the authority the accepted profile records, and a reason; an incomplete waiver disposes of nothing, and a waiver never hides a result the record holds (`code: crates/core/src/change_readiness.rs ObligationWaiver`, `test: crates/core/tests/change_readiness.rs::a_waiver_needs_an_actor_an_authority_and_a_reason`, `test: crates/core/tests/change_readiness.rs::a_waiver_never_hides_a_result_the_record_holds`). The candidate-identity, decision-authority and packet-consistency obligations cannot be waived (`test: crates/core/tests/change_readiness.rs::the_candidate_identity_prerequisite_holds_readiness_and_cannot_be_waived`). The table was renamed from `change_dispositions` by a migration that moves no row's bytes (`test: crates/core/tests/obligation_waiver_rename_migration.rs::the_waivers_survive_the_rename_byte_for_byte`).

### Packet versions and the export file (technical)

The whole-story page is `hq.packet`, derived once by `packetOf` from the one view-model; the exported file reads `hq.packet.sections[]` and composes sentences from copy strings, never re-deriving a count (`code: ui/src/workspace/change-model.js packetOf`, `test: ui/src/workspace/packet-export.test.js::(B.2a) every digit run in every format is a typed field of the model or of the records answer`). The file is named `thruline-packet-<change>-<version7>[-redacted].<md|html|json>`, its JSON carries `EXPORT_SCHEMA_VERSION`, and a redacted file carries `derived_from_sha256` naming the full file (`code: ui/src/workspace/packet-export.js EXPORT_SCHEMA_VERSION`, `test: ui/src/workspace/packet-export.test.js::(B.4) traceable provenance: derived_from_sha256 is the full file's hash, and the full file is byte-identical either way`). The HTML is self-contained and loads nothing (`test: ui/src/workspace/packet-export.test.js::(B.6) the HTML is self-contained: no external reference of any kind, a policy that loads nothing, the constant stylesheet`).

`GET /api/changes/<id>/export` reports whether each record the file rests on can be read now — readable, missing, permission denied, content changed, dangling reference, never written — and appends nothing (`code: crates/core/src/change_export.rs records`, `served: GET /api/changes/<id>/export totals`, `test: crates/core/tests/change_export.rs::looking_writes_nothing`, `test: crates/core/tests/change_export.rs::a_rewritten_decision_copy_is_content_changed`). A POST to that address is answered 405 (`test: crates/cli/tests/change_export_route.rs::the_address_takes_no_query_no_other_method_and_no_unknown_change`).

### Decisions and approval boundaries (technical)

`POST /api/changes/<id>/decisions` and `…/decisions/withdraw` each append one `change_decisions` row and write `.thruline/decisions/<version>.json` under the project's recorded worktree; nothing else (`code: crates/core/src/change_decision.rs approve`, `code: crates/core/src/change_decision.rs record_path_for`, `test: crates/core/tests/change_decision.rs::the_json_record_exists_under_the_project_with_the_named_fields`). That file is a derived copy of the latest act on its version: a withdrawal replaces it with a copy naming the withdrawal row, and a later approval of the same version replaces it again (`test: crates/core/tests/change_decision.rs::withdrawal_replaces_the_derived_copy_with_its_exact_row`, `test: crates/core/tests/change_decision.rs::reapproval_replaces_the_derived_copy_with_the_second_approval`). Approving after a commit is refused and names the committed file; two approvals at the same instant leave exactly one row (`test: crates/core/tests/change_decision.rs::approving_after_a_commit_is_refused_and_the_refusal_names_the_committed_file`, `test: crates/core/tests/change_decision.rs::two_approvals_at_the_same_instant_leave_exactly_one_row`). The served decisions shape is `DECISION_RECORD_SCHEMA_VERSION` (`code: crates/core/src/change_decision.rs DECISION_RECORD_SCHEMA_VERSION`).

### Schemas and served-shape versions (technical)

Every schema file under `spec/` (for a JSON Schema its `$id`, otherwise its top-level `schema`; fixtures excluded), and every constant whose name contains `SCHEMA` with a literal value in crate sources and `ui/src`, read from code by `schemaInventory` (`code: scripts/qualification/docs-trace.mjs schemaInventory`). Two served shapes carry an inline version rather than a constant: `GET /api/storage` answers `schema_version` 4 and the first-use answers carry 1 (`code: crates/cli/src/uiserve.rs storage`, `code: crates/cli/src/uiserve/first_use.rs overview`).

The qualification case format, `spec/qualification/case.schema.json` and the nineteen `spec/qualification/cases/Q*.json` files that carry it, is the input format of the Phase 7 qualification harness, not a product surface: nothing ThruLine serves or prints reads it (`code: spec/qualification/tools/validate.mjs validate`, `test: spec/qualification/tools/validate.test.mjs::the committed tree: 19/19 valid, every check ran`).

| Path | Name | Value |
|---|---|---|
| `spec/execution-check-v0.1.schema.json` | `$id` | `https://thrulinehq.com/spec/execution-check/v0.1` |
| `spec/execution-check-v0.2.schema.json` | `$id` | `https://thrulinehq.com/spec/execution-check/v0.2` |
| `spec/integrations/catalog.json` | `schema` | `thruline.integrations.v1` |
| `spec/integrations/coverage.json` | `schema` | `thruline.integration-coverage.v1` |
| `spec/integrations/qualification/expanded-cases.json` | `schema` | `thruline.integration-qualification-cases.v2` |
| `spec/integrations/qualification/status.json` | `schema` | `thruline.qualification-status.v1` |
| `spec/integrations/qualification/upstream-cases.json` | `schema` | `thruline.integration-qualification-cases.v1` |
| `spec/integrations/upstream-sources.lock.json` | `schema` | `thruline-upstream-source-lock/v1` |
| `spec/review-integration/initial-capabilities.json` | `schema` | `thruline.review-capabilities.v1` |
| `spec/qualification/case.schema.json` | `$id` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q01.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q02.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q03.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q04.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q05.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q06.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q07.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q08.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q09.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q10.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q11.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q12.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q13.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q14.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q15.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q16.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q17.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q18.json` | `schema` | `thruline-qualification-case@1` |
| `spec/qualification/cases/Q19.json` | `schema` | `thruline-qualification-case@1` |
| `spec/review-integration/t4-1-pr-agent-capability.json` | `schema` | `thruline.review-capability.v1` |
| `crates/attest/src/statement.rs` | `SCHEMA_ID` | `https://thrulinehq.com/spec/execution-check/v0.1` |
| `crates/cli/src/uiserve.rs` | `APPLICABILITY_VIEW_SCHEMA_VERSION` | `2` |
| `crates/cli/src/uiserve/run_route.rs` | `RUN_VIEW_SCHEMA_VERSION` | `3` |
| `crates/core/src/capture.rs` | `CAPSULE_SCHEMA_VERSION` | `1` |
| `crates/core/src/capture_product.rs` | `M5R2_PROFILE_SCHEMA_VERSION` | `1` |
| `crates/core/src/change.rs` | `CHANGE_VIEW_SCHEMA_VERSION` | `8` |
| `crates/core/src/change_decision.rs` | `DECISION_RECORD_SCHEMA_VERSION` | `3` |
| `crates/core/src/change_decision.rs` | `DECISION_RECORD_SCHEMA_VERSION_2` | `2` |
| `crates/core/src/change_evidence.rs` | `COVERAGE_VIEW_SCHEMA_VERSION` | `1` |
| `crates/core/src/change_export.rs` | `CHANGE_EXPORT_SCHEMA_VERSION` | `1` |
| `crates/core/src/change_link.rs` | `LINK_VIEW_SCHEMA_VERSION` | `1` |
| `crates/core/src/change_propose.rs` | `PROPOSAL_VIEW_SCHEMA_VERSION` | `1` |
| `crates/core/src/change_readiness.rs` | `READINESS_SCHEMA_VERSION` | `2` |
| `crates/core/src/change_review.rs` | `BINDING_SCHEMA` | `thruline.change-review-binding.v1` |
| `crates/core/src/evidence_binding.rs` | `SCHEMA_VERSION` | `1` |
| `crates/core/src/examined_inputs.rs` | `INPUTS_VIEW_SCHEMA_VERSION` | `1` |
| `crates/core/src/finding_standing.rs` | `FINDINGS_VIEW_SCHEMA_VERSION` | `2` |
| `crates/core/src/mind_grounded.rs` | `SCHEMA_VERSION` | `grounded-drafts-v1` |
| `crates/core/src/model_supply.rs` | `MODEL_STATUS_SCHEMA` | `thruline-nli-model-status-v1` |
| `crates/core/src/model_supply.rs` | `MODEL_IMPORT_SCHEMA` | `thruline-nli-model-import-v1` |
| `crates/core/src/observation.rs` | `SCHEMA_VERSION` | `1` |
| `crates/core/src/run_execution.rs` | `RUN_EXECUTION_SCHEMA_VERSION` | `1` |
| `crates/core/src/speaker.rs` | `DIGEST_SCHEMA` | `thruline_digest_v0` |
| `crates/core/src/spec/event.rs` | `EVENT_SCHEMA_VERSION` | `1` |
| `crates/core/src/test_tally.rs` | `TEST_TALLY_SCHEMA_VERSION` | `1` |
| `crates/ledger/src/schema.rs` | `SCHEMA_VERSION` | `4` |
| `crates/licence-issue/src/store.rs` | `SCHEMA_VERSION` | `1` |
| `ui/src/lib/mind.js` | `MIND_SCHEMA_VERSION` | `grounded-drafts-v1` |
| `ui/src/workspace/packet-export.js` | `EXPORT_SCHEMA_VERSION` | `1` |

The Change view serves these schema 8 keys (QUAL-F2-R2): `schema_version`, `id`, `project_id`, `opened_at`, `request`, `accepted_scope`, `scope_history`, `implementation`, `implementation_history`, `associations`, `examination`, `decision`, `decisions`, `notes`, `seconds`, `reviews`, `code_moved`, `retrospective`, `masking` (`code: crates/core/src/change.rs ChangeView`).

### JSON shapes (technical)

Every `--json` shape the binary emits, with its stable id and version, exactly as `thruline json-shapes` prints it (`code: crates/cli/src/json_out.rs JSON_SHAPES`).

| Id | Command | Version | Surface | Note |
|---|---|---|---|---|
| `thruline.acceptance.init` | `acceptance init` | `1` | --json | unaccepted draft and exact document digest |
| `thruline.acceptance.accept` | `acceptance accept` | `1` | --json | local acceptance; never a completion result |
| `thruline.acceptance.status` | `acceptance status` | `1` | --json | accepted requirements and outstanding checks |
| `thruline.acceptance.run` | `acceptance run` | `1` | --json | one explicitly authorized local attempt and current requirement evaluation |
| `thruline.acceptance.receipt` | `acceptance receipt` | `1` | --json | detached v0.2 local acceptance receipt location and identity |
| `thruline.verify-receipt.session` | `verify-receipt` | `2` | --json | v0.2 session provenance, integrity and D22 conclusion |
| `thruline.verify-receipt.acceptance` | `verify-receipt` | `2` | --json | v0.2 acceptance branch; signature and current local evidence are distinct |
| `thruline.acceptance.resume` | `acceptance resume` | `1` | --json | explicit local handoff acknowledgement after identity and evidence comparison |
| `thruline.acceptance.handoff` | `acceptance handoff` | `1` | --json | manual packet; export is not confirmed recovery |
| `thruline.now` | `now` | `1` | --json |  |
| `thruline.shadow-sessions` | `shadow-sessions` | `1` | --json |  |
| `thruline.entity.list` | `entity list` | `1` | --json |  |
| `thruline.verify-claims` | `verify-claims` | `1` | --json |  |
| `thruline.why.record` | `why` | `1` | --json | the truth-loop rendering of one recorded finding |
| `thruline.why.attention` | `why` | `1` | --json | the attention-queue view of one item |
| `thruline.evidence.one` | `evidence` | `1` | --json | one binding, named by --id |
| `thruline.evidence.list` | `evidence` | `1` | --json | every binding for the repository at the working directory |
| `thruline.doctor` | `doctor` | `2` | --json | every check doctor ran, and `ok` over them. Version 2 adds `note` to each finding: true for a check that did not apply where doctor ran (the working-tree check outside a git repository), which is reported and not counted against `ok` |
| `thruline.sessions` | `sessions` | `2` | --json | the execution store's sessions, ranked; `total` and `returned` are both present because the list is bounded and a bounded list that does not say so implies it showed everything. Version 2 adds limit_scope: store for the default global bound, project for --per-project |
| `thruline.claims` | `claims` | `1` | --json |  |
| `thruline.explain` | `explain` | `1` | --json |  |
| `thruline.promises` | `promises` | `1` | --json |  |
| `thruline.receipt.dsse` | `receipt` | `1` | --json | a DSSE envelope (in-toto Statement, BUILD-SPEC §8.1). Its exact bytes are hashed into the `ThruLine-Receipt:` commit trailer printed beside it, so any added field breaks the binding the receipt exists to make. It is versioned by in-toto's own `predicateType`. |
| `thruline.verify-receipt` | `verify-receipt` | `1` | --json |  |
| `thruline.verify-receipt.check-run` | `verify-receipt` | `1` | --format check-run | GitHub's check-runs API request body, posted verbatim by the Action. The shape belongs to GitHub; a ThruLine field in it is sent to a third party's endpoint and versions nothing we own. |
| `thruline.reexec` | `reexec` | `1` | --json |  |
| `thruline.shim.install` | `shim install` | `1` | --json |  |
| `thruline.shim.uninstall` | `shim uninstall` | `1` | --json |  |
| `thruline.shim.status` | `shim status` | `1` | --json |  |
| `thruline.fold` | `fold` | `1` | --json |  |
| `thruline.run` | `run` | `1` | --json |  |
| `thruline.import-codex` | `import-codex` | `1` | --json |  |
| `thruline.import-codex.dry-run` | `import-codex` | `1` | --json | `--dry-run`: a survey of first lines, which never touches the store |
| `thruline.import-legacy` | `import-legacy` | `1` | --json |  |
| `thruline.sync` | `sync` | `1` | --json |  |
| `thruline.policy.init` | `policy` | `1` | --json | `policy --init`: whether a template was written |
| `thruline.policy.check` | `policy` | `1` | --json | the evaluated conclusion, including the no-policy-is-not-a-failure case |
| `thruline.bench` | `bench` | `1` | --json |  |
| `thruline.demo` | `demo` | `1` | --json |  |
| `thruline.hook-install` | `hook-install` | `1` | --json |  |
| `thruline.hook-install.refused` | `hook-install` | `1` | --json | the refusal when another ThruLine hook pack already owns the settings file |
| `thruline.capture.install` | `capture install` | `1` | --json |  |
| `thruline.capture.status` | `capture status` | `1` | --json |  |
| `thruline.capture.doctor` | `capture doctor` | `1` | --json |  |
| `thruline.change.new` | `change new` | `1` | --json | one Change, opened: the id it will be known by, the project it is about, what was asked for and who asked. Nothing about what counts as done, because opening a Change decides none of it |
| `thruline.change.suggest` | `change suggest` | `1` | --json | proposed session links with the basis and the evidence for each, plus what the suggester could not decide: ties, undated sessions and scope lines it does not understand. Nothing here is a confirmed link |
| `thruline.change.associate` | `change associate` | `1` | --json | one person's decision about one link, appended beside the suggestion it answers; never an edit of it |
| `thruline.change.results` | `change results` | `1` | --json | every session linked to a Change with either its recorded verdict counts or the reason they are not counted here. A session that fails the scope gate is listed with a sentence and no numbers |
| `thruline.change.links` | `change links` | `1` | --json | one session's side of the same fact, including the case where it belongs to no Change at all |
| `thruline.change.cancel-run` | `change cancel-run` | `1` | --json | what became of one running check after a person asked for it to stop: whether it was cancelled at all, who asked, and what was found when its process group was looked at afterwards. "Nothing looked" is its own answer and is never reported as a clean stop |
| `thruline.change.waive` | `change waive` | `1` | --json | what became of one request to set an obligation aside: whether it was recorded at all, and when it was, who set it aside, the accepted policy's authority it rests on and the reason they gave. A refusal is reported as a refusal and exits non-zero; nothing here is ever a result about the obligation |
| `thruline.change.finding` | `change finding` | `1` | --json | one person's change to where one thing a review found stands: the appended row's from and to, or the refusal and what was missing; a resolved mark never reads as fixed |
| `thruline.change.prepare` | `change prepare` | `1` | --json | what exactly will be examined for one Change: the project folder, the starting point, whether unsaved changes are included, the tools that would run with their versions, and the running application this Change is about. Every field a probe could not answer is unknown, and unknown is never a value |
| `thruline.change.propose` | `change propose` | `1` | --json | proposed criteria cut from the request and the confirmed work, each with the request it came from and the reason it is here; the sentences that could not be cut into three parts, each naming the part it was missing; and a suggested check profile in which an unavailable check is still listed. Nothing here is accepted and nothing here has run |
| `thruline.change.accept` | `change accept` | `1` | --json | one person writing down what counts as done: a new scope version, its acceptance record, and whose words each line is in. Accepting is not approving, and the document says so in both registers |
| `thruline.licence.status` | `licence status` | `4` | --json | whether this machine has a licence that works, what would fix it, the one paid approval entitlement and `paid_gate`; the same document `GET /api/licence` serves |
| `thruline.json-shapes` | `json-shapes` | `1` | always | the registry listing itself |

### Command line (technical)

Every verb the built binary prints in its help, and no other, walked recursively (`code: crates/cli/src/main.rs Command`). The description is the first sentence the binary itself prints. `thruline ui` is the door for people; the binary's own help says everything from `packet` down is written for agents, integrations and maintenance.

| Verb | What the binary says it does |
|---|---|
| `thruline ui` | Open ThruLine in your browser. |
| `thruline acceptance` | Review and preserve required checks independently of an agent's claims |
| `thruline acceptance init` | Write an unaccepted draft from the repository's Python or npm scripts |
| `thruline acceptance accept` | Accept the exact reviewed document. |
| `thruline acceptance status` | Show accepted requirements and any unaccepted draft change |
| `thruline acceptance run` | Execute one accepted check once, within its declared timeout |
| `thruline acceptance handoff` | Export a manual continuity packet; export does not confirm a handoff |
| `thruline acceptance resume` | Compare a reviewed handoff with this workspace and record local acknowledgement |
| `thruline acceptance receipt` | Sign the current accepted-requirement result as a detached local receipt |
| `thruline demo` | Watch ThruLine prove itself, end to end, in a throwaway directory |
| `thruline init` | Set ThruLine up for Claude Code on this machine, in one command: the SENSOR hook pack, the speaker hooks (the in-session truth loop), and the context server. |
| `thruline licence` | Your licence: its status, installing or removing it, and this machine's fingerprint. |
| `thruline licence status` | Say, in one sentence, whether this machine has a licence that works |
| `thruline licence install` | Check a licence file and, only if it works, install it. |
| `thruline licence remove` | Delete the installed licence file. |
| `thruline licence fingerprint` | Show this machine's fingerprint, for a licence tied to one machine. |
| `thruline now` | What needs me? |
| `thruline why` | Why should I believe it? |
| `thruline ingest` | Ingest transcripts from Claude Code, Codex, or unambiguous defaults |
| `thruline dismiss` | Dismiss one attention item. |
| `thruline autopsy` | What this machine's own records hold — sixty seconds, zero config, nothing leaves this device |
| `thruline check` | Run YOUR command through ThruLine so its evidence binds to the exact workspace state: `thruline check -- cargo test --workspace`. |
| `thruline evidence` | What command evidence is bound to this workspace, and whether it still applies — re-observed now, stated in the §9 vocabulary. |
| `thruline snapshot` | A read-only capture of the workspace as it stands: lineage, the dirty tree with digests, environment names (never values), processes — and, in a git worktree, the protected byte layer (A208/§14): every tracked, staged, and in-scope untracked byte, encrypted at rest |
| `thruline packet` | Compile a deterministic Context Packet (M6.R3). |
| `thruline protect` | Preserve this work: one read-only workspace snapshot (byte layer included, A208/§14) plus a Recovery Capsule that cites it. |
| `thruline handoff` | Render a frozen packet as the handoff artifact (R17b-2b) |
| `thruline restore` | Materialize a protected snapshot byte-exactly — content, modes, symlinks, deletions, untracked files, and the staged/worktree duality — into a separate safe directory. |
| `thruline mcp` | Serve the three MCP tools over stdio (A89). |
| `thruline rescue` | Prepare a fresh session to continue this work. |
| `thruline verify-claims` | Evaluate every claim_completion submission awaiting verification (A201). |
| `thruline deltas` | What changed since a chain position you were served (R13) |
| `thruline verify-continuity` | The legacy/local-only continuity comparison. |
| `thruline capsule` | Compile a Recovery Capsule — the smallest sufficient, receipt-carrying context for continuing this work in a fresh session. |
| `thruline export` | Derive a project and write the §9 JSON contract |
| `thruline projects` | List projects with their event counts |
| `thruline identity` | Repo identity (R04/A92). |
| `thruline identity backfill` | Derive repo identity for every project and record it. |
| `thruline identity groups` | Show one repo, many working roots. |
| `thruline capture` | M5 local Assured Capture operations. |
| `thruline capture install` | Show an exact read-only installation plan, or apply one by id |
| `thruline capture status` | Read-only operational status; never repairs or creates a key |
| `thruline capture self-test` | Execute the isolated, source-native compatibility self-test |
| `thruline capture authorize` | Append an explicit project authorization or revocation |
| `thruline capture disable` | Disable new capture without deleting configuration or history |
| `thruline capture uninstall` | Remove only exact ThruLine-owned hooks; historical evidence remains |
| `thruline capture hook` | Source-native hook entrypoint. |
| `thruline capture exec` | Opaque invocation wrapper used only by the rewritten Bash input |
| `thruline capture recover` | Close unfinished durable intents with zero authority |
| `thruline capture observe` | Seal and accept a later explicit state observation |
| `thruline capture doctor` | Read-only diagnosis and exact remediation guidance |
| `thruline capture pair` | Pair a completed native callback/wrapper receipt to an accepted event |
| `thruline sensor` | Lifecycle sensor (C1.2): plan/install/uninstall the Claude Code hook set; `append` is the entrypoint the vendor's own process invokes |
| `thruline sensor plan` | Print exactly what install would write, and where. |
| `thruline sensor install` | Merge the sensor hook set into Claude Code settings (`plan` first) |
| `thruline sensor uninstall` | Remove exactly the ThruLine-owned sensor entries. |
| `thruline sensor append` | Hook entrypoint: read one payload from stdin, append one row, exit 0. |
| `thruline sensor doctor` | Read-only health check: registration, rows, chain, liveness, spills, and the speaker half (hook registration, digest cache, findings) — which is fail-open-silent, so only this check can say it is dead |
| `thruline verify` | Re-read raw files, recompute every hash and the full chain |
| `thruline rebuild` | Drop and re-derive nodes/edges for a project |
| `thruline annotate` | Score claims against evidence and persist the annotation layer |
| `thruline model` | Verify or import the operator-supplied local ONNX annotation model |
| `thruline model status` | Print the machine-readable closed-manifest status of a model directory |
| `thruline model import` | Verify an operator-supplied directory and preview or apply a local copy |
| `thruline entity` | Propose and human-adjudicate receipt-backed navigation associations |
| `thruline entity propose` | Preview or append one content-addressed association proposal |
| `thruline entity adjudicate` | Record the one human decision allowed for a proposal |
| `thruline entity list` | List immutable proposals and their optional human decisions |
| `thruline eval` | (the binary prints no description) |
| `thruline shadow-sessions` | Run the synthetic eval corpus and gate on the result (M3.6). |
| `thruline upgrade` | Bring the ledger's schema up to date — the ONLY command that migrates |
| `thruline perf` | The recorded performance figures, each with its stamped basis, and whether that basis still holds HERE (P2b) |
| `thruline guardian` | SHADOW DIAGNOSTICS — Guardian phase 1 candidates, with the classes it cannot build and why. |
| `thruline digest` | Speaker (A200, SessionEnd hook): bind the ended session's final-turn completion claims to its own tool record and cache at most three receipt-carrying lines for the next session's brief. |
| `thruline digest precompute` | Compute the ended session's digest and write the per-project cache |
| `thruline brief` | Speaker (A200, SessionStart hook): inject this project's cached digest as additionalContext, at most three lines, every one carrying a receipt. |
| `thruline audit` | Speaker (A200, Stop hook): the stop-time claim audit, v0 announce-to-ledger. |
| `thruline adjudicate` | Record YOUR judgment on the shadow-class specimens awaiting a human. |
| `thruline hook-install` | Install the BUILD-SPEC §4.1 capture hook pack for a harness |
| `thruline doctor` | Is the install intact, and does the record verify? |
| `thruline hook` | Receive one harness hook delivery on stdin (BUILD-SPEC §4.1) |
| `thruline gate` | The §4.1 Stop gate: block a stop whose completion claim has no proof |
| `thruline claims` | What the agent claimed, and what the record proves (BUILD-SPEC §5, §6) |
| `thruline explain` | What happened, why it stopped, what it forgot, and what it proved (BUILD-SPEC §10) |
| `thruline receipt` | Build a signed receipt for a session (BUILD-SPEC §8) |
| `thruline verify-receipt` | Verify a receipt, offline (BUILD-SPEC §8.1, §14) |
| `thruline reexec` | Re-run the session's checks in a clean container at the final tree (BUILD-SPEC §7) |
| `thruline promises` | What the agent said it would do later, and whether anything did it (BUILD-SPEC §3.5, §11, §14) |
| `thruline shim` | The BUILD-SPEC §4.4 PATH shim: record what test and build commands actually ran |
| `thruline shim install` | Write the shim directory and print the PATH line to use it |
| `thruline shim uninstall` | Remove exactly the wrappers this build wrote |
| `thruline shim status` | Is the shim installed, unmodified, and actually first on PATH? |
| `thruline run` | Run an agent under a pseudoterminal, recording what it executes |
| `thruline tui` | The §10 flight recorder: timeline, detail, claims and promises, in a terminal |
| `thruline import-codex` | Import Codex rollout files into the §11 execution store (§4.2) |
| `thruline sync` | §14's sync verbs. |
| `thruline sessions` | §9's policy gate: `--init` writes the documented example, otherwise the last session is evaluated against `.thruline/policy.yml` |
| `thruline change` | Work you are being asked to accept, and the sessions linked to it |
| `thruline change new` | Open a Change: the piece of work you want to be able to hand over. |
| `thruline change prepare` | Write down what exactly will be examined for this Change: the project folder, the starting point, whether your unsaved changes are included, the tools that would run, and the running app it is about |
| `thruline change propose` | What this Change could be judged against, proposed from the request and the work already confirmed as part of it. |
| `thruline change accept` | Write down what counts as done. |
| `thruline change suggest` | Propose sessions that might belong to a Change, from timing and from the files they changed. |
| `thruline change associate` | Say whether a session belongs to a Change. |
| `thruline change results` | What the linked work shows for a Change — or why it is not counted |
| `thruline change links` | Which Changes one session is linked to, or none |
| `thruline change cancel-run` | T-3-4 (`B7.1`, `B12.6`) — stop a check that is still running, and confirm nothing of it was left behind |
| `thruline change waive` | T-5-3 (`B7.18`-`B7.22`) — set one thing that counts as done aside, without evidence for it, under the authority the accepted profile records |
| `thruline change finding` | T-4-2 (`B9.10`–`B9.14`, `Q9`, `Q10`) — record where one thing a review found stands. |
| `thruline change attempts` | T-4-4-ATTEMPT-BUDGET — how many repair attempts a person allows a Change, counted from the checks run on it; once they are used, a check on a new version is not started. A change made only to ThruLine's own files in the project (under .thruline/) currently counts as a new attempt (P-2026-09-29-3; fix carded as EXAMINED-INPUTS-OWN-FILES) |
| `thruline change note` | C30 R-LEDGER — keep a note on a Change, in your own words, with your name and the time. A note is words: it changes no result, no count and no approval, and it is free |
| `thruline change second` | C30 R-LEDGER — put your name beside the approval that stands, as a second person (`--withdraw` takes your own back). It never makes an approval stand and changes no result, and it is free |
| `thruline policy` | `thruline policy`'s arguments, flattened into their own type |
| `thruline bench` | §12's benchmark harness and the Overclaim Index |
| `thruline pilot-budget` | The pilot's money ceiling — the only place the per-run spending cap is enforced |
| `thruline pilot-budget check` | Refuse before the model is called if a worst-case task would not fit |
| `thruline pilot-budget charge` | Record one task's measured cost. |
| `thruline pilot-budget show` | Print the running total and what remains |
| `thruline fold` | Fold the hook and exec spool into the §11 execution store |
| `thruline import-legacy` | Import the pre-spec transcript ledger into the BUILD-SPEC §11 execution store at `~/.thruline/exec.db` |
| `thruline integration` | Plan, capture and inspect evidence from the integrated verification tools |
| `thruline integration native-repair` | Inspect, verify or apply a bounded native deterministic remedy |
| `thruline integration agent-repair` | Resume native agent work or independently verify a captured proposal |
| `thruline integration review` | Discover, accept, run, resume and export a bounded repository review |
| `thruline integration repair` | Preview, independently verify, apply or inspect a supported isolated Ruff repair |
| `thruline integration catalog` | List pinned sources and native report routes. |
| `thruline integration evaluate` | Measure supplied independent case labels against recorded outcomes; never graduates a tool |
| `thruline integration workflow` | Guided, requirement-linked checks; no shell command construction |
| `thruline integration workspace` | Read the tool workspace for a recorded session; never starts a check |
| `thruline integration project` | Read tool setup and receipts for an explicitly selected local project |
| `thruline integration plan` | Produce a reviewable plan for one tool. |
| `thruline integration run` | Execute the exact plan, accepting its explicit criterion for this run |
| `thruline integration decode` | Decode an existing report. |
| `thruline integration results` | Re-evaluate stored runs against current inputs. |
| `thruline json-shapes` | The `--json` contract: every JSON shape this binary emits, with its stable id and its `schema_version`. |

### Local server routes (technical)

Every route `thruline ui` answers, derived from `fn handle` by `deriveRoutes` (`code: crates/cli/src/uiserve.rs handle`, `code: scripts/qualification/docs-trace.mjs deriveRoutes`). Every request must come from the loopback and carry a matching Host (and Origin, when present), or it is answered 403 (`code: crates/cli/src/uiserve.rs peer_is_allowed`, `test: crates/cli/src/uiserve.rs::mind_status_is_guarded_bounded_and_never_analyzes`). A request whose Origin is not the server's own is refused on every route. The write routes under `/api/changes/` and `/api/first-use/`, and `POST /api/verification`, `POST /api/observation` and `POST /api/integrations/action`, also refuse a request that carries no Origin at all; `POST /api/refresh` and `POST /api/mind/interpret` do not, and rely on the Host and loopback checks (`code: crates/cli/src/uiserve.rs same_origin_required`, `test: crates/cli/tests/first_use_cli.rs::a_write_without_the_same_origin_header_is_refused`).

| Route | Kind | What it answers | Handler |
|---|---|---|---|
| `GET /api/observation` | read | automatic Codex observation state | `code: crates/cli/src/uiserve.rs observer` |
| `POST /api/observation` | write | pause or resume observation | `code: crates/cli/src/uiserve.rs observer` |
| `GET /api/verification` | read | the local verification projection | `code: crates/cli/src/uiserve.rs verification_read` |
| `POST /api/verification` | write | one requested local verification action | `code: crates/cli/src/uiserve.rs verification_action` |
| `POST /api/integrations/action` | write | one explicit integration check action | `code: crates/cli/src/uiserve/integration_actions.rs handle` |
| `GET /api/acceptance` | read | the acceptance page's projection | `code: crates/cli/src/uiserve.rs acceptance_projection` |
| `GET /api/integrations` | read | the integration projection | `code: crates/cli/src/uiserve.rs integration_projection` |
| `GET /api/review-projects` | read | review projects | `code: crates/cli/src/uiserve/review_projects.rs list` |
| `GET /api/reviews/export` | read | a review download | `code: crates/cli/src/uiserve/review_projects.rs download` |
| `GET /api/mind/status` | read | whether the local model is available | `code: crates/cli/src/uiserve.rs mind_status` |
| `POST /api/mind/interpret` | write | one grounded question to the local model | `code: crates/cli/src/uiserve.rs mind_interpret` |
| `GET /api/lanes` | read | live agent-lane presence from recording mtimes | `code: crates/cli/src/uiserve.rs lanes_json` |
| `GET /api/storage` | read | the store's size, folder and parts | `code: crates/cli/src/uiserve.rs storage` |
| `GET /api/licence` | read | whether this machine has a licence that works; the document `thruline licence status --json` prints | `code: crates/cli/src/licence_cmd.rs status_body` |
| `GET /api/first-use` | read | what this machine already has | `code: crates/cli/src/uiserve/first_use.rs overview` |
| `GET /api/first-use/proposal` | read | the Change that work would make | `code: crates/cli/src/uiserve/first_use.rs proposal` |
| `POST /api/first-use/register` | write | register the picked project | `code: crates/cli/src/uiserve/first_use.rs register` |
| `POST /api/first-use/change` | write | open the proposed Change | `code: crates/cli/src/uiserve/first_use.rs accept` |
| `GET /api/state` | read | server state | `code: crates/cli/src/uiserve.rs state_json` |
| `POST /api/refresh` | write | re-read the projects | `code: crates/cli/src/uiserve.rs refresh` |
| `GET /api/explain` | read | one session's explanation | `code: crates/cli/src/uiserve.rs projection` |
| `GET /api/claims` | read | one session's claims | `code: crates/cli/src/uiserve.rs projection` |
| `GET /api/phases` | read | phases of a session | `code: crates/cli/src/uiserve.rs phases` |
| `GET /api/read` | read | the steps of reading one session's record, each with its real count and measured time, or why it has none | `code: crates/cli/src/uiserve/read_route.rs read` |
| `GET /api/sessions` | read | the session list, per project | `code: crates/cli/src/uiserve.rs spawn_projection` |
| `POST /api/changes/<id>/associations` | write | one person's decision about one session link | `code: crates/cli/src/uiserve.rs change_association` |
| `POST /api/changes/<id>/decisions/withdraw` | write | CLAUDE.md write table row 2 | `code: crates/cli/src/uiserve.rs change_decide` |
| `POST /api/changes/<id>/decisions` | write | CLAUDE.md write table row 1 | `code: crates/cli/src/uiserve.rs change_decide` |
| `POST /api/changes/<id>/obligations/<obligation>/not-applicable` | write | CLAUDE.md write table row 6 | `code: crates/cli/src/uiserve.rs change_waive` |
| `POST /api/changes/<id>/obligations/<obligation>/waive` | write | CLAUDE.md write table row 5 | `code: crates/cli/src/uiserve.rs change_waive` |
| `POST /api/changes/<id>/scope/accept` | write | one scope version and one acceptance record | `code: crates/cli/src/uiserve.rs change_scope_accept` |
| `POST /api/changes/<id>/runs/<run>/cancel` | write | CLAUDE.md write table row 3 | `code: crates/cli/src/uiserve.rs change_cancel_run` |
| `GET /api/run/token` | read | this launch's run token, to this page's own origin only | `code: crates/cli/src/uiserve/run_route.rs token` |
| `GET /api/changes/<id>/run` | read | the run sheet: what will run, where, what it can and cannot establish, permission, the run going now, the last outcome | `code: crates/cli/src/uiserve/run_route.rs view` |
| `POST /api/changes/<id>/run/permission` | write | CLAUDE.md write table row 7 | `code: crates/cli/src/uiserve/run_route.rs grant` |
| `POST /api/changes/<id>/run/permission/revoke` | write | CLAUDE.md write table row 8 | `code: crates/cli/src/uiserve/run_route.rs revoke` |
| `POST /api/changes/<id>/run` | write | CLAUDE.md write table row 9 | `code: crates/cli/src/uiserve/run_route.rs start` |
| `POST /api/changes/<id>/links` | write | CLAUDE.md write table row 4 | `code: crates/cli/src/uiserve.rs change_links` |
| `POST /api/changes/<id>/findings/<finding>/standing` | write | CLAUDE.md write table row 10 | `code: crates/cli/src/uiserve.rs change_finding_standing` |
| `GET /api/changes/<id>/findings` | read | every finding of this Change's reviews and where each one stands | `code: crates/cli/src/uiserve.rs change_findings` |
| `GET /api/changes/<id>/reviews/<review>/raw` | read | one review's raw report, as the masked inspection copy | `code: crates/cli/src/uiserve.rs change_review_artifact` |
| `GET /api/changes/<id>/reviews/<review>/parsed` | read | one review's parsed record, as the masked inspection copy | `code: crates/cli/src/uiserve.rs change_review_artifact` |
| `GET /api/changes/<id>/readiness` | read | readiness and what it rests on | `code: crates/cli/src/uiserve.rs change_readiness_route` |
| `GET /api/changes/<id>/coverage` | read | evidence per accepted criterion | `code: crates/cli/src/uiserve.rs change_coverage` |
| `GET /api/changes/<id>/applicability` | read | applicability records | `code: crates/cli/src/uiserve.rs applicability_of_change` |
| `GET /api/changes/<id>/inputs` | read | examined-inputs records | `code: crates/cli/src/uiserve.rs change_inputs` |
| `GET /api/changes/<id>/proposals` | read | proposed criteria | `code: crates/cli/src/uiserve.rs change_proposals` |
| `GET /api/changes/<id>/export` | read | whether each record the file rests on can be read now | `code: crates/cli/src/uiserve.rs change_export_records` |
| `GET /api/changes` | read | the Change index | `code: crates/cli/src/uiserve.rs changes_index` |
| `GET /api/changes/<id>` | read | one Change | `code: crates/cli/src/uiserve.rs changes_one` |
| `GET /api/sessions/<id>/changes` | read | the Changes one session is linked to | `code: crates/cli/src/uiserve.rs session_changes` |
| `GET /fixtures/<file>` | read | the exported reading | `code: crates/cli/src/uiserve.rs serve_fixture` |
| `GET /<asset>` | read | the embedded app | `code: crates/cli/src/uiserve.rs serve_asset` |

### Write addresses (technical)

The write addresses the Change screen reaches, and what each may append, are given by CLAUDE.md's write table (section "R-2026-09-23-9") and are not restated here; the route table above refers to each by its row (`code: crates/cli/src/uiserve.rs change_decide`). The checker compares that table with every non-GET route the router answers and reports each difference; it is stated here and not smoothed over. The table's heading scopes it to the Change screen, so most of these are routes outside that screen; `…/associations` and `…/scope/accept` are under `/api/changes/` and are the two a reader of the table would expect to find in it — `/scope/accept` is reached by the first-use screen, and no screen reaches `/associations`; the same act has a terminal door, `thruline change associate` (`code: ui/src/workspace/first-use.jsx SCOPE_ACCEPT`, `code: crates/cli/src/change_cmd.rs associate`).

- Disagreement — not in the table: `POST /api/observation` (`code: crates/cli/src/uiserve.rs observer`)
- Disagreement — not in the table: `POST /api/verification` (`code: crates/cli/src/uiserve.rs verification_action`)
- Disagreement — not in the table: `POST /api/integrations/action` (`code: crates/cli/src/uiserve/integration_actions.rs handle`)
- Disagreement — not in the table: `POST /api/mind/interpret` (`code: crates/cli/src/uiserve.rs mind_interpret`)
- Disagreement — not in the table: `POST /api/first-use/register` (`code: crates/cli/src/uiserve/first_use.rs register`)
- Disagreement — not in the table: `POST /api/first-use/change` (`code: crates/cli/src/uiserve/first_use.rs accept`)
- Disagreement — not in the table: `POST /api/refresh` (`code: crates/cli/src/uiserve.rs refresh`)
- Disagreement — not in the table: `POST /api/changes/<id>/associations` (`code: crates/cli/src/uiserve.rs change_association`)
- Disagreement — not in the table: `POST /api/changes/<id>/scope/accept` (`code: crates/cli/src/uiserve.rs change_scope_accept`)

The same section of CLAUDE.md says `change-workspace.test.js` "still asserts the screen reaches exactly rows 1 and 2"; the test asserts that `change-workspace.jsx` reaches rows 1, 2 and 3, the run sheet it mounts (`run-sheet.jsx`) rows 7, 8 and 9, and its review section (`change-review.jsx`) row 10, and CLAUDE.md's own paragraphs for rows 7–9 and row 10 record that census and that the older sentence was one short (`test: ui/src/workspace/change-workspace.test.js::navigating writes nothing: the screen reaches exactly three write addresses of its own, the run sheet three and the review section one, and none is reachable by navigating`).

### Capability matrix (technical)

One mark per row. **Built** means the code is on this branch and a named test holds it. Code without a test is **In progress**, "no test yet". **In progress** also covers work on another lane's branch, cited as `clone:path@commit`. **Planned** is an open card or interface request. **Not planned** is a ruling or card that defers or declines it for this release. dist's README capability table (T-DIST-3) is not merged; when it lands, it supersedes this matrix (`lane: thruline-dist:docs/loop/dist/tasks/T-DIST-3.md@74399abb`).

| Capability | Built | In progress | Planned | Not planned | Source |
|---|---|---|---|---|---|
| Reading Claude Code sessions into a hash-chained local record | ● | | | | `code: crates/core/src/ingest.rs default_db_path` `test: crates/core/tests/ingest_idempotency.rs::first_ingest_stores_every_event_and_verify_passes` |
| Importing Codex sessions | ● | | | | `code: crates/core/src/codex_ingest.rs CodexAdapter` `test: crates/core/tests/codex_atomic_ingest.rs::first_middle_and_final_envelope_failures_never_publish_partial_authority` |
| Re-reading raw files and the whole chain (`thruline verify`) | ● | | | | `code: crates/core/src/ingest.rs verify` `test: crates/core/tests/ingest_idempotency.rs::rewriting_an_ingested_line_is_reported_and_never_absorbed` |
| Opening a Change | ● | | | | `code: crates/core/src/change.rs insert_change` `test: crates/core/tests/change_records.rs::a_change_is_stored_and_read_back_with_every_component` |
| Suggesting what counts as done | ● | | | | `code: crates/core/src/change_propose.rs PROPOSAL_VIEW_SCHEMA_VERSION` `test: crates/core/tests/change_propose.rs::every_proposed_criterion_has_all_three_parts` |
| Accepting what counts as done, in versions | ● | | | | `code: crates/core/src/change.rs accept_scope` `test: crates/core/tests/change_records.rs::a_new_scope_version_supersedes_and_the_old_one_stays_readable` |
| Linking work to a Change, suggested then confirmed by a person | ● | | | | `code: crates/core/src/change.rs append_association` `test: crates/core/tests/change_records.rs::a_suggestion_becomes_a_confirmation_only_by_appending_beside_it` |
| Counting only confirmed work inside the covered folders | ● | | | | `code: crates/core/src/change_evidence.rs COVERAGE_VIEW_SCHEMA_VERSION` `test: crates/core/tests/change_scope_gate.rs::a_confirmed_link_whose_work_is_out_of_scope_is_not_counted` |
| Writing down what will be examined | ● | | | | `code: crates/core/src/examined_inputs.rs INPUTS_VIEW_SCHEMA_VERSION` `test: crates/core/tests/examined_inputs_record.rs::a_record_is_written_for_an_examination_and_read_back_whole` |
| Recording a check run against a Change (`thruline check --for-change`) | ● | | | | `code: crates/cli/src/check.rs for_change` `test: crates/cli/tests/check_for_change_cli.rs::the_ordinary_path_ends_with_a_run_attached_to_what_counts_as_done` |
| Evidence per thing that counts as done, with manual links | ● | | | | `code: crates/core/src/change_link.rs LINK_VIEW_SCHEMA_VERSION` `test: crates/cli/tests/change_link_cli.rs::confirming_without_a_name_is_refused_and_writes_nothing` |
| Whether evidence still applies after the code moved | ● | | | | `code: crates/core/src/evidence_applicability.rs invalidating_paths` `test: crates/core/tests/evidence_applicability_rule.rs::editing_a_declared_scope_path_invalidates_the_criterion` |
| Stopping a running check | ● | | | | `code: crates/core/src/run_execution.rs RUN_EXECUTION_SCHEMA_VERSION` `test: crates/cli/tests/change_cancel_run_cli.rs::a_person_stops_a_running_check_and_the_whole_tree_is_verified_gone` |
| Ready for approval, with what it rests on | ● | | | | `code: crates/core/src/change_readiness.rs compute` `test: crates/core/tests/change_readiness.rs::the_fully_satisfied_case_is_ready_and_says_what_it_rests_on` |
| Setting one thing aside, by a named person with authority | ● | | | | `code: crates/core/src/change_readiness.rs waive` `test: crates/cli/tests/change_exceptions_cli.rs::a_waiver_clears_the_line_without_becoming_evidence_and_the_approval_says_so_for_ever` |
| Approving one version, and withdrawing it | ● | | | | `code: crates/core/src/change_decision.rs approve` `test: crates/core/tests/change_decision.rs::an_approval_given_for_one_version_does_not_move_to_the_next_one` |
| The whole story on one page | ● | | | | `code: ui/src/workspace/change-model.js packetOf` `test: ui/src/workspace/packet.test.js::the twelve sections render in that order on an empty Change too: an order is not a property of a full record` |
| Saving the whole story as a file, with a separate redacted copy | ● | | | | `code: ui/src/workspace/packet-export.js EXPORT_SCHEMA_VERSION` `test: ui/src/workspace/packet-export.test.js::three formats, each with its own name, type and text; the same inputs give the same bytes` |
| Saying whether each record the file rests on can be read now | ● | | | | `code: crates/core/src/change_export.rs records` `test: crates/core/tests/change_export.rs::every_record_readable_says_so_and_counts_itself` |
| The Inbox, one reason and one action per Change | ● | | | | `code: ui/src/workspace/inbox-view.js REASONS` `test: ui/src/workspace/inbox.test.js::every item carries exactly one primary action, and it is a link to the stage that settles its reason` |
| First use: propose a Change from what is already on the machine | ● | | | | `code: crates/cli/src/uiserve/first_use.rs proposal` `test: crates/cli/tests/first_use_cli.rs::picking_a_project_writes_the_project_and_not_a_change` |
| Brief, Explore and Workbench; Plain and Technical wording | ● | | | | `code: ui/src/workspace/hq-copy.js altitudes` `test: ui/src/workspace/altitude.test.js::three altitudes, Brief first and by default` |
| The Trace of one agent session | ● | | | | `code: ui/src/workspace/hq-copy.js trace` `test: ui/src/workspace/trace.test.js::Technical shows what Plain hides: seq, ids and the full version hash come back` |
| Ask: a question about one Change, answered with its evidence | ● | | | | `code: ui/src/workspace/hq-copy.js ask` `test: ui/src/workspace/ask-hq.test.js::there are exactly five questions, they are the five the design record names, and each is offered by its own copy string` |
| A message for the agent, ready to copy, on each finding | ● | | | | `code: ui/src/workspace/verdict-panel.jsx copy_prompt` `test: ui/src/workspace/verdict-panel.test.js::the first card carries the primary Copy prompt button and later cards do not` |
| Session start, stop and end hooks that speak only with receipts | ● | | | | `code: crates/core/src/speaker.rs digest_dir` `test: crates/cli/tests/a200_speaker_cli.rs::speakers_fail_open_on_every_defect` |
| Signed session receipts, verified offline | ● | | | | `code: crates/attest/src/statement.rs SCHEMA_ID` `test: crates/attest/tests/spec_conformance.rs::the_maximal_receipt_validates` |
| Saying how much is stored on this machine | ● | | | | `code: crates/cli/src/uiserve.rs storage` `test: crates/cli/src/uiserve.rs::the_storage_route_counts_every_file_in_the_folder_it_names` |
| Screen served to this machine only | ● | | | | `code: crates/cli/src/uiserve.rs resolve_bind_address` `test: crates/cli/tests/ui_server_contract.rs::the_listener_is_on_the_loopback_interface_only` |
| Masking private text before it is served | ● | | | | `code: crates/core/src/redact.rs mask` `test: crates/cli/src/uiserve.rs::served_change_text_is_masked_before_it_reaches_the_socket` |
| Only `thruline upgrade` changes the store's schema | ● | | | | `code: crates/cli/src/main.rs find_upgrade_pending` `test: crates/cli/tests/a193_upgrade_cli.rs::an_ordinary_command_refuses_in_one_line_and_pays_nothing` |
| Running a check from the Change screen | ● | | | | `code: crates/cli/src/uiserve/run_route.rs start` `test: ui/src/workspace/run-route.test.js::the run sheet writes to exactly its three declared addresses, all with the token` |
| Recorded permission to run checks for a project | ● | | | | `code: crates/core/src/run_permission.rs grant` `test: crates/core/tests/run_permission.rs::a_grant_and_its_revocation_are_two_rows_and_the_revocation_names_the_grant` |
| The read rail on the Trace | ● | | | | `code: ui/src/workspace/read-rail.jsx ReadRail` `test: ui/src/workspace/read-rail.test.js::(p62) the rendered steps are EXACTLY the served steps, in order — nothing added, nothing dropped` |
| Reading a recorded local review of one version | ● | | | | `code: ui/src/workspace/change-review.jsx ReviewSection` `test: ui/src/workspace/review-model.test.js::T-4-1 (rendered): the Review stage keeps one primary, off, with its reason and what would turn it on` |
| Showing recorded review findings and where they stand | ● | | | | `code: ui/src/workspace/change-workspace.jsx FindingStandings` `test: ui/src/workspace/review-model.test.js::T-4-2 (rendered, Review stage): the stage lists where each stands and keeps its one primary` |
| A prepared repair message for a whole Change | | ● | | | `lane: thruline-review:docs/loop/review/tasks/T-4-3.md@e3b78f5e` |
| Licence and Pro approval | ● | | | | `code: crates/licence/src/lib.rs PAID_GATE` `code: crates/licence/src/lib.rs PAID` `code: crates/cli/src/licence_cmd.rs approve_version` `test: crates/licence/tests/production_key.rs::the_paid_gate_ships_on` `test: crates/cli/tests/licence_as_shipped_cli.rs::as_shipped_the_paid_gate_is_on_and_exactly_one_key_is_built_in` `test: crates/cli/tests/licence_flow_cli.rs::a_fresh_home_is_free_and_approving_is_off_without_pro` `test: crates/cli/tests/licence_flow_cli.rs::a_pro_licence_opens_approving_and_removing_it_shuts_it_again` `test: crates/cli/tests/licence_flow_cli.rs::an_installed_expired_licence_is_refused_and_says_expired` `test: ui/src/workspace/licence.test.js::A: absent — Approve is off with both halves of why and the door to the licence page; everything else is still on` |
| Signed macOS installer and Homebrew install | | ● | | | `lane: thruline-dist:release/macos/release.sh@74399abb` `lane: thruline-dist:docs/loop/dist/HANDOFF.md@74399abb` |
| Qualification case runner and harness self-tests | ● | | | | `code: scripts/qualification/lib/main.mjs run-case` `test: scripts/qualification/test/mechanized.test.mjs::every committed case step is generic, handled, or on the pinned not-mechanized list` |
| A picture of a Change | | | ● | | `cite: docs/loop/tasks/P6-IR-4.md` `cite: docs/loop/INTERFACE-REQUESTS.md` |
| Offering to remove stored records | | | ● | | `cite: docs/INTERFACE-REQUESTS.md` `cite: docs/loop/03-USER-LENS.md` |
| The re-run cycle after the agent worked | | | ● | | `lane: thruline-review:docs/loop/review/tasks/T-4-4.md@e3b78f5e` |
| The Inbox saying "needs a re-check" for stale evidence | | | ● | | `cite: docs/loop/tasks/INBOX-FRESHNESS-LINE.md` |
| A written specification of the record format | | | ● | | `cite: docs/loop/tasks/LEDGER-FORMAT-SPEC.md` `ruling: R-2026-09-25-1` |
| A standalone verifier for the record's chain | | | ● | | `cite: docs/loop/tasks/STANDALONE-VERIFIER.md` `ruling: R-2026-09-25-1` |
| README capability table and LICENSE | | | ● | | `lane: thruline-dist:docs/loop/dist/tasks/T-DIST-3.md@74399abb` `ruling: R-2026-09-25-2` |
| Automatic repair | | | | ● | `ruling: R-2026-09-25-3` `cite: docs/loop/tasks/T-4-5-DEFERRED.md` |
| Checking a claim against the code as it was when the claim was made | | | | ● | `ruling: R-2026-09-24-33` `ruling: R-2026-09-24-36` |

### What ThruLine stores, and where (technical)

- The transcript ledger: `~/.thruline/ledger.db`, from `$HOME` (`code: crates/core/src/ingest.rs default_db_path`).
- The execution store: `$THRULINE_HOME/exec.db`, else `~/.thruline/exec.db` (`code: crates/ledger/src/schema.rs default_path`). The two use different variables: `THRULINE_HOME` moves the execution store and not the transcript ledger (`code: crates/core/src/private_store.rs thruline_home`).
- The digest cache beside the ledger (`code: crates/core/src/speaker.rs digest_dir`); sealed Codex mirrors under `~/.thruline/mirrors` (`code: crates/core/src/ingest.rs default_mirror_root`); the reading `thruline ui` serves under `export` beside the ledger, `~/.thruline/export` by default (`code: crates/cli/src/main.rs Ui`, `code: crates/cli/src/uiserve.rs serve_fixture`).
- The only file written outside the store is a decision copy, `.thruline/decisions/<version>.json`, under the project's recorded worktree (`code: crates/core/src/change_decision.rs record_path_for`).
- `GET /api/storage` sums every file under the store folder without following links and serves no recorded text; its parts are records, command_log, preserved_transcripts, digest, export and other (`served: GET /api/storage parts`, `test: crates/cli/src/uiserve.rs::the_storage_route_serves_no_recorded_text`). When `THRULINE_HOME` keeps the execution store in a folder apart from the ledger's, that folder is added to the sum and named in `also` (`served: GET /api/storage also`, `test: crates/cli/src/uiserve.rs::the_storage_route_counts_the_execution_store_kept_in_a_folder_of_its_own`). When `THRULINE_HOME` is a folder that holds the ledger's folder (`HOME` itself, say), that folder is not walked: the execution store's own files in it, `exec.db` and its two SQLite sidecars, are added one by one and the store is named in `also_command_log` (`served: GET /api/storage also_command_log`, `test: crates/cli/src/uiserve.rs::the_storage_route_counts_the_command_log_kept_in_a_folder_that_holds_the_records_folder`, `test: crates/cli/tests/first_use_cli.rs::the_figure_counts_the_command_log_wherever_thruline_home_keeps_it`). The first screen's figure is the same number from the same function, measured before that request opens the ledger (`code: crates/cli/src/uiserve.rs kept`, `test: crates/cli/tests/first_use_cli.rs::the_first_screen_and_the_inbox_say_one_number_for_what_is_kept`).

The L1.1 measurement, run for this file on a fresh `HOME` made by `mktemp -d`: `thruline ingest --apply --source claude-code` of `crates/core/tests/fixtures/claude_code_sessions.jsonl` (23,593 bytes), then `thruline ui --no-open --port 0` and `GET /api/storage`, which answered `bytes` 1,261,720 with parts records 1,040,384, command_log 184,320, preserved_transcripts 32,305, export 4,711; `du -sk` of the folder afterwards gave 1240 KiB (`served: GET /api/storage bytes`). On the machine this file was written on, `du -sk ~/.thruline` gave 17,944,496 KiB (read-only; the store was not opened). No verb the binary's help prints removes records from the store (`code: crates/cli/src/main.rs Command`).

### What leaves this machine (technical)

The code opens network connections in these places, and no others; `ureq` is the only HTTP client in the workspace (`code: crates/core/src/language.rs ureq`, `code: crates/core/src/mind.rs ureq`, `code: crates/core/src/mind_grounded.rs ureq`, `code: crates/sigstore/src/net/mod.rs ureq`):

- The local server, bound to the IPv4 loopback only; anything else is refused before a socket exists (`code: crates/cli/src/uiserve.rs resolve_bind_address`, `test: crates/cli/src/uiserve.rs::only_loopback_addresses_can_be_bound`).
- The annotation language tier, whose base URL must be loopback; it is enabled by default and falls back to the deterministic extractor when nothing answers (`code: crates/core/src/config.rs LanguageConfig`, `code: crates/core/src/language.rs normalise_local_base`, `test: crates/core/src/language.rs::provider_accepts_only_local_openai_bases`, `test: crates/core/src/config.rs::language_tier_is_loopback_only_and_uses_hysteresis`).
- The local mind, loopback-only, and the UI's grounded client, fixed to `127.0.0.1:11434` and not following redirects (`code: crates/core/src/mind_grounded.rs OLLAMA_NATIVE`, `test: crates/core/src/mind_grounded.rs::fixed_client_does_not_follow_redirect_and_caps_response`, `test: crates/core/src/mind.rs::loopback_only`).
- `thruline reexec` asks Docker to pull a container image when it is absent; the run itself is `--network none` unless the repository asked otherwise (`code: crates/reexec/src/docker.rs ensure_image`, `test: crates/reexec/tests/git_verbs.rs::the_run_path_turns_the_network_off_unless_the_repository_asked`).
- Keyless signing reaches Sigstore only in a build with the `net` feature, which is off by default; `attest` has no network dependency (`code: crates/sigstore/src/net/mod.rs sign_keyless`, `test: crates/sigstore/tests/no_network.rs::the_default_feature_set_resolves_no_transport`, `test: crates/attest/tests/no_network.rs::the_attest_crate_has_no_network_dependency`).
- Commands you ask ThruLine to run (`thruline check -- <command>`, `thruline run`) make whatever connections they make themselves; ThruLine executes only what you typed (`code: crates/cli/src/check.rs for_change`).

### Supported environments and limitations (technical)

- Built from source with Rust 1.97 or later, edition 2021 (`code: Cargo.toml rust-version`). The app is embedded in the binary, so no Node or npm is needed to run it (`code: crates/cli/build.rs dist`).
- Unix (macOS and Linux) is the full build. On a non-Unix build there is no pseudoterminal and the recorded shape says so, and the acceptance projection refuses because it needs Unix process groups (`code: crates/capture/src/pty.rs fallback`, `code: crates/cli/src/uiserve.rs structured_projection`). The browser is opened with `open` on macOS and `xdg-open` on Linux (`code: crates/cli/src/uiserve.rs open_browser`).
- Session sources: Claude Code JSONL and Codex rollouts; an ambiguous path fails closed (`code: crates/cli/src/main.rs Ingest`, `test: crates/core/tests/parse_fixtures.rs::unknown_types_are_counted_and_become_meta`).
- Limitations on this branch, each shown on screen where it applies: starting a review from the Change screen is not built yet, “Starting a review from this screen is not built yet.” (`copy: change.blocked.review_from_terminal.reason`); setting something aside has no button, only `thruline change waive` (`code: crates/cli/src/change_cmd.rs waive`); a Change has no picture, “The outline lists the thread step by step, without the picture. This Change screen draws no picture of the record.” (`copy: shell.sidebar.outline_why.reason`); nothing removes stored records, “Nothing in this version removes any of it, and nothing on this screen deletes anything.” (`copy: change.inbox.storage.note`); `thruline sync` refuses because the hosted sync server does not ship in v1 (`code: crates/cli/src/sync_cmd.rs NOT_SHIPPED`).

### Troubleshooting (technical)

- `schema upgrade pending — run \`thruline upgrade\` (<estimate>)`, exit 3, nothing read or written. A new binary needs a schema change; run `thruline upgrade`, which is the only command that migrates (`code: crates/cli/src/main.rs find_upgrade_pending`, `code: crates/core/src/error.rs UpgradePending`, `test: crates/cli/tests/a193_upgrade_cli.rs::an_ordinary_command_refuses_in_one_line_and_pays_nothing`). The server answers the same condition with 503 and `upgrade_pending` (`code: crates/cli/src/uiserve.rs open_ledger_for_read`).
- `refusing to bind <address>: \`thruline ui\` serves your transcripts and answers only on the IPv4 loopback.`, exit 1. Use `127.0.0.1` or `localhost`, or leave `--address` off (`code: crates/cli/src/uiserve.rs resolve_bind_address`, `test: crates/cli/tests/ui_server_contract.rs::a_non_loopback_bind_address_is_refused_and_nothing_is_bound`).
- `port <n> is not available (…)`. Pick another `--port`, or leave it off and one is chosen; without `--port`, 7777 is tried first (`code: crates/cli/src/uiserve.rs PREFERRED_PORT`). No test holds this message.
- HTTP 403 `this address only answers to itself`: the request's Host or Origin is not the server's own. Open the address the command printed (`code: crates/cli/src/uiserve.rs handle`, `test: crates/cli/src/uiserve.rs::mind_status_is_guarded_bounded_and_never_analyzes`).
- `thruline verify` exits 1 and prints `CHAIN DAMAGED`, `CHAIN DOES NOT ADD UP`, or a missing source file: the record no longer matches what was ingested (`code: crates/cli/src/main.rs print_verify`, `code: crates/core/src/ingest.rs verify`, `test: crates/core/tests/ingest_idempotency.rs::verify_reports_a_missing_source_file`, `test: crates/cli/tests/a205_observation_cli.rs::verify_covers_the_observation_chain`).
- `malformed <n> line(s)` and `unknown <n> type(s) mapped to meta` after an ingest: counted and listed, never fatal (`code: crates/core/src/parse.rs ParseStats`, `test: crates/core/tests/parse_fixtures.rs::malformed_lines_are_counted_and_listed_not_fatal`).
- `thruline doctor` ends `Something above is not intact.`, exit 1; a `chain` line means the record cannot be trusted, a `quarantine` line means evidence is missing (`code: crates/cli/src/doctor.rs run`, `test: crates/cli/tests/m08_exec_shim.rs::doctor_reports_a_forged_wrapper_by_name`). Started outside a git repository, the working-tree check is printed as a `note` line instead and does not change the exit code (`code: crates/cli/src/doctor.rs inside_a_work_tree`, `test: crates/cli/tests/doctor_outside_a_repo_cli.rs::outside_a_repository_the_working_tree_check_is_a_note_and_doctor_exits_0`).
- A recurring `database is locked` from the sensor: a long ledger writer held the database while hooks fired; `thruline sensor doctor` counts spilled deliveries and says so (`code: crates/cli/src/main.rs SensorAction`, `code: crates/core/src/sensor.rs busy_timeout`, `test: crates/cli/tests/c12_sensor_cli.rs::doctor_names_the_absent_replay_path_for_spilled_deliveries`).
- The speaker hooks print nothing and exit 0 on any error, and stop themselves after 3.5 s (`code: crates/cli/src/main.rs speaker_watchdog`, `test: crates/cli/tests/a200_speaker_cli.rs::speakers_fail_open_on_every_defect`).

### Release and building from source (technical)

- Build: `cargo build -p thruline-cli`; the binary is `thruline` (`code: crates/cli/Cargo.toml thruline`).
- `ui/dist` is committed and embedded into the binary at build time, so a change to `ui/src` needs `cd ui && npm run build` and the rebuilt `ui/dist` committed with it (`code: crates/cli/build.rs dist`).
- After installing a new binary, run `thruline upgrade` once if a command says an upgrade is pending (`test: crates/cli/tests/a193_upgrade_cli.rs::the_upgrade_verb_pays_it_visibly_and_everything_works_after`).
- `RELEASING.md` on this branch is the prepared, unexecuted runbook for a first binary release; it says nothing in it has happened (`code: RELEASING.md PREPARED`). dist's release work — the signed macOS installer and the Homebrew path — arrives in a later version (`lane: thruline-dist:docs/loop/dist/HANDOFF.md@74399abb`). The Homebrew line takes this form: `brew install <tap-owner>/thruline/thruline`, where `<tap-owner>` is a placeholder and the exact line is published at launch (`test: crates/cli/tests/readme_install_cli.rs::the_homebrew_line_is_the_one_line_in_the_readme_and_the_guide_with_its_note`).

### How this file is checked (technical)

`node scripts/qualification/docs-trace.mjs --binary <thruline>` checks every trace in this file, the verb set against the binary's help, the routes against the router, the write table against the router, every schema file and version constant, every `--json` shape, the state words, the plain-section vocabulary, the capability matrix and every later-version label; `node --test scripts/qualification/docs-trace.test.mjs` breaks each check once and shows it fails (`code: scripts/qualification/docs-trace.mjs run`, `test: scripts/qualification/docs-trace.test.mjs::every check holds on the good fixture`).
