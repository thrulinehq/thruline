//! Distinct native Properties evidence and acquired module availability.
use super::super::{DecodeError, Observation, Report};
use super::{bad, list, paths};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) fn decode(v: &Value, format: &str) -> Result<Report, DecodeError> {
    if v["schema"] != "thruline.openrewrite-module-capture.v1" || v["version"] != "8.90.4" {
        return Err(bad("unknown OpenRewrite module capture"));
    }
    let availability = &v["module_availability"];
    if availability["schema"] != "thruline.openrewrite-modules.v1"
        || availability["runtime_version"] != "8.90.4"
    {
        return Err(bad("unknown acquired OpenRewrite module inventory"));
    }
    let selected = paths(v, "selected_files")?;
    let discovered = paths(v, "discovered_files")?;
    let commands = list(v, "commands")?;
    let native = &v["native_report"];
    let mut report = Report {
        format: format.into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: false,
        limitations: vec![],
    };
    let schema = v["configuration"]["schema"].as_str().unwrap_or("");
    if schema == "thruline.openrewrite-properties.v1" {
        let module: Vec<_> = list(availability, "modules")?
            .iter()
            .filter(|m| m["selected"] == true)
            .collect();
        if module.len() != 1
            || module[0]["module"] != "rewrite-properties"
            || module[0]["restricted"] != false
            || module[0]["runtime_present"] != true
            || availability["selected_module"] != "rewrite-properties"
        {
            return Err(bad(
                "native Properties module identity differs from selection",
            ));
        }
        report.complete = !selected.is_empty()
            && selected.len() == list(v, "selected_files")?.len()
            && selected == discovered
            && selected.iter().all(|p| p.ends_with(".properties"))
            && v["exit_code"] == 0
            && v["original_unchanged"] == true
            && v["dry_run_source_unchanged"] == true
            && list(v, "errors")?.is_empty()
            && list(v, "symlinks_not_followed")?.is_empty()
            && commands.iter().all(|c| c["exit_code"] == 0)
            && commands
                .iter()
                .any(|c| c["stage"] == "native-properties-recipe");
        if native.is_null() {
            report.complete = false;
        } else {
            if native["schema"] != "thruline.openrewrite-properties-native.v1"
                || native["module"] != "rewrite-properties"
                || native["recipe"] != "org.openrewrite.properties.ChangePropertyValue"
                || native["recipe"] != v["configuration"]["recipe"]
                || native["exact_key"] != v["configuration"]["options"]["property_key"]
                || native["regex"] != false
                || native["relaxed_binding"] != false
            {
                return Err(bad(
                    "native Properties recipe identity differs from accepted configuration",
                ));
            }
            report.complete &= paths(native, "parsed_files")? == selected
                && list(native, "parsed_files")?.len() == selected.len()
                && list(native, "errors")?.is_empty()
                && native["matching_key_count"].as_u64().is_some_and(|n| n > 0)
                && native["unexpected_value_count"] == 0
                && native["fixed_point_changes"] == 0;
            for entry in list(native, "parsed_entries")? {
                if !entry["path"].as_str().is_some_and(|p| selected.contains(p))
                    || entry["key"].as_str().is_none()
                    || entry["value"].as_str().is_none()
                {
                    return Err(bad(
                        "native Properties entry has no selected source/key/value",
                    ));
                }
            }
            for (index, change) in list(native, "changes")?.iter().enumerate() {
                let path = change["path"]
                    .as_str()
                    .ok_or_else(|| bad("native Properties patch has no path"))?;
                if !selected.contains(path)
                    || !change["diff"].as_str().is_some_and(|s| !s.is_empty())
                    || change["before"].as_str().is_none()
                    || change["after"].as_str().is_none()
                    || change["before"] == change["after"]
                {
                    return Err(bad(
                        "native Properties patch lacks selected before/after source",
                    ));
                }
                report.observations.push(Observation {
                    rule: "org.openrewrite.properties.ChangePropertyValue".into(),
                    message: "The accepted exact Properties value migration has a native proposed patch; independent consumer behavior remains required before apply.".into(),
                    path: Some(path.into()), line: None, pointer: format!("/native_report/changes/{index}"),
                });
            }
        }
        report.facts.insert("properties_scope".into(), json!({"selected": selected, "discovered": discovered,
            "recipe":v["configuration"]["recipe"],"options":v["configuration"]["options"],
            "parsed_entries":native["parsed_entries"],"matching_key_count":native["matching_key_count"],
            "unexpected_value_count":native["unexpected_value_count"],"fixed_point_changes":native["fixed_point_changes"],
            "compile_performed":false,"scope":"Native Properties parse and exact-key value migration only. No Java type attribution, unselected parser, or independent behavior proof is inferred."}));
    } else if schema == "thruline.openrewrite-module.v1" {
        if !native.is_null() || !commands.is_empty() || list(v, "errors")?.is_empty() {
            return Err(bad(
                "unimplemented module selection must retain explicit nonexecution and a gap",
            ));
        }
    } else {
        return Err(bad("unknown OpenRewrite module selection schema"));
    }
    for key in [
        "module_availability",
        "native_report",
        "commands",
        "configuration",
        "runtime",
        "artifacts",
        "errors",
        "mode",
        "applied_files",
    ] {
        if !v[key].is_null() {
            report.facts.insert(key.into(), v[key].clone());
        }
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    for limitation in list(v, "coverage_limitations")? {
        report.limitations.push(
            limitation
                .as_str()
                .ok_or_else(|| bad("invalid OpenRewrite module limitation"))?
                .into(),
        );
    }
    if !report.complete {
        report.limitations.push("The selected module, parser, source or recipe scope is incomplete. Runtime presence and zero proposed changes cannot establish a completed migration.".into());
    }
    Ok(report)
}
