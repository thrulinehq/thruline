use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use thruline_core::ingest::{
    default_codex_roots, default_db_path, default_mirror_root, default_roots, ingest, ingest_codex,
    CodexIngestOptions, IngestReport,
};
use thruline_core::ledger::Ledger;
use thruline_core::perf_basis;
use thruline_core::record_verification::{SealedCopyState, SourceState, VerifyReport};
use thruline_core::voice;
use thruline_core::{codex_ingest::detect_source_path, Source};

mod acceptance_cmd;
mod acceptance_exec;
mod acceptance_handoff;
mod acceptance_receipt;
mod attempts_cmd;
mod bench_cmd;
mod change_cmd;
mod check;
mod claims_cmd;
mod decide_cmd;
mod demo_cmd;
mod doctor;
mod exclusion_serve;
mod explain_cmd;
mod export_all;
mod finding_cmd;
mod gate_cmd;
mod hold_cmd;
mod hook_install;
mod import_codex;
mod import_legacy;
mod integration_cmd;
mod json_out;
mod licence_cmd;
mod note_cmd;
mod pilot_cmd;
mod policy_cmd;
mod promises_cmd;
mod receipt_cmd;
mod reexec_cmd;
mod run_cmd;
mod run_supervisor;
mod second_cmd;
mod sensor_bridge;
mod sessions_cmd;
mod shim_cmd;
mod sync_cmd;
mod tui_cmd;
mod uiserve;
mod verification_checks;
mod verification_pack;
mod verification_scope;
mod verification_structural;
mod verify_cmd;

/// The first line of a CLI-printed Context Packet (RA2).
///
/// Never reaches an agent: `thruline mcp` serves `packet.render()` directly, so
/// this frame belongs to the terminal and only to the terminal.
const PACKET_BANNER: &str =
    "# AGENT-FACING ARTIFACT — for the human view run: thruline ui (or: thruline now)";

/// A digest of the engine source this binary was compiled from, stamped in by
/// `build.rs` (P2b). `unavailable` when the source tree could not be read at
/// build time — which matches no basis, so every figure reads as stale, which is
/// the right answer when the code under measurement cannot be identified.
const CODE_DIGEST: &str = env!("THRULINE_CODE_DIGEST");

/// The conditions here, in the `[[basis]]` shape `docs/perf-basis.toml` takes,
/// so a fresh measurement is pasted rather than transcribed. Transcription is
/// where A191's stamp lost its head counts.
fn perf_stamp_block(observed: &thruline_core::perf_basis::Observed) -> String {
    let mut out = String::from(
        "[[basis]]\nid = \"B-CHANGEME\"\nrecorded = \"YYYY-MM-DD\"\nrecoverable = true\n",
    );
    out.push_str(&format!("projects = {}\n", observed.projects));
    out.push_str(&format!("events = {}\n", observed.events));
    out.push_str(&format!("sessions = {}\n", observed.sessions));
    out.push_str(&format!("nodes = {}\n", observed.nodes));
    out.push_str(&format!("project_heads = {}\n", observed.project_heads));
    out.push_str(&format!(
        "node_projections = {}\n",
        observed.node_projections
    ));
    out.push_str(&format!("code_digest = \"{}\"\n", observed.code_digest));
    out.push_str(&format!("profile = \"{}\"\n", observed.profile));
    out.push_str(&format!("projects_table = {}\n", observed.projects_table));
    if let Some(bytes) = observed.ledger_bytes {
        out.push_str(&format!("ledger_bytes = {bytes}\n"));
    }
    out
}

/// What a person sees when they type `thruline` and press return.
///
/// Five lines, and every one of them is something a non-developer can do
/// (UX-DOCTRINE §3: answers, not internals). The ops and agent verbs are real
/// and stay one `--help` away; they are not the first thing anyone meets.
const QUICKSTART: &str = "\
ThruLine reads your AI coding sessions and shows what was actually done.

  thruline ui       open it in your browser — start here
  thruline demo     a two-minute walkthrough on made-up data, nothing of yours
  thruline now      what needs you, in the terminal
  thruline autopsy  what your records hold, in one screen — nothing leaves this device
  thruline ingest   read your sessions in for the first time
  thruline --help   everything else";

#[derive(Parser)]
#[command(
    name = "thruline",
    version,
    about = "Reads local AI coding sessions and shows what the record can prove.",
    after_help = "Start with `thruline ui`. Everything from `packet` down is written for \
                  agents, integrations, and maintenance rather than for people (RA2)."
)]
struct Cli {
    /// Ledger location. Defaults to ~/.thruline/ledger.db — never inside the repo.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Optional so that bare `thruline` can answer with the human quickstart
    /// instead of clap's "error: a subcommand is required". A person who types
    /// the product's name has asked a reasonable question and should not be
    /// met with an error and a wall of ops verbs (RA2).
    #[command(subcommand)]
    command: Option<Command>,
}

/// `thruline run`'s arguments, in their own type.
///
/// **Not inlined into [`Command`], deliberately.** `clap_derive` builds every
/// inline variant's arguments in one generated function, and that function's
/// stack frame grows with the whole enum: adding this variant inline made
/// `Cli::try_parse_from` overflow the 2 MiB test-thread stack — measured, as an
/// abort in an unrelated `entity` test that had passed for months. A `clap::Args`
/// struct is built by its own function, in its own frame. The existing test is
/// what caught it, which is why it is worth saying here rather than only in a
/// commit message.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// Session id recorded execs are attributed to. Derived when absent.
    #[arg(long)]
    session: Option<String>,
    /// Which harness's spool this run joins.
    #[arg(long, default_value = "claude-code")]
    harness: String,
    #[arg(long)]
    repo: Option<PathBuf>,
    /// Give the child pipes even under a terminal.
    #[arg(long)]
    no_pty: bool,
    /// Write the JSON report here.
    ///
    /// The only unambiguous way to read it: this command is a pass-through, so
    /// the child owns stdout and stderr and a report printed to either shares
    /// the stream with the agent's own output.
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    /// Everything after `--`.
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

/// Separate clap frame, as with RunArgs, to preserve the default test-thread stack.
#[derive(Debug, clap::Args)]
pub struct ReceiptArgs {
    /// Predicate v0.1 by default (RECEIPT-V01-DEFAULT, R-2026-09-28-21): the
    /// version the `require_receipt` policy gate reads. v0.2 (extractor
    /// provenance, detached file) only when asked for by this flag.
    #[arg(long, default_value = "0.1", value_parser = ["0.1", "0.2"])]
    predicate_version: String,
    #[arg(long)]
    session: Option<String>,
    /// PKCS#8 PEM signing key. Defaults to `$THRULINE_HOME/signing.key`,
    /// generated on first use.
    #[arg(long)]
    sign_with: Option<PathBuf>,
    #[arg(long)]
    out: Option<PathBuf>,
    /// Also print the `ThruLine-Receipt:` commit trailer line.
    #[arg(long)]
    trailer: bool,
    #[arg(long)]
    exec_db: Option<PathBuf>,
    #[arg(long)]
    repo: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

/// `thruline import-codex`'s arguments, in their own type.
///
/// Flattened for the same reason `RunArgs` is: `clap_derive` builds every
/// inline variant's arguments in one generated function whose stack frame grows
/// with the whole enum, and one more variant overflowed the 2 MiB test-thread
/// stack last round.
#[derive(Debug, clap::Args)]
pub struct ImportCodexArgs {
    /// Defaults to `~/.codex/sessions` — user-global and date-nested, which is
    /// not what §4.2 says.
    #[arg(long)]
    sessions_root: Option<PathBuf>,
    /// Observe only complete JSONL records from this bounded log (16 MiB maximum).
    #[arg(long, conflicts_with_all=["sessions_root", "dry_run", "limit"])]
    session_file: Option<PathBuf>,
    #[arg(long)]
    exec_db: Option<PathBuf>,
    /// Only rollouts whose recorded `cwd` is inside this repository.
    #[arg(long)]
    repo: Option<PathBuf>,
    /// Import at most this many rollouts.
    #[arg(long)]
    limit: Option<usize>,
    /// Count what would be imported and write nothing.
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    json: bool,
}

/// `thruline import-legacy`'s arguments, flattened into their own type.
///
/// Not cosmetic: clap's derive builds every inline variant's arguments in one
/// generated function, and that function's stack frame is what overflows a test
/// thread. Flattening the largest variants moves their construction into their
/// own `Args` impls. Measured, twice: adding one more inline variant is what
/// made `cargo test -p thruline-cli --bin thruline` abort with a stack overflow
/// in an unrelated test, both times.
#[derive(Debug, clap::Args)]
pub struct ImportLegacyArgs {
    /// The legacy ledger. Defaults to `~/.thruline/ledger.db`.
    #[arg(long)]
    source: Option<PathBuf>,
    /// Write the §11 store here instead of `~/.thruline/exec.db`.
    #[arg(long)]
    exec_db: Option<PathBuf>,
    /// Import only this legacy project id.
    #[arg(long)]
    project: Option<String>,
    /// Count and report; write nothing.
    #[arg(long)]
    dry_run: bool,
    /// Stop after this many legacy rows.
    #[arg(long)]
    limit: Option<u64>,
    /// Maximum rows per page. 500 is measured, not chosen: at 400 k rows
    /// page 500 costs 101 MB / 30.6 s where page 5000 costs 352 MB / 42.0 s
    /// — 3.5x the memory AND slower.
    #[arg(long, default_value_t = 500)]
    page_size: u64,
    /// Approximate maximum payload bytes held in one page. This, not
    /// `--page-size`, is what caps memory: legacy tool results are
    /// unbounded per row.
    #[arg(long, default_value_t = 8 * 1024 * 1024)]
    page_bytes: u64,
    /// Import `kind='meta'` rows as individual events instead of folding
    /// them into one per-session census. 1.44 M rows naming unrecognised
    /// JSONL fields; off by default.
    #[arg(long)]
    include_meta: bool,
    /// After importing, walk every session's chain and report whether it
    /// verifies. Streaming; costs one event of memory.
    #[arg(long)]
    verify: bool,
    #[arg(long)]
    json: bool,
}

/// `thruline sync`'s arguments.
#[derive(Debug, clap::Args)]
pub struct SyncArgs {
    /// `login`, `push`, `pull` or `status`.
    verb: String,
    #[arg(long)]
    json: bool,
}

/// `thruline sessions`'s arguments, flattened into their own type.
///
/// Inline variant bodies are what overflowed the 2 MiB test-thread stack once
/// already; every command added since carries its arguments in a struct
/// (`crates/cli/tests/cli_surface_budget.rs`).
#[derive(Debug, clap::Args)]
pub struct SessionsArgs {
    /// The execution store to read. Defaults to `~/.thruline/exec.db`
    /// (`$THRULINE_HOME` overrides). Not the projects ledger `--db` names.
    #[arg(long)]
    exec_db: Option<PathBuf>,
    /// How many sessions to print. The store holds tens of thousands; the
    /// total is printed either way, so what was left out is stated.
    #[arg(long, default_value_t = 50)]
    limit: u32,
    /// Apply the row limit to each recorded project instead of the whole store.
    #[arg(long)]
    per_project: bool,
    #[arg(long)]
    json: bool,
}

/// `thruline change`'s arguments, flattened into their own type.
///
/// Flattened, like every command added since the stack-overflow incident the
/// `cli_surface_budget` test exists against.
#[derive(Debug, clap::Args)]
pub struct ChangeArgs {
    #[command(subcommand)]
    verb: ChangeVerb,
}

/// The seven verbs. `suggest` and `propose` may only ever propose; `new`,
/// `associate` and `accept` are the three human acts, and none of them can run
/// without a name.
#[derive(Debug, clap::Subcommand)]
pub enum ChangeVerb {
    /// Open a Change: the piece of work you want to be able to hand over.
    /// Writes one row and nothing else — what counts as done, and which work
    /// belongs to it, are separate acts.
    New {
        /// The one line you would say out loud to describe the work.
        #[arg(long)]
        title: String,
        /// Which project this Change is about. Needed only when the store
        /// holds work under more than one name.
        #[arg(long = "for", value_name = "PROJECT")]
        project: Option<String>,
        /// The project folder to bind explicitly when the captured record no
        /// longer names a folder on this machine.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Why, in your own words. Kept with the request.
        #[arg(long)]
        purpose: Option<String>,
        /// Who is asking for this work. Without it, nothing is written.
        #[arg(long)]
        requester: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Write down what exactly will be examined for this Change: the project
    /// folder, the starting point, whether your unsaved changes are included,
    /// the tools that would run, and the running app it is about.
    Prepare {
        /// The Change this is about.
        change: String,
        /// The app this Change is about is the one listening on this port.
        #[arg(long = "app-port")]
        app_port: Option<u16>,
        /// A config or test file whose bytes are part of what is examined.
        /// Repeatable.
        #[arg(long = "config-file")]
        config_files: Vec<String>,
        /// The project folder to observe. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// What this Change could be judged against, proposed from the request and
    /// the work already confirmed as part of it. Reads only; writes nothing.
    Propose {
        /// The Change to propose criteria for.
        change: String,
        #[arg(long)]
        json: bool,
    },
    /// Write down what counts as done. Requires a name. This is not an approval.
    Accept {
        change: String,
        /// Accept every line that was proposed.
        #[arg(long)]
        all: bool,
        /// Accept one line, by the number beside it. Repeatable.
        #[arg(long = "criterion", value_name = "N")]
        criteria: Vec<u32>,
        /// Change one line before accepting it, as `<n>=<the line you want>`.
        /// The proposal is kept; the accepted line is yours.
        #[arg(long = "edit", value_name = "N=TEXT")]
        edits: Vec<String>,
        /// A folder this Change covers, instead of the ones proposed from the
        /// work already confirmed for it. Repeatable; your list, attributed to
        /// you. Without it, the proposed folders are what is written down.
        #[arg(long = "scope", value_name = "PATH")]
        scope: Vec<String>,
        /// Say that you mean to drop or soften lines you already accepted.
        /// Without it, that is refused rather than done quietly.
        #[arg(long)]
        replacing: bool,
        /// Who is accepting. Without it, nothing is written.
        #[arg(long)]
        actor: Option<String>,
        /// Why, in your own words. Kept with the acceptance.
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Propose sessions that might belong to a Change, from timing and from
    /// the files they changed. Reads only unless `--record` is given.
    Suggest {
        /// The Change to propose links for.
        change: String,
        /// Write each proposal down as a suggestion. It stays a suggestion.
        #[arg(long)]
        record: bool,
        /// `--json` is per verb, not on `change` itself: a bare `thruline
        /// change` emits nothing, and a flag on it would claim a shape that
        /// does not exist.
        #[arg(long)]
        json: bool,
    },
    /// Say whether a session belongs to a Change. Requires a name.
    Associate {
        change: String,
        session: String,
        /// This session belongs to this Change.
        #[arg(long)]
        confirm: bool,
        /// This session does not belong to this Change.
        #[arg(long)]
        reject: bool,
        /// Who is saying so. Without it, nothing is written.
        #[arg(long)]
        actor: Option<String>,
        /// Why, in your own words. Kept with the decision.
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// What the linked work shows for a Change — or why it is not counted.
    Results {
        change: String,
        #[arg(long)]
        json: bool,
    },
    /// Which Changes one session is linked to, or none.
    Links {
        #[arg(long)]
        session: String,
        #[arg(long)]
        json: bool,
    },
    /// T-3-4 (`B7.1`, `B12.6`) — stop a check that is still running, and
    /// confirm nothing of it was left behind.
    ///
    /// It signals the run's own process group, waits a bounded time, LOOKS at
    /// the process table, and appends a `cancelled` record carrying who asked,
    /// when, and what was found. Whatever the run had observed up to that
    /// point is kept exactly as it stands and satisfies nothing.
    CancelRun {
        /// The run to stop, as `thruline change results <id>` names it.
        #[arg(long)]
        run: String,
        /// Who is stopping it. Defaults to the account this is running as.
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// T-5-3 (`B7.18`-`B7.22`) — set one thing that counts as done aside,
    /// without evidence for it, under the authority the accepted profile
    /// records.
    ///
    /// It never turns anything into a result: the line stays unchecked, says
    /// who set it aside and why wherever it appears, and the approval that
    /// follows reads "Approved with exceptions". Candidate identity, decision
    /// authority and packet consistency are refused outright, and so is any
    /// Change whose accepted profile names nobody who may do this.
    Waive {
        /// The Change this is about.
        #[arg(long)]
        change: String,
        /// The obligation to set aside, as the readiness answer names it
        /// (`criterion:1`, `review`).
        #[arg(long)]
        obligation: String,
        /// Who is setting it aside. Defaults to the account this is running as.
        #[arg(long)]
        actor: Option<String>,
        /// Why, in words a person can read. Required.
        #[arg(long)]
        rationale: String,
        /// Say it does not apply to this Change at all, rather than waiving it.
        #[arg(long)]
        not_applicable: bool,
        #[arg(long)]
        json: bool,
    },
    /// T-4-2 (`B9.10`–`B9.14`, `Q9`, `Q10`) — record where one thing a review
    /// found stands. Appends one row; nothing is edited. A fix needs evidence,
    /// setting one aside needs a reason and an authority, and a "resolved"
    /// mark reaches "waiting for a re-check" and nothing further.
    Finding(finding_cmd::FindingArgs),
    /// T-4-4-ATTEMPT-BUDGET — how many repair attempts a person allows this
    /// Change, counted from the checks run on it. With `--budget` it records
    /// one; without, it lists every attempt and says whether another may be
    /// made, exiting 1 once they are all used — `thruline check --for-change`
    /// then starts nothing on a new version. Runs nothing itself.
    Attempts(attempts_cmd::AttemptsArgs),
    /// C30 R-LEDGER — keep a note on a Change, in your own words, with your
    /// name and the time. A note is words: it changes no result, no count and
    /// no approval, and it is free.
    Note(note_cmd::NoteArgs),
    /// C30 R-LEDGER — put your name beside the approval that stands, as a
    /// second person (`--withdraw` takes your own back). It never makes an
    /// approval stand and changes no result, and it is free. ThruLine records
    /// the name you type; it does not check who is at the keyboard.
    Second(second_cmd::SecondArgs),
}

/// `thruline policy`'s arguments, flattened into their own type.
///
/// Inline variant bodies are what overflowed the 2 MiB test-thread stack once
/// already; every command added since carries its arguments in a struct.
#[derive(Debug, clap::Args)]
pub struct PolicyArgs {
    /// Write `.thruline/policy.yml` from the documented example. Refuses to
    /// overwrite an existing file.
    #[arg(long)]
    init: bool,
    /// Evaluate a specific session rather than the last one.
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    exec_db: Option<PathBuf>,
    #[arg(long)]
    repo: Option<PathBuf>,
    /// A policy file elsewhere than `.thruline/policy.yml`.
    #[arg(long)]
    policy: Option<PathBuf>,
    /// The branch this change merges into. The policy is then read from
    /// **there**, not from the checkout under review, so a change cannot
    /// weaken the gate that judges it. Defaults to `GITHUB_BASE_REF`.
    #[arg(long)]
    base: Option<String>,
    #[arg(long)]
    json: bool,
    /// Render §9's check-run markdown table (what the Action posts).
    #[arg(long)]
    md: bool,
    /// Print only `success`, `failure` or `neutral`, for scripts.
    #[arg(long)]
    conclusion: bool,
}

/// `thruline bench`'s arguments, in their own type — same reason as `RunArgs`.
#[derive(Debug, clap::Args)]
pub struct BenchArgs {
    /// Where downloaded corpora live. Defaults to `fixtures/bench`.
    #[arg(long)]
    fixtures: Option<PathBuf>,
    /// Render the Overclaim Index page rather than a terminal summary.
    #[arg(long)]
    html: bool,
    #[arg(long)]
    json: bool,
}

/// Keep UI argument construction in its own clap frame, as with [`RunArgs`].
/// The integrated debug parser overflowed a small test-thread stack (UQ-I19).
/// Splitting this large variant addresses the known generated-frame growth;
/// the original debug regression must verify the repair.
#[derive(Debug, clap::Args)]
pub struct UiArgs {
    /// Automatically observe Codex logs for the configured project while HQ runs.
    /// Polls local logs; messages remain agent reports, not execution proof.
    #[arg(long, value_name = "DIR", requires = "verification_project")]
    observe_codex: Option<PathBuf>,
    /// Show accepted requirements for this repository without exporting transcripts.
    #[arg(long, value_name = "REPO")]
    acceptance: Option<PathBuf>,
    /// Enable explicitly requested local checks in the existing evidence workspace.
    #[arg(long, value_name = "REPO", conflicts_with = "acceptance")]
    verification_project: Option<PathBuf>,
    /// Execution store used by the acceptance page.
    #[arg(long)]
    exec_db: Option<PathBuf>,
    /// Port to serve on. Left off, 7777 is tried and any free port is used
    /// if it is taken; the address is printed either way.
    #[arg(long)]
    port: Option<u16>,
    /// Serve without opening a browser.
    #[arg(long)]
    no_open: bool,
    /// Re-read every project before serving. Slow by nature — it walks the
    /// whole ledger. Happens automatically when there is no reading yet.
    #[arg(long)]
    refresh: bool,
    /// Where the reading is kept. Defaults to ~/.thruline/export.
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Include full-shadow classes. Diagnostics, not the product surface.
    #[arg(long)]
    diagnostics: bool,
    /// Serve a UI built elsewhere instead of the one embedded in this
    /// binary. The directory must exist and contain `index.html`; if it
    /// does not, this command refuses to start rather than show a blank
    /// page. See `docs/UI-SERVER-CONTRACT.md`.
    #[arg(long, value_name = "DIR")]
    static_dir: Option<PathBuf>,
    /// Address to bind. Loopback only — `127.0.0.1` or `localhost`.
    /// Anything else is refused before a socket exists, because this
    /// server hands out your transcripts.
    #[arg(long, value_name = "ADDR")]
    address: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Review and preserve required checks independently of an agent's claims.
    Acceptance(acceptance_cmd::Args),
    /// Review, accept, or retire a standing decision from a person's words.
    /// `decisions` is an alias for checking an exact project version.
    /// Keep the payload boxed: Clap otherwise overflows the default test-thread stack.
    #[command(visible_alias = "decisions")]
    Decide(Box<decide_cmd::DecisionRootCliArgs>),
    /// Your licence: its status, installing or removing it, and this
    /// machine's fingerprint. Checked on this machine; nothing is sent
    /// anywhere. Only approving a version needs one.
    #[command(alias = "license")]
    Licence(licence_cmd::Args),
    /// The `--json` contract: every JSON shape this binary emits, with its
    /// stable id and its `schema_version`. Always JSON, so a consumer can read
    /// the contract before it reads a document.
    #[command(display_order = 99)]
    JsonShapes,
    /// Ingest transcripts from Claude Code, Codex, or unambiguous defaults.
    #[command(display_order = 4)]
    Ingest {
        /// Actually append. Without it, `ingest` previews and writes nothing:
        /// scope, file counts, size estimate, and the free-disk headroom
        /// (A131 — a bare ingest once filled the disk and took the host down).
        #[arg(long)]
        apply: bool,

        /// Transcript root, a single project directory, or one .jsonl file.
        paths: Vec<PathBuf>,
        /// Source adapter. Auto fails closed when a supplied path is ambiguous.
        #[arg(long, value_enum, default_value_t = IngestSource::Auto)]
        source: IngestSource,
        /// Private sealed-mirror root for Codex generations.
        #[arg(long)]
        mirror_root: Option<PathBuf>,
    },
    /// Re-read raw files, recompute every hash and the full chain.
    #[command(display_order = 18)]
    Verify(Box<verify_cmd::VerifyArgs>),
    /// Evaluate every claim_completion submission awaiting verification
    /// (A201). Deterministic: each evidence-manifest entry is checked
    /// against the claiming session's own tool record and the workspace,
    /// and one verdict per claim is appended — the claim row itself is
    /// never touched. "No proof found" means the searched window held no
    /// match; it never means the claim is false.
    #[command(display_order = 12, name = "verify-claims")]
    VerifyClaims {
        #[arg(long)]
        json: bool,
    },
    /// Drop and re-derive nodes/edges for a project.
    #[command(display_order = 19)]
    Rebuild {
        #[arg(long, allow_hyphen_values = true, conflicts_with = "all")]
        project: Option<String>,
        /// Bring every project's projection to the current chain position.
        #[arg(long)]
        all: bool,
    },
    /// List projects with their event counts.
    #[command(display_order = 15)]
    Projects,
    /// Bring the ledger's schema up to date — the ONLY command that migrates.
    ///
    /// A schema upgrade is a full pass over the ledger and costs tens of
    /// seconds on a real corpus. It used to be charged silently to whichever
    /// process opened the ledger first after you installed a new binary, which
    /// is usually a background hook and not you. Now it is charged here, on
    /// purpose, where you can see it and choose when.
    ///
    /// Until you run it, every other command refuses in one line and does
    /// nothing — nothing is read, written, or half-migrated.
    #[command(display_order = 23)]
    Upgrade {
        /// Report what would be done and stop.
        #[arg(long)]
        dry_run: bool,

        /// Proceed even when the disk-space preflight says there is not room.
        #[arg(long)]
        force: bool,
    },
    /// The recorded performance figures, each with its stamped basis, and
    /// whether that basis still holds HERE (P2b).
    ///
    /// Nothing is stored about whether a figure is current: the conditions are
    /// measured now and compared axis by axis against the basis the figure was
    /// taken under. A figure whose basis has moved prints
    /// "stale basis — re-measure" INSTEAD of its numbers, because a number
    /// printed beside a warning is a number that gets quoted anyway.
    #[command(display_order = 24)]
    Perf {
        /// Print the stamp of the conditions here and stop, in the `[[basis]]`
        /// shape `docs/perf-basis.toml` takes.
        #[arg(long)]
        stamp: bool,

        /// Print the generated block `docs/perf-baseline.md` carries between
        /// its `perf-figures` markers, and stop. Opens no ledger: the block is
        /// currency-free by design, because a document cannot know whose
        /// machine is reading it.
        #[arg(long)]
        doc_block: bool,
    },
    /// Watch ThruLine prove itself, end to end, in a throwaway directory.
    ///
    /// Ingests a Claude session and a Codex session, resolves both to one repo,
    /// issues a context packet, freezes it into a handoff, runs a real command,
    /// and issues a successor packet that can now render a verified claim.
    ///
    /// Touches nothing of yours: no ledger, no config, no network. Needs `git`.
    #[command(display_order = 1)]
    Demo {
        /// **The default since M6**: BUILD-SPEC §15's first run — a scripted
        /// agent edits a file, weakens a test, runs one narrowed check for
        /// real, and claims "All tests pass". Prints what the execution record
        /// says about that.
        ///
        /// The continuity walk that used to be the default is `--continuity`.
        /// Two different demos wanted one name; the spec's owns it, and the
        /// change is recorded in CHANGELOG.md rather than made quietly.
        #[arg(long, conflicts_with_all = ["recovery", "continuity"])]
        claim: bool,

        /// The context/handoff walk that was this command's previous default.
        #[arg(long, conflicts_with = "recovery")]
        continuity: bool,

        /// Run the synthetic kill-9 preparation walk instead: protect, crash,
        /// stage, compare — killing only the demo's own child. It launches no
        /// successor and cannot prove recovery.
        #[arg(long)]
        recovery: bool,

        /// Keep the working directory and print its path, to inspect the
        /// ledger and the transcripts the walk produced.
        #[arg(long)]
        keep: bool,

        /// Keep §15's demo repository here instead of a temporary directory.
        #[arg(long)]
        dir: Option<PathBuf>,

        #[arg(long)]
        json: bool,
    },
    /// Set ThruLine up for Claude Code on this machine, in one command: the
    /// SENSOR hook pack, the speaker hooks (the in-session truth loop), and
    /// the context server. Prints exactly which files it touched and which
    /// entries it wrote — and changes nothing else.
    ///
    /// The sensor pack registers `thruline sensor append <Event>`, which
    /// appends one hash-chained delivery per hook into `sensor_events` in the
    /// ledger (`~/.thruline/ledger.db`). It is NOT the §4.1 capture pack
    /// (`thruline hook <harness> --spool`, installed by `hook-install`),
    /// which spools to the execution store — the two are different recorders
    /// and only one may be installed at a time.
    ///
    /// Given `--db <path>`, every hook and the context server are pinned to
    /// that ledger; without it the hooks name none and each resolves the
    /// default when it fires.
    #[command(display_order = 2)]
    Init {
        /// Claude Code settings file. Defaults to ~/.claude/settings.json.
        #[arg(long)]
        settings: Option<PathBuf>,
        /// Claude Code user config carrying the MCP server list. Defaults to
        /// ~/.claude.json.
        #[arg(long)]
        claude_json: Option<PathBuf>,
        /// Reverse exactly what init wrote: the hook entries and the context
        /// server entry. Recorded rows stay in the ledger.
        #[arg(long)]
        undo: bool,
        /// Answer yes to init's questions (the speaker-hook consent) without
        /// asking. On a terminal the consent defaults to yes on a plain Enter;
        /// without one (an agent or a script running init) nobody can answer,
        /// so the speaker hooks are installed only with this flag.
        #[arg(long)]
        yes: bool,
        /// Also install §4.1's opt-in Stop gate, which blocks a stop whose
        /// completion claim has no proof in the record.
        ///
        /// It rides BESIDE the speakers on `Stop` and appends no capture rows,
        /// so it adds no second recorder. Before this flag existed the gate
        /// lived only on `hook-install --gate`, which refuses on a machine
        /// `init` has already set up — so on the documented install path the
        /// gate could not be turned on at all.
        #[arg(long)]
        gate: bool,
    },
    /// Speaker (A200, SessionEnd hook): bind the ended session's final-turn
    /// completion claims to its own tool record and cache at most three
    /// receipt-carrying lines for the next session's brief. Reads the hook
    /// payload on stdin; fail-open — any error prints nothing and exits 0.
    #[command(display_order = 26)]
    Digest {
        #[command(subcommand)]
        action: DigestAction,
    },
    /// Speaker (A200, SessionStart hook): inject this project's cached digest
    /// as additionalContext, at most three lines, every one carrying a
    /// receipt. No cache → print nothing, exit 0 — healthy runs stay
    /// invisible. Reads the hook payload on stdin.
    #[command(display_order = 27)]
    Brief {
        /// Which hook invoked this; only "session-start" speaks.
        #[arg(long)]
        hook: String,
    },
    /// Speaker (A200, Stop hook): the stop-time claim audit, v0
    /// announce-to-ledger. Each completion claim from the turn with no proof
    /// in the session record is appended to `speaker_findings` and the digest
    /// cache is refreshed. Prints nothing and never blocks the session.
    #[command(display_order = 28)]
    Audit {
        /// Which hook invoked this; only "stop" audits.
        #[arg(long)]
        hook: String,
    },
    /// Render a frozen packet as the handoff artifact (R17b-2b).
    #[command(display_order = 11)]
    Handoff {
        #[arg(long, allow_hyphen_values = true, conflicts_with = "repo")]
        project: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        /// A specific issue id; defaults to the latest for the scope.
        #[arg(long)]
        issue: Option<String>,
    },
    /// What changed since a chain position you were served (R13).
    #[command(display_order = 13)]
    Deltas {
        #[arg(long, allow_hyphen_values = true, conflicts_with = "repo")]
        project: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        /// A chain position from a packet you were handed.
        #[arg(long = "since")]
        since: String,
    },
    /// Serve the three MCP tools over stdio (A89). No daemon, no port.
    #[command(display_order = 12)]
    Mcp {
        /// Who is calling. Required — there are no anonymous calls.
        #[arg(long)]
        agent: String,
    },
    /// Compile a deterministic Context Packet (M6.R3). Zero model calls.
    #[command(display_order = 10)]
    Packet {
        /// One project id.
        #[arg(long, allow_hyphen_values = true, conflicts_with = "repo")]
        project: Option<String>,
        /// A repo path, or a repo id from `identity groups` — compiles every
        /// working root of that repo as one context, across vendors.
        #[arg(long)]
        repo: Option<String>,
        /// What the packet is for. Echoed, never derived.
        #[arg(long)]
        task: Option<String>,
        #[arg(long, default_value_t = 100_000)]
        budget: usize,
    },
    /// Repo identity (R04/A92). Derived and additive — never rewrites a
    /// project id, never merges projects.
    #[command(display_order = 16)]
    Identity {
        #[command(subcommand)]
        cmd: IdentityCommand,
    },
    /// Verify or import the operator-supplied local ONNX annotation model.
    ///
    /// These commands inspect local files only. They never download, load, or
    /// execute model weights, and they are available in both build profiles.
    #[command(display_order = 20)]
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Propose and human-adjudicate receipt-backed navigation associations.
    ///
    /// This never merges project, session, event, chain, node, or source
    /// identity. A proposal is preview-only until `--apply`; only a separate
    /// human `associate` decision enters the accepted navigation projection.
    #[command(display_order = 21)]
    Entity {
        #[command(subcommand)]
        command: EntityCommand,
    },
    /// Score claims against evidence and persist the annotation layer.
    ///
    /// Loads the NLI model; holds no derived graph while it runs (§5a).
    #[command(display_order = 20)]
    Annotate {
        #[arg(long, allow_hyphen_values = true)]
        project: String,
        /// Model directory. Defaults to config, then ~/.thruline/models/…
        #[arg(long)]
        model: Option<PathBuf>,
        /// Override the local Ollama classification model selected by RAM.
        #[arg(long)]
        language_model: Option<String>,
        /// Use the deterministic extractor even when Ollama is available.
        #[arg(long)]
        no_language: bool,
    },
    /// Run the synthetic eval corpus and gate on the result (M3.6).
    /// SHADOW DIAGNOSTICS (RB1): session statuses derived at read, over the
    /// whole ledger. Labeled hypotheses on the RB0 taxonomy — visible here and
    /// nowhere else until each class graduates by adjudication. No watcher:
    /// this computes from stored rows and your clock, then exits.
    #[command(display_order = 22)]
    ShadowSessions {
        /// One project. Defaults to every project.
        #[arg(long, allow_hyphen_values = true)]
        project: Option<String>,
        /// Print every session row, not just the class counts.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(display_order = 21)]
    Eval {
        /// Corpus directory holding the .jsonl transcripts and manifest.json.
        #[arg(long)]
        corpus: PathBuf,
        /// Override the entailment thresholds, for sensitivity runs.
        #[arg(long)]
        hi: Option<f32>,
        #[arg(long)]
        lo: Option<f32>,
    },
    /// Derive a project and write the §9 JSON contract.
    #[command(display_order = 14)]
    Export {
        #[arg(long, allow_hyphen_values = true, required_unless_present = "all")]
        project: Option<String>,
        /// Every project: the §9 summary index plus one fixture per project.
        /// `--out` then names a directory (default ui/public/fixtures).
        #[arg(long, conflicts_with = "project")]
        all: bool,
        /// Defaults to ui/public/fixtures/local.json (gitignored).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Include full-shadow `unsupported`/`dropped` nodes and receipts.
        /// Ordinary exports intentionally omit them (§5s).
        #[arg(long)]
        diagnostics: bool,
        /// Stamp the export with this UTC time instead of the moment it ran.
        ///
        /// Reaches exactly one field, `generated_at` — the only thing in the
        /// export that does not come from the record. Everything else is
        /// derived and is unaffected. Use it when two exports must be
        /// comparable byte for byte; leave it off and the export says when it
        /// was taken. Form: 2026-09-17T14:05:00Z.
        #[arg(long, value_name = "TIMESTAMP")]
        generated_at: Option<String>,
    },
    /// What needs me? The attention queue, recomputed on read.
    ///
    /// Read-only. Recomputed from accepted records on every invocation — there
    /// is no watcher, poller, daemon, or network service behind this (§5u).
    /// `capture status` and `doctor` stay integration-scoped; this is the
    /// project-attention command.
    #[command(display_order = 2)]
    Now {
        /// One project. Defaults to every project in the ledger.
        #[arg(long, allow_hyphen_values = true)]
        project: Option<String>,
        /// Also show items already dismissed, with their dismissal records.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Open ThruLine in your browser. One command, no setup.
    ///
    /// Serves the app on the IPv4 loopback for as long as this command runs and
    /// not one moment longer: Ctrl+C stops it and nothing survives. The app is
    /// embedded in this binary, so no npm, dev server, or source tree is needed
    /// and it works from any directory. Your projects are read from
    /// `~/.thruline/export`, refreshed on demand rather than on every launch —
    /// see `--refresh`, and see the Refresh action in the app.
    #[command(display_order = 0)]
    Ui(UiArgs),
    /// Why should I believe it? One item, disclosed in three levels.
    ///
    /// Read-only. Answers for dismissed items and for ids that no longer
    /// derive: history is never rewritten to make an answer unavailable.
    #[command(display_order = 3)]
    Why {
        /// An item id, or an unambiguous prefix of one.
        item: String,
        #[arg(long)]
        json: bool,
    },
    /// Dismiss one attention item. Appends an acknowledgment; mutates nothing.
    #[command(display_order = 5)]
    Dismiss {
        /// An item id, or an unambiguous prefix of one.
        item: String,
    },
    /// M5 local Assured Capture operations. No subcommand changes truth.
    #[command(display_order = 17)]
    Capture {
        #[command(subcommand)]
        command: CaptureCommand,
    },
    /// Lifecycle sensor (C1.2): plan/install/uninstall the Claude Code hook
    /// set; `append` is the entrypoint the vendor's own process invokes.
    #[command(display_order = 18)]
    Sensor {
        #[command(subcommand)]
        action: SensorAction,
    },
    /// Record YOUR judgment on the shadow-class specimens awaiting a human.
    /// ThruLine renders the evidence and stores what you decide — it never
    /// decides for you, and it never fills in a likely answer
    Adjudicate {
        /// Render every pending specimen and write nothing at all
        #[arg(long)]
        dry_run: bool,
        /// Print progress per class and exit
        #[arg(long)]
        status: bool,
        /// Who is judging. Recorded with every verdict; defaults to $USER and
        /// is always printed before anything is written
        #[arg(long)]
        operator: Option<String>,
    },

    /// SHADOW DIAGNOSTICS — Guardian phase 1 candidates, with the classes it
    /// cannot build and why. Nothing here reaches a default surface.
    #[command(display_order = 25)]
    Guardian,
    /// What this machine's own records hold — sixty seconds, zero config,
    /// nothing leaves this device.
    #[command(display_order = 6)]
    Autopsy,
    /// Install the BUILD-SPEC §4.1 capture hook pack for a harness.
    ///
    /// REFUSES rather than appending when ThruLine hooks are already present:
    /// two recorders writing two records of one session is worse than none,
    /// because nothing afterwards says which record a claim was checked
    /// against. `--undo` removes exactly what this wrote and leaves other packs
    /// alone.
    HookInstall {
        #[arg(long, default_value = "claude-code")]
        harness: String,
        #[arg(long, default_value = "user")]
        scope: String,
        /// Settings file. Defaults to the scope's conventional path.
        #[arg(long)]
        settings: Option<PathBuf>,
        /// Print the enterprise managed-settings snippet and write nothing.
        #[arg(long)]
        managed_settings_snippet: bool,
        /// Also install §4.1's opt-in Stop gate, which blocks a stop whose
        /// completion claim has no proof in the record.
        #[arg(long)]
        gate: bool,
        #[arg(long)]
        undo: bool,
        #[arg(long)]
        json: bool,
    },
    /// Is the install intact, and does the record verify? (BUILD-SPEC §13.)
    ///
    /// Chain integrity is RECOMPUTED, never read from the cached head. Also
    /// reports the git-object footprint snapshotting leaves in your repository
    /// and how to reclaim it, whether capture is degraded by a `.gitattributes`
    /// clean filter, which hook packs are installed, spool and quarantine
    /// health, and whether `.thruline/redact.yml` loaded.
    ///
    /// Exits 1 when something it checked is not intact.
    Doctor {
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        spool: Option<PathBuf>,
        /// Repository to inspect. Defaults to the working directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Receive one harness hook delivery on stdin (BUILD-SPEC §4.1).
    ///
    /// Invoked by the harness, not by a human. Reads one JSON delivery, redacts
    /// it, appends one line to the spool, and **exits 0 having printed
    /// nothing** — whatever happened. A hook that errors, blocks or prints is a
    /// hook that breaks the user's agent.
    ///
    /// Deliberately does no database or git work: §4.1's budget is 50 ms p95,
    /// and `git add -A` plus opening a WAL database is not a promise that can be
    /// kept. Snapshots belong to the fold and the exec shim.
    Hook {
        /// Which harness is calling: `claude-code`, `codex` or `gemini`.
        harness: String,
        /// Spool file. Defaults to `$THRULINE_HOME/spool/<harness>.jsonl`.
        #[arg(long)]
        spool: Option<PathBuf>,
    },
    /// The §4.1 Stop gate: block a stop whose completion claim has no proof.
    ///
    /// Invoked by the harness, not by a human. Reads the `Stop` payload on
    /// stdin and prints the documented block decision when a claim is
    /// **`unsupported`** — and only then. An `unverifiable` claim never blocks:
    /// that would accuse the agent where ThruLine could not observe enough to
    /// decide, which is §6 inverted.
    ///
    /// **Always exits 0.** A 5 s watchdog allows the stop whatever the
    /// verification path is doing, because a gate that hangs is worse than one
    /// that abstains.
    Gate {
        /// Which hook is calling. Only `stop` does anything.
        #[arg(long, default_value = "stop")]
        hook: String,
        /// Blocks allowed per session, inside the harness's own cap of 8.
        #[arg(long, default_value_t = gate_cmd::DEFAULT_MAX_STOP_BLOCKS)]
        max_stop_blocks: u32,
        /// Watchdog budget in milliseconds. Honoured exactly, including
        /// values shorter than §4.1's 5 s.
        ///
        /// `THRULINE_GATE_BUDGET_MS` can only LENGTHEN the budget; this can
        /// shorten it. The difference is not an oversight: an environment
        /// variable is inherited by an unmodified hook, and an argument is
        /// authored into the hook's own command line. See
        /// `gate_cmd::budget_ms`.
        #[arg(long)]
        budget_ms: Option<u64>,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// What the agent claimed, and what the record proves (BUILD-SPEC §5, §6).
    ///
    /// Extracts claims and promises from the session's final message, then
    /// verifies each claim against captured evidence — exec events, snapshots,
    /// file changes and parsed test results. The prose says what was claimed;
    /// it never decides whether the claim holds.
    ///
    /// Exits 3 when a claim is `unsupported`, so CI can tell a refuted claim
    /// from a broken command.
    Claims {
        /// Session to report on. Defaults to the most recent.
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        last: bool,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        /// Write one JSON line per pipeline phase to stderr as it completes
        /// (`read_record`, `find_claims`, `match_proof`, `workspace_since`,
        /// `run_verifiers`, `write_up`). Stdout is unchanged.
        #[arg(long)]
        phases: bool,
    },
    /// What happened, why it stopped, what it forgot, and what it proved
    /// (BUILD-SPEC §10).
    ///
    /// Answers all six §10 questions from the ledger alone. Exits 3 when a
    /// claim is unsupported.
    Explain {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        last: bool,
        /// Print the whole timeline instead of the first lines.
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
        /// Markdown headings, for pasting into a PR.
        #[arg(long)]
        md: bool,
        /// One self-contained HTML file — no stylesheet, script, font or image
        /// is fetched, so it renders identically offline and tells nobody who
        /// opened it.
        #[arg(long)]
        html: bool,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Build a signed receipt for a session (BUILD-SPEC §8).
    ///
    /// An in-toto Statement in a DSSE envelope, built from LEDGER ROWS ONLY so
    /// it is reproducible from the record alone, signed with a local key.
    /// Sigstore keyless is not built (§8.1's own hedge; M2.4 is a spike).
    Receipt(ReceiptArgs),
    /// Verify a receipt, offline (BUILD-SPEC §8.1, §14).
    ///
    /// Exits 0 when every check passed, 1 when one failed, and **2 when nothing
    /// failed and something could not be checked here** — a receipt verified as
    /// far as this machine can is not a receipt verified.
    VerifyReceipt {
        receipt: PathBuf,
        /// Public key to verify against. Defaults to
        /// `$THRULINE_HOME/signing.pub`.
        #[arg(long)]
        key: Option<PathBuf>,
        /// Accepted and asserted: nothing in this path opens a socket.
        #[arg(long)]
        offline: bool,
        /// How to render the one conclusion: `text` (default), `json`, `md`
        /// (a PR comment table), `check-run` (a GitHub check-run payload) or
        /// `badge` (the §9 SVG).
        ///
        /// Every rendering carries the conclusion the verifier computed. A
        /// consumer that re-derives a verdict from the checks array reads a
        /// partially-verified receipt as verified.
        #[arg(long, default_value = "text")]
        format: String,
        /// The commit a check run is about. Defaults to `$GITHUB_SHA`.
        #[arg(long)]
        head_sha: Option<String>,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Re-run the session's checks in a clean container at the final tree
    /// (BUILD-SPEC §7).
    ///
    /// Materialises the final tree by writing blobs through `git cat-file` —
    /// never `git checkout`, which applies smudge filters and so does not
    /// produce a function of the tree hash. The container has no network
    /// unless `.thruline/reexec.yml` asks for one.
    ///
    /// **Refuses** when the session's capture is degraded: re-running honest
    /// code to confirm a claim about cheating code is the laundering, so the
    /// gate is a refusal rather than a footnote.
    ///
    /// Exits 0 when every re-run agreed with the agent's, 3 when the gate
    /// refused or a re-run disagreed.
    Reexec {
        #[arg(long)]
        session: Option<String>,
        /// `claimed` re-runs exactly what the agent ran; `full` runs the
        /// repository's canonical test command.
        #[arg(long, default_value = "claimed")]
        mode: String,
        /// Only `docker` is built. §7 puts the cloud backend behind a feature
        /// flag this build does not ship.
        #[arg(long, default_value = "docker")]
        backend: String,
        /// Per-command wall clock budget.
        #[arg(long, default_value_t = 900)]
        timeout_secs: u64,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// What the agent said it would do later, and whether anything did it
    /// (BUILD-SPEC §3.5, §11, §14).
    ///
    /// A promise nothing later touched stays open. That is a report of what was
    /// said and not done — not proof it was abandoned.
    Promises {
        #[arg(long)]
        open: bool,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// The BUILD-SPEC §4.4 PATH shim: record what test and build commands
    /// actually ran.
    ///
    /// `install` writes wrappers for pytest, cargo, go, npm, pnpm, yarn, npx,
    /// make, mvn, gradle and dotnet into a directory you put first on PATH.
    /// Each wrapper records argv, exit code, output digests and a git tree
    /// snapshot before and after, then runs the real program — transparently,
    /// including its exit code and its signal death.
    Shim {
        #[command(subcommand)]
        action: ShimAction,
    },
    /// Run an agent under a pseudoterminal, recording what it executes.
    ///
    /// §4.4's harness-agnostic path: no hooks, no vendor contract, no settings
    /// file. The PTY is not cosmetic — an agent that believes it is not on a
    /// terminal behaves differently, and recording a different agent from the
    /// one the user runs is not a recording.
    ///
    /// Exits with the child's own status, unchanged.
    Run(RunArgs),
    /// The §10 flight recorder: timeline, detail, claims and promises, in a
    /// terminal.
    ///
    /// Read-only. There is no key that writes anything.
    Tui {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        last: bool,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Import Codex rollout files into the §11 execution store (§4.2).
    ///
    /// Recovers the two record types §3.1 names and Codex's own log holds:
    /// `patch_apply_end` becomes `fs_change`, `context_compacted` becomes a
    /// compaction boundary.
    ///
    /// **Every imported session is `capture_degraded`.** A rollout is the
    /// harness's own log, not execution evidence (§0.2), so no claim in one can
    /// reach `supported`. This is history for `explain`, not evidence.
    ImportCodex(ImportCodexArgs),
    /// §14's sync verbs. **Refuses**: §18's answer is that the hosted sync
    /// server does not ship in v1 and the client stays a feature-flagged
    /// reference implementation, so this says so rather than being absent.
    Sync(SyncArgs),
    /// §9's policy gate: `--init` writes the documented example, otherwise the
    /// last session is evaluated against `.thruline/policy.yml`.
    ///
    /// Exits **3** when policy fails the check and **0** when it passes or is
    /// neutral — `unverifiable` is neutral by §9, which is §6's third state
    /// surviving to the merge button rather than being rounded away.
    /// The sessions in the execution store, most recently active first.
    ///
    /// `explain` and `claims` open ONE session, and given no `--session` that
    /// one is the lexicographically last id in the store — which on a real
    /// corpus is an arbitrary session with nothing in it. This is how a person
    /// finds the id to pass them. It changes no default.
    Sessions(SessionsArgs),
    /// Work you are being asked to accept, and the sessions linked to it.
    Change(ChangeArgs),
    Policy(PolicyArgs),
    /// §12's benchmark harness and the Overclaim Index.
    ///
    /// Exits **3** when no labelled corpus is downloaded, naming each §12
    /// source and what a probe of it found. Nothing is generated: a corpus
    /// written by the party being measured reports on the fixtures.
    Bench(BenchArgs),
    /// The pilot's money ceiling — the only place the per-run spending cap is enforced.
    ///
    /// `scripts/pilot-run.sh` is what spends money, and it is a shell script,
    /// so the cap has to be reachable from a shell. **Exit 4 is a budget
    /// halt**, used by nothing else in this binary.
    ///
    ///   check  — is there room for one more task at its worst price?
    ///   charge — record what a task cost; refuses rather than crossing the cap
    ///   show   — the running total
    ///
    /// The spend is an append-only file, so it survives a process restart. It
    /// used to be a `pub` field on a `Default`-derived struct living inside one
    /// `tb run`, which made a per-run cap a per-invocation cap across ten
    /// amnesiac invocations.
    PilotBudget(PilotBudgetArgs),
    /// Fold the hook and exec spool into the §11 execution store.
    ///
    /// The hook writes one line and exits (§4.1's 50 ms budget); this is where
    /// the chain, the snapshots and the §3.3 checks are built. Idempotent: a
    /// second fold of the same spool writes nothing.
    ///
    /// Exits 1 when anything could not be recorded — a snapshot that failed, a
    /// line that could not be sealed, a check whose output could not be read.
    Fold {
        #[arg(long)]
        spool: Option<PathBuf>,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        /// Repository to snapshot. Defaults to the working directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// First bring the spool level with the sensor, then fold.
        ///
        /// `thruline init` installs the SENSOR pack, which records into
        /// `sensor_events` in the ledger rather than into the spool this
        /// command reads — so on a machine set up that way the execution store,
        /// and therefore `thruline ui`, sees nothing the sensor captured. This
        /// appends every sensor delivery the spool does not already carry, in
        /// the spool's own shape, and then folds as usual.
        ///
        /// Appends only, never rewrites: `fold` numbers each line by its
        /// position in the file. Safe to repeat — a delivery already in the
        /// spool is not emitted twice.
        #[arg(long)]
        from_sensor: bool,
        #[arg(long)]
        json: bool,
    },
    /// Import the pre-spec transcript ledger into the BUILD-SPEC §11 execution
    /// store at `~/.thruline/exec.db`.
    ///
    /// Reads the source READ-ONLY and streams it; the source is never modified
    /// and no journal or WAL file is created beside it. Re-running resumes
    /// rather than duplicating.
    ///
    /// Every imported session is stamped `capture_degraded`, because these rows
    /// record what the agent SAID, not what the machine did. Per BUILD-SPEC §13
    /// their claims can reach `unverifiable` but never `supported`.
    ///
    /// NOTE: this command IGNORES the global `--db`. That flag names the legacy
    /// transcript ledger; use `--source` for the input and `--exec-db` for the
    /// output. One flag meaning two different databases depending on the
    /// subcommand is how a receipt ends up pointing at the wrong chain.
    ImportLegacy(ImportLegacyArgs),
    /// Plan, capture and inspect evidence from the integrated verification tools.
    Integration {
        #[command(subcommand)]
        command: integration_cmd::Command,
    },
    /// Run YOUR command through ThruLine so its evidence binds to the exact
    /// workspace state: `thruline check -- cargo test --workspace`. Output
    /// and exit code pass through untouched; what actually happened is
    /// recorded with receipts. ThruLine itself still never runs anything —
    /// this executes only what you typed after `--`.
    #[command(display_order = 7)]
    Check {
        /// Run through `bash -o pipefail -c`: a failing stage anywhere in a
        /// pipeline fails the whole command, and that cannot be disabled.
        #[arg(long)]
        shell: bool,
        /// Suppress the wrapper's stderr summary — both streams stay
        /// byte-identical to the child's.
        #[arg(long)]
        quiet: bool,
        /// Declare an artifact this command produces; digested after it
        /// finishes. Repeatable.
        #[arg(long = "artifact")]
        artifacts: Vec<PathBuf>,
        /// This run examined the application listening on this port. Its
        /// owner, that program and the database it has open are recorded, so
        /// a result from another running copy cannot be counted for this one.
        #[arg(long = "app-port")]
        app_port: Option<u16>,
        /// A config or test file whose bytes are part of what was examined.
        /// Repeatable; digested together.
        #[arg(long = "config-file")]
        config_files: Vec<String>,
        /// You are running this check for this Change. The execution is
        /// recorded under it, and shows up beside what counts as done. Without
        /// it the run is still recorded and still readable with `thruline
        /// evidence` — it is attached to nothing.
        #[arg(long = "for-change", value_name = "CHANGE")]
        for_change: Option<String>,
        /// Who is running it. Used only when `--for-change` is given, because
        /// attaching work to a Change is a person's act and needs a name.
        /// Defaults to the account this command is running as.
        #[arg(long)]
        actor: Option<String>,
        /// What class of evidence this run is: `naturally_occurring`,
        /// `seeded`, `synthetic` or `independent`. Say `seeded` when the
        /// thing being demonstrated was set up on purpose, and every surface
        /// that shows what this run found says so beside it. Nothing is
        /// guessed: without this flag the record holds no class, and the
        /// screens say that rather than calling it ordinary.
        #[arg(long = "evidence-class", value_name = "CLASS")]
        evidence_class: Option<String>,
        /// The command, after `--`, exactly as you would run it yourself.
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// What command evidence is bound to this workspace, and whether it
    /// still applies — re-observed now, stated in the §9 vocabulary.
    /// Exits 0 whatever state it reports: STALE is an answer, not a failure.
    #[command(display_order = 8)]
    Evidence {
        /// Evaluate one binding by id.
        #[arg(long)]
        id: Option<String>,
        /// Every binding for this repository, not just the latest per scope.
        #[arg(long)]
        all: bool,
        /// Workspace root. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Stable JSON to stdout.
        #[arg(long)]
        json: bool,
    },
    /// A read-only capture of the workspace as it stands: lineage, the dirty
    /// tree with digests, environment names (never values), processes — and,
    /// in a git worktree, the protected byte layer (A208/§14): every tracked,
    /// staged, and in-scope untracked byte, encrypted at rest.
    #[command(display_order = 9)]
    Snapshot {
        /// Workspace root. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Verify a stored protected snapshot instead of taking a new one:
        /// every referenced blob and its manifest are re-hashed, and a
        /// SNAPSHOT_VERIFIED observation is appended on success.
        #[arg(long, value_name = "SNAPSHOT_ID", conflicts_with = "root")]
        verify: Option<String>,
    },
    /// Materialize a protected snapshot byte-exactly — content, modes,
    /// symlinks, deletions, untracked files, and the staged/worktree duality
    /// — into a separate safe directory. Never the active tree by default;
    /// restoring over a working tree is the one destructive edge and takes a
    /// double gate (--over-working-tree + --expect-tree + confirmation).
    #[command(display_order = 11)]
    Restore {
        /// The snapshot id printed by `thruline protect` / `thruline snapshot`.
        snapshot: String,
        /// Where to materialize. Defaults to a fresh directory under
        /// <db-dir>/restores/<snapshot-id>; must be new or empty, outside any
        /// repository. With --over-working-tree: the worktree to restore over
        /// (defaults to the snapshot's recorded root).
        #[arg(long)]
        to: Option<PathBuf>,
        /// Write over an existing working tree instead. Prints the exact
        /// files to be written and deleted before touching anything, and
        /// touches nothing beyond that preview.
        #[arg(long)]
        over_working_tree: bool,
        /// Compare-first acknowledgment: the target tree's CURRENT wsid1
        /// identity. Run once without it to be shown the value; the restore
        /// refuses whenever the tree no longer matches what you acknowledged.
        #[arg(long, requires = "over_working_tree")]
        expect_tree: Option<String>,
        /// Skip the interactive confirmation — the double gate's documented
        /// test seam. The --expect-tree acknowledgment still holds in full.
        #[arg(long, requires = "over_working_tree")]
        yes: bool,
    },
    /// Preserve this work: one read-only workspace snapshot (byte layer
    /// included, A208/§14) plus a Recovery Capsule that cites it. Use it
    /// before a risky move, and at the end of a working day — ending your
    /// day is also a handoff.
    #[command(display_order = 10)]
    Protect {
        /// One project id.
        #[arg(long)]
        project: String,
        /// Workspace root to snapshot. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Where to write the capsule. Defaults to ./thruline-capsule.json.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Prepare a fresh session to continue this work. Previews everything
    /// first and launches nothing — ThruLine prepares and prints; you launch.
    #[command(display_order = 12)]
    Rescue {
        /// One project id.
        #[arg(long)]
        project: String,
        /// Which agent the continuation is for. Only adapters the capability
        /// matrix proves are offered.
        #[arg(long, default_value = "claude")]
        agent: String,
        /// Where to stage the artifacts. Defaults to ./thruline-rescue.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Acknowledge the capsule's secret findings. Injection is staged only
        /// with this flag: the gate fails toward consent.
        #[arg(long)]
        acknowledge: bool,
        /// Internal: emit the SessionStart hook payload for a staged capsule.
        #[arg(long, hide = true)]
        emit_hook: Option<PathBuf>,
    },
    /// The legacy/local-only continuity comparison. Read-only, and never
    /// verified recovery because it observes no successor session.
    #[command(display_order = 13)]
    VerifyContinuity {
        #[arg(long)]
        project: String,
        /// The staged capsule to check against.
        #[arg(long)]
        capsule: PathBuf,
        /// Workspace root to compare. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Confirm the successor restated the objective. A confirmation, never
        /// a score of comprehension.
        #[arg(long)]
        objective_acknowledged: bool,
        /// Accept a failed handshake, with a reason. Records
        /// "resumed with warnings" — it never rewrites the verdict.
        #[arg(long)]
        override_reason: Option<String>,
    },
    /// Compile a Recovery Capsule — the smallest sufficient, receipt-carrying
    /// context for continuing this work in a fresh session. Read-only.
    #[command(display_order = 14)]
    Capsule {
        /// One project id.
        #[arg(long)]
        project: String,
        /// Also take a workspace snapshot at this root and cite it.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Write the machine-readable capsule here (JSON). Markdown always
        /// goes to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Name the individual claims of ungraduated classes. Default surfaces
        /// carry counts only (§5s-amendment).
        #[arg(long)]
        diagnostics: bool,
    },
}

#[derive(Subcommand)]
enum DigestAction {
    /// Compute the ended session's digest and write the per-project cache.
    Precompute {
        /// Which hook invoked this; only "session-end" computes.
        #[arg(long)]
        hook: String,
    },
}

#[derive(Subcommand)]
enum IdentityCommand {
    /// Derive repo identity for every project and record it. Re-runnable;
    /// abstains rather than guessing when a working directory is gone.
    Backfill {
        /// Transcripts sampled per project when reading the recorded `cwd`.
        #[arg(long, default_value_t = 8)]
        sample: usize,
        /// Derive and print, writing nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Show one repo, many working roots. A projection — nothing is merged.
    Groups,
}

#[derive(Subcommand)]
enum ModelCommand {
    /// Print the machine-readable closed-manifest status of a model directory.
    Status {
        /// Directory to verify. Defaults to ~/.thruline/models/nli-deberta-v3-xsmall.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Verify an operator-supplied directory and preview or apply a local copy.
    Import {
        /// Source directory containing exactly the three flat A18 files.
        #[arg(long, value_name = "DIR")]
        from: PathBuf,
        /// Destination directory. Defaults to ~/.thruline/models/nli-deberta-v3-xsmall.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Copy only after the preview is reviewed. Existing destinations are refused.
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum EntityProposer {
    Operator,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum EntityDecision {
    Associate,
    Reject,
    InsufficientEvidence,
}

#[derive(Subcommand)]
enum EntityCommand {
    /// Preview or append one content-addressed association proposal.
    Propose {
        #[arg(long)]
        project: String,
        #[arg(long)]
        first_event: String,
        #[arg(long, requires = "first_end")]
        first_start: Option<u32>,
        #[arg(long, requires = "first_start")]
        first_end: Option<u32>,
        #[arg(long)]
        second_event: String,
        #[arg(long, requires = "second_end")]
        second_start: Option<u32>,
        #[arg(long, requires = "second_start")]
        second_end: Option<u32>,
        /// Safe one-to-four-word navigation label.
        #[arg(long)]
        label: String,
        #[arg(long, value_enum)]
        proposer: EntityProposer,
        /// Required only for a model proposal; must be a classification
        /// annotation on one of the two subject events.
        #[arg(long)]
        annotation: Option<String>,
        /// Required only for an operator proposal.
        #[arg(long)]
        operator: Option<String>,
        /// Append after preview validation. Without this flag nothing is written.
        #[arg(long)]
        apply: bool,
    },
    /// Record the one human decision allowed for a proposal.
    Adjudicate {
        #[arg(long)]
        project: String,
        #[arg(long)]
        candidate: String,
        #[arg(long, value_enum)]
        decision: EntityDecision,
        #[arg(long)]
        operator: String,
        #[arg(long, default_value = "cli:entity")]
        route: String,
    },
    /// List immutable proposals and their optional human decisions.
    List {
        #[arg(long)]
        project: Option<String>,
        /// Show only human-accepted navigation associations.
        #[arg(long)]
        accepted: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SensorAction {
    /// Print exactly what install would write, and where. Writes nothing.
    Plan {
        /// Claude Code settings file. Defaults to ~/.claude/settings.json.
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    /// Merge the sensor hook set into Claude Code settings (`plan` first).
    Install {
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    /// Remove exactly the ThruLine-owned sensor entries. Recorded rows stay.
    Uninstall {
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    /// Hook entrypoint: read one payload from stdin, append one row, exit 0.
    /// Prints nothing to stdout — a sensor must never speak into the session.
    /// On PreCompact it also seals the full transcript into the mirror.
    Append {
        event: String,
        /// Mirror store root. Defaults to the private mirror root.
        #[arg(long)]
        mirror_root: Option<PathBuf>,
    },
    /// Read-only health check: registration, rows, chain, liveness, spills,
    /// and the speaker half (hook registration, digest cache, findings) —
    /// which is fail-open-silent, so only this check can say it is dead.
    Doctor {
        #[arg(long)]
        settings: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CaptureCommand {
    /// Show an exact read-only installation plan, or apply one by id.
    Install {
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        source_version: String,
        #[arg(long)]
        source_executable: PathBuf,
        /// Apply a previously reviewed plan. Without this flag, no config changes.
        #[arg(long)]
        apply: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Read-only operational status; never repairs or creates a key.
    Status {
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        source_version: String,
        #[arg(long)]
        source_executable: PathBuf,
        #[arg(long)]
        project_key: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Execute the isolated, source-native compatibility self-test.
    SelfTest {
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        source_version: String,
        #[arg(long)]
        source_executable: PathBuf,
    },
    /// Append an explicit project authorization or revocation.
    Authorize {
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        project_key: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        revoke: bool,
    },
    /// Disable new capture without deleting configuration or history.
    Disable {
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        reason: String,
    },
    /// Remove only exact ThruLine-owned hooks; historical evidence remains.
    Uninstall {
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
    },
    /// Source-native hook entrypoint. Reads one callback JSON object on stdin.
    Hook {
        #[arg(long, value_enum)]
        event: HookEvent,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        source_version: String,
        #[arg(long)]
        source_executable: PathBuf,
    },
    /// Opaque invocation wrapper used only by the rewritten Bash input.
    Exec {
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        invocation: String,
    },
    /// Close unfinished durable intents with zero authority.
    Recover {
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long, value_enum)]
        source: CaptureSource,
    },
    /// Seal and accept a later explicit state observation.
    Observe {
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        project_key: String,
        #[arg(long)]
        session_key: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long)]
        source_version: String,
        #[arg(long)]
        config_sha256: String,
        #[arg(long, default_value = "git status --porcelain")]
        profile_command: String,
    },
    /// Read-only diagnosis and exact remediation guidance.
    Doctor {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        source_version: String,
        #[arg(long)]
        source_executable: PathBuf,
        #[arg(long)]
        project_key: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Pair a completed native callback/wrapper receipt to an accepted event.
    Pair {
        #[arg(long)]
        capture_root: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long, value_enum)]
        source: CaptureSource,
        #[arg(long)]
        attempt: String,
        #[arg(long)]
        event_id: String,
        #[arg(long)]
        exit_code: Option<i32>,
        #[arg(long)]
        signal: Option<i32>,
    },
    /// Pause recording for a project, kept in the record. Recorded only:
    /// nothing honours a pause yet.
    Pause(hold_cmd::ProjectArg),
    /// Resume a paused project, kept in the record.
    Resume(hold_cmd::ProjectArg),
    /// Leave one path out of a project, kept in the record. Recorded only:
    /// nothing honours an exclusion yet.
    Exclude(hold_cmd::ExcludeArgs),
    /// Put an excluded path back, kept in the record.
    Include(hold_cmd::ExcludeArgs),
    /// Show a project's pause and excluded paths. (`status` is Assured
    /// Capture's.)
    Holds(hold_cmd::ProjectArg),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CaptureSource {
    Codex,
    ClaudeCode,
}

impl CaptureSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude_code",
        }
    }
}

/// `thruline pilot-budget`'s arguments, in their own type.
///
/// Not inline on `Command`: clap's derive builds every inline variant in one
/// function and that function's stack frame overflows a test thread — a limit
/// `tests/cli_surface_budget.rs` measures and enforces, and which this change
/// hit at variant 51.
#[derive(clap::Args)]
pub struct PilotBudgetArgs {
    /// The append-only spend ledger. Defaults to
    /// `~/.thruline-bench/pilot/spend.tsv`.
    #[arg(long)]
    ledger: Option<PathBuf>,
    /// The pinned model, for pricing. Required by `check` and `charge`.
    #[arg(long)]
    model: Option<String>,
    #[command(subcommand)]
    action: PilotBudgetAction,
}

#[derive(Subcommand)]
enum PilotBudgetAction {
    /// Refuse before the model is called if a worst-case task would not fit.
    Check,
    /// Record one task's measured cost. Token buckets are DISJOINT: `--input`
    /// excludes cache reads and cache writes. The reader that produced them did
    /// the vendor-specific subtraction (Codex reports input inclusive of cached
    /// input; Claude Code reports it exclusive).
    Charge {
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 0)]
        input: u64,
        #[arg(long, default_value_t = 0)]
        output: u64,
        #[arg(long = "cache-read", default_value_t = 0)]
        cache_read: u64,
        #[arg(long = "cache-write-5m", default_value_t = 0)]
        cache_write_5m: u64,
        #[arg(long = "cache-write-1h", default_value_t = 0)]
        cache_write_1h: u64,
        /// Where the numbers came from: `rollout-file`, `turn-completed` or
        /// `reconciled`. No default — an unstated provenance must not become
        /// the optimistic one.
        #[arg(long)]
        source: String,
    },
    /// Print the running total and what remains.
    Show,
}

#[derive(Debug, Subcommand)]
enum ShimAction {
    /// Write the shim directory and print the PATH line to use it.
    Install {
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Which harness this shim records alongside: `claude-code`, `codex`
        /// or `gemini`. One session's hook lines and its execs must share one
        /// spool file, or the fold's resume guard skips the second.
        #[arg(long)]
        harness: Option<String>,
        /// Spool the recorder appends to. Defaults to the hook's spool, so
        /// hook deliveries and recorded execs land in one ordered chain.
        #[arg(long)]
        spool: Option<PathBuf>,
        /// Attribute recorded execs to this session id.
        #[arg(long)]
        session: Option<String>,
        /// Fallback repository for snapshots. Defaults to the working directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// The binary the wrappers invoke. Defaults to this one.
        #[arg(long)]
        bin: Option<PathBuf>,
        /// Which candidates get a wrapper: `resolvable` (the default — only
        /// names a real program answers to), `all` (§4.4's literal list,
        /// resolvable or not), or a comma-separated subset of the candidates.
        ///
        /// Stating it matters where the answer must not depend on the host's
        /// PATH: a test fixture about wrapper mechanics needs the wrapper to
        /// exist, and under the default it silently would not on a machine
        /// without the tool.
        #[arg(long, value_name = "resolvable|all|<names>")]
        wrap: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Remove exactly the wrappers this build wrote.
    Uninstall {
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Is the shim installed, unmodified, and actually first on PATH?
    Status {
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Record and run one wrapped command. Invoked by a wrapper, not by a human.
    #[command(hide = true)]
    Exec {
        #[arg(long)]
        program: String,
        /// Everything after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum HookEvent {
    Pre,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum IngestSource {
    Auto,
    ClaudeCode,
    Codex,
}

/// Said once, so the two refusal sites cannot drift apart.
#[cfg(not(feature = "ort"))]
const NO_ORT: &str = "this build has no local entailment backend.\n\
     It was compiled with `--no-default-features`, which drops the ONNX scorer \
     and with it the only build-time network fetch in the tree.\n\
     Everything else works: ingest, derive, now, why, packet, export, verify, demo.\n\
     To score claims, rebuild with the feature: cargo install --path crates/cli";

#[cfg(test)]
mod ra2_tests {
    use super::{PACKET_BANNER, QUICKSTART};

    /// RA2's actual claim, and the only structural way to hold it: the banner
    /// belongs to the terminal. `thruline mcp` hands agents `packet.render()`
    /// straight from the core crate (crates/core/src/mcp.rs), so if this string
    /// ever appears in core, an agent is being told to go open a browser.
    #[test]
    fn the_banner_never_reaches_the_agent_surface() {
        let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src");
        let mut offenders = Vec::new();
        for entry in walkdir::WalkDir::new(&core)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.path().extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            if text.contains("AGENT-FACING ARTIFACT") {
                offenders.push(entry.path().display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "the CLI's agent-artifact banner leaked into the core crate, which is what \
             MCP serves to agents verbatim: {offenders:?}"
        );
        assert!(PACKET_BANNER.starts_with("# AGENT-FACING ARTIFACT"));
    }

    /// UX-DOCTRINE §5v. The quickstart is the most first-level surface the
    /// product has — it is what a person sees before they have chosen anything.
    #[test]
    fn the_quickstart_speaks_english() {
        for line in QUICKSTART.lines() {
            if let Some(term) = thruline_core::voice::banned_in(line) {
                panic!("quickstart line leaks engine vocabulary ({term:?}): {line}");
            }
        }
    }

    /// The RA2 directive said five lines; A169 grew it to six, deliberately —
    /// `autopsy` is the blueprint's sixty-second wedge, and a wedge the
    /// quickstart never names is a wedge nobody finds. The bound stays pinned
    /// so a seventh verb is a decision, not a drift.
    #[test]
    fn the_quickstart_is_six_commands() {
        let commands = QUICKSTART
            .lines()
            .filter(|l| l.trim_start().starts_with("thruline "))
            .count();
        assert_eq!(
            commands, 6,
            "the quickstart is specified at six lines (A169)"
        );
    }
}

#[cfg(test)]
mod decision_cli_tests {
    use super::{decide_cmd, Cli, Command};
    use clap::{CommandFactory, Parser};

    #[test]
    fn both_decision_spellings_parse_on_the_default_test_stack() {
        let person = Cli::try_parse_from(["thruline", "decide", "list", "--json"])
            .expect("person command parses");
        let Some(Command::Decide(person)) = person.command else {
            panic!("person command shape");
        };
        assert!(matches!(
            person.verb,
            decide_cmd::DecisionRootCliVerb::Person(decide_cmd::DecideCliVerb::List)
        ));

        let check = Cli::try_parse_from([
            "thruline",
            "decisions",
            "check",
            "--version",
            "efdc063e",
            "--all",
            "--json",
        ])
        .expect("version check parses");
        let Some(Command::Decide(check)) = check.command else {
            panic!("version check command shape");
        };
        assert!(matches!(
            check.verb,
            decide_cmd::DecisionRootCliVerb::Checks(decide_cmd::DecisionsCliVerb::Check { .. })
        ));

        let help = Cli::command().render_help().to_string();
        assert!(
            help.contains("decisions"),
            "plural command must appear in help"
        );
    }
}

#[cfg(test)]
mod model_cli_tests {
    use super::{Cli, Command, ModelCommand};
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn status_accepts_an_explicit_directory_without_other_inputs() {
        let cli = Cli::try_parse_from(["thruline", "model", "status", "--dir", "/tmp/model"])
            .expect("status shape");
        match cli.command {
            Some(Command::Model {
                command: ModelCommand::Status { dir },
            }) => assert_eq!(dir, Some(PathBuf::from("/tmp/model"))),
            _ => panic!("model status command was not parsed"),
        }
    }

    #[test]
    fn import_is_preview_by_default_and_apply_is_explicit() {
        let preview = Cli::try_parse_from([
            "thruline",
            "model",
            "import",
            "--from",
            "/tmp/source",
            "--dir",
            "/tmp/destination",
        ])
        .expect("preview shape");
        match preview.command {
            Some(Command::Model {
                command: ModelCommand::Import { from, dir, apply },
            }) => {
                assert_eq!(from, PathBuf::from("/tmp/source"));
                assert_eq!(dir, Some(PathBuf::from("/tmp/destination")));
                assert!(!apply, "a bare import must remain write-free");
            }
            _ => panic!("model import command was not parsed"),
        }

        let apply = Cli::try_parse_from([
            "thruline",
            "model",
            "import",
            "--from",
            "/tmp/source",
            "--apply",
        ])
        .expect("apply shape");
        match apply.command {
            Some(Command::Model {
                command: ModelCommand::Import { apply, .. },
            }) => assert!(apply),
            _ => panic!("model import --apply command was not parsed"),
        }
    }
}

#[cfg(test)]
mod entity_cli_tests {
    use super::{
        entity_provenance, entity_span, Cli, Command, EntityCommand, EntityDecision, EntityProposer,
    };
    use clap::Parser;

    #[test]
    fn proposal_is_preview_only_until_apply_and_carries_exact_subject_flags() {
        let preview = Cli::try_parse_from([
            "thruline",
            "entity",
            "propose",
            "--project",
            "project-a",
            "--first-event",
            "event-a",
            "--first-start",
            "1",
            "--first-end",
            "4",
            "--second-event",
            "event-b",
            "--label",
            "Parser family",
            "--proposer",
            "operator",
            "--operator",
            "reviewer",
        ])
        .expect("proposal shape");
        match preview.command {
            Some(Command::Entity {
                command:
                    EntityCommand::Propose {
                        project,
                        first_event,
                        first_start,
                        first_end,
                        second_event,
                        proposer,
                        operator,
                        annotation,
                        apply,
                        ..
                    },
            }) => {
                assert_eq!(project, "project-a");
                assert_eq!(first_event, "event-a");
                assert_eq!((first_start, first_end), (Some(1), Some(4)));
                assert_eq!(second_event, "event-b");
                assert_eq!(proposer, EntityProposer::Operator);
                assert_eq!(operator.as_deref(), Some("reviewer"));
                assert!(annotation.is_none());
                assert!(!apply, "a bare entity proposal must write nothing");
            }
            _ => panic!("entity proposal was not parsed"),
        }

        let apply = Cli::try_parse_from([
            "thruline",
            "entity",
            "propose",
            "--project",
            "project-a",
            "--first-event",
            "event-a",
            "--second-event",
            "event-b",
            "--label",
            "Parser family",
            "--proposer",
            "model",
            "--annotation",
            "annotation-a",
            "--apply",
        ])
        .expect("model apply shape");
        match apply.command {
            Some(Command::Entity {
                command:
                    EntityCommand::Propose {
                        proposer,
                        annotation,
                        apply,
                        ..
                    },
            }) => {
                assert_eq!(proposer, EntityProposer::Model);
                assert_eq!(annotation.as_deref(), Some("annotation-a"));
                assert!(apply);
            }
            _ => panic!("model proposal was not parsed"),
        }
    }

    #[test]
    fn partial_spans_are_rejected_by_the_cli() {
        let bad = Cli::try_parse_from([
            "thruline",
            "entity",
            "propose",
            "--project",
            "project-a",
            "--first-event",
            "event-a",
            "--first-start",
            "1",
            "--second-event",
            "event-b",
            "--label",
            "Parser family",
            "--proposer",
            "operator",
            "--operator",
            "reviewer",
        ]);
        assert!(bad.is_err());
    }

    #[test]
    fn adjudicate_and_list_have_explicit_human_and_scope_inputs() {
        let adjudicate = Cli::try_parse_from([
            "thruline",
            "entity",
            "adjudicate",
            "--project",
            "project-a",
            "--candidate",
            "entity_assoc_deadbeef",
            "--decision",
            "insufficient-evidence",
            "--operator",
            "reviewer",
            "--route",
            "cli:test",
        ])
        .expect("adjudication shape");
        match adjudicate.command {
            Some(Command::Entity {
                command:
                    EntityCommand::Adjudicate {
                        project,
                        decision,
                        operator,
                        route,
                        ..
                    },
            }) => {
                assert_eq!(project, "project-a");
                assert_eq!(decision, EntityDecision::InsufficientEvidence);
                assert_eq!(operator, "reviewer");
                assert_eq!(route, "cli:test");
            }
            _ => panic!("entity adjudication was not parsed"),
        }

        let list = Cli::try_parse_from([
            "thruline",
            "entity",
            "list",
            "--project",
            "project-a",
            "--accepted",
            "--json",
        ])
        .expect("list shape");
        assert!(matches!(
            list.command,
            Some(Command::Entity {
                command: EntityCommand::List {
                    accepted: true,
                    json: true,
                    ..
                }
            })
        ));
    }

    #[test]
    fn proposer_provenance_and_span_pairs_fail_closed_before_ledger_work() {
        let operator = Some("reviewer".to_string());
        let annotation = Some("annotation-a".to_string());
        assert!(entity_provenance(EntityProposer::Operator, &operator, &None).is_ok());
        assert!(entity_provenance(EntityProposer::Operator, &None, &None).is_err());
        assert!(
            entity_provenance(EntityProposer::Operator, &operator, &annotation).is_err(),
            "an operator must not pretend to carry a model annotation"
        );
        assert!(entity_provenance(EntityProposer::Model, &None, &annotation).is_ok());
        assert!(entity_provenance(EntityProposer::Model, &operator, &annotation).is_err());
        assert!(entity_span(Some(1), None, "first").is_err());
        assert!(entity_span(Some(2), Some(2), "first").is_err());
    }
}

#[cfg(test)]
mod continuity_cli_tests {
    use super::continuity_outcome_is_success;
    use thruline_core::continuity::Outcome;

    #[test]
    fn only_verified_is_a_success_exit() {
        assert!(continuity_outcome_is_success(Outcome::Verified));
        for outcome in [
            Outcome::Insufficient,
            Outcome::Failed,
            Outcome::ResumedWithWarnings,
        ] {
            assert!(
                !continuity_outcome_is_success(outcome),
                "{outcome:?} must exit nonzero"
            );
        }
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .init();

    match run() {
        Ok(code) => code,
        Err(e) => {
            // A193 — a pending schema upgrade is not an error in the "something
            // broke" sense, and it must not read like one. It is one sentence
            // naming the one command that fixes it, on stderr, with nothing
            // read and nothing written.
            //
            // The exit code is NON-zero deliberately, and this is an
            // interpretation of "exits cleanly" worth stating: "cleanly" is
            // about paying nothing and not crashing, not about claiming
            // success. A script that runs `thruline export --all` and gets a
            // silent exit 0 having exported nothing has been lied to, and
            // Rule 12 forbids hiding skipped work. Code 3 is distinct from
            // ExitCode::FAILURE so a caller can tell "upgrade first" apart from
            // "this went wrong". The sensor path never reaches here: it catches
            // its own open failure, spills the payload, and exits 0 in silence.
            if let Some(pending) = find_upgrade_pending(&e) {
                eprintln!("{pending}");
                return ExitCode::from(3);
            }
            if let Some(held) = find_store_held(&e) {
                eprintln!("{held}");
                return ExitCode::from(3);
            }
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Walk the error chain for the pending-upgrade refusal.
///
/// It is looked for by chain rather than by string, and it is looked for at all
/// because every `Ledger::open` in the CLI wraps its error in a `.context(...)`
/// — so the refusal arrives buried under "opening the ledger" and would
/// otherwise print as a two-line error about a failure that did not happen.
fn find_upgrade_pending(err: &anyhow::Error) -> Option<String> {
    err.chain().find_map(
        |cause| match cause.downcast_ref::<thruline_core::error::CoreError>() {
            Some(thruline_core::error::CoreError::UpgradePending { estimate, .. }) => Some(
                format!("schema upgrade pending — run `thruline upgrade` ({estimate})"),
            ),
            _ => None,
        },
    )
}

/// A removal hold is a transient refusal even when an ordinary command wraps
/// the ledger-open error with its own context. The fixed sentence deliberately
/// omits the store path and the removal id.
fn find_store_held(err: &anyhow::Error) -> Option<String> {
    err.chain().find_map(
        |cause| match cause.downcast_ref::<thruline_core::error::CoreError>() {
            Some(held @ thruline_core::error::CoreError::StoreHeld { .. }) => {
                Some(held.to_string())
            }
            _ => None,
        },
    )
}

fn run_model_command(command: &ModelCommand) -> Result<ExitCode> {
    match command {
        ModelCommand::Status { dir } => {
            let dir = match dir {
                Some(dir) => dir.clone(),
                None => thruline_core::nli::default_model_dir()
                    .context("resolving the default NLI model directory")?,
            };
            let status = thruline_core::model_supply::verify_model_dir(&dir)
                .context("verifying the local NLI model receipt")?;
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(if status.ready {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        ModelCommand::Import { from, dir, apply } => {
            let destination = match dir {
                Some(dir) => dir.clone(),
                None => thruline_core::nli::default_model_dir()
                    .context("resolving the default NLI model directory")?,
            };
            let report = thruline_core::model_supply::import_model_dir(from, &destination, *apply)
                .context("checking the local NLI model import")?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(if report.successful() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
    }
}

fn entity_span(
    start: Option<u32>,
    end: Option<u32>,
    side: &str,
) -> Result<Option<thruline_core::entity_association::ExactSpan>> {
    match (start, end) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) => thruline_core::entity_association::ExactSpan::new(start, end)
            .with_context(|| format!("validating the {side} exact span"))
            .map(Some),
        _ => anyhow::bail!("{side} span requires both start and end flags"),
    }
}

fn entity_provenance(
    proposer: EntityProposer,
    operator: &Option<String>,
    annotation: &Option<String>,
) -> Result<thruline_core::entity_association::ProposalProvenance> {
    use thruline_core::entity_association::ProposalProvenance;
    match (proposer, operator.as_deref(), annotation.as_deref()) {
        (EntityProposer::Operator, Some(operator), None) => Ok(ProposalProvenance::Operator {
            operator: operator.to_string(),
        }),
        (EntityProposer::Model, None, Some(annotation_id)) => Ok(ProposalProvenance::Model {
            annotation_id: annotation_id.to_string(),
        }),
        (EntityProposer::Operator, None, _) => {
            anyhow::bail!("operator proposal requires --operator and forbids --annotation")
        }
        (EntityProposer::Operator, Some(_), Some(_)) => {
            anyhow::bail!("operator proposal cannot pretend to carry a model annotation")
        }
        (EntityProposer::Model, _, None) => {
            anyhow::bail!("model proposal requires --annotation and forbids --operator")
        }
        (EntityProposer::Model, Some(_), Some(_)) => {
            anyhow::bail!("model proposal provenance is the annotation, not --operator")
        }
    }
}

fn unix_time_ms() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock precedes the Unix epoch")?;
    i64::try_from(elapsed.as_millis()).context("current timestamp does not fit in the ledger")
}

fn run_entity_command(command: &EntityCommand, db: &Path) -> Result<ExitCode> {
    use thruline_core::entity_association::{
        self, AssociationDecision, AssociationSubject, ProposalInput,
    };

    let ledger = Ledger::open(db).context("opening the ledger")?;
    match command {
        EntityCommand::Propose {
            project,
            first_event,
            first_start,
            first_end,
            second_event,
            second_start,
            second_end,
            label,
            proposer,
            annotation,
            operator,
            apply,
        } => {
            let input = ProposalInput {
                project_id: project.clone(),
                first: AssociationSubject::new(
                    first_event,
                    entity_span(*first_start, *first_end, "first")?,
                )
                .context("validating the first receipt subject")?,
                second: AssociationSubject::new(
                    second_event,
                    entity_span(*second_start, *second_end, "second")?,
                )
                .context("validating the second receipt subject")?,
                label: label.clone(),
                provenance: entity_provenance(*proposer, operator, annotation)?,
                proposed_at_unix_ms: unix_time_ms()?,
            };
            let (preview, appended) = entity_association::propose(&ledger, input, *apply)
                .context("preparing the entity-association proposal")?;
            if !apply {
                println!("PREVIEW — no association proposal was written.");
                println!("Re-run with --apply to append this exact content-addressed candidate.");
                println!("{}", serde_json::to_string_pretty(&preview)?);
                return Ok(ExitCode::SUCCESS);
            }
            let stored = entity_association::proposal_for(&ledger, &preview.candidate_id)?
                .context("applied entity-association proposal is not readable")?;
            println!(
                "{}",
                if appended {
                    "APPENDED — proposal recorded; no navigation association exists until a human adjudicates it."
                } else {
                    "UNCHANGED — the identical content-addressed proposal was already recorded."
                }
            );
            println!("{}", serde_json::to_string_pretty(&stored)?);
            Ok(ExitCode::SUCCESS)
        }
        EntityCommand::Adjudicate {
            project,
            candidate,
            decision,
            operator,
            route,
        } => {
            let proposal =
                entity_association::proposal_for(&ledger, candidate)?.with_context(|| {
                    format!("entity-association candidate does not exist: {candidate}")
                })?;
            anyhow::ensure!(
                proposal.project_id == *project,
                "candidate belongs to a different project; no decision was recorded"
            );
            let decision = match decision {
                EntityDecision::Associate => AssociationDecision::Associate,
                EntityDecision::Reject => AssociationDecision::Reject,
                EntityDecision::InsufficientEvidence => AssociationDecision::InsufficientEvidence,
            };
            let recorded = entity_association::adjudicate(
                &ledger,
                candidate,
                decision,
                operator,
                route,
                unix_time_ms()?,
            )
            .context("recording the one-shot entity-association decision")?;
            println!(
                "RECORDED — {}. Only associate enters the accepted navigation projection.",
                recorded.decision.as_str()
            );
            println!("{}", serde_json::to_string_pretty(&recorded)?);
            Ok(ExitCode::SUCCESS)
        }
        EntityCommand::List {
            project,
            accepted,
            json,
        } => {
            let records = if *accepted {
                entity_association::accepted(&ledger, project.as_deref())?
            } else {
                entity_association::list(&ledger, project.as_deref())?
            };
            if *json {
                json_out::print(&json_out::ENTITY_LIST, serde_json::to_value(&records)?)?;
                return Ok(ExitCode::SUCCESS);
            }
            println!(
                "{} entity association record(s){} — navigation only; no identity was merged.",
                records.len(),
                if *accepted { " accepted" } else { "" }
            );
            for record in records {
                let status = record
                    .decision
                    .as_ref()
                    .map(|decision| decision.decision.as_str())
                    .unwrap_or("awaiting_human_decision");
                println!(
                    "{} · {} · {} · {}",
                    record.proposal.candidate_id,
                    record.proposal.project_id,
                    record.proposal.label,
                    status
                );
                println!(
                    "  receipt {} ↔ {}",
                    record.proposal.first.event_id, record.proposal.second.event_id
                );
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[cfg(feature = "ort")]
fn ensure_verified_nli_model(dir: &Path) -> Result<()> {
    let status = thruline_core::model_supply::verify_model_dir(dir)
        .context("verifying the local NLI model receipt")?;
    anyhow::ensure!(
        status.ready,
        "NLI model receipt at {} is {} — run `thruline model status --dir {}`; preview an \
         operator-supplied copy with `thruline model import --from <DIR> --dir {}` before \
         adding `--apply`",
        dir.display(),
        status.state.as_str(),
        dir.display(),
        dir.display()
    );
    Ok(())
}

/// Where the pilot's append-only spend ledger lives by default.
///
/// Beside the pilot's own working tree (`$THRULINE_PILOT_SP`, the same variable
/// `scripts/pilot-run.sh` reads), so the runner and this command cannot end up
/// counting two different piles of money.
fn default_pilot_spend_ledger() -> Result<PathBuf> {
    let sp = match std::env::var_os("THRULINE_PILOT_SP") {
        Some(v) => PathBuf::from(v),
        None => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("HOME is not set; pass --ledger"))?;
            PathBuf::from(home).join(".thruline-bench")
        }
    };
    Ok(sp.join("pilot").join("spend.tsv"))
}

/// The `--json` contract, printed. One implementation, two call sites.
fn print_json_shapes() -> Result<ExitCode> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json_out::registry_document()?)?
    );
    Ok(ExitCode::SUCCESS)
}

fn run() -> Result<ExitCode> {
    let Cli { db, command } = Cli::parse();
    let Some(command) = command else {
        println!("{QUICKSTART}");
        return Ok(ExitCode::SUCCESS);
    };

    // Model supply is deliberately ledger-independent. In particular, an
    // explicit `--dir` must not need HOME merely because the unrelated default
    // ledger path normally does.
    if let Command::Model { command } = &command {
        return run_model_command(command);
    }

    // The `--json` contract is ledger-independent by construction: it is what
    // a consumer reads *before* it has a ledger, and four teams pin against it.
    if matches!(&command, Command::JsonShapes) {
        return print_json_shapes();
    }

    // T-DIST-4: the licence verbs never open the ledger, so they are answered
    // before its path is resolved and can never be refused by the A193 gate.
    if let Command::Licence(a) = command {
        return licence_cmd::run(a);
    }

    // The money ceiling is ledger-independent for the same reason model supply
    // is: it must not need HOME merely because an unrelated default path does,
    // and — more to the point — it must not be refused by the A193 schema gate.
    // A pending migration is a reason not to *ingest*; it is not a reason to
    // stop counting money that has already been spent.
    if let Command::PilotBudget(PilotBudgetArgs {
        ledger,
        model,
        action,
    }) = command
    {
        let ledger = match ledger {
            Some(p) => p,
            None => default_pilot_spend_ledger()?,
        };
        let action = match action {
            PilotBudgetAction::Check => pilot_cmd::Action::Check,
            PilotBudgetAction::Show => pilot_cmd::Action::Show,
            PilotBudgetAction::Charge {
                task,
                input,
                output,
                cache_read,
                cache_write_5m,
                cache_write_1h,
                source,
            } => pilot_cmd::Action::Charge {
                task,
                input,
                output,
                cache_read,
                cache_write_5m,
                cache_write_1h,
                source,
            },
        };
        return pilot_cmd::run(pilot_cmd::Args {
            ledger,
            model,
            action,
        });
    }

    // Whether the ledger was NAMED, kept before the default is filled in:
    // `init` pins a ledger into the hooks it installs only when a person named
    // one. Baking the default path in would pin this machine's HOME into a hook.
    let db_named = db.is_some();
    let db = match db {
        Some(p) => p,
        None => default_db_path().context("resolving the default ledger path")?,
    };

    match command {
        Command::Decide(args) => decide_cmd::run_root_cli(db, *args),
        // Answered above, before the ledger path is resolved. Kept as a real
        // arm rather than `unreachable!()` so that moving the early return
        // degrades to a slower correct answer instead of a panic.
        Command::JsonShapes => print_json_shapes(),
        Command::Ingest {
            paths,
            source,
            mirror_root,
            apply,
        } => {
            let mut ledger = Ledger::open(&db).context("opening the ledger")?;
            let mirror_root = match mirror_root {
                Some(path) => path,
                None => default_mirror_root().context("resolving the private mirror root")?,
            };
            let (claude_paths, codex_paths) = select_ingest_paths(source, &paths)?;
            // C34 pause gate (6 of 6): recordings under a pause in force, or
            // whose session started inside one, are left out — one printed line
            // per paused folder, and the counts again in the report below.
            // With no pause in the record this returns the paths it was given,
            // opens no recording and prints nothing.
            let (claude_paths, codex_paths, paused) =
                gate_ingest_paths(&ledger, &claude_paths, &codex_paths)?;
            for line in paused.lines() {
                println!("{line}");
            }
            if claude_paths.is_empty() && codex_paths.is_empty() && paused.total() > 0 {
                println!("{}", paused_ingest_summary(&paused, true));
                return Ok(ExitCode::SUCCESS);
            }
            // A131(a,c,d) — preview by default, refuse rather than fill.
            if !apply {
                let code = ingest_preview(&claude_paths, &codex_paths, &mirror_root);
                if paused.total() > 0 {
                    println!("{}", paused_ingest_summary(&paused, false));
                }
                return code;
            }
            if let Some(refusal) = ingest_budget_refusal(&claude_paths, &codex_paths, &mirror_root)?
            {
                eprintln!("{refusal}");
                return Ok(ExitCode::FAILURE);
            }
            let mut reports = Vec::new();
            if !claude_paths.is_empty() {
                reports.push(
                    ingest(&mut ledger, &claude_paths, &mirror_root)
                        .context("ingesting Claude Code transcripts")?,
                );
            }
            if !codex_paths.is_empty() {
                reports.push(
                    ingest_codex(
                        &mut ledger,
                        &codex_paths,
                        &CodexIngestOptions {
                            mirror_root,
                            owner_key: None,
                        },
                    )
                    .context("ingesting Codex transcripts")?,
                );
            }
            anyhow::ensure!(!reports.is_empty(), "no transcript roots were available");
            for report in &reports {
                print_ingest(&db, report);
            }
            if paused.total() > 0 {
                println!("{}", paused_ingest_summary(&paused, false));
            }
            // A94b: keep projections Current without a manual rebuild. An
            // ingest moves the chain position, which leaves every touched
            // project's projection BehindRecord — and a BehindRecord projection
            // makes every read surface fall back to derivation and the
            // attention queue report "not read". Refreshing here is what makes
            // "no manual rebuilds" true rather than aspirational.
            let cfg = thruline_core::config::Config::load_default().unwrap_or_default();
            let mut refreshed = 0usize;
            for pid in ledger.project_ids().context("listing projects")? {
                if thruline_core::attention::projection_state(&ledger, &pid)
                    .context("reading projection state")?
                    != thruline_core::attention::ProjectionState::Current
                {
                    thruline_core::derive::rebuild(&ledger, &pid, &cfg)
                        .with_context(|| format!("refreshing the projection for {pid}"))?;
                    refreshed += 1;
                }
            }
            if refreshed > 0 {
                println!("projections    {refreshed} refreshed to the current chain position");
            }
            // A126 — the binding rate, surfaced where it becomes true. An MCP
            // call is recorded when it is served; the transcript event it became
            // only arrives here, at ingest. This is the first moment both exist,
            // so it is the honest place to report the health metric. Silent when
            // there are no calls at all — a rate over an empty set is undefined,
            // not zero, and a "0%" line would be the shape §4 bans.
            {
                let ledger = Ledger::open(&db).context("reopening to bind MCP calls")?;
                let report = thruline_core::mcp::bind::bind_calls(&ledger)
                    .context("binding MCP calls to their events")?;
                if report.calls > 0 {
                    println!("mcp calls      {}", report.line());
                }
            }
            // Integrity alarms and rejected candidates are never silently OK.
            Ok(
                if reports.iter().all(|report| {
                    report.integrity_alarms.is_empty() && report.rejected_candidates == 0
                }) {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                },
            )
        }
        Command::Rebuild { project, all } => {
            let cfg = thruline_core::config::Config::load_default()
                .context("loading ~/.thruline/config.toml")?;
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            if all {
                // L9.6 — persisted chain positions are derived state, so a
                // rebuild owns them too. Fails loud rather than warning: a head
                // that disagrees with its own fold is a corrupted read surface.
                let heads = ledger
                    .rebuild_heads()
                    .context("rebuilding chain positions")?;
                if !heads.mismatches.is_empty() {
                    anyhow::bail!(
                        "persisted head disagrees with the fold over events for: {}",
                        heads.mismatches.join(", ")
                    );
                }
                println!("{} project head(s) rebuilt and verified", heads.projects);
                let ids = ledger.project_ids().context("listing projects")?;
                let total = ids.len();
                let mut nodes = 0usize;
                for (i, pid) in ids.iter().enumerate() {
                    let g = thruline_core::derive::rebuild(&ledger, pid, &cfg)
                        .with_context(|| format!("deriving {pid}"))?;
                    nodes += g.nodes.len();
                    println!("[{:>3}/{total}] {:>6} nodes  {pid}", i + 1, g.nodes.len());
                }
                println!("\n{total} project(s) rebuilt · {nodes} node(s) persisted");
                return Ok(ExitCode::SUCCESS);
            }
            let Some(project) = project else {
                anyhow::bail!("give --project <id> or --all");
            };
            let g = thruline_core::derive::rebuild(&ledger, &project, &cfg).context("deriving")?;
            println!("project        {}", g.project_id);
            println!("sessions       {}", g.sessions.len());
            println!(
                "events         {} raw · {} canonical · {} duplicate ({:.1}%)",
                g.stats.raw_events,
                g.stats.canonical_events,
                g.stats.duplicate_events,
                100.0 * g.stats.duplicate_events as f64 / g.stats.raw_events.max(1) as f64
            );
            println!(
                "clusters       {} ({} collapsed)",
                g.stats.action_clusters, g.stats.collapsed_clusters
            );
            println!("statements     {}", g.stats.statement_nodes);
            println!("nodes          {}", g.nodes.len());
            for (state, n) in &g.stats.state_counts {
                println!(
                    "                 {state:<14} {n:>7}  ({:.1}%)",
                    100.0 * *n as f64 / g.nodes.len().max(1) as f64
                );
            }
            println!("edges          {}", g.edges.len());
            println!("  abandoned heads {}", g.stats.abandoned_heads);
            println!("  extra-parent    {}", g.stats.extra_parent_edges);
            println!("fingerprint    {}", thruline_core::derive::fingerprint(&g));
            Ok(ExitCode::SUCCESS)
        }
        Command::Model { command } => run_model_command(&command),
        Command::Entity { command } => run_entity_command(&command, &db),
        Command::Annotate {
            project,
            model,
            language_model,
            no_language,
        } => {
            #[cfg(not(feature = "ort"))]
            {
                let _ = (&project, &model, &language_model, &no_language);
                anyhow::bail!(NO_ORT);
            }
            #[cfg(feature = "ort")]
            {
                let cfg = thruline_core::config::Config::load_default()
                    .context("loading ~/.thruline/config.toml")?;
                cfg.validate().context("validating config")?;
                let dir = model.unwrap_or_else(|| cfg.model.dir.clone());
                ensure_verified_nli_model(&dir)?;
                let ledger = Ledger::open(&db).context("opening the ledger")?;

                // §5t runs before the entailment model is loaded. Its API contains
                // sentence language only; after this block the provider is
                // dropped and verdict scoring cannot consult it.
                let extraction = if no_language || !cfg.language.enabled {
                    println!(
                        "language       unavailable/disabled — deterministic extractor fallback"
                    );
                    thruline_core::annotate::extract_heuristic_claims(&ledger, &project)
                        .context("extracting deterministic claims")?
                } else {
                    let options = thruline_core::language::OllamaOptions {
                        base_url: cfg.language.base_url.clone(),
                        model_override: language_model.or_else(|| cfg.language.model.clone()),
                        request_timeout: std::time::Duration::from_secs(
                            cfg.language.request_timeout_secs,
                        ),
                        keep_alive: cfg.language.keep_alive.clone(),
                        ..thruline_core::language::OllamaOptions::default()
                    };
                    let provider = thruline_core::language::OllamaAnnotator::new(options)
                        .context("configuring the local language tier")?;
                    let selection = provider.model_selection().clone();
                    match provider.probe() {
                        thruline_core::language::ServiceAvailability::Available {
                            selected_model_present: true,
                            selected_model_digest: Some(digest),
                            ..
                        } => {
                            let model_identity = thruline_core::language::immutable_model_identity(
                                &selection.model,
                                &digest,
                            )
                            .context("binding the local language model digest")?;
                            println!(
                                "language       {} (sha256:{}…, {:?} RAM tier, keep_alive {})",
                                selection.model,
                                &digest[..12],
                                selection.tier,
                                cfg.language.keep_alive
                            );
                            thruline_core::annotate::extract_language_claims(
                                &ledger,
                                &project,
                                &cfg,
                                &provider,
                                &model_identity,
                            )
                            .context("classifying sentence candidates")?
                        }
                        thruline_core::language::ServiceAvailability::Available {
                            models,
                            selected_model_present,
                            selected_model_digest,
                        } => {
                            let reason =
                                if selected_model_present || selected_model_digest.is_some() {
                                    "has no verified immutable digest"
                                } else {
                                    "is not installed"
                                };
                            println!(
                                "language       selected model {} {} ({} local model(s)); \
                                 deterministic extractor fallback",
                                selection.model,
                                reason,
                                models.len()
                            );
                            thruline_core::annotate::extract_heuristic_claims(&ledger, &project)
                                .context("extracting deterministic claims")?
                        }
                        thruline_core::language::ServiceAvailability::Unavailable(reason) => {
                            println!(
                            "language       Ollama unavailable ({reason}); deterministic extractor fallback"
                        );
                            thruline_core::annotate::extract_heuristic_claims(&ledger, &project)
                                .context("extracting deterministic claims")?
                        }
                    }
                };

                let mut nli = thruline_core::nli::OrtNli::open_with(&dir, cfg.model.max_tokens)
                    .context("loading the NLI model")?;
                let tag = nli.model_tag().to_string();
                let r = thruline_core::annotate::score_claims(
                    &ledger, &project, &cfg, &mut nli, &tag, extraction,
                )
                .context("annotating")?;
                let pairs_truncated = nli.pairs_truncated();
                // §5a: release the scorer before loading claim/link rows to compute
                // the §5r emitted count from the real verdict function.
                drop(nli);
                let unsupported = thruline_core::derive::unsupported_funnel(&ledger, &project)
                    .context("computing unsupported coverage funnel")?;
                println!("project        {project}");
                println!("scorer         {tag} (max_tokens {})", cfg.model.max_tokens);
                println!(
                    "thresholds     hi {} · lo {}",
                    cfg.thresholds.hi, cfg.thresholds.lo
                );
                println!(
                    "statements     {} assistant_text events scanned",
                    r.events_scanned
                );
                println!(
                    "claims         {} ({} completed-work · {} proposal) in {} ms",
                    r.claims_written, r.completed_claims, r.proposal_claims, r.extract_ms
                );
                if let Some(language_model) = &r.language_model {
                    println!(
                    "classification {language_model}: {} sentence(s) · {} accepted · {} demoted · \
                     {} uncertain in {:.1} s ({:.1}/sec)",
                    r.classification_candidates,
                    r.classification_accepted,
                    r.classification_demoted,
                    r.classification_uncertain,
                    r.classification_ms as f64 / 1000.0,
                    r.classification_candidates as f64
                        / (r.classification_ms as f64 / 1000.0).max(0.001),
                );
                    println!(
                    "  annotations   {} inserted · {} reused · {} provider attempt(s) · {} memory pause(s)",
                    r.classification_annotations_inserted,
                    r.classification_annotations_reused,
                    r.classification_attempts,
                    r.classification_memory_pauses,
                );
                }
                println!(
                    "  no candidates  {}   proposals still recent {}",
                    r.claims_without_candidates, r.proposals_still_recent
                );
                println!(
                    "dropped-funnel  proposals {} → action-defeated {} · unsearchable {} · \
                 still-recent {} → ELIGIBLE {} (output-annotated {})",
                    r.proposal_claims,
                    r.proposals_action_defeated,
                    r.proposals_unsearchable,
                    r.proposals_still_recent,
                    r.proposals_eligible,
                    r.proposals_output_annotated
                );
                println!(
                    "families       test {} · build {} · git {} · agent {} · none {}",
                    r.family_test, r.family_build, r.family_git, r.family_agent, r.family_none
                );
                println!(
                    "unsupported-funnel  claims {} → coverage-eligible {} → EMITTED {}",
                    unsupported.claims, unsupported.coverage_eligible, unsupported.emitted
                );
                if !r.coverage_missing_by_family.is_empty() {
                    println!("  coverage misses by family");
                    for (family, claims) in &r.coverage_missing_by_family {
                        println!("    {family:<20} {claims}");
                    }
                }
                println!(
                "pair-pipeline  {} prepared + scored + stored ({} backward · {} forward) in {:.1} s → {:.1}/sec",
                r.links_written,
                r.backward_pairs,
                r.forward_pairs,
                r.score_ms as f64 / 1000.0,
                r.links_written as f64 / (r.score_ms as f64 / 1000.0).max(0.001)
            );
                println!(
                    "  inference      {:.1} s → {:.1}/sec · prepare {:.1} s · link writes {:.1} s",
                    r.inference_us as f64 / 1_000_000.0,
                    r.links_written as f64 / (r.inference_us as f64 / 1_000_000.0).max(0.001),
                    r.pair_prepare_us as f64 / 1_000_000.0,
                    r.link_write_us as f64 / 1_000_000.0,
                );
                println!(
                    "  fidelity       {} parsed · {} fallback ({:.1}% fallback)",
                    r.parsed_premises,
                    r.fallback_premises,
                    100.0 * r.fallback_premises as f64
                        / (r.parsed_premises + r.fallback_premises).max(1) as f64
                );
                println!(
                    "  truncated      {} pair(s) hit the token cap",
                    pairs_truncated
                );
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Eval { corpus, hi, lo } => {
            #[cfg(not(feature = "ort"))]
            {
                let _ = (&corpus, &hi, &lo);
                anyhow::bail!(NO_ORT);
            }
            #[cfg(feature = "ort")]
            {
                let mut cfg = thruline_core::config::Config::load_default()
                    .context("loading ~/.thruline/config.toml")?;
                if let Some(h) = hi {
                    cfg.thresholds.hi = h;
                }
                if let Some(l) = lo {
                    cfg.thresholds.lo = l;
                }
                cfg.validate().context("validating config")?;
                ensure_verified_nli_model(&cfg.model.dir)?;
                // A throwaway ledger: the eval must never touch the user's.
                let tmp = tempfile::tempdir().context("scratch dir")?;
                let mut nli =
                    thruline_core::nli::OrtNli::open_with(&cfg.model.dir, cfg.model.max_tokens)
                        .context("loading the NLI model")?;
                let tag = nli.model_tag().to_string();
                let r = thruline_core::eval::run(
                    &corpus,
                    &tmp.path().join("eval.db"),
                    &cfg,
                    &mut nli,
                    &tag,
                )
                .context("running the eval")?;
                print_eval(&r, &tag);
                Ok(if r.gate_failures().is_empty() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                })
            }
        }
        Command::Export {
            project: _,
            all,
            out,
            diagnostics,
            generated_at,
        } if all => {
            // The default is a path inside the SOURCE TREE, which is right when
            // you are in the checkout and meaningless anywhere else. An
            // installed binary run from a stranger's home directory used to
            // create a phantom ./ui/public/fixtures that nothing serves, and
            // report success. Fail loud instead of writing real transcript
            // content somewhere nobody will look for it. (A123)
            let dir = match out {
                Some(dir) => dir,
                None => {
                    let default = PathBuf::from(thruline_core::export::DEFAULT_FIXTURE_DIR);
                    let anchor = default.parent().unwrap_or(&default);
                    if !anchor.is_dir() {
                        anyhow::bail!(
                            "no `{}` here, so the default output path would be meaningless.\n\
                             The default writes into the UI's source tree and only makes sense \
                             from the repo root.\n\
                             Run this from the checkout, or pass `--out <dir>` to say where the \
                             fixtures should go.\n\
                             Note the export contains real transcript content — choose a \
                             directory you would be willing to share.",
                            anchor.display()
                        );
                    }
                    default
                }
            };
            // Same walk `thruline ui` runs, from the same function. It takes
            // minutes over a real corpus (12m26s over 90 projects, A143), so it
            // reports every project rather than going silent — an agent-facing
            // line here, since this command is the ops path.
            let pinned = generated_at
                .as_deref()
                .map(export_all::pinned_generated_at)
                .transpose()?;
            let report =
                export_all::export_all(&db, &dir, diagnostics, pinned.as_deref(), &mut |p| {
                    if let export_all::Progress::Project {
                        done, total, id, ..
                    } = p
                    {
                        eprintln!("  [{done}/{total}] {id}");
                    }
                })?;
            println!(
                "wrote    {} ({} bytes)",
                report.index_path.display(),
                report.index_bytes
            );
            println!("projects {}", report.projects);
            println!("needing  {}", report.needing_attention);
            println!(
                "surface  {}",
                if diagnostics { "diagnostics" } else { "public" }
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Export {
            project,
            all: _,
            out,
            diagnostics,
            generated_at: pinned,
        } => {
            let project = project.expect("clap requires --project unless --all");
            let pinned = pinned
                .as_deref()
                .map(export_all::pinned_generated_at)
                .transpose()?;
            let cfg = thruline_core::config::Config::load_default()
                .context("loading ~/.thruline/config.toml")?;
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let graph = thruline_core::derive::derive_project_with(&ledger, &project, &cfg)
                .context("deriving")?;
            let generated_at = pinned.unwrap_or_else(thruline_core::ingest::iso8601_utc_now);
            let mut value = thruline_core::export::export_project_with_options(
                &ledger,
                &graph,
                &generated_at,
                thruline_core::export::ExportOptions { diagnostics },
            )
            .context("building the §9 document")?;
            let root = exclusion_serve::project_roots(&ledger)?
                .remove(&project)
                .context("the exported project has no recorded root")?;
            let policy = exclusion_serve::policy(&ledger, &root)?;
            exclusion_serve::checked_apply(&mut value, &policy)?;
            let path = out.unwrap_or_else(|| PathBuf::from(thruline_core::export::DEFAULT_OUT));
            thruline_core::export::write_json(&path, &value).context("writing the export")?;
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            println!("wrote    {} ({} bytes)", path.display(), bytes);
            println!("project  {project}");
            println!("nodes    {}", value["nodes"].as_array().map_or(0, Vec::len));
            println!("edges    {}", value["edges"].as_array().map_or(0, Vec::len));
            println!(
                "sessions {}",
                value["sessions"].as_array().map_or(0, Vec::len)
            );
            println!(
                "surface  {}",
                if diagnostics { "diagnostics" } else { "public" }
            );
            if let Some(counts) = value["counts"].as_object() {
                for (state, n) in counts {
                    println!("           {state:<14} {}", n.as_u64().unwrap_or(0));
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Now { project, all, json } => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let coverage = crate::doctor::coverage(None);
            let view = thruline_core::now::now(&ledger, project.as_deref(), all, coverage)
                .context("deriving the attention queue")?;
            if json {
                json_out::print(&json_out::NOW, serde_json::to_value(&view)?)?;
            } else {
                print_now(&view);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Ui(UiArgs {
            observe_codex,
            acceptance,
            verification_project,
            exec_db,
            port,
            no_open,
            refresh,
            dir,
            diagnostics,
            static_dir,
            address,
        }) => {
            // The reading lives beside the ledger it was read from, not in the
            // source tree: `thruline ui` has to mean the same thing from any
            // directory, and A123 already established that a default path inside
            // the checkout is meaningless everywhere else.
            let dir =
                dir.unwrap_or_else(|| db.parent().unwrap_or_else(|| Path::new(".")).join("export"));
            let verification = verification_project
                .map(|repo| -> Result<_> {
                    let repo = repo.canonicalize()?;
                    let store = exec_db
                        .clone()
                        .unwrap_or(thruline_ledger::Ledger::default_path()?);
                    let store = acceptance_cmd::resolved_path(&store)?;
                    anyhow::ensure!(
                        !store.starts_with(&repo),
                        "execution evidence store must be outside the checked project"
                    );
                    Ok((repo, store))
                })
                .transpose()?;
            uiserve::run(uiserve::Options {
                observe_codex,
                verification,
                acceptance: acceptance
                    .map(|repo| -> Result<_> {
                        Ok((
                            repo.canonicalize()?,
                            exec_db.unwrap_or(thruline_ledger::Ledger::default_path()?),
                        ))
                    })
                    .transpose()?,
                db: db.clone(),
                dir,
                port,
                open: !no_open,
                diagnostics,
                refresh,
                static_dir,
                address,
            })?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Why { item, json } => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            run_why(&ledger, &db, &item, json)
        }
        Command::Dismiss { item } => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            run_dismiss(&ledger, &item)
        }
        Command::ShadowSessions { project, all, json } => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let now = thruline_core::ingest::iso8601_utc_now();
            let mut rows = thruline_core::session_status::derive_all(&ledger, &now)
                .context("deriving session statuses")?;
            if let Some(p) = &project {
                rows.retain(|r| &r.project_id == p);
            }
            if json {
                json_out::print(&json_out::SHADOW_SESSIONS, serde_json::to_value(&rows)?)?;
                return Ok(ExitCode::SUCCESS);
            }
            println!("SHADOW DIAGNOSTICS — derived at read; hypotheses, not verdicts (RB1)");
            let mut counts: std::collections::BTreeMap<(&str, &str), u64> =
                std::collections::BTreeMap::new();
            for r in &rows {
                *counts
                    .entry((r.source.as_str(), r.status.as_str()))
                    .or_default() += 1;
            }
            let mut last_src = "";
            for ((src, status), n) in &counts {
                if *src != last_src {
                    println!("{src}");
                    last_src = src;
                }
                println!("  {status:<12} {n}");
            }
            if all {
                for r in rows
                    .iter()
                    .filter(|r| r.status != thruline_core::session_status::SessionStatus::Completed)
                {
                    println!(
                        "  {} · {} · {} · {} · {}",
                        r.status.as_str(),
                        r.session_id,
                        r.project_id,
                        r.last_ts.as_deref().unwrap_or("(no ts)"),
                        r.basis
                    );
                }
            }
            println!(
                "windows  active <= {} min · idle <= {} h (assumption-grade, for adjudication)",
                thruline_core::session_status::ACTIVE_WINDOW_MINUTES,
                thruline_core::session_status::IDLE_WINDOW_HOURS
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Capture { command } => run_capture(command, &db),
        Command::Sensor { action } => run_sensor(action, &db),
        Command::Digest {
            action: DigestAction::Precompute { hook },
        } => Ok(run_speaker_precompute(&db, &hook)),
        Command::Brief { hook } => Ok(run_speaker_brief(&db, &hook)),
        Command::Audit { hook } => Ok(run_speaker_audit(&db, &hook)),
        Command::Init {
            settings,
            claude_json,
            undo,
            yes,
            gate,
        } => run_init(settings, claude_json, undo, yes, gate, &db, db_named),
        Command::Autopsy => run_autopsy(&db),
        Command::Adjudicate {
            dry_run,
            status,
            operator,
        } => run_adjudicate(&db, dry_run, status, operator),
        Command::Guardian => run_guardian(&db),
        Command::HookInstall {
            harness,
            scope,
            settings,
            managed_settings_snippet,
            gate,
            undo,
            json,
        } => hook_install::run(hook_install::Args {
            harness,
            scope,
            settings,
            managed_settings_snippet,
            gate,
            undo,
            json,
        }),
        Command::Doctor {
            exec_db,
            spool,
            repo,
            json,
        } => doctor::run(doctor::Args {
            exec_db,
            spool,
            repo,
            json,
        }),
        Command::Hook { harness, spool } => {
            // Fail open, absolutely — including on an unknown harness. Exiting
            // non-zero here would break the agent of anyone who mistypes a
            // harness name in their settings file.
            let received_at = thruline_core::ingest::iso8601_utc_now();
            let code = match thruline_capture::harness::by_name(&harness) {
                Some(h) => thruline_capture::harness::run(
                    &h,
                    &mut std::io::stdin().lock(),
                    spool.as_deref(),
                    &received_at,
                    &thruline_capture::harness::repo_redactor(),
                ),
                None => 0,
            };
            Ok(std::process::ExitCode::from(code))
        }
        Command::Gate {
            hook,
            max_stop_blocks,
            budget_ms,
            exec_db,
            repo,
        } => Ok(gate_cmd::run(
            gate_cmd::Args {
                hook,
                exec_db,
                repo,
                max_stop_blocks,
                budget_ms,
            },
            &mut std::io::stdin().lock(),
        )),
        Command::Explain {
            session,
            last,
            full,
            json,
            md,
            html,
            exec_db,
            repo,
        } => explain_cmd::run(explain_cmd::Args {
            session,
            last,
            full,
            json,
            md,
            html,
            exec_db,
            repo,
        }),
        Command::Receipt(ReceiptArgs {
            predicate_version,
            session,
            sign_with,
            out,
            trailer,
            exec_db,
            repo,
            json,
        }) => receipt_cmd::build(receipt_cmd::BuildArgs {
            predicate_version,
            session,
            sign_with,
            out,
            trailer,
            exec_db,
            repo,
            json,
        }),
        Command::VerifyReceipt {
            receipt,
            key,
            offline,
            format,
            head_sha,
            exec_db,
            repo,
            json,
        } => receipt_cmd::verify(receipt_cmd::VerifyArgs {
            receipt,
            key,
            offline,
            format,
            head_sha,
            exec_db,
            repo,
            json,
        }),
        Command::Reexec {
            session,
            mode,
            backend,
            timeout_secs,
            exec_db,
            repo,
            json,
        } => reexec_cmd::run(reexec_cmd::Args {
            session,
            mode,
            backend,
            timeout_secs,
            exec_db,
            repo,
            json,
        }),
        Command::Promises {
            open,
            all,
            repo,
            exec_db,
            json,
        } => promises_cmd::run(promises_cmd::Args {
            open_only: open,
            all,
            repo,
            exec_db,
            json,
        }),
        Command::Claims {
            session,
            last,
            exec_db,
            repo,
            json,
            phases,
        } => claims_cmd::run(claims_cmd::Args {
            session,
            last,
            exec_db,
            repo,
            json,
            phases,
        }),
        Command::Shim { action } => match action {
            ShimAction::Install {
                dir,
                harness,
                spool,
                session,
                repo,
                bin,
                wrap,
                json,
            } => shim_cmd::install(shim_cmd::InstallArgs {
                dir,
                harness,
                spool,
                session,
                repo,
                bin,
                wrap,
                undo: false,
                json,
            }),
            ShimAction::Uninstall { dir, json } => shim_cmd::install(shim_cmd::InstallArgs {
                dir,
                harness: None,
                spool: None,
                session: None,
                repo: None,
                bin: None,
                wrap: None,
                undo: true,
                json,
            }),
            ShimAction::Status { dir, json } => shim_cmd::status(dir, json),
            // The one command whose exit code is not this process's opinion:
            // it is the wrapped command's, unchanged.
            ShimAction::Exec { program, args } => Ok(shim_cmd::run_exec(program, args)),
        },
        Command::Tui {
            session,
            last,
            exec_db,
            repo,
        } => tui_cmd::run(tui_cmd::Args {
            session,
            last,
            exec_db,
            repo,
        }),
        Command::Sync(a) => sync_cmd::run(sync_cmd::Args {
            verb: a.verb,
            json: a.json,
        }),
        Command::Acceptance(a) => acceptance_cmd::run(a),
        // Answered above, before the ledger path is resolved.
        Command::Licence(a) => licence_cmd::run(a),
        Command::Change(a) => {
            let (verb, json) = match a.verb {
                // T-4-2 — its own module; it shares nothing with the other
                // verbs but the ledger path.
                ChangeVerb::Finding(f) => return finding_cmd::run(db, f),
                // T-4-4-ATTEMPT-BUDGET — its own module, like `finding`.
                ChangeVerb::Attempts(a) => return attempts_cmd::run(db, a),
                // C30 R-LEDGER — its own module, like `finding` and `attempts`.
                ChangeVerb::Note(n) => return note_cmd::run(db, n),
                // C30 R-LEDGER — its own module, like `note`.
                ChangeVerb::Second(s) => return second_cmd::run(db, s),
                ChangeVerb::New {
                    title,
                    project,
                    root,
                    purpose,
                    requester,
                    json,
                } => (
                    change_cmd::Verb::New {
                        project,
                        root,
                        title,
                        purpose,
                        requester,
                    },
                    json,
                ),
                ChangeVerb::Suggest {
                    change,
                    record,
                    json,
                } => (change_cmd::Verb::Suggest { change, record }, json),
                ChangeVerb::Associate {
                    change,
                    session,
                    confirm,
                    reject,
                    actor,
                    reason,
                    json,
                } => (
                    change_cmd::Verb::Associate {
                        change,
                        session,
                        confirm,
                        reject,
                        actor,
                        reason,
                    },
                    json,
                ),
                ChangeVerb::Results { change, json } => {
                    (change_cmd::Verb::Results { change }, json)
                }
                ChangeVerb::Prepare {
                    change,
                    app_port,
                    config_files,
                    root,
                    json,
                } => (
                    change_cmd::Verb::Prepare {
                        change,
                        app_port,
                        config_files,
                        root,
                    },
                    json,
                ),
                ChangeVerb::Links { session, json } => (change_cmd::Verb::Links { session }, json),
                ChangeVerb::CancelRun { run, actor, json } => {
                    (change_cmd::Verb::CancelRun { run, actor }, json)
                }
                ChangeVerb::Waive {
                    change,
                    obligation,
                    actor,
                    rationale,
                    not_applicable,
                    json,
                } => (
                    change_cmd::Verb::Waive {
                        change,
                        obligation,
                        actor,
                        rationale,
                        not_applicable,
                    },
                    json,
                ),
                ChangeVerb::Propose { change, json } => {
                    (change_cmd::Verb::Propose { change }, json)
                }
                ChangeVerb::Accept {
                    change,
                    all,
                    criteria,
                    edits,
                    scope,
                    replacing,
                    actor,
                    rationale,
                    json,
                } => (
                    change_cmd::Verb::Accept {
                        change,
                        all,
                        criteria,
                        edits,
                        scope,
                        replacing,
                        actor,
                        rationale,
                    },
                    json,
                ),
            };
            change_cmd::run(change_cmd::Args { db, verb, json })
        }
        Command::Sessions(a) => sessions_cmd::run(sessions_cmd::Args {
            exec_db: a.exec_db,
            limit: a.limit,
            per_project: a.per_project,
            json: a.json,
        }),
        Command::Policy(a) => policy_cmd::run(policy_cmd::Args {
            init: a.init,
            session: a.session,
            exec_db: a.exec_db,
            repo: a.repo,
            policy: a.policy,
            base: a.base,
            json: a.json,
            md: a.md,
            conclusion: a.conclusion,
        }),
        // Unreachable: handled above, before the ledger path is resolved, so
        // that the money ceiling does not depend on a database. `unreachable!`
        // rather than a duplicate implementation — two places that both charge
        // money is exactly the shape this change is removing.
        Command::PilotBudget(_) => {
            unreachable!("pilot-budget is dispatched before the ledger is opened")
        }
        Command::Bench(a) => bench_cmd::run(bench_cmd::Args {
            fixtures: a.fixtures,
            html: a.html,
            json: a.json,
        }),
        Command::ImportCodex(a) => import_codex::run(import_codex::Args {
            sessions_root: a.sessions_root,
            session_file: a.session_file,
            exec_db: a.exec_db,
            repo: a.repo,
            limit: a.limit,
            dry_run: a.dry_run,
            json: a.json,
        }),
        Command::Run(a) => run_cmd::run(run_cmd::Args {
            command: a.command,
            session: a.session,
            harness: a.harness,
            repo: a.repo,
            no_pty: a.no_pty,
            report: a.report,
            json: a.json,
        }),
        Command::Fold {
            spool,
            exec_db,
            repo,
            from_sensor,
            json,
        } => shim_cmd::fold(shim_cmd::FoldArgs {
            spool,
            exec_db,
            repo,
            from_sensor,
            sensor_db: db.clone(),
            json,
        }),
        Command::ImportLegacy(a) => import_legacy::run(import_legacy::Args {
            source: a.source,
            exec_db: a.exec_db,
            project: a.project,
            dry_run: a.dry_run,
            limit: a.limit,
            page_size: a.page_size,
            page_bytes: a.page_bytes,
            include_meta: a.include_meta,
            verify: a.verify,
            json: a.json,
        }),
        Command::Integration { command } => integration_cmd::run(&db, command),
        Command::Check {
            shell,
            quiet,
            artifacts,
            app_port,
            config_files,
            for_change,
            actor,
            evidence_class,
            command,
        } => {
            // A class word this product cannot render is refused before the
            // command runs, never stored as itself and never coerced to one
            // of the four. `B16.10` is about a label being true; a label that
            // is a typo is not one.
            let evidence_class = match evidence_class.as_deref() {
                None => None,
                Some(word) => match thruline_core::run_record::EvidenceClass::parse(word) {
                    Some(class) => Some(class),
                    None => {
                        let every = thruline_core::run_record::EvidenceClass::every()
                            .iter()
                            .map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        anyhow::bail!(
                            "--evidence-class {word:?} is not a class we record.                              It is one of: {every}."
                        );
                    }
                },
            };
            check::run_check(
                &db,
                check::CheckArgs {
                    shell,
                    quiet,
                    artifacts,
                    app_port,
                    config_files,
                    for_change,
                    actor,
                    evidence_class,
                    command,
                },
            )
        }
        Command::Evidence {
            id,
            all,
            root,
            json,
        } => check::run_evidence(
            &db,
            check::EvidenceArgs {
                id,
                all,
                root,
                json,
            },
        ),
        Command::Snapshot { root, verify } => match verify {
            Some(id) => run_snapshot_verify(&db, &id),
            None => run_snapshot(&db, root),
        },
        Command::Restore {
            snapshot,
            to,
            over_working_tree,
            expect_tree,
            yes,
        } => run_restore(&db, &snapshot, to, over_working_tree, expect_tree, yes),
        Command::Capsule {
            project,
            root,
            out,
            diagnostics,
        } => run_capsule(&db, &project, root, out, diagnostics),
        Command::Protect { project, root, out } => run_protect(&db, &project, root, out),
        Command::VerifyContinuity {
            project,
            capsule,
            root,
            objective_acknowledged,
            override_reason,
        } => run_verify_continuity(
            &db,
            &project,
            &capsule,
            root,
            objective_acknowledged,
            override_reason,
        ),
        Command::Rescue {
            project,
            agent,
            out,
            acknowledge,
            emit_hook,
        } => run_rescue(&db, &project, &agent, out, acknowledge, emit_hook),
        // §15's demo is the default; the two older walks keep their flags.
        Command::Demo {
            continuity,
            recovery,
            dir,
            json,
            ..
        } if !continuity && !recovery => demo_cmd::run(demo_cmd::Args { dir, json }),
        Command::Demo { keep, recovery, .. } if recovery => {
            // C4.4 — the kill-9 walk. Same temp-dir discipline: the user's
            // ledger is never opened, and the only process killed is the
            // walk's own child.
            let dir = tempfile::TempDir::new().context("creating the demo directory")?;
            let r = thruline_core::recovery_demo::walk(dir.path())
                .context("running the recovery demo")?;
            println!(
                "THE SYNTHETIC KILL-9 PREPARATION WALK — local fixtures, nothing of yours touched"
            );
            println!();
            println!("1 · a synthetic worker is running");
            println!("    a real child process is running beside synthetic session records");
            println!("2 · protect");
            println!(
                "    {} uncommitted file entry or entries described by path/digest · snapshot {}",
                r.snapshot_file_entries, r.snapshot_id
            );
            println!(
                "    capsule compiled · {} section(s) carry lines",
                r.capsule_sections_with_lines
            );
            println!("3 · the crash");
            println!(
                "    SIGKILL to pid {} — the walk's own child, and nothing else",
                r.killed_pid
            );
            println!("    the process is gone: {}", r.child_died);
            println!("4 · what the record shows");
            println!(
                "    a liveness marker outlived the process with no close recorded — crash candidate: {}",
                r.crash_candidate
            );
            println!("5 · rescue");
            for path in &r.staged {
                println!("    staged {path}");
            }
            println!("    no Claude or Codex successor was launched or interrogated");
            println!("6 · legacy local comparison");
            println!("    {}", r.handshake_coverage);
            println!("    outcome: {}", r.handshake_outcome);
            if let Some(reason) = &r.handshake_reason {
                println!("    reason: {reason}");
            }
            let (v, t) = r.verified_continuation_rate;
            let (legacy_v, legacy_t) = r.legacy_local_continuity_rate;
            println!();
            println!(
                "    P1 verified recovery rate: {v} of {t} protocol-valid successor handshake(s)"
            );
            println!(
                "    legacy local-only audit: {legacy_v} old success label(s) across \
                 {legacy_t} row(s); excluded from recovery"
            );
            if keep {
                let kept = dir.keep();
                println!();
                println!("    kept at {}", kept.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Demo { keep, .. } => {
            // A temp dir the walk owns. The user's ledger is never opened: the
            // `--db` flag is deliberately ignored here, because a demo that
            // wrote to your real ledger would not be a demo.
            let dir = tempfile::TempDir::new().context("creating the demo directory")?;
            let report = thruline_core::demo::walk(dir.path()).context("running the demo")?;

            println!("\n\x1b[1mTHE WEDGE — two agents, one repo, one verdict\x1b[0m\n");
            println!("  repo      {}", report.repo_id);
            for pid in &report.project_ids {
                println!("  project   {pid}");
            }
            println!(
                "\n\x1b[1m1. Claude finishes its turn.\x1b[0m A packet is compiled and issued.\n\
                 \x1b[2m   issue {}\x1b[0m\n\n\
                 \x1b[1mVERIFIED AND CURRENT\x1b[0m{}",
                report.first_issue, report.first_verified_section
            );
            println!(
                "\x1b[1m2. That exact packet is frozen into a handoff.\x1b[0m\n\n{}",
                report
                    .frozen_handoff
                    .lines()
                    .take(3)
                    .map(|l| format!("   {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            println!(
                "\n\x1b[1m3. Codex continues the work and runs a real command.\x1b[0m\n\
                 \x1b[2m   issue {}\x1b[0m\n\n\
                 \x1b[1mVERIFIED AND CURRENT\x1b[0m{}",
                report.second_issue, report.second_verified_section
            );
            println!(
                "\x1b[1m4. The chain records where the work came from.\x1b[0m\n\n{}\n",
                report
                    .frozen_successor
                    .lines()
                    .take(3)
                    .map(|l| format!("   {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            println!(
                "  The only difference between packet 1 and packet 2 is one command\n  \
                 that actually ran. {} seeded event(s) — every row above came from\n  \
                 ingesting a transcript or running a subprocess.\n",
                report.seeded_events
            );
            if keep {
                let path = dir.keep();
                println!("  kept      {}\n", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Projects => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let mut out = std::io::stdout().lock();
            for p in ledger.project_ids()? {
                // `thruline projects | head -1` is an entirely ordinary thing to
                // do, and `println!` answers it with a Rust panic and a
                // backtrace note — because `println!` panics when the write
                // fails, and a closed pipe is a failed write. Found by the A195
                // sweep while writing the operator test script, which is exactly
                // the kind of thing a stranger-run is for.
                //
                // A closed pipe is the reader saying "I have enough", which is
                // not an error and must not be reported as one. Rust cannot
                // restore the default SIGPIPE disposition without `unsafe`, and
                // this crate does not use `unsafe` — so the write is checked
                // instead, and the loop stops when nobody is listening.
                if writeln!(out, "{:>8}  {p}", ledger.event_count(&p)?).is_err() {
                    break;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Upgrade { dry_run, force } => run_upgrade(&db, dry_run, force),
        Command::Perf { stamp, doc_block } => {
            if doc_block {
                let registry =
                    perf_basis::Registry::load().context("reading the stamped-basis registry")?;
                print!("{}", perf_basis::render_doc_block(&registry));
                return Ok(ExitCode::SUCCESS);
            }
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let mut observed = perf_basis::Observed::from_ledger(&ledger, CODE_DIGEST)
                .context("measuring the conditions here")?;
            observed.ledger_bytes = fs::metadata(&db).ok().map(|m| m.len());
            if stamp {
                print!("{}", perf_stamp_block(&observed));
                return Ok(ExitCode::SUCCESS);
            }
            let registry =
                perf_basis::Registry::load().context("reading the stamped-basis registry")?;
            println!(
                "Conditions here: {} projects · {} events · {} sessions · {} nodes · \
                 {} project heads · {} node projections · code {} · {} build",
                observed.projects,
                observed.events,
                observed.sessions,
                observed.nodes,
                observed.project_heads,
                observed.node_projections,
                observed.code_digest,
                observed.profile,
            );
            println!();
            print!("{}", perf_basis::render_table(&registry, &observed));
            println!();
            let quotable = registry
                .figures
                .iter()
                .filter(|f| registry.currency(f, &observed).quotable())
                .count();
            println!(
                "{quotable} of {} figures are current here. \
                 The rest can be re-measured with: scripts/perf-budget-suite.sh <ledger.db> 20",
                registry.figures.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Handoff {
            project,
            repo,
            issue,
        } => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let scope = match (&project, &repo) {
                (Some(p), _) => format!("project:{p}"),
                (None, Some(r)) => format!("repo:{r}"),
                (None, None) => anyhow::bail!("give --project <id> or --repo <repo_id>"),
            };
            let issue_id = match issue {
                Some(i) => i,
                None => ledger
                    .latest_issue_for_scope(&scope)
                    .context("finding the latest frozen packet")?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "no packet has been served for {scope}; run `thruline packet` first"
                        )
                    })?,
            };
            match thruline_core::packet::handoff(&ledger, &issue_id)
                .context("rendering the handoff")?
            {
                Some(text) => print!("{text}"),
                None => anyhow::bail!("no frozen packet with issue id {issue_id}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Deltas {
            project,
            repo,
            since,
        } => {
            use thruline_core::packet::{repo_scope, ContextAnswer, Scope};
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let scope = match (project, repo) {
                (Some(p), _) => Scope::Project(p),
                (None, Some(r)) => repo_scope(&ledger, &r).context("resolving the repo scope")?,
                (None, None) => anyhow::bail!("give --project <id> or --repo <repo_id>"),
            };
            let cfg = thruline_core::config::Config::load_default().unwrap_or_default();
            match thruline_core::packet::context_since(&ledger, &scope, &since, &cfg)
                .context("computing the change set")?
            {
                ContextAnswer::Current { .. } => {
                    println!("Nothing has changed. What you have still matches the record.");
                }
                ContextAnswer::UnknownPosition { latest } => {
                    println!(
                        "No packet was handed out at that position, so what changed cannot be \
                         worked out. Ask for a fresh packet."
                    );
                    println!("  the record is now at {}", &latest[..16]);
                }
                ContextAnswer::Changed {
                    latest,
                    lines,
                    served_by,
                } => {
                    if lines.is_empty() {
                        println!("The record moved, but nothing in your packet changed with it.");
                    } else {
                        let mut section = String::new();
                        for line in &lines {
                            if line.section != section {
                                section = line.section.clone();
                                println!("\n## {section}\n");
                            }
                            println!("{}", line.render());
                        }
                    }
                    let fast = served_by
                        .iter()
                        .filter(|(_, s)| *s == thruline_core::packet::ServedBy::PersistedProjection)
                        .count();
                    eprintln!(
                        "\n[{} change(s) · the record is now at {} · {fast}/{} from the persisted projection]",
                        lines.len(),
                        &latest[..16],
                        served_by.len()
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Mcp { agent } => {
            let server = thruline_core::mcp::Server::new(&agent, db.clone())
                .context("starting the MCP server")?;
            let stdin = std::io::stdin();
            server
                .serve(stdin.lock(), std::io::stdout())
                .context("serving MCP over stdio")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Packet {
            project,
            repo,
            task,
            budget,
        } => {
            use thruline_core::packet::{repo_scope, Budget, Scope};
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            let scope = match (project, repo) {
                (Some(p), _) => Scope::Project(p),
                (None, Some(r)) => repo_scope(&ledger, &r).context("resolving the repo scope")?,
                (None, None) => {
                    anyhow::bail!("give --project <id> or --repo <repo_id> (see `identity groups`)")
                }
            };
            let cfg = thruline_core::config::Config::default();
            // Derived in memory, per project. The nodes table holds only the
            // last-derived project, so reading it would silently report an
            // empty world for every other project in scope.
            let packet = thruline_core::packet::compile_request(
                &ledger,
                &scope,
                task.as_deref(),
                &Budget { tokens: budget },
                &cfg,
            )
            .context("compiling the packet")?;
            // Retain what we hand over, so `deltas --since` can diff against
            // the receipt rather than a reconstruction.
            {
                let mut w = Ledger::open(&db).context("reopening to retain the issue")?;
                let at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                packet
                    .retain(&mut w, None, at)
                    .context("retaining the served packet")?;
            }
            // RA2 — surface separation. A packet is written for an agent, and a
            // human who lands in one has been handed the wrong document; the
            // banner says so in the first line and names the door they wanted.
            //
            // It is printed HERE, not in `packet.render()`, and that placement
            // is the whole point: the MCP server serves the same `render()` to
            // agents, which need no warning and must not be told to run a
            // browser command. One renderer, one audience-specific frame.
            println!("{PACKET_BANNER}");
            print!("{}", packet.render());
            let fast = packet
                .served_by
                .iter()
                .filter(|(_, s)| *s == thruline_core::packet::ServedBy::PersistedProjection)
                .count();
            eprintln!(
                "\n[{} line(s), ~{} tokens, {} excluded, {} project(s) · {fast}/{} from the persisted projection]",
                packet.lines.len(),
                packet.tokens,
                packet.exclusions.len(),
                packet.binding.heads.len(),
                packet.served_by.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Identity { cmd } => match cmd {
            IdentityCommand::Backfill { sample, dry_run } => {
                let ledger = Ledger::open(&db).context("opening the ledger")?;
                let observed_at = thruline_core::ingest::iso8601_utc_now();
                let mut counts = (0usize, 0usize, 0usize); // git, marker, unresolvable
                let mut rows = Vec::new();
                for (project_id, _root) in ledger.project_roots()? {
                    let files = ledger.recent_session_files(&project_id, sample)?;
                    let row = thruline_core::workspace::identity_for_project(
                        &project_id,
                        &files,
                        3,
                        &observed_at,
                    )
                    .with_context(|| format!("deriving identity for {project_id}"))?;
                    match row.basis.as_str() {
                        "git" => counts.0 += 1,
                        "marker" => counts.1 += 1,
                        _ => counts.2 += 1,
                    }
                    if !dry_run {
                        ledger
                            .record_workspace_identity(&row)
                            .with_context(|| format!("recording identity for {project_id}"))?;
                    }
                    rows.push(row);
                }
                for row in &rows {
                    println!(
                        "{:<12} {:<14} {}",
                        row.basis,
                        row.resolved_via.as_deref().unwrap_or("-"),
                        row.project_id
                    );
                }
                let groups = thruline_core::workspace::group_by_repo(&rows);
                let shared = groups.iter().filter(|g| g.roots.len() > 1).count();
                println!(
                    "\n{} project(s): {} git · {} marker · {} unresolvable",
                    rows.len(),
                    counts.0,
                    counts.1,
                    counts.2
                );
                println!(
                    "{} distinct repo(s); {} with more than one working root",
                    groups.len(),
                    shared
                );
                if dry_run {
                    println!("dry run — nothing written");
                }
                Ok(ExitCode::SUCCESS)
            }
            IdentityCommand::Groups => {
                let ledger = Ledger::open(&db).context("opening the ledger")?;
                let rows = ledger.workspace_identities()?;
                if rows.is_empty() {
                    println!("no identity recorded yet — run `thruline identity backfill`");
                    return Ok(ExitCode::SUCCESS);
                }
                let groups = thruline_core::workspace::group_by_repo(&rows);
                for group in &groups {
                    println!(
                        "repo {}  ({} working root(s))",
                        &group.repo_id[..16],
                        group.roots.len()
                    );
                    for root in &group.roots {
                        println!(
                            "    {}\n        {}",
                            root.project_id,
                            root.working_root.as_deref().unwrap_or("(unresolved)")
                        );
                    }
                }
                let unresolved = rows.iter().filter(|r| r.repo_id.is_none()).count();
                println!(
                    "\n{} repo(s) across {} project(s); {} project(s) unresolved and left so",
                    groups.len(),
                    rows.len() - unresolved,
                    unresolved
                );
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Verify(args) => verify_cmd::run(
            &db,
            args.project.as_deref(),
            args.json,
            Some(thruline_core::capture_hold::verify_chain),
        ),
        Command::VerifyClaims { json } => {
            let ledger = Ledger::open(&db).context("opening the ledger")?;
            run_verify_claims(&ledger, &db, json)
        }
    }
}

/// A201 — `thruline verify-claims`: the deterministic pass over every
/// claim_completion submission still awaiting verification. Verdicts append;
/// the affected projects' digest caches are then rewritten so a verdict that
/// just landed cannot leave a stale line waiting for the next brief.
fn run_verify_claims(ledger: &Ledger, db: &Path, json: bool) -> Result<ExitCode> {
    use thruline_core::verifier::{self, ClaimVerdict};

    let now = thruline_core::ingest::iso8601_utc_now();
    let spill = thruline_core::sensor::spill_path(db);
    let report = verifier::verify_claims(ledger, &spill, &now).context("verifying claims")?;

    // The stale-accusation guard: every project scope the pass touched gets
    // its cached claim lines rewritten to match the standing verdicts.
    let dir = thruline_core::speaker::digest_dir(db);
    let mut refreshed: Vec<String> = report
        .iter()
        .filter_map(|c| c.scope.strip_prefix("project:").map(str::to_owned))
        .collect();
    refreshed.sort();
    refreshed.dedup();
    for key in &refreshed {
        verifier::refresh_claim_lines(ledger, &dir, key, &now)
            .with_context(|| format!("refreshing the digest cache for {key}"))?;
    }

    if json {
        let rows: Vec<serde_json::Value> = report
            .iter()
            .map(|c| {
                serde_json::json!({
                    "claim_id": verifier::claim_display_id(&c.call_id),
                    "call_id": c.call_id,
                    "agent": c.agent,
                    "scope": c.scope,
                    "obligation": c.obligation,
                    "verdict": c.verdict.as_str(),
                    "reason": c.reason,
                    "verdict_id": c.verdict_short_id,
                    "newly_recorded": c.newly_recorded,
                })
            })
            .collect();
        json_out::print(&json_out::VERIFY_CLAIMS, serde_json::Value::Array(rows))?;
        return Ok(ExitCode::SUCCESS);
    }

    println!("CLAIM VERIFICATION — append-only; a verdict never rewrites a claim");
    if report.is_empty() {
        println!("claims           none awaiting verification");
        return Ok(ExitCode::SUCCESS);
    }
    println!("claims           {} awaiting verification", report.len());
    let mut new_rows = 0u64;
    for claim in &report {
        let obligation = claim
            .obligation
            .as_deref()
            .unwrap_or("(no obligation text)");
        println!();
        println!(
            "  {}  {}",
            verifier::claim_display_id(&claim.call_id),
            match claim.verdict {
                ClaimVerdict::Supported => claim.verdict.phrase(),
                ClaimVerdict::NoProofFound => "claimed, and no proof found in the record",
                ClaimVerdict::Unknown => claim.verdict.phrase(),
            }
        );
        println!(
            "    claim          {obligation:?} ({}, {})",
            claim.agent, claim.scope
        );
        println!("    basis          {}", claim.reason);
        println!(
            "    verdict        {} ({})",
            claim.verdict_short_id,
            if claim.newly_recorded {
                "appended this run"
            } else {
                "already on record"
            }
        );
        println!("    why            thruline why {}", claim.verdict_short_id);
        if claim.newly_recorded {
            new_rows += 1;
        }
    }
    println!();
    println!(
        "recorded         {} appended; {} already stood",
        phrase_simple(new_rows, "verdict", "verdicts"),
        report.len() as u64 - new_rows
    );
    if !refreshed.is_empty() {
        println!(
            "digest cache     rewritten for {} — the next brief reflects these verdicts",
            phrase_simple(refreshed.len() as u64, "project", "projects")
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// The first ten seconds, in a terminal: one headline per project, then items,
/// each one plain sentence and one suggested action.
///
/// Every string below arrives from the voice authority. The only text this
/// function contributes is punctuation, the `why` handle, and the labels on
/// level-three detail — and `now_output_is_free_of_engine_lexicon` walks the
/// rendered result to keep that true.
fn print_now(view: &[thruline_core::now::ProjectNow]) {
    for (i, project) in view.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{}", project.headline);
        if let Some(note) = &project.hidden_note {
            println!("{note}");
        }
        for item in &project.items {
            println!();
            println!("  {}", item.sentence);
            println!("  → {}", item.action);
            if let Some(at) = item.dismissed_at_unix_ms {
                println!(
                    "    dismissed        {}",
                    thruline_core::ingest::iso8601_utc(at / 1000)
                );
            }
            println!("    why              thruline why {}", item.id);
        }
    }
}

/// `why` — the same three levels the R2 UI will render.
/// Two million events is a number; 2,012,204 is a quantity a person can hold.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// C34 Q14 — what the row census found, in the words the founder reads after an
/// upgrade. The headline says "rows before = rows after for every existing
/// table" only when that is what was found; every other finding is named, with
/// the table and both counts, never summed away.
fn print_upgrade_census(census: &thruline_core::upgrade::CensusDiff) {
    println!("Row census, every table counted before and after:");
    let new_tables = census.new_tables.len();
    let holding = census.new_tables_holding_rows();
    let new_words = if holding.is_empty() {
        format!("new tables {new_tables}, all empty")
    } else {
        let named: Vec<String> = holding
            .iter()
            .map(|(name, rows)| format!("{name} {}", grouped(*rows)))
            .collect();
        format!(
            "new tables {new_tables}, {} holding rows ({})",
            holding.len(),
            named.join(", ")
        )
    };
    if census.unchanged() {
        println!(
            "  rows before = rows after for every existing table ({} tables, {} rows) · {new_words}",
            census.tables_compared,
            grouped(census.rows_compared)
        );
    } else {
        println!(
            "  rows differ for {} of {} existing tables · {new_words}",
            census.lost.len() + census.grew.len(),
            census.tables_compared
        );
    }
    for (table, before, after) in &census.lost {
        println!(
            "  FEWER ROWS  {table}: {} before, {} after",
            grouped(*before),
            grouped(*after)
        );
    }
    for (table, before, after) in &census.grew {
        println!(
            "  more rows   {table}: {} before, {} after (a migration step backfilled it, or another process wrote while the upgrade ran)",
            grouped(*before),
            grouped(*after)
        );
    }
    if let Some((before, after)) = census.stamp {
        println!(
            "  {} is the upgrade's own stamp: {before} before, {after} after",
            thruline_core::upgrade::STAMP_TABLE
        );
    }
}

/// A193 — the one place a schema migration is paid, and it is paid visibly.
///
/// Four things this prints that the old silent path never could: what state the
/// ledger is in, how long the work is expected to take and on whose measurement,
/// whether there is disk room for it, and each step as it completes. The last
/// one matters more than it looks — a half-minute of silence on a 6.8 GiB file
/// is indistinguishable from a hang, and a migration people kill halfway is the
/// one failure mode a schema upgrade must not have.
fn run_upgrade(db: &Path, dry_run: bool, force: bool) -> Result<ExitCode> {
    use thruline_core::upgrade::{
        disk_preflight, estimate_bases, estimate_phrase, human_bytes, UpgradeState,
    };

    let state = Ledger::upgrade_state(db).context("reading the ledger's schema state")?;
    match &state {
        UpgradeState::Fresh => {
            println!("No ledger at {} yet — nothing to upgrade.", db.display());
            println!("`thruline ingest` will create one, already current.");
            Ok(ExitCode::SUCCESS)
        }
        UpgradeState::Current => {
            println!("Schema is already up to date. Nothing to do.");
            Ok(ExitCode::SUCCESS)
        }
        UpgradeState::Pending { events } => {
            let preflight = disk_preflight(db);
            println!("Schema upgrade pending.");
            println!();
            println!("  ledger      {}", db.display());
            println!(
                "  size        {} · {} events",
                human_bytes(preflight.ledger_bytes),
                grouped(*events)
            );
            println!(
                "  estimate    {} (from measurements on basis {})",
                estimate_phrase(*events),
                estimate_bases().join(", ")
            );
            match (preflight.available_bytes, preflight.sufficient) {
                (Some(free), Some(true)) => {
                    println!("  disk        {} free — enough", human_bytes(free))
                }
                (Some(free), Some(false)) => println!(
                    "  disk        {} free — LESS than the ledger's own size",
                    human_bytes(free)
                ),
                _ => println!("  disk        free space could not be read"),
            }
            println!("  steps       {}", Ledger::SCHEMA_STEPS.len());
            println!("  rows        every table is counted before and after, and the two counts compared");
            println!();

            if dry_run {
                println!("--dry-run: nothing was changed.");
                return Ok(ExitCode::SUCCESS);
            }

            // A131: the volume this ledger lives on has been filled to 120 MiB
            // free once already, by a runaway this project shipped. A migration
            // step may rewrite a table, and SQLite rewrites by building a copy,
            // so the requirement is room for one — stated, checkable, and
            // overridable by someone who knows better than the rule.
            if preflight.sufficient == Some(false) && !force {
                eprintln!(
                    "Refusing: a migration step may need room for a full copy of the ledger."
                );
                eprintln!("Free space, or re-run with --force if you know this is safe.");
                return Ok(ExitCode::from(4));
            }
            if preflight.available_bytes.is_none() && !force {
                eprintln!("Refusing: free disk space could not be read, so the preflight");
                eprintln!("cannot pass. Re-run with --force to upgrade anyway.");
                return Ok(ExitCode::from(4));
            }

            // C34 Q14 — the census. Every table is counted before anything is
            // paid, so a failure to count stops the upgrade having changed
            // nothing, and the same count is taken again afterwards. The count
            // is a pass over the ledger, so it is announced rather than silent.
            print!("  counting    every table before the upgrade ... ");
            let _ = std::io::stdout().flush();
            let before_census = thruline_core::upgrade::row_census(db)
                .context("counting the rows of every table before the upgrade")?;
            // The stamp table is reported by itself after the upgrade, so it is
            // left out of this tally and the census headline's.
            let counted: Vec<&(String, u64)> = before_census
                .iter()
                .filter(|(name, _)| name != thruline_core::upgrade::STAMP_TABLE)
                .collect();
            println!(
                "{} tables, {} rows (schema_migrations apart)",
                counted.len(),
                grouped(counted.iter().map(|(_, rows)| *rows).sum())
            );
            println!();

            // The callback fires BEFORE each step, so a step's name appears
            // while it is running — a half-minute of silence on a 6.8 GiB file
            // is indistinguishable from a hang. Its duration therefore has to
            // close the PREVIOUS line, not decorate the new one. An earlier
            // draft printed the elapsed time beside the step about to start and
            // attributed 25.9 seconds to the wrong step, which is the exact
            // error three directives of this run were spent unwinding.
            let started = std::time::Instant::now();
            let mut last = std::time::Instant::now();
            let mut line_open = false;
            let ledger = Ledger::open_upgrading(db, &mut |step, index, total| {
                let now = std::time::Instant::now();
                if line_open {
                    println!(" ({:.1}s)", now.duration_since(last).as_secs_f64());
                }
                print!("  [{index:>2}/{total}] {step}");
                let _ = std::io::stdout().flush();
                line_open = true;
                last = now;
            })
            .context("upgrading the schema")?;
            if line_open {
                println!(" ({:.1}s)", last.elapsed().as_secs_f64());
            }
            let elapsed = started.elapsed().as_secs_f64();
            drop(ledger);

            println!();
            let after_census = thruline_core::upgrade::row_census(db)
                .context("counting the rows of every table after the upgrade")?;
            let census = thruline_core::upgrade::compare_census(&before_census, &after_census);
            print_upgrade_census(&census);
            println!();
            println!("Done in {elapsed:.1}s — every command works again.");
            println!(
                "Estimated {}; if that was far off, the measured costs in \
                 crates/core/src/upgrade.rs want re-measuring.",
                estimate_phrase(*events)
            );
            if census.lost.is_empty() {
                Ok(ExitCode::SUCCESS)
            } else {
                // The schema is upgraded, but a table holds fewer rows than it
                // did. That is never expected of a store at the previous schema,
                // and exit 0 over it would be the quiet kind of failure.
                eprintln!(
                    "The upgrade finished, but the row census found fewer rows than before. \
                     Keep any copy of the ledger you took before upgrading, and do not \
                     delete anything."
                );
                Ok(ExitCode::FAILURE)
            }
        }
    }
}

fn run_why(ledger: &Ledger, db: &Path, item: &str, json: bool) -> Result<ExitCode> {
    use thruline_core::now::Resolution;
    // A201: truth-loop ids first — the audit's findings (spkf_), left-open
    // promises (spkp_), claim verdicts (cvrd_) and claim calls (tlc_) each
    // wear a distinctive prefix, so this dispatch cannot shadow an attention
    // item. Anything else falls through to the attention namespace below.
    if let Some(resolution) = thruline_core::verifier::why_truth_loop(
        ledger,
        &thruline_core::speaker::digest_dir(db),
        item,
    )
    .context("resolving the id")?
    {
        use thruline_core::verifier::WhyResolution;
        return Ok(match resolution {
            WhyResolution::One { id, rendered } => {
                if json {
                    json_out::print(
                        &json_out::WHY_RECORD,
                        serde_json::json!({ "id": id, "text": rendered }),
                    )?;
                } else {
                    print!("{rendered}");
                }
                ExitCode::SUCCESS
            }
            WhyResolution::Ambiguous(ids) => {
                eprintln!("{item:?} matches more than one record; refusing to guess:");
                for id in &ids {
                    eprintln!("  {id}");
                }
                ExitCode::FAILURE
            }
            WhyResolution::NotFound => {
                eprintln!("nothing on record matches {item:?}");
                ExitCode::FAILURE
            }
        });
    }
    match thruline_core::now::why(ledger, item).context("resolving the item")? {
        Resolution::One(view) => {
            if json {
                json_out::print(&json_out::WHY_ATTENTION, serde_json::to_value(&view)?)?;
                return Ok(ExitCode::SUCCESS);
            }
            // Level one.
            println!("{}", view.sentence);
            println!("→ {}", view.action);
            // Level two.
            println!();
            println!("{}", view.what_happened);
            if let Some(quoted) = &view.quoted {
                println!("  the project's own words: {quoted:?}");
            }
            if let Some(at) = view.dismissed_at_unix_ms {
                println!(
                    "  dismissed {}",
                    thruline_core::ingest::iso8601_utc(at / 1000)
                );
            }
            // Level three: engine detail, unchanged, exactly one step further.
            if !view.receipts.is_empty() || !view.records.is_empty() {
                println!();
                println!("the receipt");
                for r in &view.receipts {
                    println!("  {}:{}", r.raw_file, r.raw_line);
                    println!("    event          {}", r.event_id);
                    println!("    session        {}", r.session_id);
                    println!("    line digest    {}", r.raw_sha256);
                    println!("    chain digest   {}", r.chain_hash);
                }
                for record in &view.records {
                    println!("    record         {record}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        // Ambiguity is refused, never guessed — the uninstall register.
        Resolution::Ambiguous(ids) => {
            eprintln!("{:?} matches more than one item; refusing to guess:", item);
            for id in &ids {
                eprintln!("  {id}");
            }
            Ok(ExitCode::FAILURE)
        }
        Resolution::NotFound => {
            if let Some(exit) = decide_cmd::try_why(ledger, item, json)? {
                return Ok(exit);
            }
            eprintln!("no attention item matches {item:?}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// The one write on the attention surface: an append-only acknowledgment.
fn run_dismiss(ledger: &Ledger, item: &str) -> Result<ExitCode> {
    use thruline_core::attention::{acknowledge, AcknowledgmentAction};
    use thruline_core::now::Resolution;
    match thruline_core::now::why(ledger, item).context("resolving the item")? {
        Resolution::One(view) => {
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            acknowledge(ledger, &view.id, AcknowledgmentAction::Dismissed, at)?;
            println!("dismissed        {}", view.id);
            println!("record           appended; nothing was modified");
            Ok(ExitCode::SUCCESS)
        }
        Resolution::Ambiguous(ids) => {
            eprintln!("{:?} matches more than one item; refusing to guess:", item);
            for id in &ids {
                eprintln!("  {id}");
            }
            Ok(ExitCode::FAILURE)
        }
        Resolution::NotFound => {
            eprintln!("no attention item matches {item:?}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn run_capture(command: CaptureCommand, db: &Path) -> Result<ExitCode> {
    use thruline_core::capture::{
        accept_capture_pair, accept_observation_and_publish, record_source_assurance,
        source_assurance_enabled, CaptureAcceptance, CaptureRequest, Collector,
        SourceAssuranceMeasurement, SourceDelivery,
    };
    use thruline_core::capture_product::{
        create_install_plan, derive_readiness, load_install_plan, new_compatibility_profile,
        new_lifecycle_payload, new_self_test_payload, operational_assurance_enabled,
        record_authorization, record_compatibility_profile, record_lifecycle, record_self_test,
        InstallPlanInput, NewSelfTestInput,
    };
    use thruline_core::chain::sha256_hex;

    match command {
        // C30 R-LEDGER: a person's pause and exclusions. A pause is honoured by
        // the six capture callers (C34); an exclusion is recorded, not yet applied.
        CaptureCommand::Pause(a) => hold_cmd::pause(db, a),
        CaptureCommand::Resume(a) => hold_cmd::resume(db, a),
        CaptureCommand::Exclude(a) => hold_cmd::exclude(db, a),
        CaptureCommand::Include(a) => hold_cmd::include(db, a),
        CaptureCommand::Holds(a) => hold_cmd::show(db, a),
        CaptureCommand::Install {
            source,
            config,
            capture_root,
            owner,
            source_version,
            source_executable,
            apply,
            json,
        } => {
            anyhow::ensure!(source == CaptureSource::Codex, "Claude assurance installation remains disabled until its local timing boundary is proven");
            let owner = canonical_codex_owner(&config, &owner)?;
            anyhow::ensure!(
                source_executable.is_file(),
                "source executable is unavailable"
            );
            anyhow::ensure!(
                detected_source_version(&source_executable)? == source_version,
                "supplied source version differs from the detected executable"
            );
            let wrapper = std::env::current_exe().context("resolving the thruline executable")?;
            let ledger = Ledger::open(db).context("opening ledger")?;
            if let Some(plan_id) = apply {
                let plan = load_install_plan(&ledger, &plan_id)?;
                anyhow::ensure!(
                    plan.owner_key == owner && plan.source == source.as_str(),
                    "installation plan scope mismatch"
                );
                anyhow::ensure!(
                    plan.config_identity == config_path_identity(&config)?,
                    "installation plan config identity mismatch"
                );
                let (preimage, _) = config_preimage(&config)?;
                anyhow::ensure!(
                    thruline_core::chain::sha256_hex(&preimage) == plan.config_preimage_sha256,
                    "configuration changed after planning; create a new plan"
                );
                anyhow::ensure!(
                    sha256_file(&source_executable)? == plan.source_executable_sha256,
                    "source executable changed after planning"
                );
                anyhow::ensure!(
                    sha256_file(&wrapper)? == plan.wrapper_sha256,
                    "wrapper changed after planning"
                );
                anyhow::ensure!(
                    source_version == plan.source_version,
                    "source version changed after planning"
                );
                anyhow::ensure!(
                    effective_config_sha256(&plan.proposed_config)? == plan.proposed_config_sha256,
                    "planned configuration digest is invalid"
                );
                let collector = Collector::open(&capture_root, &owner, source.as_str())?;
                let backup_dir = capture_root.join("operations").join("backups");
                create_private_dir(&backup_dir)?;
                let backup = backup_dir.join(format!("{}.json", plan.plan_id));
                write_private_file(&backup, &preimage)?;
                write_config_atomic(&config, &plan.proposed_config)?;
                let applied = capture_config_sha256(&config)?;
                if applied != plan.proposed_config_sha256 {
                    restore_config_bytes(&config, &preimage)?;
                    anyhow::bail!("applied configuration failed digest verification");
                }
                let payload = new_lifecycle_payload(
                    &collector,
                    "installed",
                    Some(&plan),
                    plan.config_identity.clone(),
                    serde_json::json!({"backup_ref": format!("operations/backups/{}.json", plan.plan_id), "authorization_count": 0}),
                )?;
                if let Err(error) = record_lifecycle(&ledger, &collector, &payload) {
                    restore_config_bytes(&config, &preimage)?;
                    return Err(error.into());
                }
                let result = serde_json::json!({
                    "operation": "install_apply",
                    "plan_id": plan.plan_id,
                    "operational_state": "installed_unverified",
                    "authorized_projects": 0,
                    "config_sha256": applied,
                    "matcher": ["Bash"],
                    "historical_evidence_retained": true
                });
                print_capture_result(&json_out::CAPTURE_INSTALL, &result, json)?;
            } else {
                let (preimage, root) = config_preimage(&config)?;
                let proposed = proposed_capture_config(
                    root,
                    &CaptureConfigCommand {
                        config: &config,
                        executable: &wrapper,
                        capture_root: &capture_root,
                        owner: &owner,
                        source_version: &source_version,
                        source_executable: &source_executable,
                        db,
                    },
                )?;
                let plan = create_install_plan(
                    &ledger,
                    InstallPlanInput {
                        owner_key: owner,
                        source: source.as_str().to_string(),
                        config_identity: config_path_identity(&config)?,
                        config_preimage_sha256: thruline_core::chain::sha256_hex(&preimage),
                        proposed_config_sha256: effective_config_sha256(&proposed)?,
                        source_version,
                        source_executable_sha256: sha256_file(&source_executable)?,
                        wrapper_sha256: sha256_file(&wrapper)?,
                        matcher: vec!["Bash".to_string()],
                        proposed_config: proposed,
                    },
                )?;
                let result = serde_json::json!({
                    "operation": "install_plan",
                    "plan_id": plan.plan_id,
                    "config_preimage_sha256": plan.config_preimage_sha256,
                    "proposed_config_sha256": plan.proposed_config_sha256,
                    "source": plan.source,
                    "source_version": plan.source_version,
                    "matcher": plan.matcher,
                    "apply": format!("thruline capture install … --apply {}", plan.plan_id),
                    "configuration_modified": false,
                    "invalidation_rule": "any config, source executable, source version, or wrapper change"
                });
                print_capture_result(&json_out::CAPTURE_INSTALL, &result, json)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Status {
            source,
            config,
            capture_root,
            owner,
            source_version,
            source_executable,
            project_key,
            json,
        } => {
            let owner = canonical_codex_owner(&config, &owner)?;
            let ledger = Ledger::open(db).context("opening ledger")?;
            let input = inspect_readiness(
                &config,
                &capture_root,
                &owner,
                source,
                &source_version,
                &source_executable,
                project_key,
            )?;
            let report = derive_readiness(&ledger, &input)?;
            let result = serde_json::to_value(report)?;
            print_capture_result(&json_out::CAPTURE_STATUS, &result, json)?;
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::SelfTest {
            source,
            config,
            capture_root,
            owner,
            source_version,
            source_executable,
        } => {
            anyhow::ensure!(source == CaptureSource::Codex, "Claude remains bounded-only until its controlled local execution boundary is proven");
            let owner = canonical_codex_owner(&config, &owner)?;
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(&capture_root, &owner, source.as_str())?;
            let wrapper = std::env::current_exe().context("resolving wrapper executable")?;
            let measured = run_controlled_codex_self_test(
                &source_executable,
                &source_version,
                &config,
                &capture_root,
                &owner,
            )?;
            let config_sha = capture_config_sha256(&config)?;
            let test = new_self_test_payload(
                &collector,
                NewSelfTestInput {
                    source_version: source_version.clone(),
                    source_executable_sha256: sha256_file(&source_executable)?,
                    config_sha256: config_sha.clone(),
                    wrapper_sha256: sha256_file(&wrapper)?,
                    native_boundary: measured.native_boundary,
                    exit_zero_preserved: measured.exit_zero_preserved,
                    exit_seven_preserved: measured.exit_seven_preserved,
                    completion_pairing: measured.completion_pairing,
                    cleanup_complete: measured.cleanup_complete,
                    detail: measured.detail.clone(),
                },
            )?;
            let self_test_id = record_self_test(&ledger, &collector, &test)?;
            if !test.passed() {
                eprintln!("self_test        {self_test_id}");
                eprintln!("detail           {}", serde_json::to_string(&test.detail)?);
                eprintln!("state            installed_unverified");
                return Ok(ExitCode::FAILURE);
            }
            let measurement = SourceAssuranceMeasurement {
                source: source.as_str().to_string(),
                source_version: source_version.clone(),
                config_sha256: config_sha,
                matcher: "Bash".to_string(),
                pre_blocks: measured.native_boundary,
                rewrite_exact: measured.native_boundary,
                stable_tool_identity: measured.native_boundary,
                completion_pairing: measured.completion_pairing,
                measured_pre_delay_ms: 1,
                child_started_after_pre: measured.native_boundary,
                wrapper_exit_code: 7,
                source_exit_code: 7,
            };
            let attestation_id = record_source_assurance(&ledger, &collector, &measurement)?;
            let profile = new_compatibility_profile(&test, self_test_id.clone(), true)?;
            let profile_id = record_compatibility_profile(&ledger, &collector, &profile)?;
            println!("self_test        {self_test_id}");
            println!("profile          {profile_id}");
            println!("attestation      {attestation_id}");
            println!("state            installed_unverified");
            println!("next             authorize a project explicitly");
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Authorize {
            source,
            capture_root,
            owner,
            project_key,
            reason,
            revoke,
        } => {
            anyhow::ensure!(
                source == CaptureSource::Codex || revoke,
                "Claude authorization cannot grant assured authority"
            );
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(capture_root, &owner, source.as_str())?;
            let id = record_authorization(&ledger, &collector, &project_key, !revoke, &reason)?;
            println!("authorization    {id}");
            println!("project          {project_key}");
            println!(
                "decision         {}",
                if revoke { "revoked" } else { "authorized" }
            );
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Disable {
            source,
            config,
            capture_root,
            owner,
            reason,
        } => {
            let owner = canonical_codex_owner(&config, &owner)?;
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(capture_root, &owner, source.as_str())?;
            let payload = new_lifecycle_payload(
                &collector,
                "disabled",
                None,
                config_path_identity(&config)?,
                serde_json::json!({"reason": reason, "historical_evidence_retained": true}),
            )?;
            let id = record_lifecycle(&ledger, &collector, &payload)?;
            println!("disabled         {id}");
            println!("history          retained");
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Uninstall {
            source,
            config,
            capture_root,
            owner,
        } => {
            let owner = canonical_codex_owner(&config, &owner)?;
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(capture_root, &owner, source.as_str())?;
            let (preimage, root) = config_preimage(&config)?;
            let config_identity = config_path_identity(&config)?;
            let plan_id: String = ledger
                .conn()
                .query_row(
                    "SELECT plan_id FROM capture_lifecycle_records
                     WHERE owner_key=?1 AND source=?2 AND config_identity=?3
                       AND plan_id IS NOT NULL
                     ORDER BY accepted_seq DESC LIMIT 1",
                    [owner.as_str(), source.as_str(), config_identity.as_str()],
                    |row| row.get(0),
                )
                .context("no accepted receipt-owned installation exists for this configuration")?;
            let plan = load_install_plan(&ledger, &plan_id)?;
            anyhow::ensure!(
                plan.owner_key == owner
                    && plan.source == source.as_str()
                    && plan.config_identity == config_identity,
                "accepted installation plan does not match uninstall scope"
            );
            anyhow::ensure!(
                effective_config_sha256(&root)? == plan.proposed_config_sha256,
                "source configuration changed since installation; refusing uninstall and requiring a fresh plan"
            );
            let (removed_config, removed) =
                remove_receipt_owned_capture_hooks(root, &plan.proposed_config)?;
            write_config_atomic(&config, &removed_config)?;
            let mut payload = new_lifecycle_payload(
                &collector,
                "uninstalled",
                None,
                config_identity,
                serde_json::json!({"removed_entries": removed, "historical_evidence_retained": true}),
            )?;
            payload.config_sha256 = Some(effective_config_sha256(&removed_config)?);
            if let Err(error) = record_lifecycle(&ledger, &collector, &payload) {
                restore_config_bytes(&config, &preimage)?;
                return Err(error.into());
            }
            println!("removed          {removed} exact managed hook entry/entries");
            println!("history          retained");
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Hook {
            event,
            config,
            capture_root,
            owner,
            source_version,
            source_executable,
        } => {
            let owner = canonical_codex_owner(&config, &owner)?;
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .context("reading source hook payload")?;
            let payload: serde_json::Value =
                serde_json::from_slice(&bytes).context("parsing source hook payload")?;
            let config_sha = capture_config_sha256(&config)?;
            let collector = Collector::open(&capture_root, &owner, "codex")
                .context("opening capture collector")?;
            match event {
                HookEvent::Pre => {
                    let tool_name = hook_string(&payload, &["tool_name", "toolName"])
                        .context("PRE hook has no tool name")?;
                    anyhow::ensure!(
                        tool_name == "Bash",
                        "capture matcher self-test failed closed: expected Bash, got {tool_name}"
                    );
                    let tool_input = payload
                        .get("tool_input")
                        .or_else(|| payload.get("toolInput"))
                        .context("PRE hook has no tool input")?;
                    let command = hook_string(tool_input, &["command"])
                        .context("Bash PRE hook has no command")?;
                    let cwd = hook_string(&payload, &["cwd"]).context("PRE hook has no cwd")?;
                    let session_key = hook_string(
                        &payload,
                        &[
                            "session_id",
                            "sessionId",
                            "conversation_id",
                            "conversationId",
                        ],
                    )
                    .context("PRE hook has no stable session identity")?;
                    let tool_id = hook_string(
                        &payload,
                        &["tool_use_id", "toolUseId", "tool_call_id", "toolCallId"],
                    )
                    .context("PRE hook has no stable tool-use identity")?;
                    let native_turn_id = hook_string(&payload, &["turn_id", "turnId"])
                        .context("PRE hook has no stable turn identity")?;
                    let owner_session_id = thruline_core::adapter::stable_source_session_id(
                        &owner,
                        Source::Codex,
                        &session_key,
                    );
                    let source_turn_id = thruline_core::codex::source_turn_identity(
                        &owner_session_id,
                        &format!("turn:{native_turn_id}"),
                    )
                    .context("PRE hook turn identity is invalid")?;
                    let native_project =
                        hook_string(&payload, &["source_project_key", "project_id", "projectId"])
                            .unwrap_or_else(|| cwd.clone());
                    let project_key = thruline_core::capture::opaque_source_project_key(
                        &owner,
                        "codex",
                        &native_project,
                    );
                    let nonce = sha256_hex(
                        format!(
                            "m5r1-wrapper-nonce\0{session_key}\0{source_turn_id}\0{tool_id}\0{config_sha}"
                        )
                            .as_bytes(),
                    );
                    let ledger = Ledger::open(db).context("opening ledger for hook health")?;
                    let source_enabled = source_assurance_enabled(
                        &ledger,
                        &owner,
                        "codex",
                        &source_version,
                        &config_sha,
                    )?;
                    let mut readiness = inspect_readiness(
                        &config,
                        &capture_root,
                        &owner,
                        CaptureSource::Codex,
                        &source_version,
                        &source_executable,
                        Some(project_key.clone()),
                    )?;
                    readiness.source_pairing_complete = true;
                    let enabled = self_test_override_active(&capture_root)?
                        || (source_enabled && operational_assurance_enabled(&ledger, &readiness)?);
                    if !enabled {
                        println!(
                            "{}",
                            serde_json::to_string(&serde_json::json!({
                                "hookSpecificOutput": {
                                    "hookEventName": "PreToolUse",
                                    "permissionDecision": "allow"
                                },
                                "capture": "inactive"
                            }))?
                        );
                        return Ok(ExitCode::SUCCESS);
                    }
                    let invocation = collector.write_invocation(CaptureRequest {
                        owner_key: owner,
                        source: "codex".to_string(),
                        source_project_key: project_key,
                        source_session_key: session_key,
                        source_turn_id: Some(source_turn_id),
                        source_tool_id: tool_id,
                        wrapper_nonce: nonce,
                        command,
                        cwd: PathBuf::from(cwd),
                        source_config_sha256: config_sha,
                        source_version,
                        assurance_enabled: enabled,
                    })?;
                    let executable =
                        std::env::current_exe().context("resolving wrapper executable")?;
                    let rewritten = format!(
                        "{} capture exec --capture-root {} --invocation {}",
                        shell_quote(&executable.display().to_string()),
                        shell_quote(&capture_root.display().to_string()),
                        shell_quote(&invocation.invocation_id),
                    );
                    println!(
                        "{}",
                        serde_json::to_string(&serde_json::json!({
                            "hookSpecificOutput": {
                                "hookEventName": "PreToolUse",
                                "permissionDecision": "allow",
                                "updatedInput": {"command": rewritten}
                            }
                        }))?
                    );
                    Ok(ExitCode::SUCCESS)
                }
                HookEvent::Post => {
                    let session_key = hook_string(
                        &payload,
                        &[
                            "session_id",
                            "sessionId",
                            "conversation_id",
                            "conversationId",
                        ],
                    )
                    .context("POST hook has no stable session identity")?;
                    let tool_id = hook_string(
                        &payload,
                        &["tool_use_id", "toolUseId", "tool_call_id", "toolCallId"],
                    )
                    .context("POST hook has no stable tool-use identity")?;
                    let native_turn_id = hook_string(&payload, &["turn_id", "turnId"])
                        .context("POST hook has no stable turn identity")?;
                    let owner_session_id = thruline_core::adapter::stable_source_session_id(
                        &owner,
                        Source::Codex,
                        &session_key,
                    );
                    let source_turn_id = thruline_core::codex::source_turn_identity(
                        &owner_session_id,
                        &format!("turn:{native_turn_id}"),
                    )
                    .context("POST hook turn identity is invalid")?;
                    let delivery = match collector.record_hook_delivery(
                        &session_key,
                        Some(&source_turn_id),
                        &tool_id,
                    ) {
                        Ok(value) => value,
                        Err(_) => {
                            println!(
                                "{}",
                                serde_json::to_string(&serde_json::json!({
                                    "capture": "inactive",
                                    "authority": "none"
                                }))?
                            );
                            return Ok(ExitCode::SUCCESS);
                        }
                    };
                    println!(
                        "{}",
                        serde_json::to_string(&serde_json::json!({
                            "capture_attempt_id": delivery.attempt_id,
                            "authority": "pending_source_event_pair"
                        }))?
                    );
                    Ok(ExitCode::SUCCESS)
                }
            }
        }
        CaptureCommand::Exec {
            capture_root,
            invocation,
        } => {
            // The signed envelope supplies the collector scope; inspect only
            // that one bounded private record before reopening under its key.
            let path = capture_root
                .join("invocations")
                .join(format!("{invocation}.json"));
            let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            anyhow::ensure!(
                bytes.len() <= 64 * 1024,
                "invocation envelope exceeds 64 KiB"
            );
            let envelope: thruline_core::capture::SignedInvocation =
                serde_json::from_slice(&bytes).context("parsing invocation envelope")?;
            let collector = Collector::open(
                &capture_root,
                &envelope.request.owner_key,
                &envelope.request.source,
            )?;
            let verified = collector.load_invocation(&invocation)?;
            let outcome = collector.execute(&verified.request)?;
            Ok(ExitCode::from(outcome.exit_code.clamp(0, 255) as u8))
        }
        CaptureCommand::Recover {
            capture_root,
            owner,
            source,
        } => {
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(capture_root, &owner, source.as_str())?;
            let recovered = collector.recover_orphans()?;
            let payload = new_lifecycle_payload(
                &collector,
                "recovered",
                None,
                "recovery-only".to_string(),
                serde_json::json!({"orphaned_attempts_closed": recovered, "authority": "zero"}),
            )?;
            let receipt = record_lifecycle(&ledger, &collector, &payload)?;
            println!("orphaned         {recovered}");
            println!("authority        zero");
            println!("receipt          {receipt}");
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Observe {
            capture_root,
            owner,
            source,
            project_key,
            session_key,
            cwd,
            source_version,
            config_sha256,
            profile_command,
        } => {
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(&capture_root, &owner, source.as_str())?;
            let enabled = source_assurance_enabled(
                &ledger,
                &owner,
                source.as_str(),
                &source_version,
                &config_sha256,
            )?;
            let nonce = sha256_hex(
                format!("m5r1-observation\0{project_key}\0{session_key}\0{config_sha256}")
                    .as_bytes(),
            );
            let observation = collector.observe(&CaptureRequest {
                owner_key: owner,
                source: source.as_str().to_string(),
                source_project_key: project_key,
                source_session_key: session_key,
                source_turn_id: None,
                source_tool_id: format!("observation-{nonce}"),
                wrapper_nonce: nonce,
                command: profile_command,
                cwd,
                source_config_sha256: config_sha256,
                source_version,
                assurance_enabled: enabled,
            })?;
            let publication = accept_observation_and_publish(&ledger, &collector, &observation)?;
            println!("capsule          {}", observation.capsule.capsule_id);
            println!(
                "strength         {}",
                observation.capsule.payload.strength.as_str()
            );
            println!(
                "complete         {}",
                observation.capsule.payload.state.completeness.complete()
            );
            println!("comparisons      {}", publication.comparison_count);
            println!("current          {}", publication.current_count);
            println!("stale            {}", publication.stale_count);
            println!("unknown          {}", publication.unknown_count);
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Doctor {
            config,
            capture_root,
            owner,
            source,
            source_version,
            source_executable,
            project_key,
            json,
        } => {
            let owner = canonical_codex_owner(&config, &owner)?;
            let ledger = Ledger::open(db).context("opening ledger")?;
            let input = inspect_readiness(
                &config,
                &capture_root,
                &owner,
                source,
                &source_version,
                &source_executable,
                project_key,
            )?;
            let report = derive_readiness(&ledger, &input)?;
            let reason_code = report.state.as_str();
            let remediation = match report.state.as_str() {
                "not_available" => "Install or select the exact supported local source binary.",
                "available_not_installed" => "Review a fresh `capture install` plan, then apply its exact id.",
                "installed_unverified" => "Run the isolated `capture self-test`, then authorize one project.",
                "degraded_source_version" => "Restore the admitted source executable; compatibility never inherits.",
                "degraded_configuration_drift" => "Review a fresh install plan; no silent repair is available.",
                "degraded_key_or_identity" => "Restore the original private collector key or use an explicit later recovery workflow.",
                "degraded_source_pairing" => "Inspect private source delivery receipts; do not invent a pairing.",
                "disabled" => "Review a new exact installation plan before re-enabling.",
                "bounded_only" => "Use explicit bounded observations; this source cannot emit assured.",
                "capture_error" => "Inspect the private attempt receipt and run `capture recover`; the original command is never rerun.",
                _ => "No remediation is required for admitted commands.",
            };
            let value = serde_json::json!({
                "diagnosis": report,
                "reason_code": reason_code,
                "remediation": remediation,
                "mutation_performed": false,
                "network_used": false
            });
            print_capture_result(&json_out::CAPTURE_DOCTOR, &value, json)?;
            let enabled = source_assurance_enabled(
                &ledger,
                &owner,
                source.as_str(),
                &source_version,
                input.config_sha256.as_deref().unwrap_or(""),
            )?;
            let _ = enabled;
            Ok(ExitCode::SUCCESS)
        }
        CaptureCommand::Pair {
            capture_root,
            owner,
            source,
            attempt,
            event_id,
            exit_code,
            signal,
        } => {
            let ledger = Ledger::open(db).context("opening ledger")?;
            let collector = Collector::open(&capture_root, &owner, source.as_str())?;
            let hook = collector.load_hook_delivery(&attempt)?;
            let outcome = collector.load_execution(&attempt)?;
            let delivery = SourceDelivery {
                schema_version: 1,
                attempt_id: attempt.clone(),
                owner_key: hook.owner_key,
                source: hook.source,
                source_project_key: hook.source_project_key,
                source_session_key: hook.source_session_key,
                source_turn_id: hook.source_turn_id,
                source_tool_id: hook.source_tool_id,
                wrapper_nonce: hook.wrapper_nonce,
                source_event_id: event_id,
                source_exit_code: exit_code,
                source_term_signal: signal,
                observed_at_unix_ms: hook.observed_at_unix_ms,
            };
            collector.write_delivery(&delivery)?;
            let pre = outcome.pre.as_ref().context("wrapper receipt has no PRE")?;
            let post = outcome
                .post
                .as_ref()
                .context("wrapper receipt has no POST")?;
            let accepted = accept_capture_pair(
                &ledger,
                &collector,
                &CaptureAcceptance {
                    outcome: &outcome,
                    delivery: &delivery,
                    pre_private_relative_ref: format!(
                        "sealed/{attempt}/pre.{}.json",
                        pre.capsule_id
                    ),
                    post_private_relative_ref: format!(
                        "sealed/{attempt}/post.{}.json",
                        post.capsule_id
                    ),
                },
            )?;
            println!("attempt          {}", accepted.attempt_id);
            println!("event            {}", accepted.event_id);
            println!("strength         {}", accepted.strength.as_str());
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn hook_string(value: &serde_json::Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(serde_json::Value::as_str))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn canonical_codex_owner(config: &Path, supplied: &str) -> Result<String> {
    let owner_root = config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical = thruline_core::adapter::stable_owner_key(owner_root)?;
    anyhow::ensure!(
        supplied == canonical,
        "capture owner does not match canonical local Codex owner ({canonical})"
    );
    Ok(canonical)
}

#[derive(Debug, Clone)]
struct ControlledSelfTestOutcome {
    native_boundary: bool,
    exit_zero_preserved: bool,
    exit_seven_preserved: bool,
    completion_pairing: bool,
    cleanup_complete: bool,
    detail: serde_json::Value,
}

fn detected_source_version(executable: &Path) -> Result<String> {
    let output = std::process::Command::new(executable)
        .arg("--version")
        .output()
        .with_context(|| format!("running {} --version", executable.display()))?;
    anyhow::ensure!(output.status.success(), "source version probe failed");
    let text = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(text
        .strip_prefix("codex-cli ")
        .unwrap_or(text.as_str())
        .to_string())
}

fn inspect_readiness(
    config: &Path,
    capture_root: &Path,
    owner: &str,
    source: CaptureSource,
    supplied_version: &str,
    source_executable: &Path,
    project_key: Option<String>,
) -> Result<thruline_core::capture_product::CaptureReadinessInput> {
    let source_present = source_executable.is_file();
    let source_version = if source_present {
        Some(
            detected_source_version(source_executable)
                .unwrap_or_else(|_| supplied_version.to_string()),
        )
    } else {
        None
    };
    let source_executable_sha256 = if source_present {
        Some(sha256_file(source_executable)?)
    } else {
        None
    };
    let (config_sha256, matcher_valid, duplicate_owned_entries) = if config.is_file() {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(config)?)?;
        let managed = managed_hook_inventory(&value);
        (
            Some(capture_config_sha256(config)?),
            managed.len() == 2
                && managed.iter().all(|(event, matcher)| {
                    matches!(event.as_str(), "PreToolUse" | "PostToolUse") && matcher == "Bash"
                }),
            managed.len() > 2,
        )
    } else {
        (None, false, false)
    };
    let wrapper = std::env::current_exe().context("resolving wrapper executable")?;
    let inspection =
        thruline_core::capture::Collector::inspect(capture_root, owner, source.as_str())?;
    Ok(thruline_core::capture_product::CaptureReadinessInput {
        owner_key: owner.to_string(),
        source: source.as_str().to_string(),
        source_project_key: project_key,
        source_present,
        source_version,
        source_executable_sha256,
        config_identity: config_path_identity(config)?,
        config_sha256,
        wrapper_sha256: Some(sha256_file(&wrapper)?),
        collector_key_id: inspection.key_id,
        collector_permissions_valid: inspection.permissions_valid,
        matcher_valid,
        duplicate_owned_entries,
        source_pairing_complete: true,
        selected_command_profile_supported: true,
        latest_capture_error: false,
    })
}

fn managed_hook_inventory(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(events) = value.get("hooks").and_then(serde_json::Value::as_object) {
        for (event, entries) in events {
            for entry in entries.as_array().into_iter().flatten() {
                if is_managed_hook(entry) {
                    out.push((
                        event.clone(),
                        entry
                            .get("matcher")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ));
                }
            }
        }
    }
    out
}

/// `capture install` / `status` / `doctor` all render one `Value`; the shape
/// they render it *as* differs, so it is passed in rather than assumed.
/// `capture install` emits two payloads (`operation: install_plan` and
/// `install_apply`) under one shape id, discriminated by that field.
fn print_capture_result(
    shape: &json_out::Shape,
    value: &serde_json::Value,
    json: bool,
) -> Result<()> {
    if json {
        json_out::print(shape, value.clone())?;
        return Ok(());
    }
    if let Some(object) = value.as_object() {
        for (key, item) in object {
            let rendered = item
                .as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| item.to_string());
            println!("{key:<20} {rendered}");
        }
    } else {
        println!("{value}");
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        anyhow::ensure!(
            fs::read(path)? == bytes,
            "conflicting immutable private receipt"
        );
        return Ok(());
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn restore_config_bytes(config: &Path, bytes: &[u8]) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    write_config_atomic(config, &value)
}

fn self_test_override_active(capture_root: &Path) -> Result<bool> {
    let token = match std::env::var("THRULINE_CAPTURE_SELF_TEST_TOKEN") {
        Ok(value) if !value.is_empty() => value,
        _ => return Ok(false),
    };
    let digest = thruline_core::chain::sha256_hex(token.as_bytes());
    let path = capture_root
        .join("operations")
        .join("self-test")
        .join(format!("{digest}.token"));
    if !path.is_file() {
        return Ok(false);
    }
    Ok(fs::read(path)? == token.as_bytes())
}

fn run_controlled_codex_self_test(
    source_executable: &Path,
    source_version: &str,
    config: &Path,
    capture_root: &Path,
    owner: &str,
) -> Result<ControlledSelfTestOutcome> {
    let detected = detected_source_version(source_executable)?;
    anyhow::ensure!(
        detected == source_version,
        "source version differs from the requested exact profile"
    );
    anyhow::ensure!(
        source_version == thruline_core::capture_product::QUALIFIED_CODEX_VERSION,
        "source version is not in the closed-world compatibility profile"
    );
    let installed: serde_json::Value = serde_json::from_slice(&fs::read(config)?)?;
    let inventory = managed_hook_inventory(&installed);
    anyhow::ensure!(
        inventory
            == [
                ("PreToolUse".to_string(), "Bash".to_string()),
                ("PostToolUse".to_string(), "Bash".to_string())
            ]
            || inventory
                == [
                    ("PostToolUse".to_string(), "Bash".to_string()),
                    ("PreToolUse".to_string(), "Bash".to_string())
                ],
        "exact PRE/POST Bash matcher set is not installed"
    );
    let pre_command = managed_hook_command(&installed, "PreToolUse")?;
    let post_command = managed_hook_command(&installed, "PostToolUse")?;
    let hook_override = format!(
        "hooks={{PreToolUse=[{{matcher=\"Bash\",hooks=[{{type=\"command\",command=\"{}\"}}]}}],PostToolUse=[{{matcher=\"Bash\",hooks=[{{type=\"command\",command=\"{}\"}}]}}]}}",
        toml_string(&pre_command),
        toml_string(&post_command),
    );

    let home = tempfile::tempdir().context("creating isolated Codex home")?;
    let repository = tempfile::tempdir().context("creating isolated self-test repository")?;
    run_git(repository.path(), &["init", "-q"])?;
    run_git(
        repository.path(),
        &["config", "user.name", "ThruLine Self Test"],
    )?;
    run_git(
        repository.path(),
        &["config", "user.email", "self-test@example.invalid"],
    )?;
    fs::write(repository.path().join("tracked.txt"), b"self-test\n")?;
    run_git(repository.path(), &["add", "tracked.txt"])?;
    run_git(repository.path(), &["commit", "-qm", "self-test fixture"])?;

    let tools = tempfile::tempdir().context("creating isolated self-test tools")?;
    let cargo_stub = tools.path().join("cargo");
    fs::write(
        &cargo_stub,
        b"#!/bin/sh\nif [ \"${1:-}\" = \"--version\" ]; then echo 'cargo 1.0.0 (m5r2)'; exit 0; fi\nexit 7\n",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cargo_stub, fs::Permissions::from_mode(0o700))?;
    }
    let token = thruline_core::chain::sha256_hex(
        format!(
            "m5r2-native-self-test\0{}\0{}\0{}",
            owner,
            std::process::id(),
            config.display()
        )
        .as_bytes(),
    );
    let token_dir = capture_root.join("operations").join("self-test");
    create_private_dir(&token_dir)?;
    let token_path = token_dir.join(format!(
        "{}.token",
        thruline_core::chain::sha256_hex(token.as_bytes())
    ));
    write_private_file(&token_path, token.as_bytes())?;

    let before = private_attempt_ids(capture_root)?;
    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let path = format!(
        "{}:{}",
        tools.path().display(),
        inherited_path.to_string_lossy()
    );
    let source_run = run_native_codex_commands(
        source_executable,
        home.path(),
        repository.path(),
        &hook_override,
        &token,
        &path,
        &["git status --porcelain".to_string(), "cargo".to_string()],
    )?;
    let after = private_attempt_ids(capture_root)?;
    let new_attempts: Vec<String> = after.difference(&before).cloned().collect();
    let collector = thruline_core::capture::Collector::open(capture_root, owner, "codex")?;
    let mut exits = std::collections::BTreeSet::new();
    let mut complete_pairs = 0usize;
    let mut deliveries = 0usize;
    let mut attempt_details = Vec::new();
    for attempt in &new_attempts {
        let execution = collector.load_execution(attempt)?;
        exits.insert(execution.exit_code);
        let pre_complete = execution
            .pre
            .as_ref()
            .is_some_and(|capsule| capsule.payload.state.completeness.complete());
        let post_complete = execution
            .post
            .as_ref()
            .is_some_and(|capsule| capsule.payload.state.completeness.complete());
        attempt_details.push(serde_json::json!({
            "attempt_sha256": thruline_core::chain::sha256_hex(attempt.as_bytes()),
            "exit_code": execution.exit_code,
            "strength": execution.receipt.strength.as_str(),
            "pre_complete": pre_complete,
            "post_complete": post_complete,
            "pre_failures": execution.pre.as_ref().map(|capsule| &capsule.payload.state.completeness.failures),
            "post_failures": execution.post.as_ref().map(|capsule| &capsule.payload.state.completeness.failures),
            "pre_capture_ns": execution.pre.as_ref().map(|capsule| capsule.payload.capture_duration_ns),
            "post_capture_ns": execution.post.as_ref().map(|capsule| capsule.payload.capture_duration_ns)
        }));
        if pre_complete
            && post_complete
            && execution.receipt.strength == thruline_core::capture::CaptureStrength::Assured
        {
            complete_pairs += 1;
        }
        if delivery_exists(capture_root, attempt)? {
            deliveries += 1;
        }
    }
    fs::remove_file(&token_path)?;
    let home_path = home.path().to_path_buf();
    let repository_path = repository.path().to_path_buf();
    let tools_path = tools.path().to_path_buf();
    home.close()?;
    repository.close()?;
    tools.close()?;
    let cleanup_complete = !token_path.exists()
        && !home_path.exists()
        && !repository_path.exists()
        && !tools_path.exists();
    let native_boundary =
        source_run.status.success() && new_attempts.len() == 2 && complete_pairs == 2;
    let exit_zero_preserved = exits.contains(&0);
    let exit_seven_preserved = exits.contains(&7) && deliveries == 2 && source_run.status.success();
    let completion_pairing = deliveries == 2;
    Ok(ControlledSelfTestOutcome {
        native_boundary,
        exit_zero_preserved,
        exit_seven_preserved,
        completion_pairing,
        cleanup_complete,
        detail: serde_json::json!({
            "source_binary_invoked": true,
            "source_version": detected,
            "isolated_model_transport": "loopback_only",
            "external_network_blocked": true,
            "native_attempts": new_attempts.len(),
            "complete_pre_post_pairs": complete_pairs,
            "native_deliveries": deliveries,
            "attempts": attempt_details,
            "exit_codes": exits,
            "duplicate_attempts": new_attempts.len().saturating_sub(2),
            "source_run_stdout_sha256": thruline_core::chain::sha256_hex(&source_run.stdout),
            "temporary_paths_removed": cleanup_complete,
            "normal_user_config_touched": false,
            "v3_interaction": false
        }),
    })
}

fn managed_hook_command(value: &serde_json::Value, event: &str) -> Result<String> {
    let entries = value
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(serde_json::Value::as_array)
        .context("managed hook event is absent")?;
    let commands: Vec<&str> = entries
        .iter()
        .filter(|entry| is_managed_hook(entry))
        .filter_map(|entry| entry.get("hooks"))
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter_map(|hook| hook.get("command"))
        .filter_map(serde_json::Value::as_str)
        .collect();
    anyhow::ensure!(
        commands.len() == 1,
        "managed hook event is missing or ambiguous"
    );
    Ok(commands[0].to_string())
}

fn toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .status()?;
    anyhow::ensure!(status.success(), "self-test git command failed");
    Ok(())
}

fn private_attempt_ids(root: &Path) -> Result<std::collections::BTreeSet<String>> {
    let mut ids = std::collections::BTreeSet::new();
    let sealed = root.join("sealed");
    if !sealed.exists() {
        return Ok(ids);
    }
    for entry in fs::read_dir(sealed)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            ids.insert(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(ids)
}

fn delivery_exists(root: &Path, attempt: &str) -> Result<bool> {
    let deliveries = root.join("deliveries");
    if !deliveries.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(deliveries)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{attempt}."))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run_native_codex_commands(
    source_executable: &Path,
    codex_home: &Path,
    repository: &Path,
    hook_override: &str,
    token: &str,
    path: &str,
    commands: &[String],
) -> Result<std::process::Output> {
    let (endpoint, server) = start_local_responses_server(commands.to_vec())?;
    let provider = format!(
        "model_providers.m5r2_self_test={{name=\"M5.R2 self-test\",base_url=\"{endpoint}/v1\",env_key=\"THRULINE_SELF_TEST_KEY\",wire_api=\"responses\"}}"
    );
    let output = std::process::Command::new(source_executable)
        .args([
            "--disable",
            "plugins",
            "--disable",
            "remote_plugin",
            "--disable",
            "plugin_sharing",
            "--disable",
            "apps",
            "--disable",
            "browser_use",
            "--disable",
            "browser_use_external",
            "--disable",
            "browser_use_full_cdp_access",
            "--disable",
            "computer_use",
            "--disable",
            "goals",
            "--disable",
            "image_generation",
            "--disable",
            "in_app_browser",
            "--disable",
            "multi_agent",
            "--disable",
            "tool_suggest",
            "--disable",
            "workspace_dependencies",
            "--disable",
            "skill_search",
            "--disable",
            "skill_mcp_dependency_install",
            "--dangerously-bypass-hook-trust",
            "-s",
            "danger-full-access",
            "-a",
            "never",
            "-c",
            "model_provider=\"m5r2_self_test\"",
            "-c",
            "model=\"gpt-5.4-nano\"",
            "-c",
            &provider,
            "-c",
            hook_override,
            "exec",
            "--skip-git-repo-check",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "Execute the single supplied shell command and then stop.",
        ])
        .current_dir(repository)
        .env("CODEX_HOME", codex_home)
        .env("THRULINE_SELF_TEST_KEY", "local-only")
        .env("THRULINE_CAPTURE_SELF_TEST_TOKEN", token)
        .env("PATH", path)
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .stdin(std::process::Stdio::null())
        .output()?;
    server
        .join()
        .map_err(|_| anyhow::anyhow!("local self-test server panicked"))??;
    Ok(output)
}

/// How long the self-test fixture waits for the agent to connect.
///
/// A local child that is going to connect does so immediately; 30 s is generous
/// for that and short enough that a child which never connects is a failed
/// self-test rather than a hung `thruline`.
const SELF_TEST_ACCEPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn start_local_responses_server(
    commands: Vec<String>,
) -> Result<(String, std::thread::JoinHandle<Result<()>>)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    // **This is a two-request fixture, not a server**, and it is joined
    // synchronously by its caller — so `accept()` blocking forever is a hang of
    // `thruline` itself, with no diagnosis.
    //
    // The loop ends only if the child opens exactly two connections. A crash, a
    // provider-config error, or a one-turn completion and the second `accept()`
    // never returns; the 5 s `set_read_timeout` below is on the accepted stream
    // and bounds reading a request, not waiting for one. Reproduced: client
    // exits after one request, `join()` still blocked.
    //
    // Same rule as everywhere else here — the Stop gate's watchdog,
    // `reexec/docker.rs`'s container deadline: a wait on something outside this
    // process gets a deadline, and missing it is a named failure.
    listener.set_nonblocking(true)?;
    let handle = std::thread::spawn(move || -> Result<()> {
        for request_index in 0..2 {
            let deadline = std::time::Instant::now() + SELF_TEST_ACCEPT_TIMEOUT;
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            anyhow::bail!(
                                "the self-test agent opened {request_index} of the 2 \
                                 connections this fixture expects, then stopped for \
                                 {}s. It probably exited early — a crash or a \
                                 provider-config error. Failing here rather than \
                                 waiting forever.",
                                SELF_TEST_ACCEPT_TIMEOUT.as_secs()
                            );
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(e) => return Err(e.into()),
                }
            };
            // The accepted stream goes back to blocking: `read_http_request`
            // expects it, and its own read timeout bounds it.
            stream.set_nonblocking(false)?;
            read_http_request(&mut stream)?;
            let (events, output) = if request_index == 0 {
                let mut items = Vec::new();
                for (command_index, command) in commands.iter().enumerate() {
                    items.push(serde_json::json!({
                        "id": format!("fc_m5r2_{command_index}"),
                        "type": "function_call",
                        "status": "completed",
                        "arguments": serde_json::to_string(&serde_json::json!({"cmd": command}))?,
                        "call_id": format!("call_m5r2_{command_index}"),
                        "name": "exec_command"
                    }));
                }
                (response_events_many(&items), items)
            } else {
                let item = serde_json::json!({
                    "id": "msg_m5r2",
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "self-test complete", "annotations": [], "logprobs": []}]
                });
                (
                    response_events_many(std::slice::from_ref(&item)),
                    vec![item],
                )
            };
            let completed = serde_json::json!({
                "type": "response.completed",
                "sequence_number": 2 * output.len() + 1,
                "response": responses_object(output, "completed")
            });
            let mut body = String::new();
            for (name, data) in events {
                body.push_str(&format!(
                    "event: {name}\ndata: {}\n\n",
                    serde_json::to_string(&data)?
                ));
            }
            body.push_str(&format!(
                "event: response.completed\ndata: {}\n\ndata: [DONE]\n\n",
                serde_json::to_string(&completed)?
            ));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )?;
            stream.flush()?;
        }
        Ok(())
    });
    Ok((format!("http://{address}"), handle))
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut buffer)?;
        anyhow::ensure!(count > 0, "self-test model request ended before headers");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        anyhow::ensure!(
            bytes.len() <= 2 * 1024 * 1024,
            "self-test model headers too large"
        );
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
        })
        .unwrap_or(0);
    while bytes.len().saturating_sub(header_end) < content_length {
        let count = stream.read(&mut buffer)?;
        anyhow::ensure!(count > 0, "self-test model request body was truncated");
        bytes.extend_from_slice(&buffer[..count]);
        anyhow::ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "self-test model request too large"
        );
    }
    Ok(())
}

fn response_events_many(items: &[serde_json::Value]) -> Vec<(&'static str, serde_json::Value)> {
    let mut events = vec![(
        "response.created",
        serde_json::json!({
            "type": "response.created",
            "sequence_number": 0,
            "response": responses_object(Vec::new(), "in_progress")
        }),
    )];
    let mut sequence = 1usize;
    for (output_index, item) in items.iter().enumerate() {
        events.push((
            "response.output_item.added",
            serde_json::json!({
                "type": "response.output_item.added",
                "sequence_number": sequence,
                "output_index": output_index,
                "item": item
            }),
        ));
        sequence += 1;
        events.push((
            "response.output_item.done",
            serde_json::json!({
                "type": "response.output_item.done",
                "sequence_number": sequence,
                "output_index": output_index,
                "item": item
            }),
        ));
        sequence += 1;
    }
    events
}

fn responses_object(output: Vec<serde_json::Value>, status: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "resp_m5r2", "object": "response", "created_at": 0,
        "status": status, "background": false, "error": null,
        "incomplete_details": null, "instructions": null, "max_output_tokens": null,
        "max_tool_calls": null, "model": "gpt-5.4-nano", "output": output,
        "parallel_tool_calls": true, "previous_response_id": null,
        "prompt_cache_key": null, "reasoning": {"effort": null, "summary": null},
        "safety_identifier": null, "service_tier": "default", "store": false,
        "temperature": null, "text": {"format": {"type": "text"}, "verbosity": "low"},
        "tool_choice": "auto", "tools": [], "top_logprobs": 0, "top_p": null,
        "truncation": "disabled", "usage": null, "user": null, "metadata": {}
    })
}

fn capture_config_sha256(config: &Path) -> Result<String> {
    let config_bytes =
        fs::read(config).with_context(|| format!("reading hook config {}", config.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&config_bytes)?;
    let mut handler_digests = Vec::new();
    if let Some(events) = value.get("hooks").and_then(serde_json::Value::as_object) {
        for entries in events.values().filter_map(serde_json::Value::as_array) {
            for command in entries
                .iter()
                .filter_map(|entry| entry.get("hooks"))
                .filter_map(serde_json::Value::as_array)
                .flatten()
                .filter_map(|hook| hook.get("command"))
                .filter_map(serde_json::Value::as_str)
            {
                let executable = first_shell_token(command);
                let path = Path::new(&executable);
                if path.is_absolute() && path.is_file() {
                    handler_digests.push(thruline_core::chain::sha256_hex(&fs::read(path)?));
                }
            }
        }
    }
    handler_digests.sort();
    let mut material = Vec::new();
    material.extend_from_slice(&(config_bytes.len() as u64).to_le_bytes());
    material.extend_from_slice(&config_bytes);
    for digest in handler_digests {
        material.extend_from_slice(&(digest.len() as u64).to_le_bytes());
        material.extend_from_slice(digest.as_bytes());
    }
    Ok(thruline_core::chain::sha256_hex(&material))
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(thruline_core::chain::sha256_hex(
        &fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    ))
}

fn config_path_identity(path: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(thruline_core::chain::sha256_hex(
        absolute.to_string_lossy().as_bytes(),
    ))
}

fn config_preimage(path: &Path) -> Result<(Vec<u8>, serde_json::Value)> {
    let bytes = if path.exists() {
        fs::read(path).with_context(|| format!("reading {}", path.display()))?
    } else {
        b"{}\n".to_vec()
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    anyhow::ensure!(
        value.is_object(),
        "hook configuration root must be an object"
    );
    Ok((bytes, value))
}

fn effective_config_sha256(value: &serde_json::Value) -> Result<String> {
    let config_bytes = serde_json::to_vec_pretty(value)?;
    let mut handler_digests = Vec::new();
    if let Some(events) = value.get("hooks").and_then(serde_json::Value::as_object) {
        for entries in events.values().filter_map(serde_json::Value::as_array) {
            for command in entries
                .iter()
                .filter_map(|entry| entry.get("hooks"))
                .filter_map(serde_json::Value::as_array)
                .flatten()
                .filter_map(|hook| hook.get("command"))
                .filter_map(serde_json::Value::as_str)
            {
                let executable = first_shell_token(command);
                let path = Path::new(&executable);
                if path.is_absolute() && path.is_file() {
                    handler_digests.push(sha256_file(path)?);
                }
            }
        }
    }
    handler_digests.sort();
    let mut material = Vec::new();
    material.extend_from_slice(&(config_bytes.len() as u64).to_le_bytes());
    material.extend_from_slice(&config_bytes);
    for digest in handler_digests {
        material.extend_from_slice(&(digest.len() as u64).to_le_bytes());
        material.extend_from_slice(digest.as_bytes());
    }
    Ok(thruline_core::chain::sha256_hex(&material))
}

fn first_shell_token(command: &str) -> String {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('\'') {
        return rest.split('\'').next().unwrap_or("").to_string();
    }
    if let Some(rest) = command.strip_prefix('"') {
        return rest.split('"').next().unwrap_or("").to_string();
    }
    command.split_whitespace().next().unwrap_or("").to_string()
}

struct CaptureConfigCommand<'a> {
    config: &'a Path,
    executable: &'a Path,
    capture_root: &'a Path,
    owner: &'a str,
    source_version: &'a str,
    source_executable: &'a Path,
    db: &'a Path,
}

fn proposed_capture_config(
    mut root: serde_json::Value,
    command: &CaptureConfigCommand<'_>,
) -> Result<serde_json::Value> {
    let hooks = root
        .as_object_mut()
        .context("hook configuration root must be an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    anyhow::ensure!(hooks.is_object(), "hooks must be an object");
    let command_for = |event: &str| {
        format!(
            "{} --db {} capture hook --event {} --config {} --capture-root {} --owner {} --source-version {} --source-executable {}",
            shell_quote(&command.executable.display().to_string()),
            shell_quote(&command.db.display().to_string()),
            event,
            shell_quote(&command.config.display().to_string()),
            shell_quote(&command.capture_root.display().to_string()),
            shell_quote(command.owner),
            shell_quote(command.source_version),
            shell_quote(&command.source_executable.display().to_string()),
        )
    };
    for (event, cli_event) in [("PreToolUse", "pre"), ("PostToolUse", "post")] {
        let entries = hooks
            .as_object_mut()
            .context("hooks must be an object")?
            .entry(event)
            .or_insert_with(|| serde_json::json!([]));
        let array = entries
            .as_array_mut()
            .context("hook event must be an array")?;
        array.retain(|entry| !is_managed_hook(entry));
        array.push(serde_json::json!({
            "matcher": "Bash",
            "hooks": [{
                "type": "command",
                "command": command_for(cli_event)
            }]
        }));
    }
    Ok(root)
}

/// A187 — the adjudication instrument.
///
/// **It renders; the human decides.** There is no default verdict, no seeded
/// answer, and no pre-filled likely one. A bare Return is not a verdict, and a
/// run with no human on the other end of stdin refuses to record anything at
/// all — an adjudication with no human behind it launders a guess into the
/// ledger wearing a human's authority, which is worse than no adjudication.
fn run_adjudicate(
    db: &Path,
    dry_run: bool,
    status: bool,
    operator: Option<String>,
) -> Result<ExitCode> {
    use std::io::{BufRead, IsTerminal, Write};
    use thruline_core::adjudicate::{self, Verdict};

    let ledger = Ledger::open(db).context("opening the ledger")?;
    let now = thruline_core::ingest::iso8601_utc_now();

    println!("Specimens awaiting a human. ThruLine shows the record and stores what you decide;");
    println!("it does not decide, and it does not suggest.");
    println!();
    print!("  reading the record… ");
    std::io::stdout().flush().ok();
    let specimens = adjudicate::pending(&ledger, &now).context("deriving pending specimens")?;
    println!("{} specimen(s) found", specimens.len());

    // Progress first, so an operator returning to a part-finished batch is told
    // where they stopped before being asked anything.
    let counted = adjudicate::counted_by_class(&ledger).context("counting adjudications")?;
    let mut answered = 0usize;
    for s in &specimens {
        if adjudicate::verdict_for(&ledger, &s.id)?.is_some() {
            answered += 1;
        }
    }
    println!(
        "  {} of {} already carry a verdict; {} still pending",
        answered,
        specimens.len(),
        specimens.len() - answered
    );
    for (class, n) in &counted {
        if class == "crash_candidate" {
            let (bar, short) = thruline_core::guardian::graduation_gap(*n);
            println!("  {class}: {n} counted toward graduation; bar {bar}, {short} short");
        } else {
            println!("  {class}: {n} counted toward graduation; no bar is encoded for this class");
        }
    }
    for (batch, why) in adjudicate::unreachable_batches() {
        println!();
        println!("  NOT SERVED — {batch}");
        println!("    {why}");
    }
    if status {
        return Ok(ExitCode::SUCCESS);
    }

    let who = operator
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "unknown".to_string());

    let interactive = std::io::stdin().is_terminal();
    if !dry_run && !interactive {
        // Rule 12. This is the load-bearing refusal of the whole command.
        println!();
        println!("Nothing was recorded: this is not an interactive terminal, so there is no human");
        println!("here to judge. Re-run attached to a terminal, or use --dry-run to look only.");
        return Ok(ExitCode::SUCCESS);
    }

    let pending: Vec<_> = specimens
        .iter()
        .filter(|s| {
            adjudicate::verdict_for(&ledger, &s.id)
                .map(|v| v.is_none())
                .unwrap_or(true)
        })
        .collect();

    println!();
    if dry_run {
        println!("DRY RUN — every specimen below is rendered and NOTHING is written.");
    } else {
        println!("Recording as: {who}. Answer a, d, or i. Anything else asks again; Ctrl-C stops.");
    }

    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut recorded = 0usize;
    for (i, s) in pending.iter().enumerate() {
        println!();
        println!(
            "  [{}/{}] {} · {}",
            i + 1,
            pending.len(),
            s.class,
            s.subject
        );
        println!("      derived     {}", s.derived_label);
        for e in &s.evidence {
            println!("      evidence    {e}");
        }
        println!("      basis       {}", s.basis);
        if dry_run {
            continue;
        }
        let verdict = loop {
            print!("      your call — (a)gree / (d)isagree / (i)nsufficient evidence: ");
            std::io::stdout().flush().ok();
            let Some(line) = lines.next() else {
                println!();
                println!("  input ended — nothing further was recorded.");
                return Ok(ExitCode::SUCCESS);
            };
            match Verdict::parse(&line.context("reading your answer")?) {
                Some(v) => break v,
                // No default, and no interpretation of an unclear answer.
                None => println!("      not recorded — please type a, d, or i."),
            }
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if adjudicate::record(&ledger, s, verdict, &who, "cli:adjudicate", at)
            .context("recording your verdict")?
        {
            recorded += 1;
            println!("      recorded    {} · {who}", verdict.as_str());
        } else {
            println!("      already carried a verdict — left exactly as it was.");
        }
    }

    println!();
    if dry_run {
        println!("Dry run complete. Nothing was written.");
    } else {
        println!("{recorded} verdict(s) recorded. Re-run to continue where you stopped.");
    }
    Ok(ExitCode::SUCCESS)
}

/// C8 — the Guardian's only surface: shadow diagnostics.
///
/// Candidates, never verdicts; every one carries its reason code, its evidence
/// rows, and the assumption its rule rests on. The classes that cannot be
/// built are printed too, because a detector list that silently omits what it
/// cannot see is the false calm this product exists to refuse.
fn run_guardian(db: &Path) -> Result<ExitCode> {
    use thruline_core::guardian::{self, Readiness};
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let now = thruline_core::ingest::iso8601_utc_now();
    let candidates = guardian::derive(&ledger, &now).context("deriving candidates")?;
    let (covered, total) = guardian::coverage(&ledger).context("reading coverage")?;

    println!("SHADOW DIAGNOSTICS — Guardian phase 1: candidates, not verdicts; nothing here");
    println!("reaches a default surface until its class graduates by adjudication.");
    println!();
    println!(
        "  reach         {covered} of {total} session(s) carry the hook rows these detectors \
read"
    );
    if total > 0 && covered * 1000 < total {
        println!(
            "                that is under 0.1% of the record — every count below is bounded by it"
        );
    }
    println!();
    println!("CLASSES");
    for (class, state) in guardian::readiness() {
        match state {
            Readiness::Implementable => {
                let n = candidates.iter().filter(|c| c.class == class).count();
                println!("  {:<22} buildable · {n} candidate(s)", class.as_str());
            }
            Readiness::NoSignalObserved { why } => {
                println!("  {:<22} NO SIGNAL — {why}", class.as_str())
            }
            Readiness::NotSeparable { why } => {
                println!("  {:<22} NOT SEPARABLE — {why}", class.as_str())
            }
        }
    }
    if candidates.is_empty() {
        println!();
        println!("  no candidate derives from this machine's rows right now");
    }
    for c in &candidates {
        println!();
        println!("  {} · session {}", c.class.as_str(), c.session_id);
        println!("      reason code {}", c.reason_code);
        for e in &c.evidence {
            println!("      evidence    {e}");
        }
        println!("      basis       {}", c.basis);
    }
    let (bar, gap) = guardian::graduation_gap(candidates.len() as u64);
    println!();
    println!("  graduation    {bar} independent held-out adjudications are the bar; {gap} short");
    println!("                every class stays in shadow until that bar is met");
    Ok(ExitCode::SUCCESS)
}

/// C6 — the autopsy: a screenshot-shaped reading of this machine's own
/// records. Measurements and labeled hypotheses, never verdicts: claim words
/// come from the voice authority, session-end lines carry the RB1 shadow
/// discipline in prose, and shadow classes print under their own honest
/// heading. Every number traces to rows; the footer says where.
fn run_autopsy(db: &Path) -> Result<ExitCode> {
    use thruline_core::session_status::SessionStatus;
    let started = std::time::Instant::now();
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let corpus = thruline_core::autopsy::corpus(&ledger).context("reading the corpus")?;
    if corpus.events == 0 {
        println!("No records on this machine yet — nothing to read.");
        println!("Point ThruLine at the transcripts your agents already leave behind:");
        println!("  thruline ingest ~/.claude/projects");
        return Ok(ExitCode::SUCCESS);
    }
    let claims = thruline_core::autopsy::claim_counts(&ledger).context("counting claims")?;
    let reex =
        thruline_core::autopsy::reexplanation(&ledger).context("reading the re-explanation tax")?;
    let now = thruline_core::ingest::iso8601_utc_now();
    let statuses = thruline_core::session_status::derive_all(&ledger, &now)
        .context("deriving session-end hypotheses")?;
    let mut ended: std::collections::BTreeMap<SessionStatus, u64> =
        std::collections::BTreeMap::new();
    for row in &statuses {
        *ended.entry(row.status).or_default() += 1;
    }
    let n = |s: SessionStatus| ended.get(&s).copied().unwrap_or(0);
    let fmt = |v: u64| {
        let s = v.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    // §4 with thousands separators: a count carries its noun, and a large
    // count is still readable. voice::count_phrase pluralizes but does not
    // group digits, so the composition happens here and the banned_in sweep
    // in the test walks every line it produces.
    let phrase = |v: u64, singular: &str, plural: &str| {
        format!("{} {}", fmt(v), if v == 1 { singular } else { plural })
    };
    let day = |ts: &Option<String>| {
        ts.as_deref()
            .map(|t| t.get(..10).unwrap_or(t).to_string())
            .unwrap_or_default()
    };

    println!("THRULINE AUTOPSY — what this machine's own records hold");
    println!(
        "  {} · {} · {} · sessions from {} to {}",
        phrase(corpus.events, "recorded event", "recorded events"),
        phrase(corpus.sessions, "session", "sessions"),
        phrase(corpus.projects, "project", "projects"),
        day(&corpus.first_session),
        day(&corpus.last_session),
    );
    println!();
    println!("HOW SESSIONS ENDED — labeled hypotheses from the records' own shapes, not verdicts (RB1 shadow)");
    // Every line below renders only when its count exists (§4).
    let closed = n(SessionStatus::Completed);
    if closed > 0 {
        println!(
            "  {} closed clean on the vendor's own marker",
            phrase(closed, "session", "sessions")
        );
    }
    let cut = n(SessionStatus::Interrupted);
    if cut > 0 {
        println!(
            "  {} cut off mid-work — the vendor said so, or a later session picks the work back up",
            phrase(cut, "session", "sessions")
        );
    }
    let quiet = n(SessionStatus::Abandoned);
    if quiet > 0 {
        println!(
            "  {} where the record simply stops — no close, no successor, nothing after",
            phrase(quiet, "session", "sessions")
        );
    }
    let fresh = n(SessionStatus::Active) + n(SessionStatus::Idle);
    if fresh > 0 {
        println!(
            "  {} still within a working window when this reading was taken",
            phrase(fresh, "session", "sessions")
        );
    }
    let odd = n(SessionStatus::Unknown);
    if odd > 0 {
        println!(
            "  {} kept unreadable rather than guessed",
            phrase(odd, "session tail", "session tails")
        );
    }
    println!();
    println!("WHAT WAS CLAIMED, AND WHAT THE RECORD HOLDS — claims across every project");
    let word_for = |slug: &str| voice::verdict(slug).map(|t| t.word).unwrap_or("unreadable");
    let public: Vec<String> = [
        "supported",
        "recorded",
        "pending",
        "unknown",
        "contradicted",
        "refused",
    ]
    .iter()
    .filter_map(|slug| {
        claims
            .iter()
            .find(|(s, _)| s == slug)
            .filter(|(_, count)| *count > 0)
            .map(|(_, count)| format!("{} {}", fmt(*count), word_for(slug)))
    })
    .collect();
    if !public.is_empty() {
        println!("  {}", public.join(" · "));
    }
    let shadow: Vec<String> = ["unsupported", "dropped"]
        .iter()
        .filter_map(|slug| {
            claims
                .iter()
                .find(|(s, _)| s == slug)
                .filter(|(_, count)| *count > 0)
                .map(|(_, count)| format!("{} {}", fmt(*count), word_for(slug)))
        })
        .collect();
    if !shadow.is_empty() {
        println!(
            "  shadow diagnostics, classes not yet graduated, never accusations: {}",
            shadow.join(" · ")
        );
    }
    println!();
    println!("THE RE-EXPLANATION TAX");
    if reex.sessions == 0 {
        println!("  no session on record begins by re-explaining an earlier one");
    } else {
        println!(
            "  {} begin by re-explaining an earlier one — ~{} KB of context retyped",
            voice::count_phrase(reex.sessions, "session", "sessions"),
            fmt(reex.preamble_bytes / 1024),
        );
    }
    println!();
    println!(
        "Nothing leaves this device. Read in {:.1} s — every number above traces to rows on this machine: thruline ui",
        started.elapsed().as_secs_f64()
    );
    Ok(ExitCode::SUCCESS)
}

fn continuity_outcome_is_success(outcome: thruline_core::continuity::Outcome) -> bool {
    matches!(outcome, thruline_core::continuity::Outcome::Verified)
}

/// C4.3 — the legacy/local-only continuity comparison. Read-only; it compares
/// and records, but cannot prove recovery without successor evidence.
fn run_verify_continuity(
    db: &Path,
    project: &str,
    capsule_path: &Path,
    root: Option<PathBuf>,
    objective_acknowledged: bool,
    override_reason: Option<String>,
) -> Result<ExitCode> {
    use thruline_core::continuity::{self, CheckState};
    let text = std::fs::read_to_string(capsule_path)
        .with_context(|| format!("reading {}", capsule_path.display()))?;
    let capsule: thruline_core::capsule::Capsule = serde_json::from_str(&text)?;
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let checked_at = thruline_core::ingest::iso8601_utc_now();
    let root = match root {
        Some(r) => r,
        None => std::env::current_dir().context("resolving the current directory")?,
    };
    let fresh = thruline_core::snapshot::capture(&root, &checked_at).ok();
    let mut h = continuity::handshake(
        &ledger,
        &capsule,
        fresh.as_ref(),
        &checked_at,
        objective_acknowledged,
    )
    .context("running the handshake")?;
    if let Some(reason) = &override_reason {
        h = continuity::override_with(&h, reason);
    }
    continuity::record(&ledger, &h).context("recording the handshake")?;

    println!("CONTINUITY HANDSHAKE — {project}");
    for c in &h.checks {
        let mark = match c.state {
            CheckState::Passed => "matched",
            CheckState::Failed => "did not match",
            CheckState::Insufficient => "insufficient evidence",
        };
        println!("  {:<24} {mark}", c.name);
        println!("      {}", c.detail);
    }
    println!();
    println!("  {}", h.coverage());
    match h.outcome {
        continuity::Outcome::Verified => {
            println!("  RECOVERED — continuity verified")
        }
        continuity::Outcome::Insufficient => {
            println!("  CONTINUITY INSUFFICIENT — required evidence is missing");
            println!(
                "      no recovery is established; resolve the insufficient checks before relying on it"
            );
        }
        continuity::Outcome::ResumedWithWarnings => {
            println!("  RESUMED WITH WARNINGS");
            println!(
                "      reason: {}",
                h.override_reason.as_deref().unwrap_or("")
            );
            println!("      the checks that did not match still say so; nothing was rewritten");
        }
        continuity::Outcome::Failed => {
            println!("  CONTINUITY FAILED — a required check did not match");
            println!("      accept it deliberately with --override-reason \"...\"");
        }
    }
    let (v, t) = continuity::verified_continuation_rate(&ledger)?;
    let (legacy_v, legacy_t) = continuity::legacy_local_continuity_rate(&ledger)?;
    println!();
    println!("  P1 verified recovery rate: {v} of {t} protocol-valid successor handshake(s)");
    println!(
        "  legacy local-only audit: {legacy_v} old success label(s) across {legacy_t} row(s); excluded from recovery"
    );
    Ok(if continuity_outcome_is_success(h.outcome) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// C4.2 — rescue: prepare a fresh session, and launch nothing.
///
/// Preview first, always. The preview states what will be read, what will be
/// injected, and **what will not happen** — and injection is staged only after
/// the secret findings are acknowledged, on the gate that fails toward consent
/// by type (D3). ThruLine prepares and prints; the human launches.
fn run_rescue(
    db: &Path,
    project: &str,
    agent: &str,
    out: Option<PathBuf>,
    acknowledge: bool,
    emit_hook: Option<PathBuf>,
) -> Result<ExitCode> {
    // The staged hook calls back into this binary to emit its payload, so the
    // capsule never has to be JSON-escaped by hand in a shell script.
    if let Some(path) = emit_hook {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading the staged capsule at {}", path.display()))?;
        let capsule: thruline_core::capsule::Capsule = serde_json::from_str(&text)?;
        let payload = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": thruline_core::capsule::render_markdown(&capsule),
            }
        });
        println!("{payload}");
        return Ok(ExitCode::SUCCESS);
    }

    // The capability matrix is the only authority for what is offered.
    let (supported, how) = match agent {
        "claude" => (
            true,
            "SessionStart additionalContext, staged as a settings file",
        ),
        "codex" => (true, "a prompt file — codex exposes no injection surface"),
        _ => (false, ""),
    };
    if !supported {
        println!("RESCUE — {agent} is not a launch-grade adapter, so nothing is offered for it");
        println!(
            "  supported     claude · codex   (docs/ADAPTER-CAPABILITIES.md is the authority)"
        );
        return Ok(ExitCode::SUCCESS);
    }

    let out = out.unwrap_or_else(|| PathBuf::from("thruline-rescue"));
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let compiled_at = thruline_core::ingest::iso8601_utc_now();
    let capsule = thruline_core::capsule::compile(&ledger, project, None, &compiled_at)
        .context("compiling the capsule")?;
    let treatment = &capsule.secrets_policy.treatment;

    // A209/§15: the canonical v2 capsule, compiled beside v1 — masked at
    // compile, sealed only after the consent gate below. Rescue observes no
    // workspace by contract, so the v2 workspace identity abstains; the
    // lineage anchor is read from the observation chain, never guessed.
    let v2_lineage = thruline_core::capsule_v2::lineage_for_project(&ledger, project)
        .context("reading the capsule lineage")?;
    let v2_ctx = thruline_core::capsule_v2::CompileContext {
        lineage: v2_lineage,
        ..Default::default()
    };
    let capsule_v2 = thruline_core::capsule_v2::compile(&ledger, project, &v2_ctx, &compiled_at)
        .context("compiling the v2 capsule")?;

    println!("RESCUE PREVIEW — nothing has been launched, killed, or modified");
    println!(
        "  scope         {} · {} session(s)",
        capsule.scope.id, capsule.scope.sessions
    );
    println!(
        "  will read     the recorded sessions for this scope, at chain position {}",
        &capsule.binding.chain_position[..16.min(capsule.binding.chain_position.len())]
    );
    println!(
        "  will stage    a Recovery Capsule (Markdown + JSON) under {}, plus the sealed \
         canonical v2 capsule (§15)",
        out.display()
    );
    println!("  will inject   {how}");
    println!("  will NOT      launch an agent, kill a process, commit, stash, or touch your files");

    // The findings, surfaced before anything is staged. The gate fails
    // toward consent on the UNION of both compiles' treatments.
    println!("  secret scan   {}", treatment.statement());
    println!(
        "  v2 scan       {}",
        capsule_v2.redaction.treatment.statement()
    );
    let needs_ack =
        treatment.needs_acknowledgement() || capsule_v2.redaction.treatment.needs_acknowledgement();
    if needs_ack && !acknowledge {
        println!();
        println!("INJECTION BLOCKED — this capsule needs your acknowledgment before it is staged.");
        if treatment.masked() > 0 || capsule_v2.redaction.treatment.masked() > 0 {
            println!(
                "  {} span(s) masked in the v1 capsule · {} in the v2 capsule. A masked span is \
                 a span that left",
                treatment.masked(),
                capsule_v2.redaction.treatment.masked()
            );
            println!("  the record — read the capsules before handing either to another model.");
        } else {
            println!(
                "  No detector ran over this capsule, so nothing is known about what it carries."
            );
        }
        println!();
        println!("  review it     thruline capsule --project={project}");
        println!(
            "  then          thruline rescue --project={project} --agent {agent} --acknowledge"
        );
        return Ok(ExitCode::SUCCESS);
    }

    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    let json_path = out.join("capsule.json");
    let md_path = out.join("capsule.md");
    std::fs::write(
        &json_path,
        format!("{}\n", serde_json::to_string_pretty(&capsule)?),
    )?;
    std::fs::write(&md_path, thruline_core::capsule::render_markdown(&capsule))?;

    println!();
    println!("STAGED — still nothing launched");
    println!(
        "  capsule       {} · {}",
        md_path.display(),
        json_path.display()
    );
    stage_capsule_v2(db, &ledger, &capsule_v2, &out, agent, &compiled_at);
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "thruline".to_string());
    match agent {
        "claude" => {
            let settings = out.join("rescue-settings.json");
            let hook = serde_json::json!({
                "hooks": {
                    "SessionStart": [{
                        "hooks": [{
                            "type": "command",
                            "command": format!("{exe} rescue --project x --emit-hook {}", json_path.display()),
                            "timeout": 10
                        }]
                    }]
                }
            });
            std::fs::write(
                &settings,
                format!("{}\n", serde_json::to_string_pretty(&hook)?),
            )?;
            println!("  injection     {}", settings.display());
            println!();
            println!("  RUN THIS YOURSELF — ThruLine will not:");
            println!("    claude --settings {}", settings.display());
        }
        _ => {
            println!();
            println!("  RUN THIS YOURSELF — ThruLine will not:");
            println!("    codex \"$(cat {})\"", md_path.display());
        }
    }
    println!();
    println!("  fallback      the capsule is a file: open it, or paste it into any agent");
    println!(
        "  after it runs thruline verify-continuity --project={project} --capsule {} --objective-acknowledged",
        json_path.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// A209/§15 — seal and stage the canonical v2 capsule beside v1's files.
///
/// The sealed bytes go content-addressed into the protected store AND to
/// `capsule-v2.json` byte-identically, so the staged file's own sha256 IS
/// the sealed digest; the markdown is derived from the same canonical
/// document by the one renderer. CAPSULE_CREATED lands at seal and
/// CAPSULE_OFFERED at staging — offers, not acceptances: nothing about a
/// successor is claimed here, and the writers abstain rather than record
/// against an unresolved repository. A defect prints loudly and skips v2;
/// the v1 staging above it stands either way.
fn stage_capsule_v2(
    db: &Path,
    ledger: &Ledger,
    capsule_v2: &thruline_core::capsule_v2::CapsuleV2,
    out: &Path,
    agent: &str,
    compiled_at: &str,
) {
    use thruline_core::capsule_v2 as capv2;
    let store = match thruline_core::protected_store::ProtectedStore::open(protected_store_root(db))
    {
        Ok(store) => store,
        Err(e) => {
            println!("  capsule v2    DEFECT — the byte store did not open, so no v2 capsule was sealed or staged: {e}");
            return;
        }
    };
    let sealed = match capv2::seal(&store, capsule_v2) {
        Ok(sealed) => sealed,
        Err(e) => {
            println!("  capsule v2    DEFECT — sealing failed, so no v2 capsule was staged: {e}");
            return;
        }
    };
    let v2_json = out.join("capsule-v2.json");
    let v2_md = out.join("capsule-v2.md");
    if let Err(e) = std::fs::write(&v2_json, &sealed.bytes)
        .and_then(|()| std::fs::write(&v2_md, capv2::render_markdown(&sealed.capsule)))
    {
        println!(
            "  capsule v2    DEFECT — sealed (digest {}) but not staged: {e}",
            sealed.digest
        );
        return;
    }
    println!(
        "  capsule v2    {} · {}",
        v2_md.display(),
        v2_json.display()
    );
    println!(
        "  sealed        id {}… · digest {}… · {} token(s) of {} budget{}",
        &sealed.capsule.capsule_id[..16.min(sealed.capsule.capsule_id.len())],
        &sealed.digest[..16.min(sealed.digest.len())],
        sealed.capsule.measurement.token_estimate,
        sealed.capsule.measurement.budget,
        match &sealed.capsule.lineage {
            Some(prior) => format!(" · supersedes {}…", &prior[..16.min(prior.len())]),
            None => String::new(),
        },
    );
    let session = format!("cli:rescue:{compiled_at}");
    for (what, outcome) in [
        (
            "CAPSULE_CREATED",
            capv2::record_capsule_created(ledger, &sealed, &v2_json, compiled_at, &session),
        ),
        (
            "CAPSULE_OFFERED",
            capv2::record_capsule_offered(ledger, &sealed, agent, &v2_json, compiled_at, &session),
        ),
    ] {
        match outcome {
            Ok(capv2::CapsuleRecordOutcome::Recorded(o)) => {
                let id = &o.record().id;
                println!("  observed      {what} {}…", &id[..16.min(id.len())]);
            }
            Ok(capv2::CapsuleRecordOutcome::NoRepositoryIdentity { reason }) => {
                println!("  observed      {what} not recorded — {reason}");
            }
            Err(e) => {
                println!(
                    "  observed      DEFECT — the capsule is sealed and staged but {what} was \
                     not appended: {e}"
                );
            }
        }
    }
}

/// C4.1 — protect: preserve before acting.
///
/// One command, two artifacts, no new machinery: the C2 snapshot (stored
/// append-only, digest-checked) and the C3 capsule that cites it by digest.
/// Idempotent in the only sense that matters — it appends a new snapshot and
/// writes a fresh capsule; it never mutates either.
///
/// ThruLine reads; it never runs. Nothing here commits, stashes, or launches.
fn run_protect(
    db: &Path,
    project: &str,
    root: Option<PathBuf>,
    out: Option<PathBuf>,
) -> Result<ExitCode> {
    let root = match root {
        Some(r) => r,
        None => std::env::current_dir().context("resolving the current directory")?,
    };
    let out = out.unwrap_or_else(|| PathBuf::from("thruline-capsule.json"));
    let taken_at = thruline_core::ingest::iso8601_utc_now();

    let mut snapshot =
        thruline_core::snapshot::capture(&root, &taken_at).context("capturing the workspace")?;
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let byte_layer = capture_byte_layer(db, &ledger, &root, &mut snapshot);
    let snap_digest =
        thruline_core::snapshot::store(&ledger, &snapshot).context("storing the snapshot")?;
    let capsule = thruline_core::capsule::compile(&ledger, project, Some(&snapshot), &taken_at)
        .context("compiling the capsule")?;
    let json = serde_json::to_string_pretty(&capsule)?;
    std::fs::write(&out, format!("{json}\n"))
        .with_context(|| format!("writing {}", out.display()))?;

    println!("PROTECTED — nothing was committed, stashed, or launched");
    println!("  workspace    {}", snapshot.root);
    match &snapshot.git {
        Some(g) => {
            let t = &snapshot.tree;
            println!(
                "  lineage      {} · {}",
                g.branch.as_deref().unwrap_or("(detached)"),
                g.head
                    .as_deref()
                    .map(|h| &h[..12.min(h.len())])
                    .unwrap_or("(no commits)")
            );
            println!(
                "  preserved    {} staged · {} changed · {} untracked · {} deleted, every file digested",
                t.staged.len(),
                t.unstaged.len(),
                t.untracked.len(),
                t.deleted.len()
            );
        }
        None => println!("  lineage      not a git worktree — no tree manifests to preserve"),
    }
    if !snapshot.complete {
        println!(
            "  {}",
            snapshot
                .note
                .as_deref()
                .unwrap_or("Snapshot incomplete — capture again.")
        );
    }
    println!(
        "  snapshot     {} · doc sha256 {}…",
        snapshot.id,
        &snap_digest[..16]
    );
    print_byte_layer(&byte_layer, &snapshot.id);
    record_snapshot_created_line(&ledger, &snapshot, &byte_layer, &taken_at, "protect");
    if snapshot.git.is_some() {
        record_workspace_identity(&ledger, &root, &taken_at, "protect", &snapshot.id)?;
    }
    let filled = capsule
        .sections
        .iter()
        .filter(|s| !s.lines.is_empty())
        .count();
    println!(
        "  capsule      {} · {} of {} sections carry lines",
        out.display(),
        filled,
        capsule.sections.len()
    );
    println!(
        "  chain        {} over {} recorded events",
        &capsule.binding.chain_position[..16.min(capsule.binding.chain_position.len())],
        capsule.binding.event_total
    );
    println!("  {}", capsule.secrets_policy.statement);
    println!("  hand it on   thruline capsule --project={project}   (or open the file above)");
    Ok(ExitCode::SUCCESS)
}

/// C3.4 — compile a Recovery Capsule. Read-only: it reads rows, optionally
/// takes a read-only workspace snapshot, and writes only where asked.
///
/// The banner is printed HERE, at the call site, never inside the renderer —
/// A143/RA2's rule, because the same renderer is what a future MCP surface
/// would serve to an agent verbatim.
fn run_capsule(
    db: &Path,
    project: &str,
    root: Option<PathBuf>,
    out: Option<PathBuf>,
    diagnostics: bool,
) -> Result<ExitCode> {
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let compiled_at = thruline_core::ingest::iso8601_utc_now();
    let snapshot = match root {
        Some(root) => Some(
            thruline_core::snapshot::capture(&root, &compiled_at)
                .context("capturing the workspace beside the capsule")?,
        ),
        None => None,
    };
    let capsule = thruline_core::capsule::compile_with(
        &ledger,
        project,
        snapshot.as_ref(),
        &compiled_at,
        diagnostics,
    )
    .context("compiling the capsule")?;
    if let Some(path) = out {
        let json = serde_json::to_string_pretty(&capsule)?;
        std::fs::write(&path, format!("{json}\n"))
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());
    }
    println!("{PACKET_BANNER}");
    print!("{}", thruline_core::capsule::render_markdown(&capsule));
    Ok(ExitCode::SUCCESS)
}

/// C2 — the workspace snapshotter's human door. Capture, store, say what was
/// seen; the receipt is one command away (`--db` row + doc digest printed).
fn run_snapshot(db: &Path, root: Option<PathBuf>) -> Result<ExitCode> {
    let root = match root {
        Some(r) => r,
        None => std::env::current_dir().context("resolving the current directory")?,
    };
    let taken_at = thruline_core::ingest::iso8601_utc_now();
    let mut snap =
        thruline_core::snapshot::capture(&root, &taken_at).context("capturing the workspace")?;
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let byte_layer = capture_byte_layer(db, &ledger, &root, &mut snap);
    let doc_sha = thruline_core::snapshot::store(&ledger, &snap).context("storing the snapshot")?;
    println!("WORKSPACE SNAPSHOT {}", snap.id);
    println!("  root         {}", snap.root);
    match &snap.git {
        Some(g) => {
            println!(
                "  lineage      {} · {}",
                g.branch.as_deref().unwrap_or("(detached)"),
                g.head
                    .as_deref()
                    .map(|h| &h[..12.min(h.len())])
                    .unwrap_or("(no commits)"),
            );
            let t = &snap.tree;
            if t.staged.is_empty()
                && t.unstaged.is_empty()
                && t.untracked.is_empty()
                && t.deleted.is_empty()
            {
                println!("  tree         clean — nothing staged, changed, or untracked");
            } else {
                let mut parts = Vec::new();
                for (n, noun) in [
                    (t.staged.len(), "staged"),
                    (t.unstaged.len(), "changed"),
                    (t.untracked.len(), "untracked"),
                    (t.deleted.len(), "deleted"),
                ] {
                    if n > 0 {
                        parts.push(format!("{n} {noun}"));
                    }
                }
                println!("  tree         {} — every file digested", parts.join(" · "));
            }
            if let Some(d) = &snap.tree.dirty_diff_sha256 {
                println!("  dirty diff   sha256 {}…", &d[..16]);
            }
        }
        None => println!(
            "  lineage      {}",
            snap.note.as_deref().unwrap_or("not a git worktree")
        ),
    }
    println!(
        "  environment  {} · {} recorded by name — values never · {}",
        snap.environment.os,
        phrase_simple(
            snap.environment.env_names.len() as u64,
            "variable",
            "variables"
        ),
        phrase_simple(
            snap.environment.lockfiles.len() as u64,
            "lockfile digested",
            "lockfiles digested"
        ),
    );
    match snap.processes.ps_lines {
        Some(lines) => println!(
            "  processes    {} in the tree at capture · {}",
            phrase_simple(lines, "line", "lines"),
            snap.processes.gaps
        ),
        None => println!(
            "  processes    {} · {}",
            voice::SNAPSHOT_NOT_READ_IN_TIME,
            snap.processes.gaps
        ),
    }
    if snap.complete {
        println!("  complete     yes — the tree held still through the capture");
    } else {
        println!(
            "  {}",
            snap.note
                .as_deref()
                .unwrap_or("Snapshot incomplete — capture again.")
        );
    }
    println!(
        "  stored       doc sha256 {}… · thruline reads; it never runs",
        &doc_sha[..16]
    );
    print_byte_layer(&byte_layer, &snap.id);
    record_snapshot_created_line(&ledger, &snap, &byte_layer, &taken_at, "snapshot");
    if snap.git.is_some() {
        record_workspace_identity(&ledger, &root, &taken_at, "snapshot", &snap.id)?;
    }
    Ok(ExitCode::SUCCESS)
}

/* ── A208 (MANDATE-v4 §14): the protected byte layer ─────────────────── */

/// What the byte layer produced beside the digest-only C2 snapshot.
enum ByteLayer {
    Captured(Box<thruline_core::protected_store::CaptureOutcome>),
    /// The layer refused or failed, and says exactly why — never silently.
    Skipped(String),
}

/// Where the protected store lives for a given ledger path: a sibling
/// `protected/` directory beside the database, never inside a repository.
fn protected_store_root(db: &Path) -> PathBuf {
    match db.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("protected"),
        _ => PathBuf::from("protected"),
    }
}

/// Capture the §14 byte layer and attach its lineage envelope to the C2
/// snapshot document. Any refusal is returned as a printed reason — the
/// digest-only snapshot still stands, explicitly stripped of any
/// byte-preservation claim.
fn capture_byte_layer(
    db: &Path,
    ledger: &Ledger,
    root: &Path,
    snap: &mut thruline_core::snapshot::WorkspaceSnapshot,
) -> ByteLayer {
    use thruline_core::protected_store as pstore;
    if snap.git.is_none() {
        return ByteLayer::Skipped(
            "not a git worktree — there is no declared tree whose bytes this could preserve"
                .to_string(),
        );
    }
    let store = match pstore::ProtectedStore::open(protected_store_root(db)) {
        Ok(store) => store,
        Err(e) => return ByteLayer::Skipped(format!("the byte store did not open: {e}")),
    };
    let outcome = match pstore::capture(
        &store,
        root,
        thruline_core::workspace_identity::Scope::everything(),
    ) {
        Ok(outcome) => outcome,
        Err(e) => return ByteLayer::Skipped(format!("byte capture refused: {e}")),
    };
    let lineage = thruline_core::snapshot::latest_protected(ledger, &outcome.manifest.root)
        .ok()
        .flatten()
        .map(|(id, _)| id);
    snap.protected = Some(outcome.protected_ref(lineage));
    ByteLayer::Captured(Box::new(outcome))
}

fn print_byte_layer(layer: &ByteLayer, snapshot_id: &str) {
    match layer {
        ByteLayer::Captured(o) if o.manifest.is_complete() => {
            println!(
                "  protected    {} files + {} repo-metadata files preserved byte-exact · \
                 {} new blobs, encrypted at rest",
                o.manifest.worktree.len(),
                o.manifest.git.len(),
                o.new_blobs
            );
            println!(
                "  manifest     TLSNAP_V1 {}… · restore with: thruline restore {snapshot_id}",
                &o.manifest_digest[..16]
            );
        }
        ByteLayer::Captured(o) => {
            println!(
                "  protected    INCOMPLETE — {}. Stored as what was read; restore will \
                 refuse it. Capture again.",
                o.manifest
                    .incomplete_reason
                    .as_deref()
                    .unwrap_or("no reason recorded")
            );
        }
        ByteLayer::Skipped(reason) => {
            println!(
                "  protected    NO byte layer — {reason}. This record proves digests, not bytes."
            );
        }
    }
}

/// Append SNAPSHOT_CREATED for a stored snapshot that carries a byte layer,
/// and print the receipt line (or the abstention, or the defect — loudly;
/// a failed observation write never un-stores the snapshot).
fn record_snapshot_created_line(
    ledger: &Ledger,
    snap: &thruline_core::snapshot::WorkspaceSnapshot,
    layer: &ByteLayer,
    taken_at: &str,
    verb: &str,
) {
    use thruline_core::protected_store as pstore;
    let (Some(pref), ByteLayer::Captured(outcome)) = (&snap.protected, layer) else {
        return;
    };
    let session = format!("cli:{verb}:{taken_at}");
    match pstore::record_snapshot_created(
        ledger,
        &snap.id,
        pref,
        Some(outcome.manifest.source_identity.as_str()),
        taken_at,
        &session,
    ) {
        Ok(pstore::SnapshotRecordOutcome::Recorded(o)) => {
            let id = &o.record().id;
            println!(
                "  observed     SNAPSHOT_CREATED {}…",
                &id[..16.min(id.len())]
            );
        }
        Ok(pstore::SnapshotRecordOutcome::NoRepositoryIdentity { reason }) => {
            println!("  observed     not recorded — {reason}");
        }
        Err(e) => {
            println!(
                "  observed     DEFECT — the snapshot is stored but its observation was \
                 not appended: {e}"
            );
        }
    }
}

/// `thruline snapshot --verify <id>`: re-hash every blob a stored snapshot
/// references, plus its manifest, and append SNAPSHOT_VERIFIED on success.
fn run_snapshot_verify(db: &Path, id: &str) -> Result<ExitCode> {
    use thruline_core::protected_store as pstore;
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let Some((snap, _doc_sha)) =
        thruline_core::snapshot::load(&ledger, id).context("loading the snapshot")?
    else {
        println!("no snapshot with id {id} is stored — nothing to verify");
        return Ok(ExitCode::FAILURE);
    };
    let Some(pref) = &snap.protected else {
        println!(
            "snapshot {id} carries no byte layer (a digest-only record) — there are no \
             stored bytes to verify"
        );
        return Ok(ExitCode::FAILURE);
    };
    let store = pstore::ProtectedStore::open(protected_store_root(db))
        .context("opening the protected store")?;
    let checked = pstore::verify_snapshot(&store, pref).context("verifying the snapshot")?;
    println!("SNAPSHOT VERIFIED {id}");
    println!(
        "  re-hashed    {checked} blobs (manifest included) · TLSNAP_V1 {}…",
        &pref.manifest_digest[..16]
    );
    let verified_at = thruline_core::ingest::iso8601_utc_now();
    let session = format!("cli:snapshot-verify:{verified_at}");
    match pstore::record_snapshot_verified(&ledger, id, pref, checked, &verified_at, &session)
        .context("recording the verification")?
    {
        pstore::SnapshotRecordOutcome::Recorded(o) => {
            let obs = &o.record().id;
            println!(
                "  observed     SNAPSHOT_VERIFIED {}…",
                &obs[..16.min(obs.len())]
            );
        }
        pstore::SnapshotRecordOutcome::NoRepositoryIdentity { reason } => {
            println!("  observed     not recorded — {reason}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `thruline restore <id>`: materialize a protected snapshot. Safe-directory
/// by default; over-working-tree only behind the double gate.
fn run_restore(
    db: &Path,
    snapshot_id: &str,
    to: Option<PathBuf>,
    over_working_tree: bool,
    expect_tree: Option<String>,
    yes: bool,
) -> Result<ExitCode> {
    use thruline_core::protected_store as pstore;
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let Some((snap, _doc_sha)) =
        thruline_core::snapshot::load(&ledger, snapshot_id).context("loading the snapshot")?
    else {
        println!("no snapshot with id {snapshot_id} is stored — nothing to restore");
        return Ok(ExitCode::FAILURE);
    };
    let Some(pref) = &snap.protected else {
        println!(
            "snapshot {snapshot_id} carries no byte layer (a digest-only record from before \
             the byte store, or a capture whose byte layer refused) — there is nothing \
             byte-exact to restore. Take a new one with `thruline protect`."
        );
        return Ok(ExitCode::FAILURE);
    };
    let store = pstore::ProtectedStore::open(protected_store_root(db))
        .context("opening the protected store")?;
    let manifest = pstore::load_manifest(&store, pref).context("loading the manifest")?;

    if !over_working_tree {
        let target = to.unwrap_or_else(|| {
            protected_store_root(db)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default()
                .join("restores")
                .join(snapshot_id)
        });
        let report =
            pstore::restore_to_new(&store, &manifest, &target).context("restoring the snapshot")?;
        println!("RESTORED {snapshot_id} — byte-exact, into a separate safe directory");
        println!("  into         {}", target.display());
        println!(
            "  materialized {} files + {} repo-metadata files · {} bytes",
            report.worktree_files, report.git_files, report.bytes_written
        );
        println!(
            "  state        the captured workspace exactly: staged stays staged, deleted \
             stays absent, symlinks are links, modes hold"
        );
        println!("  the active workspace was not touched");
        return Ok(ExitCode::SUCCESS);
    }

    // The one destructive edge. Gate one: the compare-first acknowledgment.
    let target = to.unwrap_or_else(|| PathBuf::from(&manifest.root));
    let plan =
        pstore::plan_over_working_tree(&manifest, &target).context("planning the restore")?;
    match expect_tree.as_deref() {
        None => {
            println!("REFUSED — acknowledge the target tree's current state first.");
            println!("  target       {}", target.display());
            println!("  currently    {}", plan.target_identity);
            println!(
                "  to proceed   rerun with:  --expect-tree {}",
                plan.target_identity
            );
            println!("  nothing was touched");
            return Ok(ExitCode::FAILURE);
        }
        Some(expected) if expected != plan.target_identity => {
            println!("REFUSED — the target tree is not the state you acknowledged.");
            println!("  acknowledged {expected}");
            println!("  currently    {}", plan.target_identity);
            println!("  nothing was touched — re-inspect the tree, then re-acknowledge");
            return Ok(ExitCode::FAILURE);
        }
        Some(_) => {}
    }

    // The preview names EVERYTHING that will change, before anything does.
    println!("RESTORE OVER WORKING TREE — the exact scope, before anything is touched");
    println!("  target       {}", target.display());
    if plan.writes.is_empty() && plan.deletes.is_empty() {
        println!("  the target already equals the snapshot state — nothing to change");
        return Ok(ExitCode::SUCCESS);
    }
    println!("  will write   {} files:", plan.writes.len());
    for w in &plan.writes {
        println!("    write  {} ({} bytes)", w.path, w.bytes);
    }
    println!("  will delete  {} files:", plan.deletes.len());
    for d in &plan.deletes {
        println!("    delete {d}");
    }
    println!(
        "  untouched    everything outside the snapshot's declared scope (ignored files included)"
    );

    // Gate two: the interactive confirmation (--yes is the documented seam).
    if !yes {
        print!("type exactly `restore over working tree` to proceed: ");
        use std::io::Write as _;
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("reading the confirmation")?;
        if line.trim() != "restore over working tree" {
            println!("not confirmed — nothing was touched");
            return Ok(ExitCode::FAILURE);
        }
    }

    let report = pstore::apply_over_working_tree(&store, &manifest, &target, &plan)
        .context("applying the restore")?;
    println!("RESTORED {snapshot_id} — over the working tree, exactly as previewed");
    println!(
        "  wrote        {} worktree + {} repo-metadata files · {} bytes · deleted {}",
        report.worktree_files,
        report.git_files,
        report.bytes_written,
        plan.deletes.len()
    );
    match pstore::snapshot_applies(&manifest, &target).context("re-observing the target")? {
        Some(true) => println!(
            "  verified     the tree now IS the captured workspace identity\n\
             \x20              {}",
            manifest.source_identity
        ),
        Some(false) => println!(
            "  note         the restored tree's identity differs from the source — \
             expected when restoring to a different path (root and worktree axes); \
             every file was still verified byte-exact on write"
        ),
        None => println!(
            "  note         the tree moved while being re-observed — run \
             `thruline snapshot --verify {snapshot_id}` and re-observe when it settles"
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// A206 (MANDATE-v4 §12) — record the workspace's canonical identity as a
/// WORKSPACE_OBSERVED observation from a foreground capture command. The
/// observation is read-only like the snapshot beside it; its receipt
/// terminates at the independently observed filesystem state (root path plus
/// the WSID_V1 canonical digest) and cites the stored snapshot row as
/// context. Foreground only — nothing on the hook path writes observations.
fn record_workspace_identity(
    ledger: &Ledger,
    root: &Path,
    taken_at: &str,
    verb: &str,
    snapshot_id: &str,
) -> Result<()> {
    use thruline_core::workspace_identity as wsid;
    let obs = wsid::observe(root, wsid::Scope::everything())
        .context("observing the canonical workspace identity")?;
    let session = format!("cli:{verb}:{taken_at}");
    match wsid::record_observation(ledger, &obs, taken_at, &session, Some(snapshot_id))
        .context("recording the workspace observation")?
    {
        wsid::RecordOutcome::Recorded {
            outcome,
            workspace_identity,
        } => {
            let rec = outcome.record();
            let short = &rec.id[..16.min(rec.id.len())];
            match workspace_identity {
                Some(id) => println!("  identity     {id} · observation {short}…"),
                None => println!(
                    "  identity     not established — the tree moved mid-observation; \
                     recorded as an incomplete observation {short}…"
                ),
            }
        }
        wsid::RecordOutcome::NoRepositoryIdentity { reason } => {
            println!("  identity     not recorded — {reason}");
        }
    }
    Ok(())
}

/* ── A200: the speaker verbs ─────────────────────────────────────────── */

/// The speakers' fail-open contract, half one: a hard self-timeout well under
/// the vendor's 5 s hook budget. If anything below stalls — a locked disk, a
/// starved database — the process exits 0 in silence rather than costing the
/// session its hook budget. Half two is in each verb: every error path is
/// swallowed, prints nothing, exits 0.
fn speaker_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(3_500));
        std::process::exit(0);
    });
}

/// The spine of a hook delivery, read from stdin. `None` on any defect — the
/// speaker then has nothing to say, which is exactly what it says.
struct SpeakerPayload {
    session_id: String,
    transcript_path: Option<String>,
    cwd: Option<String>,
    stop_hook_active: bool,
}

fn speaker_payload() -> Option<SpeakerPayload> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::io::stdin().read_to_end(&mut buf).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&buf).ok()?;
    let field = |key: &str| -> Option<String> {
        v.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    Some(SpeakerPayload {
        session_id: field("session_id")?,
        transcript_path: field("transcript_path"),
        cwd: field("cwd"),
        stop_hook_active: v
            .get("stop_hook_active")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

/// `thruline digest precompute --hook session-end` — the SessionEnd speaker.
fn run_speaker_precompute(db: &Path, hook: &str) -> ExitCode {
    speaker_watchdog();
    if hook == "session-end" {
        let _ = speaker_digest_pass(db, "session-end", false);
    }
    ExitCode::SUCCESS
}

/// `thruline audit --hook stop` — the stop-time claim audit and one cached
/// Decisions advisory line. It never blocks.
fn run_speaker_audit(db: &Path, hook: &str) -> ExitCode {
    speaker_watchdog();
    if hook == "stop" {
        let _ = speaker_digest_pass(db, "stop", true);
    }
    ExitCode::SUCCESS
}

/// The shared pass behind SessionEnd and Stop: extract the final turn, bind
/// it against the session's own sensor record, rewrite this project's digest
/// cache — and, for the Stop audit, append the no-proof lines to
/// `speaker_findings` (best-effort: history can miss a beat on a busy ledger;
/// the cache is the loop's delivery path and gets the real error).
///
/// Every Stop rewrites the cache wholesale, including down to nothing: a
/// no-proof line from an earlier turn may have been paid off by a later one,
/// and a stale accusation surviving into the next session's brief is the one
/// failure this loop must never produce. SessionEnd then finalizes the same
/// way.
fn speaker_digest_pass(db: &Path, hook: &str, announce: bool) -> anyhow::Result<()> {
    let Some(payload) = speaker_payload() else {
        return Ok(());
    };
    if announce && payload.stop_hook_active {
        return Ok(());
    }
    let Some(tp) = payload.transcript_path.as_deref() else {
        return Ok(());
    };
    let Some(key) = thruline_core::speaker::project_key(Some(tp), payload.cwd.as_deref()) else {
        return Ok(());
    };
    // A pending schema upgrade lands here as an error and is swallowed by the
    // caller: the speaker stays silent rather than printing a refusal into a
    // hook stream nobody reads (the A193 refusal belongs to human commands).
    let ledger = Ledger::open(db)?;
    let turn = thruline_core::speaker::final_turn(Path::new(tp))?;
    let rows = thruline_core::speaker::session_post_tool_rows(&ledger, &payload.session_id)?;
    let mut idx = thruline_core::speaker::EvidenceIndex::build(&rows);
    // A201 spill guard: a delivery that spilled off the chain is a tool event
    // the scan above never saw, so any no-proof verdict over this window
    // degrades to silence and the digest records why.
    idx.spilled_deliveries = thruline_core::sensor::spilled_for_session(
        &thruline_core::sensor::spill_path(db),
        &payload.session_id,
    );
    // A202 window-completeness guard: the transcript's own record is the
    // yardstick the scanned window is measured against (any deficit means an
    // unseen tool event could be the very proof a no-proof line would deny),
    // and a session born from a resume carries pre-resume evidence under its
    // prior id that this scan can never reach. Either gap degrades
    // accusations — and, for the transcript↔record gaps, promise lines — to
    // silence in the binder. An unreadable SessionStart record is treated as
    // "not known resumed": fail-open must not fail-accuse, and it cannot —
    // false only skips a degradation.
    idx.transcript_tool_events = Some(turn.tool_use_count);
    idx.resume_started =
        thruline_core::speaker::session_born_from_resume(&ledger, &payload.session_id)
            .unwrap_or(false);
    let now = thruline_core::ingest::iso8601_utc_now();
    // A201: the claim loop feeds the same brief — still-unresolved
    // claim_completion submissions for this project ride along, each with a
    // typeable why id. Same fail-open contract as the rest of the pass.
    let claim_lines = thruline_core::verifier::claim_digest_lines(
        &ledger,
        &key,
        thruline_core::speaker::MAX_LINES,
    )?;
    let digest = thruline_core::speaker::compose(
        &thruline_core::speaker::DigestScope {
            project_key: &key,
            session_id: &payload.session_id,
            transcript_path: tp,
            generated_at: &now,
            hook,
        },
        &turn,
        &idx,
        &claim_lines,
    );
    if announce {
        // C34 pause gate (2 of 6): while the session's folder is paused, or if
        // the session started inside a pause, nothing is appended to
        // `speaker_findings`. Fails open (an unreadable record is "not
        // paused"), and the skip is counted beside the ledger, not in it.
        match thruline_core::capture_pause::paused_session(
            &ledger,
            payload.cwd.as_deref(),
            Some(tp),
        ) {
            Some(folder) => thruline_core::capture_pause::note_skip(
                db,
                thruline_core::capture_pause::SkipGate::Audit,
                &folder,
                &now,
            ),
            None => {
                let _ = thruline_core::speaker::record_findings(&ledger, &digest, &turn, &now);
            }
        }
    }
    thruline_core::speaker::write_cache(&thruline_core::speaker::digest_dir(db), &digest)?;
    if announce {
        if let Some(cwd) = payload.cwd.as_deref() {
            stop_decision_line(db, Path::new(cwd));
        }
    }
    Ok(())
}

/// The Decisions advisory's budget: the product's 50 ms, which the environment
/// may only *lengthen* (and never past [`STOP_ADVISORY_BUDGET_CAP_MS`], well
/// under the 3.5 s watchdog) — the same lengthen-only rule as
/// `THRULINE_GATE_BUDGET_MS`. An unset, unparseable or shorter value is the
/// default. It exists so a test on a loaded machine can ask for the line
/// without the 50 ms race; a session never sets it.
const STOP_ADVISORY_BUDGET_CAP_MS: u64 = 2_000;

fn stop_advisory_budget() -> std::time::Duration {
    let default = thruline_core::decisions::speaker::DEFAULT_BUDGET;
    let asked = std::env::var("THRULINE_STOP_ADVISORY_BUDGET_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|ms| std::time::Duration::from_millis(ms.min(STOP_ADVISORY_BUDGET_CAP_MS)));
    asked.map_or(default, |budget| budget.max(default))
}

/// The existing Stop pass can do its ledger work first; this independent read
/// has a 50 ms deadline (see [`stop_advisory_budget`]) and never computes or
/// appends a decision verdict.
fn stop_decision_line(db: &Path, cwd: &Path) {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let db = db.to_path_buf();
    let cwd = cwd.to_path_buf();
    let budget = stop_advisory_budget();
    if std::thread::Builder::new()
        .name("decision-stop-advisory".into())
        .spawn(move || {
            let _ = tx.send(thruline_core::decisions::speaker::cached_line_within(
                &db, &cwd, budget,
            ));
        })
        .is_err()
    {
        return;
    }
    if let Ok(Some(line)) = rx.recv_timeout(budget) {
        println!(
            "{}",
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "Stop",
                    "additionalContext": line,
                }
            })
        );
    }
}

/// `thruline brief --hook session-start` — inject the cached digest. Opens no
/// ledger at all: the cache file is the whole read, which is what keeps this
/// path trivially inside its latency budget and out of every locking story.
fn run_speaker_brief(db: &Path, hook: &str) -> ExitCode {
    speaker_watchdog();
    if hook != "session-start" {
        return ExitCode::SUCCESS;
    }
    let Some(payload) = speaker_payload() else {
        return ExitCode::SUCCESS;
    };
    let Some(key) = thruline_core::speaker::project_key(
        payload.transcript_path.as_deref(),
        payload.cwd.as_deref(),
    ) else {
        return ExitCode::SUCCESS;
    };
    let dir = thruline_core::speaker::digest_dir(db);
    let Some(md) = thruline_core::speaker::read_cache_markdown(&dir, &key) else {
        return ExitCode::SUCCESS;
    };
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": speaker_budgeted(&md),
        }
    });
    println!("{out}");
    ExitCode::SUCCESS
}

/// The ≤400-token hard budget, enforced at the point of injection so even a
/// foreign or oversized cache file cannot flood a session: header plus at
/// most [`thruline_core::speaker::MAX_LINES`] lines, capped at 1,500
/// characters on a char boundary.
fn speaker_budgeted(md: &str) -> String {
    const MAX_CHARS: usize = 1_500;
    let kept: Vec<&str> = md
        .lines()
        .take(1 + thruline_core::speaker::MAX_LINES)
        .collect();
    let joined = kept.join("\n");
    if joined.chars().count() <= MAX_CHARS {
        return joined;
    }
    let cut: String = joined.chars().take(MAX_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// A199 — one door for setup, composing machinery that already exists.
///
/// `init` writes exactly three kinds of entry and reports each by file and by
/// name: the sensor hook pack into Claude Code's settings (the same merge as
/// `sensor install`), the speaker hook pack behind an explicit consent (A200
/// — the in-session truth loop), and one MCP server entry into Claude Code's
/// user config. It is idempotent — a second run changes nothing and says so —
/// and `init --undo` removes exactly what init wrote, sparing every foreign
/// byte (A142's one-deletion discipline, applied to other people's config).
fn run_init(
    settings: Option<PathBuf>,
    claude_json: Option<PathBuf>,
    undo: bool,
    yes: bool,
    gate: bool,
    db: &Path,
    db_named: bool,
) -> Result<ExitCode> {
    let settings_path = sensor_settings_path(settings)?;
    let claude_json_path = init_claude_json_path(claude_json)?;
    if undo {
        return run_init_undo(&settings_path, &claude_json_path);
    }

    // A hook and the context server run from a directory this command never
    // sees, so a relative `--db` would name a different file for every session.
    // `absolute` does not touch the filesystem or resolve a link, so the path
    // that is written is the path that was given. The default is absolute
    // already, so this changes nothing when no `--db` was given.
    let db_absolute = std::path::absolute(db)
        .with_context(|| format!("making the ledger path {} absolute", db.display()))?;
    let db = db_absolute.as_path();
    // Named → every hook below carries it. Not named → they carry nothing and
    // resolve the default themselves when the harness fires them.
    let hook_db = db_named.then_some(db);

    // Surface a pending schema upgrade HERE, before a single file is touched:
    // under A193 this open refuses in one line and pays nothing, so a user is
    // never left with hooks registered against a ledger no hook can reach.
    Ledger::open(db).context("opening the ledger before anything is written")?;

    // The banner names the pack this command actually installs. It said
    // "capture hooks" for as long as it has existed, which is the §4.1 pack's
    // name — a pack `init` does not install and cannot install beside this one.
    println!(
        "THRULINE INIT — the sensor hook pack (appends to `sensor_events` in {}), the speaker hooks, and the context server, for Claude Code",
        db.display()
    );

    // 1. The sensor hook pack — the exact `sensor install` merge.
    let mut settings_root = read_settings_or_empty(&settings_path)?;
    let (added, present) = sensor_install_into(&mut settings_root, hook_db)?;

    // 2. The speaker hook pack (A200) — the seam Session 1 left, now filled:
    // the in-session truth loop's voice. Behind an explicit consent: on a
    // terminal a plain Enter is yes; with no terminal nobody is there to give
    // it, so the answer is no unless `--yes` gives it (INIT-CONSENT-NO-TTY).
    let consent = if yes {
        SpeakerConsent::Yes
    } else {
        init_speaker_consent()
    };
    let speakers_wanted = consent == SpeakerConsent::Yes;
    let (spk_added, spk_present) = if speakers_wanted {
        init_speaker_install_into(&mut settings_root, hook_db)?
    } else {
        (Vec::new(), Vec::new())
    };

    // 2b. §4.1's opt-in Stop gate, on `Stop`, beside the speakers.
    //
    // Not a recorder (A199 amendment b) — it reads the record and speaks a
    // decision, and appends nothing — so it cannot collide with the pack above.
    let (gate_added, gate_present) = if gate {
        init_gate_install_into(&mut settings_root, hook_db)?
    } else {
        (Vec::new(), Vec::new())
    };

    let settings_touched = !added.is_empty() || !spk_added.is_empty() || !gate_added.is_empty();
    if settings_touched {
        write_settings(&settings_path, &settings_root)?;
    }
    println!("hooks            {}", settings_path.display());
    if !added.is_empty() {
        println!("  wrote          {}", added.join(", "));
    }
    if !present.is_empty() {
        println!("  already there  {}", present.join(", "));
    }
    println!("speaker hooks    the in-session truth loop: brief at SessionStart, audit at Stop, digest at SessionEnd");
    match consent {
        SpeakerConsent::Yes => {}
        SpeakerConsent::No => println!(
            "  skipped        by your answer — nothing was written; run init again to add them"
        ),
        SpeakerConsent::NoTerminal => println!(
            "  skipped        no terminal to ask, so nothing was written — `thruline init --yes` installs them"
        ),
    }
    if !spk_added.is_empty() {
        println!("  wrote          {}", spk_added.join(", "));
    }
    if !spk_present.is_empty() {
        println!("  already there  {}", spk_present.join(", "));
    }
    println!(
        "stop gate        {}",
        if gate {
            "blocks a stop whose completion claim is UNSUPPORTED — never one that is merely unverifiable"
        } else {
            "not installed — add it with `thruline init --gate`"
        }
    );
    if !gate_added.is_empty() {
        println!("  wrote          {}", gate_added.join(", "));
    }
    if !gate_present.is_empty() {
        println!("  already there  {}", gate_present.join(", "));
    }

    // 3. The MCP context server entry, by read-modify-write that keeps every
    // unknown field. The file belongs to Claude Code; we own one key in one
    // object and touch nothing beside it.
    let mut claude_root = read_settings_or_empty(&claude_json_path)?;
    let mcp_written = init_mcp_install_into(&mut claude_root, db)?;
    if mcp_written {
        write_settings(&claude_json_path, &claude_root)?;
    }
    println!("context server   {}", claude_json_path.display());
    match (
        mcp_written,
        claude_root
            .get("mcpServers")
            .and_then(|s| s.get(MCP_SERVER_KEY)),
    ) {
        (true, _) => println!(
            "  wrote          mcpServers.{MCP_SERVER_KEY} → {} mcp --agent claude --db {}",
            sensor_command_template()?,
            db.display()
        ),
        (false, Some(existing)) if init_mcp_entry_is_ours(existing) => {
            println!("  already there  mcpServers.{MCP_SERVER_KEY}")
        }
        (false, Some(_)) => println!(
            "  left alone     mcpServers.{MCP_SERVER_KEY} exists but was not written by init; not touched"
        ),
        (false, None) => {}
    }

    if !settings_touched && !mcp_written {
        println!("nothing to do — every entry is already registered; no file was changed");
    }
    println!("undo with        thruline init --undo");
    Ok(ExitCode::SUCCESS)
}

/// What became of the speaker consent, and why — the report says which.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SpeakerConsent {
    Yes,
    /// A person at the terminal answered `n`.
    No,
    /// stdin is not a terminal: an agent or a script ran init, so no person
    /// was there to answer, and whatever arrives on stdin is not a yes.
    NoTerminal,
}

/// The speaker consent. On a terminal: one question, one stdin line, a plain
/// Enter is yes (the loop is the product's door), an explicit `n` is no and is
/// respected without argument. Without a terminal the question is not asked
/// and the answer is no; `--yes` is the explicit opt-in (INIT-CONSENT-NO-TTY:
/// it used to take the default and install the speakers with nobody's yes).
fn init_speaker_consent() -> SpeakerConsent {
    use std::io::{BufRead, IsTerminal};
    if !std::io::stdin().is_terminal() {
        return SpeakerConsent::NoTerminal;
    }
    println!("speaker hooks    install the in-session truth loop? [Y/n]");
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    let t = line.trim().to_ascii_lowercase();
    if t == "n" || t == "no" {
        SpeakerConsent::No
    } else {
        SpeakerConsent::Yes
    }
}

/// The reversal: exactly what init wrote, and not one byte more.
fn run_init_undo(settings_path: &Path, claude_json_path: &Path) -> Result<ExitCode> {
    println!("THRULINE INIT --undo — removes what init wrote, and nothing else");
    let mut settings_root = read_settings_or_empty(settings_path)?;
    let removed = sensor_uninstall_from(&mut settings_root);
    let spk_removed = init_speaker_uninstall_from(&mut settings_root);
    // Unconditional, whatever `--gate` was passed when it went in: undo is the
    // reversal of what is THERE, and an entry only `--gate` can write is one
    // only this can remove.
    let gate_removed = init_gate_uninstall_from(&mut settings_root);
    if removed + spk_removed + gate_removed > 0 {
        write_settings(settings_path, &settings_root)?;
    }
    println!("hooks            {}", settings_path.display());
    if removed > 0 {
        println!(
            "  removed        {}",
            phrase_simple(removed as u64, "hook entry", "hook entries")
        );
    } else {
        println!("  removed        nothing — no ThruLine hook entries were registered");
    }
    if spk_removed > 0 {
        println!(
            "  removed        {}",
            phrase_simple(spk_removed as u64, "speaker entry", "speaker entries")
        );
    } else {
        println!("  removed        nothing more — no speaker entries were registered");
    }
    if gate_removed > 0 {
        println!(
            "  removed        {}",
            phrase_simple(gate_removed as u64, "stop-gate entry", "stop-gate entries")
        );
    } else {
        println!("  removed        nothing more — no stop-gate entry was registered");
    }

    let mut claude_root = read_settings_or_empty(claude_json_path)?;
    let mcp_removed = init_mcp_uninstall_from(&mut claude_root);
    if mcp_removed {
        write_settings(claude_json_path, &claude_root)?;
    }
    println!("context server   {}", claude_json_path.display());
    if mcp_removed {
        println!("  removed        mcpServers.{MCP_SERVER_KEY}");
    } else if claude_root
        .get("mcpServers")
        .and_then(|s| s.get(MCP_SERVER_KEY))
        .is_some()
    {
        println!(
            "  left alone     mcpServers.{MCP_SERVER_KEY} exists but was not written by init; not touched"
        );
    } else {
        println!("  removed        nothing — no {MCP_SERVER_KEY} entry was registered");
    }
    println!("recorded rows stay in the ledger — removal changes capture, never history");
    Ok(ExitCode::SUCCESS)
}

/// A200 — the speaker hook class, attached at the seam Session 1 left.
///
/// Speakers are the sensors' opposite by contract: a sensor appends and never
/// speaks (`crates/core/src/sensor.rs`, pinned by `c12_sensor_cli.rs` —
/// untouched by this); a speaker puts receipt-backed words into the session
/// and never appends a capture row. Three entries, one per hook event:
/// `(event, verb, report label)`.
const SPEAKER_HOOKS: [(&str, &str, &str); 3] = [
    (
        "SessionStart",
        "brief --hook session-start",
        "SessionStart brief",
    ),
    ("Stop", "audit --hook stop", "Stop audit"),
    (
        "SessionEnd",
        "digest precompute --hook session-end",
        "SessionEnd digest",
    ),
];

/// The ownership marker, exactly as for the sensors: an entry is ours when
/// its command ends with the verb. Removal matches nothing broader (A142).
fn speaker_suffix(verb: &str) -> String {
    format!(" {verb}")
}

fn init_speaker_install_into(
    root: &mut serde_json::Value,
    hook_db: Option<&Path>,
) -> Result<(Vec<&'static str>, Vec<&'static str>)> {
    let exe = sensor_command_template()?;
    let db_flag = hook_db_flag(hook_db)?;
    if !root.is_object() {
        anyhow::bail!("settings root is not a JSON object; refusing to rewrite it");
    }
    let hooks = root
        .as_object_mut()
        .expect("checked above")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        anyhow::bail!("settings `hooks` is not an object; refusing to rewrite it");
    };
    let mut added = Vec::new();
    let mut present = Vec::new();
    for (event, verb, label) in SPEAKER_HOOKS {
        let entries = hooks.entry(event).or_insert_with(|| serde_json::json!([]));
        let Some(entries) = entries.as_array_mut() else {
            anyhow::bail!("settings `hooks.{event}` is not an array; refusing to rewrite it");
        };
        if entries
            .iter()
            .any(|e| entry_carries_suffix(e, &speaker_suffix(verb)))
        {
            present.push(label);
            continue;
        }
        entries.push(serde_json::json!({
            "hooks": [{
                "type": "command",
                "command": format!("{exe}{db_flag} {verb}"),
                "timeout": 5
            }]
        }));
        added.push(label);
    }
    Ok((added, present))
}

fn init_speaker_uninstall_from(root: &mut serde_json::Value) -> usize {
    let mut removed = 0usize;
    let Some(hooks) = root
        .get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return 0;
    };
    for (event, verb, _) in SPEAKER_HOOKS {
        let suffix = speaker_suffix(verb);
        if let Some(entries) = hooks
            .get_mut(event)
            .and_then(serde_json::Value::as_array_mut)
        {
            for entry in entries.iter_mut() {
                if let Some(cmds) = entry
                    .get_mut("hooks")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    let before = cmds.len();
                    cmds.retain(|h| {
                        !h.get("command")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|c| c.ends_with(&suffix))
                    });
                    removed += before - cmds.len();
                }
            }
            entries.retain(|e| {
                e.get("hooks")
                    .and_then(serde_json::Value::as_array)
                    .is_none_or(|hooks| !hooks.is_empty())
            });
            if entries.is_empty() {
                hooks.remove(event);
            }
        }
    }
    removed
}

/// `sensor doctor`'s speaker section — read-only, like every other section.
///
/// The sensors' health had four sections and the speakers had none, even
/// though `thruline init` installs three speaker hooks beside the sensor pack.
/// That gap is not cosmetic: speakers are fail-open-silent by doctrine (any
/// error → exit 0, print nothing), so from inside a session a speaker layer
/// that has never produced a line is indistinguishable from one that had
/// nothing to say. This is the only place the difference can be stated, so an
/// absence — no cache directory, no findings table, no rows — is printed as an
/// absence and never swallowed.
fn speaker_doctor_section(settings_root: &serde_json::Value, db: &Path) {
    println!(
        "speakers         the in-session truth loop (`thruline init`) — fail-open by doctrine, so silence elsewhere proves nothing"
    );
    for (event, verb, _label) in SPEAKER_HOOKS {
        let state = if sensor_entry_present(settings_root, event, &speaker_suffix(verb)) {
            "registered"
        } else {
            "NOT registered"
        };
        println!("  {event:<18} {state} — thruline {verb}");
    }

    let dir = thruline_core::speaker::digest_dir(db);
    match std::fs::read_dir(&dir) {
        Err(e) => println!(
            "  digest cache     ABSENT — {} cannot be read ({e}); no brief has ever been written there, so `brief --hook session-start` has nothing to inject",
            dir.display()
        ),
        Ok(entries) => {
            let now = std::time::SystemTime::now();
            let mut cached: Vec<(String, String)> = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let key = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                let when = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| now.duration_since(m).ok())
                    .map(age_phrase)
                    .unwrap_or_else(|| "written at an unreadable time".to_string());
                cached.push((key, when));
            }
            cached.sort();
            if cached.is_empty() {
                println!(
                    "  digest cache     EMPTY — {} exists but holds no project brief; every speaker run so far had nothing to cache, or none has succeeded",
                    dir.display()
                );
            } else {
                println!("  digest cache     {}", dir.display());
                for (key, when) in &cached {
                    println!("    {key:<40} last written {when}");
                }
            }
        }
    }

    match Ledger::open(db) {
        Err(e) => println!("  findings         cannot say — the ledger could not be opened ({e})"),
        Ok(ledger) => {
            let conn = ledger.conn();
            let table: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='speaker_findings'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if table == 0 {
                println!(
                    "  findings         ABSENT — no `speaker_findings` table in {}; `audit --hook stop` has never recorded anything here",
                    db.display()
                );
                return;
            }
            match conn.query_row(
                "SELECT COUNT(*), MAX(recorded_at) FROM speaker_findings",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
            ) {
                Err(e) => println!("  findings         cannot say — {e}"),
                Ok((0, _)) => println!(
                    "  findings         0 rows in `speaker_findings` — the table exists and nothing has ever been appended to it"
                ),
                Ok((n, last)) => println!(
                    "  findings         {} in `speaker_findings` · last appended {}",
                    phrase_simple(n.max(0) as u64, "row", "rows"),
                    last.unwrap_or_else(|| "at an unrecorded time".to_string())
                ),
            }
        }
    }
}

/// A coarse, dependency-free age for the speaker section. The question there
/// is "has this ever been written, and recently?", which minutes/hours/days
/// answers without pulling a date formatter into the CLI.
fn age_phrase(d: std::time::Duration) -> String {
    let s = d.as_secs();
    if s < 90 {
        format!("{s} s ago")
    } else if s < 5_400 {
        format!("{} min ago", s / 60)
    } else if s < 172_800 {
        format!("{} h ago", s / 3_600)
    } else {
        format!("{} days ago", s / 86_400)
    }
}

/// The one key init owns in Claude Code's `mcpServers` object.
const MCP_SERVER_KEY: &str = "thruline";

fn init_claude_json_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".claude.json"))
}

/// An entry is recognizably init's exactly when its command resolves to the
/// `thruline` binary and its first argument is `mcp`. Removal matches nothing
/// broader — a foreign entry that happens to share the key is left alone.
fn init_mcp_entry_is_ours(entry: &serde_json::Value) -> bool {
    let command_is_thruline = entry
        .get("command")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|c| Path::new(c).file_name().is_some_and(|n| n == "thruline"));
    let first_arg_is_mcp = entry
        .get("args")
        .and_then(serde_json::Value::as_array)
        .and_then(|a| a.first())
        .and_then(serde_json::Value::as_str)
        == Some("mcp");
    command_is_thruline && first_arg_is_mcp
}

/// Merge our MCP entry into the config. Returns whether anything was written.
/// An existing `thruline` key — ours or not — is never overwritten.
fn init_mcp_install_into(root: &mut serde_json::Value, db: &Path) -> Result<bool> {
    let Some(root_obj) = root.as_object_mut() else {
        anyhow::bail!("the Claude Code config root is not a JSON object; refusing to rewrite it");
    };
    let servers = root_obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let Some(servers) = servers.as_object_mut() else {
        anyhow::bail!("`mcpServers` is not a JSON object; refusing to rewrite it");
    };
    if servers.contains_key(MCP_SERVER_KEY) {
        return Ok(false);
    }
    let exe = sensor_command_template()?;
    servers.insert(
        MCP_SERVER_KEY.to_string(),
        serde_json::json!({
            "type": "stdio",
            "command": exe,
            "args": ["mcp", "--agent", "claude", "--db", db.display().to_string()],
            "env": {}
        }),
    );
    Ok(true)
}

/// Remove exactly our MCP entry. Returns whether anything was removed.
fn init_mcp_uninstall_from(root: &mut serde_json::Value) -> bool {
    let Some(servers) = root
        .get_mut("mcpServers")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return false;
    };
    let ours = servers
        .get(MCP_SERVER_KEY)
        .is_some_and(init_mcp_entry_is_ours);
    if ours {
        servers.remove(MCP_SERVER_KEY);
    }
    ours
}

/// C1.2 — the lifecycle sensor's ops verbs.
///
/// `append` is the hot path: the vendor's process invokes it once per hook
/// event. Its contract is absolute — nothing on stdout ever (a SessionStart
/// hook's stdout is injected into the session as context), and exit 0 no
/// matter what (a nonzero hook exit surfaces inside the user's session). A
/// row that cannot reach the ledger is spilled to a sidecar file rather than
/// lost, and the failure goes to stderr — loud in the record, silent in the
/// session.
fn run_sensor(action: SensorAction, db: &Path) -> Result<ExitCode> {
    match action {
        SensorAction::Append { event, mirror_root } => {
            use std::io::Read;
            let mut payload = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut payload);
            let received_at = thruline_core::ingest::iso8601_utc_now();
            let stored = (|| -> anyhow::Result<()> {
                let mut ledger = Ledger::open(db)?;
                // C34 pause gate (1 of 6): a delivery whose `cwd` is under a
                // pause in force, or whose session started inside one, is left
                // unrecorded — no append, no pre-compaction seal, no liveness
                // marker — and it is not a failure, so nothing is spilled and
                // nothing is printed. The read fails OPEN: any error is "not
                // paused", because a delivery lost to a failed read is lost
                // for good. It is counted in a sidecar, never in the ledger.
                if let Some(folder) =
                    thruline_core::capture_pause::paused_delivery(&ledger, &payload)
                {
                    thruline_core::capture_pause::note_skip(
                        db,
                        thruline_core::capture_pause::SkipGate::HookDelivery,
                        &folder,
                        &received_at,
                    );
                    return Ok(());
                }
                let rec = thruline_core::sensor::append(&ledger, &event, &payload, &received_at)?;
                // C1.4 — the liveness marker: one small file per live session,
                // refreshed on every delivery, removed on the vendor's close.
                // A marker that outlives its process with no close on record
                // is `sensor doctor`'s crash candidate.
                let live_dir = db.with_file_name("sensor-live");
                if let Some(sid) = rec.session_id.as_deref() {
                    let marker_path = live_dir.join(format!("{sid}.json"));
                    if rec.hook_event == "SessionEnd" {
                        let _ = std::fs::remove_file(&marker_path);
                    } else {
                        let _ = std::fs::create_dir_all(&live_dir);
                        let marker = serde_json::json!({
                            "session_id": sid,
                            "transcript_path": rec.transcript_path,
                            "cwd": rec.cwd,
                            "vendor_pid": std::os::unix::process::parent_id(),
                            "last_event": rec.hook_event,
                            "updated_at": rec.received_at,
                        });
                        let _ = std::fs::write(&marker_path, marker.to_string());
                    }
                }
                // C1.3 — compaction insurance: the vendor is about to lossy-
                // summarize this transcript, so its full bytes are sealed into
                // the mirror NOW. Idempotent; loud-best-effort like every seal.
                if rec.hook_event == "PreCompact" {
                    if let Some(tp) = rec.transcript_path.as_deref() {
                        let path = PathBuf::from(tp);
                        if path.exists() {
                            let root = match &mirror_root {
                                Some(r) => r.clone(),
                                None => default_mirror_root()?,
                            };
                            let report =
                                thruline_core::ingest::seal_transcript(&mut ledger, &root, &path)?;
                            eprintln!(
                                "thruline sensor: pre-compaction seal — {} sealed, {} identical, {} failure(s)",
                                report.claude_sealed,
                                report.claude_seal_identical,
                                report.claude_seal_failures.len()
                            );
                        }
                    }
                }
                Ok(())
            })();
            if let Err(err) = stored {
                let spill = db.with_file_name("sensor-spill.jsonl");
                let line = serde_json::json!({
                    "received_at": received_at,
                    "hook_event": event,
                    "error": err.to_string(),
                    "payload": String::from_utf8_lossy(&payload),
                });
                let wrote = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&spill)
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "{line}")
                    });
                match wrote {
                    Ok(()) => eprintln!(
                        "thruline sensor: append failed ({err}); payload spilled to {}",
                        spill.display()
                    ),
                    Err(e) => {
                        eprintln!("thruline sensor: append failed ({err}); spill also failed ({e})")
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        SensorAction::Plan { settings } => {
            let path = sensor_settings_path(settings)?;
            let root = read_settings_or_empty(&path)?;
            let command_for = sensor_command_template()?;
            println!("SENSOR INSTALL PLAN — writes nothing");
            println!("settings         {}", path.display());
            println!("appender         {} sensor append <Event>", command_for);
            for event in thruline_core::sensor::install_set() {
                let installed = sensor_entry_present(&root, event, &command_suffix(event));
                let state = if installed {
                    "already registered"
                } else {
                    "will register"
                };
                if thruline_core::sensor::DORMANT_EVENTS.contains(&event) {
                    println!(
                        "  {event:<18} {state} — {}",
                        thruline_core::sensor::DORMANT_NOTE
                    );
                } else {
                    println!("  {event:<18} {state}");
                }
            }
            println!("apply with       thruline sensor install");
            Ok(ExitCode::SUCCESS)
        }
        SensorAction::Install { settings } => {
            let path = sensor_settings_path(settings)?;
            let mut root = read_settings_or_empty(&path)?;
            let (added, present) = sensor_install_into(&mut root, None)?;
            // Pay any schema migration HERE, visibly, before a single hook
            // fires. The live A168 install measured the alternative: the
            // first appender after a schema change spent 93 s migrating the
            // production ledger and every hook died at its 5 s timeout —
            // an ops verb can wait; the vendor's session must never.
            println!("opening the ledger once so any schema migration is paid here, not in a hook");
            Ledger::open(db).context("opening the ledger before hooks fire")?;
            if added.is_empty() {
                println!("nothing to do — every sensor entry is already registered");
                return Ok(ExitCode::SUCCESS);
            }
            write_settings(&path, &root)?;
            println!("registered       {}", added.join(", "));
            if !present.is_empty() {
                println!("already present  {}", present.join(", "));
            }
            println!("settings         {}", path.display());
            println!("remove with      thruline sensor uninstall");
            Ok(ExitCode::SUCCESS)
        }
        SensorAction::Doctor { settings } => {
            // C1.4 — read-only, exit 0, changes nothing. Candidate vocabulary
            // throughout: a marker whose process is gone and whose session has
            // no close on record is a crash CANDIDATE, with its evidence
            // stated — never a verdict.
            println!("SENSOR DOCTOR — read-only; it changes nothing");
            let path = sensor_settings_path(settings)?;
            let root = read_settings_or_empty(&path)?;
            println!("registration     {}", path.display());
            for event in thruline_core::sensor::install_set() {
                let installed = sensor_entry_present(&root, event, &command_suffix(event));
                let state = if installed {
                    "registered"
                } else {
                    "NOT registered"
                };
                if thruline_core::sensor::DORMANT_EVENTS.contains(&event) {
                    println!(
                        "  {event:<18} {state} — {}",
                        thruline_core::sensor::DORMANT_NOTE
                    );
                } else {
                    println!("  {event:<18} {state}");
                }
            }
            match Ledger::open(db) {
                Err(e) => println!("ledger           could not open {} — {e}", db.display()),
                Ok(ledger) => {
                    let rows = thruline_core::sensor::summary(&ledger).unwrap_or_default();
                    let total: u64 = rows.iter().map(|(_, n)| n).sum();
                    match thruline_core::sensor::verify_chain(&ledger) {
                        Ok(n) => println!(
                            "rows             {} · chain re-checked, {} add up",
                            phrase_simple(total, "delivery", "deliveries"),
                            phrase_simple(n, "link", "links")
                        ),
                        Err(e) => {
                            println!("rows             {total} · CHAIN DOES NOT ADD UP — {e}")
                        }
                    }
                    let live_dir = db.with_file_name("sensor-live");
                    let mut live = 0u64;
                    let mut stale = 0u64;
                    let mut candidates = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(&live_dir) {
                        for entry in entries.flatten() {
                            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                                continue;
                            };
                            let Ok(marker) = serde_json::from_str::<serde_json::Value>(&text)
                            else {
                                continue;
                            };
                            let sid = marker["session_id"].as_str().unwrap_or("").to_string();
                            let pid = marker["vendor_pid"].as_u64().unwrap_or(0);
                            let alive = pid > 0
                                && std::process::Command::new("kill")
                                    .args(["-0", &pid.to_string()])
                                    .output()
                                    .is_ok_and(|o| o.status.success());
                            let closed =
                                thruline_core::sensor::has_close(&ledger, &sid).unwrap_or(false);
                            if alive {
                                live += 1;
                            } else if closed {
                                stale += 1;
                            } else {
                                candidates.push((sid, marker));
                            }
                        }
                    }
                    println!(
                        "liveness         {} recording now · {} closed with a marker left behind",
                        live, stale
                    );
                    for (sid, marker) in &candidates {
                        println!(
                            "  crash candidate  {} — the vendor process is gone and no close was recorded · last event {} at {} · {}",
                            sid,
                            marker["last_event"].as_str().unwrap_or("?"),
                            marker["updated_at"].as_str().unwrap_or("?"),
                            marker["cwd"].as_str().unwrap_or("?"),
                        );
                    }
                    if candidates.is_empty() {
                        println!("  no crash candidates on this machine");
                    }
                }
            }
            let spill = db.with_file_name("sensor-spill.jsonl");
            match std::fs::read_to_string(&spill) {
                Ok(text) => {
                    println!(
                        "spill            {} at {} — these never reached the ledger",
                        phrase_simple(
                            text.lines().count() as u64,
                            "spilled delivery",
                            "spilled deliveries"
                        ),
                        spill.display()
                    );
                    // Stating a loss without stating what can be done about it
                    // reads as an offer of a recovery that does not exist.
                    // There is no replay verb; say so, rather than let the
                    // reader infer one.
                    println!(
                        "  recovery       NO replay verb exists — nothing in ThruLine reads this file back into the ledger, and this check will not either."
                    );
                    println!(
                        "  what you can do  keep the file (it is the only record of these deliveries) · each line carries the `error` that stopped it, so `grep -o '\"error\":\"[^\"]*\"' {} | sort | uniq -c` names the cause · a recurring `database is locked` means a long ledger writer was holding the database while hooks fired.",
                        spill.display()
                    );
                }
                Err(_) => println!("spill            none — every delivery reached the ledger"),
            }
            speaker_doctor_section(&root, db);
            Ok(ExitCode::SUCCESS)
        }
        SensorAction::Uninstall { settings } => {
            let path = sensor_settings_path(settings)?;
            let mut root = read_settings_or_empty(&path)?;
            let removed = sensor_uninstall_from(&mut root);
            if removed == 0 {
                println!(
                    "nothing to remove — no ThruLine sensor entries in {}",
                    path.display()
                );
                return Ok(ExitCode::SUCCESS);
            }
            write_settings(&path, &root)?;
            println!(
                "removed          {removed} sensor entry/entries from {}",
                path.display()
            );
            println!("recorded rows stay in the ledger — removal changes capture, never history");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// §4 outside the voice module: a count carries its noun.
fn phrase_simple(v: u64, singular: &str, plural: &str) -> String {
    format!("{v} {}", if v == 1 { singular } else { plural })
}

/// The Stop-gate entry `init --gate` writes, and the marker that owns it.
///
/// `gate_cmd::hook_entry` builds the entry, so the 10 s harness timeout is
/// stated in exactly one place and this does not restate it. The ownership
/// marker is the same shape as the sensors' and the speakers': an entry is ours
/// exactly when its command ends with the verb.
const GATE_VERB: &str = "gate --hook stop";

fn init_gate_install_into(
    root: &mut serde_json::Value,
    hook_db: Option<&Path>,
) -> Result<(Vec<&'static str>, Vec<&'static str>)> {
    let exe = std::env::current_exe().context("resolving the running binary")?;
    let db_flag = hook_db_flag(hook_db)?;
    if !root.is_object() {
        anyhow::bail!("settings root is not a JSON object; refusing to rewrite it");
    }
    let hooks = root
        .as_object_mut()
        .expect("checked above")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        anyhow::bail!("settings `hooks` is not an object; refusing to rewrite it");
    };
    let entries = hooks.entry("Stop").or_insert_with(|| serde_json::json!([]));
    let Some(entries) = entries.as_array_mut() else {
        anyhow::bail!("settings `hooks.Stop` is not an array; refusing to rewrite it");
    };
    let suffix = format!(" {GATE_VERB}");
    if entries.iter().any(|e| entry_carries_suffix(e, &suffix)) {
        return Ok((Vec::new(), vec!["Stop gate"]));
    }
    entries.push(gate_cmd::hook_entry_with(&exe, &db_flag));
    Ok((vec!["Stop gate"], Vec::new()))
}

fn init_gate_uninstall_from(root: &mut serde_json::Value) -> usize {
    let suffix = format!(" {GATE_VERB}");
    let mut removed = 0usize;
    let Some(hooks) = root
        .get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return 0;
    };
    if let Some(entries) = hooks
        .get_mut("Stop")
        .and_then(serde_json::Value::as_array_mut)
    {
        for entry in entries.iter_mut() {
            if let Some(cmds) = entry
                .get_mut("hooks")
                .and_then(serde_json::Value::as_array_mut)
            {
                let before = cmds.len();
                cmds.retain(|h| {
                    !h.get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|c| c.ends_with(&suffix))
                });
                removed += before - cmds.len();
            }
        }
        entries.retain(|e| {
            e.get("hooks")
                .and_then(serde_json::Value::as_array)
                .is_none_or(|hooks| !hooks.is_empty())
        });
        if entries.is_empty() {
            hooks.remove("Stop");
        }
    }
    removed
}

fn sensor_settings_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

fn sensor_command_template() -> Result<String> {
    let exe = std::env::current_exe().context("resolving the running binary")?;
    Ok(exe.display().to_string())
}

/// The ledger a hook command is pinned to, as it is spliced between the binary
/// and the verb: ` --db '<path>'`, or nothing when no ledger was named.
///
/// **Before the verb, never after it.** `--db` is a global flag, so either
/// place parses, but every recogniser of ThruLine's own entries — undo, the
/// "already there" check, the install page's switches — asks whether the command
/// *ends with* its verb (`sensor append Stop`, `gate --hook stop`). A trailing
/// `--db` would make `init --undo` unable to find what `init --db` wrote.
/// `doctor::is_recorder` only asks whether the verb is in there, so it reads
/// either form; it is the same place `capture hook` puts it.
///
/// Quoted with `shell_quote`, like that one: the harness runs the string
/// through a shell, and a path with a space in it would otherwise split into
/// two arguments and send the record to a file nobody named — silently, since
/// a hook exits 0 whatever happens. A path that is not UTF-8 is refused rather
/// than written lossily, for the same reason.
fn hook_db_flag(db: Option<&Path>) -> Result<String> {
    let Some(db) = db else {
        return Ok(String::new());
    };
    let text = db.to_str().with_context(|| {
        format!(
            "the ledger path {} is not valid UTF-8, so it cannot be written into a hook command",
            db.display()
        )
    })?;
    Ok(format!(" --db {}", shell_quote(text)))
}

/// The ownership marker: an entry is ours exactly when a command ends with
/// this suffix. Removal matches nothing broader (A142 — the one deletion
/// discipline applies to other people's config too).
fn command_suffix(event: &str) -> String {
    format!(" sensor append {event}")
}

fn read_settings_or_empty(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn write_settings(path: &Path, root: &serde_json::Value) -> Result<()> {
    // §14's collision, refused before the bytes land. See
    // `hook_install::refuse_double_recorder`: this is the same guard the §4.1
    // installer writes through, so "one recorder per event" cannot mean two
    // different things on the two install paths.
    hook_install::refuse_double_recorder(path, root)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let mut text = serde_json::to_string_pretty(root)?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn sensor_entry_present(root: &serde_json::Value, event: &str, suffix: &str) -> bool {
    root.get("hooks")
        .and_then(|h| h.get(event))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|entries| entries.iter().any(|e| entry_carries_suffix(e, suffix)))
}

fn entry_carries_suffix(entry: &serde_json::Value, suffix: &str) -> bool {
    entry
        .get("hooks")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|c| c.ends_with(suffix))
            })
        })
}

fn sensor_install_into(
    root: &mut serde_json::Value,
    hook_db: Option<&Path>,
) -> Result<(Vec<&'static str>, Vec<&'static str>)> {
    let exe = sensor_command_template()?;
    let db_flag = hook_db_flag(hook_db)?;
    if !root.is_object() {
        anyhow::bail!("settings root is not a JSON object; refusing to rewrite it");
    }
    let hooks = root
        .as_object_mut()
        .expect("checked above")
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        anyhow::bail!("settings `hooks` is not an object; refusing to rewrite it");
    };
    let mut added = Vec::new();
    let mut present = Vec::new();
    for event in thruline_core::sensor::install_set() {
        let entries = hooks.entry(event).or_insert_with(|| serde_json::json!([]));
        let Some(entries) = entries.as_array_mut() else {
            anyhow::bail!("settings `hooks.{event}` is not an array; refusing to rewrite it");
        };
        if entries
            .iter()
            .any(|e| entry_carries_suffix(e, &command_suffix(event)))
        {
            present.push(event);
            continue;
        }
        let mut entry = serde_json::json!({
            "hooks": [{
                "type": "command",
                "command": format!("{exe}{db_flag} sensor append {event}"),
                "timeout": 5
            }]
        });
        // The two tool events were probed with an explicit match-all; the
        // install reproduces the measured shape rather than a guessed one.
        if matches!(event, "PreToolUse" | "PostToolUse") {
            entry["matcher"] = serde_json::json!("*");
        }
        entries.push(entry);
        added.push(event);
    }
    Ok((added, present))
}

fn sensor_uninstall_from(root: &mut serde_json::Value) -> usize {
    let mut removed = 0usize;
    let Some(hooks) = root
        .get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return 0;
    };
    for event in thruline_core::sensor::install_set() {
        let suffix = command_suffix(event);
        if let Some(entries) = hooks
            .get_mut(event)
            .and_then(serde_json::Value::as_array_mut)
        {
            for entry in entries.iter_mut() {
                if let Some(cmds) = entry
                    .get_mut("hooks")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    let before = cmds.len();
                    cmds.retain(|h| {
                        !h.get("command")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|c| c.ends_with(&suffix))
                    });
                    removed += before - cmds.len();
                }
            }
            // Prune only shells our removal emptied; foreign entries keep
            // their bytes even when odd.
            entries.retain(|e| {
                e.get("hooks")
                    .and_then(serde_json::Value::as_array)
                    .is_none_or(|hooks| !hooks.is_empty())
            });
            if entries.is_empty() {
                hooks.remove(event);
            }
        }
    }
    removed
}

fn remove_receipt_owned_capture_hooks(
    mut root: serde_json::Value,
    accepted_config: &serde_json::Value,
) -> Result<(serde_json::Value, usize)> {
    let mut removed = 0usize;
    let accepted_hooks = accepted_config
        .get("hooks")
        .and_then(serde_json::Value::as_object)
        .context("accepted installation receipt has no hook object")?;
    if let Some(hooks) = root
        .get_mut("hooks")
        .and_then(serde_json::Value::as_object_mut)
    {
        for event in ["PreToolUse", "PostToolUse", "PostToolUseFailure"] {
            let expected: Vec<&serde_json::Value> = accepted_hooks
                .get(event)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter(|entry| is_managed_hook(entry))
                .collect();
            anyhow::ensure!(
                expected.len() <= 1,
                "accepted installation receipt has ambiguous managed entries"
            );
            if let Some(entries) = hooks
                .get_mut(event)
                .and_then(serde_json::Value::as_array_mut)
            {
                let managed: Vec<&serde_json::Value> = entries
                    .iter()
                    .filter(|entry| is_managed_hook(entry))
                    .collect();
                let exact = expected.first().map_or(0, |expected_entry| {
                    managed
                        .iter()
                        .filter(|entry| **entry == *expected_entry)
                        .count()
                });
                anyhow::ensure!(
                    managed.len() == exact && exact <= 1,
                    "ambiguous or non-receipt-owned ThruLine hook entry; refusing uninstall"
                );
                let before = entries.len();
                if let Some(expected_entry) = expected.first() {
                    entries.retain(|entry| entry != *expected_entry);
                }
                removed += before.saturating_sub(entries.len());
            }
        }
    }
    Ok((root, removed))
}

fn write_config_atomic(config: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = config.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = config.with_extension(format!("thruline-new-{}.json", std::process::id()));
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(&temp, &bytes)?;
    let file = fs::OpenOptions::new().read(true).open(&temp)?;
    file.sync_all()?;
    fs::rename(&temp, config)?;
    if let Some(parent) = config.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn is_managed_hook(value: &serde_json::Value) -> bool {
    value
        .get("hooks")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|command| command.contains(" capture hook --event "))
            })
        })
}

/// Walk the roots read-only: how many transcript files, how many bytes.
fn survey(roots: &[PathBuf]) -> (u64, u64) {
    fn walk(dir: &Path, files: &mut u64, bytes: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files, bytes);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                *files += 1;
                *bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    let (mut files, mut bytes) = (0u64, 0u64);
    for root in roots {
        walk(root, &mut files, &mut bytes);
    }
    (files, bytes)
}

/// Free bytes on the filesystem holding `path`, read from `df`.
fn free_bytes(path: &Path) -> Option<u64> {
    let out = std::process::Command::new("df")
        .arg("-k")
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let avail_kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kb * 1024)
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
}

/// A131(a) — the preview. Writes nothing, and says what applying would do.
fn ingest_preview(claude: &[PathBuf], codex: &[PathBuf], mirror_root: &Path) -> Result<ExitCode> {
    let (cf, cb) = survey(claude);
    let (xf, xb) = survey(codex);
    println!("INGEST PREVIEW — nothing has been read into the ledger, nothing written");
    if claude.is_empty() && codex.is_empty() {
        println!("  no roots in scope");
        return Ok(ExitCode::SUCCESS);
    }
    for (label, roots, files, bytes) in [("claude code", claude, cf, cb), ("codex", codex, xf, xb)]
    {
        if roots.is_empty() {
            continue;
        }
        println!(
            "  {label:<12} {} root(s) · {files} transcript file(s) · {}",
            roots.len(),
            gb(bytes)
        );
        for r in roots {
            println!("               {}", r.display());
        }
    }
    // A131(d) — the mirror is the larger half and lives outside the ledger the
    // operator knows about, so it is disclosed separately rather than folded in.
    let total = cb + xb;
    println!(
        "  ledger        grows by roughly {} of parsed events",
        gb(total / 2)
    );
    println!(
        "  mirror        grows by roughly {} of preserved bytes, at {}",
        gb(total),
        mirror_root.display()
    );
    match free_bytes(mirror_root).or_else(|| free_bytes(Path::new("/"))) {
        Some(free) => println!("  free space    {} available", gb(free)),
        None => println!("  free space    could not be read — check it yourself before applying"),
    }
    println!();
    println!("  nothing leaves this device. To go ahead:");
    println!("    thruline ingest --apply");
    println!("  a second vendor is never included unless you name it: --source codex");
    Ok(ExitCode::SUCCESS)
}

/// A131(c) — refuse rather than fill the disk. Returns the refusal to print.
fn ingest_budget_refusal(
    claude: &[PathBuf],
    codex: &[PathBuf],
    mirror_root: &Path,
) -> Result<Option<String>> {
    let (_, cb) = survey(claude);
    let (_, xb) = survey(codex);
    // The mirror keeps the bytes and the ledger keeps the parse: ~1.5x total,
    // and the headroom is deliberately generous because the failure mode this
    // guard exists for took a host down.
    let need = ((cb + xb) as f64 * 1.5) as u64;
    let Some(free) = free_bytes(mirror_root).or_else(|| free_bytes(Path::new("/"))) else {
        return Ok(None);
    };
    if need > free.saturating_sub(2 * 1_073_741_824) {
        return Ok(Some(format!(
            "REFUSED — this would need about {} and only {} is free.\n  \
             ThruLine stops rather than filling the disk (A131: an unbounded ingest once took \
             this host down).\n  Narrow the scope by naming a path, or free space first.",
            gb(need),
            gb(free)
        )));
    }
    Ok(None)
}

/// C34 pause gate (6 of 6) for `thruline ingest`: the paths ingest is about to
/// read, less the recordings a pause covers, and what was left out.
///
/// A path none of whose recordings is covered is passed through exactly as
/// given (a folder stays a folder), so a record with no pause — or a pause that
/// touches nothing here — runs the very same ingest on the very same paths. A
/// path with a recording left out becomes the list of the recordings kept. An
/// unreadable record is an error, not a pass: ingest can simply be run again,
/// and reading a folder the person paused is not undone by running it again.
fn gate_ingest_paths(
    ledger: &Ledger,
    claude: &[PathBuf],
    codex: &[PathBuf],
) -> Result<(
    Vec<PathBuf>,
    Vec<PathBuf>,
    thruline_core::capture_pause::PauseCounts,
)> {
    use thruline_core::capture_pause::{self as pause, PauseCounts, PauseGate};
    let gate = PauseGate::read(ledger).context("reading the pauses recorded for this machine")?;
    if gate.is_empty() {
        return Ok((claude.to_vec(), codex.to_vec(), PauseCounts::default()));
    }
    let mut counts = PauseCounts::default();
    let mut merge = |from: PauseCounts| {
        counts.paused_by_you += from.paused_by_you;
        counts.started_while_paused += from.started_while_paused;
        for (folder, (a, b)) in from.by_folder {
            let entry = counts.by_folder.entry(folder).or_default();
            entry.0 += a;
            entry.1 += b;
        }
    };
    let mut keep_claude = Vec::new();
    for path in claude {
        // A path ingest cannot list is left to ingest to refuse in its own words.
        let Ok(files) = pause::claude_files(path) else {
            keep_claude.push(path.clone());
            continue;
        };
        let (kept, left_out) = pause::filter_files(&gate, files);
        if left_out.total() == 0 {
            keep_claude.push(path.clone());
        } else {
            keep_claude.extend(kept);
        }
        merge(left_out);
    }
    let mut keep_codex = Vec::new();
    for path in codex {
        let (kept, left_out) = pause::filter_files(&gate, pause::codex_files(path));
        if left_out.total() == 0 {
            keep_codex.push(path.clone());
        } else {
            keep_codex.extend(kept);
        }
        merge(left_out);
    }
    Ok((keep_claude, keep_codex, counts))
}

/// The one line `thruline ingest` ends with when a pause left recordings out.
fn paused_ingest_summary(
    paused: &thruline_core::capture_pause::PauseCounts,
    nothing_read: bool,
) -> String {
    format!(
        "paused         {} recording(s) not read: {} because the folder is paused, {} because the session started while recording was paused{}",
        paused.total(),
        paused.paused_by_you,
        paused.started_while_paused,
        if nothing_read { " — nothing was read" } else { "" }
    )
}

fn select_ingest_paths(
    source: IngestSource,
    supplied: &[PathBuf],
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    if supplied.is_empty() {
        // A131(b), ruled by the C7 directive: a second vendor is NEVER in
        // scope without being named. Bare `ingest` means Claude Code only —
        // the incident that motivated this silently widened to the whole of
        // ~/.codex as well, appended 1.2M events, and filled the disk.
        return Ok(match source {
            IngestSource::Auto | IngestSource::ClaudeCode => (default_roots(), Vec::new()),
            IngestSource::Codex => (Vec::new(), default_codex_roots()),
        });
    }
    match source {
        IngestSource::ClaudeCode => Ok((supplied.to_vec(), Vec::new())),
        IngestSource::Codex => Ok((Vec::new(), supplied.to_vec())),
        IngestSource::Auto => {
            let mut claude = Vec::new();
            let mut codex = Vec::new();
            for path in supplied {
                let text = path.to_string_lossy();
                let looks_claude = text.split('/').any(|part| part == ".claude");
                let looks_codex = text.split('/').any(|part| part == ".codex");
                match (looks_claude, looks_codex) {
                    (true, false) => claude.push(path.clone()),
                    (false, true) => codex.push(path.clone()),
                    (false, false) => match detect_source_path(path).with_context(|| {
                        format!("auto-detecting transcript source for {}", path.display())
                    })? {
                        Source::ClaudeCode => claude.push(path.clone()),
                        Source::Codex => codex.push(path.clone()),
                    },
                    (true, true) => anyhow::bail!(
                        "auto source detection is ambiguous for {}; pass --source claude-code or --source codex",
                        path.display()
                    ),
                }
            }
            Ok((claude, codex))
        }
    }
}

fn print_ingest(db: &std::path::Path, r: &IngestReport) {
    // RB1.5 — the race against the sweep, logged per run (the per-file record
    // is the accepted-generation table itself).
    if r.claude_sealed + r.claude_seal_identical + r.claude_seal_rejected > 0
        || !r.claude_seal_failures.is_empty()
    {
        println!(
            "sealed   {} new generation(s) · {} already preserved · {} rejected",
            r.claude_sealed, r.claude_seal_identical, r.claude_seal_rejected
        );
    }
    for f in &r.claude_seal_failures {
        println!("!! SEAL FAILED (bytes NOT preserved): {f}");
    }
    println!("ledger   {}", db.display());
    println!("projects {}", r.projects.len());
    println!("sessions {}", r.sessions);
    println!(
        "lines    {} read · {} already ingested",
        r.lines_read, r.lines_skipped_already_ingested
    );
    println!("events   {} new", r.new_events);
    for (kind, n) in &r.events_by_kind {
        println!("           {kind:<20} {n}");
    }

    if r.accepted_generations > 0 || r.identical_observations > 0 || r.rejected_candidates > 0 {
        println!(
            "adapter  {} generation(s) accepted · {} identical no-op · {} rejected",
            r.accepted_generations, r.identical_observations, r.rejected_candidates
        );
        println!(
            "          {} disposition(s) · {} mirror suppression(s) · {} native replay suppression(s) · {} fallback message(s) · {} malformed · {} unknown",
            r.dispositions,
            r.suppressed_mirrors,
            r.suppressed_replays,
            r.fallback_messages,
            r.adapter_malformed,
            r.adapter_unknown
        );
        println!(
            "          {} quarantined envelope(s) · {} accounted total",
            r.quarantined_envelopes,
            r.dispositions + r.quarantined_envelopes
        );
        println!(
            "          spool flushes={} · max rows={} · max bytes={}",
            r.spool_flushes, r.max_spool_buffered_rows, r.max_spool_buffered_bytes
        );
        for (status, count) in &r.candidate_statuses {
            println!("          {status:<26} {count}");
        }
        for (family, count) in &r.adapter_families {
            println!("          family {family:<30} {count}");
        }
    }

    if r.unknown_types.is_empty() {
        println!("unknown  none");
    } else {
        println!("unknown  {} type(s) mapped to meta", r.unknown_types.len());
        for (t, n) in &r.unknown_types {
            println!("           {t:<26} {n}");
        }
    }

    // A207 (MANDATE-v4 §13): overflowed tool results hold only an excerpt —
    // the incompleteness is disclosed here, never silently absorbed.
    if r.overflowed_tool_results > 0 {
        println!(
            "overflow {} tool result(s) reference side files whose bytes were not \
             read — any search over them is incomplete until they are ingested",
            r.overflowed_tool_results
        );
    }

    if r.malformed.is_empty() {
        println!("malformed none");
    } else {
        println!("malformed {} line(s)", r.malformed.len());
        for m in &r.malformed {
            println!("           {}:{} — {}", m.file, m.line, m.reason);
        }
    }

    for (p, head) in &r.heads {
        println!("head     {p} {head}");
    }

    if !r.integrity_alarms.is_empty() {
        println!();
        println!(
            "!! INTEGRITY: {} previously ingested line(s) no longer match the ledger",
            r.integrity_alarms.len()
        );
        for a in &r.integrity_alarms {
            println!(
                "   {}:{}\n     stored {}\n     found  {}",
                a.file, a.line, a.stored_sha256, a.found_sha256
            );
        }
        println!("   Nothing was re-hashed. Investigate before trusting this project.");
    }

    println!("elapsed  {} ms", r.elapsed_ms);
}

fn print_verify(r: &VerifyReport) {
    for (project, v) in &r.projects {
        println!("project  {}", verify_cmd::display(project));
        println!("  events checked {}", v.events_checked);
        println!("  raw hashes     {} match", v.raw_matches);
        println!("  chain links    {} match", v.chain_matches);
        println!("  file digests   {} checked", v.digests_checked);
        println!("  head           {}", verify_cmd::display(&v.head_hash));
        for source in &v.sources {
            let seal = match source.sealed_copy {
                SealedCopyState::Intact => "sealed copy intact",
                SealedCopyState::Missing => "sealed copy missing",
                SealedCopyState::Corrupt => "sealed copy corrupt",
                SealedCopyState::Unverifiable => "sealed copy unverifiable",
                SealedCopyState::NotRecorded => "no sealed copy recorded",
            };
            if source.state == SourceState::GrewAfterRecording {
                println!(
                    "  source         {} — {} bytes not recorded; {seal}",
                    source.state.label(),
                    source.unrecorded_bytes.unwrap_or(0)
                );
            } else {
                println!("  source         {} — {seal}", source.state.label());
            }
            if let Some(reason) = &source.reason {
                println!("                 {}", verify_cmd::display(reason));
            }
        }

        if !v.missing_files.is_empty() {
            println!("  !! {} source file(s) unavailable:", v.missing_files.len());
            for f in &v.missing_files {
                println!("     {}", verify_cmd::display(f));
            }
        }
        if !v.raw_mismatches.is_empty() {
            println!("  !! {} raw-hash mismatch(es):", v.raw_mismatches.len());
            for a in &v.raw_mismatches {
                println!(
                    "     {}:{}\n       stored {}\n       found  {}",
                    verify_cmd::display(&a.file),
                    a.line,
                    verify_cmd::display(&a.stored_sha256),
                    verify_cmd::display(&a.found_sha256)
                );
            }
        }
        if !v.digest_mismatches.is_empty() {
            println!(
                "  !! {} recorded digest mismatch(es):",
                v.digest_mismatches.len()
            );
            for m in &v.digest_mismatches {
                println!("     {}", verify_cmd::display(m));
            }
        }
        if !v.chain_mismatches.is_empty() {
            println!("  !! {} chain mismatch(es):", v.chain_mismatches.len());
            for m in &v.chain_mismatches {
                println!("     {}", verify_cmd::display(m));
            }
        }
        // A150 (P1): the verdict names WHICH property failed. "FAIL" alone let
        // a dead-receipt sweep read as a damaged chain (A147).
        let chain_ok = v.chain_mismatches.is_empty();
        println!(
            "  chain          {}",
            if !chain_ok {
                "DAMAGED"
            } else if v.complete {
                "intact"
            } else {
                "not fully re-checked"
            }
        );
        if !v.missing_files.is_empty() {
            println!(
                "  receipts       {} source file(s) unavailable",
                v.missing_files.len()
            );
        }
        for reason in &v.unverifiable {
            println!("  unverifiable   {}", verify_cmd::display(reason));
        }
        let verdict = match v.exit_code() {
            0 => "INTACT",
            2 if !v.complete => "UNVERIFIABLE (verification incomplete)",
            2 => "UNVERIFIABLE (chain intact)",
            _ if !v.complete && chain_ok => "INTEGRITY MISMATCH (verification incomplete)",
            _ if chain_ok => "INTEGRITY MISMATCH (chain intact)",
            _ => "CHAIN DAMAGED",
        };
        println!("  verdict        {verdict}");
    }
    println!("elapsed  {} ms", r.elapsed_ms);
}

#[cfg(feature = "ort")]
fn print_eval(r: &thruline_core::eval::EvalReport, tag: &str) {
    println!("scorer     {tag}   thresholds hi {} · lo {}", r.hi, r.lo);
    println!(
        "pairs      {} scored · {} parsed / {} fallback premises",
        r.pairs_scored, r.parsed_premises, r.fallback_premises
    );
    println!();
    println!(
        "{:<4} {:<30} {:<30} {:<13} {:<13} {:>6} {:>3} fidelity",
        "", "case", "class", "expected", "actual", "score", "n"
    );
    for o in &r.outcomes {
        let file: String = o.file.chars().take(29).collect();
        println!(
            "{:<4} {:<30} {:<30} {:<13} {:<13} {:>6} {:>3} {}",
            if o.passed { "ok" } else { "FAIL" },
            file,
            o.class,
            o.expected,
            o.actual,
            o.score
                .map(|s| format!("{s:.3}"))
                .unwrap_or_else(|| "-".into()),
            o.candidates,
            o.fidelity.clone().unwrap_or_default()
        );
        if !o.passed {
            println!("     why: {}", o.why);
        }
    }
    println!();
    println!("cases      {}/{} as expected", r.passed(), r.outcomes.len());

    let (h, t) = r.contradicted_recall();
    println!(
        "gate       accusation-recall  {h}/{t} = {:.2}   (>= 0.90)",
        if t == 0 { 1.0 } else { h as f64 / t as f64 }
    );
    let (b, tt) = r.false_warning_rate();
    println!(
        "           false-accusation   {b}/{tt} = {:.2}   (must be ZERO)",
        if tt == 0 { 0.0 } else { b as f64 / tt as f64 }
    );
    let (cv, cvt) = r.coverage_recall();
    println!(
        "           coverage-recall    {cv}/{cvt} = {:.2}   (no-proof → unsupported)",
        if cvt == 0 {
            1.0
        } else {
            cv as f64 / cvt as f64
        }
    );
    let (dp, dpt) = r.dropped_precision();
    let (dr, drt) = r.dropped_recall();
    println!("           dropped precision  {dp}/{dpt}   recall {dr}/{drt}");
    println!(
        "           fulfilled proposals wrongly dropped: {}",
        r.paraphrase_wrongly_dropped()
    );
    println!();
    println!("per renderer shape:");
    for (shape, (ok, n)) in r.per_shape() {
        println!("           {shape:<10} {ok}/{n}");
    }
    println!();
    let fails = r.gate_failures();
    if fails.is_empty() {
        println!("VERDICT    PASS");
    } else {
        println!("VERDICT    FAIL");
        for f in &fails {
            println!("           {f}");
        }
    }
}
