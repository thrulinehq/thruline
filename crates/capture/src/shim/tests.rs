use super::*;

/// Options for the tests that are about **wrapper bytes and integrity**, not
/// about the predicate: [`WrapPolicy::All`], so what gets written does not
/// depend on which runners happen to be installed on the machine running the
/// suite. The predicate has its own tests below, and they name their own PATH.
fn opts(dir: &Path, bin: &Path) -> InstallOptions {
    let mut o = InstallOptions::under(
        dir.parent().unwrap_or(dir),
        bin.to_path_buf(),
        "2026-09-02T00:00:00Z".to_string(),
    );
    o.dir = dir.to_path_buf();
    o.policy = WrapPolicy::All;
    o
}

/// Options under the shipped default ([`WrapPolicy::Resolvable`]) with an
/// explicit `PATH`.
///
/// The PATH is passed rather than set, deliberately: `std::env::set_var` is
/// global to the whole test binary, and other tests in it spawn `sh` and `git`
/// by name. A predicate test that rewrote `PATH` would make those flaky, which
/// is a worse instrument than the one this file is fixing.
fn resolvable_opts(dir: &Path, bin: &Path, path: &OsStr) -> InstallOptions {
    let mut o = InstallOptions::under(
        dir.parent().unwrap_or(dir),
        bin.to_path_buf(),
        "2026-09-02T00:00:00Z".to_string(),
    );
    o.dir = dir.to_path_buf();
    o.install_path = Some(path.to_os_string());
    o
}

/// A directory of real, executable, do-nothing programs, and the PATH that
/// reaches it. They carry no wrapper marker, so the resolver accepts them.
fn fakebin(root: &Path, names: &[&str]) -> (PathBuf, OsString) {
    let bin = root.join("fakebin");
    std::fs::create_dir_all(&bin).expect("fakebin");
    for name in names {
        let path = bin.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write fake");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
    }
    let path = OsString::from(bin.to_string_lossy().to_string());
    (bin, path)
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

/// `command -v <name>`, asked of a real `sh` with a named PATH — the same
/// question `run-tests.sh` asks in the corpus.
fn command_v(name: &str, path: &OsStr) -> std::process::Output {
    // Absolute: `sh` itself must not be looked up in the PATH under test.
    std::process::Command::new("/bin/sh")
        .args(["-c", &format!("command -v {name}")])
        .env("PATH", path)
        .output()
        .expect("sh")
}

#[test]
fn a_wrapper_is_a_pure_function_of_its_three_inputs() {
    // The whole integrity story rests on this: `doctor` calls a wrapper
    // "modified" by comparing a hash, so identical inputs must give identical
    // bytes or every install would look tampered with.
    let a = wrapper_script(Path::new("/usr/bin/thruline"), Path::new("/s"), "cargo");
    let b = wrapper_script(Path::new("/usr/bin/thruline"), Path::new("/s"), "cargo");
    assert_eq!(a, b);
    assert_ne!(
        a,
        wrapper_script(Path::new("/usr/bin/thruline"), Path::new("/s"), "npm")
    );
    assert_ne!(
        a,
        wrapper_script(Path::new("/opt/thruline"), Path::new("/s"), "cargo")
    );
}

#[test]
fn a_path_with_a_quote_in_it_is_quoted_and_not_escaped_out_of() {
    // A directory named `it's` would otherwise close the shell string and turn
    // the rest of the script into commands.
    let script = wrapper_script(
        Path::new("/home/it's/bin/thruline"),
        Path::new("/home/it's/shim"),
        "cargo",
    );
    assert!(script.contains(r"'/home/it'\''s/bin/thruline'"), "{script}");
    // And `sh -n` agrees it parses.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("w");
    std::fs::write(&path, &script).expect("write");
    let out = std::process::Command::new("sh")
        .arg("-n")
        .arg(&path)
        .output()
        .expect("sh -n");
    assert!(
        out.status.success(),
        "the generated wrapper is not valid sh: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn every_generated_wrapper_parses_as_posix_sh() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let manifest = install(&opts(&shim, Path::new("/usr/bin/thruline"))).expect("install");
    assert_eq!(manifest.wrappers.len(), WRAPPED.len());
    for program in WRAPPED {
        let out = std::process::Command::new("sh")
            .arg("-n")
            .arg(shim.join(program))
            .output()
            .expect("sh -n");
        assert!(
            out.status.success(),
            "{program} wrapper is not valid sh: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn a_path_containing_a_newline_is_refused_rather_than_written() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("line\nbreak");
    let err = install(&opts(&dir.path().join("shim"), &bad)).expect_err("must refuse");
    assert!(matches!(err, ShimError::Unquotable(_)), "{err}");
}

#[test]
fn reinstalling_over_our_own_directory_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let first = install(&opts(&shim, Path::new("/usr/bin/thruline"))).expect("install");
    let second = install(&opts(&shim, Path::new("/usr/bin/thruline"))).expect("reinstall");
    assert_eq!(first.wrappers, second.wrappers);
    assert!(verify(&shim, Some("/nope")).modified.is_empty());
}

#[test]
fn verify_separates_modified_from_missing_from_unexpected() {
    // Three different failures with three different remedies. Collapsing them
    // into "not ok" would leave a user with nothing to do.
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    install(&opts(&shim, Path::new("/usr/bin/thruline"))).expect("install");
    std::fs::write(shim.join("cargo"), "#!/bin/sh\n").expect("tamper");
    std::fs::remove_file(shim.join("npm")).expect("remove");
    std::fs::write(shim.join("stranger"), "#!/bin/sh\n").expect("add");

    let v = verify(&shim, Some(&shim.to_string_lossy()));
    assert_eq!(v.modified, vec!["cargo".to_string()]);
    assert_eq!(v.missing, vec!["npm".to_string()]);
    assert_eq!(v.unexpected, vec!["stranger".to_string()]);
    assert!(v.on_path);
    assert!(!v.ok());
}

#[test]
fn an_uninstalled_directory_is_not_a_problem_it_is_an_absence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let v = verify(&dir.path().join("never-installed"), Some("/usr/bin"));
    assert!(!v.installed);
    assert!(v.ok(), "a missing shim was reported as a fault");
}

#[test]
fn uninstall_leaves_a_foreign_file_and_removes_nothing_else() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    install(&opts(&shim, Path::new("/usr/bin/thruline"))).expect("install");
    std::fs::write(shim.join("theirs"), "x").expect("write");
    let removed = uninstall(&shim).expect("uninstall");
    assert_eq!(removed.wrappers.len(), WRAPPED.len());
    assert_eq!(removed.left_alone, vec!["theirs".to_string()]);
    assert!(!removed.dir_removed);
    assert!(shim.join("theirs").is_file());
    assert!(!shim.join(MANIFEST).exists());
}

#[test]
fn a_directory_holding_someone_else_s_files_is_refused_with_zero_writes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mine = dir.path().join("bin");
    std::fs::create_dir_all(&mine).expect("dir");
    std::fs::write(mine.join("my-script"), "x").expect("write");
    let err = install(&opts(&mine, Path::new("/usr/bin/thruline"))).expect_err("must refuse");
    assert!(matches!(err, ShimError::Occupied { .. }), "{err}");
    assert!(!mine.join("cargo").exists(), "a wrapper was written anyway");
    assert!(!mine.join(MANIFEST).exists());
}

#[test]
fn a_shim_behind_a_directory_that_also_provides_the_program_is_shadowed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let earlier = dir.path().join("earlier");
    std::fs::create_dir_all(&earlier).expect("dir");
    std::fs::write(earlier.join("go"), "x").expect("write");
    install(&opts(&shim, Path::new("/usr/bin/thruline"))).expect("install");

    let path = format!("{}:{}", earlier.display(), shim.display());
    let v = verify(&shim, Some(&path));
    assert!(v.on_path);
    assert_eq!(v.shadowed_by.len(), 1);
    assert!(v.shadowed_by[0].ends_with("/go"));
    assert!(!v.ok(), "an unreachable shim reported as intact");
}

// --- The amended predicate (operator ruling 1) -----------------------------

#[test]
fn only_a_name_that_actually_resolves_gets_a_wrapper() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (_bin, path) = fakebin(dir.path(), &["make"]);

    let m = install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &path,
    ))
    .expect("install");

    assert_eq!(
        m.wrappers.keys().cloned().collect::<Vec<_>>(),
        vec!["make".to_string()],
        "a wrapper was written for a program that does not exist"
    );
    assert!(shim.join("make").is_file());
    // Every other candidate is named, with a reason. "Not written" is a fact
    // about the capture, and an unexplained absence is the thing Rule 12 is
    // about.
    assert_eq!(m.skipped.len(), WRAPPED.len() - 1);
    assert_eq!(m.candidates.len(), WRAPPED.len());
    for name in WRAPPED.iter().filter(|n| **n != "make") {
        assert!(
            m.skipped
                .get(*name)
                .is_some_and(|why| why.contains("resolves")),
            "{name} was skipped without saying why: {:?}",
            m.skipped.get(*name)
        );
        assert!(
            !shim.join(name).exists(),
            "{name} has no real program and yet a wrapper for it is on PATH"
        );
    }
    // The basis of every one of those skips, recorded rather than re-derived.
    assert_eq!(m.install_path.as_deref(), Some(&*path.to_string_lossy()));
    assert_eq!(m.wrap_policy, WrapPolicy::Resolvable);
}

#[test]
fn command_v_pytest_is_absent_when_no_pytest_is_installed() {
    // The lean4-proof mechanism, directly. `run-tests.sh`:
    //
    //     command -v pytest || uv pip install pytest
    //
    // The shim answered yes for a pytest that did not exist, the install was
    // skipped, and the grader died at exit 127 — with the oracle's own work
    // already verified complete. A correct solution scored as a failure by the
    // instrument measuring it.
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (_bin, fake) = fakebin(dir.path(), &["make"]);
    let path = OsString::from(format!("{}:{}", shim.display(), fake.to_string_lossy()));

    install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("install");
    let out = command_v("pytest", &path);
    assert!(
        !out.status.success() && out.stdout.is_empty(),
        "the shim answered `command -v pytest` for a pytest that does not exist: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    // …and `make`, which does exist, is still reached through the shim, so the
    // fix is not "capture less".
    let out = command_v("make", &path);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        shim.join("make").to_string_lossy(),
        "the wrapper for a program that exists was not reached"
    );

    // The failure direction: the same install under the old policy answers yes.
    let other = tempfile::tempdir().expect("tempdir");
    let shim2 = other.path().join("shim");
    let mut all = resolvable_opts(&shim2, Path::new("/usr/bin/thruline"), &fake);
    all.policy = WrapPolicy::All;
    install(&all).expect("install");
    let path2 = OsString::from(format!("{}:{}", shim2.display(), fake.to_string_lossy()));
    let out = command_v("pytest", &path2);
    assert!(
        out.status.success(),
        "this test cannot detect the regression it exists for"
    );
}

#[test]
fn a_planted_wrapper_for_a_declined_name_is_forged_not_merely_unexpected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (_bin, fake) = fakebin(dir.path(), &["make"]);
    install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("install");

    // Someone puts back the wrapper the predicate declined to write — the exact
    // file whose absence stops `command -v pytest` lying.
    std::fs::write(
        shim.join("pytest"),
        wrapper_script(Path::new("/usr/bin/thruline"), &shim, "pytest"),
    )
    .expect("plant");

    let v = verify(&shim, Some(&shim.to_string_lossy()));
    assert!(
        v.forged.contains(&"pytest".to_string()),
        "a planted wrapper was not seen: {v:?}"
    );
    assert!(!v.ok(), "a forged wrapper on PATH passed as intact");

    // It is in `unexpected` too — which is where it went BEFORE this field
    // existed, and `unexpected` is not read by `ok()`. Stated as an assertion
    // rather than as a comment, because that is the hole:
    assert!(v.unexpected.contains(&"pytest".to_string()));
    let unexpected_only = ShimIntegrity {
        installed: true,
        on_path: true,
        unexpected: vec!["pytest".to_string()],
        ..ShimIntegrity::default()
    };
    assert!(
        unexpected_only.ok(),
        "`unexpected` alone still has to pass — if it did not, this test would \
         prove nothing about `forged`"
    );

    // A file that is neither a candidate nor wrapper-shaped stays a stray file.
    std::fs::write(shim.join("notes.txt"), "mine\n").expect("write");
    let v = verify(&shim, Some(&shim.to_string_lossy()));
    assert!(v.unexpected.contains(&"notes.txt".to_string()));
    assert!(!v.forged.contains(&"notes.txt".to_string()));
}

#[test]
fn a_stale_wrapper_is_removed_when_the_program_it_wrapped_disappears() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (bin, fake) = fakebin(dir.path(), &["make", "pytest"]);
    let m = install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("install");
    assert!(m.wrappers.contains_key("pytest") && shim.join("pytest").is_file());

    // pytest is uninstalled from the machine; the shim is reinstalled.
    std::fs::remove_file(bin.join("pytest")).expect("remove");
    let m = install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("reinstall");

    assert!(!m.wrappers.contains_key("pytest"));
    assert!(
        !shim.join("pytest").exists(),
        "the wrapper survived the reinstall, so it is on PATH and absent from \
         the manifest — a forgery of our own making"
    );
    assert!(verify(&shim, Some(&shim.to_string_lossy()))
        .forged
        .is_empty());
}

#[cfg(unix)]
#[test]
fn a_removal_that_cannot_be_done_is_an_error_and_not_a_shrug() {
    // `let _ = remove_file(..)` here would leave a wrapper on PATH that the
    // manifest does not list. `verify` files that under `unexpected`, and
    // `ok()` does not read `unexpected` — so the forgery would pass green.
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (bin, fake) = fakebin(dir.path(), &["make", "pytest"]);
    install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("install");

    std::fs::remove_file(bin.join("pytest")).expect("remove");
    chmod(&shim, 0o555);
    let err = install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect_err("a removal it could not do was reported as success");
    chmod(&shim, 0o755);

    assert!(matches!(err, ShimError::Io { .. }), "{err}");
    assert!(err.to_string().contains("pytest"), "{err}");
}

#[cfg(unix)]
#[test]
fn a_read_only_shim_directory_makes_install_fail_rather_than_half_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    std::fs::create_dir_all(&shim).expect("dir");
    let (_bin, fake) = fakebin(dir.path(), &["make"]);
    let mut o = resolvable_opts(&shim, Path::new("/usr/bin/thruline"), &fake);
    o.programs = vec!["make".to_string()];
    chmod(&shim, 0o555);

    let err = install(&o).expect_err("install into an unwritable directory reported success");
    chmod(&shim, 0o755);

    assert!(matches!(err, ShimError::Io { .. }), "{err}");
    assert!(
        !shim.join(MANIFEST).exists(),
        "a manifest was written for wrappers that are not there"
    );
}

#[test]
fn a_program_that_vanishes_after_install_stops_resolving_for_the_recorder() {
    // The wrapper stays (install ran once, before the agent started), so the
    // next invocation reaches `exec::record`, which asks THIS question and
    // exits 127 when the answer is no. The predicate and the recorder share one
    // body, so this is the same call the recorder makes.
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (bin, fake) = fakebin(dir.path(), &["make"]);
    install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("install");
    assert!(resolves_on("make", &shim, Some(&fake)));

    std::fs::remove_file(bin.join("make")).expect("remove");
    assert!(
        !resolves_on("make", &shim, Some(&fake)),
        "the recorder would have run the wrapper as its own child"
    );
    // And it does not resolve to OUR wrapper even though that file is a real,
    // executable `make` sitting on the PATH under test.
    let with_shim = OsString::from(format!("{}:{}", shim.display(), fake.to_string_lossy()));
    assert!(!resolves_on("make", &shim, Some(&with_shim)));
    assert_eq!(
        crate::exec::resolve_program_on("make", Some(&shim), Some(&with_shim)),
        None
    );
}

#[test]
fn a_program_installed_after_the_shim_is_reported_uncovered_and_is_not_a_fault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (bin, fake) = fakebin(dir.path(), &["make"]);
    install(&resolvable_opts(
        &shim,
        Path::new("/usr/bin/thruline"),
        &fake,
    ))
    .expect("install");

    // The agent installs pytest mid-session. Nothing wraps it, and every test
    // it runs is invisible — reported, because an invisible test run is how a
    // claim reaches `supported` on no evidence.
    std::fs::write(bin.join("pytest"), "#!/bin/sh\nexit 0\n").expect("write");
    #[cfg(unix)]
    chmod(&bin.join("pytest"), 0o755);

    let path = format!("{}:{}", shim.display(), fake.to_string_lossy());
    let v = verify(&shim, Some(&path));
    assert!(v.uncovered.contains(&"pytest".to_string()), "{v:?}");
    assert!(
        v.ok(),
        "a tool installed after the shim was is a reinstall, not a compromise"
    );
}

#[test]
fn only_wraps_exactly_what_it_was_asked_for() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shim = dir.path().join("shim");
    let (_bin, fake) = fakebin(dir.path(), &["make", "pytest"]);
    let mut o = resolvable_opts(&shim, Path::new("/usr/bin/thruline"), &fake);
    o.policy = WrapPolicy::Only(vec!["pytest".to_string()]);
    let m = install(&o).expect("install");
    assert_eq!(
        m.wrappers.keys().cloned().collect::<Vec<_>>(),
        vec!["pytest".to_string()]
    );
    assert!(m
        .skipped
        .get("make")
        .is_some_and(|w| w.contains("requested")));
}
