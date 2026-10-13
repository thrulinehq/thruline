//! Typed extensions provenance layered over the unchanged native report decoder.
use super::super::DecodeError;
use super::{array, bad, count, parse_json, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn names(value: &Value, pointer: &str) -> Result<BTreeSet<String>, DecodeError> {
    let values = array(value, pointer)?;
    if values.len() > 32 {
        return Err(bad("extension inventory exceeds bound"));
    }
    let out: BTreeSet<_> = values
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .map(str::to_owned)
                .ok_or_else(|| bad("invalid extension inventory name"))
        })
        .collect::<Result<_, _>>()?;
    if out.len() != values.len() {
        return Err(bad("duplicate extension inventory name"));
    }
    Ok(out)
}
pub(super) fn decode(value: &Value, format: &str) -> Result<Report, DecodeError> {
    if value["schema"] != "thruline.schemathesis-extensions-capture.v1"
        || value["runtime_version"] != "4.26.1"
    {
        return Err(bad("unsupported Schemathesis extensions capture"));
    }
    let selection = &value["selection"];
    if selection["schema"] != "thruline.schemathesis-extensions.v1"
        || !matches!(
            selection["specification"].as_str(),
            Some("graphql" | "openapi")
        )
    {
        return Err(bad("unsupported Schemathesis mode selection"));
    }
    let expected = names(selection, "/operations")?;
    let parsed = names(value, "/schema_operations")?;
    let mut complete = !expected.is_empty()
        && expected == parsed
        && value["capture_complete"] == true
        && value["original_unchanged"] == true
        && value["service_stopped"] == true
        && value["process"]["state"] == "completed"
        && array(value, "/capture_gaps")?.is_empty()
        && array(value, "/required_missing")?.is_empty();
    let mut missing = vec![];
    for category in ["checks", "hooks", "scalars", "serializers"] {
        let required = names(selection, &format!("/required_extensions/{category}"))?;
        if category == "checks" && required.is_empty() {
            return Err(bad("no selected business check"));
        }
        let actual = value["registration"]["registered"][category]
            .as_array()
            .map(|_| names(value, &format!("/registration/registered/{category}")))
            .transpose()?
            .unwrap_or_default();
        for name in required.difference(&actual) {
            missing.push(json!({"category":category,"name":name}));
        }
    }
    complete &= missing.is_empty();
    let artifacts = array(value, "/artifacts")?;
    if artifacts.len() > 32 {
        return Err(bad("too many Schemathesis native artifacts"));
    }
    let mut paths = BTreeSet::new();
    for artifact in artifacts {
        let name = artifact["relative_path"]
            .as_str()
            .ok_or_else(|| bad("native artifact name missing"))?;
        if name.is_empty()
            || name.contains(['/', '\\'])
            || name == "."
            || name == ".."
            || !paths.insert(name)
            || count(artifact, "/bytes")? > 262144
        {
            return Err(bad("invalid bounded native artifact inventory"));
        }
        complete &= artifact["complete"] == true;
    }
    let native = &value["native"];
    let mut report = if native.is_null() {
        complete = false;
        Report {
            format: format.into(),
            observations: vec![],
            facts: BTreeMap::new(),
            complete: false,
            limitations: vec![],
        }
    } else {
        let raw = value["native_raw_json"]
            .as_str()
            .ok_or_else(|| bad("original native JSON text missing"))?;
        if parse_json(raw.as_bytes())? != *native {
            return Err(bad("native JSON differs from original captured bytes"));
        }
        let report = super::decode(format, raw.as_bytes())?;
        complete &= native["operations"]["total"].as_u64() == Some(expected.len() as u64)
            && native["seed"] == selection["seed"]
            && native["exit_code"] == value["process"]["exit_code"]
            && native["phases"]["fuzzing"]["status"] != "skip"
            && paths.contains("native.json")
            && paths.contains("native.har")
            && paths.contains("registrations.json");
        report
    };
    report.complete &= complete;
    report.facts.insert("schemathesis_extensions".into(), json!({"schema":"thruline.native-api-extension-scope.v1","selection":selection,"parsed_operations":parsed,"registration":value["registration"],"missing":missing,"source_files":value["source_files"],"native_argv":value["native_argv"],"native_phases":native["phases"],"process":value["process"],"scope":value["scope"],"capture_gaps":value["capture_gaps"],"extension_events":value["extension_events"],"capture_complete":value["capture_complete"]}));
    report.facts.insert("native_capture".into(), value.clone());
    if !complete {
        report.limitations.push("The selected schema operation inventory, required extensions, native completion or original artifact closure is incomplete. Missing registration and missing native reports cannot establish a clear result.".into());
    }
    report.limitations.push("Only the selected fuzzing phase, seed and bounded example count were executed. Registering a scalar strategy does not establish exhaustive value coverage. Custom checks cover their named assertions; request serialization is shown by retained native HTTP bodies. Other phases and extension families remain outside this receipt.".into());
    Ok(report)
}
