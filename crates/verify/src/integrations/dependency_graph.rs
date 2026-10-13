//! Preserve the native graph and opt-in reporter output without inferring code.
use super::{parse_json, DecodeError, Report};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[path = "dependency_resolution.rs"]
mod dependency_resolution;
#[path = "dependency_review_modes.rs"]
mod dependency_review_modes;

fn invalid(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}

pub(super) fn capture(report: &mut Report, value: &Value) -> Result<(), DecodeError> {
    let modules = value["modules"]
        .as_array()
        .ok_or_else(|| invalid("missing native modules"))?;
    let mut names = BTreeSet::new();
    let mut edge_count = 0usize;
    for module in modules {
        let source = module["source"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("missing native module source"))?;
        if !names.insert(source) {
            return Err(invalid("duplicate native module source"));
        }
        let dependencies = module["dependencies"]
            .as_array()
            .ok_or_else(|| invalid("missing native module dependencies"))?;
        edge_count += dependencies.len();
        for dependency in dependencies {
            if dependency["resolved"].as_str().is_none_or(str::is_empty) {
                return Err(invalid("missing native dependency destination"));
            }
        }
        if let Some(instability) = module.get("instability") {
            if !instability
                .as_f64()
                .is_some_and(|n| (0.0..=1.0).contains(&n))
            {
                return Err(invalid("invalid native module instability"));
            }
        }
    }
    if let Some(folders) = value.get("folders") {
        for folder in folders
            .as_array()
            .ok_or_else(|| invalid("invalid native folder list"))?
        {
            // The native aggregator represents a noncalculable folder sink
            // with -1. Preserve that sentinel; it is not a zero-module folder.
            if folder["name"].as_str().is_none()
                || !folder["moduleCount"].as_i64().is_some_and(|n| n >= -1)
            {
                return Err(invalid("invalid native folder identity or module count"));
            }
            for name in ["afferentCouplings", "efferentCouplings"] {
                if folder.get(name).is_some_and(|n| n.as_u64().is_none()) {
                    return Err(invalid("invalid native coupling count"));
                }
            }
            if folder
                .get("instability")
                .is_some_and(|n| !n.as_f64().is_some_and(|n| (0.0..=1.0).contains(&n)))
            {
                return Err(invalid("invalid native folder instability"));
            }
        }
    }
    dependency_resolution::capture(report, value)?;
    let mut graph = json!({"schema":"thruline.native-dependency-graph.v1",
        "modules":modules, "folders":value.get("folders"), "summary":value["summary"],
        "module_count":modules.len(), "dependency_count":edge_count,
        "metrics_requested":false, "formats":{},
        "scope":"Producer-reported modules, dependencies and optional coupling metrics. Missing resolution, configured exclusions and unparsed languages remain outside the established graph."});
    if let Some(native) = value.get("thruline_reports") {
        if native["schema"] != "thruline.dependency-native-reports.v1" {
            return Err(invalid("unknown native dependency reporter schema"));
        }
        graph["capture"] = native.clone();
        if native["complete"] != true {
            report.complete = false;
            report.limitations.push(format!(
                "Native dependency report capture is incomplete: {}",
                native["error"]
                    .as_str()
                    .unwrap_or("one or more native reporters did not finish")
            ));
        } else {
            let selected = &native["selection"];
            if selected["schema"] != "thruline.dependency-report-selection.v1"
                || selected["metrics"].as_bool().is_none()
            {
                return Err(invalid("missing native report selection"));
            }
            let formats = selected["formats"]
                .as_array()
                .ok_or_else(|| invalid("missing selected report formats"))?;
            if formats.is_empty() || formats.len() > 4 {
                return Err(invalid("invalid native reporter count"));
            }
            let processes = native["processes"]
                .as_array()
                .ok_or_else(|| invalid("missing actual reporter processes"))?;
            if processes.len() != formats.len() + 1 {
                return Err(invalid("missing or duplicate actual reporter processes"));
            }
            let analysis = &processes[0];
            let stdout = analysis["stdout"]
                .as_str()
                .ok_or_else(|| invalid("missing native analysis stdout"))?;
            let mut original = value.clone();
            original
                .as_object_mut()
                .ok_or_else(|| invalid("native report must be an object"))?
                .remove("thruline_reports");
            original
                .as_object_mut()
                .unwrap()
                .remove("thruline_review_modes");
            if analysis["id"] != "native-analysis"
                || ![Some(0), Some(1)].contains(&analysis["exit_code"].as_i64())
                || !analysis["error"].is_null()
                || parse_json(stdout.as_bytes())? != original
                || native["analysis_stdout_sha256"] != analysis["stdout_sha256"]
            {
                return Err(invalid(
                    "native graph differs from its captured analysis process",
                ));
            }
            let outputs = native["formats"]
                .as_object()
                .ok_or_else(|| invalid("missing native reporter outputs"))?;
            if outputs.len() != formats.len() {
                return Err(invalid("native reporter output denominator mismatch"));
            }
            let mut unique = BTreeSet::new();
            for (index, format) in formats.iter().enumerate() {
                let name = format
                    .as_str()
                    .ok_or_else(|| invalid("invalid native reporter name"))?;
                if !["dot", "mermaid", "metrics", "archi"].contains(&name) || !unique.insert(name) {
                    return Err(invalid("unknown or duplicate selected native reporter"));
                }
                let output = outputs
                    .get(name)
                    .ok_or_else(|| invalid("selected native reporter is absent"))?;
                let process = &processes[index + 1];
                let expected_id = format!("native-report-{name}");
                if process["id"] != expected_id
                    || output["process_id"] != expected_id
                    || process["exit_code"] != 0
                    || !process["error"].is_null()
                    || output["text"].as_str().is_none_or(str::is_empty)
                    || output["text"] != process["stdout"]
                    || output["stdout_sha256"] != process["stdout_sha256"]
                    || output["input_sha256"] != native["analysis_stdout_sha256"]
                {
                    return Err(invalid(
                        "native reporter output does not match its actual process",
                    ));
                }
            }
            graph["metrics_requested"] = selected["metrics"].clone();
            graph["formats"] = native["formats"].clone();
        }
    }
    dependency_review_modes::capture(report, value, &mut graph)?;
    report.facts.insert("dependency_graph".into(), graph);
    Ok(())
}
