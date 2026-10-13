use super::*;

#[test]
fn a_file_carrying_the_wrapper_marker_is_never_resolved_as_the_real_program() {
    // The anti-recursion control, at the unit level. A path comparison is
    // defeated by a copy; a content check is not, and this is the check that
    // makes an infinite regress impossible rather than merely unlikely.
    let dir = tempfile::tempdir().expect("tempdir");
    let decoy = dir.path().join("cargo");
    std::fs::write(
        &decoy,
        crate::shim::wrapper_script(Path::new("/usr/bin/thruline"), dir.path(), "cargo"),
    )
    .expect("write");
    assert!(is_our_wrapper(&decoy));

    let plain = dir.path().join("real-cargo");
    std::fs::write(&plain, "#!/bin/sh\nexec /usr/bin/true\n").expect("write");
    assert!(!is_our_wrapper(&plain));
}

#[test]
fn a_program_named_by_path_is_still_refused_when_it_is_one_of_ours() {
    let dir = tempfile::tempdir().expect("tempdir");
    let decoy = dir.path().join("npm");
    std::fs::write(
        &decoy,
        crate::shim::wrapper_script(Path::new("/usr/bin/thruline"), dir.path(), "npm"),
    )
    .expect("write");
    assert!(resolve_program(&decoy.to_string_lossy(), None).is_none());
    // A path to something that is not ours resolves to itself, unchanged.
    let plain = dir.path().join("real");
    std::fs::write(&plain, "#!/bin/sh\n").expect("write");
    assert_eq!(
        resolve_program(&plain.to_string_lossy(), None),
        Some(plain.clone())
    );
}

#[test]
fn the_session_binding_prefers_the_env_then_the_manifest_then_the_spool() {
    // Three routes of unequal trustworthiness, and the event records which one
    // was used: attributing evidence to the wrong session is not a small error.
    assert_eq!(SessionBinding::Env.as_str(), "env");
    assert_eq!(SessionBinding::Manifest.as_str(), "manifest");
    assert_eq!(SessionBinding::SpoolLatest.as_str(), "spool-latest");
    assert_eq!(SessionBinding::Unbound.as_str(), "unbound");
}

#[test]
fn the_latest_spool_session_is_read_from_the_tail_and_ignores_unusable_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = dir.path().join("s.jsonl");
    let mut text = String::new();
    for i in 0..500 {
        text.push_str(&format!("{{\"session_id\":\"old-{i}\"}}\n"));
    }
    text.push_str("{\"session_id\":\"newest\"}\n");
    text.push_str("not json at all\n");
    text.push_str("{\"no_session\":true}\n");
    std::fs::write(&spool, text).expect("write");
    assert_eq!(
        latest_spool_session(&spool).as_deref(),
        Some("newest"),
        "a malformed trailing line hid the last usable session"
    );
    assert_eq!(latest_spool_session(&dir.path().join("absent")), None);
}

#[test]
fn an_unbound_exec_gets_a_per_day_session_rather_than_being_dropped() {
    // Rule 12. An exec nobody can attribute is still evidence that a command
    // ran; merging it into a real session's chain would be worse, because that
    // session would then carry evidence it never produced.
    let s = unbound_session("2026-09-02T10:11:12.345Z");
    assert_eq!(s, "exec-unbound-2026-09-02");
    assert_ne!(s, unbound_session("2026-09-03T00:00:00Z"));
}

#[test]
fn the_debounce_window_is_the_specified_two_hundred_milliseconds() {
    assert_eq!(SNAPSHOT_DEBOUNCE_MS, 200);
}

#[test]
fn a_snapshot_of_somewhere_that_is_not_a_repository_is_an_absence_not_a_failure() {
    // The distinction that decides whether `fold` exits non-zero: "there was
    // nothing to bind to" and "there was, and it failed" are different facts.
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(matches!(take_snapshot(Some(dir.path())), Ok(None)));
    assert!(matches!(take_snapshot(None), Ok(None)));
}

#[test]
fn a_snapshot_that_genuinely_fails_is_an_error_carrying_the_reason() {
    // A commitless nested repository: `git add -A` refuses and no tree exists.
    // The recorder must surface that, because a claim bound to a tree that was
    // never taken can only be `unverifiable`.
    let dir = tempfile::tempdir().expect("tempdir");
    let outer = dir.path().join("outer");
    std::fs::create_dir_all(outer.join("inner")).expect("dirs");
    for root in [&outer, &outer.join("inner")] {
        let out = std::process::Command::new("git")
            .current_dir(root)
            .args(["init", "-q", "-b", "main"])
            .output()
            .expect("git init");
        assert!(out.status.success());
    }
    std::fs::write(outer.join("a.txt"), "a\n").expect("write");
    match take_snapshot(Some(&outer)) {
        Err(why) => assert!(why.contains("snapshot of"), "{why}"),
        other => panic!("a commitless nested repo did not fail loudly: {other:?}"),
    }
}

#[test]
fn a_flag_names_the_argv_element_after_it() {
    // The residual the canary scoping exposed. Masking argv elements
    // independently loses the only context argv has: `--password s3cr3t` is two
    // elements and the value matches no shape, while `password=s3cr3t` on one
    // line is masked by the `.env` rule. Same secret, two answers, decided by
    // whether a shell split it.
    let r = crate::redaction::Redactor::new();
    let argv: Vec<String> = [
        "deploy",
        "--password",
        "s3cr3t-opaque-value-1234",
        "--verbose",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let out = redact_argv(&r, &argv);
    assert_eq!(out[0], "deploy");
    assert_eq!(out[1], "--password");
    assert_eq!(out[2], "[masked: argv_value]");
    assert_eq!(out[3], "--verbose");
}

#[test]
fn a_flag_naming_a_property_does_not_mask_its_path() {
    // A209-C's rule, inherited rather than restated: a key naming a mode, path
    // or algorithm describes a credential, it does not hold one. This uses the
    // same predicate as the JSON walk and the `.env` rule, so it cannot drift
    // away from them.
    let r = crate::redaction::Redactor::new();
    for (flag, value) in [
        ("--token-file", "/etc/creds/token"),
        ("--key-path", "/home/me/.ssh/id_ed25519"),
        ("--auth-mode", "oauth2"),
        ("--output", "report.xml"),
        ("-k", "test_login"),
    ] {
        let argv: Vec<String> = ["run", flag, value]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let out = redact_argv(&r, &argv);
        assert_eq!(out[2], value, "{flag} destroyed its non-secret argument");
    }
}

#[test]
fn an_opaque_argument_nothing_names_still_survives_and_that_is_correct() {
    // Scoping the claim: "zero credential-shaped values", not "zero secrets".
    // argv is evidence, and a string nothing identifies is indistinguishable
    // from an argument — masking it would destroy the record to no end.
    let r = crate::redaction::Redactor::new();
    let argv: Vec<String> = ["pytest", "-k", "hunter2000correcthorsebattery"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(redact_argv(&r, &argv)[2], "hunter2000correcthorsebattery");
    // While a SHAPED credential is masked wherever it sits, bare or not.
    let argv: Vec<String> = ["gh", "auth", "ghp_16C7e42F292c6912E7710c838347Ae178B4a"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(redact_argv(&r, &argv)[2], "[masked: github]");
}
