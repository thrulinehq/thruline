//! BUILD-SPEC §7 — re-execution in a clean sandbox.
//!
//! > *Re-execution beats trust. Any claimed check can be re-run in a clean
//! > environment at the final tree hash, and the receipt records whether it
//! > was.* (§0 principle 4)
//!
//! # Three rulings this crate exists to obey
//!
//! **1. Materialisation never uses `git checkout`** (C.7/4). A checkout applies
//! smudge filters, so its content is not a function of the tree hash at all.
//! [`materialize`] writes blobs through `git cat-file`, and a source-walking
//! test enforces it.
//!
//! **2. Re-execution is *gated*, not annotated** (C.7/3). If the tree-fidelity
//! signature tripped, [`Gate::check`] **refuses** and `reexecuted: true` is
//! unreachable. Annotating instead would leave the laundering in place with a
//! footnote: re-running honest code to confirm a claim about cheating code *is*
//! the laundering.
//!
//! **3. `reexecuted: true` is derivable, never asserted.** Nothing in this
//! crate sets a boolean. A re-execution produces an `exec` event carrying the
//! container image digest, appended to the session's hash chain; the receipt
//! builder *derives* `reexecuted` from that event, and `thruline verify`
//! refuses a receipt that claims re-execution without a digest behind it.
//!
//! # The TTY trap, pinned before any digest is compared
//!
//! The M0.8 recorder gives its children a pipe, not a TTY, and records
//! `stdio: "pipe"` on every `exec` event for exactly this reason. Output bytes
//! — and sometimes behaviour: pytest's terminal width, progress rendering,
//! colour — differ between the two. A comparison that ignored this would report
//! a **digest mismatch that is not one**, which in §7's vocabulary reads as
//! evidence of tampering. [`Comparison::of`] refuses to compare output digests
//! across a stdio mismatch and says so, rather than producing a finding it
//! cannot stand behind.

#[cfg(feature = "reexec-cloud")]
pub mod cloud;
pub mod docker;
pub mod image;
pub mod materialize;
pub mod tree_read;

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

pub use image::{DependencyLayer, Ecosystem, ImageSource, ImageSpec, ReexecConfig};
pub use materialize::{materialize, Materialized};

/// The BUILD-SPEC section that governs this crate.
pub const SPEC_SECTION: &str = "§7";

/// What a sandbox needs in order to build a clean checkout.
#[derive(Debug, Clone)]
pub struct PrepareRequest {
    /// The repository whose object store holds the tree.
    pub repo: PathBuf,
    /// §6.1's `final_tree_hash`: the tree the checks must run against.
    pub tree_hash: String,
    pub image: ImageSpec,
    /// §7: *"Network off by default"*. `true` only when the repository asked
    /// for it in `.thruline/reexec.yml`.
    pub network: bool,
}

/// A prepared sandbox: the tree is on disk and the image is resolved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Prepared {
    /// The image reference actually used.
    pub image: String,
    /// **The evidence `reexecuted` rests on.** A content digest of the image,
    /// resolved from the runtime after preparation — not the tag, which is
    /// mutable.
    pub image_digest: String,
    /// The registry digest, when the image came from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_digest: Option<String>,
    pub image_source: ImageSource,
    /// Where the tree was materialised on the host.
    pub workdir: PathBuf,
    /// Paths written, symlinks written, and any submodule holes.
    #[serde(skip)]
    pub materialized: Materialized,
    /// Whether the network was left on.
    pub network: bool,
}

/// One command to run inside the sandbox.
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub argv: Vec<String>,
    /// Working directory **relative to the materialised tree root**. An
    /// absolute host path would name nothing inside the container.
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
}

/// What running it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub exit_code: i64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_ms: u64,
    /// `pipe` or `tty`. Recorded, never assumed — see the crate docs.
    pub stdio: &'static str,
    pub timed_out: bool,
}

/// The §3.3 `parsed` block for a re-executed check, carrying **the evidence
/// `reexecuted` is derived from**.
///
/// The container image digest is the load-bearing field: `thruline receipt`
/// reads it here and nowhere else, and `thruline verify` refuses a receipt that
/// claims re-execution without one. Everything beside it is context a reviewer
/// needs to know what "a clean container" meant on the day.
///
/// It rides in `parsed` because `checks` has no column for it and a DDL change
/// costs a migration on every store — the same reason, and the same precedent,
/// as the fold's `bypass` flag.
///
/// # Why this lives here and not at the call site
///
/// `argv` in this block is the **only** record of what a re-executed check ran:
/// there is no `exec` event to read it from, by design. `thruline claims` reads
/// it back to feed §6.3's narrowing detector, so writer and reader must agree on
/// the shape. One public producer means a test can build the same block the
/// product builds, rather than a hand-written fixture that drifts away from it.
pub fn parsed_block(
    tally: &Value,
    prepared: &Prepared,
    mode: &str,
    reexec_of: &str,
    argv: &[String],
    result: &ExecResult,
) -> Value {
    let mut out = tally.clone();
    let block = json!({
        "backend": "docker",
        "image": prepared.image,
        // THE field.
        "image_digest": prepared.image_digest,
        "repo_digest": prepared.repo_digest,
        "image_source": prepared.image_source.as_str(),
        "network": if prepared.network { "host" } else { "none" },
        "materialized_via": "git cat-file",
        "files": prepared.materialized.files.len() as u64,
        "mode": mode,
        "of_check": reexec_of,
        "argv": argv,
        // The recorder's children get a pipe and so do the container's. §7's
        // comparison conditions on this rather than assuming it.
        "stdio": result.stdio,
        "duration_ms": result.duration_ms,
        "timed_out": result.timed_out,
    });
    match out.as_object_mut() {
        Some(map) => {
            map.insert("reexec".into(), block);
            out
        }
        // `tally` is always an object from `parsed_of`; this keeps the function
        // total without an unwrap and never silently drops the digest.
        None => json!({ "unverifiable": "no tally", "reexec": block }),
    }
}

/// §7's trait.
///
/// `prepare` / `run` / `destroy`, as the spec writes it. The cloud backend
/// (feature `reexec-cloud`, spec §7) is the same three methods over an HTTP
/// API; nothing in the callers below reaches for Docker directly, so adding one
/// is a new implementation and not a change to this file.
pub trait Sandbox {
    fn prepare(&mut self, req: &PrepareRequest) -> Result<Prepared, SandboxError>;
    fn run(&mut self, req: &RunRequest) -> Result<ExecResult, SandboxError>;
    fn destroy(&mut self) -> Result<(), SandboxError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error(
        "no container runtime: `docker` is not on PATH or its daemon is not running ({detail}). \
         Re-execution needs one; without it a claim stays `unverifiable`, which is the honest \
         answer rather than a pass."
    )]
    NoRuntime { detail: String },
    #[error("`docker {argv}` failed with {status}: {stderr}")]
    Docker {
        argv: String,
        status: String,
        stderr: String,
    },
    #[error("running docker: {0}")]
    Spawn(#[source] std::io::Error),
    #[error(transparent)]
    Materialize(#[from] materialize::MaterializeError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error("preparing a workspace: {0}")]
    Workspace(#[source] std::io::Error),
    #[error("the sandbox was asked to run before it was prepared")]
    NotPrepared,
    #[error(
        "the image {image} resolved to no content digest, so nothing could bind the \
         re-execution to an environment. `reexecuted: true` is derived from that digest, so \
         a run without one produces no re-execution evidence at all."
    )]
    NoImageDigest { image: String },
    #[error(
        "{what} differs between the working copy and the tree being re-executed: the \
         caller asked for {worktree}, and the tree names {tree}. Refused: re-execution that \
         reads the worktree is not re-execution of the tree, and a receipt bound to \
         {SPEC_SECTION}'s final tree hash cannot be evidence about bytes that are not in it. \
         Commit or snapshot the change and re-run."
    )]
    TreeDivergence {
        what: String,
        worktree: String,
        tree: String,
    },
}

/// Why re-execution refused.
///
/// **Refusal, not annotation** (C.7/3). Each variant names a condition under
/// which a passing re-execution would be evidence about something other than
/// the artifact under review.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Refusal {
    /// S1 or S2 tripped: the worktree changed and the tree did not, so the tree
    /// this would materialise is not the code that ran.
    TreeFidelityBroken { detail: String },
    /// The session never produced a snapshot, so there is no final tree to
    /// re-execute at.
    NoFinalTree,
    /// The tree names a submodule, whose content is not in it (C.7 row 8).
    SubmoduleContentAbsent { paths: Vec<String> },
    /// Nothing in the session is a check that could be re-run.
    NoClaimedChecks,
}

impl Refusal {
    /// The one-line reason a human reads, and the receipt records.
    pub fn message(&self) -> String {
        match self {
            Refusal::TreeFidelityBroken { detail } => format!(
                "tree_fidelity_broken: {detail}. Re-execution materialises the tree, and the \
                 tree does not name the bytes that ran — re-running honest code to confirm a \
                 claim about cheating code IS the laundering, so this refuses rather than \
                 annotating."
            ),
            Refusal::NoFinalTree => "the session has no snapshot, so there is no final tree \
                 to re-execute against (§6.1)"
                .to_string(),
            Refusal::SubmoduleContentAbsent { paths } => format!(
                "the final tree names submodule(s) [{}], whose content is not in it. A \
                 directory built from this tree has a hole where source the tests may \
                 import used to be.",
                paths.join(", ")
            ),
            Refusal::NoClaimedChecks => {
                "no check in this session could be re-run: nothing recognised a runner".to_string()
            }
        }
    }
}

/// The gate §7 sits behind.
///
/// Deliberately a separate type from the sandbox: the decision *not to run* has
/// to be reachable, and testable, without a container runtime anywhere near it.
#[derive(Debug, Clone, Default)]
pub struct Gate {
    /// The session's capture-degraded flag, and why.
    pub capture_degraded: bool,
    pub degraded_reason: Option<String>,
    /// §6.1's final tree, if the session has one.
    pub final_tree_hash: Option<String>,
    /// How many checks the session recorded that a runner was recognised for.
    pub claimed_checks: usize,
}

impl Gate {
    /// May re-execution proceed?
    pub fn check(&self) -> Result<&str, Refusal> {
        if self.capture_degraded {
            return Err(Refusal::TreeFidelityBroken {
                detail: self
                    .degraded_reason
                    .clone()
                    .unwrap_or_else(|| "capture is degraded for this session".to_string()),
            });
        }
        let Some(tree) = self.final_tree_hash.as_deref() else {
            return Err(Refusal::NoFinalTree);
        };
        if self.claimed_checks == 0 {
            return Err(Refusal::NoClaimedChecks);
        }
        Ok(tree)
    }
}

/// Comparing an agent's run with ThruLine's re-run of it.
///
/// §7: *"Compare parsed results with the agent's run; mismatches are
/// `unsupported` with both records attached."*
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Comparison {
    /// Both runs agree on the exit code and, where comparable, on the parsed
    /// tally.
    Agrees { detail: String },
    /// They disagree. Both records are attached by the caller.
    Disagrees { detail: String },
    /// The two runs are not comparable on this axis, and saying so is the
    /// honest answer. See the crate docs on the TTY trap.
    NotComparable { detail: String },
}

impl Comparison {
    /// Compare exit codes and, when both sides parsed, tallies.
    ///
    /// **Output digests are deliberately not compared here.** They are a
    /// function of terminal width, colour, timing and absolute paths, none of
    /// which the tree hash controls; a digest comparison across a `pipe` run
    /// and a `tty` run reports tampering that did not happen. `stdio` is
    /// carried on both records so this can be conditioned on rather than
    /// guessed at.
    pub fn of(original: &RunFacts, rerun: &RunFacts) -> Comparison {
        if original.stdio != rerun.stdio {
            // Exit codes still compare — they are not a function of the
            // terminal — so only the parts that are get withheld.
            if original.exit_code != rerun.exit_code {
                return Comparison::Disagrees {
                    detail: format!(
                        "the agent's run exited {} and the re-execution exited {}",
                        original.exit_code, rerun.exit_code
                    ),
                };
            }
            return Comparison::NotComparable {
                detail: format!(
                    "the agent's run used stdio {:?} and the re-execution used {:?}; exit \
                     codes agree ({}), and output is not compared across that difference \
                     because it changes the bytes without anything having been tampered with",
                    original.stdio, rerun.stdio, original.exit_code
                ),
            };
        }
        if original.exit_code != rerun.exit_code {
            return Comparison::Disagrees {
                detail: format!(
                    "the agent's run exited {} and the re-execution of the same command at \
                     the same tree exited {}",
                    original.exit_code, rerun.exit_code
                ),
            };
        }
        match (&original.tally, &rerun.tally) {
            (Some(a), Some(b)) if a != b => Comparison::Disagrees {
                detail: format!(
                    "the agent's run reported {a:?} and the re-execution reported {b:?}"
                ),
            },
            (Some(a), Some(_)) => Comparison::Agrees {
                detail: format!("both runs exited {} and reported {a:?}", rerun.exit_code),
            },
            _ => Comparison::NotComparable {
                detail: format!(
                    "both runs exited {}, and at least one produced no parsable tally, so \
                     the results themselves were not compared",
                    rerun.exit_code
                ),
            },
        }
    }
}

/// The facts about one run that a comparison is entitled to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFacts {
    pub exit_code: i64,
    /// `(total, passed, failed)`, when the runner's output parsed.
    pub tally: Option<(u64, u64, u64)>,
    /// `pipe` or `tty`.
    pub stdio: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_section_is_recorded() {
        assert!(SPEC_SECTION.starts_with('§'));
    }

    #[test]
    fn a_degraded_session_cannot_re_execute() {
        let gate = Gate {
            capture_degraded: true,
            degraded_reason: Some("filter drivers [thruline-test]".into()),
            final_tree_hash: Some("abc".into()),
            claimed_checks: 1,
        };
        let refusal = gate.check().expect_err("must refuse");
        assert!(matches!(refusal, Refusal::TreeFidelityBroken { .. }));
        assert!(refusal.message().contains("tree_fidelity_broken"));
    }

    #[test]
    fn the_gate_lets_an_undegraded_session_through() {
        // The control. A gate that refuses everything passes the test above and
        // is worthless.
        let gate = Gate {
            capture_degraded: false,
            degraded_reason: None,
            final_tree_hash: Some("abc".into()),
            claimed_checks: 1,
        };
        assert_eq!(gate.check().expect("must allow"), "abc");
    }

    #[test]
    fn a_stdio_mismatch_withholds_the_output_comparison_but_not_the_exit_code() {
        let pipe = RunFacts {
            exit_code: 0,
            tally: Some((2, 2, 0)),
            stdio: "pipe".into(),
        };
        let tty = RunFacts {
            exit_code: 0,
            tally: Some((2, 2, 0)),
            stdio: "tty".into(),
        };
        assert!(matches!(
            Comparison::of(&pipe, &tty),
            Comparison::NotComparable { .. }
        ));
        let tty_failed = RunFacts {
            exit_code: 1,
            ..tty.clone()
        };
        // A real disagreement is still reported across the mismatch: an exit
        // code is not a function of the terminal.
        assert!(matches!(
            Comparison::of(&pipe, &tty_failed),
            Comparison::Disagrees { .. }
        ));
    }

    #[test]
    fn a_tally_difference_at_the_same_tree_disagrees() {
        let agent = RunFacts {
            exit_code: 0,
            tally: Some((2, 2, 0)),
            stdio: "pipe".into(),
        };
        let rerun = RunFacts {
            exit_code: 0,
            tally: Some((3, 3, 0)),
            stdio: "pipe".into(),
        };
        assert!(matches!(
            Comparison::of(&agent, &rerun),
            Comparison::Disagrees { .. }
        ));
        assert!(matches!(
            Comparison::of(&agent, &agent),
            Comparison::Agrees { .. }
        ));
    }
}
