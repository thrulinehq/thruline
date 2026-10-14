//! The product fact `spec_git_env.rs` deliberately isolates itself from: a
//! content-transform filter in the machine's own git configuration IS part of
//! the declared git environment and IS reported. That file runs without the
//! ambient configuration so it stays about the repository; this one, in its
//! own test binary so the environment it sets cannot reach those tests, points
//! the global configuration at a file holding a filter and asserts it shows.

use std::process::Command;

use thruline_core::spec::git_env::git_env;

#[test]
fn a_content_filter_in_the_global_git_configuration_is_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let global = dir.path().join("global-gitconfig");
    std::fs::write(&global, "[filter \"x\"]\n\tclean = cat\n").expect("write config");
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).expect("repo dir");
    let init = Command::new("git")
        .current_dir(&repo)
        .args(["init", "-q", "-b", "main"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git init");
    assert!(init.success());

    // This binary has one test, so setting the process environment cannot race
    // another test's git process.
    std::env::set_var("GIT_CONFIG_GLOBAL", &global);
    std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    let env = git_env(&repo).expect("env");
    assert!(
        env.has_content_transform(),
        "a filter.x.clean entry in the global configuration was not reported: {:?}",
        env.filters
    );
}
