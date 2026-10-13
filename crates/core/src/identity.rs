//! §5i — semantic identity for evidence.
//!
//! An identity key answers one question: *are these two events the same
//! invocation?* Round 4 answered it with raw command text and got it wrong in
//! the most expensive way available (A51): `cd /Users/…/atlas-proxy && npm test
//! 2>&1 | tail -25` and `npm test 2>&1 | tail -10` are the same suite — the
//! second is the re-run that fixed the first — and textual keys put them in
//! different supersession groups. The stale failure stayed "latest in its own
//! group", kept verdict authority, and convicted a true claim. Twice.
//!
//! So identity is normalised, not compared:
//!
//! * `cd <path> &&` folds into a **cwd** component, and where the command names
//!   no `cd`, the per-line `cwd` from the raw JSONL supplies it. This is what
//!   actually unifies the round-4 pairs: one runs `cd …/atlas-proxy` from the
//!   repo root, the other runs bare from `…/atlas-proxy`.
//! * Display-only pipe stages and redirections are **stripped**. `| tail -25`
//!   versus `| tail -10` changes what a human reads, never what executed.
//! * Program, subcommand and **scope-bearing** arguments are canonicalised.
//!   Filters and selectors are kept: `--filter EvictionTests` is the difference
//!   between two suites, not two spellings of one.
//!
//! Where this cannot decide, callers treat the runs as same-family for
//! conviction (§5f-rev). Undecidability resolves toward abstention.

/// Pipe stages that only change what is displayed.
const DISPLAY_ONLY: [&str; 8] = ["tail", "head", "tee", "cat", "less", "more", "nl", "column"];

/// Arguments that name *which* suite ran. Dropping these would merge genuinely
/// different runs, which is the opposite error and a far worse one.
const SCOPE_FLAGS: [&str; 10] = [
    "--filter",
    "--package",
    "-p",
    "--test",
    "--bin",
    "--example",
    "--scheme",
    "--target",
    "--testplan",
    "--project",
];

/// Runner heads we recognise, longest-first so `node --test` wins over `node`.
const RUNNERS: [&str; 14] = [
    "cargo test",
    "cargo build",
    "cargo clippy",
    "npm test",
    "pnpm test",
    "yarn test",
    "swift test",
    "swift build",
    "xcodebuild",
    "node --test",
    "pytest",
    "vitest",
    "jest",
    "go test",
];

/// Does this §5i identity key name a recognised runner invocation?
///
/// A59's three stale-run convictions share one mechanism: a LATER green run
/// whose compound output abstains (several same-family summaries in one
/// payload → family `none`) leaves an earlier failure "latest identity-
/// matched". The outcome is unreadable, but the key still tells us the event
/// *ran a test/build tool* — and an unreadable later run of a runner is
/// exactly the situation where the final state is unknown (§5f-rev).
pub fn is_runner_invocation(identity_key: &str) -> bool {
    let Some(rest) = identity_key.strip_prefix("bash:") else {
        return false;
    };
    let Some((_cwd, invocation)) = rest.split_once(':') else {
        return false;
    };
    RUNNERS.iter().any(|r| invocation.contains(r))
}

/// Closed-world family recognition for conviction (§5q).
///
/// Identity normalisation may preserve any shell command, but only an
/// affirmatively recognised test/build invocation may turn its outcome into
/// an accusation. `grep` exit 1, for example, often means the desired
/// zero-match result; treating every non-zero shell status as a failed build
/// produced a Round-6 false conviction.
pub fn is_test_invocation(identity_key: &str) -> bool {
    let Some(rest) = identity_key.strip_prefix("bash:") else {
        return false;
    };
    let Some((_cwd, invocation)) = rest.split_once(':') else {
        return false;
    };
    if [
        "cargo test",
        "npm test",
        "pnpm test",
        "yarn test",
        "swift test",
        "node --test",
        "pytest",
        "vitest",
        "jest",
        "go test",
    ]
    .iter()
    .any(|part| invocation.starts_with(part))
    {
        return true;
    }
    let head = invocation.split_whitespace().next().unwrap_or("");
    let script = head.rsplit('/').next().unwrap_or(head);
    script == "test.sh"
        || script == "tests.sh"
        || script.starts_with("test-")
        || script.ends_with("-test.sh")
}

pub fn is_build_invocation(identity_key: &str) -> bool {
    let Some(rest) = identity_key.strip_prefix("bash:") else {
        return false;
    };
    let Some((_cwd, invocation)) = rest.split_once(':') else {
        return false;
    };
    if [
        "cargo build",
        "cargo clippy",
        "swift build",
        "xcodebuild",
        "npm run build",
        "npm run lint",
        "pnpm build",
        "pnpm lint",
        "yarn build",
        "yarn lint",
        "make",
        "cmake --build",
    ]
    .iter()
    .any(|part| invocation.starts_with(part))
    {
        return true;
    }
    let head = invocation.split_whitespace().next().unwrap_or("");
    let script = head.rsplit('/').next().unwrap_or(head);
    script == "build.sh"
        || script == "lint.sh"
        || script == "compile.sh"
        || script.ends_with("-build.sh")
        || script.ends_with("-lint.sh")
        || script.ends_with("-compile.sh")
}

/// Normalise an absolute-ish path for comparison. Trailing slashes go; the
/// path is kept whole rather than reduced to a basename, because two projects
/// can both contain a `server` directory.
fn norm_path(p: &str) -> String {
    p.trim()
        .trim_matches(['\'', '"'])
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

/// Resolve the directory a command actually ran in.
///
/// `line_cwd` is the per-line `cwd` recorded in the raw JSONL. A `cd` inside
/// the command wins over it; a relative `cd` is joined onto it.
pub fn effective_cwd(command: &str, line_cwd: Option<&str>) -> String {
    let base = line_cwd.map(norm_path).unwrap_or_default();
    let first = command.replace(['\n', '\r', '\t'], " ");
    for part in first.split("&&") {
        let t = part.trim();
        if let Some(rest) = t.strip_prefix("cd ") {
            let target = rest.split_whitespace().next().unwrap_or("");
            if target.is_empty() {
                continue;
            }
            let target = norm_path(target);
            if target.starts_with('/') || target.starts_with('~') {
                return target;
            }
            if base.is_empty() {
                return target;
            }
            return format!("{base}/{}", target.trim_start_matches("./"));
        }
    }
    base
}

/// Strip redirections and display-only pipe stages from one command.
///
/// Everything after a display-only pipe is dropped. A pipe into something that
/// *is not* display-only (`grep`, `jq`) is kept, because it changes which lines
/// exist and so can change what the run appears to report.
fn strip_noise(command: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for (i, stage) in command.split('|').enumerate() {
        let t = stage.trim();
        let head = t.split_whitespace().next().unwrap_or("");
        if i > 0 && DISPLAY_ONLY.contains(&head) {
            continue;
        }
        // Redirections: drop the operator and its target.
        let mut kept: Vec<&str> = Vec::new();
        let mut skip_next = false;
        for tok in t.split_whitespace() {
            if skip_next {
                skip_next = false;
                continue;
            }
            if tok == "2>&1" || tok == "&>" || tok == ">" || tok == ">>" || tok == "<" {
                skip_next = tok != "2>&1";
                continue;
            }
            if tok.starts_with("2>") || tok.starts_with(">>") || tok.starts_with('>') {
                continue;
            }
            kept.push(tok);
        }
        if !kept.is_empty() {
            out.push(kept.join(" "));
        }
    }
    out.join(" | ")
}

/// The canonical invocation of a shell command: runner head plus any
/// scope-bearing arguments, in a stable order.
fn canonical_invocation(command: &str) -> String {
    let one = strip_noise(
        &command
            .replace(['\n', '\r', '\t'], " ")
            .to_ascii_lowercase(),
    );
    // Filter stages that SURVIVED `strip_noise` change which lines exist, so
    // they can change what the run appears to report. They stay in the key —
    // merging `npm test` with `npm test | grep '^not ok'` would be a silent
    // lie about what was observed.
    let filters: Vec<String> = one
        .split('|')
        .skip(1)
        .filter_map(|st| st.split_whitespace().next())
        .map(|h| format!("|{h}"))
        .collect();
    let filter_suffix = filters.join("");
    let one = one.split('|').next().unwrap_or(&one).trim().to_string();
    // Drop every `cd …` clause; the directory lives in the cwd component.
    let body: Vec<&str> = one
        .split("&&")
        .map(|p| p.trim())
        .filter(|p| !p.starts_with("cd "))
        .collect();
    let body = body.join(" && ");

    let start = RUNNERS.iter().filter_map(|r| body.find(r)).min();
    let Some(start) = start else {
        // Not a recognised runner: keep the first clause, normalised. Better a
        // conservative key than a wrong merge.
        return format!(
            "{}{filter_suffix}",
            body.split("&&")
                .next()
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        );
    };
    let rest = &body[start..];
    let end = ["&&", ";"]
        .iter()
        .filter_map(|sep| rest.find(sep))
        .min()
        .unwrap_or(rest.len());
    let toks: Vec<&str> = rest[..end].split_whitespace().collect();

    // Runner head = the matched runner phrase (one or two tokens).
    let head_len = RUNNERS
        .iter()
        .filter(|r| rest[..end].starts_with(*r))
        .map(|r| r.split_whitespace().count())
        .max()
        .unwrap_or(1);
    let head = toks
        .iter()
        .take(head_len)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

    // Scope-bearing args, sorted so flag order cannot split an identity.
    let mut scope: Vec<String> = Vec::new();
    let mut i = head_len;
    while i < toks.len() {
        let t = toks[i];
        if let Some((flag, val)) = t.split_once('=') {
            if SCOPE_FLAGS.contains(&flag) {
                scope.push(format!("{flag}={val}"));
            }
            i += 1;
            continue;
        }
        if SCOPE_FLAGS.contains(&t) {
            if let Some(v) = toks.get(i + 1) {
                scope.push(format!("{t}={v}"));
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    scope.sort();
    if scope.is_empty() {
        format!("{head}{filter_suffix}")
    } else {
        format!("{head} {}{filter_suffix}", scope.join(" "))
    }
}

/// The §5i identity key for one evidence event.
///
/// `tool_input` is the raw JSON the tool was called with; `line_cwd` is the
/// per-line `cwd` from the raw JSONL, where the caller has it.
pub fn identity_key(tool_name: &str, tool_input: &str, line_cwd: Option<&str>) -> String {
    let tool = tool_name.to_ascii_lowercase();
    if tool == "bash" {
        let command = crate::derive::json_string_field(tool_input, "command")
            .unwrap_or_else(|| tool_input.to_string());
        let cwd = effective_cwd(&command, line_cwd);
        return format!("bash:{cwd}:{}", canonical_invocation(&command));
    }
    for key in ["file_path", "path", "notebook_path", "command", "subject"] {
        if let Some(v) = crate::derive::json_string_field(tool_input, key) {
            return format!("{tool}:{}", norm_path(&v));
        }
    }
    format!("{tool}:")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A51's first named regression, from an ATLAS session.
    #[test]
    fn round4_pair_one_normalises_to_one_identity() {
        let stale = identity_key(
            "Bash",
            r#"{"command":"cd /Users/dev/Documents/ATLAS/atlas-proxy && npm test 2>&1 | tail -25"}"#,
            Some("/Users/dev/Documents/ATLAS"),
        );
        let rerun = identity_key(
            "Bash",
            r#"{"command":"npm test 2>&1 | tail -10"}"#,
            Some("/Users/dev/Documents/ATLAS/atlas-proxy"),
        );
        assert_eq!(
            stale, rerun,
            "seq 1005 and 1015 must share an identity — they are the same suite, \
             and the second is the re-run that fixed the first"
        );
        assert!(
            stale.ends_with("atlas-proxy:npm test"),
            "identity should be the invocation, not its decoration: {stale}"
        );
    }

    /// A51's second named regression.
    #[test]
    fn round4_pair_two_normalises_to_one_identity() {
        let stale = identity_key(
            "Bash",
            r#"{"command":"cd /Users/dev/Documents/ATLAS/atlas-proxy && npm test 2>&1 | tail -12"}"#,
            Some("/Users/dev/Documents/ATLAS"),
        );
        let rerun = identity_key(
            "Bash",
            r#"{"command":"npm test 2>&1 | tail -12"}"#,
            Some("/Users/dev/Documents/ATLAS/atlas-proxy"),
        );
        assert_eq!(stale, rerun, "seq 1207 and 1218 must share an identity");
    }

    #[test]
    fn different_packages_stay_different() {
        // The opposite error, and the worse one: merging two real suites would
        // let one package's failure convict another package's claim.
        let proxy = identity_key(
            "Bash",
            r#"{"command":"npm test"}"#,
            Some("/repo/atlas-proxy"),
        );
        let rooms = identity_key(
            "Bash",
            r#"{"command":"npm test"}"#,
            Some("/repo/atlas-rooms"),
        );
        assert_ne!(proxy, rooms);
    }

    #[test]
    fn suite_selectors_remain_identity_bearing() {
        let a = identity_key(
            "Bash",
            r#"{"command":"swift test --filter EvictionPolicyTests"}"#,
            Some("/repo"),
        );
        let b = identity_key(
            "Bash",
            r#"{"command":"swift test --filter ReplayCursorTests"}"#,
            Some("/repo"),
        );
        let all = identity_key("Bash", r#"{"command":"swift test"}"#, Some("/repo"));
        assert_ne!(a, b, "two filters are two suites");
        assert_ne!(a, all, "a filtered run is not the whole suite");
    }

    #[test]
    fn conviction_family_recognition_is_a_whitelist() {
        let tests = identity_key(
            "Bash",
            r#"{"command":"./scripts/acceptance-test.sh"}"#,
            Some("/repo"),
        );
        let build = identity_key(
            "Bash",
            r#"{"command":"./scripts/release-build.sh"}"#,
            Some("/repo"),
        );
        let grep = identity_key(
            "Bash",
            r#"{"command":"grep -R old-glyph src"}"#,
            Some("/repo"),
        );
        assert!(is_test_invocation(&tests));
        assert!(is_build_invocation(&build));
        assert!(!is_test_invocation(&grep));
        assert!(!is_build_invocation(&grep));
        let grep_build = identity_key(
            "Bash",
            r#"{"command":"grep -R build-manifest src"}"#,
            Some("/repo"),
        );
        let grep_test = identity_key(
            "Bash",
            r#"{"command":"grep -R acceptance-test.sh docs"}"#,
            Some("/repo"),
        );
        assert!(!is_build_invocation(&grep_build));
        assert!(!is_test_invocation(&grep_test));
    }

    #[test]
    fn flag_order_and_decoration_do_not_split_an_identity() {
        let a = identity_key(
            "Bash",
            r#"{"command":"cargo test -p core --test derive 2>&1 | tail -40"}"#,
            Some("/repo"),
        );
        let b = identity_key(
            "Bash",
            r#"{"command":"cargo test --test derive -p core > /tmp/out.log"}"#,
            Some("/repo"),
        );
        assert_eq!(a, b, "same run, different spelling and flag order");
    }

    #[test]
    fn a_non_display_pipe_is_kept() {
        // `| grep` changes which lines exist, so it can change what the run
        // appears to report. Merging it with the unfiltered run would be a
        // silent lie about what was observed.
        let plain = identity_key("Bash", r#"{"command":"npm test"}"#, Some("/repo"));
        let grepped = identity_key(
            "Bash",
            r#"{"command":"npm test | grep -E '^not ok'"}"#,
            Some("/repo"),
        );
        assert_ne!(plain, grepped);
    }

    #[test]
    fn relative_cd_joins_onto_the_line_cwd() {
        let a = identity_key(
            "Bash",
            r#"{"command":"cd atlas-rooms && npm test"}"#,
            Some("/repo"),
        );
        let b = identity_key(
            "Bash",
            r#"{"command":"npm test"}"#,
            Some("/repo/atlas-rooms"),
        );
        assert_eq!(a, b);
    }
}
