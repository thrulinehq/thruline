use super::*;
use crate::parse::Runner;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}

fn cmds(script: &str) -> Vec<Vec<String>> {
    split_commands(script)
        .unwrap_or_else(|e| panic!("{script:?}: {e}"))
        .into_iter()
        .map(|c| c.argv)
        .collect()
}

#[test]
fn the_plan_s_own_example_splits_into_two_commands() {
    // BUILD-PLAN M0.8 done-when (b), stated as a parse rather than as a match.
    assert_eq!(
        cmds("pytest -q && echo ok"),
        vec![argv(&["pytest", "-q"]), argv(&["echo", "ok"])]
    );
}

#[test]
fn every_separator_ends_a_command() {
    for (script, expected) in [
        ("a; b", 2),
        ("a && b", 2),
        ("a || b", 2),
        ("a | b", 2),
        ("a & b", 2),
        ("a\nb", 2),
        ("a && b || c ; d | e", 5),
    ] {
        assert_eq!(cmds(script).len(), expected, "{script:?}");
    }
}

#[test]
fn quotes_hold_words_together_and_do_not_leak_their_delimiters() {
    assert_eq!(
        cmds(r#"pytest -k "test_a or test_b" -q"#),
        vec![argv(&["pytest", "-k", "test_a or test_b", "-q"])]
    );
    assert_eq!(
        cmds("pytest -k 'a && b'"),
        vec![argv(&["pytest", "-k", "a && b"])],
        "an operator inside quotes split a command"
    );
    assert_eq!(cmds(r#"echo "a\"b""#), vec![argv(&["echo", "a\"b"])]);
    assert_eq!(cmds(r"echo a\ b"), vec![argv(&["echo", "a b"])]);
}

#[test]
fn a_redirection_is_not_an_argument() {
    // `["pytest","-q",">","out.txt"]` would be a false argv, and argv is what
    // `argv_hash` commits to.
    let parsed = split_commands("pytest -q > out.txt 2>&1").expect("parse");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].argv, argv(&["pytest", "-q"]));
    assert!(parsed[0].redirected, "the redirection was not recorded");
}

#[test]
fn an_environment_prefix_is_not_the_program() {
    // `PYTHONPATH=. pytest` runs pytest, not a program called `PYTHONPATH=.`.
    let parsed = split_commands("PYTHONPATH=. CI=1 pytest -q").expect("parse");
    assert_eq!(parsed[0].argv, argv(&["pytest", "-q"]));
    assert_eq!(parsed[0].env_prefix, argv(&["PYTHONPATH=.", "CI=1"]));
    assert_eq!(
        Runner::from_argv(&parsed[0].argv),
        Some(Runner::Pytest),
        "the runner was hidden behind an env prefix"
    );
}

#[test]
fn everything_that_expands_at_run_time_is_refused_rather_than_guessed() {
    // Each of these has a value this module cannot compute. Returning a
    // best-effort argv would be inventing evidence; the shim records the real
    // exec regardless, so refusing costs a label and nothing else.
    for script in [
        "$(echo pytest) -q",
        "`echo pytest` -q",
        "$RUNNER -q",
        "pytest test_*.py",
        "(cd sub && pytest)",
        "pytest \"$FLAGS\"",
    ] {
        assert!(
            split_commands(script).is_err(),
            "{script:?} was parsed instead of refused"
        );
    }
}

#[test]
fn an_unterminated_quote_is_an_error_not_a_truncated_word() {
    assert!(matches!(
        split_commands("pytest -k 'unterminated"),
        Err(ShellParseError::Unterminated { .. })
    ));
    assert!(matches!(
        split_commands(r#"pytest -k "unterminated"#),
        Err(ShellParseError::Unterminated { .. })
    ));
}

#[test]
fn a_shell_invocation_is_recognised_including_flag_clusters() {
    for words in [
        vec!["sh", "-c", "pytest -q"],
        vec!["bash", "-c", "pytest -q"],
        vec!["/bin/bash", "-lc", "pytest -q"],
        vec!["zsh", "-ec", "pytest -q"],
        vec!["bash", "--command", "pytest -q"],
    ] {
        match shell_payload(&argv(&words)) {
            ShellPayload::Parsed(c) => {
                assert_eq!(c[0].argv, argv(&["pytest", "-q"]), "{words:?}")
            }
            other => panic!("{words:?} → {other:?}"),
        }
    }
}

#[test]
fn npx_call_carries_a_shell_payload_and_npx_is_a_wrapped_runner() {
    // The reachable non-shell case: `npx` is on §4.4's wrap list, and `-c`
    // hands it a script. Without this the invocation records as "npx, no
    // runner" while a test suite ran inside it.
    match shell_payload(&argv(&["npx", "-c", "pytest -q"])) {
        ShellPayload::Parsed(c) => assert_eq!(c[0].argv, argv(&["pytest", "-q"])),
        other => panic!("{other:?}"),
    }
    match shell_payload(&argv(&["npm", "exec", "--call", "cargo test"])) {
        ShellPayload::Parsed(c) => assert_eq!(c[0].argv, argv(&["cargo", "test"])),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plain_command_is_not_a_shell_payload() {
    assert_eq!(
        shell_payload(&argv(&["pytest", "-q"])),
        ShellPayload::NotShell
    );
    assert_eq!(
        shell_payload(&argv(&["bash", "script.sh"])),
        ShellPayload::NotShell,
        "a script FILE is not an inline payload; its contents were never read"
    );
    assert_eq!(
        shell_payload(&argv(&["sh", "--login"])),
        ShellPayload::NotShell
    );
    // `-c` with nothing after it carries no script.
    assert_eq!(shell_payload(&argv(&["sh", "-c"])), ShellPayload::NotShell);
    // And the deliberate agreement with a real shell: `sh -config x` sets the
    // flags c,o,n,f,i,g and runs `x`. Treating that as "not a `-c`" would be a
    // hole an agent could drive a test run through.
    assert_eq!(
        shell_payload(&argv(&["sh", "-config", "pytest -q"])),
        ShellPayload::Parsed(vec![Command {
            argv: argv(&["pytest", "-q"]),
            redirected: false,
            env_prefix: Vec::new(),
        }])
    );
}

#[test]
fn an_unparseable_payload_is_reported_as_unparsed_never_as_empty() {
    // The distinction a caller must be able to make: "no runner in this
    // script" is evidence, "this script could not be read" is not.
    match shell_payload(&argv(&["sh", "-c", "$RUNNER -q"])) {
        ShellPayload::Unparsed { payload, reason } => {
            assert_eq!(payload, "$RUNNER -q");
            assert!(reason.contains("expands at run time"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    // The control: an empty parse is a *different* answer.
    assert_eq!(
        shell_payload(&argv(&["sh", "-c", ""])),
        ShellPayload::Parsed(vec![])
    );
}

#[test]
fn a_comment_is_not_a_command() {
    assert_eq!(
        cmds("pytest -q\n# echo pytest passed\n"),
        vec![argv(&["pytest", "-q"])]
    );
}

#[test]
fn the_substring_matcher_this_replaces_is_wrong_in_both_directions() {
    // The reason this module exists, stated as a test rather than a comment.
    let lying = "echo 'pytest: all tests pass'";
    assert!(
        lying.contains("pytest"),
        "the premise of the false positive"
    );
    let commands = split_commands(lying).expect("parse");
    assert_eq!(commands.len(), 1);
    assert_eq!(
        Runner::from_argv(&commands[0].argv),
        None,
        "an echo was recognised as a test run"
    );

    let hidden = "py.test -q";
    assert!(
        !hidden.contains("pytest"),
        "the premise of the false negative"
    );
    let commands = split_commands(hidden).expect("parse");
    assert_eq!(Runner::from_argv(&commands[0].argv), Some(Runner::Pytest));
}

#[test]
fn a_here_document_is_refused_rather_than_read_as_commands() {
    // The terminator and every body line were being parsed as commands. A body
    // containing `pytest -q` would then produce a command that never ran —
    // invented evidence, and the exact thing this module refuses to do.
    let script = "cat <<EOF\npytest -q\nEOF\n";
    assert!(
        matches!(
            split_commands(script),
            Err(ShellParseError::Unexpandable {
                construct: "a here-document",
                ..
            })
        ),
        "{:?}",
        split_commands(script)
    );
    for variant in [
        "cat <<-EOF\nx\nEOF\n",
        "cat <<'EOF'\nx\nEOF\n",
        "a && cat <<EOF\nx\nEOF\n",
    ] {
        assert!(split_commands(variant).is_err(), "{variant:?}");
    }
    // A single `<` is still an ordinary redirection whose target is consumed.
    let parsed = split_commands("pytest -q < input.txt").expect("parse");
    assert_eq!(parsed[0].argv, argv(&["pytest", "-q"]));
    assert!(parsed[0].redirected);
}
