//! C4.4 / P1 Packet 1 — the synthetic kill-9 preparation demo, pinned by the
//! same code path it ships.
//!
//! `thruline demo --recovery` and this test call `recovery_demo::walk`. A demo
//! that drifted from its test would be a demo of nothing.

use thruline_core::recovery_demo;

#[test]
fn the_synthetic_walk_prepares_but_cannot_verify_recovery() {
    let dir = tempfile::tempdir().expect("tempdir");
    let r = recovery_demo::walk(dir.path()).expect("the walk runs");

    // The crash was real: a live child, killed, gone.
    assert!(r.killed_pid > 0, "no child was ever started");
    assert!(r.child_died, "the child survived SIGKILL");

    // And it presents as a crash CANDIDATE — the doctor's vocabulary, not a
    // verdict: the marker outlived the process and no close was recorded.
    assert!(
        r.crash_candidate,
        "the killed session did not present as a crash candidate"
    );

    // The read-only snapshot described the in-flight work: one modified file
    // and one untracked file existed at kill time. This is not a restorable
    // content checkpoint.
    assert!(
        r.snapshot_file_entries >= 2,
        "the uncommitted file entries were not described: {} file(s)",
        r.snapshot_file_entries
    );

    // The capsule carries recorded context, and rescue staged it without
    // launching or interrogating a successor.
    assert!(
        r.capsule_sections_with_lines >= 4,
        "the capsule is too empty to hand anyone: {} section(s)",
        r.capsule_sections_with_lines
    );
    assert_eq!(r.staged.len(), 2, "both capsule views must be staged");
    for path in &r.staged {
        assert!(
            std::path::Path::new(path).exists(),
            "a staged artifact is missing: {path}"
        );
    }

    // The handshake ran, and it says so in coverage rather than a score.
    assert!(
        r.handshake_coverage.starts_with("Continuity: "),
        "{}",
        r.handshake_coverage
    );
    assert!(!r.handshake_coverage.contains('%'));

    // Local checks can pass, but no observed successor means recovery cannot.
    assert_eq!(
        r.handshake_outcome, "insufficient",
        "the synthetic walk manufactured recovery authority: {}",
        r.handshake_coverage
    );
    assert_eq!(
        r.handshake_coverage, "Continuity: 6 of 7 checks · 1 insufficient evidence",
        "the missing successor evidence is not visible"
    );
    assert!(
        r.handshake_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("successor_evidence_missing")),
        "the synthetic walk did not name why recovery is unproved"
    );

    // The local-only result remains auditable but is excluded from P1.
    let (verified, total) = r.verified_continuation_rate;
    assert_eq!(
        (verified, total),
        (0, 0),
        "a synthetic local check entered the P1 recovery metric"
    );
    assert_eq!(
        r.legacy_local_continuity_rate,
        (0, 1),
        "the local-only row was not retained for audit"
    );
}

#[test]
fn the_demo_kills_only_its_own_child() {
    // C4.5's guardrail at its sharpest point. The one destructive act in this
    // repository lives in the demo harness, and it is bounded BY TYPE: the
    // kill path takes a `Child` this process spawned. No pid may arrive from a
    // row, a payload, or a user.
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/recovery_demo.rs"),
    )
    .expect("read the demo module");

    // Exactly one kill call, and it is a method on our own child handle.
    let kills = src.matches(".kill()").count();
    assert_eq!(kills, 1, "the demo has {kills} kill calls; it may have one");
    assert!(
        src.contains("fn kill_own_child(child: &mut Child)"),
        "the kill path no longer takes a Child handle by type"
    );

    // No route to killing anything else: no signal-sending command, no pid
    // plumbed in from outside.
    for forbidden in [
        "Command::new(\"kill\")",
        "Command::new(\"pkill\")",
        "Command::new(\"killall\")",
        "libc::kill",
        "signal::kill",
        "from_raw_pid",
    ] {
        assert!(
            !src.contains(forbidden),
            "the demo can reach a process it did not spawn, via `{forbidden}`"
        );
    }

    // Ruling D requires a bounded trusted-compiler query before fetch. Founder
    // ruling R-2026-09-28-7 (OFFLINE-KILL-GUARD) admits its cleanup as this
    // census's ONE carve-out, and only on this condition: the kill goes through
    // the same guarded signaller as SNAP-PGID-GUARD —
    // `bounded_child::signal_group`, which refuses pgid <= 1 and HQ's own group
    // before any syscall — reached through `bounded_child::kill_owned_group`,
    // which takes the spawned `Child` by type. cargo_fetch.rs itself may hold
    // no direct kill of any kind. Every other module keeps the original blanket
    // prohibition.
    let core = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let guard = std::fs::read_to_string(core.join("bounded_child.rs")).expect("read bounded_child");
    let guard = guard
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .expect("bounded_child production source");
    let door = guard
        .split("pub(crate) fn kill_owned_group(child: &mut Child)")
        .nth(1)
        .expect("bounded_child exposes kill_owned_group taking a spawned Child by type");
    let door = door.split("\n}\n").next().expect("kill_owned_group body");
    assert!(
        door.contains("signal_group("),
        "kill_owned_group does not go through the guarded signal_group"
    );
    assert!(
        !door.contains(".kill()") && !door.contains("kill_process_group("),
        "kill_owned_group signals around the guard"
    );
    assert!(
        guard.contains("if group <= 1 {")
            && guard.contains("if group == rustix::process::getpgrp().as_raw_nonzero().get() {"),
        "signal_group no longer refuses pgid <= 1 and HQ's own group"
    );
    for entry in std::fs::read_dir(&core).expect("walk core") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "recovery_demo.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        if name == "cargo_fetch.rs" {
            let production = text
                .split("\n#[cfg(test)]")
                .next()
                .expect("production source");
            for direct in [
                ".kill()",
                "Child::kill",
                "kill_process_group",
                "rustix::process::kill",
                "Command::new(\"kill\")",
                "Command::new(\"pkill\")",
                "Command::new(\"killall\")",
                "Command::new(\"/bin/kill\")",
                "libc::kill",
                "signal::kill",
                "from_raw_pid",
                "Pid::from_raw",
            ] {
                assert!(
                    !production.contains(direct),
                    "cargo_fetch.rs holds a direct kill via `{direct}`; its one kill must go through bounded_child's guarded signaller"
                );
            }
            assert!(
                production.contains("fn kill_owned_probe(child: &mut Child)"),
                "compiler cleanup must take its spawned Child by type"
            );
            assert_eq!(
                production
                    .matches("crate::bounded_child::kill_owned_group(child)")
                    .count(),
                1,
                "compiler cleanup must reach the guarded signaller exactly once, from kill_owned_probe"
            );
            assert_eq!(
                production.matches("kill_owned_probe(&mut child)").count(),
                2,
                "only failed-wait and expired-deadline paths may request compiler cleanup"
            );
            continue;
        }
        assert!(
            !text.contains(".kill()"),
            "{name} can kill a process — only the demo harness may, and only its own child"
        );
    }
}

#[test]
fn the_walk_is_reproducible_and_touches_no_real_ledger() {
    // Two runs in two directories: same shape, and neither reaches ~/.thruline.
    let a = tempfile::tempdir().expect("a");
    let b = tempfile::tempdir().expect("b");
    let ra = recovery_demo::walk(a.path()).expect("a");
    let rb = recovery_demo::walk(b.path()).expect("b");
    assert_eq!(ra.project_id, rb.project_id);
    assert_eq!(ra.crash_candidate, rb.crash_candidate);
    assert_eq!(
        ra.snapshot_file_entries, rb.snapshot_file_entries,
        "the walk is not reproducible"
    );
    assert_eq!(ra.verified_continuation_rate, rb.verified_continuation_rate);
    assert_eq!(
        ra.legacy_local_continuity_rate,
        rb.legacy_local_continuity_rate
    );
    assert_eq!(ra.handshake_reason, rb.handshake_reason);
    assert_ne!(ra.killed_pid, rb.killed_pid, "two runs, two real children");
    // Each run's ledger lives in its own directory.
    assert!(a.path().join("ledger.db").exists());
    assert!(b.path().join("ledger.db").exists());
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/recovery_demo.rs"),
    )
    .expect("read");
    assert!(
        !src.contains(".thruline"),
        "the demo reaches for the user's real ledger"
    );
}
