//! T-DIST-4 — the licence verbs, the entitlement answer, and the ONE gated door.
//!
//! Sam's question: "can I still see my own work if I don't pay?" Yes. The only
//! thing a missing licence turns off is approving a version, and it is turned
//! off in exactly one function, [`approve_version`], which every door that can
//! approve calls — the Change screen's `POST /api/changes/<id>/decisions`
//! today, and any terminal door later. `crates/cli/tests/licence_cli.rs` holds
//! that to the code: `change_decision::approve` is reached from this file and
//! nowhere else in the CLI, and every route the UI server serves is walked
//! unlicensed with only that one refused.
//!
//! Nothing here opens a socket. The licence is a file in the ThruLine home,
//! verified by `thruline_licence::verify` against keys compiled into this
//! binary; the clock is the system clock. **No environment variable or flag
//! changes either in a release build**: the four test seams below (a trusted
//! key, the built-in keys removed, the paid gate turned off, and a pause
//! between an install's rename and its read-back) are `cfg(debug_assertions)`
//! and do not exist in a release binary. Three of them exist so a census can
//! keep its reference builds now that the shipped build has the gate ON.
//!
//! THE PAID GATE IS ON (P11: FI-34, D-238 — "don't ship v0.1.0 with the paid
//! gate disabled"). [`entitled`] is the one place a surface is asked about,
//! and it first asks [`paid_gate_on`]: with the gate off it answers `Allowed`
//! for every surface, reads no source. The gate is on when
//! `thruline_licence::PAID_GATE` says so (it is `true`; it was `false` under
//! R-2026-09-24-25 until P11) AND this binary trusts at least one key — a
//! build without a licence key is always off, and that safety stays: such a
//! build could never install a licence, so a gate in it would lock approving
//! for good. The shipped build carries the one production key, so it is on.
//! With the gate on, exactly the surfaces in `thruline_licence::PAID` refuse;
//! `crates/cli/tests/licence_cli.rs` walks every route and verb both ways
//! against one table.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use thruline_core::change_decision::{Decide, Outcome};
use thruline_core::ledger::Ledger;
use thruline_licence::{
    check_file, decide, fingerprint, verify, Entitlement, Fingerprint, Problem, Source, Surface,
    TrustedKey, Verdict, PAID, SOLD,
};

/// The installed licence's file name, inside the ThruLine home.
pub(crate) const LICENCE_FILE: &str = "licence.txt";
const FREE_STATUS: &str = "Free. Approving a version needs a Pro licence.";

/// `thruline licence …` (alias `license`).
#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Say, in one sentence, whether this machine has a licence that works.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Check a licence file and, only if it works, install it. A licence that
    /// does not work is never installed, so it can never replace one that does.
    Install {
        /// The licence file you downloaded from the licence page on ThruLine's
        /// website, with your licence key.
        file: PathBuf,
    },
    /// Delete the installed licence file. Nothing else is touched: every
    /// record ThruLine keeps stays exactly as it was.
    Remove,
    /// Show this machine's fingerprint, for a licence tied to one machine.
    /// It is printed here and nowhere else, and never sent anywhere.
    Fingerprint,
}

/* ── where, which keys, and when ──────────────────────────────────────── */

/// Where the installed licence lives, or `None` when there is no home at all.
pub(crate) fn installed_path() -> Option<PathBuf> {
    thruline_core::private_store::thruline_home().map(|h| h.join(LICENCE_FILE))
}

/// The keys this binary trusts: the committed production keys, plus — in a
/// DEBUG build only — the test seam.
pub(crate) fn trusted_keys() -> Vec<TrustedKey> {
    let mut keys = built_in_keys();
    keys.extend(test_seam_keys());
    keys
}

/// The keys compiled into this binary. In a DEBUG build a test can ask for
/// none of them (see [`built_in_keys`]'s seam below), which is a keyless build.
#[cfg(not(debug_assertions))]
fn built_in_keys() -> Vec<TrustedKey> {
    thruline_licence::production_keys()
}

/// The built-in-keys seam: `THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=1` makes a
/// DEBUG build trust none of the keys compiled into it, so a process-level
/// test can stand in the keyless build (whose gate is forced off) now that the
/// build as it ships carries the production key. Like every seam here, the
/// whole body, the variable's NAME included, is compiled only under
/// `debug_assertions`.
#[cfg(debug_assertions)]
fn built_in_keys() -> Vec<TrustedKey> {
    if std::env::var("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS").is_ok_and(|v| v == "1") {
        return Vec::new();
    }
    thruline_licence::production_keys()
}

/// The test seam, and why it cannot reach a release binary.
///
/// Process-level tests need the real binary to trust a key they generated a
/// moment ago. This reads one public key, as hex, from an environment variable
/// — and the whole function body, the variable's NAME included, is compiled
/// only under `debug_assertions`. `cargo build --release` (and the `dist`
/// profile, which inherits it) produces a binary in which the name does not
/// exist, so setting it does nothing.
#[cfg(debug_assertions)]
fn test_seam_keys() -> Vec<TrustedKey> {
    std::env::var("THRULINE_LICENCE_DEV_PUBLIC_KEY")
        .ok()
        .and_then(|hex| TrustedKey::from_hex(&hex).ok())
        .into_iter()
        .collect()
}

#[cfg(not(debug_assertions))]
fn test_seam_keys() -> Vec<TrustedKey> {
    Vec::new()
}

/// Whether this binary refuses anything for want of a licence at all.
///
/// On only when the flag says so (the compiled one, which is on, or, in a
/// DEBUG build, the test seam) AND at least one key is trusted: a build
/// without a licence key could never install a licence, so a gate in it would
/// lock approving for good.
pub(crate) fn paid_gate_on() -> bool {
    gate_flag() && !trusted_keys().is_empty()
}

/// The compiled flag. In a release build that is all it is.
#[cfg(not(debug_assertions))]
fn gate_flag() -> bool {
    thruline_licence::PAID_GATE
}

/// The paid-gate seam: `THRULINE_PAID_GATE_DEV=off` turns the gate OFF in a
/// DEBUG build (and `on` turns it on), so the process-level tests can keep
/// their gate-off reference build now that the flag ships ON. Anything else
/// leaves the compiled flag as it is. Like the key seams above, the name
/// exists only under `debug_assertions`.
#[cfg(debug_assertions)]
fn gate_flag() -> bool {
    match std::env::var("THRULINE_PAID_GATE_DEV").as_deref() {
        Ok("off") => false,
        Ok("on") => true,
        _ => thruline_licence::PAID_GATE,
    }
}

/// The last test seam: `THRULINE_LICENCE_DEV_AFTER_RENAME=<path>` makes
/// `licence install` read that path to its end after its rename and before
/// its read-back, in a DEBUG build. A test names a FIFO there, so the install
/// waits at exactly that point, with its licence in place, while the test
/// replaces or removes it; the read-back is then held to a test instead of to
/// timing luck (the race test cannot aim at a moment this short). It only
/// waits: it writes nothing, and without the variable it does nothing. Like
/// the seams above, the name exists only under `debug_assertions`.
#[cfg(debug_assertions)]
fn after_rename_seam() {
    if let Some(path) = std::env::var_os("THRULINE_LICENCE_DEV_AFTER_RENAME") {
        let _ = std::fs::read(path);
    }
}

#[cfg(not(debug_assertions))]
fn after_rename_seam() {}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// What the installed licence says, checked now.
pub(crate) fn installed_verdict() -> Verdict {
    let Some(path) = installed_path() else {
        return Verdict::Refused(Problem::Absent);
    };
    check_file(&path, &trusted_keys(), now_unix(), &fingerprint::local())
}

/* ── the entitlement answer ───────────────────────────────────────────── */

/// The installed licence, as a source. Reading it writes nothing.
fn licence_source() -> Source {
    let installed = installed_path().is_some_and(|p| p.exists());
    Source::Licence {
        verdict: installed_verdict(),
        installed,
    }
}

/// The installed licence is the only entitlement source in v0.1.1.
fn sources() -> Vec<Source> {
    vec![licence_source()]
}

/// Whether `surface` is open on this machine now, and if not, why and what
/// would open it. **The one entitlement call**, and where the paid gate is
/// read: off, every surface is `Allowed` and no source is asked.
fn entitled(surface: Surface) -> Entitlement {
    if !paid_gate_on() {
        return Entitlement::Allowed;
    }
    decide(surface, &sources())
}

/// What came of asking to approve.
pub(crate) enum Approval {
    /// The licence refused it. Nothing was appended and no file was written.
    Refused {
        reason: String,
        enable: String,
        status: String,
    },
    /// The licence allowed it, and this is what the record said.
    Acted(Outcome),
}

/// **The one gated door.** Approve exactly one version, if this machine is
/// entitled to; otherwise refuse before the ledger is touched.
///
/// Every door that can approve calls this and nothing else: the Change
/// screen's decision route today, a terminal verb tomorrow. Withdrawing an
/// approval does NOT come through here and is never gated — taking back what
/// you said is not a paid feature.
pub(crate) fn approve_version(
    ledger: &Ledger,
    change_id: &str,
    act: &Decide,
) -> thruline_core::error::Result<Approval> {
    if let Entitlement::Refused {
        reason,
        enable,
        status,
    } = entitled(Surface::ApproveVersion)
    {
        return Ok(Approval::Refused {
            reason,
            enable,
            status,
        });
    }
    thruline_core::change_decision::approve(ledger, change_id, act).map(Approval::Acted)
}

/* ── the one status document ──────────────────────────────────────────── */

/// The status, as `licence status --json` prints it and `GET /api/licence`
/// serves it. One function, so the terminal and the screen cannot disagree.
///
/// It carries no path and no fingerprint: the fingerprint is shown by
/// `thruline licence fingerprint` on this machine's own terminal only.
pub(crate) fn status_body() -> Value {
    let gate = paid_gate_on();
    let sources = sources();
    let verdict = match &sources[0] {
        Source::Licence { verdict, .. } => verdict.clone(),
    };
    let free = gate && matches!(&verdict, Verdict::Refused(Problem::Absent));
    let headline = if free { "free" } else { "licence" };
    let keys = trusted_keys().len();
    let approve = if gate {
        decide(Surface::ApproveVersion, &sources)
    } else {
        Entitlement::Allowed
    };
    let (plan, expires_at, enable, technical) = match &verdict {
        Verdict::Valid(l) => (
            json!(l.plan),
            json!(l.expires_at),
            Value::Null,
            json!({
                "key_id": l.key_id,
                "licence_id": l.licence_id,
                "machine_bound": l.machine_bound,
                "keys_built_in": keys,
            }),
        ),
        Verdict::Refused(p) => (
            Value::Null,
            Value::Null,
            json!(p.fix()),
            json!({
                "key_id": null,
                "licence_id": null,
                "machine_bound": null,
                "keys_built_in": keys,
            }),
        ),
    };
    let expired_on = match &verdict {
        Verdict::Refused(Problem::Expired { on }) => json!(on),
        _ => Value::Null,
    };
    json!({
        "status": verdict.slug(),
        "sentence": if free { FREE_STATUS.to_string() } else { verdict.sentence() },
        "enable": enable,
        "plan": plan,
        "expires_at": expires_at,
        "expires_on": match &verdict {
            Verdict::Valid(l) => json!(thruline_licence::day(&l.expires_at)),
            Verdict::Refused(_) => expired_on,
        },
        // Ruling 2 (D-298): the plans listed are the plans anyone can buy.
        // Dormant Team still unlocks approving (`PAID`), and is not listed.
        "paid": PAID.iter().map(|(s, plans)| json!({
            "surface": s.slug(),
            "plans": plans.iter().filter(|p| SOLD.contains(p)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "entitlements": {
            "approve_version": match approve {
                Entitlement::Allowed => json!({"allowed": true}),
                Entitlement::Refused { reason, enable, status } => json!({
                    "allowed": false,
                    "reason": reason,
                    "enable": enable,
                    "status": status,
                }),
            },
        },
        "headline": headline,
        // R-2026-09-24-25: whether anything refuses for want of a licence at
        // all. `true` in the build as it ships (P11: FI-34, D-238); `false`
        // only in a build without a licence key (or a DEBUG build asked for it
        // by seam): `paid` above is then what WOULD need one, and nothing does.
        "paid_gate": gate,
        "technical": technical,
    })
}

/// The one line `licence status` prints.
fn status_line() -> String {
    let verdict = installed_verdict();
    if paid_gate_on() && matches!(&verdict, Verdict::Refused(Problem::Absent)) {
        FREE_STATUS.to_string()
    } else {
        verdict.sentence()
    }
}

/* ── the verbs ────────────────────────────────────────────────────────── */

pub(crate) fn run(args: Args) -> Result<ExitCode> {
    match args.action {
        Action::Status { json } => {
            if json {
                crate::json_out::print(&crate::json_out::LICENCE_STATUS, status_body())?;
            } else {
                println!("{}", status_line());
            }
            Ok(ExitCode::SUCCESS)
        }
        Action::Install { file } => install(&file),
        Action::Remove => remove(),
        Action::Fingerprint => match fingerprint::local() {
            Fingerprint::Available(hex) => {
                println!("This machine's fingerprint, for a licence tied to this machine. ThruLine never sends it anywhere.");
                println!("{hex}");
                Ok(ExitCode::SUCCESS)
            }
            Fingerprint::Unavailable => {
                println!("ThruLine can't tell which machine this is, so a licence tied to one machine won't work here. Ask for a licence that isn't tied to one machine.");
                Ok(ExitCode::FAILURE)
            }
        },
    }
}

/// Verify first; install only what verified. A licence that does not work
/// never reaches the installed path, so it can never replace one that does.
///
/// The source is read ONCE, and the bytes that verified are the bytes written
/// (ADV-dist-install-1). It used to be read twice — once to verify, once to
/// copy — so a file whose bytes changed between the two reads was checked in
/// one form and installed in another, and a working licence could be replaced
/// by a tampered one while the command said "Installed.".
fn install(file: &std::path::Path) -> Result<ExitCode> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Not installed. There is no file at that path. Check the path and try again.");
            return Ok(ExitCode::FAILURE);
        }
        Err(_) => {
            let p = Problem::Unreadable;
            println!("Not installed. {} {}", p.problem(), p.fix());
            return Ok(ExitCode::FAILURE);
        }
    };
    let keys = trusted_keys();
    let verdict = if keys.is_empty() {
        Verdict::Refused(Problem::NoTrustedKey)
    } else {
        verify(&bytes, &keys, now_unix(), &fingerprint::local())
    };
    if let Verdict::Refused(p) = &verdict {
        println!("Not installed. {} {}", p.problem(), p.fix());
        return Ok(ExitCode::FAILURE);
    }
    let target =
        installed_path().context("no ThruLine home: set HOME (or THRULINE_HOME) and try again")?;
    // On a fresh home the licence may be the first thing ThruLine writes, so
    // the folder is made owner-only here, as every other ThruLine folder is
    // (ADV-dist-install-5): it used to follow the umask and come out 0755.
    if let Some(dir) = target.parent() {
        thruline_core::private_store::create_private_dir(dir)
            .with_context(|| format!("creating {}", dir.display()))?;
    }
    // Written to a staging file only this process uses and renamed over the
    // target in one step, so neither a half-written file nor a mixture of two
    // installs running at once is ever the installed licence. Two installs
    // used to share one staging name (ADV-dist-install-2): one could truncate
    // the other's half-written file, or rename it away from under it.
    let staging =
        target.with_file_name(format!("{LICENCE_FILE}.installing-{}", std::process::id()));
    place(&staging, &target, &bytes).with_context(|| format!("installing {}", target.display()))?;
    after_rename_seam();
    // Verify after the write: "Installed." is said only of the licence that is
    // in place. Another install, or a remove, landing between the rename and
    // this read changes what is there, and the person is told so instead.
    let in_place = match std::fs::read(&target) {
        Ok(in_place) => Some(in_place),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("reading back {}", target.display())),
    };
    if in_place.as_deref() != Some(bytes.as_slice()) {
        println!("{REPLACED_AT_ONCE}");
        return Ok(ExitCode::FAILURE);
    }
    println!("Installed. {}", verdict.sentence());
    Ok(ExitCode::SUCCESS)
}

/// What `licence install` says when the licence it installed is not the one
/// in place a moment later, because another install or a remove landed at the
/// same time. It was installed; it is not what this machine has now.
const REPLACED_AT_ONCE: &str = "This licence was installed, and at the same moment something else replaced or removed it. Run `thruline licence status` to see which licence this machine has now, and install this one again if it is the one you want.";

/// Put `bytes` at `target` by way of `staging`: create the staging file new,
/// write and sync it, then rename it over `target` in one step.
///
/// The staging name carries this process's id, so no other running ThruLine
/// process writes to it. One left behind by a process that died mid-write and
/// whose id this process now has is removed first, and a write that fails
/// removes its own.
///
/// The staging file is created owner-only (0600) before a byte is written,
/// and the rename keeps that mode, so the installed licence is never readable
/// by another account on this machine, not even for a moment. It used to
/// follow the umask and come out 0644 (ADV-dist-install-5); a licence tied to
/// one machine carries that machine's fingerprint.
fn place(staging: &std::path::Path, target: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let _ = std::fs::remove_file(staging);
    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(staging)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
        std::fs::rename(staging, target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(staging);
    }
    written
}

/// Delete the installed licence file, and nothing else.
fn remove() -> Result<ExitCode> {
    let Some(target) = installed_path() else {
        println!("No licence is installed, so there was nothing to remove.");
        return Ok(ExitCode::SUCCESS);
    };
    match std::fs::remove_file(&target) {
        Ok(()) => {
            println!("Removed the licence. Every record ThruLine keeps is untouched.");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("No licence is installed, so there was nothing to remove.");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => Err(e).with_context(|| format!("removing {}", target.display())),
    }
}
