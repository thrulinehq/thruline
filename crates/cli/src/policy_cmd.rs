//! `thruline policy [--init]` — BUILD-SPEC §9's gate, run locally.
//!
//! # Why this exists before the hosted App
//!
//! §9's Action "ships first because it needs no hosted service", and the Action
//! is this command in a container. A developer who can run the gate on their
//! own machine and get the same answer CI will give has a gate they can trust;
//! one who can only find out by pushing has a lottery. So the decision is
//! computed here, by the same evaluator, from the same claims `thruline claims`
//! prints.
//!
//! # Exit codes are the interface
//!
//! `0` when the check passes or is neutral, `3` when policy fails it, `1` when
//! this command could not decide — a broken policy file, no store, a base ref
//! that does not resolve. **Neutral is not a failure**: §9 says `unverifiable`
//! produces a neutral conclusion with instructions, which is §6's third state
//! surviving all the way to the merge button instead of being rounded to one of
//! the other two.
//!
//! # Where the subject comes from, and why every gap in it is `None`
//!
//! Three of Track R's findings were the same defect wearing different clothes:
//! this file built [`Subject`] out of things that looked like measurements and
//! were not. `receipt_present` asked whether a *directory* existed, so
//! `mkdir -p .thruline/receipts` satisfied `require_receipt` (F10).
//! `stop_blocks` was the literal `0`, so `max_stop_blocks` could not fire for
//! any input and read as coverage it never gave (F11). And the policy itself
//! was read from the head checkout, so a pull request could disarm the gate
//! reviewing it (F8). Every one of them returned **success** on a run that
//! should have been refused.
//!
//! The rule that replaces them: a field of the subject is either **measured
//! here** or **`None`**, and `None` is a neutral conclusion the reader can see.
//! Nothing in this file may synthesise a value that means "satisfied".

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_policy::{Claim, Conclusion, Policy, Subject, Verdict};
use thruline_verify::verdict::Verdict as ClaimVerdict;

pub struct Args {
    pub init: bool,
    pub session: Option<String>,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub policy: Option<PathBuf>,
    /// The branch this change would merge into. **When it is given, the policy
    /// is read from there and never from the working tree.**
    ///
    /// Track R's F8: `.thruline/policy.yml` was read from the head checkout
    /// only, so a pull request that rewrote the policy was judged by the policy
    /// it had just written — measured, `required_when: ["**"]` narrowed to
    /// `["vendor/**"]` and `require_receipt` turned off took the same session
    /// from `failure` (exit 3) to `success` (exit 0). The operator's ruling:
    /// policy for a pull request is read from the base branch, never the head.
    pub base: Option<String>,
    pub json: bool,
    /// Render §9's check-run markdown table.
    ///
    /// **In Rust rather than in the Action's shell**: the table is what a
    /// reviewer reads instead of the receipt, so it is worth testing, and a
    /// heredoc in a composite action is not.
    pub md: bool,
    /// Print exactly one of `success`, `failure`, `neutral` and nothing else.
    ///
    /// So the Action does not parse JSON in shell. A `sed` over a JSON document
    /// is a parser nobody tested, and the one place it would go wrong is a
    /// conclusion misread as the wrong one.
    pub conclusion: bool,
}

/// §9's documented example, written verbatim by `policy --init`.
///
/// The file a user starts from is the file the spec shows, so what they read in
/// the docs and what is in their repository are the same text.
const TEMPLATE: &str = r#"# ThruLine policy. See docs: policy reference.
#
# The schema is CLOSED: an unrecognised key is an error, not a shrug. A policy
# written for a newer ThruLine fails loudly on an older one rather than quietly
# enforcing less than you wrote.
version: 1
require_receipt: true
claims:
  tests_pass:
    required_when: { paths: ["src/**", "lib/**"] }
    require_reexecuted: true
    min_changed_line_coverage: 70
    allow_skips: false
  behavior_unchanged:
    require_evidence: true
  test_file_changes:
    require_human_ack: true
promises:
  block_on_open: false
  comment_open: true
stop_gate:
  enabled: true
  max_stop_blocks: 2
reexec:
  mode: claimed
  reruns: 2
  image: auto
"#;

pub fn run(args: Args) -> Result<ExitCode> {
    let repo = args
        .repo
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    // **Resolved against `--repo`, not against the process's cwd.** The shipped
    // Action passes `--policy "$TL_POLICY"` and the value a user writes there is
    // repository-relative; with `--base` now adopted from `GITHUB_BASE_REF`
    // without a flag, that is the path every pull-request run takes.
    let path = args
        .policy
        .clone()
        .map(|p| if p.is_absolute() { p } else { repo.join(p) })
        .unwrap_or_else(|| repo.join(".thruline").join("policy.yml"));

    if args.init {
        return init(&path, args.json);
    }
    check(&path, &repo, args)
}

fn init(path: &Path, json: bool) -> Result<ExitCode> {
    if path.exists() {
        // Never overwrite: a policy file is the thing standing between a merge
        // and an unsupported claim, and clobbering one silently is the worst
        // outcome this command could have.
        if json {
            crate::json_out::print_compact(
                &crate::json_out::POLICY_INIT,
                serde_json::json!({ "ok": false, "reason": "policy already exists", "path": path }),
            )?;
        } else {
            println!("{} already exists; nothing written.", path.display());
        }
        return Ok(ExitCode::from(1));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(path, TEMPLATE).with_context(|| format!("writing {}", path.display()))?;
    // Parse what was just written. A template that does not parse would be a
    // command that hands the user a broken file and reports success.
    Policy::parse(TEMPLATE).context("the shipped policy template does not parse")?;
    if json {
        crate::json_out::print_compact(
            &crate::json_out::POLICY_INIT,
            serde_json::json!({ "ok": true, "path": path, "version": 1 }),
        )?;
    } else {
        println!("Wrote {}.", path.display());
        println!("Run `thruline policy` to evaluate it against the last session.");
    }
    Ok(ExitCode::SUCCESS)
}

fn check(path: &Path, repo: &Path, args: Args) -> Result<ExitCode> {
    let base = base_ref(&args)?;
    let policy = match policy_text(path, repo, base.as_deref())? {
        Some(text) => Policy::parse(&text)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
            .context("the policy file was not understood")?,
        None => {
            // **No policy is not a failing policy.** A repository that never
            // opted in must not be gated by a default nobody wrote.
            if args.json {
                crate::json_out::print_compact(
                    &crate::json_out::POLICY_CHECK,
                    serde_json::json!({
                        "ok": true,
                        "conclusion": "success",
                        "reason": "no policy file; nothing is gated",
                        "path": path,
                    }),
                )?;
            } else {
                match &base {
                    Some(r) => println!(
                        "No policy at {} on the base branch `{r}`; nothing is gated.",
                        path.display()
                    ),
                    None => println!("No policy at {}; nothing is gated.", path.display()),
                }
                println!("Run `thruline policy --init` to start from the documented example.");
            }
            return Ok(ExitCode::SUCCESS);
        }
    };

    let sc = crate::claims_cmd::session_claims(
        args.session.clone(),
        args.exec_db.clone(),
        Some(repo.to_path_buf()),
    )?;
    let subject = subject_of(&sc, repo);
    let decision = policy.evaluate(&subject);

    let word = match decision.conclusion {
        Conclusion::Success => "success",
        Conclusion::Failure => "failure",
        Conclusion::Neutral => "neutral",
    };
    if args.conclusion {
        println!("{word}");
    } else if args.md {
        print!("{}", markdown(&sc.session, path, &decision));
    } else if args.json {
        crate::json_out::print(
            &crate::json_out::POLICY_CHECK,
            serde_json::json!({
                "ok": decision.conclusion != Conclusion::Failure,
                "session": sc.session,
                "policy": path,
                "conclusion": word,
                "reasons": decision.reasons.iter().map(|r| serde_json::json!({
                    "claim": r.claim,
                    "rule": r.rule,
                    "detail": r.detail,
                    "conclusion": match r.conclusion {
                        Conclusion::Success => "success",
                        Conclusion::Failure => "failure",
                        Conclusion::Neutral => "neutral",
                    },
                })).collect::<Vec<_>>(),
                "advisory": decision.advisory,
            }),
        )?;
    } else {
        println!("session {}", sc.session);
        if let Some(r) = &base {
            println!("base    `{r}` — the policy that judges this change is read from there");
        }
        println!(
            "policy  {} — {}",
            path.display(),
            match decision.conclusion {
                Conclusion::Success => "success",
                Conclusion::Failure => "failure",
                Conclusion::Neutral => "neutral (nothing blocked; something could not be decided)",
            }
        );
        for r in &decision.reasons {
            let who = r.claim.clone().unwrap_or_else(|| "-".to_string());
            println!("  [{}] {who}: {}", r.rule, r.detail);
        }
        for (extractor, n) in &decision.advisory {
            println!(
                "  {n} claim(s) from the `{extractor}` extractor are advisory and cannot \
                 fail this gate (BUILD-SPEC §0.1)."
            );
        }
    }
    Ok(match decision.conclusion {
        Conclusion::Failure => ExitCode::from(3),
        _ => ExitCode::SUCCESS,
    })
}

/// The base ref whose policy judges this change, or `None` for the local
/// working tree.
///
/// `--base` wins; otherwise `GITHUB_BASE_REF`, which GitHub sets on every
/// `pull_request` event, so the Action does not have to remember a flag. And a
/// pull-request run with **no** base ref available is a refusal, not a fall
/// back to the head checkout: falling back restores F8 exactly, and does it on
/// the one run where something has already gone wrong.
fn base_ref(args: &Args) -> Result<Option<String>> {
    if let Some(b) = &args.base {
        let b = b.trim();
        anyhow::ensure!(!b.is_empty(), "--base was given an empty ref");
        return Ok(Some(b.to_string()));
    }
    let from_env = std::env::var("GITHUB_BASE_REF")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if from_env.is_none() {
        let event = std::env::var("GITHUB_EVENT_NAME").unwrap_or_default();
        if event == "pull_request" || event == "pull_request_target" {
            anyhow::bail!(
                "this is a `{event}` run and no base ref is available (neither `--base` nor \
                 `GITHUB_BASE_REF`). Policy for a pull request is read from the base branch; \
                 refusing rather than judging this change by the policy it may have just written."
            );
        }
    }
    Ok(from_env)
}

/// The policy text to evaluate. `Ok(None)` means *no policy here*, which gates
/// nothing.
///
/// With a base ref this reads the blob out of git rather than off disk, so the
/// bytes cannot have been written by the change under review.
fn policy_text(path: &Path, repo: &Path, base: Option<&str>) -> Result<Option<String>> {
    let Some(base) = base else {
        return match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        };
    };
    let rel = path.strip_prefix(repo).map_err(|_| {
        anyhow::anyhow!(
            "--base reads the policy out of git, so {} must be inside {}",
            path.display(),
            repo.display()
        )
    })?;
    // Where `repo` sits inside the work tree, so the path handed to `git show`
    // is relative to the top level rather than to wherever this was invoked.
    let prefix = git(repo, &["rev-parse", "--show-prefix"])
        .context("--base needs a git repository and this is not one")?;
    let in_tree = format!("{}{}", prefix.trim(), rel.display());
    let rev = resolve_base(repo, base)?;
    read_blob(repo, &rev, &in_tree)
}

/// Resolve a base ref the way CI has it: the branch name, or the remote-tracking
/// ref a shallow `actions/checkout` actually leaves behind.
///
/// **A ref that resolves nowhere is an error.** It must never degrade into
/// reading the head checkout.
fn resolve_base(repo: &Path, base: &str) -> Result<String> {
    let candidates = [
        base.to_string(),
        format!("origin/{base}"),
        format!("refs/remotes/origin/{base}"),
    ];
    for c in &candidates {
        if let Ok(sha) = git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{c}^{{commit}}"),
            ],
        ) {
            let sha = sha.trim().to_string();
            if !sha.is_empty() {
                return Ok(sha);
            }
        }
    }
    anyhow::bail!(
        "the base ref `{base}` does not resolve in {} (tried {}). Fetch the base branch \
         (`git fetch origin {base}`) — this refuses rather than falling back to the policy \
         in the checkout under review.",
        repo.display(),
        candidates.join(", ")
    )
}

/// One blob at one revision. `Ok(None)` when the revision has no such path.
fn read_blob(repo: &Path, rev: &str, in_tree: &str) -> Result<Option<String>> {
    let out = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(["show", &format!("{rev}:{in_tree}")])
        .output()
        .with_context(|| format!("running git show {rev}:{in_tree}"))?;
    if out.status.success() {
        return Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()));
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("does not exist") || err.contains("exists on disk, but not in") {
        return Ok(None);
    }
    anyhow::bail!("git show {rev}:{in_tree} failed: {}", err.trim())
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    anyhow::ensure!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Is there a receipt **for this session** in the repository?
///
/// Track R's F10: this used to be `.thruline/receipts/.is_dir()`, so
/// `mkdir -p .thruline/receipts` — an empty directory, created by anything —
/// satisfied `require_receipt`. A directory is not a receipt.
///
/// What counts here is **structural**: a DSSE envelope this binary's own
/// producer would recognise, carrying an execution-check statement, whose
/// session is the session under policy. Deliberately *not* a signature check —
/// key trust is `thruline verify-receipt`'s job and duplicating a weaker copy of
/// it here would be the more dangerous kind of half-verification. This answers
/// only "did a receipt for this run get issued into the repository", which is
/// exactly what `require_receipt` asks.
fn receipt_for_session(repo: &Path, session: &str) -> bool {
    let dir = repo.join(".thruline").join("receipts");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        // A receipt is small. A multi-gigabyte file in this directory must not
        // become a way to make the gate spend the runner's memory.
        if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > 8 << 20 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(envelope) = serde_json::from_str::<thruline_attest::Envelope>(&text) else {
            continue;
        };
        if envelope.payload_type != thruline_attest::PAYLOAD_TYPE || envelope.signatures.is_empty()
        {
            continue;
        }
        let Ok(payload) = envelope.payload_bytes() else {
            continue;
        };
        let Ok(statement) = serde_json::from_slice::<thruline_attest::Statement>(&payload) else {
            continue;
        };
        if statement.predicate_type == thruline_attest::PREDICATE_TYPE
            && statement.predicate.session.id == session
        {
            return true;
        }
    }
    false
}

/// How many times the §4.1 Stop gate blocked this session, or `None` when that
/// cannot be read here.
///
/// Track R's F11: this was the literal `0`, so `max_stop_blocks` could not fire
/// for any input — *"the rule is dead on arrival"*. It reads the gate's own
/// block ledger now, and **an absent or unreadable ledger is `None`, not zero**.
/// The two are genuinely different: a session with the gate installed and no
/// blocks writes no file, and neither does a machine where the gate was never
/// installed at all. Only one of those has satisfied the limit, and nothing here
/// can tell them apart — so the rule reports that it could not be evaluated
/// instead of reporting a limit it never checked as met.
///
/// Since G2 it reads the `gate_blocks` table rather than a loose JSON file, so
/// a **chain that does not verify** is `None` too — the gate's own rule, kept
/// here rather than restated: a store that cannot be trusted to say is not a
/// store that said zero.
fn stop_blocks_recorded(session: &str) -> Option<u32> {
    let db = thruline_ledger::Ledger::default_path().ok()?;
    if !db.is_file() {
        return None;
    }
    let ledger = thruline_ledger::Ledger::open(&db).ok()?;
    let blocks = ledger.gate_blocks(session).ok()?;
    // **Zero rows is `None`, not `Some(0)`** — F11's distinction, kept across
    // the move into the table. It survived before because a gate that blocked
    // nothing wrote no file; with a table, "the gate ran and blocked nothing"
    // and "the gate was never installed" both read as an empty result set, and
    // reporting `Some(0)` would mark a limit satisfied that was never checked.
    // The cost is that a genuinely block-free session reports "could not be
    // evaluated"; that is neutral, which is the direction that cannot pass
    // something it did not verify.
    if blocks.is_empty() {
        return None;
    }
    u32::try_from(blocks.len()).ok()
}

/// §9's check-run body: the conclusion, then one row per finding.
///
/// Written for someone who has never installed ThruLine — §15 calls the PR
/// comment "the viral surface" — so every row says which rule, about which
/// claim, and what to do, and the neutral case says plainly that nothing was
/// blocked.
fn markdown(session: &str, path: &Path, decision: &thruline_policy::Decision) -> String {
    let mut out = String::new();
    let (icon, headline) = match decision.conclusion {
        Conclusion::Success => (
            "✅",
            "Every claim policy requires is supported by the record.",
        ),
        Conclusion::Failure => (
            "❌",
            "A claim policy requires is not supported by the record.",
        ),
        Conclusion::Neutral => (
            "◽",
            "Nothing is blocked. Something policy asks about could not be decided \
             from the record.",
        ),
    };
    out.push_str(&format!("### {icon} thruline/receipt\n\n{headline}\n\n"));
    if decision.reasons.is_empty() {
        out.push_str("No policy rule had anything to report.\n\n");
    } else {
        out.push_str("| | Claim | Rule | What the record shows |\n");
        out.push_str("|---|---|---|---|\n");
        for r in &decision.reasons {
            let mark = match r.conclusion {
                Conclusion::Failure => "❌",
                Conclusion::Neutral => "◽",
                Conclusion::Success => "✅",
            };
            let claim = r.claim.clone().unwrap_or_else(|| "—".to_string());
            // A detail containing a pipe would break the table; escape rather
            // than trusting that none ever will.
            let detail = r.detail.replace('|', "\\|");
            out.push_str(&format!(
                "| {mark} | `{claim}` | `{}` | {detail} |\n",
                r.rule
            ));
        }
        out.push('\n');
    }
    for (extractor, n) in &decision.advisory {
        out.push_str(&format!(
            "> {n} claim(s) from the `{extractor}` extractor are **advisory**: they \
             appear in `thruline explain` and cannot fail this check. A model's \
             output does not gate a merge (BUILD-SPEC §0.1).\n\n"
        ));
    }
    out.push_str(&format!(
        "<sub>session `{session}` · policy `{}` · every row above traces to an \
         event in the execution record; run `thruline explain --last` to see \
         it.</sub>\n",
        path.display()
    ));
    out
}

/// Builds §9's subject from what §5 extracted and §6 decided.
///
/// **Every input is either measured or explicitly `None`.** The tempting
/// version of this function fills the gaps with `Vec::new()` and `0`, which
/// reads as "no changed paths, no skipped tests" — a silent pass on exactly the
/// runs where something went wrong. Where a value cannot be determined here it
/// is `None`, and the policy engine turns that into a neutral conclusion.
fn subject_of(sc: &crate::claims_cmd::SessionClaims, repo: &Path) -> Subject {
    let by_index: std::collections::BTreeMap<usize, &thruline_verify::verdict::VerdictRecord> =
        sc.verdicts.iter().map(|(i, v)| (*i, v)).collect();
    // The same diff §6.3 and §6.8 read. `api_diff_available` is what
    // distinguishes "the change touched nothing" from "the diff could not be
    // computed", and only the first of those is an empty path list.
    let diff = crate::claims_cmd::diff_evidence_inputs(repo, &sc.view);
    let changed_paths = diff.api_diff_available.then(|| diff.changed_paths.clone());
    let coverage_pct = match &diff.coverage {
        Some(thruline_verify::coverage::Coverage::Measured { pct_bp, .. }) => Some(*pct_bp / 100),
        _ => None,
    };
    // §6's parsed run summary is where a skip count would come from. It is not
    // carried on the verdict record, so it is `None` here rather than zero, and
    // a policy forbidding skips gets a neutral instead of a pass it did not
    // earn. Named in REPORT.md as an open gap rather than papered over.
    let skipped_tests: Option<u32> = None;
    let claims = sc
        .extraction
        .claims
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let v = by_index.get(&i)?;
            Some(Claim {
                kind: c.kind.as_str().to_string(),
                // §0.1's boundary, carried as data rather than assumed.
                extractor: c.extractor.as_str().to_string(),
                verdict: match v.verdict {
                    ClaimVerdict::Supported => Verdict::Supported,
                    ClaimVerdict::Unsupported => Verdict::Unsupported,
                    ClaimVerdict::Unverifiable => Verdict::Unverifiable,
                },
                reexecuted: v.binding.is_some(),
                evidence_present: !v.evidence.is_empty(),
                // A human acknowledgement is a PR label or an approving review;
                // neither exists locally, so this is `false` here and the
                // Action supplies it. **Not defaulted to true**: a gate that
                // assumes the acknowledgement it cannot see is not a gate.
                human_acked: None,
                changed_line_coverage: coverage_pct,
                skipped_tests,
            })
        })
        .collect();
    Subject {
        receipt_present: receipt_for_session(repo, &sc.session),
        changed_paths,
        open_promises: u32::try_from(sc.extraction.promises.len()).unwrap_or(u32::MAX),
        stop_blocks: stop_blocks_recorded(&sc.session),
        claims,
    }
}
