//! R17b-2a″ item 5 — the mutation regime, at the gate and at the binding.
//!
//! Every case here is a way a tool identity could be wrong. All of them must
//! ABSTAIN. None may produce a verdict, and none may fall through to a
//! different reader than the one registered.

use thruline_core::capture::{
    raw_tool_name, registered_command, registration, RegistrationStatus, SourceAssuranceMeasurement,
};

fn measurement(source: &str, matcher: &str) -> SourceAssuranceMeasurement {
    SourceAssuranceMeasurement {
        source: source.to_string(),
        source_version: "mutation-v1".to_string(),
        config_sha256: "c".to_string(),
        matcher: matcher.to_string(),
        pre_blocks: true,
        rewrite_exact: true,
        stable_tool_identity: true,
        completion_pairing: true,
        measured_pre_delay_ms: 400,
        child_started_after_pre: true,
        wrapper_exit_code: 0,
        source_exit_code: 0,
    }
}

/// NAMESPACE CONFUSION, the lie-dead case: a CANONICAL value supplied where the
/// RAW name is expected.
///
/// `Bash` is what ThruLine calls a codex shell tool after canonicalisation. It
/// is never what codex called it. An integration recording it as its matcher is
/// describing ThruLine's vocabulary, not its own interception target — so the
/// gate must refuse, even though every other field is honest.
#[test]
fn capital_bash_for_codex_abstains() {
    assert!(
        !measurement("codex", "Bash").enables_assured(),
        "the canonical name must never satisfy a raw-keyed gate"
    );
    assert!(
        registration("codex", "Bash").is_none(),
        "and it is absent from the registry, not merely unmatched"
    );
}

/// TRUTHFUL BUT UNREGISTERED: a real vendor name nobody has registered.
///
/// `bash` is a plausible raw codex tool name and canonicalises to `Bash`, so
/// before raw-name preservation it was indistinguishable from `exec_command`.
/// It abstains because it is UNREGISTERED — not because it failed, and the
/// distinction is the point.
#[test]
fn lowercase_bash_abstains_because_it_is_unregistered() {
    assert!(!measurement("codex", "bash").enables_assured());
    assert!(
        registration("codex", "bash").is_none(),
        "unregistered means absent: neither a Registered row nor an Excluded one"
    );
    // The abstention is about registration, and the registry can say so: an
    // EXCLUDED tool carries a reason, an unregistered one carries no row at all.
    assert!(
        registration("codex", "exec").is_some_and(
            |r| r.status == RegistrationStatus::Excluded && r.reason.contains("row-receipt")
        ),
        "an excluded tool names why; an unregistered one is simply not there"
    );
}

#[test]
fn a_registered_pair_still_passes_the_gate() {
    assert!(
        measurement("codex", "exec_command").enables_assured(),
        "the regime must not be vacuous — a truthful registered matcher passes"
    );
}

/// REGISTRATION IS NOT ELIGIBILITY, and rekeying the gate must not have blurred
/// them.
///
/// `(claude_code, Bash)` is registered — its command is readable, which is what
/// let the legacy chain be deleted against byte-identical live reads. But
/// Claude Code remains barred from ASSURED strength by the standing M5 vendor
/// gate, a separate NO-GO that predates this work. A registry that quietly
/// granted assurance by registering a tool would have overturned that ruling as
/// a side effect.
#[test]
fn claude_code_is_registered_but_still_never_assured() {
    assert!(
        registration("claude_code", "Bash")
            .is_some_and(|r| r.status == RegistrationStatus::Registered),
        "its command must be readable"
    );
    assert_eq!(
        registered_command("claude_code", "Bash", r#"{"command":"cargo test"}"#).as_deref(),
        Some("cargo test")
    );
    assert!(
        !measurement("claude_code", "Bash").enables_assured(),
        "and it must STILL be barred from assured strength — registration reads a \
         command, it does not grant assurance"
    );
}

/// An EXCLUDED tool is refused by the gate exactly like an unregistered one,
/// but for a recorded reason.
#[test]
fn an_excluded_tool_abstains_at_the_gate() {
    for raw in ["exec", "apply_patch", "write_stdin"] {
        assert!(
            !measurement("codex", raw).enables_assured(),
            "{raw} is excluded and must not satisfy the gate"
        );
    }
}

/// BOTH FIELDS DIFFER: the registered path is read, and only it.
#[test]
fn both_fields_present_reads_only_the_registered_path() {
    let both = r#"{"cmd":"registered-path","command":"other-path"}"#;
    assert_eq!(
        registered_command("codex", "exec_command", both).as_deref(),
        Some("registered-path")
    );
    assert_eq!(
        registered_command("codex", "shell_command", both).as_deref(),
        Some("other-path")
    );
    assert_eq!(
        registered_command("claude_code", "Bash", both).as_deref(),
        Some("other-path")
    );
}

/// ABSENT FIELD: abstain, never fall through to the other key.
#[test]
fn an_absent_registered_field_abstains_without_fallthrough() {
    assert_eq!(
        registered_command("codex", "exec_command", r#"{"command":"present"}"#),
        None,
        "exec_command reads `cmd`; finding `command` instead must not rescue it"
    );
    assert_eq!(
        registered_command("codex", "shell_command", r#"{"cmd":"present"}"#),
        None
    );
    assert_eq!(registered_command("codex", "exec_command", "null"), None);
    assert_eq!(registered_command("codex", "exec_command", "{}"), None);
}

/// The accessor is what makes all of the above expressible.
#[test]
fn the_accessor_separates_the_two_namespaces() {
    // A preserved raw name wins over the canonical one.
    assert_eq!(raw_tool_name("Bash", Some("exec_command")), "exec_command");
    // Absent preservation means raw and canonical agree.
    assert_eq!(raw_tool_name("exec", None), "exec");
    // And a canonical Bash with no preservation is NOT exec_command — which is
    // why a claude_code Bash and a codex exec_command cannot be confused.
    assert_eq!(raw_tool_name("Bash", None), "Bash");
    assert!(registered_command("codex", raw_tool_name("Bash", None), r#"{"cmd":"x"}"#).is_none());
}
