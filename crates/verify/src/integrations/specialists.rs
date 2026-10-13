use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[path = "joern_frontends.rs"]
mod joern_frontends;
#[path = "rewrite_modules.rs"]
mod rewrite_modules;

fn bad(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn list<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    value[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing specialist array {key}")))
}
fn paths(value: &Value, key: &str) -> Result<BTreeSet<String>, DecodeError> {
    list(value, key)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| bad("invalid specialist source path"))
        })
        .collect()
}
fn relative<'a>(root: &str, path: &'a str) -> &'a str {
    path.strip_prefix(&format!("{root}/")).unwrap_or(path)
}

pub fn decode(format: &str, bytes: &[u8]) -> Result<Report, DecodeError> {
    let v = parse_json(bytes)?;
    let joern = format == "joern-json-v1";
    if !joern && v["schema"] == "thruline.openrewrite-module-capture.v1" {
        return rewrite_modules::decode(&v, format);
    }
    if joern && v["schema"] == "thruline.joern-frontend-capture.v1" {
        return joern_frontends::decode(&v, format);
    }
    if joern && v["schema"] == "thruline.joern-predefined-capture.v1" {
        return predefined(&v, format);
    }
    if v["schema"]
        != if joern {
            "thruline.joern-capture.v1"
        } else {
            "thruline.openrewrite-capture.v1"
        }
        || v["version"] != if joern { "4.0.626" } else { "8.90.4" }
    {
        return Err(bad("unrecognized specialist capture version"));
    }
    let selected = paths(&v, "selected_files")?;
    let discovered = paths(&v, "discovered_files")?;
    let commands = list(&v, "commands")?;
    let mut report = Report {
        format: format.into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: !selected.is_empty()
            && selected == discovered
            && v["exit_code"] == 0
            && v["original_unchanged"] == true
            && list(&v, "errors")?.is_empty()
            && list(&v, "symlinks_not_followed")?.is_empty()
            && !commands.is_empty()
            && commands.iter().all(|r| r["exit_code"] == 0),
        limitations: vec![],
    };
    let native = &v["native_report"];
    if !native.is_null() {
        if joern {
            if native["schema"] != "thruline.joern-query.v1"
                || native["query"] != "thruline.c.dataflow.v1"
            {
                return Err(bad("unrecognized owned Joern query schema"));
            }
            let stage = v["isolated_source"]
                .as_str()
                .ok_or_else(|| bad("missing graph source root"))?;
            let graph: BTreeSet<String> = paths(native, "graph_files")?
                .iter()
                .map(|p| relative(stage, p).to_string())
                .collect();
            report.complete &= selected.is_subset(&graph)
                && native["source_count"].as_u64().is_some_and(|n| n > 0)
                && native["sink_count"].as_u64().is_some_and(|n| n > 0)
                && native["paths_truncated"] == false
                && list(native, "overlays")?.iter().any(|o| o == "dataflowOss")
                && list(native, "model_gaps")?.is_empty();
            for (index, path) in list(native, "paths")?.iter().enumerate() {
                let nodes = path
                    .as_array()
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| bad("empty Joern path"))?;
                let last = nodes.last().ok_or_else(|| bad("missing Joern sink"))?;
                let file = last["file"]
                    .as_str()
                    .ok_or_else(|| bad("missing Joern path location"))?;
                let file = relative(stage, file);
                if !selected.contains(file) {
                    return Err(bad("Joern path ends outside selected source"));
                }
                let methods: Vec<&str> =
                    nodes.iter().filter_map(|n| n["method"].as_str()).collect();
                report.observations.push(Observation {
                    rule: "thruline.c.dataflow.v1".into(),
                    message: format!(
                        "The selected source reaches the selected sink in Joern's static model: {}",
                        methods.join(" → ")
                    ),
                    path: Some(file.into()),
                    line: last["line"].as_u64(),
                    pointer: format!("/native_report/paths/{index}"),
                });
            }
            report.facts.insert(
                "graph_scope".into(),
                json!({"selected":selected,"discovered":discovered,"graph_files":graph}),
            );
        } else {
            if native["schema"] != "thruline.openrewrite-native.v1" {
                return Err(bad("unrecognized native OpenRewrite schema"));
            }
            report.complete &= paths(native, "parsed_files")? == selected
                && list(native, "errors")?.is_empty()
                && v["dry_run_source_unchanged"] == true
                && commands.iter().any(|r| r["stage"] == "compile-before")
                && commands.iter().any(|r| r["stage"] == "dry-run");
            for (index, change) in list(native, "changes")?.iter().enumerate() {
                let path = change["path"]
                    .as_str()
                    .ok_or_else(|| bad("missing recipe change path"))?;
                if !selected.contains(path)
                    || !change["diff"].as_str().is_some_and(|d| !d.is_empty())
                {
                    return Err(bad(
                        "recipe change is outside selection or has no native patch",
                    ));
                }
                report.observations.push(Observation { rule: "org.openrewrite.java.ChangeMethodName".into(),
                    message: "The named Java method migration has a native proposed patch; independent behavior verification remains required.".into(),
                    path: Some(path.into()), line: None, pointer: format!("/native_report/changes/{index}") });
            }
            report.facts.insert("compile_scope".into(), json!({"selected":selected,"discovered":discovered,"modules":v["configuration"]["modules"]}));
        }
    } else {
        report.complete = false;
    }
    for key in [
        "native_report",
        "commands",
        "configuration",
        "runtime",
        "artifacts",
        "errors",
        "mode",
        "applied_files",
        "inspected_source_version",
        "module_availability",
    ] {
        if !v[key].is_null() {
            report.facts.insert(key.into(), v[key].clone());
        }
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    for limitation in list(&v, "coverage_limitations")? {
        report.limitations.push(
            limitation
                .as_str()
                .ok_or_else(|| bad("invalid specialist limitation"))?
                .into(),
        );
    }
    if !report.complete {
        report.limitations.push("This run has incomplete source, model or execution coverage. A zero observation count cannot clear the requested criterion.".into());
    }
    Ok(report)
}

fn predefined(v: &Value, format: &str) -> Result<Report, DecodeError> {
    const QUERY: &str = "format-controlled-printf";
    if v["version"] != "4.0.626"
        || v["mode"] != "predefined-query"
        || v["configuration"]["schema"] != "thruline.joern-predefined-c.v1"
    {
        return Err(bad("unknown predefined Joern capture"));
    }
    let selected = paths(v, "selected_files")?;
    let discovered = paths(v, "discovered_files")?;
    let commands = list(v, "commands")?;
    let inventory = list(v, "query_inventory")?;
    let errors = list(v, "errors")?;
    let mut report = Report {
        format: format.into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: !selected.is_empty()
            && selected == discovered
            && v["exit_code"] == 0
            && v["original_unchanged"] == true
            && errors.is_empty()
            && list(v, "parse_problems")?.is_empty()
            && list(v, "symlinks_not_followed")?.is_empty()
            && ["inventory", "parse", "query"].iter().all(|stage| {
                commands
                    .iter()
                    .any(|c| c["stage"] == *stage && c["exit_code"] == 0)
            })
            && commands.iter().all(|c| c["exit_code"] == 0),
        limitations: vec![],
    };
    let native = &v["native_report"];
    if native.is_null() {
        report.complete = false;
    } else {
        if native["schema"] != "thruline.joern-predefined-query.v1"
            || native["query"]["name"] != QUERY
            || v["configuration"]["query"] != QUERY
            || native["query"]["language"] != "c"
            || native["query_count"] != 1
        {
            return Err(bad(
                "native predefined query identity differs from selection",
            ));
        }
        let mut names = BTreeSet::new();
        for entry in inventory {
            let name = entry["name"]
                .as_str()
                .ok_or_else(|| bad("query inventory name missing"))?;
            if !names.insert(name) {
                return Err(bad("query inventory names duplicate"));
            }
        }
        let selected_queries: Vec<_> = inventory.iter().filter(|q| q["name"] == QUERY).collect();
        if selected_queries.len() != 1
            || native["inventory_count"].as_u64() != Some(inventory.len() as u64)
            || native["query"]["score"].as_f64() != selected_queries[0]["score"].as_f64()
            || [
                "name",
                "author",
                "title",
                "description",
                "language",
                "tags",
                "traversalAsString",
            ]
            .iter()
            .any(|key| native["query"][*key] != selected_queries[0][*key])
        {
            return Err(bad("executed query metadata differs from native inventory"));
        }
        let stage = v["isolated_source"]
            .as_str()
            .ok_or_else(|| bad("missing predefined graph source root"))?;
        let graph: BTreeSet<String> = paths(native, "graph_files")?
            .iter()
            .map(|p| relative(stage, p).to_string())
            .collect();
        report.complete &= selected.is_subset(&graph)
            && native["unknown_nodes"] == 0
            && native["method_count"].as_u64().is_some_and(|n| n > 0)
            && v["runtime"]["automatic_download"] == false;
        let findings = list(native, "native_findings")?;
        if findings.len() > 1000
            || native["traversal_evidence_count"].as_u64() != Some(findings.len() as u64)
        {
            return Err(bad("native finding count differs from checked traversal"));
        }
        let mut ids = BTreeSet::new();
        for (index, finding) in findings.iter().enumerate() {
            if !ids.insert(
                finding["id"]
                    .as_str()
                    .ok_or_else(|| bad("finding ID missing"))?,
            ) {
                return Err(bad("duplicate native Finding ID"));
            }
            let mut pairs = BTreeMap::new();
            for pair in list(finding, "key_value_pairs")? {
                let key = pair["key"]
                    .as_str()
                    .ok_or_else(|| bad("native finding key missing"))?;
                let value = pair["value"]
                    .as_str()
                    .ok_or_else(|| bad("native finding value missing"))?;
                if pairs.insert(key, value).is_some() {
                    return Err(bad("native finding key duplicate"));
                }
            }
            if ["name", "author", "title", "description"]
                .iter()
                .any(|key| pairs.get(key).copied() != native["query"][*key].as_str())
                || pairs.get("score").and_then(|s| s.parse::<f64>().ok())
                    != native["query"]["score"].as_f64()
            {
                return Err(bad("native Finding metadata differs from selected query"));
            }
            let evidence = list(finding, "evidence")?;
            if evidence.len() != 1 {
                return Err(bad("native Finding evidence is not singular"));
            }
            let node = &evidence[0];
            let file = relative(
                stage,
                node["file"]
                    .as_str()
                    .ok_or_else(|| bad("native Finding source missing"))?,
            );
            if !selected.contains(file)
                || node["label"] != "CALL"
                || !node["code"].as_str().is_some_and(|c| !c.is_empty())
            {
                return Err(bad("native Finding evidence is outside selected C calls"));
            }
            report.observations.push(Observation {rule:QUERY.into(),message:format!("Joern's native nonliteral format heuristic matched {}. Constant format pointers can also match; this does not prove an exploit.",node["code"].as_str().unwrap_or("")),path:Some(file.into()),line:node["line"].as_u64(),pointer:format!("/native_report/native_findings/{index}")});
        }
    }
    report.facts.insert("joern_predefined".into(),json!({"query":native.get("query"),"inventory_count":inventory.len(),"executed_query_count":native.get("query_count"),"selected_files":selected,"discovered_files":discovered,"native_findings":native.get("native_findings"),"parse_problems":v["parse_problems"],"runtime":v["runtime"]}));
    for key in [
        "native_report",
        "commands",
        "configuration",
        "runtime",
        "artifacts",
        "errors",
        "mode",
        "query_inventory",
        "parse_problems",
    ] {
        report.facts.insert(key.into(), v[key].clone());
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    for limitation in list(v, "coverage_limitations")? {
        report.limitations.push(
            limitation
                .as_str()
                .ok_or_else(|| bad("predefined limitation is not text"))?
                .into(),
        );
    }
    if !report.complete {
        report.limitations.push("Native predefined-query execution or graph coverage is incomplete; zero findings cannot clear this criterion.".into());
    }
    Ok(report)
}

#[cfg(test)]
mod predefined_tests {
    use super::*;
    fn fixture(name: &str) -> Value {
        let bytes: &[u8] = match name {
            "defective" => include_bytes!("../../tests/fixtures/joern-predefined/defective.json"),
            "fixed" => include_bytes!("../../tests/fixtures/joern-predefined/fixed.json"),
            "constant" => {
                include_bytes!("../../tests/fixtures/joern-predefined/constant-pointer.json")
            }
            "upstream" => {
                include_bytes!("../../tests/fixtures/joern-predefined/upstream-pair.json")
            }
            "syntax" => include_bytes!("../../tests/fixtures/joern-predefined/syntax-error.json"),
            "unknown" => include_bytes!("../../tests/fixtures/joern-predefined/unknown-query.json"),
            "missing" => {
                include_bytes!("../../tests/fixtures/joern-predefined/missing-querydb.json")
            }
            _ => unreachable!(),
        };
        parse_json(bytes).unwrap()
    }
    #[test]
    fn actual_native_predefined_defect_fixed_and_false_positive() {
        for (name, count) in [
            ("defective", 1),
            ("fixed", 0),
            ("constant", 1),
            ("upstream", 2),
        ] {
            let value = fixture(name);
            let report = predefined(&value, "joern-json-v1").unwrap();
            assert!(report.complete, "{name}: {:?}", report.limitations);
            assert_eq!(report.observations.len(), count);
            assert_eq!(report.facts["joern_predefined"]["inventory_count"], 58);
            assert!(report
                .limitations
                .iter()
                .any(|text| text.contains("false positives")));
        }
    }
    #[test]
    fn actual_native_partial_graph_and_missing_queries_cannot_clear() {
        for name in ["syntax", "unknown", "missing"] {
            let value = fixture(name);
            let report = predefined(&value, "joern-json-v1").unwrap();
            assert!(!report.complete, "{name}");
            assert!(report.observations.is_empty());
        }
        let mut partial = fixture("syntax");
        partial["errors"] = json!([]);
        partial["parse_problems"] = json!([]);
        partial["exit_code"] = json!(0);
        assert!(
            !predefined(&partial, "joern-json-v1").unwrap().complete,
            "unknown native graph nodes remain incomplete even if generic errors are erased"
        );
    }
    #[test]
    fn native_query_identity_locations_and_counts_are_bound() {
        let original = fixture("defective");
        let mut changed = original.clone();
        changed["native_report"]["query"]["author"] = json!("substituted");
        assert!(predefined(&changed, "joern-json-v1").is_err());
        let mut changed = original.clone();
        changed["native_report"]["native_findings"][0]["evidence"][0]["file"] =
            json!("../outside.c");
        assert!(predefined(&changed, "joern-json-v1").is_err());
        let mut changed = original.clone();
        changed["native_report"]["traversal_evidence_count"] = json!(0);
        assert!(predefined(&changed, "joern-json-v1").is_err());
        let mut changed = original;
        let duplicate = changed["query_inventory"][0].clone();
        changed["query_inventory"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(predefined(&changed, "joern-json-v1").is_err());
    }
}
