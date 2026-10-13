//! Every blocking wait in the workspace is enumerated, with a reason.
//!
//! # Why an allowlist, after arguing against a census
//!
//! I swept for unbounded waits by hand, classified `main.rs`'s self-test
//! fixture as "the accept loops are servers", and declined to write a census
//! because the distinction — waiting on a condition *the product controls*
//! versus on a process the *user* launched — is a judgement a substring walk
//! cannot make.
//!
//! The premise was right and the conclusion was the wrong branch. It is not a
//! server: it is a two-request fixture, joined synchronously by its caller, and
//! it hung `thruline` with no diagnosis whenever the child opened fewer than two
//! connections. The sweep that was looking for exactly that family walked past
//! it.
//!
//! The third option is an **allowlist census**: the walk *detects* every
//! blocking site, and a human *judges* each one exactly once by writing its
//! line here. A new site fails the build until someone classifies it. That asks
//! the walk to do only what a walk can do, and it is the pattern this repository
//! already runs four times — `construction_census.rs`, `ok_basis_census.rs`,
//! `temporal_language_census.rs`, `truncation_sweep.rs`.
//!
//! **It found two more sites on its first run** — `gate_cmd.rs` and the poll
//! interval inside `annotate.rs` — that the hand sweep had not listed. Both
//! turned out to be bounded, and one of them *is* the Stop gate's watchdog. That
//! is the point: the walk found them, a human judged them, and the judgement is
//! now written down instead of being re-derived from memory next time.

use std::path::{Path, PathBuf};

/// `(file, symbol, how many, why each is bounded)`.
///
/// **The count is load-bearing.** Keying on `(file, symbol)` alone made a
/// *second* `thread::sleep` in an already-listed file invisible — measured, by
/// injecting one into `main.rs` and watching this test stay green. An allowlist
/// that cannot tell one site from two is not an allowlist. Adding a blocking
/// call now changes a count and fails until a human updates the line.
const BLOCKING_SITES: &[(&str, &str, usize, &str)] = &[
    ("core/src/cargo_fetch.rs", "sleep", 1,
     "trusted compiler metadata polling is bounded to two seconds, followed by at most 500 ms of kill/reap polling; refusal preserves owned-child reaping if the OS does not report exit within that bound"),
    ("cli/src/check.rs", "sleep", 1,
     "each foreground-child polling sleep is bounded to 20 ms during the owned fetch and requested check; every iteration observes cancellation, then cleanup uses the existing stop bound plus 500 ms; the user command has no imposed execution deadline"),
    ("cli/src/decide_cmd.rs", "sleep", 1,
     "bounded: supervised inline-worker polling sleeps 5 ms and checks the shared INLINE_BUDGET (20 s) plus an 8 MiB output limit; expiry kills the owned process group and reaps its leader"),
    ("cli/src/uiserve/observation.rs", "sleep", 1,
     "explicit foreground observation cadence; 100 ms intervals check shutdown and bounded child execution has a 15-second timeout"),
    // SNAPSHOT-ORPHAN-ON-STOP: output polls retain the shared deadline;
    // shutdown and per-child Drop drain for at most 2 s. A detached reaper
    // retains ownership if the kernel keeps a killed child uninterruptible.
    ("core/src/bounded_child.rs", "recv_timeout", 1,
     "bounded: output polls at most 10 ms while observing shutdown and the caller's 20 s deadline"),
    ("core/src/bounded_child.rs", "sleep", 4,
     "bounded shutdown and Drop drains (2 s), deadline-bounded exit polling, and an ownership-retaining detached reaper after that bound, which cannot hold its caller"),
    ("core/src/store_hold.rs", "recv_timeout", 1,
     "the removal hold heartbeat has a bounded five-second receive interval; a stop message or channel disconnect interrupts the wait, and Drop sends stop before joining the heartbeat"),
    ("capture/src/bounded.rs", "recv_timeout", 2,
     "stdout and stderr share one 500 ms drain deadline after the bounded child exits"),
    ("capture/src/bounded.rs", "sleep", 4,
     "capture polls to its bounded accepted deadline; ps observation polls for at most 150 ms; two test-only sites wait 150 ms for cancellation and poll cleanup to a fixed deadline"),
    ("cli/src/uiserve.rs", "recv_timeout", 2,
     "the exited status child's stdout and stderr each have a fixed 500 ms drain timeout"),
    ("cli/src/uiserve.rs", "sleep", 3,
     "status projection is bounded to 30 seconds; listener polling ends on shutdown; owned check cleanup drains for at most 10 seconds"),
    ("cli/src/main.rs", "recv_timeout", 1,
     "the cached Decisions Stop line has a 50 ms receive bound that the environment may only lengthen (THRULINE_STOP_ADVISORY_BUDGET_MS, capped at 2 s); silence follows expiry, and the hook also has its existing 3.5 s watchdog"),
    ("core/src/decisions/checks_visual.rs", "recv_timeout", 1,
     "exact-version UI server startup is bounded by EXACT_SERVER_START_TIMEOUT (30 minutes); timeout kills and reaps the owned child"),
    ("core/src/decisions/checks_visual.rs", "sleep", 1,
     "exact-record export polls every 250 ms to the same fixed 30-minute bound; timeout kills and reaps the owned child"),
    ("core/src/decisions/checks_k_fixture_lookup.rs", "recv_timeout", 1,
     "the bounded K6 Git reader receives at most its byte limit plus one to a fixed five-second deadline; expiry refuses the source read and kills the owned Git child"),
    ("core/src/decisions/checks_k_fixture_lookup.rs", "sleep", 1,
     "read-only Git exit polling is bounded by GIT_TIMEOUT (five seconds), with one-millisecond sleeps; expiry refuses the source read and kills the owned child"),
    ("core/src/decisions/bridge.rs", "sleep", 3,
     "one terminal answer poll is bounded by its fresh challenge deadline (20 ms cadence); two owned OS child polls have caller bounds of two seconds for account checks and 100 seconds for presence, then kill/reap and refuse"),
    ("core/src/decisions/checks_k_assertions.rs", "recv_timeout", 1,
     "bounded Git output receive uses the smaller of five seconds and the caller's remaining assertion-check deadline; excessive bytes or timeout kill/reap the owned reader child"),
    ("core/src/decisions/checks_k_assertions.rs", "sleep", 1,
     "one-millisecond Git exit polling is bounded by the same receive/check timeout; expiry kills/reaps its owned child and returns no assertion proof"),
    ("core/src/decisions/speaker.rs", "sleep", 1,
     "one-millisecond owned Git polling is bounded by cached_line's single deadline (50 ms by default); expiry kills/reaps the child and the advisory speaker stays silent"),
    ("core/src/decisions/visual_cargo_target.rs", "sleep", 2,
     "shared compiler-lock polling is bounded to five seconds; the second cadence monitors the owned build's mount identity, disk stop floor and output bound every 100 ms, then stops/reaps on refusal; it imposes no deadline on the user's compiler"),
    ("core/src/decisions/visual_process.rs", "sleep", 1,
     "ten-millisecond capture polling is bounded by the caller's capture deadline and output/progress limits; expiry stops the owned process group and refuses the capture"),
    ("core/src/decisions/visual_record.rs", "sleep", 1,
     "ten-millisecond native snapshot-writer polling is bounded by one shared help/upgrade deadline, at most 30 minutes; timeout or output-limit refusal stops/reaps its owned process group"),
    ("core/src/decisions/visual_static.rs", "sleep", 1,
     "owned nonblocking listener cadence is bounded to five milliseconds between shutdown checks; capture owns the server lifetime, Drop signals stop and joins it, and each client read/write has a two-second timeout"),
    // T-3-4: stopping a run's process group and then VERIFYING nothing of it
    // survived. Both waits are deadline-bounded by `stop_group`'s own
    // arguments: 50 ms polls to the caller's `STOP_BOUND` (5 s from HQ's
    // shutdown path) before SIGKILL, then 25 ms polls to a fixed 500 ms
    // deadline after it, at which point the record says how many survived
    // rather than the product waiting longer.
    // P6-5: the run route's guard tests poll the route until a REAL process
    // the wrapper started has ended. Test-only, and each poll has a fixed
    // deadline: 60 s for a run to end, 20 s for its first record.
    ("cli/src/uiserve/tests/run_route_guards.rs", "sleep", 6,
     "test-only: bounded polls every 25-50 ms until a run the wrapper started has ended (fixed 60 s deadline) or has written its first record (fixed 20 s deadline), then fails loudly; ADV-run-security-5's wait_until polls every 25 ms to the bound its caller names (at most 20 s), then returns false for the caller to fail on; C30 R-RUN: a poll every 25 ms to a fixed 30 s deadline for the run sheet to say what the test waits for, then fails loudly, and two fixed 2.2 s and 1.1 s waits so a recorded elapsed time can be seen to move and then to stop"),
    // ADV-run-security-5: a request whose guard might wait on a file the
    // repository controls is waited for on a thread, to a fixed bound.
    ("cli/src/uiserve/tests/run_route_guards.rs", "recv_timeout", 2,
     "test-only: a route request is waited for to a fixed 20 s bound; past it, a reader blocked on the test's FIFO is released and the test fails; and a sheet whose question HQ's stop ended is waited for to a fixed 20 s bound"),
    // ADV-run-security-5: the question the run route puts to rustup.
    ("cli/src/uiserve/run_route.rs", "sleep", 1,
     "bounded: the rustup question polls every 10 ms to a fixed RUSTUP_DEADLINE (4 s); past that bound the question's process group is killed and reaped, and the run is refused"),
    ("cli/src/run_supervisor.rs", "sleep", 2,
     "process-group cleanup polls to the caller's stop bound before SIGKILL and to a fixed 500 ms deadline after it; a group that outlives both is RECORDED as having survivors, never waited on further"),
    ("cli/src/integration_cmd/container.rs", "recv_timeout", 1,
     "container metadata stdout has a bounded one-second drain deadline"),
    ("cli/src/integration_cmd/container.rs", "sleep", 1,
     "metadata/cleanup process polling has a five-second deadline and kills/reaps on timeout"),
    ("cli/src/integration_cmd/worker.rs", "recv_timeout", 2,
     "stdout and stderr each have a bounded one-second drain deadline after owned-group cleanup"),
    ("cli/src/integration_cmd/worker.rs", "sleep", 3,
     "main 20 ms polls stop at the accepted plan timeout; graceful cancellation has a separate 900 ms deadline; observed descendant cleanup polls for at most six seconds and otherwise reports capture failure"),
    ("cli/src/integration_cmd/native_repair.rs", "sleep", 1,
     "bounded owned-container cleanup polls to a ten-second deadline or one-MiB output bound, then kills/reaps and returns an explicit unaccounted-job error"),
    // T-4-1a-FIX-R3: the review door's model probe (`probe_review_model`)
    // itself has no sleep and no recv: connect, write and every read each take
    // a timeout of what is LEFT of one REVIEW_MODEL_PROBE_BOUND (2 s) deadline,
    // and it gives up when nothing is left. The two new sleeps and the one
    // recv_timeout are its tests' stand-ins, which the tests hold to 2.6 s.
    ("cli/src/integration_cmd/review.rs", "sleep", 3,
     "one production: Git source inventory polls every 20 ms to a ten-second deadline; timeout kills/reaps its owned child and refuses the inventory. Two test-only, each inside a detached stand-in thread the probe never joins: the silent accepter's fixed 5 s sleep and the slow answerer's 100 ms per byte of a 208-byte answer (at most 20.8 s, ending at the first failed write once the probe has closed); the probe that faces them is bounded by REVIEW_MODEL_PROBE_BOUND (2 s) and the tests fail if it takes 2.6 s"),
    ("cli/src/integration_cmd/review.rs", "recv_timeout", 1,
     "test-only: after the one probe request has been received, a wait bounded by a fixed 300 ms timeout for a second request that must NOT arrive"),
    ("cli/src/integration_cmd/worker_broker_container.rs", "sleep", 1,
     "bounded owned Docker namespace metadata/cleanup commands poll to a five-second deadline, then kill/reap and return an explicit cleanup error"),
    ("cli/src/integration_cmd/worker_broker_container.rs", "recv_timeout", 2,
     "Docker stdout/stderr drains each have a one-second deadline after the bounded metadata/cleanup command exits"),
    ("cli/src/uiserve/integration_actions.rs", "sleep", 2,
     "transport polls to a bounded 360-second deadline; Drop waits at most 100 times 20 ms before kill and reap"),
    (
        "sigstore/src/net/oidc.rs",
        "sleep",
        1,
        "RFC 8628 §3.5's device-flow poll interval, in Track B's vendored crate. \
         Bounded by the server's own `expires_in` — staging answers 300 s with \
         an `interval` of 5, recorded in fixtures/net/oidc-staging-device-code.json \
         — and `device_flow` hands the URL and user code to its caller BEFORE it \
         sleeps, so the five minutes are the person's, not the process's. The \
         bound is the server's answer and B's module docs state it; enumerated \
         here by the core at the vendoring, because this list is the core's and \
         no track can reach it",
    ),
    (
        "core/src/annotate.rs",
        "wait_before_retry",
        3,
        "three: the trait method, its production impl, and the call site. The \
         loop is bounded by language.memory_pause_timeout_secs, and on expiry \
         the candidate is Uncertain with a stated reason",
    ),
    (
        "core/src/annotate.rs",
        "sleep",
        1,
        "the 250 ms poll interval inside that wait: a fixed duration, and the \
         loop around it is bounded by the timeout above",
    ),
    (
        "cli/src/gate_cmd.rs",
        "sleep",
        1,
        "IS the Stop gate's watchdog: a fixed sleep of THRULINE_GATE_BUDGET_MS \
         that then fires the timeout. Bounded by construction — it is the bound",
    ),
    (
        "reexec/src/docker.rs",
        "sleep",
        1,
        "bounded by started + req.timeout, and it kills the container",
    ),
    (
        "cli/src/main.rs",
        "sleep",
        2,
        "two: the self-test fixture's accept poll, bounded by \
         SELF_TEST_ACCEPT_TIMEOUT; and `speaker_watchdog`'s fixed 3.5 s before \
         `exit(0)`, which IS the bound — it is the hook self-timeout this \
         project cites as its pattern. The second was found by this census, not \
         by the hand sweep",
    ),
    (
        "bench/src/pilot.rs",
        "sleep",
        1,
        "the backoff inside `LedgerLock::acquire`, waiting for another \
         `pilot-budget charge` to release the spend ledger. It is bounded by \
         LedgerLock::WAIT, a 10 s timeout on expiry of which the charge is \
         REFUSED as \
         BudgetStop::NotRecorded naming the lock file — it never proceeds \
         without the lock, because two writers is how the ledger gets shredded",
    ),
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .to_path_buf()
}

/// Shipped source only: `src/` trees, with inline `#[cfg(test)]` blocks cut.
fn shipped() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let root = workspace();
    let mut files = Vec::new();
    for c in std::fs::read_dir(&root).expect("crates/").flatten() {
        let src = c.path().join("src");
        if src.is_dir() {
            walk(&src, &mut files);
        }
    }
    files.sort();
    assert!(files.len() > 30, "the walk found {} files", files.len());
    files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&p).expect("read");
            (rel, strip_test_modules(&text))
        })
        .filter(|(name, _)| !name.ends_with("tests.rs"))
        .collect()
}

/// Removes `#[cfg(test)]` blocks by matching braces, keeping everything after.
///
/// **Truncating at the first `#[cfg(test)]` was wrong**, and this census caught
/// it on itself: `main.rs` has inline test modules long before the site this
/// file exists for, so cutting at the first one made the walk blind to
/// thousands of lines of shipped code — the same "the search space is wrong"
/// failure this project has now hit six times. The `BLOCKING_SITES`-must-exist
/// assertion is what surfaced it.
fn strip_test_modules(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        // A `#[cfg(test)] mod tests;` declaration has no block to skip.
        let Some(open) = after.find('{') else {
            // The prefix was already appended. Do not append it a second
            // time when the file ends in an out-of-line test declaration.
            rest = "";
            break;
        };
        if after[..open].contains(';') {
            rest = &after[after.find(';').expect("checked") + 1..];
            continue;
        }
        let bytes = after.as_bytes();
        let mut depth = 0usize;
        let mut end = None;
        for (i, b) in bytes.iter().enumerate().skip(open) {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(e) => rest = &after[e..],
            None => {
                // This lexical helper does not parse Rust string literals.
                // If braces inside one prevent matching, retain the entire
                // unparsed region conservatively, but never duplicate the
                // already-appended shipped prefix.
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The calls that make a thread wait on something outside itself.
///
/// Deliberately narrow: these are *sleeps and retry waits*, the shape that
/// spins until an external condition changes. `child.wait()` and `join()` are
/// **not** here — they block on a process this code started, which ends on its
/// own, and putting a deadline on a user's own test command would cap how long
/// their suite may take.
const BLOCKING_CALLS: &[&str] = &["thread::sleep(", "wait_before_retry(", "recv_timeout("];

#[test]
fn every_blocking_wait_is_enumerated_with_a_reason() {
    use std::collections::BTreeMap;
    let mut allowed: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (f, sym, n, _) in BLOCKING_SITES {
        allowed.insert((f, sym), *n);
    }
    let mut found: BTreeMap<(String, String), usize> = BTreeMap::new();

    for (name, text) in shipped() {
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            for call in BLOCKING_CALLS {
                if t.contains(call) {
                    let sym = call
                        .trim_end_matches('(')
                        .rsplit("::")
                        .next()
                        .unwrap_or(call);
                    *found.entry((name.clone(), sym.to_string())).or_insert(0) += 1;
                }
            }
        }
    }

    // Positive control: the walk sees the sites we know about.
    assert!(
        found.len() >= 3,
        "the walk found {} blocking sites; it is not looking at the source",
        found.len()
    );

    let mut problems: Vec<String> = Vec::new();
    for ((file, sym), n) in &found {
        match allowed.get(&(file.as_str(), sym.as_str())) {
            None => problems.push(format!("{file}::{sym} ({n}) is not listed at all")),
            Some(want) if want != n => problems.push(format!(
                "{file}::{sym} has {n} occurrence(s); BLOCKING_SITES says {want}"
            )),
            Some(_) => {}
        }
    }
    for (file, sym, n, why) in BLOCKING_SITES {
        if !found.contains_key(&((*file).to_string(), (*sym).to_string())) {
            problems.push(format!(
                "BLOCKING_SITES lists {file}::{sym}, which the walk cannot find"
            ));
        }
        assert!(*n > 0, "{file}::{sym} is listed with a count of zero");
        assert!(
            why.contains("bounded")
                || why.contains("timeout")
                || why.contains("TIMEOUT")
                || why.contains("bound"),
            "{file}::{sym} is listed without naming its bound: {why}"
        );
    }

    assert!(
        problems.is_empty(),
        "blocking-wait census:\n  {}\n\n\
         A wait on something outside this process needs a deadline, and a human \
         has to say what bounds it. Add or update the line in BLOCKING_SITES. The \
         site this census exists for — `main.rs`'s self-test fixture — hung \
         `thruline` with no diagnosis and survived a hand sweep looking for it.",
        problems.join("\n  ")
    );
}

/// The bound on the site this census exists for, asserted in the source: an
/// accept loop with no deadline is what hung `thruline`.
#[test]
fn the_self_test_fixture_bounds_its_accept() {
    let src = std::fs::read_to_string(workspace().join("cli/src/main.rs")).expect("main.rs");
    assert!(
        src.contains("SELF_TEST_ACCEPT_TIMEOUT"),
        "the self-test fixture's accept loop lost its deadline"
    );
    assert!(
        src.contains("listener.set_nonblocking(true)"),
        "a blocking accept cannot observe a deadline"
    );
    let at = src
        .find("SELF_TEST_ACCEPT_TIMEOUT: std::time::Duration")
        .expect("const");
    let decl = &src[at..at + 120];
    assert!(decl.contains("from_secs("), "{decl}");
    // **Named is not checked.** Replacing the comparison with `if false` left
    // the constant in place and this test green — measured. The deadline has to
    // be compared against the clock, in the loop, for it to be a deadline.
    let loop_at = src
        .find("listener.set_nonblocking(true)")
        .expect("accept loop");
    let body = &src[loop_at..loop_at + 2000];
    assert!(
        body.contains("std::time::Instant::now() >= deadline"),
        "the accept loop names a timeout it never compares against the clock"
    );
    assert!(
        body.contains("WouldBlock"),
        "the accept loop is not polling, so the deadline cannot be observed"
    );
}

/// The stripper keeps shipped code that follows a test module. Without this the
/// walk was blind to everything in `main.rs` after its first inline `mod tests`,
/// which is where the site this census exists for lives.
#[test]
fn the_test_module_stripper_does_not_truncate_the_file() {
    let text = "\
fn shipped_before() { std::thread::sleep(x); }
#[cfg(test)]
mod tests {
    fn in_a_test() { std::thread::sleep(y); }
}
fn shipped_after() { std::thread::sleep(z); }
";
    let out = strip_test_modules(text);
    assert!(out.contains("shipped_before"), "{out}");
    assert!(
        out.contains("shipped_after"),
        "the walk truncated at the test module: {out}"
    );
    assert!(
        !out.contains("in_a_test"),
        "the test module survived: {out}"
    );

    // And an out-of-line declaration is skipped without eating the file.
    let decl = "fn a() {}\n#[cfg(test)]\nmod tests;\nfn b() {}\n";
    let out = strip_test_modules(decl);
    assert!(out.contains("fn a()") && out.contains("fn b()"), "{out}");
}

#[test]
fn a_terminal_out_of_line_test_declaration_does_not_duplicate_shipped_waits() {
    let source = "fn a() { std::thread::sleep(x); }\n#[cfg(test)]\nmod tests;\n";
    assert_eq!(
        strip_test_modules(source).matches("thread::sleep(").count(),
        1
    );
    let unmatched = "fn a() { std::thread::sleep(x); }\n#[cfg(test)]\nmod tests { const JSON: &str = \"{\"; }\n";
    let kept = strip_test_modules(unmatched);
    assert_eq!(kept.matches("thread::sleep(").count(), 1);
    assert!(
        kept.contains("const JSON"),
        "an unparsed region must not disappear"
    );
}
