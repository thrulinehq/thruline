//! Commands: what was run, how it ended, and which runs were tests.
//!
//! ## How a command ended
//!
//! * `succeeded` — the record holds the command's result and the result is not
//!   marked an error (`is_error = 0`; for Claude Code `Bash` that is exit 0).
//! * `failed` — the result is marked an error (a non-zero exit, a refused or
//!   interrupted call).
//! * `unknown` — the record holds no result for the call, **or** the command
//!   ran inside a Codex code-mode `exec` program (see [`super::codemode`]) and
//!   the program ran more than that one command, or its output states neither
//!   the command's exit code nor (for a test run) a runner summary
//!   ([`super::tally`]: any failure or error → `failed`, else ≥ 1 passed →
//!   `succeeded`). A program's single result covers every call it made, so
//!   its success is not any one command's success.
//!
//! `exit_code` is filled only when the result text states one, in one of the
//! shapes in [`exit_code`]; it is never inferred from `is_error`.
//!
//! ## Which runs are tests (a heuristic, stated)
//!
//! A command is a test run when any of its segments — split on newlines, `&&`,
//! `||`, `;` and `|`, with leading `cd …`, environment assignments (`A=b`),
//! `env`, `time`, `timeout N`, `nice`, `command` and `exec` removed — starts
//! with a runner [`crate::identity::is_test_invocation`] already recognises
//! (`cargo test`, `npm test`, `pnpm test`, `yarn test`, `swift test`,
//! `node --test`, `pytest`, `vitest`, `jest`, `go test`, `test*.sh`) or one of
//! [`EXTRA_TEST_RUNNERS`].

use serde_json::Value;

/// Runners recognised here in addition to `identity::is_test_invocation`.
pub const EXTRA_TEST_RUNNERS: &[&str] = &[
    "cargo nextest",
    "cargo insta test",
    "npm run test",
    "npm run vitest",
    "pnpm run test",
    "yarn run test",
    "npx vitest",
    "npx jest",
    "npx playwright test",
    "bun test",
    "deno test",
    "python -m pytest",
    "python3 -m pytest",
    "uv run pytest",
    "make test",
    "make check",
    "mvn test",
    "gradle test",
    "./gradlew test",
    "dotnet test",
    "rspec",
    "bundle exec rspec",
    "phpunit",
    "mix test",
    "ctest",
];

/// The command text of a `Bash` row's input: `command` or `cmd`, as a string
/// or an argv array (`["bash", "-lc", X]` is `X`; any other array is joined
/// with spaces).
pub fn command_of(input: &Value) -> Option<String> {
    let v = input.get("command").or_else(|| input.get("cmd"))?;
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            if parts.len() >= 3
                && matches!(
                    parts[0],
                    "bash" | "sh" | "zsh" | "/bin/bash" | "/bin/sh" | "/bin/zsh"
                )
                && parts[1].starts_with('-')
                && parts[1].contains('c')
            {
                return Some(parts[2..].join(" "));
            }
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        _ => None,
    }
}

/// The segments of a command line, with wrappers removed (see module docs).
pub fn segments(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in command.lines() {
        let mut pieces = vec![line.to_string()];
        for sep in ["&&", "||", ";", "|"] {
            pieces = pieces
                .iter()
                .flat_map(|p| p.split(sep).map(str::to_string).collect::<Vec<_>>())
                .collect();
        }
        for piece in pieces {
            let seg = unwrap_segment(piece.trim());
            if !seg.is_empty() {
                out.push(seg);
            }
        }
    }
    out
}

fn unwrap_segment(seg: &str) -> String {
    let mut words: Vec<&str> = seg
        .trim_start_matches(['(', '{'])
        .split_whitespace()
        .collect();
    while let Some(first) = words.first().copied() {
        let is_assignment = first.contains('=')
            && first.split('=').next().is_some_and(|k| {
                !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
        if is_assignment || matches!(first, "env" | "time" | "nice" | "command" | "exec" | "sudo") {
            words.remove(0);
            continue;
        }
        if first == "timeout" || first == "gtimeout" {
            words.remove(0);
            if words
                .first()
                .is_some_and(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
            {
                words.remove(0);
            }
            continue;
        }
        break;
    }
    words.join(" ")
}

/// Is this command a test run? (See module docs for the rule.)
pub fn is_test(command: &str) -> bool {
    segments(command).iter().any(|seg| {
        crate::identity::is_test_invocation(&format!("bash::{seg}"))
            || EXTRA_TEST_RUNNERS.iter().any(|r| {
                seg == r
                    || seg
                        .strip_prefix(r)
                        .is_some_and(|rest| rest.starts_with(' '))
            })
    })
}

/// Is this command a `git commit`? (`git commit …` or `git -C <dir> commit …`.)
pub fn is_commit(command: &str) -> bool {
    segments(command).iter().any(|seg| {
        let words: Vec<&str> = seg.split_whitespace().collect();
        if words.first() != Some(&"git") {
            return false;
        }
        let mut i = 1;
        while i < words.len() {
            match words[i] {
                "-C" | "-c" | "--git-dir" | "--work-tree" => i += 2,
                w if w.starts_with('-') => i += 1,
                w => return w == "commit",
            }
        }
        false
    })
}

/// The `-m` message of a `git commit`, verbatim (unmasked; the caller masks).
/// Handles `-m "…"`, `-m '…'`, `--message=…` and the heredoc form
/// `-m "$(cat <<'EOF' … EOF)"`. `None` when no message is written out.
pub fn commit_message(command: &str) -> Option<String> {
    if let Some(start) = command.find("<<'EOF'").or_else(|| command.find("<<EOF")) {
        let body = &command[start..];
        let body = &body[body.find('\n')? + 1..];
        let end = body.find("\nEOF").unwrap_or(body.len());
        let msg = body[..end].trim();
        return (!msg.is_empty()).then(|| msg.to_string());
    }
    let idx = command
        .find(" -m ")
        .map(|i| i + 4)
        .or_else(|| command.find("--message=").map(|i| i + 10))?;
    let rest = command[idx..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        let word: String = rest.split_whitespace().next()?.to_string();
        return (!word.starts_with('$')).then_some(word);
    }
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    let msg = &inner[..end];
    (!msg.is_empty() && !msg.starts_with("$(")).then(|| msg.to_string())
}

/// The commit hash a `git commit` result names: `[branch abc1234] message`.
pub fn commit_hash(result_text: &str) -> Option<String> {
    for line in result_text.lines() {
        let t = line.trim();
        if !t.starts_with('[') {
            continue;
        }
        let close = t.find(']')?;
        let inside = &t[1..close];
        let hash = inside.split_whitespace().last()?;
        if hash.len() >= 7 && hash.len() <= 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hash.to_string());
        }
    }
    None
}

/// The exit code a result text states, in one of these shapes (first match):
/// `Exit code N` (Claude Code, at the start), `Process exited with code N`,
/// `exited with code N`, `exit code: N`, `"exit_code": N`, `exit_code: N`.
pub fn exit_code(text: &str) -> Option<i64> {
    let lower = text.to_ascii_lowercase();
    for marker in [
        "exit code ",
        "process exited with code ",
        "exited with code ",
        "exit code: ",
        "\"exit_code\":",
        "\"exitcode\":",
        "exit_code: ",
        "exit_code=",
    ] {
        if let Some(pos) = lower.find(marker) {
            let rest = lower[pos + marker.len()..].trim_start();
            let digits: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            if let Ok(code) = digits.parse::<i64>() {
                return Some(code);
            }
        }
    }
    None
}

/// The text of a stored `tool_result` JSON (`{content, toolUseResult}`):
/// `content` as a string, or the `text` of each element when it is an array.
pub fn result_text(result: &Value) -> String {
    fn flatten(v: &Value, out: &mut String) {
        match v {
            Value::String(s) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(s);
            }
            Value::Array(items) => {
                for item in items {
                    if let Some(t) = item.get("text") {
                        flatten(t, out);
                    } else if item.is_string() {
                        flatten(item, out);
                    }
                }
            }
            Value::Object(map) => {
                if let Some(t) = map.get("text").or_else(|| map.get("output")) {
                    flatten(t, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    if let Some(content) = result.get("content") {
        flatten(content, &mut out);
    }
    out
}

/// How many trailing lines of a command's output are served.
pub const EXCERPT_LINES: usize = 40;
/// How many trailing lines are masked before the excerpt is cut, so a secret
/// shape that spans lines is seen whole by the masker.
pub const EXCERPT_MASK_WINDOW: usize = 200;
/// Longest served output line, in characters.
pub const EXCERPT_LINE_CHARS: usize = 400;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_runs_are_recognised_through_wrappers() {
        for cmd in [
            "cargo test -p thruline-core",
            "cd ui && npm test",
            "RUST_TEST_THREADS=4 cargo test --workspace 2>&1 | tail -20",
            "timeout 600 pytest -q",
            "npx vitest run",
            "git status\ncargo nextest run",
        ] {
            assert!(is_test(cmd), "{cmd}");
        }
        for cmd in [
            "cargo build",
            "git commit -m 'test'",
            "echo cargo test",
            "ls tests/",
        ] {
            assert!(!is_test(cmd), "{cmd}");
        }
    }

    #[test]
    fn commits_and_their_messages() {
        assert!(is_commit("git add -A && git commit -m \"fix x\""));
        assert!(is_commit("git -C /r commit -q -m x"));
        assert!(!is_commit("git log --grep commit"));
        assert_eq!(
            commit_message("git commit -m \"fix the parser\"").as_deref(),
            Some("fix the parser")
        );
        assert_eq!(
            commit_message("git commit -m \"$(cat <<'EOF'\nAdd HQ routes\n\nBody\nEOF\n)\"")
                .as_deref(),
            Some("Add HQ routes\n\nBody")
        );
        assert_eq!(
            commit_hash("[hq-next/ui 1a2b3c4] Add HQ routes\n 3 files changed").as_deref(),
            Some("1a2b3c4")
        );
    }

    #[test]
    fn exit_codes_are_read_only_when_stated() {
        assert_eq!(exit_code("Exit code 101\nerror: test failed"), Some(101));
        assert_eq!(exit_code("Process exited with code 0\nok"), Some(0));
        assert_eq!(exit_code("{\"exit_code\": 2}"), Some(2));
        assert_eq!(exit_code("all good"), None);
    }

    #[test]
    fn command_text_from_each_input_shape() {
        assert_eq!(command_of(&json!({"command": "ls"})).as_deref(), Some("ls"));
        assert_eq!(
            command_of(&json!({"cmd": "ls -la"})).as_deref(),
            Some("ls -la")
        );
        assert_eq!(
            command_of(&json!({"command": ["bash", "-lc", "cargo test"]})).as_deref(),
            Some("cargo test")
        );
        assert_eq!(command_of(&json!({"other": 1})), None);
    }
}
