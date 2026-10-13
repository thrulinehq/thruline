//! Validate and disclose the resolver scope reported by the native producer.
use super::{invalid, DecodeError, Report};
use serde_json::Value;

pub(super) fn capture(report: &mut Report, value: &Value) -> Result<(), DecodeError> {
    let modules = value["modules"]
        .as_array()
        .ok_or_else(|| invalid("missing native modules"))?;
    if modules.is_empty() {
        report.complete = false;
        report.limitations.push(
            "The native dependency graph contains no modules; source coverage is unestablished."
                .into(),
        );
    }
    for module in modules {
        if let Some(dependencies) = module["dependencies"].as_array() {
            for edge in dependencies {
                for key in ["module", "moduleSystem"] {
                    if edge.get(key).is_some_and(|v| v.as_str().is_none()) {
                        return Err(invalid("invalid native dependency resolver identity"));
                    }
                }
                if edge
                    .get("preCompilationOnly")
                    .is_some_and(|v| v.as_bool().is_none())
                {
                    return Err(invalid("invalid native precompilation classification"));
                }
                if let Some(types) = edge.get("dependencyTypes") {
                    if !types
                        .as_array()
                        .is_some_and(|items| items.iter().all(|v| v.as_str().is_some()))
                    {
                        return Err(invalid("invalid native dependency types"));
                    }
                }
            }
        }
    }
    if let Some(mode) = value["summary"]["optionsUsed"].get("tsPreCompilationDeps") {
        if mode.as_bool().is_none() && mode.as_str() != Some("specify") {
            return Err(invalid("unknown native precompilation mode"));
        }
    }
    let Some(environment) = value["summary"].get("environment") else {
        report
            .limitations
            .push("Native parser availability was not recorded for this historical graph.".into());
        return Ok(());
    };
    if !environment.is_object() {
        return Err(invalid("invalid native parser environment"));
    }
    for (key, identity) in [
        ("transpilersFound", "name"),
        ("extensionsFound", "extension"),
    ] {
        if let Some(rows) = environment.get(key) {
            let rows = rows
                .as_array()
                .ok_or_else(|| invalid("invalid native parser availability list"))?;
            for row in rows {
                if row[identity].as_str().is_none() || row["available"].as_bool().is_none() {
                    return Err(invalid("invalid native parser availability"));
                }
                if row
                    .get("currentVersion")
                    .is_some_and(|v| v.as_str().is_none())
                {
                    return Err(invalid("invalid native parser version"));
                }
            }
        }
    }
    if let Some(issues) = environment.get("issues") {
        let issues = issues
            .as_array()
            .ok_or_else(|| invalid("invalid native environment issues"))?;
        for issue in issues {
            let severity = issue["severity"]
                .as_str()
                .ok_or_else(|| invalid("missing native environment issue severity"))?;
            let name = issue["name"]
                .as_str()
                .ok_or_else(|| invalid("missing native environment issue identity"))?;
            let description = issue["description"]
                .as_str()
                .ok_or_else(|| invalid("missing native environment issue description"))?;
            if matches!(severity, "warn" | "error") || name.starts_with("missing-") {
                report.complete = false;
            }
            report.limitations.push(format!(
                "Native parser environment {severity} ({name}): {description}"
            ));
        }
    }
    Ok(())
}
