//! The harness driver — the gap that made the pilot unrunnable.
//!
//! `REPORT.md` documented steps 1–5, 7 and 8 of the class-2 sequence. Step 6 —
//! *run the agent and collect what it produced* — was never built, so
//! [`crate::pilot::PilotTask`] was a struct nothing filled. A paid run would
//! have reported zero for every coverage measure, because nothing was there to
//! report.
//!
//! # §0.2 decides the capture route, so this module encodes the decision
//!
//! Terminal-Bench writes per-trial `agent-logs/`, `commands.txt`, `panes/` and
//! `sessions/`. Ingesting those needs no container-side install, and that is
//! exactly why it is tempting. But it is **post-hoc harness output** — the
//! harness's own account of what it did — and §0.2 classes that as *a hint*,
//! not *the execution record*. The whole product exists because those two are
//! different.
//!
//! So the route is not a runtime choice:
//!
//! - **Container-side capture is the primary path.** ThruLine runs inside the
//!   task image, its spool lands on a mounted volume, and the fold reaches it on
//!   the host.
//! - **Harness artifacts are corroboration only.** They are recorded with their
//!   provenance and **can never be the sole basis for a bound check** — which is
//!   enforced by [`TaskEvidence`] being unable to express it, not by a rule
//!   someone remembers.
//!
//! # What the base images actually contain
//!
//! Measured, not assumed: **neither dominant base image has `git`**.
//! `python-3-13:20250620` (88 tasks) and `ubuntu-24-04:20250624` (97 tasks) —
//! 185 of 241 — both report `git: ABSENT`, and both have `apt-get`. So the
//! operator's `git init` precondition needs an **install step before it is even
//! possible**, which is a stronger statement than the one the 0-of-241 count
//! supported.

use std::collections::BTreeMap;

use crate::pilot::{PilotTask, TaskCost};

/// Where a piece of evidence came from. **Not interchangeable.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Route {
    /// ThruLine's own capture, from inside the container: hooks and the `PATH`
    /// shim, folded from a mounted spool. **The execution record.**
    ContainerCapture,
    /// Terminal-Bench's own per-trial output. **A hint**, in §0.2's sense.
    HarnessArtifact,
}

impl Route {
    pub fn as_str(self) -> &'static str {
        match self {
            Route::ContainerCapture => "container-capture",
            Route::HarnessArtifact => "harness-artifact",
        }
    }

    /// May a check bound to a git tree rest on this?
    ///
    /// Only the execution record. A harness artifact describing a command is
    /// the harness saying what it believes it ran, which is the thing §6 exists
    /// not to trust.
    pub fn can_bind_a_check(self) -> bool {
        matches!(self, Route::ContainerCapture)
    }
}

/// What ThruLine captured from inside the container.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerCapture {
    pub hook_events: u64,
    pub shim_exec_events: u64,
    /// Checks executed **and** bound to a git tree.
    pub checks_bound_to_a_tree: u64,
    /// The tree hash recorded before the agent's first action.
    pub tree_before_first_action: Option<String>,
}

/// One of Terminal-Bench's own per-trial files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessArtifact {
    /// `agent-logs`, `commands.txt`, `panes`, `sessions`, …
    pub kind: String,
    pub path: String,
    pub bytes: u64,
}

/// Everything one task produced, **with the two routes kept apart**.
///
/// This is the ruling as a type. `container` is `Option` and `corroboration` is
/// a `Vec`, and no amount of the second produces any of the first: a task with
/// a hundred harness artifacts and no container capture reports **zero** for
/// every coverage measure, because that is the truth about what §6 can read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskEvidence {
    pub task_id: String,
    pub container: Option<ContainerCapture>,
    pub corroboration: Vec<HarnessArtifact>,
    pub terminal_state: String,
    pub cost: Option<TaskCost>,
}

impl TaskEvidence {
    /// Builds the pilot's per-task record.
    ///
    /// **Every coverage count comes from `container` alone.** `corroboration`
    /// reaches the report as provenance and nothing else.
    pub fn into_pilot_task(self) -> PilotTask {
        let c = self.container.unwrap_or_default();
        PilotTask {
            task_id: self.task_id,
            hook_events: c.hook_events,
            shim_exec_events: c.shim_exec_events,
            checks_bound_to_a_tree: c.checks_bound_to_a_tree,
            tree_before_first_action: c.tree_before_first_action,
            cost: self.cost,
            terminal_state: self.terminal_state,
        }
    }

    /// The corroborating artifacts, by kind, for the report.
    pub fn corroboration_census(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for a in &self.corroboration {
            *out.entry(a.kind.clone()).or_insert(0) += 1;
        }
        out
    }

    /// Did this task produce the execution record at all?
    pub fn has_execution_record(&self) -> bool {
        self.container.is_some()
    }
}

/// The image layer that makes container-side capture possible.
///
/// Generated rather than hand-written per task: 241 tasks with 6 distinct base
/// images is not a place for a copied `RUN` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureLayer {
    /// Where the spool is mounted from the host.
    pub spool_mount: String,
    /// The harness whose hooks are installed.
    pub harness: String,
    /// The builder image tag, matching `rust-toolchain.toml`.
    pub rust_toolchain: String,
}

/// The multi-stage builder's stage name.
pub const BUILDER_STAGE: &str = "thruline-builder";

impl Default for CaptureLayer {
    fn default() -> Self {
        CaptureLayer {
            spool_mount: "/thruline-spool".to_string(),
            harness: "claude-code".to_string(),
            rust_toolchain: "1.97-bookworm".to_string(),
        }
    }
}

impl CaptureLayer {
    /// The **builder stage**, prepended before the task's own `FROM`.
    ///
    /// A multi-stage build rather than cross-compilation: cross-compiling from
    /// macOS needs a linux builder to provision or a toolchain to keep working,
    /// and both are things that break silently later. This needs nothing
    /// outside Docker.
    ///
    /// **glibc, and the OLDEST target's glibc.** Measured, after the obvious
    /// version failed:
    ///
    /// | image | glibc |
    /// |---|---|
    /// | `rust:1.97` (trixie) | **2.41** |
    /// | `rust:1.97-bookworm` | 2.36 |
    /// | `python-3-13:20250620` | **2.36** ← oldest target |
    /// | `ubuntu-24-04:20250624` | 2.39 |
    ///
    /// glibc is backward compatible, not forward: a binary built against 2.41
    /// will not start on 2.36. Building on the *oldest* target — bookworm's
    /// 2.36 — produces one that runs on both. The unqualified `rust:1.97` tag
    /// tracks the newest Debian and produced exactly that failure:
    /// `libc.so.6: version 'GLIBC_2.39' not found`.
    ///
    /// An Alpine builder would be worse still: a **dynamically linked** musl
    /// binary cannot run in a Debian image at all, failing as a container that
    /// starts and does nothing.
    ///
    /// **THE QUALIFIER IS LOAD-BEARING AND WAS MISSING UNTIL 2026-09-05.** This
    /// said "a musl binary", unqualified, and that is false for a *statically*
    /// linked one. Measured, not reasoned: a `--target
    /// aarch64-unknown-linux-musl` build with no `PT_INTERP` segment
    /// (`readelf -l` reports 0) runs `--version` in `debian:bookworm-slim` and
    /// in `ghcr.io/laude-institute/t-bench/python-3-13:20250620`, which is the
    /// exact image five instrument failures were on.
    ///
    /// **This paragraph governs THIS artefact only — the in-image multi-stage
    /// builder — and the rule it states is still right for it.** That build
    /// happens inside each task image and links against that image's glibc, so
    /// an Alpine stage here would produce a binary for the wrong libc in the
    /// one place the libc is not ours to choose. It is a different artefact
    /// from the release binaries `scripts/pilot-run.sh` bind-mounts, which ARE
    /// static musl and are selected per architecture inside the container. The
    /// distinction is written down because the unqualified sentence reads as
    /// forbidding the very binary the pilot now mounts.
    ///
    /// The version matches `rust-toolchain.toml`: the pin is part of the
    /// byte-identical rebuild contract, and a builder on a different compiler is
    /// a different binary.
    pub fn builder_stage(&self) -> String {
        format!(
            "# --- ThruLine builder (generated) ---\n\
             # Multi-stage, not cross-compilation: nothing outside Docker to keep\n\
             # working. Debian-based on purpose — every task base image is\n\
             # Debian/Ubuntu, and a DYNAMICALLY linked musl binary would not run\n\
             # in them. A statically linked one would; this stage is glibc\n\
             # because it links against the task image's own libc, not because\n\
             # musl cannot reach these images. See CaptureLayer::builder_stage.\n\
             FROM rust:{toolchain} AS {stage}\n\
             WORKDIR /src\n\
             COPY . /src\n\
             # --no-default-features: `default = [\"ort\"]` pulls ONNX Runtime, which\n\
             # downloads a binary from a CDN AT BUILD TIME. The container captures;\n\
             # it never annotates. Leaving the default on would add a network\n\
             # dependency to 241 image builds for a model none of them runs.\n\
             RUN cargo build --release --locked --no-default-features -p thruline-cli\n\
             # --- end ThruLine builder ---\n\n",
            toolchain = self.rust_toolchain,
            stage = BUILDER_STAGE,
        )
    }

    /// The whole Dockerfile: builder, then the task's own content, then the
    /// capture layer.
    ///
    /// Composed rather than hand-edited per task — 241 tasks across six base
    /// images is not a place for a copied `RUN` line.
    pub fn compose(&self, task_dockerfile: &str) -> String {
        format!(
            "{}{}\n{}",
            self.builder_stage(),
            task_dockerfile.trim_end(),
            self.dockerfile()
        )
    }

    /// The Dockerfile fragment appended to a task's own image.
    ///
    /// Three things, in order, and the order matters:
    ///
    /// 1. **`git`**, because neither dominant base image has it — measured, not
    ///    assumed. Without this the precondition cannot even be attempted.
    /// 2. **ThruLine**, with its hooks and `PATH` shim installed *inside* the
    ///    container, spooling to a mounted volume so the fold reaches it on the
    ///    host. Host-side capture observes nothing about a process in a
    ///    container.
    /// 3. **`git init` plus an initial commit**, so a tree hash exists **before
    ///    the agent's first action**. `git init` alone leaves an unborn
    ///    repository with no tree, which would satisfy the letter of the
    ///    precondition and none of its purpose.
    pub fn dockerfile(&self) -> String {
        format!(
            "\n# --- ThruLine capture layer (generated) ---\n\
             # `git` is installed because NEITHER dominant base image has it:\n\
             # python-3-13:20250620 and ubuntu-24-04:20250624 both report absent,\n\
             # which is 185 of the 241 tasks. Measured, not assumed.\n\
             RUN (command -v git >/dev/null 2>&1) || \\\n\
             \x20   (apt-get update && apt-get install -y --no-install-recommends git \\\n\
             \x20    && rm -rf /var/lib/apt/lists/*)\n\
             COPY --from={builder} /src/target/release/thruline /usr/local/bin/thruline\n\
             RUN mkdir -p {mount} && chmod 0777 {mount}\n\
             ENV THRULINE_HOME={mount}\n\
             # Hooks and the PATH shim, INSIDE the container. Host-side capture\n\
             # observes nothing about a process running in one. The verbs are\n\
             # `hook-install` and `shim install` -- checked against the CLI, not\n\
             # guessed: an earlier draft of this layer invoked `thruline init\n\
             # --harness ... --scope ...`, and `init` has neither flag.\n\
             RUN thruline hook-install --harness {harness} --scope user\n\
             RUN thruline shim install\n\
             ENV PATH={mount}/shim:$PATH\n\
             # A tree hash must exist BEFORE the agent's first action. `git init`\n\
             # alone leaves an unborn repository with no tree at all.\n\
             RUN git init -q -b main . \\\n\
             \x20 && git config user.email thruline@bench.invalid \\\n\
             \x20 && git config user.name 'ThruLine bench' \\\n\
             \x20 && git add -A \\\n\
             \x20 && git commit -qm 'bench baseline' --allow-empty \\\n\
             \x20 && git rev-parse HEAD^{{tree}} > {mount}/tree-before-first-action\n\
             # --- end ThruLine capture layer ---\n",
            mount = self.spool_mount,
            harness = self.harness,
            builder = BUILDER_STAGE,
        )
    }

    /// Adds the spool mount to a task's `docker-compose.yaml`.
    ///
    /// # Feasible, and the harness already shows how
    ///
    /// Terminal-Bench's per-task compose file **already** bind-mounts two host
    /// paths into the container:
    ///
    /// ```yaml
    /// volumes:
    ///   - ${T_BENCH_TASK_LOGS_PATH}:${T_BENCH_CONTAINER_LOGS_PATH}
    ///   - ${T_BENCH_TASK_AGENT_LOGS_PATH}:${T_BENCH_CONTAINER_AGENT_LOGS_PATH}
    /// ```
    ///
    /// So container-side capture is **unbuilt rather than infeasible** — the
    /// mechanism is the one the harness already uses, and the spool is a third
    /// entry in a list that has two.
    ///
    /// **A separate mount, deliberately.** The spool could ride in the existing
    /// agent-logs directory, and it must not: that is where the *harness* writes
    /// its own account, and §0.2's boundary between the execution record and a
    /// hint should not depend on two things being told apart inside one
    /// directory. They do not share a mount.
    ///
    /// A literal host path rather than a `${VAR}`: an unset variable
    /// interpolates to empty and yields a mount of the container root, which
    /// fails in a way nobody would read as a missing environment variable.
    pub fn compose_yaml(&self, task_compose: &str, host_spool_dir: &str) -> String {
        let mut out = String::new();
        let mut added = false;
        for line in task_compose.lines() {
            out.push_str(line);
            out.push('\n');
            if !added && line.trim() == "volumes:" {
                out.push_str(&format!(
                    "      # ThruLine's spool: the EXECUTION RECORD, on its own mount so it\n\
                     \x20     # never shares a directory with the harness's own account.\n\
                     \x20     - {host_spool_dir}:{}\n",
                    self.spool_mount
                ));
                added = true;
            }
        }
        out
    }

    /// The `-v` argument the run needs so the spool survives the container.
    pub fn volume_arg(&self, host_dir: &str) -> String {
        format!("{host_dir}:{}", self.spool_mount)
    }
}

/// One `tb run` invocation, built rather than interpolated at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TbRun {
    pub agent: String,
    pub task_id: String,
    pub model: Option<String>,
    pub output_dir: String,
    pub dataset_path: String,
}

impl TbRun {
    /// **The zero-cost validation run.** `oracle` replays the task's own
    /// solution: it exercises the container build, the workspace, `git init`,
    /// the shim, the fold and the oracle, and makes **no model calls at all**.
    ///
    /// Everything except the hook path is provable this way, which is why the
    /// hook path is the last thing bought rather than the first.
    pub fn oracle(task_id: &str, output_dir: &str, dataset_path: &str) -> TbRun {
        TbRun {
            agent: "oracle".to_string(),
            task_id: task_id.to_string(),
            model: None,
            output_dir: output_dir.to_string(),
            dataset_path: dataset_path.to_string(),
        }
    }

    /// The argv. `model` is required for a paying agent and forbidden for
    /// `oracle`, which has nothing to bill.
    pub fn argv(&self) -> Vec<String> {
        let mut v = vec![
            "run".to_string(),
            "--agent".to_string(),
            self.agent.clone(),
            "--task-id".to_string(),
            self.task_id.clone(),
            "--dataset-path".to_string(),
            self.dataset_path.clone(),
            "--output-path".to_string(),
            self.output_dir.clone(),
        ];
        if let Some(m) = &self.model {
            v.push("--model".to_string());
            v.push(m.clone());
        }
        v
    }

    /// Does this invocation spend money?
    pub fn costs_money(&self) -> bool {
        self.agent != "oracle" && self.agent != "null"
    }
}

/// Where Terminal-Bench writes a trial's own artifacts.
///
/// Enumerated so the corroboration census names them rather than globbing:
/// an artifact kind nobody anticipated should show up as unrecognised rather
/// than be silently folded into a known one.
pub const HARNESS_ARTIFACT_KINDS: &[&str] = &[
    "agent-logs",
    "commands.txt",
    "panes",
    "sessions",
    "results.json",
];

/// Classifies a trial artifact path. `None` means unrecognised — **reported,
/// never dropped**.
pub fn classify_artifact(path: &str) -> Option<&'static str> {
    HARNESS_ARTIFACT_KINDS
        .iter()
        .find(|k| path.contains(*k))
        .copied()
}
