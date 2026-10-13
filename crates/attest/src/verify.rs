//! BUILD-SPEC §8.1 — `thruline verify`, offline.
//!
//! > *offline verification of envelope signature, statement schema, subject
//! > digest against the local tree/commit, event-chain head against the local
//! > ledger (if present), and policy hash.*
//!
//! # The failure this file is built against
//!
//! **A `verify` that returns success because it never checked** — §6's "never
//! collapse `unverifiable` into `supported`", wearing a signature.
//!
//! So the result is not a boolean. It is a list of named checks, each of which
//! is `Pass`, `Fail` or **`Skipped`**, and [`Report::ok`] requires every one to
//! be `Pass`. A check that could not run is *not* a check that passed, and
//! there is no code path that turns one into the other: `Skipped` is a distinct
//! variant, and the only way to reach `ok() == true` is for the vector to
//! contain nothing else.
//!
//! # Everything is recomputed, nothing is read
//!
//! The statement says what the tree hash is. That is the *claim*, not the
//! evidence. Every digest below is recomputed from the local repository and the
//! local ledger and then compared — a receipt is a thing to check, not a thing
//! to believe.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::dsse::Envelope;
use crate::statement::{CheckEntry, Statement};

/// git's empty tree, which is byte-identical in every repository on earth.
///
/// **Therefore universally forgeable**: a receipt whose subject is the empty
/// tree verifies anywhere, about nothing. Refused by name.
pub const EMPTY_TREE_SHA1: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// The sha256 object-format empty tree, for repositories using it.
pub const EMPTY_TREE_SHA256: &str =
    "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Pass {
        detail: String,
    },
    Fail {
        reason: String,
    },
    /// **Not a pass.** The check could not run — no ledger, no repository, a
    /// key that was not supplied. The receipt asserts something and this
    /// machine could not confirm it.
    Skipped {
        reason: String,
    },
}

// # There is no fourth outcome, and there used to be
//
// `NotApplicable` stood here, meaning *"the receipt asserts nothing on this
// axis, so there is nothing to check"* — and it did not block `ok()`. It had
// exactly one producer: the arm `(_, "") => not_applicable("policy_hash")`.
// That is how a policy rewritten to its literal opposite produced
// byte-identical `Verified.` output at exit 0 (measured 2026-09-04): the
// producer wrote an empty policy hash, the verifier called the emptiness
// "not applicable", and the sentence printed.
//
// It was introduced so that `ok()` would stay reachable in a repository with
// no policy file. That reachability is now supplied honestly — every receipt
// states a policy digest, and "no policy file" has its own well-known digest
// (`thruline_policy::absent_digest`) — so the variant has no remaining use.
//
// It is **deleted rather than left unproduced**, because an outcome that does
// not block `ok()` and that nothing currently emits is an outcome a later
// session reaches for the moment a check is inconvenient. With three variants,
// `blocks()` is `!is_pass()`: a check either passed or it stands in the way.

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass { .. })
    }

    /// Does this outcome stand in the way of `ok()`?
    ///
    /// **Defined as "did not pass", not as a list of variants.** A list is a
    /// thing a new variant can be left off; this cannot be.
    pub fn blocks(&self) -> bool {
        !self.is_pass()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// The conclusion, as one value.
///
/// **This exists because the checks array alone collapses the distinction.** A
/// consumer reading `all(c.outcome != "fail")` — the obvious `jq` idiom — reads
/// a partially-verified receipt as verified, while the process exits 2. Every
/// downstream consumer (the Action, a CI script, a one-liner) is exactly the
/// surface where §6's "never collapse `unverifiable` into `supported`" gets
/// undone, so the conclusion is stated rather than left to be derived.
///
/// # The spelling is not decoration
///
/// The wire form used to be `"verified": "yes" | "partial" | "no"`, and the
/// **field name invited the bug it was built to prevent**: `"partial"` and
/// `"no"` are both truthy in Python and JavaScript, so `if r["verified"]:` —
/// exactly what that name suggests — reads a partially-verified and a *failed*
/// receipt as success. That is the same collapse, re-entering through a field
/// name.
///
/// So the JSON carries a real boolean `verified`, true only for [`Verified::Yes`],
/// **beside** `status`, whose spellings (`verified` / `partial` / `failed`) do
/// not contain a variant that reads as another one. [`Report`]'s serializer
/// writes all three from the one function, so they cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verified {
    /// Every check passed.
    #[serde(rename = "verified")]
    Yes,
    /// Nothing contradicts the receipt, and something could not be checked
    /// here. **Not a pass.**
    #[serde(rename = "partial")]
    Partial,
    /// At least one check failed.
    #[serde(rename = "failed")]
    No,
}

impl Verified {
    /// The process exit code this conclusion corresponds to. The JSON carries
    /// both, and a test asserts they agree with the real exit status.
    pub fn exit_code(self) -> u8 {
        match self {
            Verified::Yes => 0,
            Verified::No => 1,
            Verified::Partial => 2,
        }
    }

    /// The boolean a truthiness check may safely read. **Only `Yes` is true.**
    pub fn is_verified(self) -> bool {
        matches!(self, Verified::Yes)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Serialize for Report {
    /// Serialised with the conclusion FIRST, so a reader meets it before the
    /// detail.
    ///
    /// Three fields for one conclusion, all from [`Report::verified`]:
    /// `verified` is a **boolean** that is true only for a fully verified
    /// receipt (so `if r["verified"]:` is correct in Python and JavaScript),
    /// `status` is the tri-state, and `exit` is the process code. They are
    /// written from one call, so no consumer can find them disagreeing.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let conclusion = self.verified();
        let mut out = s.serialize_struct("Report", 4)?;
        out.serialize_field("verified", &conclusion.is_verified())?;
        out.serialize_field("status", &conclusion)?;
        out.serialize_field("exit", &conclusion.exit_code())?;
        out.serialize_field("checks", &self.checks)?;
        out.end()
    }
}

impl<'de> Deserialize<'de> for Report {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Report, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            checks: Vec<Check>,
        }
        Ok(Report {
            checks: Wire::deserialize(d)?.checks,
        })
    }
}

impl Report {
    fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.to_string(),
            outcome: Outcome::Pass {
                detail: detail.into(),
            },
        });
    }
    fn fail(&mut self, name: &str, reason: impl Into<String>) {
        self.checks.push(Check {
            name: name.to_string(),
            outcome: Outcome::Fail {
                reason: reason.into(),
            },
        });
    }
    fn skip(&mut self, name: &str, reason: impl Into<String>) {
        self.checks.push(Check {
            name: name.to_string(),
            outcome: Outcome::Skipped {
                reason: reason.into(),
            },
        });
    }

    /// **Every check must have passed.** A *skipped* check is never a passed
    /// one, and there is no third thing a check can be that still reaches
    /// `ok()` — see the note under [`Outcome`].
    pub fn ok(&self) -> bool {
        !self.checks.is_empty() && !self.checks.iter().any(|c| c.outcome.blocks())
    }

    /// The conclusion, stated. See [`Verified`].
    pub fn verified(&self) -> Verified {
        if self.checks.is_empty() {
            // Nothing was checked, so nothing is verified.
            return Verified::No;
        }
        if self
            .checks
            .iter()
            .any(|c| matches!(c.outcome, Outcome::Fail { .. }))
        {
            return Verified::No;
        }
        if self.ok() {
            Verified::Yes
        } else {
            Verified::Partial
        }
    }

    pub fn failed(&self) -> Vec<&Check> {
        self.checks
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::Fail { .. }))
            .collect()
    }

    pub fn skipped(&self) -> Vec<&Check> {
        self.checks
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::Skipped { .. }))
            .collect()
    }
}

/// What the local machine can tell the verifier.
///
/// Supplied by the caller so this module reaches neither git nor SQLite, and
/// every branch below is testable without either.
#[derive(Debug, Clone, Default)]
pub struct LocalEvidence {
    /// Was a git object store reachable at all? Distinguishes "no repository"
    /// (skip) from "repository that does not have it" (fail).
    pub have_objects: bool,
    /// Trees the local object store confirms it has.
    ///
    /// **Answers, not a walk.** This used to be a set built by listing 500
    /// commits and resolving 200 of their trees with one `git rev-parse`
    /// process each. Measured on this repository: the 500 bound excluded
    /// **0%** of history and the 200 bound excluded **53.2%**, at 1,907 ms —
    /// while asking the object store directly about the two hashes the receipt
    /// actually names costs ~10 ms **with no bound at all**. The bound that bit
    /// was the one the process spawns forced, not the one that was written
    /// down.
    pub tree_hashes: BTreeSet<String>,
    /// Commits the local object store confirms it has.
    pub commits: BTreeSet<String>,
    /// The chain head recomputed from the ledger's own rows for the session the
    /// receipt names, with its event count. `None` when the ledger has no such
    /// session.
    pub recomputed_chain: Option<(String, u64)>,
    /// Whether a ledger was available at all — distinguishes "no ledger" from
    /// "ledger without that session".
    pub ledger_present: bool,
    /// **Is this a shallow clone?**
    ///
    /// The finding that makes this field necessary: `actions/checkout` defaults
    /// to `fetch-depth: 1`, and `cat-file --batch-check` truthfully answers
    /// `missing` for an object a shallow clone does not have. Treating that as
    /// a *failure* would make the Action report a red check run on **entirely
    /// honest receipts, by default configuration** — and a check that fails
    /// honest work is as damaging as one that passes cheating.
    ///
    /// So a missing object in a shallow clone is `Skipped` (exit 2, a neutral
    /// check run, with instructions) rather than `Fail`. It is not a
    /// loosening: `Skipped` blocks `ok()`, so nothing reaches "verified" that
    /// would not have; the difference is between "this repository says the
    /// receipt is wrong" and "this repository cannot see far enough to say".
    pub shallow: bool,
    /// The digest of the policy in force **on this machine**, resolved the same
    /// way the producer resolved its own (`thruline_policy::digest_for_repo`).
    ///
    /// `None` only when this machine could not resolve one at all — no
    /// repository, an unreadable file, or a policy file that does not parse.
    /// That is a `Skipped`, because the failure is the machine's. A repository
    /// with **no** policy file resolves to `thruline_policy::absent_digest()`,
    /// which is `Some`: "there is no policy here" is an answer.
    pub policy_hash: Option<String>,
    /// **The commit the reviewer is actually looking at**, when one can be
    /// resolved: `--head-sha`, else `$GITHUB_SHA`, else this checkout's `HEAD`.
    ///
    /// Without it, binding is *existence* — `tree_hashes.contains(tree)` asks
    /// only whether an object is in the store, which is true of every commit a
    /// hostile branch can reach. A receipt about an entirely different line of
    /// history verified `success` against the head under review because nothing
    /// here ever named that head.
    pub head_commit: Option<String>,
    /// `git merge-base --is-ancestor <subject.gitCommit> <head>`, **answered**.
    ///
    /// `None` when it could not be decided here — no repository, a shallow
    /// clone, an object this checkout does not have. That is a `Skipped`, never
    /// a pass and never an accusation: `actions/checkout` defaults to
    /// `fetch-depth: 1`, so a failure here would redden honest work by default
    /// configuration.
    ///
    /// # Why ancestry and not equality
    ///
    /// The obvious rule — *fail when `subject.gitCommit != $GITHUB_SHA`* — was
    /// built and measured, and it turned **all eight** Action scenarios red.
    /// The receipt's commit is the commit the session worked at; the repository
    /// head is the commit that *adds the receipt*. They can never be equal,
    /// because committing a receipt is itself a commit, and "the developer
    /// committed a receipt path" is §9's own zero-install path. The correct
    /// rule is a relationship, not an identity.
    pub receipt_is_ancestor_of_head: Option<bool>,
    /// The paths at which the receipt's **subject tree** differs from the tree
    /// of the head under review. `Some(vec![])` means the two trees are
    /// identical; `None` means the comparison could not be made.
    ///
    /// Ancestry alone proves nothing: a commit that adds the receipt *and*
    /// rewrites the code the check ran on is an ancestor-descendant pair too.
    pub head_tree_differs_at: Option<Vec<String>>,
    /// **The ledger's own check rows** for the session this receipt names, as
    /// `id -> (exit_code, input_tree_hash)`. `None` when there is no ledger
    /// here, or no such session in it.
    ///
    /// # Why this field had to exist
    ///
    /// Re-deriving a verdict from `predicate.checks` is worth something only if
    /// `predicate.checks` is itself evidence, and it was bound to nothing:
    /// `chain_head` recomputes the **event** chain, and the check rows sit
    /// beside it, unhashed. Measured: a re-signer who flipped a verdict AND
    /// invented a coherent passing check — right tree, exit 0, clean tally,
    /// cited as the verdict's evidence — reached `Verified.` at exit 0 in a
    /// repository whose own `pytest -q` reported `1 failed`. Every other guard
    /// passed it, because each asks whether the receipt agrees with itself, and
    /// a forger writing the whole document can make it agree.
    pub ledger_checks: Option<std::collections::BTreeMap<String, (i64, String)>>,
    /// **Is the tree in this working directory, right now, the tree the receipt
    /// is about?**
    ///
    /// When it is, the binding question is already answered and needs no
    /// commit: the code a reviewer is looking at *is* the code the checks ran
    /// on. This matters because the ordinary case — issue a receipt about work
    /// in progress, check it here — is about a tree that is no commit at all,
    /// and resolving `HEAD` as "the head under review" there invents a question
    /// nobody asked.
    ///
    /// It is not a loophole: to reach it, an attacker's checkout must literally
    /// be the tree the checks ran on, which is the binding.
    pub subject_is_working_tree: bool,
    /// The receipt file's own path inside the repository, when it is in there.
    ///
    /// **The one path allowed to differ**, and the reason is narrow: the
    /// receipt did not exist when its checks ran, so its own addition cannot be
    /// something they covered. Nothing else is allowed, so an attacker who
    /// edits the receipt *and* the source is caught by the source.
    pub receipt_path_in_repo: Option<String>,
    /// Content transforms in scope in **this** repository, described, when they
    /// degrade capture (`ContentFilters::degrades_capture`).
    ///
    /// A `.gitattributes` clean filter means a tree hash computed here does not
    /// name the bytes on disk (`core::spec::snapshot`'s module docs), so the
    /// subject-tree pass above is not evidence *about those bytes*. Reported so
    /// the conclusion can say so instead of printing `Verified.` eight lines
    /// below the snapshotter's own warning — which is what it did.
    pub content_filters: Option<String>,
    /// **The verifying key was found inside the repository under review** —
    /// its repo-relative path.
    ///
    /// # Why a valid signature is not enough (F12, operator ruling 8)
    ///
    /// > *"`neutral` is the hosted App's ceiling. A repo-committed signing key
    /// > verifies to `neutral` with the reason stated. `success` requires a
    /// > signature whose identity the policy names: a keyless OIDC identity, or
    /// > a policy-listed public key held outside the repository."*
    ///
    /// A signature answers *who wrote this document*, and it answers it with
    /// whatever key the verifier was handed. When that key is committed in the
    /// repository being gated, everyone who can open a pull request against
    /// that repository can also sign a receipt with it — so the signature is
    /// valid, and it establishes nothing about identity at all. The check below
    /// therefore does not *fail*: the cryptography is fine and saying otherwise
    /// would be a false statement about the artefact. It **skips**, which is
    /// `Verified::Partial`, which is `Conclusion::Neutral`.
    ///
    /// `None` means no key used here was found in the repository. That is not
    /// the same as the key being *named by a policy*, which is the other half
    /// of ruling 8 and is not built — see the check's own reason text, which
    /// says so rather than letting a reader infer a promotion.
    pub verifying_key_in_repo: Option<String>,
    /// A Sigstore bundle sitting beside the receipt, and the policy to judge it
    /// under (Track B, DECISIONS #130 §1).
    ///
    /// `None` means this receipt was signed with a local key — the path that has
    /// always existed and still works. **Keyless is additive**: every receipt
    /// already issued under an Ed25519 key keeps verifying, because an
    /// overwritten guarantee is worse than a missing one.
    pub sigstore: Option<(String, thruline_sigstore::Policy)>,
}

/// Verify an envelope offline.
pub fn verify(
    envelope: &Envelope,
    keys: &[ed25519_dalek::VerifyingKey],
    local: &LocalEvidence,
) -> Report {
    let mut r = Report::default();

    // --- 1. the signature, over the exact PAE bytes -----------------------
    //
    // Two paths, and the second does not replace the first. Track B's §1.
    match &local.sigstore {
        // Keyless. Five checks under five names, because collapsing them into
        // one "signature: pass" is how a reader stops being able to tell which
        // guarantee actually held — a stale trust root and a tampered artifact
        // must not look alike in a check run.
        Some((bundle_json, policy)) => {
            match thruline_sigstore::Bundle::from_json(bundle_json)
                .and_then(|b| thruline_sigstore::verify(&b, policy))
            {
                Ok(id) => {
                    r.pass("signature", "DSSE PAE, by the certified key");
                    r.pass("certificate chain", "leaf → intermediate → Sigstore root");
                    r.pass("SCT", "embedded SCT verifies against the CT log");
                    r.pass(
                        "transparency log",
                        match id.log_index {
                            Some(i) => format!("Rekor inclusion proof verifies at index {i}"),
                            None => "verified".to_string(),
                        },
                    );
                    r.pass("identity", format!("{} via {}", id.san, id.issuer));
                }
                // `Error::axis()` names which of the five broke, so a failure
                // lands on the check it belongs to instead of on "signature".
                Err(e) => r.fail(&e.axis().to_string(), e.to_string()),
            }
        }
        None if keys.is_empty() => r.skip(
            "signature",
            "no verifying key and no Sigstore bundle were supplied, so the signature was \
             not checked — this is NOT a pass",
        ),
        None => match envelope.verify(keys) {
            Ok(i) => r.pass(
                "signature",
                format!("signature {i} verifies over the DSSE PAE"),
            ),
            Err(e) => r.fail("signature", e.to_string()),
        },
    }

    // --- 1b. whose signature it is (F12, ruling 8) -------------------------
    //
    // Deliberately outside the `keys.is_empty()` branch above: a receipt with
    // no key supplied already skips, and a receipt verified by a key sitting in
    // the repository under review must skip for its own, different reason. Two
    // reasons that both land on neutral still have to be two sentences, or the
    // one a reader needs is the one they do not get.
    if let Some(path) = &local.verifying_key_in_repo {
        r.skip(
            "signing_identity",
            format!(
                "the verifying key is committed in the repository under review, at {path}. \
                 The signature is valid and says nothing: anyone who can write this \
                 repository can sign a receipt with that key. `success` needs a signature \
                 whose identity a policy names — a keyless OIDC identity, or a public key \
                 held outside the repository — and this build has no policy-named identity \
                 to check against, so no key can reach `success` by this route yet"
            ),
        );
    }

    // --- 2. payload type and statement schema -----------------------------
    if envelope.payload_type != crate::statement::PAYLOAD_TYPE {
        r.fail(
            "payload_type",
            format!(
                "payloadType is {:?}, expected {:?} — the signature may be valid over a \
                 different kind of document entirely",
                envelope.payload_type,
                crate::statement::PAYLOAD_TYPE
            ),
        );
        return r;
    }
    r.pass("payload_type", "application/vnd.in-toto+json");

    let bytes = match envelope.payload_bytes() {
        Ok(b) => b,
        Err(e) => {
            r.fail("payload", e.to_string());
            return r;
        }
    };
    let statement: Statement = match crate::session_v2::decode(&bytes) {
        Ok(decoded) => decoded.statement,
        Err(e) => {
            r.fail(
                "statement_schema",
                format!("payload is not a Statement: {e}"),
            );
            return r;
        }
    };
    match statement.validate() {
        Ok(()) => r.pass(
            "statement_schema",
            "in-toto Statement v1, ThruLine predicate",
        ),
        Err(e) => {
            r.fail("statement_schema", e.to_string());
            return r;
        }
    }

    // --- 3. EVERY subject, recomputed locally -----------------------------
    //
    // in-toto statements legitimately carry more than one subject, and a reader
    // shown a verified receipt believes all of them were attested. Checking
    // only `subject[0]` meant a receipt whose first subject was legitimate and
    // whose second was bogus — an empty tree, say — verified clean at exit 0.
    for (i, subject) in statement.subject.iter().enumerate() {
        let name = |base: &str| {
            if statement.subject.len() == 1 {
                base.to_string()
            } else {
                format!("{base}[{i}]")
            }
        };
        let Some(tree) = subject.digest.get("gitTree") else {
            r.fail(
                &name("subject_tree"),
                format!("subject {i} carries no gitTree digest"),
            );
            continue;
        };
        if tree == EMPTY_TREE_SHA1 || tree == EMPTY_TREE_SHA256 {
            // The empty tree is byte-identical in every repository, so a
            // receipt about it verifies anywhere and says nothing.
            r.fail(
                &name("subject_tree"),
                format!(
                    "subject {i} is git's EMPTY TREE, which is identical in every \
                     repository — such a receipt would verify anywhere and attest to nothing"
                ),
            );
        } else if !local.have_objects {
            r.skip(
                &name("subject_tree"),
                "no local repository was available to recompute the tree from",
            );
        } else if local.tree_hashes.contains(tree) {
            r.pass(
                &name("subject_tree"),
                format!("{tree} is a tree in this repository"),
            );
        } else if local.shallow {
            r.skip(
                &name("subject_tree"),
                format!(
                    "tree {tree} is not in this SHALLOW clone, which cannot see far enough \
                     to say whether it exists — check out with `fetch-depth: 0` \
                     (actions/checkout defaults to 1) and re-run. This is NOT a pass."
                ),
            );
        } else {
            r.fail(
                &name("subject_tree"),
                format!(
                    "the receipt is about tree {tree}, which this repository does not \
                     have — resolved against the object store rather than read from the \
                     statement"
                ),
            );
        }

        if let Some(commit) = subject.digest.get("gitCommit") {
            if !local.have_objects {
                r.skip(
                    &name("subject_commit"),
                    "no local repository to look the commit up in",
                );
            } else if local.commits.contains(commit) {
                r.pass(
                    &name("subject_commit"),
                    format!("{commit} is present locally"),
                );
            } else if local.shallow {
                r.skip(
                    &name("subject_commit"),
                    format!(
                        "commit {commit} is not in this SHALLOW clone — check out with \
                         `fetch-depth: 0` (actions/checkout defaults to 1) and re-run. \
                         This is NOT a pass."
                    ),
                );
            } else {
                r.fail(
                    &name("subject_commit"),
                    format!(
                        "the receipt names commit {commit}, which this repository does not have"
                    ),
                );
            }
        }
    }

    // --- 4. the event chain, recomputed from the ledger --------------------
    let head = &statement.predicate.session.event_chain_head;
    match (&local.recomputed_chain, local.ledger_present) {
        (Some((local_head, count)), _) => {
            if local_head != head {
                r.fail(
                    "chain_head",
                    format!(
                        "the receipt claims chain head {head}, and the ledger's own rows \
                         recompute to {local_head} — the predicate does not describe the \
                         record this machine holds"
                    ),
                );
            } else if *count != statement.predicate.session.event_count {
                // The ledger is a forest of per-session chains, so a PREFIX of
                // a session verifies as a chain in its own right. A matching
                // head with a different count is a truncated session.
                r.fail(
                    "chain_head",
                    format!(
                        "the chain head matches but the receipt claims {} events where the \
                         ledger holds {count} — a truncated session verifies as a chain of \
                         its own, so the count is part of the identity",
                        statement.predicate.session.event_count
                    ),
                );
            } else {
                r.pass(
                    "chain_head",
                    format!("{head} recomputed from {count} ledger rows"),
                );
            }
        }
        (None, true) => r.fail(
            "chain_head",
            format!(
                "this machine has a ledger and no session {:?} in it, so the record the \
                 receipt describes cannot be checked here",
                statement.predicate.session.id
            ),
        ),
        (None, false) => r.skip(
            "chain_head",
            "no ledger on this machine, so the chain head was not checked — this is NOT \
             a pass",
        ),
    }

    // --- 5. re-execution must be derivable, never asserted -----------------
    //
    // §7 exists now, so `reexecuted: true` is legitimate — but only when
    // something backs it. The backing is the container image digest: the
    // re-execution appended an `exec` event carrying it to the hash chain this
    // receipt names, so a receipt that adds the boolean after the fact cannot
    // also produce the digest without breaking `chain_head`.
    //
    // A bare boolean is a claim. This check is what stops it being read as
    // evidence.
    let reexecuted: Vec<&CheckEntry> = statement
        .predicate
        .checks
        .iter()
        .filter(|c| c.reexecuted)
        .collect();
    let unbacked: Vec<&str> = reexecuted
        .iter()
        .filter(|c| {
            c.container_image_digest
                .as_deref()
                .map(str::trim)
                .is_none_or(|d| !d.starts_with("sha256:") || d.len() < "sha256:".len() + 32)
        })
        .map(|c| c.id.as_str())
        .collect();
    // The inverse must also hold, or the digest becomes decorative: a check
    // carrying a container digest and `reexecuted: false` is a record whose two
    // halves disagree about whether §7 ran.
    let orphan_digest: Vec<&str> = statement
        .predicate
        .checks
        .iter()
        .filter(|c| !c.reexecuted && c.container_image_digest.is_some())
        .map(|c| c.id.as_str())
        .collect();
    if !unbacked.is_empty() {
        r.fail(
            "reexecution",
            format!(
                "check(s) {} assert `reexecuted: true` with no container image digest \
                 behind them. The field is DERIVED from an `exec` event carrying that \
                 digest inside the hash chain this receipt names — a bare boolean is a \
                 claim, not evidence.",
                unbacked.join(", ")
            ),
        );
    } else if !orphan_digest.is_empty() {
        r.fail(
            "reexecution",
            format!(
                "check(s) {} carry a container image digest and say `reexecuted: false` — \
                 the record disagrees with itself about whether §7 ran",
                orphan_digest.join(", ")
            ),
        );
    } else if reexecuted.is_empty() {
        r.pass("reexecution", "no check claims to have been re-executed");
    } else {
        r.pass(
            "reexecution",
            format!(
                "{} check(s) re-executed, each bound to a container image digest",
                reexecuted.len()
            ),
        );
    }

    // --- 6. policy ---------------------------------------------------------
    //
    // §8 binds the policy into the receipt so a receipt cannot be verified
    // against a different policy than it was produced under. **Absent and
    // empty are different failures and are reported as different failures**;
    // neither is a pass, and there is no longer an outcome that would let
    // either be one.
    match (
        &local.policy_hash,
        statement.predicate.verifier.policy_hash.as_deref(),
    ) {
        (_, None) => r.fail(
            "policy_hash",
            "the receipt states NO policy hash. Every session is judged under some \
             policy — the shipped defaults included — so silence here is a MISSING \
             assertion, not an inapplicable check, and a receipt that cannot name the \
             policy it was produced under cannot be bound to one.",
        ),
        (_, Some("")) => r.fail(
            "policy_hash",
            "the receipt records an EMPTY policy hash. An empty string is neither a \
             digest nor an absence: it is a producer that wrote a placeholder. This \
             build refuses it under its own reason so it can never again be read as \
             `no policy claim` — which is how a policy rewritten to its literal \
             opposite once produced byte-identical `Verified.` output.",
        ),
        (None, Some(_)) => r.skip(
            "policy_hash",
            "this machine could not resolve a policy to compare against (no repository, \
             or a `.thruline/policy.yml` that does not parse), so the receipt's policy \
             was not checked — this is NOT a pass",
        ),
        (Some(local_hash), Some(receipt_hash)) if local_hash == receipt_hash => r.pass(
            "policy_hash",
            format!("the policy this receipt was judged under is the policy here ({receipt_hash})"),
        ),
        (Some(local_hash), Some(receipt_hash)) => r.fail(
            "policy_hash",
            format!(
                "the receipt was judged under policy {receipt_hash} and this machine's \
                 policy is {local_hash}"
            ),
        ),
    }

    // --- 7. capture fidelity ------------------------------------------------
    //
    // §0.3 binds everything to a tree hash, and a `.gitattributes` clean filter
    // breaks that binding at its root: git runs the filter on the way into the
    // object database, so the tree names the FILTERED bytes while the working
    // tree holds the originals. `thruline receipt` used to call the snapshotter,
    // let it print its warning, discard the finding, and sign
    // `capture_degraded: false`; `verify` then printed `Verified.` eight lines
    // below that same warning.
    //
    // The outcome is `Skipped`, not `Fail`, and the choice is deliberate.
    // §13 says degraded capture makes claims `unverifiable`, never `supported`
    // — `unverifiable`, and §13.1's most expensive error is accusing honest
    // users. `filter=lfs` is the commonest `.gitattributes` line in the world.
    // So this reports "could not be observed" (exit 2), never "you cheated".
    let cap = &statement.predicate.capture;
    if cap.content_filters.is_some() && !cap.capture_degraded {
        r.fail(
            "capture_fidelity",
            "the receipt names content transforms and says `capture_degraded: false` — \
             the record disagrees with itself about whether the tree hash names the \
             bytes on disk",
        );
    } else {
        let mut fidelity: Vec<String> = Vec::new();
        if cap.capture_degraded {
            // §8.1's causes, when the producer stated them. The `content_filters`
            // fallback is kept for receipts issued before `degraded_causes`
            // existed, and the bare sentence for receipts that state neither —
            // three cases, so an old receipt is read as an old receipt rather
            // than as a receipt with nothing wrong.
            // **Every cause, verbatim, including tokens this build does not
            // know.** The spec's rule (§5.7): *"The token set is open by
            // design, and a consumer that meets one it does not recognise MUST
            // report it rather than drop it — a cause you cannot parse is still
            // a reason the capture was incomplete, and silently discarding it
            // turns an incomplete record into a clean-looking one."*
            //
            // So this joins rather than matches. There is deliberately no
            // `match` on the token here: the moment one exists, an unrecognised
            // arm has to be written, and the tempting arm to write is the one
            // that skips. `an_unrecognised_degraded_cause_is_reported_not_dropped`
            // is the pin.
            fidelity.push(if !cap.degraded_causes.is_empty() {
                format!(
                    "the receipt records DEGRADED capture: {}",
                    cap.degraded_causes.join("; ")
                )
            } else {
                match &cap.content_filters {
                    Some(d) => format!("the receipt records DEGRADED capture ({d})"),
                    None => "the receipt records DEGRADED capture and states no cause; it \
                             was issued before §8.1's `degraded_causes` was emitted"
                        .to_string(),
                }
            });
        }
        if let Some(here) = &local.content_filters {
            fidelity.push(format!(
                "this repository has content transforms in scope ({here}), so a tree hash \
                 computed here does not name the bytes on disk and the subject check \
                 above is not evidence about them"
            ));
        }
        if fidelity.is_empty() {
            r.pass(
                "capture_fidelity",
                "capture was not degraded, and no content transform is in scope here",
            );
        } else {
            r.skip(
                "capture_fidelity",
                format!(
                    "{}. §13: degraded capture makes claims `unverifiable`, never \
                     `supported`. This is NOT a pass.",
                    fidelity.join("; ")
                ),
            );
        }
    }

    // --- 8. the receipt's own verdicts --------------------------------------
    //
    // The sentence this command prints on success is *"Every check the receipt
    // asserts was confirmed here."* A receipt carrying an `unverifiable`
    // verdict asserts, in writing, that a claim could NOT be decided. Printing
    // that sentence over it is §6's collapse in the most visible place the
    // product has, and it was measured doing exactly that: a receipt whose only
    // verdict read `unverifiable — no test check was captured in this session`
    // verified clean at exit 0.
    //
    // `unsupported` is deliberately NOT blocking here. An honestly recorded
    // `unsupported` verdict is a correct receipt; whether it may merge is §9's
    // gate, not this command's. It is counted in the detail so it is visible.
    let unverifiable: Vec<&str> = statement
        .predicate
        .verdicts
        .iter()
        .filter(|v| v.verdict == "unverifiable")
        .map(|v| v.claim_id.as_str())
        .collect();
    let unsupported = statement
        .predicate
        .verdicts
        .iter()
        .filter(|v| v.verdict == "unsupported")
        .count();
    if unverifiable.is_empty() {
        r.pass(
            "verdicts",
            format!(
                "{} verdict(s), none left `unverifiable` ({unsupported} `unsupported`, \
                 which is the gate's business and not this command's)",
                statement.predicate.verdicts.len()
            ),
        );
    } else {
        r.skip(
            "verdicts",
            format!(
                "{} of {} verdict(s) are `unverifiable` — the receipt itself says these \
                 claims could not be decided ({}). Reporting that as verified is the \
                 collapse §6 forbids. This is NOT a pass.",
                unverifiable.len(),
                statement.predicate.verdicts.len(),
                unverifiable.join(", ")
            ),
        );
    }

    // --- 9. every `supported` verdict, RE-DERIVED from the evidence beside it -
    //
    // # The finding
    //
    // A genuine receipt was minted over a **failing** check, re-signed with the
    // verdict flipped to `supported`, and this command reported `success`: exit
    // 0, a green badge, and a PR table listing the claim as supported — over a
    // `predicate.checks[0]` recording `exit_code: 1` and `parsed.failed: 2`.
    //
    // **The signature was valid.** It has to be: an agent that can run commands
    // can write a receipt, and the party the gate is about is the party holding
    // the key. So a signature says who wrote the document, never whether the
    // document is true, and every conclusion this build draws from a verdict
    // was drawn from a word the gated party typed.
    //
    // Separately, nothing read `input_tree_hash` at all — `grep -c` over this
    // file returned 0. A repository sabotaged *after* a green run produced a
    // receipt whose subject was the sabotaged tree, whose check recorded the
    // tree that passed, and whose verdict said `supported` for the stated
    // reason *"ran on the final tree"*. The reason was prose; the two hashes
    // sat in the same document, unfetched.
    //
    // # What is re-derived, and what deliberately is not
    //
    // Only `supported` is re-derived, and only *downward*. An honestly recorded
    // `unsupported` is a correct receipt whose merge is §9's business, and
    // "re-deriving" it could only move it up. The four contradictions below are
    // each a fact carried in the same document as the verdict:
    //
    // 1. the check exited non-zero;
    // 2. the check's own tally reports failures (a `|| true`, or a wrapper that
    //    swallows the status, exits 0 over a red run);
    // 3. the check's `parsed` block says the run could not be read — §6:
    //    `unverifiable` never collapses into `supported`;
    // 4. the check ran on a tree that is not any subject of this receipt.
    //
    // **A verdict this receipt cannot re-derive is `Skipped`, not `Fail`.**
    // §3.4's `evidence` legitimately carries `evt_…` ledger ids, which are not
    // in the receipt, and §13.1's most expensive error is accusing honest work.
    // `Skipped` blocks `ok()`, so nothing reaches "verified" that would not
    // have.
    let subject_trees: BTreeSet<&str> = statement
        .subject
        .iter()
        .filter_map(|s| s.digest.get("gitTree").map(String::as_str))
        .collect();
    let by_id: std::collections::BTreeMap<&str, &CheckEntry> = statement
        .predicate
        .checks
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    // §6.3, re-derived without needing the `evidence` array.
    //
    // **Why this second rule exists.** The producer populates `evidence` on the
    // path that reaches `supported` and leaves it empty on the `unsupported`
    // early return — so the cheapest forgery is exactly the measured one:
    // flip an evidence-less `unsupported` to `supported` and the citation-based
    // rule above has nothing to resolve. §6.3 gives a rule that needs no
    // citation: *a failing test check is evidence AGAINST and outranks any
    // passing one*, so a `tests_pass` claim cannot be `supported` in a receipt
    // that carries a failing test-category check. That is the producer's own
    // rule (`tests_pass::verify` returns `Unsupported` on the first such check
    // it finds, over the same unfiltered set of rows this receipt lists), and
    // re-deriving it here means the receipt is checked against it rather than
    // trusted to have applied it.
    let claim_kind: std::collections::BTreeMap<&str, &str> = statement
        .predicate
        .claims
        .iter()
        .map(|c| (c.id.as_str(), c.kind.as_str()))
        .collect();
    let failing_test_check = statement.predicate.checks.iter().find(|c| {
        c.category == "test"
            && (c.exit_code != 0
                || c.parsed
                    .get("failed")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    > 0)
    });
    let mut contradictions: Vec<String> = Vec::new();
    let mut unrederivable: Vec<String> = Vec::new();
    let mut rederived = 0usize;
    if let Some(bad) = failing_test_check {
        for v in statement
            .predicate
            .verdicts
            .iter()
            .filter(|v| v.verdict == "supported")
            .filter(|v| claim_kind.get(v.claim_id.as_str()) == Some(&"tests_pass"))
        {
            contradictions.push(format!(
                "{}: a `tests_pass` claim recorded `supported` while this same receipt carries \
                 test check {} (exit {}, parsed {}). §6.3: a failing test check outranks any \
                 passing one, so this pair cannot be produced honestly",
                v.claim_id, bad.id, bad.exit_code, bad.parsed
            ));
        }
    }
    for v in statement
        .predicate
        .verdicts
        .iter()
        .filter(|v| v.verdict == "supported")
    {
        let cited: Vec<&&CheckEntry> = v
            .evidence
            .iter()
            .filter_map(|id| by_id.get(id.as_str()))
            .collect();
        if cited.is_empty() {
            unrederivable.push(format!(
                "{}: `supported`, and none of its evidence ({}) names a check in this receipt",
                v.claim_id,
                if v.evidence.is_empty() {
                    "none cited".to_string()
                } else {
                    v.evidence.join(", ")
                }
            ));
            continue;
        }
        let mut derived_here = false;
        for c in cited {
            // §8.1: `{}` is never written — an unparsed run carries
            // `{"unverifiable": …}` — so an absent or empty block is a producer
            // that wrote a placeholder, and a placeholder must not read as a
            // clean run.
            let Some(p) = c.parsed.as_object().filter(|p| !p.is_empty()) else {
                unrederivable.push(format!(
                    "{}: check {} carries no parsed result, so `supported` cannot be re-derived \
                     from it",
                    v.claim_id, c.id
                ));
                continue;
            };
            derived_here = true;
            if c.exit_code != 0 {
                contradictions.push(format!(
                    "{}: `supported` over check {}, which exited {}",
                    v.claim_id, c.id, c.exit_code
                ));
            }
            for tally in ["failed", "errored"] {
                if p.get(tally)
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    > 0
                {
                    contradictions.push(format!(
                        "{}: `supported` over check {}, whose own tally reports {} {tally}",
                        v.claim_id, c.id, p[tally]
                    ));
                }
            }
            if p.contains_key("unverifiable") {
                contradictions.push(format!(
                    "{}: `supported` over check {}, whose parsed result is `unverifiable` — \
                     §6 forbids that collapse",
                    v.claim_id, c.id
                ));
            }
            if !subject_trees.contains(c.input_tree_hash.as_str()) {
                contradictions.push(format!(
                    "{}: `supported` over check {}, which ran on tree {} while this receipt is \
                     about {}. The verdict's own stated reason may say it ran on the final tree; \
                     these two hashes are in the same document and they disagree.",
                    v.claim_id,
                    c.id,
                    c.input_tree_hash,
                    subject_trees.iter().copied().collect::<Vec<_>>().join(", ")
                ));
            }
        }
        if derived_here {
            rederived += 1;
        }
    }
    if !contradictions.is_empty() {
        r.fail(
            "verdict_derivation",
            format!(
                "the receipt's own evidence contradicts what it says, {} time(s): {}. A verdict is a \
                 word the producer wrote; the checks beside it are the record. The signature \
                 proves who wrote the document, never that it is true — the signer is the party \
                 the gate is about.",
                contradictions.len(),
                contradictions.join("; ")
            ),
        );
    } else if !unrederivable.is_empty() {
        r.skip(
            "verdict_derivation",
            format!(
                "{} `supported` verdict(s) could not be re-derived from evidence carried in this \
                 receipt ({}). Nothing contradicts them and nothing here confirms them, so they \
                 are reported as unchecked. This is NOT a pass.",
                unrederivable.len(),
                unrederivable.join("; ")
            ),
        );
    } else {
        r.pass(
            "verdict_derivation",
            format!(
                "{rederived} `supported` verdict(s) re-derived from the checks cited beside \
                 them, and each ran on a tree this receipt is about"
            ),
        );
    }

    // --- 10. the checks themselves, against the ledger that recorded them -----
    //
    // Everything above asks whether the receipt agrees with itself. A forger
    // who writes the whole document can make it agree — and did: a re-signed
    // receipt carrying an invented but perfectly coherent passing check
    // verified clean. So where the ledger is here, the receipt's copy of the
    // check rows is compared against the rows themselves.
    //
    // Where it is NOT here, this is a `Skipped`. §8.1 says "against the local
    // ledger (if present)", and a reviewer holding only the repository and the
    // receipt has no ledger — accusing them would be §13.1's expensive error.
    // Nothing is endorsed by the skip: it blocks `ok()` like every other.
    match &local.ledger_checks {
        None => r.skip(
            "check_provenance",
            "no ledger here holds this session, so the receipt's check rows were compared \
             against nothing. They are a COPY of the record, not the record — and a re-signed \
             receipt can carry a check that was never run. This is NOT a pass.",
        ),
        Some(rows) => {
            let mut wrong: Vec<String> = Vec::new();
            for c in &statement.predicate.checks {
                match rows.get(&c.id) {
                    None => wrong.push(format!(
                        "check {} is in this receipt and in no ledger row of session {:?} — an \
                         invented check is the cheapest way to make a forged verdict re-derive",
                        c.id, statement.predicate.session.id
                    )),
                    Some((exit_code, tree)) => {
                        if *exit_code != c.exit_code {
                            wrong.push(format!(
                                "check {} says exit {} and the ledger recorded exit {exit_code}",
                                c.id, c.exit_code
                            ));
                        }
                        if tree != &c.input_tree_hash {
                            wrong.push(format!(
                                "check {} says it ran on tree {} and the ledger recorded {tree}",
                                c.id, c.input_tree_hash
                            ));
                        }
                    }
                }
            }
            if wrong.is_empty() {
                r.pass(
                    "check_provenance",
                    format!(
                        "{} check(s) in this receipt are the ledger's own rows",
                        statement.predicate.checks.len()
                    ),
                );
            } else {
                r.fail("check_provenance", wrong.join("; "));
            }
        }
    }

    // --- 11. the head under review, bound by RELATIONSHIP not by existence ----
    //
    // # The finding
    //
    // Binding was `local.tree_hashes.contains(tree)` — *is this object in the
    // store* — and `head_sha` appeared nowhere in this file. Every object a
    // hostile branch can reach is in the store, so a receipt about an entirely
    // different line of history verified `success` against the head under
    // review, and the check run posted that success against the hostile sha.
    //
    // # The remedy that does not work, and why it is not used
    //
    // *"Fail when `subject.gitCommit != $GITHUB_SHA`"* was implemented and
    // measured: **all eight** Action scenarios turned red. The receipt's commit
    // is the commit the session worked at; the head is the commit that *adds
    // the receipt*. They can never be equal, because committing a receipt is
    // itself a commit — and "the developer committed a receipt path" is §9's
    // own documented zero-install path. A rule that fails every honest pull
    // request following the documented workflow is not a strict rule, it is a
    // broken one.
    //
    // # The relationship
    //
    // The receipt's commit must be an **ancestor** of the head, and the diff
    // between the receipt's subject tree and the head's tree must touch nothing
    // the checks were about. Nothing in a receipt records which paths its
    // checks covered, so "touches nothing they were about" is not knowable
    // here: the only path allowed to differ is the receipt file's own, which
    // did not exist when the checks ran. Anything else is `Skipped` — *this
    // could not be established* — rather than `Fail`, because the receipt may
    // be entirely honest and simply not describe this head.
    //
    // Not an ancestor at all is different, and is a `Fail`: that receipt is
    // about another history.
    //
    // # The case that needs no commit
    //
    // When the tree in this working directory IS the subject tree, the binding
    // is already established and no commit is involved: the code being looked
    // at is the code the checks ran on. That is the ordinary local flow — a
    // receipt about work in progress, checked where it was made — and treating
    // it as unbound would make the honest answer unreachable for every session
    // that has not committed yet. It is not a way around the rule: reaching it
    // requires the reviewer's own checkout to be that exact tree.
    let receipt_commit = statement
        .subject
        .first()
        .and_then(|s| s.digest.get("gitCommit"));
    if local.subject_is_working_tree {
        r.pass(
            "head_binding",
            "the tree in this working directory IS the tree this receipt is about, so what is \
             being reviewed here is what the checks ran on — no commit needed",
        );
    } else {
        match (receipt_commit, &local.head_commit) {
            (None, _) => r.skip(
                "head_binding",
                "the receipt names no commit, only a tree, so it cannot be tied to the head under \
             review. An uncommitted working tree is a normal state — this is not an accusation, \
             and it is NOT a pass.",
            ),
            (Some(_), None) => r.skip(
                "head_binding",
                "no head under review could be resolved here (no repository, or no `--head-sha`, \
             `$GITHUB_SHA` or local `HEAD`), so the receipt was not tied to one. Existence of \
             the objects in this store is not identity with the code being reviewed. This is \
             NOT a pass.",
            ),
            (Some(commit), Some(head)) => {
                // Equality is the trivial case, not the rule: the receipt's commit
                // may legitimately be an ancestor. Either way the trees are still
                // compared below.
                let related = if commit == head {
                    Some(true)
                } else {
                    local.receipt_is_ancestor_of_head
                };
                // The same relationship, said correctly in both shapes: the head
                // may BE the receipt's commit, or be a descendant of it.
                let relation = if commit == head {
                    format!("the head under review IS the receipt's commit {commit}")
                } else {
                    format!(
                    "the receipt's commit {commit} is an ancestor of the head under review {head}"
                )
                };
                match related {
                    None => r.skip(
                        "head_binding",
                        format!(
                        "whether the receipt's commit {commit} is an ancestor of the head under \
                         review {head} could not be decided here{}. Existence in the object \
                         store is not identity. This is NOT a pass.",
                        if local.shallow {
                            " — this is a SHALLOW clone; check out with `fetch-depth: 0` \
                             (actions/checkout defaults to 1) and re-run"
                        } else {
                            ""
                        }
                    ),
                    ),
                    Some(false) => r.fail(
                        "head_binding",
                        format!(
                        "the receipt is about commit {commit}, which is NOT an ancestor of the \
                         head under review {head} — it describes a different line of history. \
                         The objects it names are in this store, which is why an existence check \
                         passed it; being in the store is not being what is being reviewed."
                    ),
                    ),
                    Some(true) => match &local.head_tree_differs_at {
                        None => r.skip(
                            "head_binding",
                            format!(
                            "{relation}, and the difference between the receipt's tree and that \
                             head's tree could not be computed here, so nothing establishes that \
                             the checks cover what is being reviewed. This is NOT a pass."
                        ),
                        ),
                        Some(paths) => {
                            let allowed = local.receipt_path_in_repo.as_deref();
                            let touched: Vec<&str> = paths
                                .iter()
                                .map(String::as_str)
                                .filter(|p| Some(*p) != allowed)
                                .collect();
                            if touched.is_empty() {
                                r.pass(
                                    "head_binding",
                                    format!(
                                    "the head under review ({head}) carries the tree this receipt \
                                     is about{}",
                                    match allowed {
                                        Some(p) if !paths.is_empty() =>
                                            format!(", differing only at the receipt itself ({p})"),
                                        _ => String::new(),
                                    }
                                ),
                                );
                            } else {
                                r.skip(
                                    "head_binding",
                                    format!(
                                        "{relation}, and the tree being reviewed differs \
                                     from the tree the checks ran on at {} path(s): {}. Nothing \
                                     in a receipt records which paths its checks covered, so this \
                                     build cannot say those changes are harmless — only that the \
                                     evidence does not reach them. This is NOT a pass.",
                                        touched.len(),
                                        touched.join(", ")
                                    ),
                                );
                            }
                        }
                    },
                }
            }
        }
    }

    r
}
