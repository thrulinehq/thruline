use super::{DecodeError, Report};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};

/// Structured presentation of retained native observations, never a second vote.
pub(super) fn enrich(report: &mut Report, native: &Value) -> Result<(), DecodeError> {
    let rows = native["results"]
        .as_array()
        .ok_or_else(|| DecodeError::Invalid("Opengrep results missing".into()))?;
    let mut findings = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let extra = &row["extra"];
        for endpoint in ["start", "end"] {
            for field in ["line", "col", "offset"] {
                if row[endpoint][field].as_u64().is_none() {
                    return Err(DecodeError::Invalid(
                        "Opengrep native span is missing a nonnegative coordinate".into(),
                    ));
                }
            }
        }
        if row["end"]["offset"].as_u64() < row["start"]["offset"].as_u64() {
            return Err(DecodeError::Invalid(
                "Opengrep native span is reversed".into(),
            ));
        }
        if !extra["metavars"].is_object() || !extra["fingerprint"].is_string() {
            return Err(DecodeError::Invalid(
                "Opengrep native metavariables or fingerprint missing".into(),
            ));
        }
        if extra.get("fix").is_some_and(|fix| !fix.is_string()) {
            return Err(DecodeError::Invalid(
                "Opengrep native fix is not text".into(),
            ));
        }
        findings.push(json!({"rule":row["check_id"],"path":row["path"],"start":row["start"],"end":row["end"],
            "message":extra["message"],"fingerprint":extra["fingerprint"],"metavariables":extra["metavars"],
            "trace":extra["dataflow_trace"],"fix":extra["fix"],"native_pointer":format!("/results/{index}")}));
    }
    let paths = native["paths"]["scanned"]
        .as_array()
        .ok_or_else(|| DecodeError::Invalid("Opengrep scanned paths missing".into()))?;
    let extensions: BTreeSet<_> = paths
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|path| Path::new(path).extension().and_then(|part| part.to_str()))
        .collect();
    let limitations = [
        "Constructor fields, same-file methods and higher-order callbacks use the same Opengrep taint engine and rule model. A native trace is a static possible flow, not an executed exploit.",
        "Class inheritance is not supported by the acquired intrafile implementation; mutual recursion is not fully supported. Zero findings cannot establish these flows are safe.",
        "Observed file extensions describe only this invocation. Visual Basic uses the native vb alias; the upstream vbnet alias is rejected by this pinned stable CLI. No other language or grammar is qualified by this receipt.",
        "Semgrep and Opengrep share engine ancestry. Agreement is correlated evidence; native disagreement and parse errors remain visible.",
        "Parser recovery is not a language compiler check; absence of a native parse error does not establish source syntax correctness.",
        "Experimental enclosing-context output, cross-file analysis and unselected rule-defined fix categories are outside this accepted mode.",
    ];
    report
        .limitations
        .extend(limitations.iter().map(|value| (*value).into()));
    report.facts.insert("opengrep_scope".into(), json!({"schema":"thruline.opengrep-native-details.v1","engine":"opengrep","version":"1.30.0",
        "mode":"selected stable intrafile rules","scanned_file_extensions":extensions,"findings":findings,
        "native_errors":native["errors"],"skipped_rules":native["skipped_rules"],"skipped_paths":native["paths"]["skipped"],
        "limitations":limitations}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::decode::decode;
    #[test]
    fn retained_native_constructor_callback_and_safe_controls() {
        for (bytes, count) in [
            (
                include_bytes!("../../tests/fixtures/opengrep/constructor.json").as_slice(),
                5,
            ),
            (
                include_bytes!("../../tests/fixtures/opengrep/callback.json").as_slice(),
                6,
            ),
            (
                include_bytes!("../../tests/fixtures/opengrep/safe.json").as_slice(),
                0,
            ),
        ] {
            let report = decode("opengrep-json-v1", bytes).unwrap();
            assert!(report.complete);
            assert_eq!(report.observations.len(), count);
            assert_eq!(
                report.facts["opengrep_scope"]["findings"]
                    .as_array()
                    .unwrap()
                    .len(),
                count
            );
        }
    }
    #[test]
    fn retained_native_vb_scope_and_invalid_rule() {
        let vb = decode(
            "opengrep-json-v1",
            include_bytes!("../../tests/fixtures/opengrep/vb.json"),
        )
        .unwrap();
        assert!(vb.complete);
        assert_eq!(vb.observations.len(), 1);
        let error = decode(
            "opengrep-json-v1",
            include_bytes!("../../tests/fixtures/opengrep/invalid-rule.json"),
        )
        .unwrap();
        assert!(!error.complete);
        assert!(!error.facts["rule_engine_errors"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
