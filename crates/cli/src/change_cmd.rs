//! `thruline change` — proposing, deciding and reading session ↔ Change links,
//! and proposing and accepting what counts as done.
//!
//! Seven verbs, and the split between them is the point:
//!
//! * `new` opens one. It is the only verb that creates a Change, it requires a
//!   title and a name, and it writes exactly one row: no scope, no acceptance,
//!   no link. Until T-2-4's fix round there was no way at all for a person to
//!   open a Change — the screen that shows one could only be filled by a
//!   program calling the library — and a surface with no door in front of it is
//!   the "gives up" failure mode of `docs/loop/03-USER-LENS.md` §2.
//! * `suggest` reads and proposes. With `--record` it writes, and the only
//!   thing it can write is a `suggested` row.
//! * `associate` is the human act. It is the only verb here that can produce a
//!   `confirmed` or `rejected` row, it requires `--actor`, and it appends
//!   beside the suggestion it answers rather than editing it.
//! * `results` is the gated read. Every session linked to the Change is
//!   listed; the ones that do not pass the scope gate carry a sentence instead
//!   of a number.
//! * `links` is a session's side of the same fact, for a session that may
//!   belong to no Change at all.
//! * `propose` (T-2-3) reads the request and the confirmed work and proposes
//!   what counts as done. It writes nothing, ever.
//! * `accept` (T-2-3) is the human act for that half. It requires `--actor`, it
//!   appends one scope version and one acceptance record, and **it is not an
//!   approval** — it says what this work will be judged against, not that the
//!   work is good.
//!
//! **Every sentence printed here comes from `thruline_core::voice`.** Nothing
//! in this file writes prose for a person. A string literal in an output line
//! would fail `crates/core/tests/change_link_copy.rs`, which reads this file.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use thruline_core::change::AssociationProvenance;
use thruline_core::change_link;
use thruline_core::change_propose;
use thruline_core::ledger::Ledger;
use thruline_core::voice;

pub enum Verb {
    New {
        project: Option<String>,
        root: Option<PathBuf>,
        title: String,
        purpose: Option<String>,
        requester: Option<String>,
    },
    Propose {
        change: String,
    },
    Prepare {
        change: String,
        app_port: Option<u16>,
        config_files: Vec<String>,
        root: Option<PathBuf>,
    },
    Accept {
        change: String,
        all: bool,
        criteria: Vec<u32>,
        edits: Vec<String>,
        scope: Vec<String>,
        replacing: bool,
        actor: Option<String>,
        rationale: Option<String>,
    },
    Suggest {
        change: String,
        record: bool,
    },
    Associate {
        change: String,
        session: String,
        confirm: bool,
        reject: bool,
        actor: Option<String>,
        reason: Option<String>,
    },
    Results {
        change: String,
    },
    Links {
        session: String,
    },
    /// T-3-4 — stop a running check and verify nothing of it survived.
    CancelRun {
        run: String,
        actor: Option<String>,
    },
    /// T-5-3 — set one obligation aside, with a name, an authority and a
    /// reason. The terminal half of the two write routes.
    Waive {
        change: String,
        obligation: String,
        actor: Option<String>,
        rationale: String,
        not_applicable: bool,
    },
}

pub struct Args {
    pub db: PathBuf,
    pub verb: Verb,
    pub json: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let ledger =
        Ledger::open(&args.db).with_context(|| format!("opening {}", args.db.display()))?;
    match args.verb {
        Verb::New {
            project,
            root,
            title,
            purpose,
            requester,
        } => open(
            &ledger, project, root, &title, purpose, requester, args.json,
        ),
        Verb::Suggest { change, record } => suggest(&ledger, &change, record, args.json),
        Verb::Associate {
            change,
            session,
            confirm,
            reject,
            actor,
            reason,
        } => associate(
            &ledger, &change, &session, confirm, reject, actor, reason, args.json,
        ),
        Verb::Results { change } => results(&ledger, &change, args.json),
        Verb::Links { session } => links(&ledger, &session, args.json),
        Verb::CancelRun { run, actor } => cancel_run(&ledger, &run, actor, args.json),
        Verb::Waive {
            change,
            obligation,
            actor,
            rationale,
            not_applicable,
        } => waive(
            &ledger,
            &change,
            &obligation,
            actor,
            &rationale,
            not_applicable,
            args.json,
        ),
        Verb::Propose { change } => propose(&ledger, &change, args.json),
        Verb::Prepare {
            change,
            app_port,
            config_files,
            root,
        } => prepare(&ledger, &change, app_port, &config_files, root, args.json),
        Verb::Accept {
            change,
            all,
            criteria,
            edits,
            scope,
            replacing,
            actor,
            rationale,
        } => accept(
            &ledger, &change, all, &criteria, &edits, &scope, replacing, actor, rationale,
            args.json,
        ),
    }
}

/// The Plain sentence for a slug, with its counts filled in.
///
/// A slug that is not in the copy file is a programming error, and it is
/// refused loudly rather than printed as an empty line: a person reading a
/// blank where a reason belongs learns nothing and distrusts everything.
fn plain(slug: &str, files: u64, n: u64) -> Result<String> {
    match voice::association_sentence(slug, files, n) {
        Some((plain, _)) => Ok(plain),
        None => bail!("no copy-file line for {slug:?}"),
    }
}

fn technical(slug: &str, files: u64, n: u64) -> Result<String> {
    match voice::association_sentence(slug, files, n) {
        Some((_, technical)) => Ok(technical),
        None => bail!("no copy-file line for {slug:?}"),
    }
}

/// The Plain sentence for a proposal slug. Same rule as `plain`: a slug with no
/// line is a programming error and is refused loudly, never printed blank.
fn said(slug: &str, count: u64, version: u64, name: &str) -> Result<String> {
    match voice::proposal_sentence(slug, count, version, name) {
        Some((plain, _)) => Ok(plain),
        None => bail!("no copy-file line for {slug:?}"),
    }
}

fn said_technical(slug: &str, count: u64, version: u64, name: &str) -> Result<String> {
    match voice::proposal_sentence(slug, count, version, name) {
        Some((_, technical)) => Ok(technical),
        None => bail!("no copy-file line for {slug:?}"),
    }
}

/// Both registers for one slug, as the `--json` surfaces serve them.
fn both(slug: &str, count: u64, version: u64, name: &str) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "slug": slug,
        "plain": said(slug, count, version, name)?,
        "technical": said_technical(slug, count, version, name)?,
    }))
}

/// The Plain sentence for one of `new`'s slugs, refused loudly when the slug
/// has no line — same rule as `plain`.
fn opened(slug: &str) -> Result<&'static str> {
    match voice::change_open_sentence(slug) {
        Some((plain, _)) => Ok(plain),
        None => bail!("no copy-file line for {slug:?}"),
    }
}

/// Both registers for one of `new`'s slugs, as `--json` serves them.
fn opened_both(slug: &str) -> Result<serde_json::Value> {
    let Some((plain, technical)) = voice::change_open_sentence(slug) else {
        bail!("no copy-file line for {slug:?}");
    };
    Ok(serde_json::json!({
        "slug": slug,
        "plain": plain,
        "technical": technical,
    }))
}

/// `thruline change new` — open one Change.
///
/// It writes one row and stops. Everything a Change later carries — what counts
/// as done, which sessions belong to it, which snapshot it is bound to — is a
/// separate act by a separate verb, so that opening a Change can never be read
/// as having decided any of them. `B3.4` is the whole of what is written here:
/// the title, the purpose, who asked, and that a person asked rather than an
/// agent.
fn open(
    ledger: &Ledger,
    project: Option<String>,
    root: Option<PathBuf>,
    title: &str,
    purpose: Option<String>,
    requester: Option<String>,
    json: bool,
) -> Result<ExitCode> {
    if title.trim().is_empty() {
        bail!("{}", opened("change_open_needs_title")?);
    }
    let Some(requester) = requester.filter(|r| !r.trim().is_empty()) else {
        bail!("{}", opened("change_open_needs_requester")?);
    };
    let known = ledger.project_ids().context("reading the projects")?;
    let project = match (project, known.as_slice()) {
        (_, []) => bail!("{}", opened("change_open_no_projects")?),
        (Some(asked), _) if !known.contains(&asked) => {
            bail!(
                "{}\n{}",
                opened("change_open_unknown_project")?,
                known.join("\n")
            )
        }
        (Some(asked), _) => asked,
        (None, [only]) => only.clone(),
        (None, _) => bail!(
            "{}\n{}",
            opened("change_open_project_needed")?,
            known.join("\n")
        ),
    };
    let change = thruline_core::change::NewChange {
        id: format!("chg_{}", ulid::Ulid::generate()),
        project_id: project,
        opened_at: thruline_core::ingest::iso8601_utc_now(),
        request: thruline_core::change::ChangeRequest {
            title: title.trim().to_string(),
            purpose: purpose.unwrap_or_default(),
            prompt_event_id: None,
            requester: requester.trim().to_string(),
            // A person typed this command. `Agent` and `Imported` are not
            // reachable from here, and guessing between them from a flag would
            // be recording a provenance nobody asserted.
            source: thruline_core::change::RequestSource::Person,
        },
    };
    thruline_core::change::insert_change(ledger, &change)
        .map_err(|problem| anyhow::anyhow!("{problem}"))?;
    // T-3-2b — opening a Change binds it to the project folder as it stands
    // now, so that every later examination has something to be compared
    // against. It observes; it claims nothing about what was examined, and a
    // folder this machine cannot read is recorded as unknown rather than
    // guessed. A failure here is said and never silently swallowed (Rule 12),
    // and it does not unwrite the Change.
    // `--root` is an explicit person-provided selection. The process cwd is
    // deliberately not substituted here: an unrelated checkout is not made
    // part of this Change merely because the command happened to run there.
    let bound = match thruline_core::examined_inputs::bind_intended_at(
        ledger,
        &change.id,
        &change.opened_at,
        root.as_deref(),
    ) {
        Ok(outcome) => outcome.map(|o| o.id),
        Err(problem) => {
            eprintln!("what this Change is about was not written down: {problem}");
            None
        }
    };
    if json {
        return crate::json_out::print(
            &crate::json_out::CHANGE_NEW,
            serde_json::json!({
                "change": change.id,
                "project": change.project_id,
                "title": change.request.title,
                "opened_at": change.opened_at,
                "requester": change.request.requester,
                "source": change.request.source.as_str(),
                "bound_inputs": bound,
                "said": opened_both("change_opened")?,
            }),
        )
        .map(|_| ExitCode::SUCCESS);
    }
    println!("{}", opened("change_opened")?);
    println!("{}", change.id);
    Ok(ExitCode::SUCCESS)
}

fn suggest(ledger: &Ledger, change: &str, record: bool, json: bool) -> Result<ExitCode> {
    let report = change_link::suggest(ledger, change).context("proposing links")?;
    let written = if record {
        let now = thruline_core::ingest::iso8601_utc_now();
        Some(
            change_link::record_suggestions(ledger, &report, &now)
                .context("recording the proposals")?,
        )
    } else {
        None
    };

    if json {
        let mut lines = Vec::new();
        for suggestion in &report.suggestions {
            let files = suggestion.intersecting_paths.len() as u64;
            lines.push(serde_json::json!({
                "session": suggestion.session_id,
                "basis": suggestion.basis.as_str(),
                "plain": plain(suggestion.sentence_slug, files, 0)?,
                "technical": technical(suggestion.sentence_slug, files, 0)?,
            }));
        }
        return crate::json_out::print(
            &crate::json_out::CHANGE_SUGGEST,
            serde_json::json!({
                "report": report,
                "sentences": lines,
                "recorded": written,
            }),
        )
        .map(|_| ExitCode::SUCCESS);
    }

    // What was found, and then — always — what was not read.
    //
    // This used to return here, and `T-2-8-F3` is what that cost: on a store
    // where every recording carried a timestamp this matcher could not read,
    // the person was told "nothing in the record looks like it belongs" while
    // the same run's report named both recordings as unplaceable. An empty
    // result and an unread one are different answers, and the surface was
    // giving the reassuring one. Nothing is skipped on the way out now.
    if let Some(slug) = report.empty_slug {
        println!("{}", plain(slug, 0, 0)?);
    }
    for suggestion in &report.suggestions {
        let files = suggestion.intersecting_paths.len() as u64;
        println!("  {}", suggestion.session_id);
        println!("    {}", plain(suggestion.sentence_slug, files, 0)?);
    }
    // The slug for each of these comes from the report, not from here: the
    // engine names the sentence, the surface only prints it.
    for (items, slug) in [
        (&report.ties, report.ties_slug),
        (&report.undated_sessions, report.undated_slug),
        (
            &report.unreadable_scope_patterns,
            report.unreadable_scope_slug,
        ),
    ] {
        let Some(slug) = slug else { continue };
        for item in items {
            println!("  {item}");
            println!("    {}", plain(slug, 0, 0)?);
        }
    }
    // And the one sentence that says what the answer above leaves out, with the
    // count read off the typed list it is about. It is last because it is the
    // thing a person should still be holding when they stop reading.
    for (items, slug) in [
        (report.undated_sessions.len(), report.undated_summary_slug),
        (
            report.unreadable_scope_patterns.len(),
            report.unreadable_scope_summary_slug,
        ),
    ] {
        let Some(slug) = slug else { continue };
        println!("{}", plain(slug, items as u64, 0)?);
    }
    if let Some(written) = written {
        println!();
        println!(
            "{}",
            voice::count_phrase(
                written.written.len() as u64,
                "link proposed",
                "links proposed"
            )
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
fn associate(
    ledger: &Ledger,
    change: &str,
    session: &str,
    confirm: bool,
    reject: bool,
    actor: Option<String>,
    reason: Option<String>,
    json: bool,
) -> Result<ExitCode> {
    let provenance = match (confirm, reject) {
        (true, false) => AssociationProvenance::Confirmed,
        (false, true) => AssociationProvenance::Rejected,
        _ => bail!("say which: --confirm or --reject, exactly one of them"),
    };
    // Refused here, before the store is touched, with the copy file's words —
    // and the table's CHECK still refuses it independently.
    let Some(actor) = actor.filter(|a| !a.trim().is_empty()) else {
        bail!("{}", plain("confirm_needs_actor", 0, 0)?);
    };
    let reason = reason.unwrap_or_default();
    let at = thruline_core::ingest::iso8601_utc_now();
    let id = change_link::decide(ledger, change, session, provenance, &actor, &reason, &at)
        .context("recording the decision")?;

    let label = voice::change_label(provenance.label_slug())
        .map(|t| t.word)
        .unwrap_or(provenance.as_str());
    if json {
        return crate::json_out::print(
            &crate::json_out::CHANGE_ASSOCIATE,
            serde_json::json!({
                "association": id,
                "change": change,
                "session": session,
                "provenance": provenance.as_str(),
                "actor": actor,
                "asserted_at": at,
                "label": label,
            }),
        )
        .map(|_| ExitCode::SUCCESS);
    }
    println!("{label}");
    println!(
        "  {}",
        voice::change_label(provenance.label_slug())
            .map(|t| t.sentence)
            .unwrap_or_default()
    );
    Ok(ExitCode::SUCCESS)
}

fn results(ledger: &Ledger, change: &str, json: bool) -> Result<ExitCode> {
    let view = change_link::change_results(ledger, change).context("reading linked work")?;
    if json {
        let mut sentences = Vec::new();
        for session in &view.sessions {
            let (files, version) = match &session.applicability {
                change_link::Applicability::Applies {
                    scope_version,
                    in_scope_paths,
                } => (in_scope_paths.len() as u64, *scope_version as u64),
                // One captured execution: no changed file put it here, so the
                // count that would be printed is not a fact and is not printed.
                change_link::Applicability::AppliesAsExamination { scope_version, .. } => {
                    (0, *scope_version as u64)
                }
                change_link::Applicability::NotApplicable { .. } => (0, 0),
            };
            sentences.push(serde_json::json!({
                "session": session.session_id,
                "plain": plain(session.sentence_slug, files, version)?,
                "technical": technical(session.sentence_slug, files, version)?,
            }));
        }
        return crate::json_out::print(
            &crate::json_out::CHANGE_RESULTS,
            serde_json::json!({ "results": view, "sentences": sentences }),
        )
        .map(|_| ExitCode::SUCCESS);
    }
    for session in &view.sessions {
        let (files, version) = match &session.applicability {
            change_link::Applicability::Applies {
                scope_version,
                in_scope_paths,
            } => (in_scope_paths.len() as u64, *scope_version as u64),
            change_link::Applicability::AppliesAsExamination { scope_version, .. } => {
                (0, *scope_version as u64)
            }
            change_link::Applicability::NotApplicable { .. } => (0, 0),
        };
        println!("  {}", session.session_id);
        println!("    {}", plain(session.sentence_slug, files, version)?);
        // A count reaches a person with its reviewed word attached, built by
        // `voice::composition` — never a bare number beside an engine enum.
        let parts: Vec<(u64, &str)> = session
            .verdict_counts
            .iter()
            .map(|(slug, count)| {
                (
                    *count,
                    voice::verdict(slug)
                        .map(|t| t.word)
                        .unwrap_or(slug.as_str()),
                )
            })
            .collect();
        if !parts.is_empty() {
            println!("      {}", voice::composition(&parts));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// T-3-4 — `thruline change cancel-run <run>`.
///
/// One act, one record, and the sentence a person reads is the copy file's,
/// not this file's: the execution and cleanup lines come from
/// `voice::execution_line`, which is the same table the screen renders. The
/// verb never says "pid", "process group" or "SIGTERM" — those live in the
/// Technical half, one level down, where they belong.
///
/// Exit 1 on a refusal, because "there was nothing to stop" is not success.
fn cancel_run(ledger: &Ledger, run: &str, actor: Option<String>, json: bool) -> Result<ExitCode> {
    let who = actor
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| match std::env::var("USER") {
            Ok(user) if !user.trim().is_empty() => user,
            _ => "you".to_string(),
        });
    let acted = crate::run_supervisor::cancel(
        ledger,
        run,
        &who,
        thruline_core::run_execution::Reason::CancelledByPerson,
    )
    .context("stopping this check and looking at what it left")?;
    match acted {
        Err(refusal) => {
            if json {
                crate::json_out::print(
                    &crate::json_out::CHANGE_CANCEL_RUN,
                    serde_json::json!({
                        "cancelled": false,
                        "refusal": refusal.slug(),
                        "plain": refusal.plain(),
                    }),
                )?;
            } else {
                println!("{}", refusal.plain());
            }
            Ok(ExitCode::FAILURE)
        }
        Ok(done) => {
            let stopped = voice::execution_line("execution_cancelled")
                .map(|l| l.plain)
                .unwrap_or_default();
            let cleanup_slug = match done.cleanup {
                crate::run_supervisor::Cleanup::Verified => "execution_cleanup_verified",
                crate::run_supervisor::Cleanup::Survivors(_) => "execution_cleanup_survivors",
                crate::run_supervisor::Cleanup::Unknown => "execution_cleanup_unknown",
            };
            let cleanup = voice::execution_line(cleanup_slug)
                .map(|l| l.plain)
                .unwrap_or_default();
            if json {
                crate::json_out::print(
                    &crate::json_out::CHANGE_CANCEL_RUN,
                    serde_json::json!({
                        "cancelled": true,
                        "run": done.run_key,
                        "name": done.name,
                        "actor": who,
                        "cleanup_slug": cleanup_slug,
                        "plain": stopped,
                        "cleanup": cleanup,
                    }),
                )?;
            } else {
                println!("{}  {}", done.name, stopped);
                println!("  {cleanup}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn links(ledger: &Ledger, session: &str, json: bool) -> Result<ExitCode> {
    let view =
        change_link::session_links(ledger, session).context("reading this session's links")?;
    if json {
        let sentence = match view.none_slug {
            Some(slug) => Some(plain(slug, 0, 0)?),
            None => None,
        };
        return crate::json_out::print(
            &crate::json_out::CHANGE_LINKS,
            serde_json::json!({ "links": view, "sentence": sentence }),
        )
        .map(|_| ExitCode::SUCCESS);
    }
    if let Some(slug) = view.none_slug {
        println!("{}", plain(slug, 0, 0)?);
        return Ok(ExitCode::SUCCESS);
    }
    for link in &view.changes {
        println!("  {}", link.change_title);
        println!(
            "    {}",
            voice::change_label(link.label_slug)
                .map(|t| t.word)
                .unwrap_or_default()
        );
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// T-2-3 — what counts as done: proposed, then accepted
// ---------------------------------------------------------------------------

/// `thruline change propose` — read the request and the confirmed work, and say
/// what this Change could be judged against. Writes nothing.
fn propose(ledger: &Ledger, change: &str, json: bool) -> Result<ExitCode> {
    let environment = change_propose::machine_environment(ledger, change)
        .context("reading what this machine can run")?;
    let view = change_propose::propose(ledger, change, &environment)
        .context("proposing what counts as done")?;

    if json {
        let mut criteria = Vec::new();
        for item in &view.criteria {
            criteria.push(serde_json::json!({
                "number": item.number,
                "text": item.text,
                "reasoning": both(item.reasoning_slug, 0, 0, "")?,
            }));
        }
        let mut notes = Vec::new();
        for note in &view.notes {
            notes.push(serde_json::json!({
                "text": note.text,
                "reason": both(note.reason_slug, 0, 0, "")?,
            }));
        }
        let mut checks = Vec::new();
        for check in &view.profile.checks {
            checks.push(serde_json::json!({
                "id": check.id,
                "need": both(check.need_slug, 0, 0, "")?,
                "availability": both(check.availability_slug, 0, 0, &check.availability_name)?,
            }));
        }
        return crate::json_out::print(
            &crate::json_out::CHANGE_PROPOSE,
            serde_json::json!({
                "proposals": view,
                "criteria_said": criteria,
                "notes_said": notes,
                "checks_said": checks,
                "not_evidence": both(view.profile.not_evidence_slug, 0, 0, "")?,
                "versions": both(
                    view.versions_slug,
                    view.versions_used as u64,
                    view.versions_allowed as u64,
                    "",
                )?,
            }),
        )
        .map(|_| ExitCode::SUCCESS);
    }

    if let Some(slug) = view.empty_slug {
        println!("{}", said(slug, 0, 0, "")?);
    }
    for item in &view.criteria {
        println!("  {}. {}", item.number, item.text);
        println!("     {}", said(item.reasoning_slug, 0, 0, "")?);
    }
    for note in &view.notes {
        println!("  {}", note.text);
        println!("     {}", said(note.reason_slug, 0, 0, "")?);
    }
    for slug in &view.profile.change_type_slugs {
        println!("  {}", said(slug, 0, 0, "")?);
    }
    for check in &view.profile.checks {
        println!("  {}", check.id);
        println!("     {}", said(check.need_slug, 0, 0, "")?);
        println!(
            "     {}",
            said(check.availability_slug, 0, 0, &check.availability_name)?
        );
    }
    if let Some(slug) = view.profile.unknown_slug {
        println!("  {}", said(slug, 0, 0, "")?);
    }
    println!("{}", said(view.profile.not_evidence_slug, 0, 0, "")?);
    if let Some(slug) = view.not_accepted_slug {
        println!("{}", said(slug, 0, 0, "")?);
    }
    println!(
        "{}",
        said(
            view.versions_slug,
            view.versions_used as u64,
            view.versions_allowed as u64,
            "",
        )?
    );
    Ok(ExitCode::SUCCESS)
}

/// `thruline change accept` — one person writes down what counts as done.
///
/// `--actor` is required and the refusal is the copy file's sentence, said
/// before the store is opened. `--all` and `--criterion` are exclusive and
/// exactly one is required: a verb that guessed which lines were meant would be
/// accepting something nobody chose.
#[allow(clippy::too_many_arguments)]
fn accept(
    ledger: &Ledger,
    change: &str,
    all: bool,
    criteria: &[u32],
    edits: &[String],
    scope: &[String],
    replacing: bool,
    actor: Option<String>,
    rationale: Option<String>,
    json: bool,
) -> Result<ExitCode> {
    let selection = match (all, criteria.is_empty()) {
        (true, true) => change_propose::Selection::All,
        (false, false) => change_propose::Selection::Numbers(criteria.to_vec()),
        _ => bail!("say which: --all, or --criterion <n> one or more times, not both"),
    };
    let Some(actor) = actor.filter(|a| !a.trim().is_empty()) else {
        bail!("{}", said("accept_needs_actor", 0, 0, "")?);
    };
    let mut parsed_edits = std::collections::BTreeMap::new();
    for edit in edits {
        let Some((number, text)) = edit.split_once('=') else {
            bail!("an edit is written `--edit <n>=<the line you want>`");
        };
        let number: u32 = number
            .trim()
            .parse()
            .with_context(|| format!("{number:?} is not one of the numbers beside a line"))?;
        parsed_edits.insert(number, text.to_string());
    }

    let environment = change_propose::machine_environment(ledger, change)
        .context("reading what this machine can run")?;
    let outcome = change_propose::accept(
        ledger,
        change,
        &change_propose::Acceptance {
            selection,
            // No `--scope` is not "cover nothing": it is "the folders you were
            // shown", which is what the ordinary path accepts. A person who
            // types one takes the list over, and it is attributed to them.
            scope: if scope.is_empty() {
                change_propose::ScopeChoice::AsProposed
            } else {
                change_propose::ScopeChoice::Paths(scope.to_vec())
            },
            replacing,
            edits: parsed_edits,
            actor,
            rationale: rationale.unwrap_or_default(),
            accepted_at: thruline_core::ingest::iso8601_utc_now(),
        },
        &environment,
    )
    // Not `.context(...)`: every refusal `accept` returns is already a sentence
    // from the copy file, and a context line in front of it makes the first
    // thing a person reads a fragment of engine prose.
    .map_err(|problem| anyhow::anyhow!("{problem}"))?;

    let count = outcome.criteria.len() as u64;
    let version = outcome.version as u64;
    if json {
        let mut lines = Vec::new();
        for item in &outcome.criteria {
            lines.push(serde_json::json!({
                "text": item.text,
                "attribution": both(item.attribution_slug, 0, 0, "")?,
                "not_observable": match item.not_observable_slug {
                    Some(slug) => Some(both(slug, 0, 0, "")?),
                    None => None,
                },
            }));
        }
        return crate::json_out::print(
            &crate::json_out::CHANGE_ACCEPT,
            serde_json::json!({
                "accepted": outcome,
                "criteria_said": lines,
                "said": both(outcome.sentence_slug, count, version, "")?,
                "not_an_approval": both(outcome.not_an_approval_slug, 0, 0, "")?,
            }),
        )
        .map(|_| ExitCode::SUCCESS);
    }

    println!("{}", said(outcome.sentence_slug, count, version, "")?);
    for item in &outcome.criteria {
        println!("  {}", item.text);
        println!("     {}", said(item.attribution_slug, 0, 0, "")?);
        if let Some(slug) = item.not_observable_slug {
            println!("     {}", said(slug, 0, 0, "")?);
        }
    }
    if !outcome.allowed_scope.is_empty() {
        println!("{}", said("scope_from_the_work", 0, 0, "")?);
        for path in &outcome.allowed_scope {
            println!("  {path}");
        }
        println!("     {}", said(outcome.scope_attribution_slug, 0, 0, "")?);
    } else {
        println!("{}", said("scope_none_found", 0, 0, "")?);
    }
    println!("{}", said(outcome.not_an_approval_slug, 0, 0, "")?);
    Ok(ExitCode::SUCCESS)
}

/// `change prepare` (T-3-2, `B4.6`, `B6.2`, `B6.3`) — write down what will be
/// examined for this Change, and read it back in Sam's words.
///
/// It observes: the workspace identity (the tree hash that includes uncommitted
/// changes), one stored snapshot for the lockfile digests and the dirty-diff
/// digest, the tools the accepted check profile names with their own
/// `--version` answers, and — only when a port is named — the owner of that
/// listening socket, its program and the database that process has open.
///
/// It decides nothing. Writing down what will be examined is not a claim that
/// anything was.
fn prepare(
    ledger: &Ledger,
    change: &str,
    app_port: Option<u16>,
    config_files: &[String],
    root: Option<PathBuf>,
    json: bool,
) -> Result<ExitCode> {
    use thruline_core::examined_inputs as exi;

    let at = thruline_core::ingest::iso8601_utc_now();
    let root = match root {
        Some(r) => r,
        None => std::env::current_dir().context("reading the current directory")?,
    };
    // T-S-2: the standing `intended` record before this write, so a second
    // `change prepare` on an already-open Change — the other moment (besides
    // `check --for-change`) a person can move its tree version — can tell
    // whether it moved. Read before the new one lands, exactly as
    // `bind_intended_at` reads it, so a first prepare correctly compares
    // against nothing rather than an invented baseline.
    let previous = exi::standing(ledger, exi::SubjectKind::Intended, change)?;

    let (mut inputs, snapshot) = exi::observe_source_with_snapshot(&root, &at);
    if let Some(snap) = &snapshot {
        if thruline_core::snapshot::store(ledger, snap).is_err() {
            inputs.snapshot_id = exi::Input::Unknown;
        }
    }
    inputs.subject_kind = exi::SubjectKind::Intended;
    inputs.subject_id = change.to_string();
    let (digest, names) = exi::config_digest(&root, config_files);
    inputs.config_sha256 = digest;
    inputs.config_files = names;
    inputs.tools = profile_tools(ledger, change)?
        .iter()
        .map(|t| crate::uiserve::probe_tool_for_prepare(t, &root))
        .collect();
    if let Some(port) = app_port {
        inputs.target = exi::probe_running_app(port);
    }
    let outcome = exi::record(ledger, &inputs, &at).context("writing what will be examined")?;

    // The freshness trigger (T-S-2): unconditional, the same way
    // `bind_intended_at` calls it — whether the tree actually moved (a new
    // snapshot id) is decided inside `on_intended_bound`, not here. A
    // failure here does not fail `prepare` itself: writing down what will be
    // examined is the more load-bearing fact, but it is not swallowed.
    if let Err(e) = thruline_core::evidence_applicability::on_intended_bound(
        ledger,
        change,
        previous.as_ref(),
        &inputs,
        &at,
    ) {
        eprintln!("thruline change prepare: freshness was not reassessed: {e}");
    }
    let view = exi::prepare_view(ledger, change).context("reading what will be examined")?;
    let sentences = prepare_sentences(&view)?;

    if json {
        return crate::json_out::print(
            &crate::json_out::CHANGE_PREPARE,
            serde_json::json!({
                "record": outcome,
                "prepare": view,
                "sentences": sentences,
            }),
        )
        .map(|_| ExitCode::SUCCESS);
    }
    for (plain_line, _) in &sentences {
        println!("  {plain_line}");
    }
    Ok(ExitCode::SUCCESS)
}

/// Every sentence the Prepare view renders, in both registers, looked up by
/// slug. Nothing in this file writes prose for a person, and nothing in this
/// file chooses which sentence a fact gets: the engine's `prepare_view` names
/// the slug, so the CLI and the screen cannot disagree about what a port with
/// nothing on it says.
fn prepare_sentences(
    view: &thruline_core::examined_inputs::PrepareView,
) -> Result<Vec<(String, String)>> {
    use thruline_core::examined_inputs::Fills;

    let missing = inputs_both(view.missing_slug, &Fills::default())?.0;
    let mut out = Vec::new();
    for line in &view.lines {
        let fills = Fills {
            port: line.value.as_deref().and_then(|v| v.parse::<u16>().ok()),
            app: view.app_program.clone(),
            tool: line.tool.clone(),
            value: Some(line.value.clone().unwrap_or_else(|| missing.clone())),
            count: line.value.as_deref().and_then(|v| v.parse::<u64>().ok()),
        };
        out.push(inputs_both(line.slug, &fills)?);
    }
    Ok(out)
}

fn inputs_both(
    slug: &str,
    fills: &thruline_core::examined_inputs::Fills,
) -> Result<(String, String)> {
    match voice::inputs_sentence(slug, fills) {
        Some(pair) => Ok(pair),
        None => bail!("no copy-file line for {slug:?}"),
    }
}

/// The programs the accepted check profile names, in the profile's own order.
/// A Change with no accepted profile names no tools — it does not guess one.
fn profile_tools(ledger: &Ledger, change: &str) -> Result<Vec<String>> {
    let profile = change_propose::accepted_check_profile(ledger, change)
        .context("reading the accepted check profile")?;
    let environment = change_propose::machine_environment(ledger, change)
        .context("reading what this machine can run")?;
    let mut out: Vec<String> = Vec::new();
    for entry in change_propose::accepted_check_availability(&profile, &environment) {
        if !out.iter().any(|t| t == entry.tool) {
            out.push(entry.tool.to_string());
        }
    }
    Ok(out)
}

/// T-5-3 — `thruline change waive` (`B7.18`-`B7.22`).
///
/// The terminal door onto the same act as
/// `POST /api/changes/<id>/obligations/<obligation>/waive`: both call
/// `change_readiness::waive`, so an exception written here and one written
/// from the screen are one record shape with one set of refusals.
///
/// **The authority is not an argument.** It is read off the accepted check
/// profile, which is what makes the record `B7.19`'s and not the caller's. A
/// Change whose accepted profile configures no exception path is refused, as
/// is any of the three prerequisites of `B7.12`, and a refusal exits 1 —
/// "nothing was set aside" is not success.
fn waive(
    ledger: &Ledger,
    change: &str,
    obligation: &str,
    actor: Option<String>,
    rationale: &str,
    not_applicable: bool,
    json: bool,
) -> Result<ExitCode> {
    let who = actor
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| match std::env::var("USER") {
            Ok(user) if !user.trim().is_empty() => user,
            _ => "you".to_string(),
        });
    let kind = if not_applicable {
        thruline_core::change_readiness::WaiverKind::NotApplicable
    } else {
        thruline_core::change_readiness::WaiverKind::Waived
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    match thruline_core::change_readiness::waive(
        ledger, change, obligation, kind, &who, rationale, &at,
    ) {
        Err(refusal) => {
            let sentence = refusal.to_string();
            if json {
                crate::json_out::print(
                    &crate::json_out::CHANGE_WAIVE,
                    serde_json::json!({
                        "recorded": false,
                        "change": change,
                        "obligation": obligation,
                        "plain": sentence,
                    }),
                )?;
            } else {
                println!("{sentence}");
            }
            Ok(ExitCode::FAILURE)
        }
        Ok(record) => {
            /* The sentence a person reads is the copy file's, filled from the
             * record's own fields. Nothing here writes prose. */
            let sentence = voice::readiness_line(kind.slug())
                .map(|line| {
                    line.plain
                        .replace("{who}", &record.actor)
                        .replace("{authority}", &record.authority)
                        .replace("{why}", &record.rationale)
                })
                .unwrap_or_default();
            if json {
                crate::json_out::print(
                    &crate::json_out::CHANGE_WAIVE,
                    serde_json::json!({
                        "recorded": true,
                        "change": change,
                        "obligation": record.obligation_id,
                        "kind": kind.as_str(),
                        "actor": record.actor,
                        "authority": record.authority,
                        "rationale": record.rationale,
                        "plain": sentence,
                    }),
                )?;
            } else {
                println!("{}", record.obligation_id);
                println!("  {sentence}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}
