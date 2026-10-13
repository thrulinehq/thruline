use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn bad(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing Sonar array {key}")))
}
fn page_issues(v: &Value, key: &str) -> Result<(Vec<Value>, bool), DecodeError> {
    let pages = array(v, key)?;
    let mut total = None;
    let mut seen = BTreeSet::new();
    let mut items = vec![];
    for (index, page) in pages.iter().enumerate() {
        let paging = &page["paging"];
        let count = paging["total"]
            .as_u64()
            .ok_or_else(|| bad("missing Sonar total"))?;
        let size = paging["pageSize"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| bad("invalid Sonar page size"))?;
        if paging["pageIndex"] != index + 1 || total.is_some_and(|n| n != count) {
            return Err(bad(
                "Sonar pages have missing/repeated page or changed total",
            ));
        }
        total = Some(count);
        let rows = array(page, "issues")?;
        let expected = count.saturating_sub(index as u64 * size).min(size);
        if rows.len() as u64 != expected {
            return Err(bad("Sonar page has missing issues"));
        }
        for row in rows {
            let key = row["key"]
                .as_str()
                .ok_or_else(|| bad("missing Sonar issue key"))?;
            if !seen.insert(key.to_string()) {
                return Err(bad("duplicate issue across Sonar pages"));
            }
            items.push(row.clone());
        }
    }
    let complete = total == Some(items.len() as u64);
    Ok((items, complete))
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let v = parse_json(bytes)?;
    if v["schema"] != "thruline.sonar-capture.v1"
        || v["version"] != "26.9.0.129388"
        || v["scanner_version"] != "8.1.0.6389"
    {
        return Err(bad("unrecognized Sonar capture version"));
    }
    let (issues, all_pages) = page_issues(&v, "issue_pages")?;
    let (resolved, all_resolved) = page_issues(&v, "resolved_issue_pages")?;
    let selected: BTreeSet<&str> = array(&v, "selected_files")?
        .iter()
        .map(|p| p.as_str().ok_or_else(|| bad("invalid selected Sonar path")))
        .collect::<Result<_, _>>()?;
    let key = v["configuration"]["project_key"]
        .as_str()
        .ok_or_else(|| bad("missing Sonar project"))?;
    let page_size = v["configuration"]["page_size"]
        .as_u64()
        .filter(|size| (1..=100).contains(size))
        .ok_or_else(|| bad("invalid requested Sonar page size"))? as usize;
    let file_count = v["measures"]["component"]["measures"]
        .as_array()
        .and_then(|m| m.iter().find(|r| r["metric"] == "files"))
        .and_then(|r| r["value"].as_str())
        .and_then(|s| s.parse::<usize>().ok());
    let mut report = Report {
        format: "sonarqube-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: v["scanner"]["exit_code"] == 0
            && v["task"]["status"] == "SUCCESS"
            && v["analysis_fresh"] == true
            && v["original_unchanged"] == true
            && v["analysis"]["analyses"][0]["revision"] == v["requested_revision"]
            && v["analysis"]["analyses"][0]["key"] == v["task"]["analysisId"]
            && all_pages
            && all_resolved
            && !selected.is_empty()
            && file_count == Some(selected.len())
            && array(&v, "errors")?.is_empty(),
        limitations: vec![],
    };
    for (index, issue) in issues.iter().enumerate() {
        let component = issue["component"]
            .as_str()
            .ok_or_else(|| bad("missing Sonar component"))?;
        let path = component.strip_prefix(&format!("{key}:"));
        if path.is_some_and(|p| !selected.contains(p)) {
            return Err(bad("Sonar issue is outside selected source"));
        }
        report.observations.push(Observation {
            rule: issue["rule"]
                .as_str()
                .ok_or_else(|| bad("missing Sonar rule"))?
                .into(),
            message: issue["message"]
                .as_str()
                .ok_or_else(|| bad("missing Sonar message"))?
                .into(),
            path: path.map(str::to_owned),
            line: issue["line"].as_u64(),
            pointer: format!(
                "/issue_pages/{}/issues/{}",
                index / page_size,
                index % page_size
            ),
        });
    }
    report
        .facts
        .insert("finding_count".into(), json!(issues.len()));
    report.facts.insert("native_issues".into(), json!(issues));
    report
        .facts
        .insert("resolved_successors".into(), json!(resolved));
    report.facts.insert(
        "pagination_complete".into(),
        json!(all_pages && all_resolved),
    );
    for key in [
        "server",
        "scanner",
        "analysis",
        "requested_revision",
        "source_head",
        "source_hashes",
        "selected_files",
        "rules",
        "quality_gate",
        "quality_profiles",
        "plugins",
        "measures",
        "task",
        "api_records",
        "artifacts",
        "errors",
    ] {
        report.facts.insert(key.into(), v[key].clone());
    }
    for note in array(&v, "limitations")? {
        report.limitations.push(
            note.as_str()
                .ok_or_else(|| bad("invalid Sonar limitation"))?
                .into(),
        );
    }
    if !report.complete {
        report.limitations.push("Completed fresh analysis, complete issue pages or selected-file coverage was not established; this cannot clear current source.".into());
    }
    Ok(report)
}
