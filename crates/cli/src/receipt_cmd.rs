//! `thruline receipt` and receipt verification (BUILD-SPEC §8, §14).
//!
//! # The predicate is built from ledger rows only
//!
//! BUILD-PLAN M2.1: *"the predicate is built from ledger rows only, never from
//! live process state — a receipt must be reproducible from the ledger alone."*
//! Nothing here reads the working directory except to compute the subject
//! digest, which is the one thing that must describe *now*.
//!
//! # Verification reports a level, and never rounds it up
//!
//! A reviewer holding only the repository and the receipt can check the
//! signature, the schema and that the subject really is this repository's tree.
//! They cannot check the event chain, because they do not have the ledger —
//! §8.1 says "against the local ledger **(if present)**".
//!
//! That is reported as a **distinct exit code**, not as success: `0` when every
//! check passed, `1` when one failed, and `2` when nothing failed and something
//! could not be checked. Collapsing `2` into `0` would be the exact failure this
//! subsystem is built against — success because it never checked.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_attest::dsse::{key_id, Envelope};
use thruline_attest::statement::{
    AssertedBy, CheckEntry, ClaimEntry, Predicate, Session, Statement, Subject, VerdictEntry,
    PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify as verify_envelope, LocalEvidence, Outcome};
use thruline_ledger::Ledger;

pub struct BuildArgs {
    pub predicate_version: String,
    pub session: Option<String>,
    pub sign_with: Option<PathBuf>,
    pub out: Option<PathBuf>,
    pub trailer: bool,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub json: bool,
}

pub struct VerifyArgs {
    pub receipt: PathBuf,
    pub key: Option<PathBuf>,
    pub offline: bool,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub json: bool,
    /// `text` | `json` | `md` | `check-run` | `badge`.
    ///
    /// **Every one of these is a rendering of the SAME conclusion.** The
    /// Action consumes `check-run`, which carries GitHub's conclusion computed
    /// by `attest::report`; nothing downstream re-derives a verdict from the
    /// checks array, which is how a partially-verified receipt was once read as
    /// verified.
    pub format: String,
    /// The commit a check run is about. Falls back to `$GITHUB_SHA`.
    pub head_sha: Option<String>,
}

/// Default key locations under `$THRULINE_HOME`.
fn key_paths() -> Result<(PathBuf, PathBuf)> {
    let home = crate::shim_cmd::thruline_home()?;
    Ok((home.join("signing.key"), home.join("signing.pub")))
}

pub fn build(args: BuildArgs) -> Result<ExitCode> {
    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let repo = args
        .repo
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let session = match args.session {
        Some(s) => s,
        None => crate::claims_cmd::last_session(&ledger)?.context("no session in the store")?,
    };
    let (harness, model) = if args.predicate_version == "0.2" {
        let (harness, model, repository): (String, Option<String>, String) =
            ledger.conn().query_row(
                "SELECT harness, model, repo_id FROM sessions WHERE id = ?1",
                [&session],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        anyhow::ensure!(
            repository
                == thruline_ledger::RepoId::from_key(&repo.canonicalize()?.to_string_lossy())
                    .as_str(),
            "session belongs to a different repository; specify the matching --repo"
        );
        (harness, model)
    } else {
        ("claude-code".into(), None)
    };

    // The chain head is RECOMPUTED, never read from the `sessions` cache — the
    // cache is maintained at append and is the ledger's claim about itself.
    let head = ledger
        .verify_session(&session)
        .with_context(|| format!("recomputing the chain for {session}"))?;

    let events = ledger.session_events(&session)?;
    let view = crate::claims_cmd::session_view(&session, &events, &ledger, &repo)?;
    let snapshot = thruline_core::spec::snapshot::snapshot_tree(&repo)
        .context("snapshotting the working tree for the receipt's subject")?;

    // **The snapshotter's finding, kept.** This call used to return a
    // `TreeSnapshot` whose `content_filters` field was dropped on the floor:
    // the snapshotter printed `WARN snapshot taken under content filters: the
    // tree hash may not name the bytes on disk`, and eight lines below that
    // warning `verify` printed `Verified.` A `.gitattributes` clean filter is
    // the one thing that breaks §0.3's tree binding at its root, so a receipt
    // silent about it attests to bytes nobody looked at.
    let content_filters = snapshot
        .content_filters
        .degrades_capture()
        .then(|| snapshot.content_filters.describe());

    // §8.1's `policy_hash`, which was `String::new()` here — bound to nothing,
    // and read by the verifier as "no policy claim". A policy rewritten to its
    // literal opposite produced byte-identical `Verified.` output.
    //
    // **An unparseable policy file is an error, not an absence.** Downgrading
    // it to `absent_digest()` would let a repository that intended to enforce
    // rules issue a receipt saying it had none.
    let policy_hash = thruline_policy::digest_for_repo(&repo).with_context(|| {
        format!(
            "resolving the policy this receipt would be produced under ({}). A receipt \
             that cannot name its policy cannot be bound to one, so this is refused \
             rather than left blank.",
            thruline_policy::policy_path(&repo).display()
        )
    })?;

    let mut digest = std::collections::BTreeMap::new();
    digest.insert("gitTree".to_string(), snapshot.tree_hash.clone());
    if let Some(commit) = &snapshot.head_commit {
        digest.insert("gitCommit".to_string(), commit.clone());
    }

    let checks = ledger.checks_for_session(&session)?;
    // §7's `reexecuted`, **derived from the check record and never set**.
    //
    // §8.1 puts `container_image_digest` on the Check, and `thruline reexec`
    // writes it there. This is the ONLY producer: there is no argument, flag or
    // field anywhere on this path that turns the boolean on, and `verify`
    // refuses a receipt carrying one without the other in either direction.
    //
    // Read by check id rather than by exec event id: a re-executed check has no
    // exec event (see `reexec_cmd`'s module docs on why a re-execution leaves
    // the chain untouched), and keying on an empty id would attach the digest
    // to every other unbound check at once.
    let image_digests: std::collections::BTreeMap<String, String> = checks
        .iter()
        .filter_map(|c| {
            let parsed: serde_json::Value = serde_json::from_str(&c.parsed).ok()?;
            let digest = parsed
                .get("reexec")?
                .get("image_digest")?
                .as_str()?
                .to_string();
            (!digest.is_empty()).then(|| (c.id.clone(), digest))
        })
        .collect();
    let any_reexecuted = !image_digests.is_empty();
    let predicate = Predicate {
        session: Session {
            id: session.clone(),
            harness: harness.clone(),
            model: model.clone(),
            started: events.first().map(|e| e.ts().to_string()),
            ended: events.last().map(|e| e.ts().to_string()),
            event_chain_head: head.head_hash.clone(),
            event_count: head.event_count,
        },
        claims: Vec::new(),
        checks: checks
            .iter()
            .map(|c| CheckEntry {
                id: c.id.clone(),
                category: c.category.clone(),
                runner: c.runner.clone(),
                argv_hash: c.argv_hash.clone(),
                env_hash: c.env_hash.clone(),
                cwd_hash: c.cwd_hash.clone(),
                input_tree_hash: c.input_tree_hash.clone(),
                exit_code: c.exit_code,
                parsed: serde_json::from_str(&c.parsed).unwrap_or(serde_json::Value::Null),
                // Both fields come from the same lookup, so they cannot
                // disagree — and `verify` refuses either one without the other.
                reexecuted: image_digests.contains_key(&c.id),
                container_image_digest: image_digests.get(&c.id).cloned(),
            })
            .collect(),
        verdicts: Vec::new(),
        promises: Default::default(),
        verifier: thruline_attest::statement::VerifierInfo {
            thruline_version: env!("CARGO_PKG_VERSION").to_string(),
            // §8.1's `mode`. Derived from the same lookup as `reexecuted`, so
            // a receipt cannot say `clean-sandbox-reexec` with no re-executed
            // check behind it.
            mode: if any_reexecuted {
                "clean-sandbox-reexec".into()
            } else {
                "local".into()
            },
            policy_hash: Some(policy_hash),
        },
        capture: thruline_attest::statement::Capture {
            sources: vec!["claude-code-hook".into(), "exec-recorder".into()],
            transcript_hash: None,
            redactions_applied: 0,
            // Degraded if the session said so **or** if the tree this receipt
            // is about was snapshotted under a content transform: both mean the
            // evidence does not name what a reader will assume it names.
            capture_degraded: view.capture_degraded || content_filters.is_some(),
            // §8.1's `degraded_causes`. Assembled from BOTH producers, because
            // a session can be degraded for its own reason and snapshotted
            // under a content transform at the same time, and a reader handed
            // one of the two takes the wrong next step. Empty exactly when the
            // flag above is false — the schema enforces both directions.
            degraded_causes: degraded_causes(&view, content_filters.as_deref()),
            content_filters,
            // C.7's S3, §8.1's field. A reviewer on another machine can see the
            // conditions the evidence was produced under — including the rows
            // of C.7's table that live OUTSIDE the tree and that no diff-based
            // detector can reach. `None` only when git itself could not be run,
            // which is already an error the snapshot above would have raised.
            git_env_digest: thruline_core::spec::git_env::git_env(&repo)
                .ok()
                .map(|e| e.digest),
        },
    };

    // Claims and verdicts come from the same path `thruline claims` uses, so a
    // receipt and the CLI can never disagree about what was claimed.
    let mut predicate = predicate;
    let mut origins = Vec::new();
    // **IR-7.** `judge` derives every verdict against the SESSION'S FINAL
    // SNAPSHOT. This receipt's subject is the WORKING TREE AT RECEIPT TIME —
    // `gitTree`, set from `snapshot.tree_hash` far above. Those are two
    // different trees the moment anything is written after the tests ran, and
    // the acceptance walk writes something on every run, because `policy
    // --init` is the step immediately before this one.
    //
    // Neither derivation is wrong on its own. What was wrong is asserting the
    // first one's answer about the second one's subject, which is what this
    // document did the instant it was signed — and `verify-receipt` said so one
    // command later, re-deriving against `gitTree` and contradicting a receipt
    // this binary had just produced and signed.
    //
    // §6.1's rules bind a claim to the tree its check ran on. Rule 1 is tree
    // equality and rule 2 is "no relevant change since"; neither reaches a tree
    // the check did not run on, and rule 2 over a changed source path is a
    // larger claim that would need its own evidence. So the verdict moves DOWN
    // and never up, and the sentence records which tree it was true of, which
    // tree this receipt is about, and why the rule does not carry across.
    let receipt_tree = snapshot.tree_hash.clone();
    for (claim, mut record) in crate::claims_cmd::judge(&session, &events, &view, &repo)? {
        if matches!(record.verdict, thruline_verify::verdict::Verdict::Supported) {
            if let Some(check) = checks
                .iter()
                .find(|c| record.evidence.iter().any(|e| e == &c.id))
            {
                if check.input_tree_hash != receipt_tree {
                    record.verdict = thruline_verify::verdict::Verdict::Unverifiable;
                    record.reasons.push(format!(
                        "lowered from `supported` for this receipt (§6.1): the check ran on \
                         tree {}, and this receipt is about tree {}. The rule that made it \
                         `supported` was applied against the session's final snapshot, which \
                         is not this receipt's subject. Rule 1 is tree equality and does not \
                         hold here; rule 2 (\"no relevant change since\") is a larger claim \
                         about paths this receipt records no coverage for, so it is not \
                         available either.",
                        check.input_tree_hash, receipt_tree
                    ));
                } else {
                    // §6.1 wants the producer to say WHICH rule carried the
                    // claim, and IR-7 is the case that requirement exists for:
                    // D could read the verdict and the hashes and still not
                    // tell whether rule 1 had been misapplied or rule 2 had
                    // been stretched.
                    //
                    // After the branch above, the answer is not a judgement
                    // call. A `supported` that survives into a receipt has a
                    // check whose input tree IS this receipt's subject, so
                    // rule 1 holds by construction and rule 2 — which exists
                    // precisely to cover a tree that moved — cannot be what
                    // carried it. That is worth writing down rather than
                    // leaving to be re-derived from two hashes.
                    record.reasons.push(format!(
                        "§6.1 rule 1 applied: the check ran on tree {receipt_tree}, which is \
                         this receipt's subject. Rule 2 (\"no relevant change since\") was \
                         not used and cannot be, because a receipt only keeps `supported` \
                         where the trees are equal."
                    ));
                }
            }
        }
        origins.push(thruline_attest::session_v2::ClaimOrigin {
            claim_id: claim.id.clone(),
            extractor: match claim.extractor {
                thruline_claims::claim::Extractor::Rules => {
                    thruline_attest::session_v2::Extractor::Rules
                }
                thruline_claims::claim::Extractor::LocalModel(_) => {
                    thruline_attest::session_v2::Extractor::LocalModel
                }
            },
        });
        predicate.claims.push(ClaimEntry {
            id: claim.id.clone(),
            kind: claim.kind.as_str().to_string(),
            text: claim.text.clone(),
            asserted_by: AssertedBy {
                harness: harness.clone(),
                model: model.clone(),
            },
        });
        predicate.verdicts.push(VerdictEntry {
            claim_id: record.claim_id.clone(),
            verdict: record.verdict.as_str().to_string(),
            verifier: record.verifier.clone(),
            evidence: record.evidence.clone(),
            reasons: record.reasons.clone(),
            warnings: record.warnings.clone(),
            findings: record
                .findings
                .iter()
                .filter_map(|f| serde_json::to_value(f).ok())
                .collect(),
        });
    }

    if args.predicate_version == "0.2" {
        let redactor = thruline_capture::redaction::Redactor::from_repo(&repo)?;
        predicate.capture.sources = events
            .iter()
            .map(|event| {
                if matches!(event.source(), thruline_core::spec::event::Source::Other(_)) {
                    Ok("unknown-source".to_string())
                } else {
                    serde_json::to_value(event.source())
                        .map(|value| value.as_str().unwrap_or("unknown-source").to_string())
                }
            })
            .collect::<std::result::Result<std::collections::BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        for cause in &mut predicate.capture.degraded_causes {
            *cause = redactor.redact(cause).into_text();
        }
        if let Some(filters) = &mut predicate.capture.content_filters {
            *filters = redactor.redact(filters).into_text();
        }
        for claim in &mut predicate.claims {
            claim.text = format!("{} claim {} (original text omitted)", claim.kind, claim.id);
        }
        for verdict in &mut predicate.verdicts {
            verdict.findings.clear();
            for reason in verdict
                .reasons
                .iter_mut()
                .chain(verdict.warnings.iter_mut())
            {
                *reason = redactor.redact(reason).into_text();
            }
        }
    }
    let statement = Statement::new(
        vec![Subject {
            name: format!("git+{}", repo.display()),
            digest,
        }],
        predicate,
    );
    let payload = if args.predicate_version == "0.2" {
        let versioned = thruline_attest::session_v2::SessionStatement::new(statement, origins)
            .map_err(anyhow::Error::msg)?;
        thruline_core::spec::canonical::canonical_bytes(&versioned)?
    } else {
        serde_json::to_vec(&statement)?
    };

    let (default_priv, default_pub) = key_paths()?;
    let key_path = args.sign_with.unwrap_or(default_priv);
    let key = match thruline_attest::key::load_signing(&key_path) {
        Ok(k) => k,
        Err(_) => {
            let k = thruline_attest::key::generate(&key_path, &default_pub)
                .with_context(|| format!("generating a signing key at {}", key_path.display()))?;
            eprintln!(
                "generated a signing key at {} (public half at {})",
                key_path.display(),
                default_pub.display()
            );
            k
        }
    };
    let envelope = thruline_attest::dsse::sign(
        &payload,
        PAYLOAD_TYPE,
        &key,
        Some(key_id(&key.verifying_key())),
    );
    let json = serde_json::to_string_pretty(&envelope)?;

    let destination = if args.predicate_version == "0.2" && args.out.is_none() && !args.json {
        Some(
            crate::shim_cmd::thruline_home()?
                .join("receipts")
                .join(format!(
                    "{}.json",
                    thruline_core::chain::sha256_hex(json.as_bytes())
                )),
        )
    } else {
        args.out.clone()
    };
    if args.predicate_version == "0.2" {
        if let Some(path) = &destination {
            let path = crate::acceptance_cmd::resolved_path(path)?;
            anyhow::ensure!(!path.starts_with(repo.canonicalize()?), "new receipts must be detached outside the checked source; explicit --predicate-version 0.1 retains legacy trailer semantics");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)?;
            use std::io::Write;
            file.write_all(format!("{json}\n").as_bytes())?;
            file.sync_all()?;
        }
    }
    match &destination {
        Some(path) => {
            if args.predicate_version == "0.1" {
                std::fs::write(path, format!("{json}\n"))
                    .with_context(|| format!("writing {}", path.display()))?;
            }
            if !args.json {
                println!("wrote {}", path.display());
            }
        }
        None => println!("{json}"),
    }
    if args.trailer {
        println!("{}", thruline_attest::trailer::line(json.as_bytes(), None));
    }
    Ok(ExitCode::SUCCESS)
}

/// The environment variable through which `.thruline/policy.yml`'s conclusion
/// reaches the receipt renderings.
///
/// # Why an environment variable and not a flag
///
/// Operator ruling point 3: *"with a policy file present, the conclusion comes
/// from the policy."* The word is produced by **`thruline policy --conclusion`**
/// and carried here — it is never re-derived, so there is exactly one producer
/// of a policy verdict in the product and `verify-receipt` cannot disagree with
/// `policy` about what the policy said. The Action passes it the same way it
/// already passes `THRULINE_RECEIPT` and `THRULINE_KEY`: a step `env:` block.
///
/// Empty or unset means **no policy was evaluated**. Anything else that is not
/// one of the three words is a hard error, not a shrug: a gate that reads a
/// word it does not understand as "fine" passes whenever it breaks.
pub const POLICY_CONCLUSION_ENV: &str = "THRULINE_POLICY_CONCLUSION";

fn policy_conclusion() -> Result<Option<thruline_attest::report::PolicyConclusion>> {
    let raw = std::env::var(POLICY_CONCLUSION_ENV).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    thruline_attest::report::PolicyConclusion::parse(raw)
        .map(Some)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{POLICY_CONCLUSION_ENV}={raw:?} is not one of success, neutral or failure. \
                 It is refused rather than ignored: a gate that reads a word it does not \
                 understand as `fine` passes exactly when it has broken."
            )
        })
}

pub fn verify(args: VerifyArgs) -> Result<ExitCode> {
    let envelope =
        crate::acceptance_receipt::read(&args.receipt).context("reading the DSSE receipt")?;
    if crate::acceptance_receipt::is_v2(&envelope) {
        return crate::acceptance_receipt::verify(args, envelope);
    }

    let repo = args
        .repo
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut keys = Vec::new();
    let mut key_paths_used: Vec<PathBuf> = Vec::new();
    let (_, default_pub) = key_paths()?;
    for path in [args.key.clone(), Some(default_pub)].into_iter().flatten() {
        if let Ok(k) = thruline_attest::key::load_verifying(&path) {
            keys.push(k);
            key_paths_used.push(path);
        }
    }

    let mut local = local_evidence(
        &repo,
        &envelope,
        args.exec_db.as_deref(),
        args.head_sha.as_deref(),
        &args.receipt,
    );
    local.verifying_key_in_repo = key_paths_used
        .iter()
        .find_map(|p| key_path_inside_repo(&repo, p));
    let report = verify_envelope(&envelope, &keys, &local);
    let statement: Option<thruline_attest::statement::Statement> =
        envelope.payload_bytes().ok().and_then(|b| {
            thruline_attest::session_v2::decode(&b)
                .ok()
                .map(|decoded| decoded.statement)
        });

    // Renderings of **one** conclusion, over three axes: the receipt's
    // integrity (`report.verified()`), the receipt's own claim verdicts, and
    // `.thruline/policy.yml` when one was evaluated. `conclusion_of` combines
    // them downward only — no axis can lift another.
    let policy = policy_conclusion()?;
    if let Some(versioned) = envelope
        .payload_bytes()
        .ok()
        .and_then(|bytes| thruline_attest::session_v2::decode(&bytes).ok())
        .and_then(|decoded| decoded.provenance)
    {
        return render_session_v2(&args, &repo, report, &versioned, policy);
    }
    let overall = thruline_attest::report::conclusion_of(&report, statement.as_ref(), policy);
    let format = if args.json {
        "json"
    } else {
        args.format.as_str()
    };
    match format {
        "json" => {
            crate::json_out::print(
                &crate::json_out::VERIFY_RECEIPT,
                verify_json(&report, statement.as_ref(), policy),
            )?;
            return Ok(ExitCode::from(report.verified().exit_code()));
        }
        "md" => {
            println!(
                "{}",
                thruline_attest::report::markdown_of(&report, statement.as_ref(), policy)
            );
            return Ok(ExitCode::from(report.verified().exit_code()));
        }
        "check-run" => {
            let sha = args
                .head_sha
                .clone()
                .or_else(|| std::env::var("GITHUB_SHA").ok());
            println!(
                "{}",
                serde_json::to_string_pretty(&thruline_attest::report::check_run(
                    &report,
                    statement.as_ref(),
                    sha.as_deref(),
                    policy,
                ))?
            );
            return Ok(ExitCode::from(report.verified().exit_code()));
        }
        "badge" => {
            print!("{}", thruline_attest::report::badge_of(overall));
            return Ok(ExitCode::from(report.verified().exit_code()));
        }
        "text" => {}
        other => {
            anyhow::bail!("unknown --format {other:?}: expected text, json, md, check-run or badge")
        }
    }

    {
        for c in &report.checks {
            let (mark, detail) = match &c.outcome {
                Outcome::Pass { detail } => ("ok  ", detail.as_str()),
                Outcome::Fail { reason } => ("FAIL", reason.as_str()),
                Outcome::Skipped { reason } => ("skip", reason.as_str()),
            };
            println!("{mark} {:<18} {detail}", c.name);
        }
        println!();
        if args.offline {
            // Asserted rather than branched on: nothing in this path opens a
            // socket, so `--offline` describes what already happened. A Rekor
            // lookup would be the first thing that could change that, and it
            // does not exist — so the flag can never silently degrade to an
            // online check.
            println!("(offline: no network was used, and none is reachable from this path)");
        }
        // **One conclusion, matched totally.** This was `if report.ok()` with
        // two more branches behind it — a second predicate beside the one the
        // JSON, the exit code, the markdown and the check run all read. A
        // `match` on `report.verified()` means the sentence and the exit status
        // are the same decision, and `Verified::Yes` is the only arm that can
        // print the word: a single `Skipped` check anywhere — an `unverifiable`
        // verdict, a content filter in scope, a policy the receipt cannot name
        // — takes the `Partial` arm and exits 2.
        match report.verified() {
            thruline_attest::verify::Verified::Yes => {
                println!("Verified. Every check the receipt asserts was confirmed here.");
            }
            thruline_attest::verify::Verified::Partial => {
                println!(
                    "PARTIALLY verified: nothing contradicts the receipt, and {} check(s) \
                     could not be made on this machine. That is not the same as verified.",
                    report.skipped().len()
                );
            }
            thruline_attest::verify::Verified::No => {
                println!("NOT verified: {} check(s) failed.", report.failed().len());
            }
        }
    }

    // §14's codes, with a third for the honest middle. Collapsing it into 0
    // would be success because nothing was checked.
    //
    // The code comes from the same `Verified` the JSON carries, so the two
    // cannot drift — a consumer reading the JSON and a consumer reading `$?`
    // are reading the same conclusion.
    Ok(ExitCode::from(report.verified().exit_code()))
}

/// New session renderings preserve extractor and integrity ceilings together.
fn render_session_v2(
    args: &VerifyArgs,
    repo: &Path,
    mut report: thruline_attest::verify::Report,
    session: &thruline_attest::session_v2::SessionStatement,
    policy: Option<thruline_attest::report::PolicyConclusion>,
) -> Result<ExitCode> {
    use thruline_attest::session_v2::{ClaimOrigin, Extractor};
    let statement = session.compatibility_view();
    let local_origins = (|| -> Result<Vec<ClaimOrigin>> {
        let db = args.exec_db.clone().unwrap_or(Ledger::default_path()?);
        let ledger = Ledger::open_read_only(&db)?;
        let id = &statement.predicate.session.id;
        ledger.verify_session(id)?;
        let events = ledger.session_events(id)?;
        let view = crate::claims_cmd::session_view(id, &events, &ledger, repo)?;
        let judged = crate::claims_cmd::judge(id, &events, &view, repo)?;
        anyhow::ensure!(
            judged.len() == statement.predicate.verdicts.len()
                && judged.iter().all(|(claim, verdict)| {
                    statement.predicate.verdicts.iter().any(|signed| {
                        signed.claim_id == claim.id && signed.verdict == verdict.verdict.as_str()
                    })
                }),
            "signed claim outcomes do not reproduce locally"
        );
        Ok(judged
            .into_iter()
            .map(|(claim, _)| ClaimOrigin {
                claim_id: claim.id,
                extractor: match claim.extractor {
                    thruline_claims::claim::Extractor::Rules => Extractor::Rules,
                    thruline_claims::claim::Extractor::LocalModel(_) => Extractor::LocalModel,
                },
            })
            .collect())
    })();
    let origins_match = local_origins
        .as_ref()
        .is_ok_and(|origins| origins == &session.predicate.claim_origins);
    if !origins_match {
        report.checks.push(thruline_attest::verify::Check { name: "claim origin and outcome custody".into(), outcome: Outcome::Skipped { reason: "the signed claim origins and outcomes could not be reproduced from this local session; model or producer claims cannot establish policy authority".into() } });
    }
    let mut body = thruline_attest::session_v2::presentation(&report, session, policy);
    body["evidence_trust"] = serde_json::json!(if origins_match { "local" } else { "untrusted" });
    body["integrity"] = serde_json::to_value(&report)?;
    body["signed_record"] = serde_json::to_value(session)?;
    let conclusion = body["conclusion"]
        .as_str()
        .context("missing computed conclusion")?
        .to_string();
    let title = body["cause_title"]
        .as_str()
        .context("missing cause title")?;
    let counts = &body["verdict_summary"];
    let summary = format!("{title}\n\n{} supported · {} unsupported · {} unverifiable · {} total claims.\nEvidence trust: {}. A signature does not establish independent custody.\nInspect the signed scope and incomplete integrity checks before taking action.",
        counts["supported"], counts["unsupported"], counts["unverifiable"], counts["total"], body["evidence_trust"].as_str().unwrap_or("untrusted"));
    let format = if args.json {
        "json"
    } else {
        args.format.as_str()
    };
    match format {
        "json" => crate::json_out::print(&crate::json_out::VERIFY_SESSION_RECEIPT, body)?,
        "text" | "md" => println!("{summary}"),
        "check-run" => {
            let head = args
                .head_sha
                .clone()
                .or_else(|| std::env::var("GITHUB_SHA").ok())
                .context("check-run rendering needs an explicit --head-sha or GITHUB_SHA")?;
            println!(
                "{}",
                serde_json::json!({"name":thruline_attest::report::CHECK_NAME,"head_sha":head,"status":"completed",
                "conclusion":conclusion,"output":{"title":title,"summary":summary}})
            );
        }
        "badge" => {
            let label = body["badge_label"]
                .as_str()
                .context("missing badge label")?;
            let colour = if conclusion == "success" {
                "#238636"
            } else if conclusion == "neutral" {
                "#8a6d3b"
            } else {
                "#b54735"
            };
            println!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"240\" height=\"24\" role=\"img\" aria-label=\"ThruLine: {label}\"><rect width=\"240\" height=\"24\" rx=\"4\" fill=\"{colour}\"/><text x=\"120\" y=\"16\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"12\" fill=\"white\">ThruLine: {label}</text></svg>");
        }
        _ => anyhow::bail!("unsupported v0.2 session rendering"),
    }
    Ok(ExitCode::from(match conclusion.as_str() {
        "success" => 0,
        "failure" => 1,
        _ => 2,
    }))
}

/// The `--json` surface: **the conclusion first, the distribution beside it.**
///
/// # Ruling point 6, and the shape it is written against
///
/// > *"`verify --json` carries a top-level verdict summary, so that no consumer
/// > can infer 'passed' from 'no check failed.'"*
///
/// This surface has been undone twice already by exactly that inference:
/// `all(c.outcome != "fail")` read a partially-verified receipt as verified,
/// and a tri-state string named `verified` read `"partial"` as truthy. Both
/// times the fix was to **state the answer instead of leaving it to be
/// derived**, and both times one axis was still missing: the receipt's own
/// claim verdicts. A receipt can pass every integrity check and carry a claim
/// the record does not support.
///
/// So the object names its axes:
///
/// - `conclusion` — **the answer**, and the only field a gate may read. It is
///   what the check run posts, combined downward from all three axes.
/// - `verified` — a real boolean, true only when `conclusion == "success"`.
/// - `status` — `conclusion`'s other spelling, whose three words do not read as
///   one another.
/// - `exit` — **the process exit code, and the integrity axis only.** Ruling
///   point 1: `verify` stays integrity-only and non-blocking, so an
///   `unsupported` claim does not move it. A gate that reads `$?` instead of
///   `conclusion` is reading the wrong thing, and the `integrity` block beside
///   it says so in words.
/// - `integrity` / `claims` / `policy` — one block per axis, so a reader can
///   see *which* one produced the conclusion rather than guessing.
/// - `checks` — unchanged.
fn verify_json(
    report: &thruline_attest::verify::Report,
    statement: Option<&Statement>,
    policy: Option<thruline_attest::report::PolicyConclusion>,
) -> serde_json::Value {
    use thruline_attest::report::{conclusion_of, Conclusion, PolicyConclusion, VerdictSummary};
    use thruline_attest::verify::Verified;
    let integrity = report.verified();
    let claims = VerdictSummary::of(statement);
    let overall = conclusion_of(report, statement, policy);
    let spell = |v: Verified| match v {
        Verified::Yes => "verified",
        Verified::Partial => "partial",
        Verified::No => "failed",
    };
    let status = match overall {
        Conclusion::Success => "verified",
        Conclusion::Neutral => "partial",
        Conclusion::Failure => "failed",
    };
    serde_json::json!({
        "conclusion": overall.as_str(),
        "verified": overall == Conclusion::Success,
        "status": status,
        "exit": integrity.exit_code(),
        "integrity": {
            "status": spell(integrity),
            "exit": integrity.exit_code(),
            "note": "the axis `verify` itself decides, and the axis `exit` reports. \
                     `unsupported` claims never move it (ruling 1) — gate on \
                     `conclusion`, never on `exit`.",
        },
        "claims": claims,
        "policy": match policy {
            None => serde_json::json!({
                "evaluated": false,
                "conclusion": serde_json::Value::Null,
                "note": "no policy was evaluated, so no `failure` can come from one",
            }),
            Some(p) => serde_json::json!({
                "evaluated": true,
                "conclusion": match p {
                    PolicyConclusion::Success => "success",
                    PolicyConclusion::Neutral => "neutral",
                    PolicyConclusion::Failure => "failure",
                },
            }),
        },
        "checks": serde_json::to_value(report).ok().and_then(|v| v.get("checks").cloned()),
    })
}

/// What this machine can say, gathered without a network.
///
/// `--offline` is the only mode: nothing here opens a socket, so the flag is
/// accepted and asserted rather than branched on. A Rekor lookup would be the
/// first thing that could change that, and it does not exist.
/// Is this verifying key inside the repository being gated? (F12, ruling 8.)
///
/// **Inside the working tree, not merely tracked by git.** Ruling 8 says "a
/// signing key committed in the gated repository", and in a CI checkout those
/// are the same set — an untracked file is not there at all. On a developer's
/// machine they differ, and the wider reading is taken deliberately: it can
/// only produce more `neutral`s, never more `success`es, which is the standing
/// rule for a decision taken without the operator present. A key that is about
/// to be committed is not safer than one that already is.
///
/// Both paths are canonicalised first, so `../` and a symlinked checkout cannot
/// walk a key out of the repository it is in. If either path cannot be
/// canonicalised — the key was deleted between loading and here, say — the
/// answer is `None`: this function reports what it can see, and the signature
/// check above has already run on a key that did load.
fn key_path_inside_repo(repo: &Path, key: &Path) -> Option<String> {
    let repo = repo.canonicalize().ok()?;
    let key = key.canonicalize().ok()?;
    let rel = key.strip_prefix(&repo).ok()?;
    Some(rel.display().to_string())
}

/// §8.1's `degraded_causes`, assembled from every producer that has one.
///
/// A session can be degraded for its own reason **and** snapshotted under a
/// content transform at the same time, and a reader handed one of the two takes
/// the wrong next step — so both are listed rather than the first winning.
///
/// **The last arm is the one that matters.** A session the ledger marks
/// degraded with no recorded reason would otherwise produce
/// `capture_degraded: true` with an empty cause list, which is the exact defect
/// this field exists to close, reintroduced through an edge case. It says so
/// instead, and the schema's "degraded implies a cause" rule stays true for
/// every receipt this build can emit rather than for most of them.
fn degraded_causes(
    view: &thruline_verify::binding::SessionView,
    content_filters: Option<&str>,
) -> Vec<String> {
    let mut causes: Vec<String> = view.degraded_reason.clone().into_iter().collect();
    if let Some(d) = content_filters {
        causes.push(format!("content_filters: {d}"));
    }
    if causes.is_empty() && view.capture_degraded {
        causes.push(
            "unstated: this session is recorded as degraded and the ledger holds no reason              for it"
                .to_string(),
        );
    }
    causes
}

fn local_evidence(
    repo: &Path,
    envelope: &Envelope,
    exec_db: Option<&Path>,
    head_sha: Option<&str>,
    receipt_path: &Path,
) -> LocalEvidence {
    // Is there an object store at all? This is what separates "no repository"
    // (skip) from "repository that does not have this object" (fail).
    let have_objects = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // **Is this a shallow clone?** `actions/checkout` defaults to
    // `fetch-depth: 1`, and in that state `cat-file --batch-check` truthfully
    // answers `missing` for objects the clone simply does not have. Without
    // this flag the Action would report a red check run on entirely honest
    // receipts, by default configuration.
    let shallow = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["rev-parse", "--is-shallow-repository"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false);
    // The policy in force HERE, resolved by the same function the producer
    // used, so the two cannot look in different places or hash different
    // things. `None` only when this machine cannot resolve one at all — an
    // unreadable or unparseable file — which is a `Skipped`, never a pass.
    let policy_hash = thruline_policy::digest_for_repo(repo).ok();
    let mut local = LocalEvidence {
        have_objects,
        shallow,
        policy_hash,
        ..LocalEvidence::default()
    };

    // Ask the object store about the hashes the receipt NAMES, rather than
    // walking history hoping to meet them.
    //
    // The previous version listed 500 commits and resolved 200 trees with one
    // process each: 1,907 ms, and the 200 bound already excluded 53.2% of this
    // repository's own history — so a legitimate tree a few hundred commits
    // back was reported absent. It also missed anything unreachable from HEAD.
    // One `cat-file --batch-check` answers exactly the question, unbounded, in
    // about 10 ms.
    if local.have_objects {
        let mut wanted: Vec<(String, bool)> = Vec::new();
        if let Ok(bytes) = envelope.payload_bytes() {
            if let Ok(decoded) = thruline_attest::session_v2::decode(&bytes) {
                let statement = decoded.statement;
                for subject in &statement.subject {
                    if let Some(t) = subject.digest.get("gitTree") {
                        wanted.push((t.clone(), true));
                    }
                    if let Some(c) = subject.digest.get("gitCommit") {
                        wanted.push((c.clone(), false));
                    }
                }
            }
        }
        // The working tree's own hash too, so a receipt about the state right
        // now resolves even before it is committed and reachable.
        if let Ok(snap) = thruline_core::spec::snapshot::snapshot_tree(repo) {
            // And the transforms in scope while it was computed. Without this,
            // the tree hash below "matches" bytes the verifier never saw.
            local.content_filters = snap
                .content_filters
                .degrades_capture()
                .then(|| snap.content_filters.describe());
            // **Is the tree here, right now, the tree the receipt is about?**
            // When it is, binding is settled without reference to any commit:
            // the code a reviewer is looking at IS the code the checks ran on.
            // The ordinary case — a receipt about work in progress, checked in
            // the same tree — is about a tree that is no commit at all.
            local.subject_is_working_tree = wanted
                .iter()
                .any(|(hash, is_tree)| *is_tree && *hash == snap.tree_hash);
            local.tree_hashes.insert(snap.tree_hash);
            if let Some(c) = snap.head_commit {
                local.commits.insert(c);
            }
        }
        for (hash, is_tree) in wanted {
            if let Some(kind) = object_kind(repo, &hash) {
                match (kind.as_str(), is_tree) {
                    ("tree", true) => {
                        local.tree_hashes.insert(hash);
                    }
                    ("commit", false) => {
                        local.commits.insert(hash);
                    }
                    // A hash that exists but is the wrong KIND is not the object
                    // the receipt names; leaving it out makes the check fail,
                    // which is correct.
                    _ => {}
                }
            }
        }
    }

    // **The head under review**, which nothing here used to name at all.
    //
    // `--head-sha`, else `$GITHUB_SHA`, else this checkout's own `HEAD`. The
    // local fallback matters: without it every ordinary `verify-receipt` would
    // report "no head could be resolved" and never reach `verified`, and the
    // ordinary case — a receipt issued and checked in the same clean tree — is
    // one where the answer is plainly yes.
    if local.have_objects {
        local.head_commit = head_sha
            .map(str::to_string)
            .or_else(|| std::env::var("GITHUB_SHA").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| Some("HEAD".to_string()))
            .and_then(|rev| {
                git_line(
                    repo,
                    &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
                )
            });

        let statement: Option<Statement> = envelope.payload_bytes().ok().and_then(|b| {
            thruline_attest::session_v2::decode(&b)
                .ok()
                .map(|decoded| decoded.statement)
        });
        let subject = statement.as_ref().and_then(|s| s.subject.first());
        let receipt_commit = subject.and_then(|s| s.digest.get("gitCommit"));
        let subject_tree = subject.and_then(|s| s.digest.get("gitTree"));

        if let (Some(commit), Some(head)) = (receipt_commit, &local.head_commit) {
            // `--is-ancestor` answers with its exit status: 0 yes, 1 no,
            // anything else (a missing object in a shallow clone, most often)
            // is **not an answer** and must not be read as one.
            local.receipt_is_ancestor_of_head = std::process::Command::new("git")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .current_dir(repo)
                .args(["merge-base", "--is-ancestor", commit, head])
                .output()
                .ok()
                .and_then(|o| match o.status.code() {
                    Some(0) => Some(true),
                    Some(1) => Some(false),
                    _ => None,
                });
        }
        if let (Some(tree), Some(head)) = (subject_tree, &local.head_commit) {
            local.head_tree_differs_at = std::process::Command::new("git")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .current_dir(repo)
                .args([
                    "diff",
                    "--name-only",
                    "--no-renames",
                    tree,
                    &format!("{head}^{{tree}}"),
                ])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .map(str::to_string)
                        .filter(|l| !l.is_empty())
                        .collect()
                });
        }
        // The receipt's own path, so the documented workflow — commit the
        // receipt, open the pull request — is not accused of changing the code
        // its checks ran on. Exactly this one path, nothing beside it.
        local.receipt_path_in_repo =
            git_line(repo, &["rev-parse", "--show-toplevel"]).and_then(|root| {
                let root = std::fs::canonicalize(root).ok()?;
                let full = std::fs::canonicalize(receipt_path).ok()?;
                let rel = full.strip_prefix(&root).ok()?;
                Some(rel.to_string_lossy().replace('\\', "/"))
            });
    }

    let db = exec_db
        .map(Path::to_path_buf)
        .or_else(|| Ledger::default_path().ok());
    if let Some(db) = db.filter(|p| p.is_file()) {
        if let Ok(ledger) = Ledger::open_read_only(&db) {
            local.ledger_present = true;
            if let Ok(bytes) = envelope.payload_bytes() {
                if let Ok(decoded) = thruline_attest::session_v2::decode(&bytes) {
                    let statement = decoded.statement;
                    // Recomputed from the rows, never read from the cache.
                    if let Ok(head) = ledger.verify_session(&statement.predicate.session.id) {
                        local.recomputed_chain = Some((head.head_hash, head.event_count));
                    }
                    // **The ledger's own check rows.** The receipt carries a
                    // COPY of them, and nothing bound the copy to the original:
                    // `chain_head` recomputes the event chain and the check
                    // rows sit beside it. A re-signer who invented a coherent
                    // passing check verified clean, measured.
                    if let Ok(rows) = ledger.checks_for_session(&statement.predicate.session.id) {
                        local.ledger_checks = Some(
                            rows.into_iter()
                                .map(|c| (c.id, (c.exit_code, c.input_tree_hash)))
                                .collect(),
                        );
                    }
                }
            }
        }
    }
    local
}

/// One line of `git` output, when the command succeeded and said something.
fn git_line(repo: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!line.is_empty()).then_some(line)
}

/// What kind of object the local store has under `hash`, if any.
///
/// `cat-file --batch-check` answers "is this object mine" exactly, for any
/// object, with no bound — including one on an unreferenced branch, which the
/// old reachability walk reported as absent.
fn object_kind(repo: &Path, hash: &str) -> Option<String> {
    use std::io::Write;
    let mut child = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["cat-file", "--batch-check"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()?
        .write_all(format!("{hash}\n").as_bytes())
        .ok()?;
    let out = child.wait_with_output().ok()?;
    let line = String::from_utf8_lossy(&out.stdout);
    let mut fields = line.split_whitespace();
    let (_oid, kind) = (fields.next()?, fields.next()?);
    // `<hash> missing` for anything the store does not have.
    (kind != "missing").then(|| kind.to_string())
}
