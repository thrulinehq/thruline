use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn paths(value: &Value) -> Result<BTreeSet<&str>, DecodeError> {
    value
        .as_array()
        .ok_or_else(|| invalid("missing Infer source selection"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("invalid Infer source path"))
        })
        .collect()
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let value = parse_json(bytes)?;
    if value["schema"] != "thruline.infer-capture.v1" || value["version"] != "1.3.0" {
        return Err(invalid("unrecognized Infer capture contract"));
    }
    let selected = paths(&value["selected_files"])?;
    let discovered = paths(&value["discovered_files"])?;
    let stage = value["isolated_source"]
        .as_str()
        .ok_or_else(|| invalid("missing isolated compile root"))?;
    let captured = value["captured_files"]
        .as_array()
        .ok_or_else(|| invalid("missing native capture inventory"))?;
    let mut captured_paths = BTreeSet::new();
    let mut fresh = !captured.is_empty();
    for row in captured {
        let path = row["path"]
            .as_str()
            .ok_or_else(|| invalid("invalid captured source path"))?;
        captured_paths.insert(path.strip_prefix(&format!("{stage}/")).unwrap_or(path));
        fresh &= row["freshly_captured"] == 1;
    }
    let native = value["native_report"]
        .as_str()
        .ok_or_else(|| invalid("missing native Infer report"))?;
    // Infer may fail its compiler capture before producing report.json at all.
    // Preserve that absence and the compiler/capture facts; never turn it into
    // a successful empty native diagnostic list.
    let no_native_report = native.trim().is_empty()
        && value["diagnostics"].is_null()
        && value["exit_code"].as_i64().is_some_and(|code| code != 0);
    let diagnostics: &[Value] = if no_native_report {
        &[]
    } else {
        if parse_json(native.as_bytes())? != value["diagnostics"] {
            return Err(invalid(
                "Infer normalized diagnostics differ from native report",
            ));
        }
        value["diagnostics"]
            .as_array()
            .ok_or_else(|| invalid("Infer report is not a list"))?
    };
    let mut report = Report {
        format: "infer-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: !no_native_report
            && value["exit_code"] == 0
            && !selected.is_empty()
            && selected == captured_paths
            && selected == discovered
            && fresh
            && value["capture_error"].is_null()
            && value["original_unchanged"] == true
            && value["symlinks_not_followed"]
                .as_array()
                .is_some_and(Vec::is_empty),
        limitations: vec![],
    };
    if !report.complete {
        report.limitations.push("Infer did not establish complete fresh build capture for the selected Java sources. Inspect the compiler failure, captured source inventory and excluded inputs.".into());
    }
    for (index, row) in diagnostics.iter().enumerate() {
        let text = |key: &str| {
            row[key]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("incomplete Infer diagnostic"))
        };
        let path = text("file")?;
        let relative = path.strip_prefix(&format!("{stage}/")).unwrap_or(path);
        if !selected.contains(relative) {
            return Err(invalid(
                "Infer diagnostic lies outside the captured compile scope",
            ));
        }
        let line = row["line"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("invalid Infer source line"))?;
        if row["bug_trace"].as_array().is_none() {
            return Err(invalid("missing native Infer issue trace"));
        }
        report.observations.push(Observation {
            rule: text("bug_type")?.into(),
            message: text("qualifier")?.into(),
            path: Some(relative.into()),
            line: Some(line),
            pointer: format!("/diagnostics/{index}"),
        });
    }
    report.facts.insert(
        "finding_count".into(),
        if no_native_report {
            Value::Null
        } else {
            json!(diagnostics.len())
        },
    );
    report.facts.insert(
        "native_report_status".into(),
        json!(if no_native_report {
            "not_produced_after_failed_capture"
        } else {
            "produced"
        }),
    );
    report
        .facts
        .insert("native_diagnostics".into(), value["diagnostics"].clone());
    report.facts.insert("compile_scope".into(), json!({"selected":selected,"captured":captured_paths,"discovered":discovered,"freshly_captured":fresh,"configuration":value["configuration"]}));
    report
        .facts
        .insert("native_artifacts".into(), value["artifacts"].clone());
    report.facts.insert(
        "compiler_diagnostics".into(),
        value["native_stderr"].clone(),
    );
    report.limitations.push("This Java17 javac capture uses Infer Pulse default analysis. A native issue trace is a supported potential defect; corrected behavior and incomplete capture must be independently exercised. Other languages, classpaths and build systems are outside this recipe.".into());
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiler_failure() -> Value {
        json!({"schema":"thruline.infer-capture.v1","version":"1.3.0",
            "selected_files":["NullPath.java"],"discovered_files":["NullPath.java"],
            "isolated_source":"/owned/source","captured_files":[],"exit_code":1,
            "native_report":"","diagnostics":null,"capture_error":null,
            "native_stderr":"NullPath.java:3: error: cannot find symbol\nMissingLibrary.load(enabled)",
            "configuration":{"java_release":17},"original_unchanged":true,
            "symlinks_not_followed":[],"artifacts":[]})
    }

    #[test]
    fn failed_compiler_capture_preserves_absent_report_and_diagnostics() {
        let value = compiler_failure();
        let report = decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(!report.complete);
        assert!(report.observations.is_empty());
        assert!(report.facts["finding_count"].is_null());
        assert!(report.facts["native_diagnostics"].is_null());
        assert_eq!(
            report.facts["native_report_status"],
            "not_produced_after_failed_capture"
        );
        assert_eq!(report.facts["compiler_diagnostics"], value["native_stderr"]);
        assert_eq!(report.facts["compile_scope"]["captured"], json!([]));
        assert_eq!(report.facts["compile_scope"]["freshly_captured"], false);
        assert_eq!(
            report.facts["compile_scope"]["selected"],
            json!(["NullPath.java"])
        );
    }

    #[test]
    fn empty_report_is_not_accepted_as_successful_capture() {
        let mut value = compiler_failure();
        value["exit_code"] = json!(0);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        value["exit_code"] = json!(1);
        value["diagnostics"] = json!([]);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}
