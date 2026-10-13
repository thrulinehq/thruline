//! Fixed fidelity checks over an exact Git tree and immutable run receipts.
//! No command is executed while deciding a verdict.

use std::path::Path;
use std::process::Command;

use crate::chain::sha256_hex;
use crate::error::Result;
use crate::evidence_binding::{self, EvidenceState, StreamCapture, WsidScopeDecl};
use crate::examined_inputs::{self, SubjectKind, Uncommitted};
use crate::ledger::Ledger;
use crate::redact::Masked;
use crate::run_execution;
use crate::workspace;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::{ByteSpan, FileEqualsParams, GoldenOutputParams, Surface};

pub fn check_file_equals_at(repo: &Path, version: &str, params: &FileEqualsParams) -> CheckOutcome {
    if !safe_path(&params.left) || !safe_path(&params.right) {
        return CheckOutcome::unavailable("file_equals paths are not project-relative", None);
    }
    let Some((commit, tree)) = exact_version(repo, version) else {
        return CheckOutcome::unavailable("version cannot be resolved to an exact tree", None);
    };
    let left = match blob_at(repo, &commit, &params.left) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return paired_outcome(
                VerdictState::Broken,
                tree,
                &params.left,
                &params.right,
                vec![file_finding(
                    &params.left,
                    0,
                    "left file is absent from the version",
                )],
            )
        }
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree)),
    };
    let right = match blob_at(repo, &commit, &params.right) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return paired_outcome(
                VerdictState::Broken,
                tree,
                &params.left,
                &params.right,
                vec![file_finding(
                    &params.right,
                    0,
                    "right file is absent from the version",
                )],
            )
        }
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree)),
    };
    let left_span = params.span.as_ref().and_then(|span| span.left.as_ref());
    let right_span = params.span.as_ref().and_then(|span| span.right.as_ref());
    let Some(left) = selected_bytes(&left, left_span) else {
        return paired_outcome(
            VerdictState::Broken,
            tree,
            &params.left,
            &params.right,
            vec![file_finding(
                &params.left,
                0,
                "accepted left byte span is absent from the version",
            )],
        );
    };
    let Some(right) = selected_bytes(&right, right_span) else {
        return paired_outcome(
            VerdictState::Broken,
            tree,
            &params.left,
            &params.right,
            vec![file_finding(
                &params.right,
                0,
                "accepted right byte span is absent from the version",
            )],
        );
    };
    if left == right {
        return paired_outcome(
            VerdictState::Holds,
            tree,
            &params.left,
            &params.right,
            Vec::new(),
        );
    }
    let offset = left
        .iter()
        .zip(right.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| left.len().min(right.len()));
    let line = 1 + left[..offset].iter().filter(|byte| **byte == b'\n').count() as u64;
    let diff = difference_summary(left, right, offset);
    paired_outcome(
        VerdictState::Broken,
        tree,
        &params.left,
        &params.right,
        vec![file_finding(&params.right, line, &diff)],
    )
}

/// Compare stdout only. EVB_V1 stores stdout and stderr as separate digests;
/// their interleaving is not recorded, so a combined terminal stream cannot
/// be reconstructed or truthfully compared with a golden file.
pub fn check_golden_output_at(
    ledger: &Ledger,
    repo: &Path,
    version: &str,
    params: &GoldenOutputParams,
) -> Result<CheckOutcome> {
    if !safe_path(&params.golden_path)
        || !valid_sha256(&params.golden_sha256)
        || params.command_id.trim().is_empty()
    {
        return Ok(CheckOutcome::unavailable(
            "golden_output parameters are invalid",
            None,
        ));
    }
    let Some((commit, tree)) = exact_version(repo, version) else {
        return Ok(CheckOutcome::unavailable(
            "version cannot be resolved to an exact tree",
            None,
        ));
    };
    let golden = match blob_at(repo, &commit, &params.golden_path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Ok(golden_outcome(
                VerdictState::Broken,
                tree,
                &params.golden_path,
                &params.command_id,
                vec![file_finding(
                    &params.golden_path,
                    0,
                    "golden file is absent from the version",
                )],
            ))
        }
        Err(reason) => return Ok(CheckOutcome::unavailable(reason, Some(tree))),
    };
    let actual_golden_hash = sha256_hex(&golden);
    if actual_golden_hash != params.golden_sha256 {
        return Ok(golden_outcome(
            VerdictState::Broken,
            tree,
            &params.golden_path,
            &params.command_id,
            vec![file_finding(
                &params.golden_path,
                0,
                "golden file differs from its accepted SHA-256",
            )],
        ));
    }
    let Some(stored) = evidence_binding::load(ledger, &params.command_id)? else {
        return Ok(CheckOutcome::unavailable(
            "recorded command output is absent",
            Some(tree),
        ));
    };
    let binding = &stored.binding;
    let Some(repo_id) = workspace::repo_identity_at(repo)?.repo_id else {
        return Ok(CheckOutcome::unavailable(
            "repository identity is unavailable",
            Some(tree),
        ));
    };
    let inputs = examined_inputs::standing(ledger, SubjectKind::CheckRun, &stored.id)?;
    let execution = run_execution::latest(ledger, &binding.source_session)?;
    let exact_run = binding.repository_identity == repo_id
        && binding.wsid_scope == WsidScopeDecl::everything()
        && evidence_binding::state_at_binding(binding) == EvidenceState::CurrentStateConfirmed
        && binding.wrapper_defects.is_empty()
        && binding.term_signal.is_none()
        && inputs.as_ref().is_some_and(|inputs| {
            inputs.base_revision.value() == Some(commit.as_str())
                && matches!(inputs.uncommitted, Uncommitted::Clean)
                && inputs.source_identity.value().is_some()
                && inputs.source_identity.value() == binding.pre.identity.as_deref()
                && inputs.source_identity.value() == binding.post.identity.as_deref()
        })
        && execution.as_ref().is_some_and(|row| {
            row.settles
                && row.reason.is_none()
                && row.binding_id.as_deref() == Some(stored.id.as_str())
        });
    if !exact_run {
        return Ok(CheckOutcome::unavailable(
            "command output is not bound to this exact clean version",
            Some(tree),
        ));
    }
    if binding.exit_code != Some(0) {
        return Ok(golden_outcome(
            VerdictState::Broken,
            tree,
            &params.golden_path,
            &params.command_id,
            vec![run_finding(
                &params.command_id,
                "recorded command did not exit successfully",
            )],
        ));
    }
    let StreamCapture::Captured { sha256, bytes } = &binding.stdout else {
        return Ok(CheckOutcome::unavailable(
            "recorded stdout is incomplete",
            Some(tree),
        ));
    };
    if *sha256 == params.golden_sha256 && *bytes == golden.len() as u64 {
        Ok(golden_outcome(
            VerdictState::Holds,
            tree,
            &params.golden_path,
            &params.command_id,
            Vec::new(),
        ))
    } else {
        Ok(golden_outcome(
            VerdictState::Broken,
            tree,
            &params.golden_path,
            &params.command_id,
            vec![run_finding(
                &params.command_id,
                "recorded stdout differs from the pinned golden file",
            )],
        ))
    }
}

fn selected_bytes<'a>(bytes: &'a [u8], span: Option<&ByteSpan>) -> Option<&'a [u8]> {
    let Some(span) = span else {
        return Some(bytes);
    };
    if span.end <= span.start {
        return None;
    }
    let start = usize::try_from(span.start).ok()?;
    let end = usize::try_from(span.end).ok()?;
    bytes.get(start..end)
}

fn difference_summary(left: &[u8], right: &[u8], offset: usize) -> String {
    let mut summary = format!(
        "bytes differ at offset {offset} (left {} bytes, right {} bytes)",
        left.len(),
        right.len()
    );
    if let (Some(left_line), Some(right_line)) =
        (text_line_at(left, offset), text_line_at(right, offset))
    {
        use std::fmt::Write;
        let _ = write!(
            summary,
            "; - {} | + {}",
            Masked::mask(left_line).text(),
            Masked::mask(right_line).text()
        );
    }
    summary
}

fn text_line_at(bytes: &[u8], offset: usize) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    let offset = offset.min(bytes.len());
    let start = bytes[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |pos| pos + 1);
    let end = bytes[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |pos| offset + pos);
    text.get(start..end).map(|line| {
        let mut last = line.len().min(120);
        while !line.is_char_boundary(last) {
            last -= 1;
        }
        &line[..last]
    })
}

fn paired_outcome(
    state: VerdictState,
    tree: String,
    left: &str,
    right: &str,
    findings: Vec<Finding>,
) -> CheckOutcome {
    outcome(state, tree, &[left, right], findings)
}

fn golden_outcome(
    state: VerdictState,
    tree: String,
    golden: &str,
    command_id: &str,
    findings: Vec<Finding>,
) -> CheckOutcome {
    outcome(
        state,
        tree,
        &[golden, &format!("run:{command_id}")],
        findings,
    )
}

fn outcome(
    state: VerdictState,
    tree: String,
    checked: &[&str],
    findings: Vec<Finding>,
) -> CheckOutcome {
    let mut surfaces = Vec::<SurfaceResult>::new();
    for path in checked {
        let surface = surface_of(path);
        if let Some(row) = surfaces.iter_mut().find(|row| row.surface == surface) {
            row.checked_places += 1;
        } else {
            surfaces.push(SurfaceResult {
                surface,
                checked_places: 1,
                finding_count: 0,
                state,
            });
        }
    }
    for finding in &findings {
        if let Some(row) = surfaces
            .iter_mut()
            .find(|row| row.surface == finding.surface)
        {
            row.finding_count += 1;
        }
    }
    surfaces.sort_by_key(|row| row.surface.as_str());
    CheckOutcome {
        state,
        tree_hash: Some(tree),
        reason: None,
        checked_places: checked.len() as u64,
        findings,
        surfaces,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}

fn file_finding(path: &str, line: u64, span: &str) -> Finding {
    Finding {
        surface: surface_of(path),
        path: Masked::mask(path).into_text(),
        line,
        span: Masked::mask(span).into_text(),
    }
}

fn run_finding(command_id: &str, span: &str) -> Finding {
    Finding {
        surface: Surface::Other,
        path: format!("run:{}", Masked::mask(command_id).text()),
        line: 0,
        span: span.into(),
    }
}

fn surface_of(path: &str) -> Surface {
    if path.starts_with("run:") {
        Surface::Other
    } else if path.starts_with("tests/") || path.contains(".test.") {
        Surface::Tests
    } else if path.starts_with(".github/workflows/") {
        Surface::Ci
    } else if path.starts_with("docs/") || path.ends_with(".md") {
        Surface::Docs
    } else if path.starts_with("ui/") || path.ends_with(".html") || path.ends_with(".tsx") {
        Surface::UiCopy
    } else {
        Surface::Code
    }
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('~')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn valid_sha256(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn exact_version(repo: &Path, version: &str) -> Option<(String, String)> {
    fn oid(repo: &Path, spec: &str) -> Option<String> {
        let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["rev-parse", "--verify", "--end-of-options", spec])
            .current_dir(repo)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let oid = std::str::from_utf8(&output.stdout).ok()?.trim();
        (oid.len() >= 40 && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| oid.to_owned())
    }
    let commit = oid(repo, &format!("{version}^{{commit}}"))?;
    let tree = oid(repo, &format!("{commit}^{{tree}}"))?;
    Some((commit, tree))
}

fn blob_at(
    repo: &Path,
    commit: &str,
    path: &str,
) -> std::result::Result<Option<Vec<u8>>, &'static str> {
    let listed = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .arg("--literal-pathspecs")
        .args(["ls-tree", "-z", "--full-tree", commit, "--", path])
        .current_dir(repo)
        .output()
        .map_err(|_| "git tree read could not start")?;
    if !listed.status.success() {
        return Err("git tree read failed");
    }
    let entries = listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return if entries.is_empty() {
            Ok(None)
        } else {
            Err("git tree path is ambiguous")
        };
    };
    let entry = std::str::from_utf8(entry).map_err(|_| "git tree path is not UTF-8")?;
    let (meta, found_path) = entry
        .split_once('\t')
        .ok_or("git tree entry is incomplete")?;
    if found_path != path {
        return Ok(None);
    }
    let fields = meta.split_whitespace().collect::<Vec<_>>();
    let [mode, kind, oid] = fields.as_slice() else {
        return Err("git tree entry has invalid metadata");
    };
    if !matches!(*mode, "100644" | "100755")
        || *kind != "blob"
        || oid.len() < 40
        || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(None);
    }
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .arg("cat-file")
        .arg("blob")
        .arg(oid)
        .current_dir(repo)
        .output()
        .map_err(|_| "git blob read could not start")?;
    if output.status.success() {
        Ok(Some(output.stdout))
    } else {
        Err("git blob read failed")
    }
}
