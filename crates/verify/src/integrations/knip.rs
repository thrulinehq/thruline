//! Knip native issue families. Locations, groups and scope remain producer facts.
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::BTreeMap;
const CATEGORIES: &[&str] = &[
    "binaries",
    "catalog",
    "catalogReferences",
    "cycles",
    "dependencies",
    "devDependencies",
    "duplicates",
    "enumMembers",
    "exports",
    "files",
    "namespaceMembers",
    "nsExports",
    "nsTypes",
    "optionalPeerDependencies",
    "types",
    "unlisted",
    "unresolved",
];
fn bad(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, DecodeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad(&format!("missing Knip {key}")))
}
fn position(value: &Value, key: &str) -> Result<Option<u64>, DecodeError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|n| key == "pos" || *n > 0)
            .map(Some)
            .ok_or_else(|| bad("invalid Knip source location")),
    }
}
fn symbol(value: &Value, allow_empty: bool) -> Result<String, DecodeError> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| allow_empty || !s.is_empty())
        .ok_or_else(|| bad("missing Knip symbol name"))?;
    for key in ["line", "col", "pos"] {
        position(value, key)?;
    }
    for key in ["namespace", "kind", "specifier"] {
        if value
            .get(key)
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(bad("invalid Knip symbol metadata"));
        }
    }
    Ok(
        if let Some(namespace) = value
            .get("namespace")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            format!("{namespace}.{name}")
        } else {
            name.into()
        },
    )
}
fn symbol_projection(value: &Value) -> Value {
    let mut object = serde_json::Map::new();
    for key in [
        "name",
        "namespace",
        "kind",
        "specifier",
        "line",
        "col",
        "pos",
    ] {
        if let Some(value) = value.get(key).filter(|value| !value.is_null()) {
            object.insert(key.into(), value.clone());
        }
    }
    Value::Object(object)
}
fn issue_key(category: &str, path: &str, item: &Value) -> String {
    let item = if let Some(members) = item.as_array() {
        Value::Array(members.iter().map(symbol_projection).collect())
    } else {
        symbol_projection(item)
    };
    json!([category, path, item]).to_string()
}
fn incomplete(report: &mut Report, message: &str) {
    report.complete = false;
    report.limitations.push(message.into());
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let value = parse_json(bytes)?;
    let issues = value
        .get("issues")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("missing Knip issues array"))?;
    let mut result = Report {
        format: "knip-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: true,
        limitations: vec![],
    };
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut issue_keys = BTreeMap::<String, u64>::new();
    for (index, row) in issues.iter().enumerate() {
        let path = text(row, "file")?;
        for (category, entries) in row
            .as_object()
            .ok_or_else(|| bad("Knip issue must be object"))?
        {
            if ["file", "owners"].contains(&category.as_str()) {
                continue;
            }
            if !CATEGORIES.contains(&category.as_str()) {
                return Err(bad("unrecognized Knip issue category"));
            }
            for (item_index, item) in entries
                .as_array()
                .ok_or_else(|| bad("Knip category must be array"))?
                .iter()
                .enumerate()
            {
                let grouped = ["duplicates", "cycles"].contains(&category.as_str());
                let (names, line) = if grouped && item.is_array() {
                    let members = item.as_array().unwrap();
                    if members.is_empty() || (category == "duplicates" && members.len() < 2) {
                        return Err(bad("invalid Knip duplicate/cycle group size"));
                    }
                    (
                        members
                            .iter()
                            .map(|member| symbol(member, false))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(" ↔ "),
                        position(&members[0], "line")?,
                    )
                } else {
                    // Legacy coverage reporters flattened a group to name/symbol strings.
                    if item.is_array() || !item.is_object() {
                        return Err(bad("Knip issue must contain a named symbol"));
                    }
                    (symbol(item, category == "files")?, position(item, "line")?)
                };
                *issue_keys
                    .entry(issue_key(category, path, item))
                    .or_default() += 1;
                let message = match category.as_str() {
                    "unlisted" => format!("Dependency `{names}` is imported but not listed for this workspace. Review the owning package manifest."),
                    "unresolved" => format!("Import `{names}` could not be resolved. Its dependency graph remains incomplete."),
                    "duplicates" => format!("Duplicate exports refer to the same value: {names}. Preserve public aliases unless the API change is intentional."),
                    "catalog" => format!("Catalog entry `{names}` has no captured reference in the selected workspace scope."),
                    "catalogReferences" => format!("Catalog reference `{names}` has no matching entry. Package-manager resolution remains incomplete."),
                    "cycles" => format!("Native dependency cycle: {names}."),
                    _ => format!("Possible unused {category}: `{names}`. Review configured and dynamic use before removal."),
                };
                result.observations.push(Observation {
                    rule: category.clone(),
                    message,
                    path: Some(path.into()),
                    line,
                    pointer: format!("/issues/{index}/{category}/{item_index}"),
                });
                *counts.entry(category.as_str()).or_default() += 1;
                if ["unresolved", "catalogReferences"].contains(&category.as_str()) {
                    incomplete(
                        &mut result,
                        "Unresolved imports or catalog references limit this dependency inventory.",
                    );
                }
            }
        }
    }
    result
        .facts
        .insert("knip_issues".into(), value["issues"].clone());
    result
        .facts
        .insert("knip_issue_counts".into(), json!(counts));
    if let Some(native) = value.get("nativeIssues") {
        let native = native
            .as_array()
            .ok_or_else(|| bad("Knip nativeIssues must be array"))?;
        let mut native_counts: BTreeMap<&str, u64> = BTreeMap::new();
        let mut native_keys = BTreeMap::<String, u64>::new();
        for row in native {
            let category = text(row, "category")?;
            if !CATEGORIES.contains(&category) {
                return Err(bad("unknown native Knip issue category"));
            }
            let path = text(row, "file")?;
            if !row.get("workspace").is_some_and(Value::is_string) {
                return Err(bad("missing native Knip workspace"));
            }
            symbol(row, category == "files")?;
            let item = if ["duplicates", "cycles"].contains(&category) {
                let symbols = row
                    .get("symbols")
                    .and_then(Value::as_array)
                    .ok_or_else(|| bad("missing native Knip group symbols"))?;
                for member in symbols {
                    symbol(member, false)?;
                }
                row.get("symbols").unwrap()
            } else {
                row
            };
            *native_keys
                .entry(issue_key(category, path, item))
                .or_default() += 1;
            *native_counts.entry(category).or_default() += 1;
        }
        if issue_keys != native_keys {
            return Err(bad(
                "Knip native and reported symbols or source locations disagree",
            ));
        }
        if counts != native_counts {
            return Err(bad("Knip native and reported category counts disagree"));
        }
        result
            .facts
            .insert("knip_native_issues".into(), json!(native));
    }
    if let Some(coverage) = value.get("coverage") {
        if text(coverage, "schema")? != "thruline.knip-coverage.v1" {
            return Err(bad("unsupported Knip coverage schema"));
        }
        for field in ["includedWorkspaceDirs", "selectedWorkspaces"] {
            if let Some(value) = coverage.get(field).filter(|value| !value.is_null()) {
                let rows = value
                    .as_array()
                    .ok_or_else(|| bad("invalid Knip workspace scope"))?;
                if rows.iter().any(|row| !row.is_string()) {
                    return Err(bad("invalid Knip workspace selector"));
                }
            }
        }
        if let Some(options) = coverage.get("nativeOptions") {
            if !options.is_object()
                || !options.get("isProduction").is_some_and(Value::is_boolean)
                || !options
                    .get("configFilePath")
                    .is_some_and(|value| value.is_null() || value.is_string())
            {
                return Err(bad("invalid native Knip configuration scope"));
            }
        }
        let counters = coverage
            .get("counters")
            .and_then(Value::as_object)
            .ok_or_else(|| bad("missing Knip counters"))?;
        let number = |key: &str| {
            counters
                .get(key)
                .and_then(Value::as_u64)
                .ok_or_else(|| bad("invalid Knip counter"))
        };
        let processed = number("processed")?;
        let total = number("total")?;
        if processed > total {
            return Err(bad("Knip processed count exceeds inventory"));
        }
        if total == 0 || processed != total {
            incomplete(&mut result, "Some project files were ignored or not processed. An empty issue list does not cover those files.");
        }
        let hints = coverage
            .get("configurationHints")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("missing Knip configuration hints"))?;
        if !hints.is_empty() {
            incomplete(&mut result, "Review the captured configuration hints; the selected inventory may differ from the intended scope.");
        }
        if let Some(enabled) = coverage.get("report") {
            let enabled = enabled
                .as_object()
                .ok_or_else(|| bad("invalid Knip report filter"))?;
            for (category, selected) in enabled {
                if !CATEGORIES.contains(&category.as_str()) || !selected.is_boolean() {
                    return Err(bad("invalid Knip report category selection"));
                }
                if counters.contains_key(category) {
                    let observed = counts.get(category.as_str()).copied().unwrap_or(0);
                    let count = number(category)?;
                    if selected == true && count != observed {
                        return Err(bad("Knip enabled issue count differs from native counter"));
                    }
                    if selected == false && count > 0 {
                        incomplete(&mut result, "Native issue categories with findings were excluded from the displayed report.");
                    }
                }
            }
        }
        result
            .facts
            .insert("scan_coverage".into(), coverage.clone());
        result.facts.insert("knip_scope".into(), json!({
            "included_workspaces":coverage.get("includedWorkspaceDirs"), "selected_workspaces":coverage.get("selectedWorkspaces"), "enabled_plugins":coverage.get("enabledPlugins"), "native_options":coverage.get("nativeOptions"),
            "compiler_boundary":"Knip built-in compilers extract imports from supported non-JavaScript files; they do not establish unused-export coverage inside those files. Custom compiler behavior depends on the accepted configuration and its runtime.",
            "source_mapping_boundary":"Knip source mapping depends on the selected tsconfig, package exports and workspace resolver. These counters do not identify every mapping or prove that build artifacts were compiled.",
            "catalog_boundary":"Catalog entries and missing catalog references are distinct native families. Selected workspaces, catalog precedence and package-manager configuration limit the result. No lockfile upgrade or dependency installation was performed."}));
    } else {
        incomplete(&mut result, "This Knip finding list has no processed/inventory counts. Run with the provisioned coverage reporter.");
    }
    result.limitations.push("Reachability depends on configured entrypoints, plugins and dynamic loading. No automatic deletion.".into());
    result
        .facts
        .insert("finding_count".into(), json!(result.observations.len()));
    Ok(result)
}

#[cfg(test)]
#[path = "knip_tests.rs"]
mod tests;
