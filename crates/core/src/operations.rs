//! C5 — the operation/replay ledger (A182).
//!
//! The machinery the continuity handshake has been abstaining and **naming**
//! since A180: "until the C5 operation/replay ledger exists, whether an
//! operation was re-run cannot be decided from rows." This module is that
//! ledger, and the abstention becomes a real answer.
//!
//! Three rules:
//!
//! 1. **Identity is the command, not the sentence about it.** An operation's
//!    identity is the sha256 of its normalized command text, so "was this
//!    re-run?" is a digest comparison rather than a judgement.
//! 2. **The replay class is carried by a deterministic rule, and the rule is
//!    named in the row.** `safe · conditional · unsafe · unknown` are Layer-3
//!    words; RB7 permits them when carried by **proof — a deterministic rule
//!    that entails the word** — so every row stores the `basis` that produced
//!    its class. A class with no rule behind it is `unknown`, which is a real
//!    answer and the default.
//! 3. **The outcome is read from the record, never assumed.** A tool call with
//!    no captured result is `unknown`, not `succeeded`.

use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClass {
    /// Re-running changes nothing outside this machine.
    Safe,
    /// Re-running is usually harmless but can overwrite or duplicate.
    Conditional,
    /// Re-running touches the world: a deploy, a migration, a publish.
    Unsafe,
    /// No rule claims this shape. The default, and a real answer.
    Unknown,
}

impl ReplayClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayClass::Safe => "safe",
            ReplayClass::Conditional => "conditional",
            ReplayClass::Unsafe => "unsafe",
            ReplayClass::Unknown => "unknown",
        }
    }
    /// Whether re-running this without a human deciding is a problem worth
    /// naming. `Unknown` counts: not knowing is exactly when to ask.
    pub fn must_not_repeat_silently(self) -> bool {
        matches!(
            self,
            ReplayClass::Unsafe | ReplayClass::Conditional | ReplayClass::Unknown
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub op_id: String,
    pub project_id: String,
    pub session_id: String,
    pub event_id: String,
    pub op_type: String,
    pub target: Option<String>,
    pub command_sha256: String,
    pub command_preview: String,
    pub observed_at: Option<String>,
    pub outcome: String,
    pub replay_class: ReplayClass,
    /// The rule that entailed the class. RB7 proof provenance, per row.
    pub basis: String,
    /// What the masking pass on `command_preview` did (A209-C4). Never
    /// persisted — it is provenance for the renderer, carried beside the
    /// text so the 120-character cut below cannot erase it.
    #[serde(default, skip_serializing)]
    pub mask: crate::redact::MaskRecord,
}

/// One classification rule: a command shape, the operation type it names, the
/// class it entails, and the sentence that says why. **This table IS the proof
/// provenance** — a class exists only because a rule here produced it.
struct Rule {
    needle: &'static str,
    op_type: &'static str,
    class: ReplayClass,
    basis: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        needle: "git push",
        op_type: "git_push",
        class: ReplayClass::Conditional,
        basis: "a push writes to a remote others can read; re-running is usually a no-op but \
                can overwrite a branch that moved",
    },
    Rule {
        needle: "git commit",
        op_type: "git_commit",
        class: ReplayClass::Conditional,
        basis: "a commit is local and reversible, but re-running duplicates it on a tree that \
                has already moved",
    },
    Rule {
        needle: "npm publish",
        op_type: "package_publish",
        class: ReplayClass::Unsafe,
        basis: "publishing releases an artifact to a public registry; the effect leaves this \
                machine and cannot be withdrawn by re-running",
    },
    Rule {
        needle: "functions deploy",
        op_type: "deploy",
        class: ReplayClass::Unsafe,
        basis: "a deploy replaces running code that other people are using",
    },
    Rule {
        needle: "deploy",
        op_type: "deploy",
        class: ReplayClass::Unsafe,
        basis: "a deploy replaces running code that other people are using",
    },
    Rule {
        needle: "migrate",
        op_type: "migration",
        class: ReplayClass::Unsafe,
        basis: "a migration changes stored data; re-running one that already ran is the classic \
                way to corrupt a database",
    },
    Rule {
        needle: "git status",
        op_type: "git_read",
        class: ReplayClass::Safe,
        basis: "a read-only inspection: re-running it changes nothing anywhere",
    },
    Rule {
        needle: "git diff",
        op_type: "git_read",
        class: ReplayClass::Safe,
        basis: "a read-only inspection: re-running it changes nothing anywhere",
    },
];

/// Normalize a command for identity: collapse whitespace so that formatting
/// alone never makes one operation look like two.
fn normalize(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn classify(command: &str) -> (&'static str, ReplayClass, &'static str) {
    let hay = command.to_lowercase();
    // First match wins, and the table is ordered most-specific first
    // (`functions deploy` before the bare `deploy`).
    for rule in RULES {
        if hay.contains(rule.needle) {
            return (rule.op_type, rule.class, rule.basis);
        }
    }
    (
        "unclassified",
        ReplayClass::Unknown,
        "no rule in the replay table claims this command shape, so no class is entailed — \
         not knowing is a real answer and is the one that asks a human",
    )
}

/// Derive every side-effect-shaped operation this project's rows hold.
///
/// Read-only and deterministic: the same rows always produce the same
/// operations, with the same identities.
pub fn derive(ledger: &Ledger, project_id: &str) -> Result<Vec<Operation>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT e.id, e.session_id, e.ts, e.tool_input, e.tool_use_id,
                (SELECT r.tool_result FROM events r
                  WHERE r.project_id = e.project_id AND r.kind = 'tool_result'
                    AND r.tool_use_id = e.tool_use_id LIMIT 1),
                (SELECT r.is_error FROM events r
                  WHERE r.project_id = e.project_id AND r.kind = 'tool_result'
                    AND r.tool_use_id = e.tool_use_id LIMIT 1)
         FROM events e
         WHERE e.project_id = ?1 AND e.kind = 'tool_use' AND e.tool_name = 'Bash'
         ORDER BY e.seq",
    )?;
    let rows = stmt.query_map([project_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<i64>>(6)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (event_id, session_id, ts, input, _tool_use_id, result, is_error) = row?;
        let command = input
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|v| {
                v.get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        if command.is_empty() {
            continue;
        }
        let (op_type, class, basis) = classify(&command);
        // Reads are recorded but are not what this ledger is for; keeping them
        // out means "operations" means "things that may need a decision".
        if class == ReplayClass::Safe {
            continue;
        }
        let normalized = normalize(&command);
        let command_sha256 = sha256_hex(normalized.as_bytes());
        // The outcome is read, never assumed.
        let outcome = match (&result, is_error) {
            (Some(_), Some(1)) => "failed",
            (Some(_), _) => "succeeded",
            (None, _) => "unknown",
        };
        let (masked_command, mask) = crate::redact::mask_recorded(&normalize(&command));
        let preview = crate::redact::truncate_respecting_markers(&masked_command, 120);
        out.push(Operation {
            // Identity is stable across re-derivation: same event, same op.
            op_id: format!("op_{}", &sha256_hex(event_id.as_bytes())[..24]),
            project_id: project_id.to_string(),
            session_id,
            event_id,
            op_type: op_type.to_string(),
            target: None,
            command_sha256,
            // A209-C3: mask BEFORE the cut. Truncating first can slice a
            // credential's body below its detector threshold, and the cut
            // string is what every downstream renderer (both capsules)
            // masks — so the secret is already unrecognisable by then. A
            // 143-char curl carrying a token past char 105 reached the
            // sealed capsule bytes exactly this way.
            command_preview: preview,
            observed_at: ts,
            outcome: outcome.to_string(),
            replay_class: class,
            basis: basis.to_string(),
            mask,
        });
    }
    Ok(out)
}

/// Record derived operations. Append-only and idempotent: re-deriving the same
/// rows inserts nothing new, because the identity is the event.
pub fn record(ledger: &Ledger, ops: &[Operation]) -> Result<u64> {
    let mut appended = 0u64;
    for op in ops {
        let n = ledger.conn().execute(
            "INSERT OR IGNORE INTO operations(op_id, project_id, session_id, event_id, op_type,
                target, command_sha256, command_preview, observed_at, outcome, replay_class, basis)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            rusqlite::params![
                op.op_id,
                op.project_id,
                op.session_id,
                op.event_id,
                op.op_type,
                op.target,
                op.command_sha256,
                op.command_preview,
                op.observed_at,
                op.outcome,
                op.replay_class.as_str(),
                op.basis,
            ],
        )?;
        appended += n as u64;
    }
    Ok(appended)
}

/// The do-not-repeat set: operations that completed and whose replay is not
/// known-safe. These are what a successor must not silently re-run.
pub fn do_not_repeat(ops: &[Operation]) -> Vec<&Operation> {
    ops.iter()
        .filter(|o| o.outcome == "succeeded" && o.replay_class.must_not_repeat_silently())
        .collect()
}

/// Did anything re-run one of these after `after_event_id`?
///
/// A digest comparison, not a judgement: the same normalized command appearing
/// in a later event is a repeat, and anything else is not.
pub fn repeats_after(
    ledger: &Ledger,
    project_id: &str,
    ops: &[&Operation],
    after_event_id: &str,
) -> Result<Vec<String>> {
    let mut repeated = Vec::new();
    for op in ops {
        let mut stmt = ledger.conn().prepare(
            "SELECT e.id, e.tool_input FROM events e
             WHERE e.project_id = ?1 AND e.kind = 'tool_use' AND e.tool_name = 'Bash'
               AND e.id > ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![project_id, after_event_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (_id, input) = row?;
            let command = input
                .as_deref()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .and_then(|v| {
                    v.get("command")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            if sha256_hex(normalize(&command).as_bytes()) == op.command_sha256 {
                repeated.push(op.command_preview.clone());
                break;
            }
        }
    }
    Ok(repeated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_is_entailed_by_a_named_rule() {
        for (command, expect) in [
            ("git push origin main", ReplayClass::Conditional),
            ("supabase functions deploy fn", ReplayClass::Unsafe),
            ("npm publish", ReplayClass::Unsafe),
            ("pnpm migrate:up", ReplayClass::Unsafe),
            ("ls -la", ReplayClass::Unknown),
        ] {
            let (_t, class, basis) = classify(command);
            assert_eq!(class, expect, "{command} classified {class:?}");
            assert!(
                basis.len() > 30,
                "a class with no reasoning behind it is not proof provenance: {command}"
            );
        }
    }

    #[test]
    fn identity_ignores_formatting_but_not_content() {
        let a = sha256_hex(normalize("git   push    origin main").as_bytes());
        let b = sha256_hex(normalize("git push origin main").as_bytes());
        let c = sha256_hex(normalize("git push origin other").as_bytes());
        assert_eq!(a, b, "whitespace alone split one operation into two");
        assert_ne!(a, c, "two different commands share an identity");
    }

    #[test]
    fn unknown_is_the_default_and_must_not_repeat_silently() {
        assert!(ReplayClass::Unknown.must_not_repeat_silently());
        assert!(ReplayClass::Unsafe.must_not_repeat_silently());
        assert!(ReplayClass::Conditional.must_not_repeat_silently());
        assert!(!ReplayClass::Safe.must_not_repeat_silently());
    }
}
