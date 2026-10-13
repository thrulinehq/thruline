//! The temp roots this crate's tests create are **gone afterwards**, and this
//! asserts that they are gone rather than that a removal was called.
//!
//! # What it is guarding
//!
//! Three sites built directories under the system temp dir by name with no
//! owner, so nothing removed them. Measured when Track C found it (DECISIONS
//! #158): **1,997 left behind** on the development machine — 1,583 `tl-c2-*`,
//! 234 `tl-holdout-*`, 180 `tl-acct-*` — about 2,000 per suite run, growing
//! with every gate.
//!
//! Nothing failed because of it. That is why it survived a day of gates: a leak
//! costs the run that causes it nothing at all, and is paid by whoever runs the
//! machine next.
//!
//! # Why the assertions are about the filesystem
//!
//! "`remove_dir_all` returned `Ok`" is a claim about a call. "The directory is
//! not there" is a claim about the world, and only the second is what a leak
//! guard is for. Every case below re-`stat`s the path.
//!
//! **The scope is stated in each failure message**, because this suite cannot
//! see the whole class: it proves the guard removes ITS root, not that no test
//! anywhere builds a directory by hand. A future site that does will leak
//! exactly as these three did, and this file will stay green — which is a limit
//! worth writing down rather than one worth implying.

mod common;

use common::TempRoot;

#[test]
fn a_temp_root_is_gone_after_it_drops() {
    let path = {
        let root = TempRoot::new("guard-drop");
        assert!(
            root.path().is_dir(),
            "the root was not created, so its removal proves nothing"
        );
        std::fs::write(root.join("evidence.txt"), b"content").expect("write");
        root.path().to_path_buf()
    };
    assert!(
        !path.exists(),
        "the temp root {} still exists after its guard dropped. SCOPE: this \
         checks one root created by TempRoot; a test that builds a directory by \
         hand is not covered here and will leak silently, which is how the \
         1,997 directories this file exists for accumulated.",
        path.display()
    );
}

#[test]
fn a_temp_root_is_gone_even_when_the_test_panics() {
    // THE CASE THAT MATTERS. A cleanup written at the end of a test body is
    // deleted by the first failing assertion, and failing runs are the ones
    // that leak most. `Drop` runs while unwinding; a trailing `remove_dir_all`
    // does not.
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
    let sink = std::sync::Arc::clone(&captured);

    let result = std::panic::catch_unwind(move || {
        let root = TempRoot::new("guard-panic");
        *sink.lock().expect("lock") = Some(root.path().to_path_buf());
        panic!("a test failing partway, which is the run that used to leak");
    });
    assert!(result.is_err(), "the closure was supposed to panic");

    let path = captured
        .lock()
        .expect("lock")
        .clone()
        .expect("the root path was recorded before the panic");
    assert!(
        !path.exists(),
        "the temp root {} survived a panic. A cleanup that only runs on the \
         happy path leaks on exactly the runs that fail. SCOPE: one root, \
         created by TempRoot.",
        path.display()
    );
}

#[test]
fn two_roots_never_collide() {
    // The old `fresh_dir` carried a per-call counter for this reason; the guard
    // has to keep it, or two tests in one process share a ledger and the
    // failure looks like a bug in the code under test.
    let a = TempRoot::new("guard-unique");
    let b = TempRoot::new("guard-unique");
    assert_ne!(
        a.path(),
        b.path(),
        "two roots with the same tag collided in one process. Tests would then \
         share a holdout ledger, and a spend by one would be seen as a prior \
         run by the other."
    );
}

/// **The suite leaves nothing of its own behind**, checked over the temp
/// directory rather than over a counter this file keeps.
///
/// It looks only for the tags this file creates. It deliberately does NOT sweep
/// `tl-*`: other suites run concurrently under the same temp directory, and a
/// test that fails because a sibling is mid-run would be a flake dressed as a
/// leak check — the shape this repository spent a day removing.
#[test]
fn this_suites_own_roots_do_not_survive_it() {
    let tmp = std::env::temp_dir();
    let mine = format!("tl-guard-collect-{}", std::process::id());

    let path = {
        let root = TempRoot::new("guard-collect");
        root.path().to_path_buf()
    };
    assert!(
        !path.exists(),
        "the root {} outlived its guard",
        path.display()
    );

    let strays: Vec<String> = std::fs::read_dir(&tmp)
        .expect("temp dir is readable")
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.starts_with(&mine))
        .collect();
    assert!(
        strays.is_empty(),
        "this process left {} directory/directories under {}: {}. SCOPE: only \
         names beginning {mine} are swept, because sibling suites share this \
         temp directory and sweeping `tl-*` would fail on their live roots.",
        strays.len(),
        tmp.display(),
        strays.join(", ")
    );
}

// ---------------------------------------------------------------------------
// THE CENSUS. Every temp root in this crate has an owner, checked in the SOURCE
// rather than on the filesystem.
// ---------------------------------------------------------------------------

/// Call sites allowed to reach `std::env::temp_dir()` directly, each with the
/// owner that removes what it creates.
///
/// **A site absent from this list fails, and that is the point.** Everything
/// above this test measures the filesystem: it can only see a leak that has
/// already happened, under a name someone thought to sweep. That is how 758
/// `thruline-drive-*` roots survived a `tl-*` sweep run minutes earlier — the
/// leaking code was written after the sweep's search space was decided.
///
/// Track C reached the same conclusion from the other side, with a registry
/// that records each root at creation so a prefix nobody thought to sweep is
/// covered by construction (DECISIONS #171). A registry and a guard object both
/// cover the sites that *use* them and neither covers a site that bypasses them
/// — C's own probe bypassed its registry, exactly as `drive.rs` bypassed this
/// guard. This census is the part that catches the bypass, and it catches it
/// when the code is written rather than when the directory count is noticed.
const TEMP_DIR_CALLERS: &[(&str, &str)] = &[
    (
        "tests/common/mod.rs",
        "TempRoot itself — the owner every other site is required to use",
    ),
    (
        "tests/temp_roots.rs",
        "this file: it reads the temp directory to check for survivors, and          creates roots only through TempRoot",
    ),
    (
        "tests/drive.rs",
        "Fixture, which owns its root through Drop. It did NOT, and leaked 758          directories in a day; the entry stays so the exemption is visible          rather than implied",
    ),
];

#[test]
fn every_temp_dir_call_site_has_a_named_owner() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }

    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    walk(&crate_root.join("src"), &mut files);
    walk(&crate_root.join("tests"), &mut files);
    assert!(
        !files.is_empty(),
        "the walk found no .rs files under {}. A census with an empty search          space passes forever.",
        crate_root.display()
    );

    let mut unowned = Vec::new();
    let mut seen_owned = 0usize;
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        // Comments are excluded: this file and common/mod.rs both DESCRIBE the
        // call in prose, and a census that counts its own explanation is the
        // wrong-search-space failure this crate has already committed twice.
        let code_has_call = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .any(|l| l.contains("env::temp_dir()"));
        if !code_has_call {
            continue;
        }
        let rel = f
            .strip_prefix(crate_root)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        match TEMP_DIR_CALLERS.iter().find(|(p, _)| rel.ends_with(p)) {
            Some(_) => seen_owned += 1,
            None => unowned.push(rel),
        }
    }

    assert!(
        unowned.is_empty(),
        "these files call std::env::temp_dir() and are not in TEMP_DIR_CALLERS:\n           {}\n\n         Build the root through `common::TempRoot`, which removes it on Drop          including while unwinding. If the site genuinely cannot, add it to          TEMP_DIR_CALLERS **with the owner that removes what it creates** — the          entry is the claim, and an entry saying `it is fine` is not one.\n\n         This is checked in the source because the filesystem checks above          cannot see a leak under a name nobody swept, which is how 758 roots          survived a sweep run minutes earlier.",
        unowned.join("\n  ")
    );

    assert_eq!(
        seen_owned,
        TEMP_DIR_CALLERS.len(),
        "TEMP_DIR_CALLERS lists {} sites and {seen_owned} were found in the          source. A registered site that no longer calls temp_dir() is a stale          exemption, and an exemption list that outlives its reasons is how the          next site gets waved through.",
        TEMP_DIR_CALLERS.len()
    );
}

/// Scripts in Track E's lane that create a temp root, each with its owner.
///
/// **The census above walks `src/` and `tests/` and never saw `scripts/`.** That
/// is the blind spot it was built to catch, in itself: a search space fixed when
/// the instrument was written, excluding a whole class of creators in the same
/// lane. `scripts/pilot-run-guard-test.sh` has been creating a root on every
/// gate all day, and neither the filesystem sweeps nor the Rust census could see
/// it — the coordinator's census pass named it from outside.
///
/// Only Track E's own scripts are listed (`docs/tracks/E/INTERFACE-REQUESTS.md`
/// carries the ruled lane). Scripts belonging to other tracks are theirs to
/// census; flagging them from here would be an exception list about somebody
/// else's code.
const SCRIPT_TEMP_OWNERS: &[(&str, &str)] = &[
    (
        "pilot-run.sh",
        "BUILD_CTX, removed by an EXIT trap. It was a trailing `rm -rf` after          `docker build`, so under `set -e` a failed build leaked it",
    ),
    (
        "pilot-run-guard-test.sh",
        "WORK, removed by an EXIT trap since it was written",
    ),
    (
        "compose-instrument-test.py",
        "its own tempfile handling, in Track E's lane and unchanged here",
    ),
    (
        "classify-workspace-test.py",
        "registered with atexit AT CREATION. It was an rmtree at the bottom of          the file, which an exception above it skips",
    ),
    (
        "pilot-report-coverage-test.py",
        "its own tempfile handling, in Track E's lane and unchanged here",
    ),
];

#[test]
fn every_script_temp_root_in_this_lane_has_a_named_owner() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/bench -> repo root");
    let scripts = repo.join("scripts");

    // The ruled lane, from DECISIONS #94 and ruling 6. A file outside it is
    // another track's to census.
    const LANE: &[&str] = &[
        "pilot-run.sh",
        "pilot-run-guard-test.sh",
        "compose-instrument.py",
        "compose-instrument-test.py",
        "classify-workspace.py",
        "classify-workspace-test.py",
        "pilot-report.py",
        "pilot-report-coverage-test.py",
    ];

    let mut unowned = Vec::new();
    let mut found = 0usize;
    for name in LANE {
        let Ok(text) = std::fs::read_to_string(scripts.join(name)) else {
            continue;
        };
        let creates = text.lines().any(|l| {
            let t = l.trim_start();
            !t.starts_with('#')
                && (t.contains("mktemp")
                    || t.contains("mkdtemp")
                    || t.contains("TemporaryDirectory"))
        });
        if !creates {
            continue;
        }
        found += 1;
        if !SCRIPT_TEMP_OWNERS.iter().any(|(n, _)| n == name) {
            unowned.push((*name).to_string());
        }
    }

    assert!(
        !LANE.is_empty() && found > 0,
        "no script in the lane was found to create a temp root. Either the lane          list is wrong or the scripts moved, and a census with an empty search          space passes forever — which is the defect this file exists for."
    );
    assert!(
        unowned.is_empty(),
        "these scripts in Track E's lane create a temp root and are not in          SCRIPT_TEMP_OWNERS:\n  {}\n\n         Give the root an owner that survives the failure paths — an EXIT trap          in shell, `atexit` registered AT CREATION in Python — and add the entry          naming it. A cleanup written at the end of the file is skipped by          whatever fails above it, which is the shape that leaked here twice.\n\n         STATED LIMIT: none of these survive SIGKILL, and no in-process owner          does. Roots orphaned that way are the machine's to reap, not this          test's to prevent.",
        unowned.join("\n  ")
    );
    assert_eq!(
        found,
        SCRIPT_TEMP_OWNERS.len(),
        "SCRIPT_TEMP_OWNERS lists {} scripts and {found} in the lane create a          temp root. A listed script that no longer creates one is a stale          exemption.",
        SCRIPT_TEMP_OWNERS.len()
    );
}
