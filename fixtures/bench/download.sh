#!/bin/sh
# §12 fixture fetch. **Resumable, digest-pinned, and the corpus is never
# committed** — `fixtures/bench/*/` is gitignored.
#
# τ2-bench is the one §12 source that publishes third-party trajectories with
# third-party oracle labels (`reward_info`), which is what makes it class 1.
# 90 result files at 30–50 MB is 3–4.5 GB, so this fetches a named subset by
# default and takes `--all` for the rest.
set -eu
BASE="https://raw.githubusercontent.com/sierra-research/tau2-bench/main/data/tau2/results/final"
DIR="$(cd "$(dirname "$0")" && pwd)/tau2"
mkdir -p "$DIR"

# file <name> <sha256>. The digest is the class-1 evidence: a corpus whose bytes
# do not match is not the published corpus, and the loader refuses it.
fetch() {
  name=$1; want=$2
  out="$DIR/$name"
  if [ -f "$out" ]; then
    have=$(shasum -a 256 "$out" | cut -d' ' -f1)
    [ "$have" = "$want" ] && { echo "ok       $name"; return 0; }
    echo "redo     $name (have $have)"
  fi
  # `-C -` resumes a partial file; a 40 MB fetch that dies halfway should not
  # start over, and should never be left half-written and read as complete.
  curl -fsSL -C - -o "$out.part" "$BASE/$name"
  mv "$out.part" "$out"
  have=$(shasum -a 256 "$out" | cut -d' ' -f1)
  if [ "$have" != "$want" ]; then
    echo "DIGEST MISMATCH $name: got $have want $want" >&2
    exit 1
  fi
  echo "fetched  $name"
}

fetch o4-mini-2025-04-16_airline_default_gpt-4.1-2025-04-14_4trials.json \
      6a3a354c9bce42b52905af096e9cea6f555548acbb17b567fd403c356df7b4ec
fetch gpt-4.1-2025-04-14_airline_default_gpt-4.1-2025-04-14_4trials.json \
      31d19e79b8ddc5934a60701f913ff5fefb41dee5e68f7b813c26254ab2e2d5c8
fetch claude-3-7-sonnet-20250219_airline_default_gpt-4.1-2025-04-14_4trials.json \
      40a2c6a246eab27db5cdefda895fbe7f44be78a23d300817248534aa606d66de
fetch o4-mini-2025-04-16_telecom_no-user-op_gpt-4.1-2025-04-14_4trials.json \
      e4f0e4827bdd36438f6daf80ceecffe7f9fd175cef80ae1f821422cb4e5743c4
# ---------------------------------------------------------------------------
# §12 class 2: task sources. These publish TASKS, not trajectories — which is
# what class 2 is for. Nothing here is a corpus until a run under a committed
# protocol produces one.
# ---------------------------------------------------------------------------

# Terminal-Bench. Pinned to a COMMIT, not a branch: `main` is a moving target
# and a task set that changes under a committed protocol is a protocol that
# committed to nothing. Measured at this commit: 241 tasks, 241 `run-tests.sh`
# oracles, 241 `parser_name: pytest`, and **0 of 241 task directories are git
# repositories** — which is why the protocol's `git init` precondition exists.
TB_DIR="$(cd "$(dirname "$0")" && pwd)/terminal-bench"
TB_COMMIT="d28711d0da2675d0bb1d56de45ae5df6082438a3"
TB_SHA256="7850bccaa5ff25ff11cb88dcebedceea49136e362bb80c9639deffeabd339081"
fetch_terminal_bench() {
  mkdir -p "$TB_DIR"
  out="$TB_DIR/terminal-bench-1-$(printf %.8s "$TB_COMMIT").tar.gz"
  if [ -f "$out" ]; then
    have=$(shasum -a 256 "$out" | cut -d' ' -f1)
    [ "$have" = "$TB_SHA256" ] && { echo "ok       terminal-bench @ $TB_COMMIT"; return 0; }
    echo "redo     terminal-bench (have $have)"
  fi
  curl -fsSL -C - -o "$out.part" \
    "https://codeload.github.com/harbor-framework/terminal-bench-1/tar.gz/$TB_COMMIT"
  mv "$out.part" "$out"
  have=$(shasum -a 256 "$out" | cut -d' ' -f1)
  if [ "$have" != "$TB_SHA256" ]; then
    echo "DIGEST MISMATCH terminal-bench: got $have want $TB_SHA256" >&2
    exit 1
  fi
  echo "fetched  terminal-bench @ $TB_COMMIT (126 MB)"
}
fetch_terminal_bench

# ImpossibleBench, the second class-2 task source. Its tasks are impossible by
# construction, so a pass IS a cheat and the label is the task's own.
#
# The dataset is `fjzzq2002/impossible_swebench`. It is NOT
# `safety-research/impossiblebench` — that is the harness, which this project's
# own probe recorded as publishing zero tasks-as-data.
#
# THREE subsets, and the third one matters: `conflicting` and `oneoff` are the
# impossible variants, and `original` is the UNMODIFIED CONTROL. A pass on an
# `original` task is honest work, not a cheat. The loader reads each row's own
# `impossible_type` for exactly this reason; fetching the control alongside the
# two impossible sets is what makes that check meaningful rather than decorative.
IB_DIR="$(cd "$(dirname "$0")" && pwd)/impossiblebench"
IB_REV="9c2d34f364b7229e8c0ff807c646100bdc18bbb5"
fetch_impossible() {
  name=$1; want=$2
  mkdir -p "$IB_DIR"
  out="$IB_DIR/$name.parquet"
  if [ -f "$out" ]; then
    have=$(shasum -a 256 "$out" | cut -d' ' -f1)
    [ "$have" = "$want" ] && { echo "ok       impossible_swebench/$name"; return 0; }
    echo "redo     impossible_swebench/$name (have $have)"
  fi
  curl -fsSL -C - -o "$out.part" \
    "https://huggingface.co/datasets/fjzzq2002/impossible_swebench/resolve/$IB_REV/data/$name-00000-of-00001.parquet"
  mv "$out.part" "$out"
  have=$(shasum -a 256 "$out" | cut -d' ' -f1)
  if [ "$have" != "$want" ]; then
    echo "DIGEST MISMATCH impossible_swebench/$name: got $have want $want" >&2
    exit 1
  fi
  echo "fetched  impossible_swebench/$name"
}
fetch_impossible conflicting 25d96f864431f9279375c94df05ffdaeeafbcacde0be8b2b0a0333daefa4186d
fetch_impossible oneoff      6308b6fa360213dc40a5bb5e39217efcc440f48c76b8caaf00bbf3497cb67462
fetch_impossible original    4ad148f9812e9a347ec529690bff8fed49b8ec7d0256e3b6768d375fed499f3b

echo
echo "Fetched into $DIR. The corpus is NOT committed: a benchmark corpus in git"
echo "is 4 GB of blobs nobody can gc, and this repository has already paid that"
echo "bill once."
