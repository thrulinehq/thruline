//! `thruline gate --hook stop` — BUILD-SPEC §4.1's Stop gate (M3.4).
//!
//! > *on `Stop`, run claim extraction + verifiers synchronously with a 5 s
//! > budget; if policy requires evidence for a claim and it is `unsupported`,
//! > return the documented block decision … Respect `stop_hook_active` to avoid
//! > infinite loops: block at most `max_stop_blocks` (default 2) per session.*
//!
//! # It blocks on `unsupported` and **only** on `unsupported`
//!
//! [`blocks`] matches `Verdict::Unsupported` alone. Widening it to
//! `!Supported` — which is the obvious "be safe" edit, and the one a future
//! reader will reach for — would make the gate block on **`unverifiable`**, and
//! that is §6 inverted: `unverifiable` means ThruLine could not observe enough
//! to decide, so blocking on it accuses the agent where the product cannot
//! tell. Refusing to answer is not evidence of a lie.
//!
//! A repository with no exec capture would then block every stop, forever, for
//! reasons it cannot substantiate — and the fix a user would reach for is
//! uninstalling. This used to cite a `gate_predicate.rs` that has never
//! existed; the test that holds the line is
//! `m3_stop_gate::p2_unverifiable_never_blocks_and_the_same_harness_does_block`,
//! and it is behavioural — widen [`blocks`] and it goes red on a real session.
//!
//! # The budget is enforced by a watchdog, not by hope
//!
//! §4.1 gives the gate 5 s. A gate that *hangs* is worse than a gate that
//! abstains: the agent stops responding and the user's only recourse is to kill
//! it. So a watchdog thread starts first, and on expiry prints nothing and
//! exits 0 — **allow** — whatever the verification path is doing. The budget is
//! therefore a property of the process, not of the code paths inside it, and it
//! holds for a path nobody anticipated.
//!
//! # Fail open, absolutely — but not silently
//!
//! Every error is an allow. A malformed payload, an unreadable ledger, a fold
//! that fails, a repository that is not a repository — exit 0, print no
//! decision. The gate exists to stop an unsupported completion claim, not to
//! stop work.
//!
//! **One of those allows used to be invisible, and it was the dangerous one.**
//! [`History::save`] wrote the block ledger with `let _ = …`, so a
//! `$THRULINE_HOME` the gate cannot write left the count permanently at zero
//! and `max_stop_blocks` bounding nothing: measured **6 blocks out of 6
//! consecutive stops against a 1/6 control**, with the harness's own cap of 8
//! as the only remaining floor. A block that cannot be counted cannot be
//! bounded, so it is not emitted at all — and the reason goes to stderr, where
//! `--debug` and `doctor` can see it, rather than nowhere.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{json, Value};
use thruline_verify::verdict::Verdict;

/// §4.1's `max_stop_blocks`, inside the harness's own cap of 8.
pub const DEFAULT_MAX_STOP_BLOCKS: u32 = 2;

/// §4.1's 5 s budget.
pub const DEFAULT_BUDGET_MS: u64 = 5_000;

pub struct Args {
    /// Which hook is calling. Only `stop` does anything.
    pub hook: String,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub max_stop_blocks: u32,
    /// The watchdog budget, **from the command line**. `None` is the default.
    ///
    /// Separate from `THRULINE_GATE_BUDGET_MS` on purpose — see
    /// [`budget_ms`]. An argument is authored; an environment variable is
    /// inherited.
    pub budget_ms: Option<u64>,
}

/// How long the watchdog waits, and why the environment cannot shorten it.
///
/// # F13 — the gate's own budget disarmed it
///
/// Track R's reproduction, which worked:
///
/// ```text
/// $TL gate --hook stop < stop.json | jq -r .decision   # -> "block"
/// rm -f "$THRULINE_HOME/gate/"*.blocks                 # clear the per-claim dedup
/// THRULINE_GATE_BUDGET_MS=0 $TL gate --hook stop < stop.json
/// # -> (empty stdout == allow): the watchdog fires first and the process
/// #    exits 0 having written nothing
/// ```
///
/// The read was `.parse().ok().unwrap_or(DEFAULT_BUDGET_MS)` — **any**
/// parseable value was honoured, `0` included, and a zero budget means the
/// watchdog wins every race. The gate stayed installed, exited 0, and stopped
/// gating.
///
/// # The rule now: the environment may only LENGTHEN
///
/// The budget exists to protect the **user** from a gate that hangs. It was
/// never meant to let the thing being gated choose how long it is looked at.
/// So a value from the environment is floored at [`DEFAULT_BUDGET_MS`]: a
/// slower machine can ask for more time, and nothing can ask for less.
///
/// # Why the command line is not the same hole
///
/// `--budget-ms` is honoured exactly, and that is deliberate rather than an
/// oversight. **An environment variable is inherited; a command line is
/// authored.** The gate's argv is written once by `thruline init` into
/// `.claude/settings.json`; an attacker who can rewrite that has already won
/// more cheaply by pointing the hook at `true`, which no budget floor can stop
/// — that is Track R's F7, and D18 is what catches its consequence. An
/// attacker who can only export a variable into an unmodified hook's
/// environment is stopped here.
///
/// A shortening request is **refused out loud** on stderr rather than ignored,
/// because a silently-clamped value is how the next person concludes the
/// variable works and builds on it.
pub fn budget_ms(args: &Args) -> u64 {
    if let Some(ms) = args.budget_ms {
        return ms;
    }
    let Some(raw) = std::env::var("THRULINE_GATE_BUDGET_MS").ok() else {
        return DEFAULT_BUDGET_MS;
    };
    let Some(asked) = raw.parse::<u64>().ok() else {
        eprintln!(
            "thruline gate: THRULINE_GATE_BUDGET_MS={raw:?} is not a number; using the \
             {DEFAULT_BUDGET_MS} ms default"
        );
        return DEFAULT_BUDGET_MS;
    };
    if asked < DEFAULT_BUDGET_MS {
        eprintln!(
            "thruline gate: THRULINE_GATE_BUDGET_MS={asked} would shorten the gate's budget \
             below §4.1's {DEFAULT_BUDGET_MS} ms and is REFUSED. The environment may lengthen \
             this budget, never shorten it: a zero budget makes the watchdog win every race, \
             which leaves the gate installed, exiting 0, and gating nothing (Track R, F13)."
        );
        return DEFAULT_BUDGET_MS;
    }
    asked
}

/// **Does this verdict block?**
///
/// `Unsupported` only. See the module docs for why `!Supported` is the wrong
/// answer.
pub fn blocks(verdict: Verdict) -> bool {
    matches!(verdict, Verdict::Unsupported)
}

/// Run the gate. **Always exits 0**; the decision is on stdout.
pub fn run(args: Args, stdin: &mut impl Read) -> ExitCode {
    // The watchdog goes up before anything else can be slow.
    let budget = budget_ms(&args);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(budget));
        // Allow: print nothing, exit 0. A gate that hangs is worse than one
        // that abstains.
        std::process::exit(0);
    });

    if args.hook != "stop" {
        return ExitCode::SUCCESS;
    }
    let mut raw = Vec::new();
    if stdin.read_to_end(&mut raw).is_err() {
        return ExitCode::SUCCESS;
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&raw) else {
        return ExitCode::SUCCESS;
    };
    match decide(&payload, &args) {
        Some(reason) => {
            // B.1, verbatim: `decision` is TOP-LEVEL on Stop, with `reason`
            // beside it. Exit 0 with this JSON on stdout is the documented
            // block; exit 2 routes the same way, and is deliberately not used
            // because it also prints a hook-error notice.
            println!("{}", json!({ "decision": "block", "reason": reason }));
            ExitCode::SUCCESS
        }
        None => ExitCode::SUCCESS,
    }
}

/// The whole decision, separated from I/O so it is testable.
///
/// `None` is allow.
fn decide(payload: &Value, args: &Args) -> Option<String> {
    let session = payload.get("session_id").and_then(Value::as_str)?;
    let repo = args
        .repo
        .clone()
        .or_else(|| {
            payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from)
        })
        .or_else(|| std::env::current_dir().ok())?;

    // The ledger, opened BEFORE any work, because the block history is in it
    // now and a session that has spent its budget must still be cheap to let
    // through. Opening SQLite is milliseconds; the fold below is the expensive
    // part and it stays after the early return.
    let db = match &args.exec_db {
        Some(p) => p.clone(),
        None => match thruline_ledger::Ledger::default_path() {
            Ok(p) => p,
            Err(e) => return allow_because(&format!("no execution store here ({e})")),
        },
    };
    let ledger = match thruline_ledger::Ledger::open(&db) {
        Ok(l) => l,
        Err(e) => return allow_because(&format!("{} could not be opened ({e})", db.display())),
    };

    // The block ledger, read BEFORE any work.
    //
    // A read that FAILS is not a fresh history. A corrupt `.blocks` file used to
    // read as `count: 0`, which is the state that makes `max_stop_blocks` bound
    // nothing — and since G2 the same rule covers a chain that does not verify.
    // Both are an allow with a named reason instead.
    let history = match History::load(&ledger, session) {
        Ok(h) => h,
        Err(why) => return allow_because(&why),
    };
    if history.count() >= args.max_stop_blocks {
        return None;
    }
    // `stop_hook_active` is the harness saying it is already continuing because
    // of a stop hook. It is a signal, not the guard — the guard is the budget
    // above, because the harness sets this for ANY stop hook on the machine,
    // not only ours.
    let already_continuing = payload
        .get("stop_hook_active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if already_continuing && history.count() == 0 {
        // Someone else's stop hook is driving the loop and we have blocked
        // nothing. Adding a second reason to continue is not this gate's job.
        return None;
    }

    // Fold first, so the gate judges the session that just happened rather than
    // the one the last fold saw. Failure is an allow.
    let spool = crate::shim_cmd::thruline_home()
        .ok()
        .map(|h| h.join("spool").join("claude-code.jsonl"));
    if let Some(spool) = spool.filter(|p| p.is_file()) {
        let key = repo.to_string_lossy().to_string();
        // **A fold that fails is a reason, not a shrug.** This was
        // `let _ = fold(…)`, and it hid the case this whole test family exists
        // for: a store the gate cannot WRITE folds nothing, so the Stop that
        // just arrived is not in the ledger, so no claim is extracted, so the
        // gate allows — silently, having judged a session that predates the
        // stop it was called about. Measured when the block ledger moved into
        // that same store and `a_block_the_gate_cannot_write_down_…` went from
        // six named reasons to none.
        //
        // The decision is unchanged: still an allow. What changes is that it
        // says why, which is the difference the module docs already draw
        // between failing open and failing open silently.
        match ledger.repo(&key, Some(&key)) {
            Ok(repo_id) => {
                if let Err(e) =
                    thruline_capture::fold::fold(&spool, &ledger, &repo_id, Some(repo.as_path()))
                {
                    return allow_because(&format!(
                        "the spool could not be folded ({e}), so this stop is not in the                          ledger and the gate would be judging an older session"
                    ));
                }
            }
            Err(e) => {
                return allow_because(&format!(
                    "this repository has no namespace in the ledger and one could not be                      created ({e})"
                ))
            }
        }
    }

    let events = ledger.session_events(session).ok()?;
    let view = crate::claims_cmd::session_view(session, &events, &ledger, &repo).ok()?;
    let judged = crate::claims_cmd::judge(session, &events, &view, &repo).ok()?;

    let blocking: Vec<_> = judged
        .iter()
        .filter(|(_, record)| blocks(record.verdict))
        .collect();

    // **Exactly once per claim** (§17's done-when, literally). A claim is
    // identified by what the agent said — its kind and its text — because a
    // claim id is derived from the `stop` event it was extracted from, and a
    // second `Stop` delivery is a second event. Keying on the id would let the
    // gate block the same restated sentence forever on unchanged evidence,
    // which burns the agent's turns and teaches the user to uninstall.
    let (claim, record) = blocking
        .iter()
        .find(|(claim, _)| !history.already_blocked(claim.kind.as_str(), &claim.text))?;

    let mut history = history;
    // The block is emitted only if it was recorded. See the module docs: an
    // unrecorded block repeats on every stop until the harness stops it.
    if let Err(why) = history.record(&ledger, session, claim.kind.as_str(), &claim.text) {
        return allow_because(&why);
    }
    Some(reason_for(
        claim.kind.as_str(),
        &claim.text,
        &record.reasons,
        blocking.len(),
    ))
}

/// Fail open, out loud.
///
/// Returns `None` (allow) and says on **stderr** why the gate stood down. Not
/// stdout: stdout is the harness's decision channel and anything printed there
/// that is not a decision is a protocol error. Exit stays 0 either way.
fn allow_because(why: &str) -> Option<String> {
    eprintln!(
        "thruline gate: allowing this stop — {why}. A block this gate cannot count is a block \
         it cannot bound, so none was issued."
    );
    None
}

/// The message the agent reads.
///
/// §4.1 asks for a reason "telling the agent exactly which claim lacks evidence
/// and what command would produce it". The vocabulary is CLAUDE.md's and
/// A199's: *"claimed, and no proof found in the record"* — never "lie", and
/// never an accusation the record cannot carry.
fn reason_for(kind: &str, text: &str, reasons: &[String], total: usize) -> String {
    let remedy = match kind {
        "tests_pass" => {
            "Run the full suite at the current tree — e.g. `pytest -q` or `cargo test` \
             with no `-k`/`--filter` narrowing — and let it finish before you stop."
        }
        "build_ok" => "Run the build at the current tree (e.g. `cargo build`, `npm run build`).",
        "lint_ok" => "Run the linter at the current tree with no `--exit-zero`.",
        "typecheck_ok" => "Run the type checker at the current tree (e.g. `tsc --noEmit`, `mypy`).",
        _ => "Run the command that would produce evidence for it at the current tree.",
    };
    let detail = if reasons.is_empty() {
        String::new()
    } else {
        format!(" ({})", reasons.join("; "))
    };
    let more = if total > 1 {
        format!(" {} other claim(s) are also unsupported.", total - 1)
    } else {
        String::new()
    };
    format!(
        "ThruLine: you claimed \"{text}\" ({kind}), and no proof was found in the \
         execution record{detail}.{more} {remedy} If the claim is wrong, say so instead \
         of restating it."
    )
}

/// What this session's gate has already done.
///
/// # G2 — the count was a free integer in an agent-writable file
///
/// Track R's reproduction, which worked:
///
/// ```text
/// $TL gate --hook stop < stop.json | jq -r .decision            # fresh -> "block"
/// printf '{"count":9999,"claims":[]}' > $THRULINE_HOME/gate/$SID.blocks
/// $TL gate --hook stop < stop.json                              # -> (empty == allow)
/// ```
///
/// The history lived in a loose JSON file with no integrity of any kind, and
/// `count` was read straight out of it. Deriving the count from the `claims`
/// beside it defeated that one line and nothing more: a file anything running
/// as the user can write cannot be authenticated.
///
/// **It is now the `gate_blocks` table** — append-only by SQLite trigger, hash
/// chained, and verified on every read
/// ([`thruline_ledger::Ledger::gate_blocks`]). The loose file is gone; nothing
/// reads or writes it.
///
/// # The property, stated exactly
///
/// **Forgery is DETECTABLE, not prevented.** An agent that could write the old
/// file can also write `exec.db`; SQLite is not a permission boundary and the
/// gate runs as the same user. What changed is that a forgery must now either
/// produce a valid chain — which needs the hash of every block before it — or
/// leave one that does not verify, and this gate reads the chain rather than
/// believing the rows.
///
/// And the gate is **fail-open by design** (§4.1), so a detected tamper still
/// ends in an allow for that stop. What it no longer ends in is *silence*: it
/// is an allow with a stated reason on stderr, where `--debug` and `doctor`
/// read it. Saying "forgery prevented" here would be the exact failure this
/// product exists to catch.
///
/// # The one-time consequence of the move
///
/// A session that was mid-flight when this build landed starts from an empty
/// history, because the old file is not read. It can therefore be blocked up to
/// `max_stop_blocks` times more than it otherwise would. That is bounded, it
/// errs toward blocking rather than toward allowing, and the harness's own cap
/// of 8 is unchanged. Reading the old file as a floor was rejected: a forged
/// file could then still add claims to `already_blocked` and suppress a block,
/// which is the bypass with the sign flipped.
#[derive(Debug, Default)]
struct History {
    claims: Vec<String>,
}

impl History {
    /// `Ok(empty)` means *no blocks yet*. `Err` means *this gate cannot tell* —
    /// which is not the same thing, and conflating them is what let the
    /// bookkeeping fail open into an unbounded loop.
    fn load(ledger: &thruline_ledger::Ledger, session: &str) -> Result<History, String> {
        match ledger.gate_blocks(session) {
            Ok(blocks) => Ok(History {
                claims: blocks.into_iter().map(|b| b.claim_key).collect(),
            }),
            Err(e) => Err(e.to_string()),
        }
    }

    /// §4.1's `max_stop_blocks` counter, **derived** rather than stored.
    ///
    /// One recorded block is one block. Keeping a separate integer meant the
    /// two could disagree, and G2 is what happens when they do: the integer
    /// wins and nobody notices, because nothing ever compared them.
    fn count(&self) -> u32 {
        u32::try_from(self.claims.len()).unwrap_or(u32::MAX)
    }

    /// A claim's identity, as the agent stated it.
    fn key(kind: &str, text: &str) -> String {
        thruline_core::chain::sha256_hex(format!("{kind}|{text}").as_bytes())
    }

    fn already_blocked(&self, kind: &str, text: &str) -> bool {
        self.claims.contains(&History::key(kind, text))
    }

    /// Append this block to the chain, or say exactly why it could not.
    fn record(
        &mut self,
        ledger: &thruline_ledger::Ledger,
        session: &str,
        kind: &str,
        text: &str,
    ) -> Result<(), String> {
        let key = History::key(kind, text);
        ledger
            .record_gate_block(session, &key, kind)
            .map_err(|e| format!("the block could not be recorded ({e})"))?;
        self.claims.push(key);
        Ok(())
    }
}

/// The `Stop` hook entry `--gate` installs.
pub fn hook_entry(exe: &Path) -> Value {
    hook_entry_with(exe, "")
}

/// [`hook_entry`] with global flags spliced between the binary and the verb
/// (`" --db '/x'"`), or `""` for none. The command still ENDS with
/// `gate --hook stop`, which is how every recogniser of this entry finds it.
pub fn hook_entry_with(exe: &Path, global: &str) -> Value {
    json!({
        "hooks": [{
            "type": "command",
            "command": format!("{}{global} gate --hook stop", exe.display()),
            // Explicit, so the failure is loud rather than the harness's
            // default 600 s. The gate's own watchdog is the real bound; this is
            // the harness's independent one.
            "timeout": 10
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unsupported_blocks() {
        assert!(blocks(Verdict::Unsupported));
        // The two that must not. `Unverifiable` is the important one: blocking
        // on it accuses where the product cannot tell.
        assert!(!blocks(Verdict::Unverifiable));
        assert!(!blocks(Verdict::Supported));
    }

    #[test]
    fn the_two_numbers_44_1_states_are_the_two_numbers_shipped() {
        // §4.1: "a 5 s budget" and "block at most `max_stop_blocks` (default
        // 2)". Both are constants a refactor can change without any behavioural
        // test noticing — the integration suite drives the budget through
        // `THRULINE_GATE_BUDGET_MS` and the cap through `--max-stop-blocks`
        // precisely so it runs fast, which means neither one reads the default.
        assert_eq!(DEFAULT_BUDGET_MS, 5_000, "§4.1's budget is 5 s");
        assert_eq!(DEFAULT_MAX_STOP_BLOCKS, 2, "§4.1's default is 2");
        // And the harness's independent bound must sit OUTSIDE our own, or the
        // harness kills the gate before its watchdog can allow the stop.
        let entry = hook_entry(Path::new("/opt/thruline"));
        let timeout = entry["hooks"][0]["timeout"]
            .as_u64()
            .expect("the gate entry states a timeout");
        assert!(
            timeout * 1_000 > DEFAULT_BUDGET_MS,
            "the harness timeout ({timeout}s) does not outlast the gate's own budget \
             ({DEFAULT_BUDGET_MS}ms), so the harness would kill the gate mid-decision"
        );
    }

    #[test]
    fn the_reason_names_the_claim_and_a_command() {
        let r = reason_for(
            "tests_pass",
            "All tests pass.",
            &["1/2 passed".to_string()],
            1,
        );
        assert!(r.contains("All tests pass."), "{r}");
        assert!(r.contains("tests_pass"), "{r}");
        assert!(r.contains("pytest"), "{r}");
        assert!(r.contains("1/2 passed"), "{r}");
        // The vocabulary is binding (CLAUDE.md / A199): never an accusation.
        for forbidden in ["lie", "lied", "lying", "false", "dishonest"] {
            assert!(
                !r.to_lowercase()
                    .split_whitespace()
                    .any(|w| { w.trim_matches(|c: char| !c.is_alphanumeric()) == forbidden }),
                "forbidden vocabulary {forbidden:?} in {r:?}"
            );
        }
    }
}
