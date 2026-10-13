//! The harness driver, and §0.2's ruling expressed as a type.
//!
//! The pilot could not run because step 6 was never built: `PilotTask` was a
//! struct nothing filled. These assertions are about the two ways filling it
//! could go wrong — capturing nothing, and capturing the wrong *kind* of thing.

use thruline_bench::driver::{
    classify_artifact, CaptureLayer, ContainerCapture, HarnessArtifact, Route, TaskEvidence, TbRun,
    HARNESS_ARTIFACT_KINDS,
};
use thruline_bench::pilot::TaskCost;

fn artifact(kind: &str) -> HarnessArtifact {
    HarnessArtifact {
        kind: kind.to_string(),
        path: format!("runs/2026-09-03/t0/{kind}"),
        bytes: 4096,
    }
}

// ---------------------------------------------------------------------------
// The ruling: corroboration can never become the record.
// ---------------------------------------------------------------------------

/// **A task with every harness artifact and no container capture reports
/// zero.**
///
/// This is §0.2 as a type rather than a rule. Terminal-Bench's `agent-logs`,
/// `commands.txt`, `panes` and `sessions` are the harness's own account of what
/// it believes it ran — a hint. Ingesting them is easy, which is exactly why
/// the temptation exists; the whole product is built on those two things being
/// different.
#[test]
fn harness_artifacts_alone_produce_no_coverage_at_all() {
    let e = TaskEvidence {
        task_id: "t0".into(),
        container: None,
        corroboration: HARNESS_ARTIFACT_KINDS.iter().map(|k| artifact(k)).collect(),
        terminal_state: "completed".into(),
        cost: Some(TaskCost {
            tokens_in: 1000,
            tokens_out: 100,
            wall_ms: 1000,
        }),
    };
    assert_eq!(e.corroboration.len(), 5);
    assert!(!e.has_execution_record());

    let t = e.into_pilot_task();
    assert_eq!(t.hook_events, 0);
    assert_eq!(t.shim_exec_events, 0);
    assert_eq!(
        t.checks_bound_to_a_tree, 0,
        "harness artifacts produced a bound check; a hint became the record"
    );
    assert_eq!(t.tree_before_first_action, None);
}

/// And with container capture the counts are the container's, unaffected by how
/// much corroboration is or is not present.
#[test]
fn coverage_comes_from_the_container_and_only_the_container() {
    let container = ContainerCapture {
        hook_events: 12,
        shim_exec_events: 4,
        checks_bound_to_a_tree: 2,
        tree_before_first_action: Some("a".repeat(40)),
    };
    let bare = TaskEvidence {
        task_id: "t0".into(),
        container: Some(container.clone()),
        corroboration: Vec::new(),
        terminal_state: "completed".into(),
        cost: None,
    };
    let padded = TaskEvidence {
        corroboration: (0..50).map(|_| artifact("agent-logs")).collect(),
        ..bare.clone()
    };
    let a = bare.into_pilot_task();
    let b = padded.into_pilot_task();
    assert_eq!(a.checks_bound_to_a_tree, 2);
    assert_eq!(
        a.checks_bound_to_a_tree, b.checks_bound_to_a_tree,
        "fifty harness artifacts changed the bound-check count"
    );
    assert_eq!(a.hook_events, b.hook_events);
}

/// The routes are not interchangeable, and only one can bind a check.
#[test]
fn only_the_execution_record_can_bind_a_check() {
    assert!(Route::ContainerCapture.can_bind_a_check());
    assert!(
        !Route::HarnessArtifact.can_bind_a_check(),
        "post-hoc harness output was allowed to bind a check"
    );
    assert_ne!(
        Route::ContainerCapture.as_str(),
        Route::HarnessArtifact.as_str()
    );
}

/// Corroboration is recorded **with its provenance**, and an unrecognised
/// artifact is reported rather than folded into a known kind.
#[test]
fn corroboration_is_censused_by_kind_and_strangers_are_not_absorbed() {
    let e = TaskEvidence {
        task_id: "t0".into(),
        container: None,
        corroboration: vec![
            artifact("agent-logs"),
            artifact("agent-logs"),
            artifact("panes"),
        ],
        terminal_state: "completed".into(),
        cost: None,
    };
    let census = e.corroboration_census();
    assert_eq!(census.get("agent-logs"), Some(&2));
    assert_eq!(census.get("panes"), Some(&1));

    assert_eq!(
        classify_artifact("runs/x/t0/agent-logs/a.txt"),
        Some("agent-logs")
    );
    assert_eq!(
        classify_artifact("runs/x/t0/commands.txt"),
        Some("commands.txt")
    );
    assert_eq!(
        classify_artifact("runs/x/t0/something-new.bin"),
        None,
        "an unanticipated artifact kind was absorbed into a known one"
    );
}

// ---------------------------------------------------------------------------
// The image layer: git, ThruLine, and a tree before the first action.
// ---------------------------------------------------------------------------

/// **`git` is installed because neither dominant base image has it.**
/// Measured: `python-3-13:20250620` (88 tasks) and `ubuntu-24-04:20250624`
/// (97 tasks) both report `git: ABSENT`, and both have `apt-get`.
#[test]
fn the_capture_layer_installs_git_before_it_uses_git() {
    let d = CaptureLayer::default().dockerfile();
    let install = d
        .find("install -y --no-install-recommends git")
        .expect("no git install");
    let init = d.find("git init").expect("no git init");
    assert!(
        install < init,
        "the layer runs `git init` before installing git"
    );
    assert!(d.contains("command -v git"), "the install is unconditional");
}

/// **A tree hash must exist before the agent's first action.** `git init` alone
/// leaves an unborn repository with no tree, satisfying the letter of the
/// precondition and none of its purpose.
#[test]
fn the_capture_layer_commits_so_a_tree_exists() {
    let d = CaptureLayer::default().dockerfile();
    let init = d.find("git init").expect("init");
    let commit = d
        .find("git commit")
        .expect("no commit — an unborn repo has no tree");
    let record = d
        .find("tree-before-first-action")
        .expect("no recorded tree");
    assert!(init < commit && commit < record, "{d}");
    assert!(
        d.contains("--allow-empty"),
        "an empty workspace would not commit"
    );
    assert!(
        d.contains("rev-parse HEAD^{tree}"),
        "records a commit, not a tree"
    );
}

/// Capture must run **inside** the container, with the spool on a mounted
/// volume so the fold reaches it on the host. Host-side hooks observe nothing
/// about a process in a container — which is why every coverage number would
/// have been zero.
#[test]
fn capture_is_installed_in_the_container_and_spools_to_a_mount() {
    let layer = CaptureLayer::default();
    let d = layer.dockerfile();
    assert!(d.contains("/usr/local/bin/thruline"), "{d}");
    assert!(d.contains("thruline init"), "{d}");
    assert!(d.contains("ENV THRULINE_HOME=/thruline-spool"), "{d}");
    assert!(
        d.contains("ENV PATH=/thruline-spool/shim:$PATH"),
        "the shim is not on PATH"
    );
    assert_eq!(
        layer.volume_arg("/host/runs/t0"),
        "/host/runs/t0:/thruline-spool",
        "the spool would not survive the container"
    );
}

// ---------------------------------------------------------------------------
// The invocation.
// ---------------------------------------------------------------------------

/// **The zero-cost validation run.** `oracle` replays the task's own solution
/// and makes no model calls, so it proves the container build, the workspace,
/// `git init`, the shim, the fold and the oracle for nothing.
#[test]
fn the_oracle_run_costs_nothing_and_names_no_model() {
    let r = TbRun::oracle("broken-python", "/tmp/out", "/tmp/tasks");
    assert!(!r.costs_money(), "the validation run would have billed");
    assert_eq!(r.model, None);
    let argv = r.argv();
    assert!(argv.contains(&"--agent".to_string()));
    assert!(argv.contains(&"oracle".to_string()));
    assert!(
        !argv.contains(&"--model".to_string()),
        "oracle has nothing to bill and must not name a model"
    );
    assert!(argv.contains(&"broken-python".to_string()));
}

/// A paying agent names its model explicitly — nothing is defaulted.
#[test]
fn a_paying_run_names_its_model() {
    let r = TbRun {
        agent: "codex".into(),
        task_id: "t0".into(),
        model: Some("gpt-5.6-luna".into()),
        output_dir: "/tmp/out".into(),
        dataset_path: "/tmp/tasks".into(),
    };
    assert!(r.costs_money());
    let argv = r.argv();
    let at = argv
        .iter()
        .position(|a| a == "--model")
        .expect("no --model");
    assert_eq!(argv[at + 1], "gpt-5.6-luna");
}

/// **Every `thruline` verb the layer invokes must exist — and so must its
/// flags.**
///
/// An earlier draft ran `thruline init --harness … --scope …`. `init` is a real
/// verb; it has neither flag — they belong to `hook-install`. That would have
/// built 241 images whose capture install failed, and the pilot would have
/// reported zero coverage *again*, for a second reason, after being fixed for
/// the first.
///
/// **The first version of this test checked only the verb and passed on that
/// exact defect.** Checking that a verb exists is not checking that the command
/// is valid — the fourth time in this build that a guard searched a space
/// adjacent to the one it meant.
#[test]
fn the_capture_layer_only_invokes_commands_the_cli_actually_accepts() {
    let main_rs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/src/main.rs"),
    )
    .expect("cli main.rs");

    /// The source block of one `Command` variant.
    fn variant_block<'a>(src: &'a str, camel: &str) -> Option<&'a str> {
        let head = format!("\n    {camel} {{");
        let at = src.find(&head)?;
        let rest = &src[at + 1..];
        let end = rest.find("\n    },")?;
        Some(&rest[..end])
    }

    let d = CaptureLayer::default().dockerfile();
    let mut checked_verbs = 0usize;
    let mut checked_flags = 0usize;

    for line in d.lines() {
        let Some(at) = line.find("thruline ") else {
            continue;
        };
        let mut words = line[at + "thruline ".len()..].split_whitespace();
        let Some(verb) = words.next() else { continue };
        if verb.starts_with('-') || verb.contains('/') {
            continue;
        }
        checked_verbs += 1;
        let camel: String = verb
            .split('-')
            .map(|p| {
                let mut c = p.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        let block = variant_block(&main_rs, &camel).unwrap_or_else(|| {
            panic!("the layer invokes `thruline {verb}`, which is not a Command variant")
        });

        // And every long flag it passes must be a field of THAT variant.
        for w in words {
            let Some(flag) = w.strip_prefix("--") else {
                continue;
            };
            let flag = flag.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            if flag.is_empty() {
                continue;
            }
            checked_flags += 1;
            let field = flag.replace('-', "_");
            assert!(
                block.contains(&format!("{field}:")),
                "`thruline {verb} --{flag}` — `{verb}` exists but has no `{flag}`. \
                 That is the exact defect this test was written for, and its first \
                 version passed on it by checking only the verb."
            );
        }
    }
    assert!(
        checked_verbs >= 2,
        "the walk found {checked_verbs} invocations"
    );
    assert!(checked_flags >= 2, "the walk checked {checked_flags} flags");
}

// ---------------------------------------------------------------------------
// The multi-stage build.
// ---------------------------------------------------------------------------

/// **Multi-stage, not cross-compilation**, and glibc rather than musl.
///
/// Every task base image is Debian or Ubuntu. An Alpine builder would produce a
/// **dynamically linked** musl binary that cannot run in them, and the failure
/// would arrive as a container that starts and does nothing — the worst shape,
/// because it looks like zero coverage rather than a broken build.
///
/// **The assertion below stands; its stated reason needed the qualifier.** A
/// *statically* linked musl binary runs in these images fine — measured, in
/// `debian:bookworm-slim` and in the t-bench base image itself — and
/// `scripts/pilot-run.sh` now bind-mounts exactly that, selected per
/// architecture inside the container. This test governs the **in-image
/// multi-stage builder**, a different artefact: it compiles inside each task
/// image against that image's own glibc, so Alpine here would be wrong for a
/// reason that has nothing to do with whether musl can reach Debian.
/// Keeping the assertion and fixing only the sentence is deliberate — the rule
/// is right, the explanation was not, and an explanation nobody can trust is
/// how the next person argues themselves out of a correct constraint.
#[test]
fn the_builder_stage_is_glibc_and_matches_the_pinned_toolchain() {
    use thruline_bench::driver::BUILDER_STAGE;
    let layer = CaptureLayer::default();
    let b = layer.builder_stage();

    assert!(b.contains(&format!("AS {BUILDER_STAGE}")), "{b}");
    assert!(b.contains("cargo build --release --locked"), "{b}");
    // **`--no-default-features`**: the CLI's `default = ["ort"]` pulls ONNX
    // Runtime, which downloads a binary from a CDN at BUILD time. The container
    // captures; it never annotates. Leaving the default on would add a network
    // dependency to 241 image builds for a model none of them runs.
    assert!(
        b.contains("--no-default-features"),
        "the builder pulls ONNX Runtime into every task image: {b}"
    );
    // Over the RUNNING lines only. The first version of this check matched the
    // comment that *explains* why musl is wrong — the same wrong-search-space
    // failure this file documents, committed inside the test written to guard
    // against it.
    let running: String = b
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for musl in ["alpine", "musl"] {
        assert!(
            !running.contains(musl),
            "the builder is musl-based. This stage compiles INSIDE each task \
             image against that image's own glibc, so an Alpine stage is wrong \
             here — not because static musl cannot reach a Debian image (it \
             can; the pilot bind-mounts one), but because this artefact is not \
             the one that gets mounted."
        );
    }
    assert!(
        running.contains("from rust:"),
        "the walk sees no FROM: {running}"
    );

    // The builder's compiler is the repository's pinned one: a different
    // compiler is a different binary, and the pin is part of the
    // byte-identical rebuild contract.
    let toolchain = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rust-toolchain.toml"),
    )
    .expect("rust-toolchain.toml");
    let pinned = toolchain
        .lines()
        .find_map(|l| l.trim().strip_prefix("channel = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("channel");
    let (version, distro) = layer
        .rust_toolchain
        .split_once('-')
        .expect("the builder tag must name its Debian release explicitly");
    assert!(
        pinned.starts_with(version),
        "the builder uses rust:{} and the repository pins {pinned}",
        layer.rust_toolchain
    );
    // **The distro suffix is load-bearing, and measured.** Unqualified
    // `rust:1.97` tracks the newest Debian — glibc 2.41 — and glibc is backward
    // compatible, not forward, so its binary dies on the oldest target with
    // `libc.so.6: version 'GLIBC_2.39' not found`. Measured: builder 2.41,
    // python-3-13 2.36, ubuntu-24-04 2.39. Build on the OLDEST target.
    assert_eq!(
        distro, "bookworm",
        "the builder must be built on the oldest target's glibc (2.36); \
         `{distro}` is not it"
    );
}

/// The composed Dockerfile puts the builder first, the task's own content in
/// the middle, and the capture layer last — and the `COPY --from` resolves to
/// the stage the builder declares.
#[test]
fn the_composed_dockerfile_is_ordered_and_its_copy_resolves() {
    use thruline_bench::driver::BUILDER_STAGE;
    let task = "FROM ghcr.io/laude-institute/t-bench/python-3-13:20250620\nRUN rm -rf /x\n";
    let d = CaptureLayer::default().compose(task);

    let builder = d.find("AS thruline-builder").expect("builder stage");
    let task_from = d.find("FROM ghcr.io").expect("task FROM");
    let copy = d.find("COPY --from=").expect("copy");
    assert!(builder < task_from, "the builder stage is not first");
    assert!(
        task_from < copy,
        "the capture layer precedes the task image"
    );
    assert!(d.contains(&format!("COPY --from={BUILDER_STAGE} ")), "{d}");
    assert!(
        d.contains("RUN rm -rf /x"),
        "the task's own content was dropped"
    );

    // And nothing copies a host-built binary any more: that was the gap.
    assert!(
        !d.contains("COPY thruline "),
        "the layer still copies a host binary, which is the wrong platform"
    );
}

/// **Container-side capture is unbuilt, not infeasible** — and the harness
/// already demonstrates the mechanism.
///
/// Terminal-Bench's own per-task compose file bind-mounts two host paths into
/// the container. The spool is a third entry in a list that has two. This is
/// the real file from `original-tasks/broken-python`.
#[test]
fn the_spool_mount_is_a_third_entry_in_a_list_the_harness_already_has() {
    let real = "services:\n  client:\n    build:\n      dockerfile: Dockerfile\n    \
                command: [ \"sh\", \"-c\", \"sleep infinity\" ]\n    volumes:\n      \
                - ${T_BENCH_TASK_LOGS_PATH}:${T_BENCH_CONTAINER_LOGS_PATH}\n      \
                - ${T_BENCH_TASK_AGENT_LOGS_PATH}:${T_BENCH_CONTAINER_AGENT_LOGS_PATH}\n";
    let out = CaptureLayer::default().compose_yaml(real, "/host/runs/t0/spool");

    assert!(
        out.contains("- /host/runs/t0/spool:/thruline-spool"),
        "{out}"
    );
    // The harness's own mounts survive untouched.
    assert!(out.contains("${T_BENCH_TASK_LOGS_PATH}"), "{out}");
    assert!(out.contains("${T_BENCH_TASK_AGENT_LOGS_PATH}"), "{out}");
    assert_eq!(out.matches("volumes:").count(), 1, "a second volumes: key");

    // **A literal path, never a ${VAR}**: an unset variable interpolates to
    // empty and mounts the container root, which fails in a way nobody reads as
    // a missing environment variable.
    let spool_line = out
        .lines()
        .find(|l| l.contains("/thruline-spool"))
        .expect("spool line");
    assert!(!spool_line.contains("${"), "{spool_line}");

    // And the spool does NOT ride in the harness's agent-logs mount: §0.2's
    // boundary should not depend on telling two things apart inside one
    // directory.
    assert!(
        !spool_line.contains("AGENT_LOGS"),
        "the execution record shares a mount with the harness's own account"
    );
}

/// The artifact kinds, checked against a **real** oracle run rather than a
/// guess.
///
/// Paths taken verbatim from `tb run --agent oracle --task-id broken-python`:
/// exit 0, 1 resolved, 100%, zero model calls.
#[test]
fn artifact_classification_matches_a_real_harness_run() {
    let trial = "2026-09-03__05-53-58/broken-python/broken-python.1-of-1.2026-09-03__05-53-58";
    for (path, want) in [
        (format!("{trial}/commands.txt"), Some("commands.txt")),
        (format!("{trial}/panes/pre-agent.txt"), Some("panes")),
        (format!("{trial}/panes/post-test.txt"), Some("panes")),
        (format!("{trial}/sessions/agent.cast"), Some("sessions")),
        (format!("{trial}/sessions/tests.log"), Some("sessions")),
        (format!("{trial}/results.json"), Some("results.json")),
        // Run-level files this list does not name. **Unrecognised is the right
        // answer** — reported, never folded into a kind it is not.
        ("2026-09-03__05-53-58/run_metadata.json".to_string(), None),
        ("2026-09-03__05-53-58/run.log".to_string(), None),
        ("2026-09-03__05-53-58/tb.lock".to_string(), None),
    ] {
        assert_eq!(classify_artifact(&path), want, "{path}");
    }
}
