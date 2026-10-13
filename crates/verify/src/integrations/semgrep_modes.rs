//! Native explicit targeting, ancestor-differential and rule/fixtest assertions.
//! A differential success says nothing about older findings or whole behavior.
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn bad(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn ensure(ok: bool, message: &str) -> Result<(), DecodeError> {
    if ok {
        Ok(())
    } else {
        Err(bad(message))
    }
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing Semgrep mode array {key}")))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, DecodeError> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(&format!("missing Semgrep mode text {key}")))
}
fn incomplete(r: &mut Report, message: &str) {
    r.complete = false;
    r.limitations.push(message.into());
}
fn path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains(['\\', '\0', '\n', '\r'])
        && value
            .split('/')
            .all(|x| !x.is_empty() && x != "." && x != ".." && x != ".git")
}
fn hex(value: &Value, n: usize) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
fn lines(v: &Value) -> Result<BTreeSet<u64>, DecodeError> {
    let values = v
        .as_array()
        .ok_or_else(|| bad("missing native assertion line array"))?;
    let result: BTreeSet<u64> = values
        .iter()
        .map(|x| {
            x.as_u64()
                .filter(|n| *n > 0)
                .ok_or_else(|| bad("invalid assertion line"))
        })
        .collect::<Result<_, _>>()?;
    ensure(result.len() == values.len(), "repeated assertion line")?;
    Ok(result)
}

fn run<'a>(runs: &'a [Value], name: &str) -> Result<&'a Value, DecodeError> {
    let matching: Vec<_> = runs.iter().filter(|v| v["name"] == name).collect();
    ensure(matching.len() == 1, "missing or repeated native phase")?;
    Ok(matching[0])
}
fn output(run: &Value) -> Result<Value, DecodeError> {
    let bytes = run["stdout"]["text"]
        .as_str()
        .ok_or_else(|| bad("missing native stdout"))?;
    parse_json(bytes.as_bytes())
}
fn scan(
    r: &mut Report,
    run: &Value,
    source: &Value,
    selected: &Value,
) -> Result<Value, DecodeError> {
    let v = output(run)?;
    ensure(
        v["version"] == "1.176.0",
        "unrecognized native Semgrep scan version",
    )?;
    let findings = array(&v, "results")?;
    let errors = array(&v, "errors")?;
    let scanned = array(&v["paths"], "scanned")?;
    let skipped = array(&v["paths"], "skipped")?;
    let mut names = BTreeSet::new();
    for value in scanned {
        let name = value
            .as_str()
            .ok_or_else(|| bad("nontext native source path"))?;
        ensure(
            path(name) && source.get(name).is_some() && names.insert(name),
            "unbound or repeated native source path",
        )?;
    }
    for f in findings {
        let file = text(f, "path")?;
        ensure(
            names.contains(file),
            "finding outside native scanned source",
        )?;
        text(f, "check_id")?;
        text(&f["extra"], "message")?;
        ensure(
            f["start"]["line"].as_u64().is_some_and(|n| n > 0),
            "missing finding line",
        )?;
        ensure(
            f["extra"]["is_ignored"] != true,
            "native suppressed finding in suppression-disabled scan",
        )?;
    }
    let mut expected = vec![
        json!("/opt/python/bin/semgrep"),
        json!("scan"),
        json!("--config"),
        selected["rules"].clone(),
        json!("--json"),
        json!("--metrics=off"),
        json!("--disable-version-check"),
        json!("--strict"),
        json!("--disable-nosem"),
        json!("--jobs=1"),
        json!("--no-rewrite-rule-ids"),
        json!("--error"),
        json!("--verbose"),
    ];
    if run["name"] == "delta" {
        expected.extend([
            json!("--baseline-commit"),
            selected["baseline_commit"].clone(),
        ]);
    }
    expected.extend(array(selected, "targets")?.iter().cloned());
    ensure(
        run["argv"] == json!(expected),
        "native scan command differs from explicit selected scope",
    )?;
    if !matches!(run["exit_code"].as_i64(), Some(0 | 1)) || !errors.is_empty() {
        incomplete(
            r,
            "Native scan termination or parser errors leave this scope incomplete.",
        );
    } else {
        ensure(
            run["exit_code"] == json!(i64::from(!findings.is_empty())),
            "native --error exit contradicts scan findings",
        )?;
    }
    if scanned.is_empty() {
        incomplete(r,"The native scan did not identify any source files; zero work cannot clear a criterion.");
    }
    if !skipped.is_empty() {
        incomplete(r,"Native ignored or excluded paths remain outside the completed source scope. Their exact paths and reasons are retained.");
    }
    Ok(v)
}

fn escaped(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn finding(r: &mut Report, f: &Value, pointer: String) -> Result<(), DecodeError> {
    r.observations.push(Observation {
        rule: text(f, "check_id")?.into(),
        message: text(&f["extra"], "message")?.into(),
        path: Some(text(f, "path")?.into()),
        line: f["start"]["line"].as_u64(),
        pointer,
    });
    Ok(())
}

fn rule_tests(
    r: &mut Report,
    v: &Value,
    selected: &Value,
    runs: &[Value],
) -> Result<Value, DecodeError> {
    let row = run(runs, "rule-test")?;
    let native = output(row)?;
    ensure(
        native == v["native_test"],
        "native test fields differ from retained stream",
    )?;
    let expected = json!([
        "/opt/python/bin/semgrep",
        "scan",
        "--config",
        selected["rules"],
        "--json",
        "--metrics=off",
        "--disable-version-check",
        "--strict",
        "--disable-nosem",
        "--jobs=1",
        "--no-rewrite-rule-ids",
        "--test",
        selected["tests"]
    ]);
    ensure(
        row["argv"] == expected,
        "native test invocation differs from selected directories",
    )?;
    let inventory = &v["fixture_inventory"];
    let rules = array(inventory, "rules")?;
    let pairs = array(inventory, "pairs")?;
    ensure(
        !rules.is_empty() && rules.len() <= 512 && !pairs.is_empty(),
        "empty native rule/fixture inventory",
    )?;
    let source = v["source_files"]
        .as_object()
        .ok_or_else(|| bad("missing source bindings"))?;
    let mut selected_rules = BTreeMap::new();
    let mut fixtures = BTreeSet::new();
    let mut configs = BTreeSet::new();
    for rule in rules {
        let id = text(rule, "rule")?;
        let config = text(rule, "configuration")?;
        let fixture = text(rule, "fixture")?;
        let fixed = text(rule, "fixed_fixture")?;
        ensure(
            path(config)
                && path(fixture)
                && path(fixed)
                && source.contains_key(config)
                && selected_rules.insert(id, rule).is_none(),
            "unbound or repeated selected rule",
        )?;
        configs.insert(config);
        fixtures.insert(fixture);
        if !source.contains_key(fixture) || !source.contains_key(fixed) {
            incomplete(
                r,
                "Selected rule or fix fixtures are missing; a zero exit does not qualify them.",
            );
        }
        if lines(&rule["positive_lines"])?.is_empty() || lines(&rule["negative_lines"])?.is_empty()
        {
            incomplete(r,"Every selected rule must exercise both positive and negative annotated assertions.");
        }
    }
    if !array(inventory, "todo_annotations")?.is_empty()
        || !array(inventory, "suppression_annotations")?.is_empty()
    {
        incomplete(r,"TODO or suppression annotations exclude intended rule assertions from this mode's coverage.");
    }
    for key in [
        "config_missing_tests",
        "config_missing_fixtests",
        "config_with_errors",
    ] {
        if !array(&native, key)?.is_empty() {
            incomplete(r,"Native rule testing reported missing fixtures or configuration errors. Inspect the preserved native lists.");
        }
    }
    let results = native["results"]
        .as_object()
        .ok_or_else(|| bad("missing native rule-test map"))?;
    let mut seen = BTreeSet::new();
    let mut native_checks = Vec::new();
    for (config, result) in results {
        ensure(
            configs.contains(config.as_str()),
            "native result names an unselected rule file",
        )?;
        let checks = result["checks"]
            .as_object()
            .ok_or_else(|| bad("missing rule checks"))?;
        for (id, check) in checks {
            let selected_rule = selected_rules
                .get(id.as_str())
                .ok_or_else(|| bad("native result names an unselected rule ID"))?;
            ensure(
                seen.insert(id.as_str()) && selected_rule["configuration"] == config.as_str(),
                "repeated or misattributed native rule",
            )?;
            let passed = check["passed"]
                .as_bool()
                .ok_or_else(|| bad("missing native passed boolean"))?;
            let errors = array(check, "errors")?;
            let matches = check["matches"]
                .as_object()
                .filter(|x| !x.is_empty())
                .ok_or_else(|| bad("missing expected/reported assertion map"))?;
            let mut equal = true;
            for (file, actual) in matches {
                let fixture = file
                    .strip_prefix("/tmp/thruline-semgrep-stage/")
                    .ok_or_else(|| bad("native fixture is outside private stage"))?;
                ensure(
                    path(fixture) && selected_rule["fixture"] == fixture,
                    "native assertion belongs to a different fixture",
                )?;
                let expected = lines(&actual["expected_lines"])?;
                let reported = lines(&actual["reported_lines"])?;
                // TODO annotations change native expected-lines intentionally;
                // that mode is incomplete and must not be normalized as clean.
                if array(inventory, "todo_annotations")?.is_empty() {
                    ensure(
                        expected == lines(&selected_rule["positive_lines"])?,
                        "native expected lines differ from captured annotations",
                    )?;
                }
                equal &= expected == reported;
            }
            ensure(
                passed == (equal && errors.is_empty()),
                "native assertion boolean contradicts expected/reported lines or errors",
            )?;
            if !errors.is_empty() {
                incomplete(r, "Native rule assertion includes parser or engine errors.");
            }
            if !passed {
                r.observations.push(Observation{rule:format!("rule-test:{id}"),message:"Native annotated rule assertion failed; inspect expected and reported lines.".into(),path:selected_rule["fixture"].as_str().map(String::from),line:None,pointer:format!("/native_test/results/{}/checks/{}",escaped(config),escaped(id))});
            }
            native_checks.push(json!({"rule":id,"configuration":config,"fixture":selected_rule["fixture"],"assertion":check}));
        }
    }
    if seen != selected_rules.keys().copied().collect() {
        incomplete(
            r,
            "Not every selected rule produced a native assertion result.",
        );
    }
    let fixes = native["fixtest_results"]
        .as_object()
        .ok_or_else(|| bad("missing native fix-test map"))?;
    let mut seen_fixes = BTreeSet::new();
    for (file, fix) in fixes {
        ensure(
            fixtures.contains(file.as_str()) && seen_fixes.insert(file.as_str()),
            "native fix result names a different fixture",
        )?;
        let passed = fix["passed"]
            .as_bool()
            .ok_or_else(|| bad("missing native fix assertion boolean"))?;
        if !passed {
            r.observations.push(Observation {
                rule: format!("fixtest:{file}"),
                message: "Native autofix does not match the reviewed fixed fixture.".into(),
                path: Some(file.clone()),
                line: None,
                pointer: format!("/native_test/fixtest_results/{}", escaped(file)),
            });
        }
    }
    if seen_fixes != fixtures {
        incomplete(
            r,
            "Not every selected fix produced its expected fixed-fixture assertion.",
        );
    }
    if !matches!(row["exit_code"].as_i64(), Some(0 | 1)) {
        incomplete(
            r,
            "Native rule-test process did not terminate with a qualified status.",
        );
    }
    if r.complete {
        ensure(
            row["exit_code"] == json!(i64::from(!r.observations.is_empty())),
            "native test exit contradicts actual assertions",
        )?;
    }
    r.facts.insert("scanned_paths".into(), json!(fixtures));
    Ok(json!({"inventory":inventory,"native":native,"checks":native_checks}))
}

pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let v = parse_json(bytes)?;
    ensure(
        v["schema"] == "thruline.semgrep-mode-capture.v1" && v["version"] == "1.176.0",
        "unrecognized Semgrep mode capture",
    )?;
    let mut r=Report{format:"semgrep-json-v1".into(),observations:vec![],facts:BTreeMap::new(),complete:true,
        limitations:vec!["Semgrep CE local rules are bounded native observations. Baseline success does not clear older findings; rule-test or fixed-fixture success does not verify application behavior or authorize applying a patch.".into()]};
    r.facts.insert("semgrep_mode".into(),json!({"mode":v["mode"],"selection":v["configuration"],"capture_error":v["capture_error"],"git_identity":v["git_identity"],"whole_project_clean":null}));
    // Exact inline streams are exported using their content hashes; no analyzer
    // path grants the collector access to a host file.
    r.facts.insert("semgrep_native_capture".into(), v.clone());
    let runs = array(&v, "runs")?;
    ensure(runs.len() <= 16, "native phase budget exceeded")?;
    for run in runs {
        for key in ["stdout", "stderr"] {
            let stream = &run[key];
            let value = stream["text"]
                .as_str()
                .ok_or_else(|| bad("missing inline native stream"))?;
            ensure(
                value.len() <= 262145
                    && stream["size"] == value.len()
                    && hex(&stream["sha256"], 64),
                "native stream size/digest shape differs",
            )?;
        }
    }
    if v["capture_complete"] != true
        || v["source_unchanged"] != true
        || !v["capture_error"].is_null()
    {
        incomplete(&mut r,"The isolated Semgrep mode could not capture complete native evidence. Its partial native streams and setup error are retained.");
        return Ok(r);
    }
    let version = run(runs, "version")?;
    ensure(
        version["exit_code"] == 0
            && version["stdout"]["text"]
                .as_str()
                .is_some_and(|s| s.trim() == "1.176.0"),
        "native version phase did not match the pinned engine",
    )?;
    let source = &v["source_files"];
    ensure(
        source.as_object().is_some_and(|x| !x.is_empty()) && hex(&v["configuration_sha256"], 64),
        "missing bound source/configuration identity",
    )?;
    let selected = &v["configuration"];
    let mode = text(&v, "mode")?;
    ensure(
        selected["schema"] == "thruline.semgrep-mode.v1" && selected["mode"] == mode,
        "capture mode differs from configuration",
    )?;
    match mode {
        "targeted" | "baseline" => {
            let current = scan(&mut r, run(runs, "current")?, source, selected)?;
            ensure(
                current == v["current"],
                "current fields differ from retained native stream",
            )?;
            r.facts
                .insert("scanned_paths".into(), current["paths"]["scanned"].clone());
            r.facts.get_mut("semgrep_mode").unwrap()["current"] = current.clone();
            let chosen = if mode == "baseline" {
                let git = &v["git_identity"];
                ensure(
                    hex(&git["accepted_object_closure"], 64)
                        && [
                            "head",
                            "baseline",
                            "merge_base",
                            "head_tree",
                            "baseline_tree",
                        ]
                        .iter()
                        .all(|k| hex(&git[*k], 40))
                        && git["baseline"] == selected["baseline_commit"]
                        && git["merge_base"] == git["baseline"],
                    "native ancestor/tree identity was incomplete or changed",
                )?;
                let delta = scan(&mut r, run(runs, "delta")?, source, selected)?;
                ensure(
                    delta == v["delta"],
                    "delta fields differ from retained native stream",
                )?;
                let current_findings = array(&current, "results")?;
                for finding in array(&delta, "results")? {
                    ensure(
                        current_findings.iter().any(|x| {
                            x["check_id"] == finding["check_id"]
                                && x["path"] == finding["path"]
                                && x["start"] == finding["start"]
                                && x["end"] == finding["end"]
                        }),
                        "native differential finding does not belong to current source scan",
                    )?;
                }
                r.facts.get_mut("semgrep_mode").unwrap()["delta"] = delta.clone();
                delta
            } else {
                current
            };
            for (index, f) in array(&chosen, "results")?.iter().enumerate() {
                finding(
                    &mut r,
                    f,
                    format!(
                        "/{}/results/{index}",
                        if mode == "baseline" {
                            "delta"
                        } else {
                            "current"
                        }
                    ),
                )?;
            }
        }
        "rule-test" => {
            let facts = rule_tests(&mut r, &v, selected, runs)?;
            r.facts.get_mut("semgrep_mode").unwrap()["rule_test"] = facts;
        }
        _ => return Err(bad("unsupported explicit Semgrep mode")),
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn captured(name: &str) -> Value {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/semgrep_modes");
        parse_json(&std::fs::read(dir.join(format!("{name}.json"))).unwrap()).unwrap()
    }
    fn report(name: &str) -> Report {
        decode(&serde_json::to_vec(&captured(name)).unwrap()).unwrap()
    }
    #[test]
    fn exact_native_target_and_ancestor_results_remain_distinct() {
        let ignored = report("scope-ignored");
        assert!(!ignored.complete);
        assert_eq!(ignored.observations.len(), 1);
        let full = report("scope-full");
        assert!(full.complete);
        assert_eq!(full.observations.len(), 2);
        let new = report("baseline-new");
        assert!(new.complete);
        assert_eq!(new.observations.len(), 1);
        assert_eq!(new.observations[0].path.as_deref(), Some("new.py"));
        let fixed = report("baseline-fixed");
        assert!(fixed.complete);
        assert!(fixed.observations.is_empty());
        assert_eq!(
            fixed.facts["semgrep_mode"]["current"]["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(fixed.facts["semgrep_mode"]["whole_project_clean"].is_null());
        assert!(report("targeted-fixed").complete);
    }
    #[test]
    fn exact_native_rule_and_fix_assertions_preserve_failures_and_gaps() {
        for name in ["rule-pass", "upstream-pass"] {
            let r = report(name);
            assert!(r.complete, "{name}");
            assert!(r.observations.is_empty());
        }
        for name in ["rule-bad-fix", "upstream-bad-fix"] {
            let r = report(name);
            assert!(r.complete, "{name}");
            assert_eq!(r.observations.len(), 1);
            assert!(r.observations[0].rule.starts_with("fixtest:"));
        }
        for name in [
            "rule-missing-fix",
            "rule-missing-test",
            "rule-no-negative",
            "rule-invalid",
            "rule-todo",
            "rule-wrong-annotation",
            "baseline-invalid",
            "baseline-stale-objects",
        ] {
            assert!(
                !report(name).complete,
                "{name} cannot clear its coverage from exit0 or empty native results"
            );
        }
    }
    #[test]
    fn native_fields_and_source_spans_cannot_diverge_from_original_stream() {
        let mut value = captured("baseline-new");
        value["delta"]["results"][0]["path"] = json!("unselected.py");
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut value = captured("rule-pass");
        value["native_test"]["fixtest_results"][".thruline/rule-tests/rule.py"]["passed"] =
            json!(false);
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut value = captured("baseline-new");
        value["git_identity"]["merge_base"] = json!("0".repeat(40));
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut value = captured("scope-full");
        value["source_files"]
            .as_object_mut()
            .unwrap()
            .remove("app.py");
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[test]
    fn every_observation_pointer_resolves_inside_its_native_receipt() {
        for name in [
            "scope-full",
            "baseline-new",
            "rule-bad-fix",
            "rule-wrong-annotation",
            "rule-invalid",
        ] {
            let raw = captured(name);
            let r = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
            for observation in r.observations {
                assert!(
                    raw.pointer(&observation.pointer).is_some(),
                    "{name}: {}",
                    observation.pointer
                );
            }
        }
    }
}
