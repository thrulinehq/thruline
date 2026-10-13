//! RB7 Layer 2 — candidate structures, derived at read. CANDIDATE VOCABULARY ONLY.
//!
//! The three-layer boundary (UX-DOCTRINE §RB7 amendment) governs this file:
//! Layer 1 is rows; this module computes Layer-2 shapes — edit clusters,
//! commit boundaries, uncommitted tails, opening asks, artifact candidates —
//! and NOTHING here may wear a meaning's name. The proxy rule is pinned by
//! `rb7_signals.rs`: the semantic words live only in Layer 3, attached by
//! proof, operator attestation, or a shadow model proposal. Every structure
//! carries the event ids it stands on — an unprovenanced structure is an
//! invented edge with extra steps.
//!
//! The A156 probe licenses exactly these anchors and their measured rates;
//! anything beyond them is invention.

use serde::Serialize;

use crate::error::Result;
use crate::ledger::Ledger;

/// A run of consecutive edit-tool events on one file within a session.
#[derive(Debug, Clone, Serialize)]
pub struct EditCluster {
    pub session_id: String,
    pub file: String,
    pub edits: u64,
    pub first_event_id: String,
    pub last_event_id: String,
    /// Layer-3 slot: null until proof, attestation, or a shadow proposal
    /// fills it. The UI renders the empty slot honestly.
    pub interpretation: Option<String>,
}

/// A `git commit` invocation observed in the rows.
#[derive(Debug, Clone, Serialize)]
pub struct CommitBoundary {
    pub session_id: String,
    pub event_id: String,
    /// The -m message when the invocation carried one; never synthesised.
    pub message: Option<String>,
}

/// Edits recorded after a session's last commit boundary — unresolved, and
/// said exactly that way. ("Dead end" is a Layer-3 word this type may not wear.)
#[derive(Debug, Clone, Serialize)]
pub struct UncommittedTail {
    pub session_id: String,
    pub edits_after_last_boundary: u64,
    pub files: Vec<String>,
    pub interpretation: Option<String>,
}

/// The first user message of a session, labeled exactly as what it is.
#[derive(Debug, Clone, Serialize)]
pub struct OpeningAsk {
    pub session_id: String,
    pub event_id: String,
    /// What the masking pass on this ask actually did (A209-C4). Carried
    /// as a value so it survives the truncation below — the record is
    /// never parsed back out of `text`.
    #[serde(default)]
    pub mask: crate::redact::MaskRecord,
    /// Truncated verbatim text — the person's own words, never rewritten.
    pub text: String,
    pub continuation: bool,
}

/// A file the transcripts name, with its provable fate. Only fates the rows
/// or the filesystem can prove are emitted; everything else is `unresolved`
/// (the probe's per-file law: absolute paths, no demangle guessing).
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactCandidate {
    pub file: String,
    pub edits: u64,
    pub sessions: Vec<String>,
    /// "exists" (stat-proven now) | "unresolved". The two further fates the
    /// directive defines — moved (digest-proven) and deleted-in-git
    /// (commit-proven) — need proof machinery this revision abstains from
    /// rather than approximates; they are emitted only when they can be true.
    pub fate: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidates {
    pub clusters: Vec<EditCluster>,
    pub boundaries: Vec<CommitBoundary>,
    pub tails: Vec<UncommittedTail>,
    pub opening_asks: Vec<OpeningAsk>,
    pub artifacts: Vec<ArtifactCandidate>,
}

const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];
const ASK_TRUNCATE: usize = 280;
const CONTINUATION_PREFIX: &str = "This session is being continued from a previous";

/// Derive every Layer-2 candidate for one project, at read, from stored rows.
pub fn derive(ledger: &Ledger, project_id: &str) -> Result<Candidates> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT id, session_id, seq, kind, tool_name, tool_input, text
         FROM events WHERE project_id = ?1 ORDER BY session_id, seq",
    )?;
    struct Row {
        id: String,
        session_id: String,
        seq: i64,
        kind: String,
        tool_name: Option<String>,
        tool_input: Option<String>,
        text: Option<String>,
    }
    let rows: Vec<Row> = stmt
        .query_map([project_id], |r| {
            Ok(Row {
                id: r.get(0)?,
                session_id: r.get(1)?,
                seq: r.get(2)?,
                kind: r.get(3)?,
                tool_name: r.get(4)?,
                tool_input: r.get(5)?,
                text: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    let mut clusters: Vec<EditCluster> = Vec::new();
    let mut boundaries: Vec<CommitBoundary> = Vec::new();
    let mut tails: Vec<UncommittedTail> = Vec::new();
    let mut opening_asks: Vec<OpeningAsk> = Vec::new();
    let mut by_file: std::collections::BTreeMap<String, (u64, std::collections::BTreeSet<String>)> =
        std::collections::BTreeMap::new();

    let mut i = 0usize;
    while i < rows.len() {
        let session = rows[i].session_id.clone();
        let mut j = i;
        while j < rows.len() && rows[j].session_id == session {
            j += 1;
        }
        let session_rows = &rows[i..j];

        if let Some(first_user) = session_rows.iter().find(|r| r.kind == "user_msg") {
            let raw = first_user.text.clone().unwrap_or_default();
            // The continuation marker is a property of the ORIGINAL text;
            // masking cannot move it, but read it before masking anyway so
            // the classification never depends on detector behaviour.
            let continuation = raw.starts_with(CONTINUATION_PREFIX);
            // A209-C3: mask BEFORE the 280-char cut. A Google key
            // straddling this boundary reached a sealed capsule because the
            // cut left a body below the detector's threshold.
            let (text, mask) = crate::redact::mask_recorded(&raw);
            let mut shown = crate::redact::truncate_respecting_markers(&text, ASK_TRUNCATE);
            if text.chars().count() > shown.chars().count() {
                shown.push('…');
            }
            opening_asks.push(OpeningAsk {
                session_id: session.clone(),
                event_id: first_user.id.clone(),
                mask,
                text: shown,
                continuation,
            });
        }

        let mut cur: Option<EditCluster> = None;
        let mut last_boundary_seq: Option<i64> = None;
        let mut tail_edits: Vec<(i64, String)> = Vec::new();
        for r in session_rows {
            let is_edit = r.kind == "tool_use"
                && r.tool_name
                    .as_deref()
                    .is_some_and(|t| EDIT_TOOLS.contains(&t));
            if is_edit {
                let path = r
                    .tool_input
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .and_then(|v| {
                        v["file_path"]
                            .as_str()
                            .or(v["notebook_path"].as_str())
                            .map(str::to_string)
                    });
                if let Some(path) = path {
                    let entry = by_file.entry(path.clone()).or_default();
                    entry.0 += 1;
                    entry.1.insert(session.clone());
                    tail_edits.push((r.seq, path.clone()));
                    match cur.as_mut() {
                        Some(c) if c.file == path => {
                            c.edits += 1;
                            c.last_event_id = r.id.clone();
                        }
                        _ => {
                            if let Some(done) = cur.take() {
                                clusters.push(done);
                            }
                            cur = Some(EditCluster {
                                session_id: session.clone(),
                                file: path,
                                edits: 1,
                                first_event_id: r.id.clone(),
                                last_event_id: r.id.clone(),
                                interpretation: None,
                            });
                        }
                    }
                    continue;
                }
            }
            if r.kind == "tool_use"
                && r.tool_name.as_deref() == Some("Bash")
                && r.tool_input
                    .as_deref()
                    .is_some_and(|s| s.contains("git commit"))
            {
                last_boundary_seq = Some(r.seq);
                let message = r.tool_input.as_deref().and_then(extract_commit_message);
                boundaries.push(CommitBoundary {
                    session_id: session.clone(),
                    event_id: r.id.clone(),
                    message,
                });
            }
        }
        if let Some(done) = cur.take() {
            clusters.push(done);
        }
        if let Some(boundary_seq) = last_boundary_seq {
            let after: Vec<&(i64, String)> = tail_edits
                .iter()
                .filter(|(s, _)| *s > boundary_seq)
                .collect();
            if !after.is_empty() {
                let mut files: Vec<String> = after.iter().map(|(_, f)| f.clone()).collect();
                files.dedup();
                files.truncate(12);
                tails.push(UncommittedTail {
                    session_id: session.clone(),
                    edits_after_last_boundary: after.len() as u64,
                    files,
                    interpretation: None,
                });
            }
        }
        i = j;
    }

    let artifacts = by_file
        .into_iter()
        .map(|(file, (edits, sessions))| {
            let fate = if std::path::Path::new(&file).exists() {
                "exists"
            } else {
                "unresolved"
            };
            ArtifactCandidate {
                file,
                edits,
                sessions: sessions.into_iter().collect(),
                fate,
            }
        })
        .collect();

    Ok(Candidates {
        clusters,
        boundaries,
        tails,
        opening_asks,
        artifacts,
    })
}

/// The -m string of a `git commit` invocation, when one is present verbatim.
fn extract_commit_message(input: &str) -> Option<String> {
    let idx = input.find("-m ")?;
    let rest = &input[idx + 3..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    // A209-C3: mask before the cut, same ordering invariant.
    let msg: String = crate::redact::mask(&inner[..end])
        .0
        .chars()
        .take(120)
        .collect();
    (!msg.is_empty()).then_some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_messages_are_extracted_verbatim_or_not_at_all() {
        assert_eq!(
            extract_commit_message(r#"git commit -m "fix the parser""#).as_deref(),
            Some("fix the parser")
        );
        assert_eq!(
            extract_commit_message("git commit -m 'one line'").as_deref(),
            Some("one line")
        );
        assert_eq!(extract_commit_message("git commit --amend"), None);
        assert_eq!(extract_commit_message("git commit -m $MSG"), None);
    }
}
