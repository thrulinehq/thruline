//! Native selected frontend structure, separate from independent program behavior.
use super::super::{DecodeError, Observation, Report};
use super::{bad, list, paths, relative};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn decode(v: &Value, format: &str) -> Result<Report, DecodeError> {
    if v["schema"] != "thruline.joern-frontend-capture.v1"
        || v["version"] != "4.0.626"
        || v["frontend_inventory"]["schema"] != "thruline.joern-frontend-inventory.v1"
    {
        return Err(bad("unknown Joern frontend capture"));
    }
    let config = &v["configuration"];
    let frontend = config["frontend"]
        .as_str()
        .ok_or_else(|| bad("selected frontend missing"))?;
    let selected = paths(v, "selected_files")?;
    let discovered = paths(v, "discovered_files")?;
    let commands = list(v, "commands")?;
    let native = &v["native_report"];
    let inventory = &v["frontend_inventory"];
    let current: Vec<_> = list(inventory, "frontends")?
        .iter()
        .filter(|x| x["selected"] == true)
        .collect();
    let mut report = Report {
        format: format.into(),
        complete: false,
        observations: vec![],
        facts: BTreeMap::new(),
        limitations: vec![],
    };
    if config["schema"] == "thruline.joern-frontend.v1" {
        let binary = frontend == "ghidra2cpg";
        let rule = if binary {
            "binary-structure"
        } else {
            "python-no-string-conversion"
        };
        if !["pysrc2cpg", "ghidra2cpg"].contains(&frontend)
            || config["rule"] != rule
            || v["binary_scope"] != binary
            || current.len() != 1
            || current[0]["frontend"] != frontend
            || inventory["selected_frontend"] != frontend
        {
            return Err(bad(
                "frontend/query inventory identity differs from selection",
            ));
        }
        report.complete = !selected.is_empty()
            && selected.len() == list(v, "selected_files")?.len()
            && selected == discovered
            && (!binary || selected.len() == 1)
            && current[0]["runtime_present"] == true
            && v["original_unchanged"] == true
            && v["exit_code"] == 0
            && list(v, "errors")?.is_empty()
            && list(v, "parse_problems")?.is_empty()
            && list(v, "symlinks_not_followed")?.is_empty()
            && v["artifact_retention"]["complete"] == true
            && commands.len() == 2
            && ["parse", "query"].iter().all(|stage| {
                commands
                    .iter()
                    .any(|c| c["stage"] == *stage && c["exit_code"] == 0)
            })
            && commands.iter().all(|c| c["exit_code"] == 0);
        if native.is_null() {
            report.complete = false;
        } else {
            if native["schema"] != "thruline.joern-frontend-native.v1"
                || native["frontend"] != frontend
                || native["rule"] != rule
            {
                return Err(bad("native structural query identity mismatch"));
            }
            let root = v["isolated_source"]
                .as_str()
                .ok_or_else(|| bad("native copied source root missing"))?;
            let graph: BTreeSet<_> = paths(native, "graph_files")?
                .iter()
                .map(|p| relative(root, p).to_string())
                .collect();
            let languages = paths(native, "languages")?;
            let methods = list(native, "selected_methods")?;
            report.complete &= selected.is_subset(&graph)
                && languages
                    == BTreeSet::from([if binary { "GHIDRA" } else { "PYTHONSRC" }.to_string()])
                && native["selected_method_count"].as_u64() == Some(methods.len() as u64)
                && !methods.is_empty()
                && native["unknown_nodes"] == 0
                && native["method_inventory_truncated"] == false
                && list(native, "errors")?.is_empty();
            for method in methods {
                if method["name"] != config["method"]
                    || method["is_external"] != false
                    || !method["file"]
                        .as_str()
                        .is_some_and(|p| selected.contains(relative(root, p)))
                {
                    return Err(bad("native selected method escapes accepted method/source"));
                }
            }
            for (index, finding) in list(native, "findings")?.iter().enumerate() {
                if binary {
                    return Err(bad(
                        "binary structure availability does not produce defect findings",
                    ));
                }
                let file = finding["file"]
                    .as_str()
                    .ok_or_else(|| bad("Python structural finding has no location"))?;
                let file = relative(root, file);
                if !selected.contains(file)
                    || finding["name"] != "str"
                    || finding["code"].as_str().is_none()
                    || !methods
                        .iter()
                        .any(|method| method["full_name"] == finding["method"])
                {
                    return Err(bad(
                        "Python structural finding escapes query/source selection",
                    ));
                }
                report.observations.push(Observation{rule:rule.into(),message:"The selected Python method contains a native call named str. This structural observation does not prove builtin binding or runtime return type; independent consumer behavior remains separate.".into(),path:Some(file.into()),line:finding["line"].as_u64(),pointer:format!("/native_report/findings/{index}")});
            }
        }
    } else if config["schema"] == "thruline.joern-frontend-availability.v1" {
        if !native.is_null() || !commands.is_empty() || list(v, "errors")?.is_empty() {
            return Err(bad(
                "availability-only selection requires explicit nonexecution and a gap",
            ));
        }
    } else {
        return Err(bad("unknown frontend selection schema"));
    }
    report.facts.insert("joern_frontend_scope".into(),json!({"frontend":frontend,"selected":selected,"discovered":discovered,"binary_scope":v["binary_scope"],"rule":config["rule"],"method":config["method"],"native":native,"parse_problems":v["parse_problems"],"errors":v["errors"],"artifact_retention":v["artifact_retention"],"assessment_scope":if v["binary_scope"]==true{"Concrete native method graph availability for one binary object only; no defect-absence or source-equivalence claim."}else{"Selected Python structural call-name query; builtin binding, types and consumer behavior are not proven by this graph."}}));
    for key in [
        "frontend_inventory",
        "commands",
        "configuration",
        "artifacts",
        "artifact_retention",
        "errors",
        "native_report",
    ] {
        if !v[key].is_null() {
            report.facts.insert(key.into(), v[key].clone());
        }
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    for limit in list(v, "coverage_limitations")? {
        report.limitations.push(
            limit
                .as_str()
                .ok_or_else(|| bad("invalid frontend limitation"))?
                .into(),
        );
    }
    if !report.complete {
        report.limitations.push("This frontend selection has an availability, parser, source or query gap. No finding count can clear an incomplete scope.".into());
    }
    Ok(report)
}
