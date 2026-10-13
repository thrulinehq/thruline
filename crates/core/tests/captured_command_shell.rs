//! The interpreter a captured command runs under must exist on the machines
//! this product is installed on, and must not read anything that is not in the
//! record.
//!
//! Track C measured the defect in `debian:bookworm-slim`, installing from the
//! published `curl | sh` path:
//!
//! ```text
//! $ thruline demo --continuity
//! error: running the demo: i/o error at /bin/zsh: No such file or directory
//! ```
//!
//! §15 makes `thruline demo` the first thing a new user is told to run, so on
//! Linux the first recommended command did not run. It survived because it is
//! unreachable from a developer's Mac — only
//! `packaging/acceptance-fresh-machine.sh` exercises the install path on a
//! machine that has never seen ThruLine, and that script cannot run here.
//!
//! **So this test is a stand-in, and says so.** It cannot prove ThruLine works
//! on Linux; it proves the one property whose absence broke it, on a machine
//! that cannot reproduce the break. `execute_shell` is private, so the fact is
//! read out of the source — the same method the redaction and detector-parity
//! censuses use, and for the same reason: the alternative is a test that
//! restates the constant it guards.

/// The only interpreter path POSIX requires to exist. Not a preference list:
/// `/bin/bash` is absent from Alpine, `/bin/zsh` from Debian, and `$SHELL` is
/// a property of whoever ran the command rather than of the run.
const POSIX_SHELL: &str = "/bin/sh";

fn execute_shell_body() -> String {
    let src = include_str!("../src/capture.rs");
    let start = src
        .find("fn execute_shell(")
        .expect("capture.rs no longer defines `execute_shell` — this test is reading nothing");
    let rest = &src[start..];
    // To the end of the function: the first line that is a lone closing brace
    // at column 0.
    let end = rest
        .find("\n}\n")
        .expect("`execute_shell` has no closing brace at column 0");
    rest[..end].to_string()
}

#[test]
fn a_captured_command_runs_under_the_one_shell_posix_guarantees() {
    let body = execute_shell_body();

    assert!(
        body.contains(&format!("Command::new(\"{POSIX_SHELL}\")")),
        "captured commands do not run under {POSIX_SHELL}. A shell that is not \
         on the machine is an install that fails at the first command §15 tells \
         a new user to run.\n\n{body}"
    );

    // Named individually rather than as "anything else", so the failure says
    // which one came back.
    for absent in ["/bin/zsh", "/bin/bash", "/usr/bin/zsh", "/usr/bin/bash"] {
        assert!(
            !body.contains(absent),
            "`execute_shell` names {absent}, which no stock Linux image is \
             required to have.\n\n{body}"
        );
    }
    assert!(
        !body.contains("SHELL"),
        "`execute_shell` reads the environment's shell. What a command MEANS \
         must be the same on the machine that captured it and the machine that \
         verifies it; a shell from the environment makes `shell_profile` a \
         property of whoever ran it.\n\n{body}"
    );
}

#[test]
fn it_is_not_a_login_shell_so_no_unrecorded_file_can_change_what_a_command_means() {
    let body = execute_shell_body();
    assert!(
        body.contains(".arg(\"-c\")"),
        "`execute_shell` does not pass `-c`.\n\n{body}"
    );
    // `-lc`, `-l`, `--login`: any of them sources dotfiles that are not in the
    // record and cannot be re-executed with it. That is the failure a receipt
    // exists to prevent, one layer down.
    for login in ["-lc", "\"-l\"", "--login"] {
        assert!(
            !body.contains(login),
            "`execute_shell` starts a LOGIN shell ({login}), which sources \
             files that are not in the record.\n\n{body}"
        );
    }
}

#[test]
fn the_interpreter_the_source_names_exists_on_this_machine() {
    // The half a source census cannot give: the string is only worth anything
    // if something is there. This would also catch a future "portable" choice
    // that is portable in name only.
    let named = POSIX_SHELL;
    assert!(
        std::path::Path::new(named).exists(),
        "{named} does not exist here, so nothing captured on this machine can \
         run"
    );
}

#[test]
fn the_reader_this_test_depends_on_actually_reads_something() {
    // A census that extracts an empty region passes forever. This is the
    // program's most-repeated defect and it gets an assertion rather than a
    // comment.
    let body = execute_shell_body();
    assert!(
        body.len() > 80 && body.contains("Command::new"),
        "the extracted `execute_shell` body is {} bytes and holds no command — \
         the reader is broken, not the code:\n{body}",
        body.len()
    );
}
