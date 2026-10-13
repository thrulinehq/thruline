//! Selected OSV native modes retain inventory, policy, metadata and uncertainty.
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn bad(s: &str) -> DecodeError {
    DecodeError::Invalid(s.into())
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing OSV mode array {key}")))
}
fn incomplete(r: &mut Report, s: &str) {
    r.complete = false;
    r.limitations.push(s.into());
}
fn digest(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
fn checked_ref(v: &Value) -> bool {
    v["path"].as_str().is_some_and(|p| !p.is_empty())
        && digest(&v["sha256"])
        && v["bytes"].as_u64().is_some()
}
fn inventory(v: &Value) -> Result<Vec<Value>, DecodeError> {
    let mut rows = Vec::new();
    for (i, source) in array(v, "results")?.iter().enumerate() {
        for (j, p) in array(source, "packages")?.iter().enumerate() {
            for key in ["name", "version", "ecosystem"] {
                if p["package"][key].as_str().is_none_or(str::is_empty) {
                    return Err(bad("missing native package identity"));
                }
            }
            rows.push(json!({"source":source["source"],"package":p["package"],"licenses":p.get("licenses"),"license_violations":p.get("license_violations"),"groups":p.get("groups"),"vulnerability_count":p.get("vulnerabilities").and_then(Value::as_array).map_or(0,Vec::len),"pointer":format!("/results/{i}/packages/{j}")}));
        }
    }
    Ok(rows)
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let v = parse_json(bytes)?;
    if v["schema"] != "thruline.osv-mode-capture.v1" || v["version"] != "2.5.1" {
        return Err(bad("unrecognized OSV mode capture"));
    }
    let mode = v["mode"]
        .as_str()
        .filter(|m| ["sbom", "installed-npm", "image", "go"].contains(m))
        .ok_or_else(|| bad("unsupported OSV mode"))?;
    if !digest(&v["configuration_sha256"])
        || v["configuration"]["schema"] != "thruline.osv-modes.v1"
        || v["configuration"]["mode"] != mode
    {
        return Err(bad("missing selected OSV configuration identity"));
    }
    let mut r = if v["native_report"].is_object() {
        super::decode::decode(
            "osv-json-v1",
            &serde_json::to_vec(&v["native_report"]).map_err(|_| bad("native encoding"))?,
        )?
    } else {
        Report {
            format: "osv-json-v1".into(),
            observations: Vec::new(),
            facts: BTreeMap::new(),
            complete: false,
            limitations: vec![
                "Native OSV JSON was not captured; no clean result is available.".into(),
            ],
        }
    };
    // The legacy JSON normalizer also serves Cargo. Its Cargo-specific UI
    // limitation does not describe explicit artifact/metadata mode execution.
    r.limitations.retain(|s| !s.contains("This Cargo recipe"));
    for n in &mut r.observations {
        n.pointer = format!("/native_report{}", n.pointer);
    }
    if v["complete"] != true || v["original_unchanged"] != true || !array(&v, "errors")?.is_empty()
    {
        incomplete(&mut r,"OSV native execution or source preservation was incomplete. Inspect the retained native phases.");
    }
    if !array(&v, "uncovered_packages")?.is_empty() {
        incomplete(&mut r,"Some package versions are outside the prepared advisory snapshot; no absence conclusion is available for them.");
    }
    let commands = array(&v, "commands")?;
    for phase in ["unfiltered", "selected-policy"] {
        let rows: Vec<_> = commands.iter().filter(|x| x["name"] == phase).collect();
        if rows.len() != 1 {
            return Err(bad("missing or repeated OSV native phase"));
        }
        if !matches!(rows[0]["exit_code"].as_i64(), Some(0 | 1))
            || !checked_ref(&rows[0]["stdout"])
            || !checked_ref(&rows[0]["stderr"])
        {
            incomplete(
                &mut r,
                "A required native phase failed or lacks bounded stream identities.",
            );
        }
        if !v["configuration"]["metadata"].is_null() {
            let transport = &rows[0]["metadata_transport"];
            if transport["used"].as_array().is_none_or(Vec::is_empty)
                || transport["errors"].as_array().is_none_or(|a| !a.is_empty())
                || !digest(&transport["manifest_sha256"])
            {
                incomplete(&mut r,"The native metadata transport did not prove complete prepared metadata access.");
            }
        }
    }
    let effective = if v["native_report"].is_object() {
        inventory(&v["native_report"])?
    } else {
        Vec::new()
    };
    let raw = if v["unfiltered_report"].is_object() {
        inventory(&v["unfiltered_report"])?
    } else {
        incomplete(
            &mut r,
            "The unfiltered native inventory is missing; suppression provenance is incomplete.",
        );
        Vec::new()
    };
    if raw.is_empty() {
        incomplete(
            &mut r,
            "OSV did not identify any package within the selected input.",
        );
    }
    for (i, source) in v["native_report"]["results"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        for (j, p) in source["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let name = p["package"]["name"].as_str().unwrap_or("unknown");
            let version = p["package"]["version"].as_str().unwrap_or("unknown");
            let path = source["source"]["path"].as_str().map(str::to_owned);
            if let Some(violations) = p.get("license_violations") {
                for (k, license) in violations
                    .as_array()
                    .ok_or_else(|| bad("native license violations must be an array"))?
                    .iter()
                    .enumerate()
                {
                    let license = license
                        .as_str()
                        .ok_or_else(|| bad("native license expression missing"))?;
                    r.observations.push(Observation{rule:"osv.license-policy".into(),message:format!("{name} {version}: declared license {license} is outside the selected allowlist."),path:path.clone(),line:None,pointer:format!("/native_report/results/{i}/packages/{j}/license_violations/{k}")});
                }
            }
            if p["package"]["deprecated"] == true {
                r.observations.push(Observation {
                    rule: "osv.deprecated-package".into(),
                    message: format!(
                        "{name} {version} was marked deprecated in the prepared deps.dev metadata."
                    ),
                    path,
                    line: None,
                    pointer: format!("/native_report/results/{i}/packages/{j}/package/deprecated"),
                });
            }
        }
    }
    if mode == "go" {
        let groups: Vec<_> = raw
            .iter()
            .flat_map(|p| p["groups"].as_array().into_iter().flatten())
            .collect();
        if groups.is_empty()
            || groups.iter().any(|g| {
                g["experimental_analysis"]
                    .as_object()
                    .is_none_or(|o| o.is_empty())
            })
        {
            incomplete(&mut r,"Native symbol analysis is unavailable for at least one advisory group; missing analysis is not called=false.");
        }
        let configuration = &v["analysis_configuration"];
        if !checked_ref(&configuration["toolchain"])
            || !checked_ref(&configuration["version_stdout"])
            || configuration["version_exit_code"] != 0
            || configuration["environment"]["CGO_ENABLED"] != "0"
            || configuration["environment"]["GOPROXY"] != "off"
        {
            incomplete(&mut r,"The pinned offline Go toolchain and Cgo-disabled analysis scope were not retained.");
        }
    }
    r.facts
        .insert("finding_count".into(), json!(r.observations.len()));
    r.facts.insert("osv_mode".into(),json!({"mode":mode,"configuration":v["configuration"],"source_hashes":v["source_hashes"],"unfiltered_inventory":raw,"selected_inventory":effective,"policy":v["policy"],"native_image_metadata":v["native_report"].get("image_metadata"),"dataset_scope":v["dataset_scope"],"uncovered_packages":v["uncovered_packages"],"commands":v["commands"],"errors":v["errors"],"archive_budget":v["archive_budget"],"analysis_configuration":v["analysis_configuration"],"runtime_manifest":v["runtime_manifest"],"dataset_files":v["dataset_files"],"artifacts":v["artifacts"]}));
    r.limitations.extend(["Native static symbol analysis prioritizes known package advisories; it does not prove dynamic exploitability or absence of exploitable flows.".into(),"A policy-suppressed advisory remains in the unfiltered inventory. Suppression and license overrides are not vulnerability repairs.".into(),"License and deprecation records are dated upstream package metadata. SPDX output is an inventory format; the separate native JSON supplies findings.".into()]);
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn license_policy_is_a_distinct_native_observation() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/metadata-disallowed.json"
        ))
        .unwrap();
        assert!(r.complete);
        assert_eq!(r.observations.len(), 1);
        assert_eq!(r.observations[0].rule, "osv.license-policy");
        assert_eq!(r.facts["finding_count"], 1);
        assert!(!r.limitations.iter().any(|s| s.contains("Cargo recipe")));
        assert_eq!(
            r.facts["osv_mode"]["selected_inventory"][0]["vulnerability_count"],
            0
        );
    }
    #[test]
    fn true_deprecation_does_not_supply_missing_advisory_coverage() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/deprecated-uncovered.json"
        ))
        .unwrap();
        assert!(!r.complete);
        assert!(r
            .observations
            .iter()
            .any(|o| o.rule == "osv.deprecated-package"));
        assert!(!r.facts["osv_mode"]["uncovered_packages"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn suppression_preserves_the_original_advisory_inventory() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/all-suppressed.json"
        ))
        .unwrap();
        assert!(r.complete);
        assert!(r.observations.is_empty());
        assert_eq!(
            r.facts["osv_mode"]["unfiltered_inventory"][0]["vulnerability_count"],
            2
        );
        assert_eq!(
            r.facts["osv_mode"]["selected_inventory"][0]["vulnerability_count"],
            0
        );
        assert!(r
            .limitations
            .iter()
            .any(|s| s.contains("not vulnerability repairs")));
    }
    #[test]
    fn actual_image_layer_identity_is_retained() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/image-affected.json"
        ))
        .unwrap();
        assert!(r.complete);
        assert_eq!(r.observations.len(), 2);
        assert_eq!(
            r.facts["osv_mode"]["selected_inventory"][0]["package"]["image_origin_details"]
                ["index"],
            1
        );
        assert_eq!(
            r.facts["osv_mode"]["native_image_metadata"]["layer_metadata"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn native_metadata_errors_never_become_an_empty_clean_check() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/metadata-missing.json"
        ))
        .unwrap();
        assert!(!r.complete);
        assert!(r.observations.is_empty());
        assert!(r
            .limitations
            .iter()
            .any(|s| s.contains("metadata transport")));
    }
    #[test]
    fn missing_go_analysis_is_not_called_false() {
        let mut v = parse_json(include_bytes!(
            "../../tests/fixtures/osv_modes/all-suppressed.json"
        ))
        .unwrap();
        v["mode"] = json!("go");
        v["configuration"]["mode"] = json!("go");
        let r = decode(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(!r.complete);
        assert!(r
            .limitations
            .iter()
            .any(|s| s.contains("missing analysis is not called=false")));
    }
    #[test]
    fn actual_go_analysis_retains_called_and_uncalled_advisories() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/go-called.json"
        ))
        .unwrap();
        assert!(r.complete);
        assert_eq!(r.observations.len(), 2);
        let raw = r.facts["osv_mode"]["unfiltered_inventory"]
            .as_array()
            .unwrap();
        let ipfs = raw
            .iter()
            .find(|p| p["package"]["name"] == "github.com/ipfs/go-bitfield")
            .unwrap();
        let gogo = raw
            .iter()
            .find(|p| p["package"]["name"] == "github.com/gogo/protobuf")
            .unwrap();
        assert_eq!(
            ipfs["groups"][0]["experimental_analysis"]["GO-2023-1558"]["called"],
            true
        );
        assert_eq!(
            gogo["groups"][0]["experimental_analysis"]["GO-2021-0053"]["called"],
            false
        );
        assert_eq!(
            r.facts["osv_mode"]["analysis_configuration"]["environment"]["CGO_ENABLED"],
            "0"
        );
    }
    #[test]
    fn actual_go_advisory_without_symbol_metadata_is_incomplete() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/go-metadata-gap.json"
        ))
        .unwrap();
        assert!(!r.complete);
        assert_eq!(r.observations.len(), 2);
        let raw = r.facts["osv_mode"]["unfiltered_inventory"]
            .as_array()
            .unwrap();
        let ipfs = raw
            .iter()
            .find(|p| p["package"]["name"] == "github.com/ipfs/go-bitfield")
            .unwrap();
        assert!(ipfs["groups"][0]["experimental_analysis"].is_null());
        assert!(r
            .limitations
            .iter()
            .any(|s| s.contains("missing analysis is not called=false")));
    }
    #[test]
    fn actual_go_compiler_failure_preserves_static_advisories() {
        let r = decode(include_bytes!(
            "../../tests/fixtures/osv_modes/go-compiler-gap.json"
        ))
        .unwrap();
        assert!(!r.complete);
        assert_eq!(r.observations.len(), 2);
        assert!(r.facts["osv_mode"]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s
                .as_str()
                .unwrap()
                .contains("incomplete extraction or call graph")));
    }
    #[test]
    fn removing_one_native_analysis_cannot_reuse_other_groups_success() {
        let mut v = parse_json(include_bytes!(
            "../../tests/fixtures/osv_modes/go-called.json"
        ))
        .unwrap();
        v["unfiltered_report"]["results"][0]["packages"][0]["groups"][0]["experimental_analysis"] =
            json!({});
        let r = decode(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(!r.complete);
        assert_eq!(r.observations.len(), 2);
    }
}
