//! Completion evidence is separate from an empty finding list.
use super::Report;
use serde_json::json;
use std::collections::BTreeSet;

/// ast-grep 0.45.3 emits this summary with --inspect=summary or entity.
/// Entity output additionally binds scanned paths and effective rule IDs.
/// Missing, duplicate, conflicting, zero-work and skipped-work summaries cannot
/// establish coverage. This is the producer's report, not independent attestation.
pub fn apply_capture_diagnostics(report: &mut Report, stderr: &[u8]) {
    if report.format == "semgrep-json-v1" {
        apply_semgrep_omissions(report, stderr);
        return;
    }
    if report.format != "ast-grep-json-v1" {
        return;
    }
    let mut entity_files = BTreeSet::new();
    let mut entity_rules = BTreeSet::new();
    let mut has_entities = false;
    let counts = std::str::from_utf8(stderr).ok().and_then(|text| {
        let mut files = None;
        let mut rules = None;
        for line in text.lines() {
            if let Some(s) = line.strip_prefix("sg: summary|file: ") {
                if files.is_some() {
                    return None;
                }
                files = Some(pair(s, "scannedFileCount=", "skippedFileCount=")?);
            } else if let Some(s) = line.strip_prefix("sg: summary|rule: ") {
                if rules.is_some() {
                    return None;
                }
                rules = Some(pair(s, "effectiveRuleCount=", "skippedRuleCount=")?);
            } else if let Some(s) = line.strip_prefix("sg: entity|file|") {
                has_entities = true;
                let (path, details) = s.rsplit_once(": language=")?;
                let (language, applied) = details.split_once(",appliedRuleCount=")?;
                if path.is_empty() || language.is_empty() || applied.parse::<u64>().ok()? == 0 {
                    return None;
                }
                // Language injections may legitimately repeat one file path.
                entity_files.insert(path.to_string());
            } else if let Some(s) = line.strip_prefix("sg: entity|rule|") {
                has_entities = true;
                let (id, severity) = s.rsplit_once(": finalSeverity=")?;
                if id.is_empty()
                    || !["Error", "Warning", "Info", "Hint"].contains(&severity)
                    || !entity_rules.insert(id.to_string())
                {
                    return None;
                }
            } else if !line.starts_with("sg: summary|project: ") && !line.is_empty() {
                // Diagnostics outside the versioned summary need interpretation.
                return None;
            }
        }
        Some((files?, rules?))
    });
    if let Some(((scanned, skipped_files), (effective, skipped_rules))) = counts {
        if has_entities {
            if entity_files.len() as u64 != scanned || entity_rules.len() as u64 != effective {
                report.complete = false;
                report.limitations.push("Structural entity paths/rules do not agree with the scan summary; exact source scope is unavailable.".into());
                return;
            }
            report.facts.insert(
                "scan_entities".into(),
                json!({"files":entity_files,"rules":entity_rules}),
            );
        }
        report.facts.insert(
            "scan_coverage".into(),
            json!({
                "basis":"captured upstream summary", "scanned_files":scanned,
                "skipped_files":skipped_files, "effective_rules":effective,
                "skipped_rules":skipped_rules
            }),
        );
        if scanned > 0 && effective > 0 && skipped_files == 0 && skipped_rules == 0 {
            report.limitations.push("The summary covers the traversed selection. Captured source audit supplies suppression limits separately; files excluded before traversal and whole-project semantics still require scope review.".into());
            return;
        }
    }
    report.complete = false;
    report.limitations.push("The structural scan did not establish an active rule over a nonempty, fully scanned selection. Check disabled rules, skipped files and the captured --inspect=summary output before accepting an empty finding list.".into());
}

/// Semgrep 1.176.0's ordinary JSON omits paths/skipped, while target_manager.py
/// reports configured exclusions in its captured stderr summary. Retain those
/// omissions without fabricating a finding or trusting an empty JSON list.
fn apply_semgrep_omissions(report: &mut Report, stderr: &[u8]) {
    let Ok(text) = std::str::from_utf8(stderr) else {
        report.complete = false;
        report.limitations.push(
            "The scanner's coverage diagnostics are unreadable; skipped source scope could not be established.".into(),
        );
        return;
    };
    let mut omissions = Vec::new();
    for line in text.lines() {
        let Some(detail) = line.trim().strip_prefix("◦ ") else {
            continue;
        };
        for reason in [
            "Not matching --include patterns",
            "Matching --exclude patterns",
            "Files without read access",
            "Files matching .semgrepignore patterns",
        ] {
            let Some(count) = detail.strip_prefix(&format!("{reason}: ")) else {
                continue;
            };
            match count.parse::<u64>() {
                Ok(0) => {}
                Ok(files) => omissions.push(json!({"reason":reason,"files":files})),
                Err(_) => {
                    omissions.push(json!({"reason":reason,"files":null,"count_unreadable":true}))
                }
            }
        }
    }
    if !omissions.is_empty() {
        report.complete = false;
        report.facts.insert(
            "captured_scope_omissions".into(),
            json!({"basis":"captured Semgrep 1.176.0 stderr summary","omissions":omissions}),
        );
        report.limitations.push(
            "The scanner reported source files omitted by include/exclude rules, ignore patterns or read permissions. Review the retained omission counts before accepting the full selected scope.".into(),
        );
    }
}

fn pair(s: &str, first: &str, second: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once(',')?;
    Some((
        a.strip_prefix(first)?.parse().ok()?,
        b.strip_prefix(second)?.parse().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_entity_scope_requires_consistent_paths_and_rule_ids() {
        let entity = "sg: summary|project: isProject=false\nsg: entity|rule|selected: finalSeverity=Warning\nsg: entity|file|source.ts: language=TypeScript,appliedRuleCount=1\nsg: summary|file: scannedFileCount=1,skippedFileCount=0\nsg: summary|rule: effectiveRuleCount=1,skippedRuleCount=0\n";
        let mut good = super::super::decode("ast-grep-json-v1", b"[]").unwrap();
        apply_capture_diagnostics(&mut good, entity.as_bytes());
        assert!(good.complete);
        assert_eq!(good.facts["scan_entities"]["files"], json!(["source.ts"]));
        assert_eq!(good.facts["scan_entities"]["rules"], json!(["selected"]));
        for bad in [
            entity.replace("scannedFileCount=1", "scannedFileCount=2"),
            entity.replace("appliedRuleCount=1", "appliedRuleCount=0"),
            entity.replace("finalSeverity=Warning", "finalSeverity=Unknown"),
        ] {
            let mut report = super::super::decode("ast-grep-json-v1", b"[]").unwrap();
            apply_capture_diagnostics(&mut report, bad.as_bytes());
            assert!(!report.complete);
        }
    }

    #[test]
    fn disabled_or_absent_work_is_not_a_clean_scan() {
        for stderr in ["", "sg: summary|file: scannedFileCount=1,skippedFileCount=0\nsg: summary|rule: effectiveRuleCount=0,skippedRuleCount=1\n", "sg: summary|file: scannedFileCount=0,skippedFileCount=0\nsg: summary|rule: effectiveRuleCount=1,skippedRuleCount=0\n", "sg: summary|file: scannedFileCount=1,skippedFileCount=1\nsg: summary|rule: effectiveRuleCount=1,skippedRuleCount=0\n"] {
            let mut r=super::super::decode("ast-grep-json-v1",b"[]").unwrap();
            apply_capture_diagnostics(&mut r,stderr.as_bytes());
            assert!(!r.complete);
        }
    }
    #[test]
    fn exactly_one_nonempty_summary_is_required() {
        let summary=b"sg: summary|file: scannedFileCount=1,skippedFileCount=0\nsg: summary|rule: effectiveRuleCount=1,skippedRuleCount=0\n";
        let mut r = super::super::decode("ast-grep-json-v1", b"[]").unwrap();
        apply_capture_diagnostics(&mut r, summary);
        assert!(r.complete);
        let mut duplicate = summary.to_vec();
        duplicate.extend_from_slice(summary);
        apply_capture_diagnostics(&mut r, &duplicate);
        assert!(!r.complete);
    }

    #[test]
    fn semgrep_captured_exclusion_cannot_be_hidden_by_empty_json() {
        // Minimized actual installed08 semgrep-configured-exclusion output:
        // JSON has no paths/skipped, but stderr reports one CLI exclusion.
        let raw = br#"{"version":"1.176.0","results":[],"errors":[],"paths":{"scanned":["/input/api.py","/input/subject.py"]}}"#;
        let mut report = super::super::decode("semgrep-json-v1", raw).unwrap();
        assert!(report.complete);
        let stderr = " • Scan skipped: \n   ◦ Matching --exclude patterns: 1\n   ◦ Files matching .semgrepignore patterns: 1\n";
        apply_capture_diagnostics(&mut report, stderr.as_bytes());
        assert!(!report.complete);
        assert!(report.observations.is_empty());
        assert_eq!(
            report.facts["captured_scope_omissions"]["omissions"][0]["files"],
            1
        );
        assert!(report.limitations.iter().any(|l| l.contains("omitted")));
    }

    #[test]
    fn semgrep_clean_diagnostics_and_marker_prose_do_not_invent_omissions() {
        for stderr in [
            " • No ignore information available\nRan 1 rule on 1 file: 0 findings.\n",
            "Documentation says Matching --exclude patterns: 1\n",
            "   ◦ Matching --exclude patterns: 0\n",
        ] {
            let raw = br#"{"version":"1.176.0","results":[],"errors":[],"paths":{"scanned":["/input/subject.py"]}}"#;
            let mut report = super::super::decode("semgrep-json-v1", raw).unwrap();
            apply_capture_diagnostics(&mut report, stderr.as_bytes());
            assert!(report.complete);
            assert!(!report.facts.contains_key("captured_scope_omissions"));
        }
    }
}
