//! A89 — the stdio-direct MCP server.
//!
//! No daemon, no port, no resident process, no tokio. A short-lived process
//! speaking JSON-RPC 2.0 over stdin/stdout, invoking the engine directly. The
//! protocol is hand-rolled against `serde_json` rather than pulled from a
//! crate, because the alternative brings an async runtime this project has
//! deliberately not spent.
//!
//! **Stateless between calls.** Nothing is cached in the process. Every tool
//! call opens the ledger, does its work, and drops it, so a kill between two
//! calls is invisible to the client.
//!
//! **The call is ledgered, but not by this file.** `Binding.digest` folds
//! `compute_project_head` over every event in a project, so writing an event
//! per call would move the chain position the packet had just issued — the gate
//! would report "not current" for a position the agent received milliseconds
//! ago, purely because it asked. Instead the *client's own transcript* ledgers
//! the call: Claude Code writes its `mcp__*` tool_use/tool_result lines, and
//! ingest turns them into real events with real file+line+sha256 receipts. This
//! module records the server's side of the same call in the append-only
//! `mcp_calls` table, which is bound to that event afterwards — never at call
//! time, because when the tool runs the transcript line does not exist yet.
//!
//! **No anonymous calls.** `agent` is required at construction; there is no
//! default and no fallback.

use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::chain::sha256_hex;
use crate::config::Config;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::packet::{repo_scope, Binding, Budget, Scope};

pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// One recorded call. Append-only; there is no update path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord {
    pub call_id: String,
    pub agent: String,
    pub tool: String,
    pub scope: String,
    pub obligation: Option<String>,
    pub gated_chain_position: Option<String>,
    pub latest_chain_position: Option<String>,
    pub request_sha256: String,
    pub manifest_json: Option<String>,
    pub manifest_sha256: Option<String>,
    pub outcome: String,
    pub accepted_at_unix_ms: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Content-address for a call.
///
/// Time and a nonce are folded in **on purpose**. An id over (agent, tool,
/// position, request) alone collides for exactly the calls that matter most:
/// two `check_context` polls that both answer "current" are byte-identical by
/// construction, as is a re-fetched packet at an unchanged position. A
/// colliding id plus an idempotent insert would silently discard the second
/// call, and "every call is recorded" would quietly become false. This is the
/// same split the capture layer already makes: content addresses omit time for
/// idempotent *facts*, and include it for *actions taken at a time*.
fn call_id(agent: &str, tool: &str, request_sha256: &str, at_ms: i64, nonce: &str) -> String {
    let buf = format!(
        "a89-mcp-call-v1\n{}:{agent}\n{}:{tool}\n{}:{request_sha256}\n{at_ms}\n{}:{nonce}\n",
        agent.len(),
        tool.len(),
        request_sha256.len(),
        nonce.len()
    );
    sha256_hex(buf.as_bytes())
}

fn canonical(value: &Value) -> String {
    // serde_json's Map is ordered by insertion unless the `preserve_order`
    // feature is off, in which case it is a BTreeMap and already sorted. This
    // project does not enable preserve_order, so `to_string` is canonical.
    serde_json::to_string(value).unwrap_or_default()
}

/// Resolve the scope named by a tool call.
fn scope_from(ledger: &Ledger, args: &Value) -> Result<Scope> {
    if let Some(p) = args.get("project").and_then(Value::as_str) {
        return Ok(Scope::Project(p.to_string()));
    }
    if let Some(r) = args.get("repo").and_then(Value::as_str) {
        return repo_scope(ledger, r);
    }
    Err(CoreError::Config(
        "give either `project` or `repo` — a packet is always scoped".to_string(),
    ))
}

/// One call's worth of record fields. A struct rather than ten positional
/// arguments, so a future field cannot be silently transposed with its
/// neighbour at a call site.
struct Entry<'a> {
    tool: &'a str,
    scope: &'a str,
    args: &'a Value,
    outcome: &'a str,
    gated: Option<String>,
    latest: Option<String>,
    obligation: Option<String>,
    manifest: Option<&'a Value>,
}

/// The server. Holds only the agent identity and where the ledger lives —
/// never a connection, never a cached packet.
pub struct Server {
    agent: String,
    db: std::path::PathBuf,
}

impl Server {
    /// `agent` is required. There is no anonymous caller.
    pub fn new(agent: &str, db: std::path::PathBuf) -> Result<Server> {
        if agent.trim().is_empty() {
            return Err(CoreError::Config(
                "--agent is required: an unattributed call is not recordable".to_string(),
            ));
        }
        Ok(Server {
            agent: agent.trim().to_string(),
            db,
        })
    }

    fn open(&self) -> Result<Ledger> {
        Ledger::open(&self.db)
    }

    /// Record one call; returns the call id so a response can hand the
    /// claimant the handle `thruline why` answers at.
    fn record(&self, ledger: &mut Ledger, entry: Entry<'_>) -> Result<String> {
        let Entry {
            tool,
            scope,
            args,
            outcome,
            gated,
            latest,
            obligation,
            manifest,
        } = entry;
        let request_sha256 = sha256_hex(canonical(args).as_bytes());
        let at = now_ms();
        let nonce = ulid::Ulid::generate().to_string();
        let manifest_json = manifest.map(canonical);
        let record = CallRecord {
            call_id: call_id(&self.agent, tool, &request_sha256, at, &nonce),
            agent: self.agent.clone(),
            tool: tool.to_string(),
            scope: scope.to_string(),
            obligation,
            gated_chain_position: gated,
            latest_chain_position: latest,
            request_sha256,
            manifest_sha256: manifest_json.as_deref().map(|m| sha256_hex(m.as_bytes())),
            manifest_json,
            outcome: outcome.to_string(),
            accepted_at_unix_ms: at,
        };
        ledger.record_mcp_call(&record)?;
        Ok(record.call_id)
    }

    /// The tool list, exactly three.
    pub fn tools() -> Value {
        json!([
            {
                "name": "get_context_packet",
                "description": "Compile the deterministic Context Packet for a project or repo. \
                                Returns the packet and the chain position it is bound to; that \
                                position IS the snapshot id for the other two tools. Pass \
                                `handoff_of` with a frozen issue id to receive that issue's \
                                retained content verbatim as `handoff`, and to record this \
                                packet as its successor.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {"type": "string"},
                        "repo": {
                            "type": "string",
                            "description": "A repo path, or a repo id from `identity groups`. \
                                            Spans every working root of that repo, across \
                                            vendors. An id nothing carries is an error, not an \
                                            empty answer."
                        },
                        "task": {"type": "string"},
                        "budget_tokens": {"type": "integer"},
                        "handoff_of": {"type": "string"}
                    }
                }
            },
            {
                "name": "check_context",
                "description": "Is this chain position still current? Returns current/not-current \
                                and the latest position. Deltas are not in v1 — a not-current \
                                answer means refetch the packet.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {"type": "string"},
                        "repo": {
                            "type": "string",
                            "description": "A repo path, or a repo id from `identity groups`. \
                                            Spans every working root of that repo, across \
                                            vendors. An id nothing carries is an error, not an \
                                            empty answer."
                        },
                        "chain_position": {"type": "string"}
                    },
                    "required": ["chain_position"]
                }
            },
            {
                "name": "claim_completion",
                "description": "Submit a completion claim with its evidence manifest. \
                                Snapshot-gated: a stale chain position is rejected with the \
                                latest. Accepted submissions are recorded as READY FOR \
                                VERIFICATION. This tool never reports work as done — the \
                                verdict belongs to the verifier (`thruline verify-claims`), \
                                which later checks each manifest entry against the claiming \
                                session's own tool record and the workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {"type": "string"},
                        "repo": {
                            "type": "string",
                            "description": "A repo path, or a repo id from `identity groups`. \
                                            Spans every working root of that repo, across \
                                            vendors. An id nothing carries is an error, not an \
                                            empty answer."
                        },
                        "obligation": {"type": "string"},
                        "chain_position": {"type": "string"},
                        "manifest": {
                            "type": "object",
                            "description": "Evidence the verifier can check deterministically: \
                                            `tests`/`commands_run` (array of test or build \
                                            commands that ran green), `files`/`files_changed` \
                                            (array of paths, or {path, sha256} objects), \
                                            `commit`/`commits` (commit ids). Other keys are \
                                            recorded verbatim but are not checkable, and an \
                                            uncheckable manifest can never be marked supported."
                        }
                    },
                    "required": ["obligation", "chain_position", "manifest"]
                }
            }
        ])
    }

    pub fn call(&self, tool: &str, args: &Value) -> Result<Value> {
        match tool {
            "get_context_packet" => self.get_context_packet(args),
            "check_context" => self.check_context(args),
            "claim_completion" => self.claim_completion(args),
            other => Err(CoreError::Config(format!("unknown tool: {other}"))),
        }
    }

    fn get_context_packet(&self, args: &Value) -> Result<Value> {
        let mut ledger = self.open()?;
        let scope = scope_from(&ledger, args)?;
        let budget = Budget {
            tokens: args
                .get("budget_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(100_000) as usize,
        };
        let packet = crate::packet::compile_request(
            &ledger,
            &scope,
            args.get("task").and_then(Value::as_str),
            &budget,
            &Config::default(),
        )?;
        let position = packet.binding.digest.clone();
        // A handoff delivers the PREDECESSOR's frozen bytes — never a
        // recompilation, which would hand the successor a different document
        // than the predecessor was given.
        let predecessor = args.get("handoff_of").and_then(Value::as_str);
        let handoff = match predecessor {
            Some(issue) => crate::packet::handoff(&ledger, issue)?,
            None => None,
        };
        // Retain what we hand over: the delta engine diffs against the receipt,
        // not against a reconstruction of what we believe we served.
        packet.retain_after(&mut ledger, Some(&self.agent), now_ms(), predecessor)?;
        self.record(
            &mut ledger,
            Entry {
                tool: "get_context_packet",
                scope: &scope_label(&scope),
                args,
                outcome: "served",
                gated: None,
                latest: Some(position.clone()),
                obligation: None,
                manifest: None,
            },
        )?;
        Ok(json!({
            "packet_md": packet.render(),
            "handoff": handoff,
            "continues_from": predecessor,
            // The chain position IS the snapshot id. There is no second identity.
            "chain_position": position,
            "tokens": packet.tokens,
            "excluded": packet.exclusions.len(),
        }))
    }

    fn check_context(&self, args: &Value) -> Result<Value> {
        let mut ledger = self.open()?;
        let scope = scope_from(&ledger, args)?;
        let given = args
            .get("chain_position")
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Config("chain_position is required".to_string()))?;
        // The binding alone — NOT a full compile. Deriving is the expensive
        // half, and the gate does not need it.
        let answer = crate::packet::context_since(&ledger, &scope, given, &Config::default())?;
        let (current, latest_digest, outcome) = match &answer {
            crate::packet::ContextAnswer::Current { position } => {
                (true, position.clone(), "current")
            }
            crate::packet::ContextAnswer::Changed { latest, .. } => {
                (false, latest.clone(), "not_current")
            }
            crate::packet::ContextAnswer::UnknownPosition { latest } => {
                (false, latest.clone(), "not_current")
            }
        };
        let latest = Binding {
            digest: latest_digest,
            heads: Vec::new(),
        };
        let _ = outcome;
        self.record(
            &mut ledger,
            Entry {
                tool: "check_context",
                scope: &scope_label(&scope),
                args,
                outcome: if current { "current" } else { "not_current" },
                gated: Some(given.to_string()),
                latest: Some(latest.digest.clone()),
                obligation: None,
                manifest: None,
            },
        )?;
        Ok(match answer {
            crate::packet::ContextAnswer::Current { position } => json!({
                "status": "current",
                "current": true,
                "latest_chain_position": position,
                "deltas": [],
                "note": "the packet you hold is still bound to the current chain position",
            }),
            crate::packet::ContextAnswer::Changed {
                latest,
                lines,
                served_by,
            } => json!({
                "status": "changed",
                "current": false,
                "latest_chain_position": latest,
                "deltas": lines.iter().map(delta_json).collect::<Vec<_>>(),
                "served_by": served_by
                    .iter()
                    .map(|(p, s)| json!({"project": p, "path": s.as_str()}))
                    .collect::<Vec<_>>(),
                "note": "these are the lines that differ from the packet you were served; \
                         you do not need to refetch the whole packet",
            }),
            crate::packet::ContextAnswer::UnknownPosition { latest } => json!({
                "status": "unknown_position",
                "current": false,
                "latest_chain_position": latest,
                "deltas": Value::Null,
                // Not "nothing changed" — we have no record of serving that
                // position, so there is nothing to diff against and saying
                // otherwise would be a guess.
                "note": "no packet was served at that position, so the change set cannot be \
                         computed; refetch the packet",
            }),
        })
    }

    fn claim_completion(&self, args: &Value) -> Result<Value> {
        let mut ledger = self.open()?;
        let scope = scope_from(&ledger, args)?;
        let scope_name = scope_label(&scope);

        let obligation = args.get("obligation").and_then(Value::as_str);
        let position = args.get("chain_position").and_then(Value::as_str);
        let manifest = args.get("manifest");

        let mut missing = Vec::new();
        if obligation.is_none() {
            missing.push("obligation");
        }
        if position.is_none() {
            missing.push("chain_position");
        }
        if manifest.is_none() {
            missing.push("manifest");
        }
        if !missing.is_empty() {
            self.record(
                &mut ledger,
                Entry {
                    tool: "claim_completion",
                    scope: &scope_name,
                    args,
                    outcome: "rejected_missing_fields",
                    gated: position.map(str::to_string),
                    latest: None,
                    obligation: obligation.map(str::to_string),
                    manifest,
                },
            )?;
            return Ok(json!({
                "accepted": false,
                "status": "rejected_missing_fields",
                "missing": missing,
            }));
        }

        let given = position.unwrap_or_default();
        let latest = Binding::compute(&ledger, &scope.project_ids())?;
        if latest.digest != given {
            self.record(
                &mut ledger,
                Entry {
                    tool: "claim_completion",
                    scope: &scope_name,
                    args,
                    outcome: "rejected_stale_position",
                    gated: Some(given.to_string()),
                    latest: Some(latest.digest.clone()),
                    obligation: obligation.map(str::to_string),
                    manifest,
                },
            )?;
            return Ok(json!({
                "accepted": false,
                "status": "rejected_stale_position",
                "latest_chain_position": latest.digest,
                "note": "the record moved after your packet was issued; refetch and resubmit",
            }));
        }

        let call_id = self.record(
            &mut ledger,
            Entry {
                tool: "claim_completion",
                scope: &scope_name,
                args,
                outcome: "ready_for_verification",
                gated: Some(given.to_string()),
                latest: Some(latest.digest.clone()),
                obligation: obligation.map(str::to_string),
                manifest,
            },
        )?;
        Ok(json!({
            "accepted": true,
            // Never "done", never "complete", never "verified".
            "status": "ready_for_verification",
            "chain_position": given,
            // The typeable handle: `thruline why <claim_id>` walks this
            // claim's receipt chain, before and after any verdict exists.
            "claim_id": crate::verifier::claim_display_id(&call_id),
            "axes": {
                "lifecycle": "completed (asserted by you, not established)",
                "evidence": "unknown",
                "currency": "current",
                "applicability": "unknown",
            },
            // A truthful description of the loop that now exists — and no
            // more than exists: the verifier runs when invoked, not here.
            "note": "Claim recorded. No proof has been evaluated yet — the verdict belongs to \
                     the verifier, and this response is not one. `thruline verify-claims` \
                     checks each manifest entry against the claiming session's own tool \
                     record and the workspace when it is run; until a verdict lands, the \
                     claim surfaces in later session briefs as claimed with no proof \
                     recorded, wherever the truth-loop hooks are installed.",
        }))
    }

    /// The JSON-RPC 2.0 loop. One request per line in, one response per line
    /// out, which is what MCP's stdio transport specifies.
    pub fn serve(&self, input: impl BufRead, mut output: impl Write) -> Result<()> {
        for line in input.lines() {
            let line = line.map_err(|e| CoreError::Config(format!("stdin: {e}")))?;
            if line.trim().is_empty() {
                continue;
            }
            let Some(response) = self.handle_line(&line) else {
                continue; // a notification expects no reply
            };
            writeln!(output, "{response}")
                .map_err(|e| CoreError::Config(format!("stdout: {e}")))?;
            output
                .flush()
                .map_err(|e| CoreError::Config(format!("flush: {e}")))?;
        }
        Ok(())
    }

    /// Handle one JSON-RPC line. `None` means "notification, no reply".
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                return Some(error_response(
                    Value::Null,
                    -32700,
                    &format!("parse error: {e}"),
                ));
            }
        };
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        // A request without an id is a notification: act, but never answer.
        let id = id?;

        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "thruline", "version": env!("CARGO_PKG_VERSION")},
            })),
            "tools/list" => Ok(json!({"tools": Server::tools()})),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                self.call(name, &args).map(|value| {
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&value).unwrap_or_default()
                        }]
                    })
                })
            }
            "ping" => Ok(json!({})),
            other => Err(CoreError::Config(format!("unknown method: {other}"))),
        };

        Some(match result {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}).to_string(),
            // Tool failures are reported as JSON-RPC errors rather than as a
            // successful result carrying a sad message, so a client cannot mistake
            // a failure for an answer.
            Err(e) => error_response(id, -32000, &e.to_string()),
        })
    }
}

fn delta_json(d: &crate::packet::DeltaLine) -> Value {
    let mut o = json!({
        "change": d.kind.as_str(),
        "section": d.section,
        "text": d.text,
        "origin": d.origin,
        "evidence": d.evidence,
        "currency": d.currency,
        "applicability": d.applicability,
        "reason": d.reason,
    });
    if let Some((e, c, a)) = &d.was {
        o["was"] = json!({"evidence": e, "currency": c, "applicability": a});
    }
    o
}

fn scope_label(scope: &Scope) -> String {
    match scope {
        Scope::Project(p) => format!("project:{p}"),
        Scope::Repo { repo_id, .. } => format!("repo:{repo_id}"),
    }
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anonymous_server_is_refused() {
        assert!(Server::new("", std::path::PathBuf::from("/tmp/x")).is_err());
        assert!(Server::new("   ", std::path::PathBuf::from("/tmp/x")).is_err());
    }

    #[test]
    fn identical_calls_get_distinct_ids() {
        // Two check_context polls that both answer "current" are byte-identical
        // by construction. If their ids collided, the second would be silently
        // discarded and "every call is recorded" would quietly be false.
        let a = call_id("claude", "check_context", "sha", 1, "nonce-a");
        let b = call_id("claude", "check_context", "sha", 1, "nonce-b");
        assert_ne!(a, b, "the nonce must separate identical calls");
        let c = call_id("claude", "check_context", "sha", 2, "nonce-a");
        assert_ne!(a, c, "the timestamp must separate identical calls");
    }

    #[test]
    fn call_id_is_length_prefixed_against_field_smear() {
        assert_ne!(
            call_id("ab", "c", "s", 1, "n"),
            call_id("a", "bc", "s", 1, "n"),
            "field boundaries must not be ambiguous"
        );
    }

    #[test]
    fn there_are_exactly_three_tools_and_none_can_say_done() {
        let tools = Server::tools();
        let list = tools.as_array().expect("array");
        assert_eq!(list.len(), 3);
        let names: Vec<&str> = list
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(
            names,
            ["get_context_packet", "check_context", "claim_completion"]
        );
    }
}

/// A126 — binding recorded MCP calls to the transcript events they became.
///
/// **Exact, never proximity.** A call records
/// `request_sha256 = sha256(canonical(arguments))`; when the client's session is
/// later ingested, the same arguments arrive as a `tool_use` event named
/// `mcp__thruline__<tool>`. Re-canonicalising that event's `tool_input` and
/// hashing it reproduces the call's own hash, so a match is an identity rather
/// than a guess. Nearest-in-time would have bound the wrong row the first time
/// two identical calls appeared, and this project does not get to guess about
/// provenance.
pub mod bind {
    use super::*;

    /// The health metric. `calls == 0` and `bound == 0` are different facts and
    /// are kept apart on purpose: a rate over an empty set is not 0%, it is
    /// undefined, and A95 recorded that reporting it as 0% would be exactly the
    /// kind of number this product refuses.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct BindReport {
        pub calls: usize,
        pub bound: usize,
        pub newly_bound: usize,
        /// Bindings the ledger held that the data no longer justifies, removed by
        /// this run. A135: the binder is a projection, so a row it can no longer
        /// derive is withdrawn rather than left standing. Reported because
        /// silently dropping rows is hidden work (Rule 12).
        pub withdrawn: usize,
        /// Calls with no matching event in the ledger yet — usually just "that
        /// session has not been ingested", which is not a failure — or calls the
        /// binder **abstained** on because what is recorded cannot tell them
        /// apart. Both are facts; neither is a guess.
        pub unmatched: Vec<String>,
    }

    impl BindReport {
        /// `None` when there is nothing to measure. Never 0.0 by construction.
        pub fn rate(&self) -> Option<f64> {
            if self.calls == 0 {
                return None;
            }
            Some(self.bound as f64 / self.calls as f64)
        }

        /// One line, honest about the empty case.
        pub fn line(&self) -> String {
            match self.rate() {
                None => "no MCP calls on record — nothing to bind".to_string(),
                Some(r) => format!(
                    "{} of {} MCP call(s) bound to a transcript event ({:.0}%){}{}",
                    self.bound,
                    self.calls,
                    r * 100.0,
                    if self.newly_bound > 0 {
                        format!(", {} new", self.newly_bound)
                    } else {
                        String::new()
                    },
                    if self.withdrawn > 0 {
                        format!(", {} withdrawn", self.withdrawn)
                    } else {
                        String::new()
                    }
                ),
            }
        }
    }

    /// Milliseconds since the Unix epoch for a transcript's ISO-8601 UTC stamp
    /// (`2026-08-08T05:52:43.717Z`, fraction optional).
    ///
    /// `None` when the stamp is missing or not in that shape. Callers treat
    /// `None` as **inadmissible**, never as admissible: a candidate whose time we
    /// cannot read is one whose causality we cannot check, and A135 is the entry
    /// recording what happens when the binder assumes in its own favour.
    fn epoch_millis(ts: &str) -> Option<i64> {
        let b = ts.as_bytes();
        if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
            return None;
        }
        let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
        let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
        if !(1..=12).contains(&mo) {
            return None;
        }
        // Fraction, if present, to milliseconds. `.7` is 700 ms, not 7.
        let frac = match ts.get(19..20) {
            Some(".") => {
                let digits: String = ts[20..].chars().take_while(char::is_ascii_digit).collect();
                if digits.is_empty() {
                    return None;
                }
                let ms: i64 = format!("{digits:0<3}")[..3].parse().ok()?;
                ms
            }
            _ => 0,
        };

        // Howard Hinnant's days_from_civil — the inverse of the civil_from_days
        // used by `ingest::iso8601_utc`.
        let y = if mo <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;

        Some((days * 86_400 + h * 3_600 + mi * 60 + s) * 1_000 + frac)
    }

    /// Derive bindings from `mcp_calls` + `events`.
    ///
    /// **A135 rewrote the rule this uses.** It used to walk the calls in time
    /// order and hand each one the next unclaimed candidate carrying the same
    /// `tool+request_sha256`. That is a guess, and it guessed wrong: it bound a
    /// call to an event that happened 41.7 s *after* the call, because candidates
    /// were ordered by session UUID while calls were ordered by time. It was also
    /// not rebuildable — the claimed set was seeded from the existing table, so
    /// the answer depended on the order the ingests had run in, not on the data.
    ///
    /// The rule now: **bind only what is identified; abstain otherwise.**
    ///
    /// 1. *Causality floor.* A candidate whose transcript timestamp postdates the
    ///    call's `accepted_at_unix_ms` is inadmissible — a served call cannot be
    ///    caused by a `tool_use` that had not happened yet. This is a **floor and
    ///    not the fix**: it cannot separate a group whose candidates all
    ///    legitimately precede all of its calls. Do not mistake it for identity.
    /// 2. *Abstention.* A `(tool, request_sha256)` group binds only when it holds
    ///    exactly one call and exactly one admissible candidate. Anything else is
    ///    unidentifiable from what is recorded — `mcp_calls` names no session —
    ///    so every call in the group goes to `unmatched`. An abstention is a fact;
    ///    a guess wearing a true-looking hash is a false receipt.
    /// 3. *Path-independence.* The result is computed from the data alone and the
    ///    table is then reconciled to it. An incremental bind and a
    ///    rebuild-from-empty over identical data produce identical pairs.
    ///
    /// Timestamp order was considered as a rule and rejected: a permission prompt
    /// sat ~84 s between one call's emission and its acceptance, so emission order
    /// and acceptance order diverge by arbitrary amounts. That would have replaced
    /// one proximity guess with a better-behaved one.
    pub fn bind_calls(ledger: &Ledger) -> Result<BindReport> {
        let calls: Vec<(String, String, i64)> = {
            let mut stmt = ledger.conn().prepare(
                "SELECT call_id, tool, request_sha256, accepted_at_unix_ms
                   FROM mcp_calls ORDER BY accepted_seq",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    format!("{}\u{1}{}", r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                    r.get::<_, i64>(3)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };
        let total = calls.len();

        // What the table holds now. Read only to reconcile against — never to
        // decide with. Seeding the decision from this is precisely what made the
        // old binder a function of ingest history (A135).
        let existing: std::collections::BTreeMap<String, String> = {
            let mut stmt = ledger
                .conn()
                .prepare("SELECT call_id, event_id FROM mcp_call_bindings")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = std::collections::BTreeMap::new();
            for row in rows {
                let (c, e) = row?;
                out.insert(c, e);
            }
            out
        };

        // Candidate events, keyed the same way the call is: tool + argument hash,
        // carrying the timestamp the causality floor needs. Ordering no longer
        // selects anything — a group binds only when it holds exactly one
        // admissible candidate — so `id` is used purely for a stable read.
        let mut candidates: std::collections::BTreeMap<String, Vec<(String, Option<i64>)>> =
            std::collections::BTreeMap::new();
        {
            let mut stmt = ledger.conn().prepare(
                "SELECT id, tool_name, tool_input, ts FROM events
                  WHERE kind='tool_use' AND tool_name LIKE 'mcp\\_\\_thruline\\_\\_%' ESCAPE '\\'
                  ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })?;
            for row in rows {
                let (id, tool_name, input, ts) = row?;
                let Some(tool) = tool_name.strip_prefix("mcp__thruline__") else {
                    continue;
                };
                let Some(raw) = input else { continue };
                let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                    continue;
                };
                let hash = sha256_hex(canonical(&value).as_bytes());
                candidates
                    .entry(format!("{tool}\u{1}{hash}"))
                    .or_default()
                    .push((id, ts.as_deref().and_then(epoch_millis)));
            }
        }

        // How many calls carry each key. A key held by more than one call cannot
        // be resolved from anything on record: `mcp_calls` stores `agent` but no
        // session, so two byte-identical calls are indistinguishable rows.
        let mut calls_per_key: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for (_, key, _) in &calls {
            *calls_per_key.entry(key.as_str()).or_default() += 1;
        }

        // THE DECISION, computed from the data alone. Nothing below reads
        // `existing`, which is what makes an incremental run and a rebuild agree.
        let mut desired: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for (call_id, key, accepted_ms) in &calls {
            if calls_per_key.get(key.as_str()).copied().unwrap_or(0) != 1 {
                continue; // abstain: more than one call wears this key
            }
            let Some(events) = candidates.get(key) else {
                continue; // that session is not ingested yet
            };
            // The causality floor. A candidate stamped after the call was served
            // cannot be the call. This refuses the A135 crossing, and it is only a
            // floor: it cannot separate candidates that all precede all the calls.
            let mut admissible = events
                .iter()
                .filter(|(_, ts)| ts.is_some_and(|t| t <= *accepted_ms));
            let (Some((event_id, _)), None) = (admissible.next(), admissible.next()) else {
                continue; // zero admissible, or several — either way, abstain
            };
            desired.insert(call_id.clone(), event_id.clone());
        }

        // Reconcile the table to the decision. Withdrawals happen before inserts
        // so an event moving between calls cannot trip `UNIQUE(event_id)`.
        let mut withdrawn = 0usize;
        for (call_id, event_id) in &existing {
            if desired.get(call_id) != Some(event_id) {
                ledger.conn().execute(
                    "DELETE FROM mcp_call_bindings WHERE call_id = ?1",
                    rusqlite::params![call_id],
                )?;
                withdrawn += 1;
            }
        }
        let mut newly_bound = 0usize;
        let now = crate::ingest::iso8601_utc_now();
        for (call_id, event_id) in &desired {
            if existing.get(call_id) == Some(event_id) {
                continue;
            }
            ledger.conn().execute(
                "INSERT INTO mcp_call_bindings(call_id, event_id, basis, bound_at)
                 VALUES(?1,?2,?3,?4)",
                rusqlite::params![call_id, event_id, "tool+request_sha256+unique+causal", now],
            )?;
            newly_bound += 1;
        }

        // Named, never dropped (A133). Calls are walked in `accepted_seq` order so
        // the list reads in the order the calls happened.
        let unmatched: Vec<String> = calls
            .iter()
            .filter(|(call_id, _, _)| !desired.contains_key(call_id))
            .map(|(call_id, _, _)| call_id.clone())
            .collect();

        // Count what the ledger actually holds, never what we attempted.
        let bound: usize =
            ledger
                .conn()
                .query_row("SELECT COUNT(*) FROM mcp_call_bindings", [], |r| {
                    r.get::<_, i64>(0)
                })? as usize;

        Ok(BindReport {
            calls: total,
            bound,
            newly_bound,
            withdrawn,
            unmatched,
        })
    }
}
