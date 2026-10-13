//! Authored native JSON fixtures through the existing parser and bridge reader.
#[path = "fixtures/decision_conflict/mod.rs"]
mod fixture;
#[path = "fixtures/decision_revival_native/mod.rs"]
mod native;
#[path = "support/person_action.rs"]
mod person_action;
use fixture::{commit, git};
use native::*;
use serde_json::json;
use thruline_core::decisions::{engine, revival::RevivalState};

#[test]
fn unique_partial_edit_uses_parent_blob_and_exact_current_line() {
    let f = setup(
        Some(BEFORE),
        AFTER,
        "Edit",
        |p| edit(BEFORE, "ACTIVE", "OBSOLETE", false, p),
        |_| {},
    );
    let r = report(&f);
    assert_eq!(r.revivals.len(), 1);
    assert_eq!(r.revivals[0].state, RevivalState::ReAdded);
    assert_eq!(r.revivals[0].line, 2);
    assert_eq!(r.revivals[0].witnesses.len(), 4);
}
#[test]
fn replace_all_attributes_each_exact_replaced_occurrence() {
    let before = "const ACTIVE: bool = false;\nconst ACTIVE: bool = true;\n";
    let after = before.replace("ACTIVE", "OBSOLETE");
    let f = setup(
        Some(before),
        &after,
        "Edit",
        |p| edit(before, "ACTIVE", "OBSOLETE", true, p),
        |_| {},
    );
    let r = report(&f);
    assert_eq!(r.revivals.len(), 2);
    assert!(r.revivals.iter().all(|r| r.state == RevivalState::ReAdded));
}
#[test]
fn ambiguous_nonreplace_all_substring_stays_unverifiable() {
    let before = "const ACTIVE: bool = false;\nconst ACTIVE: bool = true;\n";
    let after = before.replacen("ACTIVE", "OBSOLETE", 1);
    let f = setup(
        Some(before),
        &after,
        "Edit",
        |p| edit(before, "ACTIVE", "OBSOLETE", false, p),
        |_| {},
    );
    assert!(report(&f)
        .revivals
        .iter()
        .all(|r| r.state == RevivalState::Unverifiable));
}
#[test]
fn replace_all_requires_the_complete_expected_post_blob() {
    let before = "const ACTIVE: bool = false;\nconst ACTIVE: bool = true;\n";
    let after = before.replacen("ACTIVE", "OBSOLETE", 1);
    let f = setup(
        Some(before),
        &after,
        "Edit",
        |p| edit(before, "ACTIVE", "OBSOLETE", true, p),
        |_| {},
    );
    assert!(report(&f)
        .revivals
        .iter()
        .all(|r| r.state == RevivalState::Unverifiable));
}
#[test]
fn partial_edit_rejects_wrong_original_or_change_echo() {
    for field in ["originalFile", "oldString", "newString", "filePath"] {
        let f = setup(
            Some(BEFORE),
            AFTER,
            "Edit",
            |p| edit(BEFORE, "ACTIVE", "OBSOLETE", false, p),
            |rows| rows[1]["toolUseResult"][field] = json!("unrelated bytes"),
        );
        assert!(report(&f)
            .revivals
            .iter()
            .all(|r| r.state == RevivalState::Unverifiable));
    }
}
#[test]
fn full_write_update_proves_exact_content_and_native_type() {
    let f = setup(
        Some(BEFORE),
        AFTER,
        "Write",
        |p| write(AFTER, "update", p),
        |_| {},
    );
    assert_eq!(report(&f).revivals[0].state, RevivalState::ReAdded);
}
#[test]
fn full_write_create_independently_proves_parent_path_absence() {
    let f = setup(None, AFTER, "Write", |p| write(AFTER, "create", p), |_| {});
    assert_eq!(report(&f).revivals[0].state, RevivalState::ReAdded);
}
#[test]
fn write_type_must_match_exact_parent_path_presence() {
    for (before, kind) in [(Some(BEFORE), "create"), (None, "update")] {
        let f = setup(before, AFTER, "Write", |p| write(AFTER, kind, p), |_| {});
        assert!(report(&f)
            .revivals
            .iter()
            .all(|r| r.state == RevivalState::Unverifiable));
    }
}
#[test]
fn write_rejects_wrong_content_path_and_unrecognized_type_echo() {
    for field in ["content", "filePath", "type"] {
        let f = setup(
            Some(BEFORE),
            AFTER,
            "Write",
            |p| write(AFTER, "update", p),
            |rows| rows[1]["toolUseResult"][field] = json!("unrelated bytes"),
        );
        assert!(report(&f)
            .revivals
            .iter()
            .all(|r| r.state == RevivalState::Unverifiable));
    }
}
#[test]
fn weak_native_write_echo_names_missing_required_bytes() {
    for field in ["content", "filePath", "type"] {
        let f = setup(
            Some(BEFORE),
            AFTER,
            "Write",
            |p| write(AFTER, "update", p),
            |rows| {
                rows[1]["toolUseResult"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
            },
        );
        let r = report(&f);
        assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
        assert!(
            r.revivals[0].reason.as_deref().unwrap().contains("Write"),
            "the missing native echo requirement must be named"
        );
    }
}
#[test]
fn native_changes_without_successful_matching_result_stay_unverifiable() {
    for tool in ["Edit", "Write"] {
        for missing in [false, true] {
            let f = setup(
                Some(BEFORE),
                AFTER,
                tool,
                |p| {
                    if tool == "Edit" {
                        edit(BEFORE, "ACTIVE", "OBSOLETE", false, p)
                    } else {
                        write(AFTER, "update", p)
                    }
                },
                |rows| {
                    if missing {
                        rows.remove(1);
                    } else {
                        rows[1]["message"]["content"][0]["is_error"] = json!(true);
                    }
                },
            );
            assert!(report(&f)
                .revivals
                .iter()
                .all(|r| r.state == RevivalState::Unverifiable));
        }
    }
}
#[test]
fn native_change_never_binds_unrelated_stdout_or_later_commit() {
    let f = setup(
        Some(BEFORE),
        AFTER,
        "Write",
        |p| write(AFTER, "update", p),
        |rows| rows[3]["toolUseResult"]["stdout"] = json!("not an exact object id"),
    );
    assert_eq!(report(&f).revivals[0].state, RevivalState::Unverifiable);
    let f = setup(
        Some(BEFORE),
        AFTER,
        "Write",
        |p| write(AFTER, "update", p),
        |rows| {
            let path = std::path::Path::new(
                rows[0]["message"]["content"][0]["input"]["file_path"]
                    .as_str()
                    .unwrap(),
            );
            let root = path.parent().unwrap();
            std::fs::write(root.join("unrelated.rs"), "const UNRELATED: bool = true;\n").unwrap();
            let unrelated = commit(root);
            rows[3]["toolUseResult"]["stdout"] = json!(unrelated);
            rows[3]["message"]["content"][0]["content"] = json!(unrelated);
        },
    );
    assert_eq!(report(&f).revivals[0].state, RevivalState::Unverifiable);
}
#[test]
fn two_independent_native_sources_cannot_choose_an_introducer() {
    let mut f = setup(
        Some(BEFORE),
        AFTER,
        "Write",
        |p| write(AFTER, "update", p),
        |_| {},
    );
    let rows = std::fs::read_to_string(f.root().join(".git/agent.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    capture(&mut f, "other-native", rows);
    let r = report(&f);
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
    assert!(r.revivals[0]
        .reason
        .as_deref()
        .unwrap()
        .contains("more than one"));
}
#[test]
fn exact_introducing_blob_survives_an_unchanged_later_target() {
    let mut f = setup(
        Some(BEFORE),
        AFTER,
        "Edit",
        |p| edit(BEFORE, "ACTIVE", "OBSOLETE", false, p),
        |_| {},
    );
    let introduction = f.version.clone();
    std::fs::write(
        f.root().join("unrelated.rs"),
        "const UNRELATED: bool = true;\n",
    )
    .unwrap();
    f.version = commit(f.root());
    engine::check_project_version(
        &f.ledger,
        "p",
        f.root(),
        &f.version,
        "2026-10-08T00:04:00Z",
        false,
    )
    .unwrap();
    let r = report(&f);
    assert_eq!(r.revivals[0].state, RevivalState::ReAdded);
    assert_eq!(
        r.revivals[0].introducing_version.as_deref(),
        Some(introduction.as_str())
    );
    assert_ne!(
        r.revivals[0].introducing_version.as_deref(),
        Some(f.version.as_str())
    );
    assert!(git(f.root(), &["rev-parse", "HEAD"]).len() == 40);
}

#[test]
fn overlapping_partial_substring_is_ambiguous_even_with_one_nonoverlapping_match() {
    let before = "// aaa\nconst ACTIVE: bool = false;\n";
    let after = before.replacen("aa", "obsolete ", 1);
    let f = setup(
        Some(before),
        &after,
        "Edit",
        |p| edit(before, "aa", "obsolete ", false, p),
        |_| {},
    );
    assert_eq!(report(&f).revivals[0].state, RevivalState::Unverifiable);
}

#[cfg(unix)]
#[test]
fn native_write_create_cannot_treat_a_parent_symlink_as_absence() {
    let f = setup_with_parent(
        None,
        AFTER,
        "Write",
        |p| write(AFTER, "create", p),
        |_| {},
        |f| {
            std::os::unix::fs::symlink("retained.rs", f.root().join("app.rs")).unwrap();
        },
    );
    let r = report(&f);
    assert_eq!(r.revivals[0].path, "app.rs");
    assert!(
        r.revivals[0].held_version.is_some(),
        "this rejection must exercise the parent-entry guard"
    );
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
}
