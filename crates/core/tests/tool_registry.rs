//! R17b-2a″ item 3 — the raw-keyed registry.
//!
//! Every seed is justified by a stored row (A112), not by a rollout count —
//! which is the mistake A108 made and A111 corrected.

use thruline_core::capture::{
    registered_command, registration, CommandExtractor, RegistrationStatus, TOOL_REGISTRY,
};

#[test]
fn the_seeds_are_registered_with_their_measured_extractors() {
    for (vendor, raw, extractor) in [
        ("codex", "exec_command", CommandExtractor::TopLevelCmd),
        ("codex", "shell_command", CommandExtractor::TopLevelCommand),
        ("claude_code", "Bash", CommandExtractor::TopLevelCommand),
    ] {
        let r = registration(vendor, raw).unwrap_or_else(|| panic!("{vendor}/{raw} unregistered"));
        assert_eq!(r.status, RegistrationStatus::Registered);
        assert_eq!(r.extractor, Some(extractor), "{vendor}/{raw}");
    }
}

#[test]
fn the_exclusions_are_rows_with_row_receipts() {
    for raw in ["exec", "apply_patch", "write_stdin"] {
        let r = registration("codex", raw).unwrap_or_else(|| panic!("{raw} must be a ROW"));
        assert_eq!(
            r.status,
            RegistrationStatus::Excluded,
            "{raw} is excluded, not absent — 'we looked and it cannot work' is a \
             different fact from 'nobody looked'"
        );
        assert!(r.extractor.is_none());
        assert!(
            r.reason.contains("row-receipt"),
            "an exclusion must cite the stored row that justifies it: {}",
            r.reason
        );
    }
}

/// The three that are deliberately, and now *expressibly*, unregistered.
///
/// Before raw-name preservation these were indistinguishable from
/// `exec_command` in storage, so leaving them out could not have been a choice.
#[test]
fn bash_shell_and_terminal_are_visibly_unregistered() {
    for raw in ["bash", "shell", "terminal"] {
        assert!(
            registration("codex", raw).is_none(),
            "{raw} must be absent — neither registered nor excluded, because it has \
             not been measured in a real corpus"
        );
        assert_eq!(
            registered_command("codex", raw, r#"{"command":"echo hi"}"#),
            None,
            "an unregistered tool reads nothing even when its shape would parse"
        );
    }
}

#[test]
fn the_registry_is_vendor_scoped() {
    // The same raw name under a different vendor is a different decision.
    assert!(registration("claude_code", "Bash").is_some());
    assert!(
        registration("codex", "Bash").is_none(),
        "codex never emits a raw `Bash`; registering one would be the namespace \
         confusion the mutation regime tests for"
    );
    assert!(registration("claude_code", "exec_command").is_none());
}

#[test]
fn each_extractor_reads_only_its_own_key() {
    assert_eq!(
        registered_command("codex", "exec_command", r#"{"cmd":"a","command":"b"}"#).as_deref(),
        Some("a"),
        "the registered path is `cmd`; the presence of `command` must not change it"
    );
    assert_eq!(
        registered_command("codex", "shell_command", r#"{"cmd":"a","command":"b"}"#).as_deref(),
        Some("b"),
        "the registered path is `command`; the presence of `cmd` must not change it"
    );
}

#[test]
fn an_absent_field_abstains_without_falling_through() {
    assert_eq!(
        registered_command(
            "codex",
            "exec_command",
            r#"{"command":"only-the-other-key"}"#
        ),
        None,
        "exec_command reads `cmd` and nothing else — no fallthrough to `command`"
    );
    assert_eq!(
        registered_command("codex", "shell_command", r#"{"cmd":"only-the-other-key"}"#),
        None,
        "shell_command reads `command` and nothing else"
    );
}

#[test]
fn every_row_is_internally_consistent() {
    for r in TOOL_REGISTRY {
        match r.status {
            RegistrationStatus::Registered => {
                assert!(
                    r.extractor.is_some(),
                    "{}/{} registered without an extractor",
                    r.vendor,
                    r.raw_tool
                );
                assert!(r.class.is_some());
            }
            RegistrationStatus::Excluded => {
                assert!(
                    r.extractor.is_none(),
                    "{}/{} excluded but carries an extractor",
                    r.vendor,
                    r.raw_tool
                );
            }
        }
        assert!(
            !r.reason.is_empty(),
            "{}/{} has no reason",
            r.vendor,
            r.raw_tool
        );
    }
}
