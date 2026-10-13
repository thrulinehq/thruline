//! Validate additional native views without replacing the full configured graph.
use super::{invalid, parse_json, DecodeError, Report};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, DecodeError> {
    value[field]
        .as_str()
        .ok_or_else(|| invalid("missing native mode text"))
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn process(value: &Value) -> Result<(), DecodeError> {
    text(value, "id")?;
    if !value["argv"]
        .as_array()
        .is_some_and(|args| !args.is_empty() && args.iter().all(Value::is_string))
    {
        return Err(invalid("missing actual native mode argv"));
    }
    for stream in ["stdout", "stderr"] {
        if value[format!("{stream}_sha256")] != hash(text(value, stream)?.as_bytes()) {
            return Err(invalid("native mode process hash mismatch"));
        }
    }
    if !value["exit_code"].is_null() && value["exit_code"].as_i64().is_none() {
        return Err(invalid("invalid native mode exit status"));
    }
    Ok(())
}
fn inline(value: &Value) -> Result<(), DecodeError> {
    let bytes = text(value, "text")?.as_bytes();
    if value["sha256"] != hash(bytes) || value["bytes"].as_u64() != Some(bytes.len() as u64) {
        return Err(invalid(
            "inline native mode artifact differs from its hash or size",
        ));
    }
    Ok(())
}
fn comparable(value: &Value) -> Value {
    let mut modules = value["modules"].clone();
    if let Some(rows) = modules.as_array_mut() {
        for row in rows {
            if let Some(object) = row.as_object_mut() {
                object.remove("checksum");
            }
        }
    }
    let mut rules = value["summary"]["ruleSetUsed"].clone();
    for category in ["forbidden", "allowed", "required"] {
        if let Some(rows) = rules[category].as_array_mut() {
            for row in rows {
                if row["scope"].is_null() {
                    row["scope"] = json!("module");
                }
            }
        }
    }
    json!({"modules":modules,"violations":value["summary"]["violations"],"rules":rules})
}

pub(super) fn capture(
    report: &mut Report,
    value: &Value,
    graph: &mut Value,
) -> Result<(), DecodeError> {
    let Some(mode) = value.get("thruline_review_modes") else {
        return Ok(());
    };
    if mode["schema"] != "thruline.dependency-review-modes.v1"
        || mode["complete"].as_bool().is_none()
    {
        return Err(invalid("unknown native dependency review mode envelope"));
    }
    let primary = &mode["primary"];
    process(primary)?;
    let mut original = value.clone();
    let object = original
        .as_object_mut()
        .ok_or_else(|| invalid("native graph is not an object"))?;
    object.remove("thruline_reports");
    object.remove("thruline_review_modes");
    if primary["id"] != "native-analysis"
        || parse_json(text(primary, "stdout")?.as_bytes())? != original
        || ![Some(0), Some(1)].contains(&primary["exit_code"].as_i64())
        || !primary["error"].is_null()
        || original["summary"]["optionsUsed"]["cache"] != false
        || original["summary"]["ignore"] != 0
    {
        return Err(invalid(
            "additional modes lack their actual uncached full companion",
        ));
    }
    let errors = mode["errors"]
        .as_array()
        .filter(|items| items.iter().all(Value::is_string))
        .ok_or_else(|| invalid("missing native mode errors"))?;
    let captures = mode["processes"]
        .as_array()
        .filter(|items| items.len() <= 12)
        .ok_or_else(|| invalid("invalid native mode process denominator"))?;
    let mut by_id = BTreeMap::new();
    let allowed = [
        "source-head",
        "source-files",
        "baseline-generation",
        "baseline-filtered",
        "affected-base",
        "affected-changes",
        "affected",
        "cache",
        "source-head-after",
        "source-files-after",
    ];
    for capture in captures {
        process(capture)?;
        let id = text(capture, "id")?;
        if !allowed.contains(&id) || by_id.insert(id, capture).is_some() {
            return Err(invalid("unknown or repeated native mode process"));
        }
    }
    let reports = mode["reports"]
        .as_object()
        .ok_or_else(|| invalid("missing native mode reports"))?;
    for (name, native) in reports {
        let capture = by_id
            .get(name.as_str())
            .ok_or_else(|| invalid("mode report has no native process"))?;
        if ![
            "baseline-generation",
            "baseline-filtered",
            "affected",
            "cache",
        ]
        .contains(&name.as_str())
            || native["process_id"] != *name
            || parse_json(text(capture, "stdout")?.as_bytes())? != native["native"]
        {
            return Err(invalid("mode report differs from its actual native output"));
        }
    }
    for key in ["policy", "baseline_input"] {
        if !mode[key].is_null() {
            inline(&mode[key])?;
        }
    }
    if mode["complete"] == false {
        if errors.is_empty() {
            return Err(invalid("incomplete native modes have no recorded cause"));
        }
        report.complete = false;
        for error in errors {
            report.limitations.push(format!(
                "Selected native dependency mode is incomplete: {}",
                error.as_str().unwrap()
            ));
        }
    } else {
        if !errors.is_empty() {
            return Err(invalid("complete native modes contain errors"));
        }
        let selection = &mode["selection"];
        if selection["schema"] != "thruline.dependency-review-modes.v1"
            || !selection
                .as_object()
                .is_some_and(|fields| fields.len() == 4)
            || selection["cache"].as_bool().is_none()
            || !(selection["baseline"].is_null() || selection["baseline"].is_string())
            || !(selection["affected_base"].is_null() || selection["affected_base"].is_string())
            || (selection["baseline"].is_null()
                && selection["affected_base"].is_null()
                && selection["cache"] == false)
            || parse_json(text(&mode["policy"], "text")?.as_bytes())? != *selection
        {
            return Err(invalid("missing bound native mode selection"));
        }
        let mut expected = vec!["source-head", "source-files"];
        let mut expected_reports = vec![];
        if !selection["baseline"].is_null() {
            expected.extend(["baseline-generation", "baseline-filtered"]);
            expected_reports.extend(["baseline-generation", "baseline-filtered"]);
            if mode["baseline_input"]["relative_path"] != selection["baseline"]
                || !mode["reports"]["baseline-generation"]["native"].is_array()
            {
                return Err(invalid(
                    "selected baseline input or native proposal is absent",
                ));
            }
        }
        if !selection["affected_base"].is_null() {
            expected.extend(["affected-base", "affected-changes", "affected"]);
            expected_reports.push("affected");
            let base = text(selection, "affected_base")?;
            if base.len() != 40
                || !base
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || mode["affected_base"] != base
                || by_id
                    .get("affected-base")
                    .is_none_or(|p| p["stdout"].as_str().is_none_or(|s| s.trim() != base))
            {
                return Err(invalid("affected scope lacks its exact commit"));
            }
        }
        if selection["cache"] == true {
            expected.push("cache");
            expected_reports.push("cache");
            let cache = &mode["cache"];
            if !cache["before"].is_null() {
                inline(&cache["before"])?;
            }
            inline(&cache["after"])?;
            let capture = by_id
                .get("cache")
                .ok_or_else(|| invalid("native cache process missing"))?;
            let progress = text(capture, "stderr")?
                .lines()
                .filter(|line| !line.is_empty())
                .map(|line| parse_json(line.as_bytes()))
                .collect::<Result<Vec<_>, _>>()?;
            if Value::Array(progress.clone()) != cache["native_progress"] {
                return Err(invalid("cache progress differs from actual native stderr"));
            }
            let reused = progress
                .iter()
                .any(|row| row["message"] == "report: from cache");
            let extracted = progress.iter().any(|row| row["message"] == "extract");
            let kind = if reused {
                "native_cached_result_reused"
            } else {
                "native_analysis_executed"
            };
            if reused == extracted
                || cache["execution_kind"] != kind
                || cache["strategy"] != "content"
                || cache["key"] != mode["source_identity_sha256"]
                || cache["matches_uncached_full_companion"] != true
                || comparable(&mode["reports"]["cache"]["native"]) != comparable(&original)
            {
                return Err(invalid("cache evidence does not establish the named native execution and full companion equality"));
            }
        }
        expected.extend(["source-head-after", "source-files-after"]);
        if captures
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>()
            != expected
            || reports.len() != expected_reports.len()
            || expected_reports
                .iter()
                .any(|name| !reports.contains_key(*name))
            || captures
                .iter()
                .any(|capture| capture["exit_code"] != 0 || !capture["error"].is_null())
        {
            return Err(invalid(
                "completed native mode process denominator or outcome mismatch",
            ));
        }
        let encoded = text(mode, "source_identity_encoded")?;
        if hash(encoded.as_bytes()) != mode["source_identity_sha256"]
            || parse_json(encoded.as_bytes())? != mode["source_identity"]
            || mode["source_identity"]["root"] != original["summary"]["optionsUsed"]["baseDir"]
            || mode["source_identity"]["policy_sha256"] != mode["policy"]["sha256"]
            || mode["source_identity"]["baseline_sha256"] != mode["baseline_input"]["sha256"]
        {
            return Err(invalid(
                "native mode source identity differs from its recorded inputs",
            ));
        }
        let head = text(by_id["source-head"], "stdout")?.trim();
        if head != text(by_id["source-head-after"], "stdout")?.trim()
            || mode["source_identity"]["head"] != head
            || by_id["source-files"]["stdout"] != by_id["source-files-after"]["stdout"]
        {
            return Err(invalid(
                "native mode source identity changed during execution",
            ));
        }
    }
    graph["review_modes"] = mode.clone();
    report.limitations.push("The primary assessment uses the uncached configured full graph. Baseline and affected views are scoped; native cache reuse is not fresh analysis or independent confirmation.".into());
    Ok(())
}
