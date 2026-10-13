#[path = "schemathesis_extensions.rs"]
mod schemathesis_extensions;
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn bad(s: &str) -> DecodeError {
    DecodeError::Invalid(s.into())
}
fn array<'a>(v: &'a Value, p: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v.pointer(p)
        .and_then(Value::as_array)
        .ok_or_else(|| bad(&format!("missing array {p}")))
}
fn text<'a>(v: &'a Value, p: &str) -> Result<&'a str, DecodeError> {
    v.pointer(p)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(&format!("missing text {p}")))
}
fn count(v: &Value, p: &str) -> Result<u64, DecodeError> {
    v.pointer(p)
        .and_then(Value::as_u64)
        .ok_or_else(|| bad(&format!("missing nonnegative integer {p}")))
}
fn note(
    r: &mut Report,
    rule: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u64>,
    pointer: String,
) {
    // Secret scanners never pass their match, secret, or freeform message here.
    r.observations.push(Observation {
        rule: rule.into(),
        message: message.into(),
        path: path.map(str::to_owned),
        line,
        pointer,
    });
}
fn limitation(r: &mut Report, s: &str) {
    r.complete = false;
    r.limitations.push(s.into());
}
fn facts(r: &mut Report, v: &Value, pairs: &[(&str, &str)]) -> Result<(), DecodeError> {
    for (key, pointer) in pairs {
        r.facts.insert((*key).into(), json!(count(v, pointer)?));
    }
    Ok(())
}
pub fn decode(format: &str, bytes: &[u8]) -> Result<Report, DecodeError> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(DecodeError::TooLarge);
    }
    if format == "osv-json-v1" && parse_json(bytes)?["schema"] == "thruline.osv-mode-capture.v1" {
        return super::osv_modes::decode(bytes);
    }
    if format == "knip-json-v1" {
        return super::knip::decode(bytes);
    }
    if format == "semgrep-json-v1"
        && parse_json(bytes)?["schema"] == "thruline.semgrep-mode-capture.v1"
    {
        return super::semgrep_modes::decode(bytes);
    }
    if format == "ruff-json-v1" {
        return super::ruff::decode(bytes);
    }
    if format == "infer-json-v1" {
        if parse_json(bytes)?["schema"] == "thruline.infer-mode-capture.v1" {
            return super::infer_modes::decode(bytes);
        }
        return super::infer::decode(bytes);
    }
    if format == "clippy-json-v1" {
        return super::clippy::decode(bytes);
    }
    if ["joern-json-v1", "openrewrite-json-v1"].contains(&format) {
        return super::specialists::decode(format, bytes);
    }
    if format == "sonarqube-json-v1" {
        return super::sonar::decode(bytes);
    }
    if format == "megalinter-json-v1" {
        return super::megalinter::decode(bytes);
    }
    if [
        "aider-json-v1",
        "openhands-sdk-json-v1",
        "pr-agent-json-v1",
        "mini-swe-agent-json-v1",
    ]
    .contains(&format)
    {
        return super::local_agent::decode(format, bytes);
    }
    let mut r = Report {
        format: format.into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: true,
        limitations: vec![],
    };
    if format.ends_with("text-v1") {
        return decode_text(r, bytes);
    }
    if format == "cargo-deny-json-v1" {
        return deny(r, bytes);
    }
    if format == "ast-grep-json-v1" {
        super::strict_json::visit_array(bytes, |i, m| {
            let path = text(&m, "/file")?;
            let rule = text(&m, "/ruleId")?;
            let start = count(&m, "/range/byteOffset/start")?;
            let end = count(&m, "/range/byteOffset/end")?;
            if end < start {
                return Err(bad("reversed source range"));
            }
            if rule == "unused-suppression" {
                r.facts
                    .entry("unused_suppression_ranges".into())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("array initialized above")
                    .push(json!({"file":path,"start":start,"end":end}));
            }
            // Pinned ast-grep CombinedScan::unused_config emits this exact
            // optional Hint diagnostic. Preserve every observation and its raw
            // count; a separately accepted structural condition can distinguish
            // the auxiliary diagnostic from selected-pattern matches.
            if rule == "unused-suppression"
                && m.get("severity").and_then(Value::as_str) == Some("hint")
                && m.get("message").and_then(Value::as_str)
                    == Some("Unused 'ast-grep-ignore' directive.")
            {
                r.facts
                    .entry("auxiliary_diagnostics".into())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .expect("array initialized above")
                    .push(json!({"kind":"unused_suppression_hint","rule":rule,
                        "severity":"hint","pointer":format!("/{i}"),"file":path,
                        "start":start,"end":end,"message":m["message"]}));
            }
            note(
                &mut r,
                rule,
                text(&m, "/message")?,
                Some(path),
                Some(count(&m, "/range/start/line")?.saturating_add(1)),
                format!("/{i}"),
            );
            Ok(())
        })?;
        r.facts
            .insert("finding_count".into(), json!(r.observations.len()));
        return Ok(r);
    }
    let v = parse_json(bytes)?;
    if format == "schemathesis-json-v1"
        && v["schema"] == "thruline.schemathesis-extensions-capture.v1"
    {
        return schemathesis_extensions::decode(&v, format);
    }
    match format {
        "dependency-cruiser-json-v1" => {
            super::dependency_graph::capture(&mut r, &v)?;
            array(&v, "/modules")?;
            let violations = array(&v, "/summary/violations")?;
            for (i, m) in violations.iter().enumerate() {
                let rule = text(m, "/rule/name")?;
                let from = text(m, "/from")?;
                // Pinned producer paths are evidence. Do not replace them with
                // an inferred import or invent a source line absent from JSON.
                let to = m
                    .get("to")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad("missing dependency destination"))?;
                let mut message = if to.is_empty() || from == to {
                    format!("Dependency rule {rule} was triggered at {from}")
                } else {
                    format!("Dependency rule {rule} was triggered: {from} → {to}")
                };
                for (field, label) in [("cycle", "Reported cycle"), ("via", "Reported path")] {
                    if let Some(value) = m.get(field) {
                        let steps = value
                            .as_array()
                            .ok_or_else(|| bad("invalid dependency path"))?;
                        let mut path = vec![from];
                        for step in steps {
                            path.push(text(step, "/name")?);
                        }
                        if !steps.is_empty() {
                            message.push_str(&format!(". {label}: {}", path.join(" → ")));
                        }
                    }
                }
                note(
                    &mut r,
                    rule,
                    &message,
                    Some(from),
                    None,
                    format!("/summary/violations/{i}"),
                );
            }
            if !violations.is_empty() {
                r.limitations.push("Dependency locations are producer-reported module paths. This report does not include import source lines; inspect the retained raw report and the named source files.".into());
            }
            for m in array(&v, "/modules")? {
                for d in array(m, "/dependencies")? {
                    if d.get("couldNotResolve") == Some(&Value::Bool(true)) {
                        limitation(&mut r, "Some dependencies could not be resolved.");
                    }
                }
            }
        }
        "semgrep-json-v1" | "opengrep-json-v1" => {
            let expected = if format == "opengrep-json-v1" {
                "1.30.0"
            } else {
                "1.176.0"
            };
            if text(&v, "/version")? != expected {
                return Err(bad("unrecognized rule engine version"));
            }
            r.facts.insert("engine_version".into(), json!(expected));
            r.facts
                .insert("rule_engine_errors".into(), v["errors"].clone());
            r.facts
                .insert("native_diagnostics".into(), v["results"].clone());
            for (i, m) in array(&v, "/results")?.iter().enumerate() {
                note(
                    &mut r,
                    text(m, "/check_id")?,
                    text(m, "/extra/message")?,
                    Some(text(m, "/path")?),
                    Some(count(m, "/start/line")?),
                    format!("/results/{i}"),
                );
                if m.pointer("/extra/is_ignored") == Some(&json!(true)) {
                    limitation(&mut r, "The report contains suppressed findings.");
                }
            }
            if !array(&v, "/errors")?.is_empty() {
                limitation(&mut r, "The scanner reported errors.");
            }
            let scanned = array(&v, "/paths/scanned")?;
            if scanned.iter().any(|p| p.as_str().is_none()) {
                return Err(bad("scanned source paths must be strings"));
            }
            r.facts.insert("scanned_paths".into(), json!(scanned));
            if scanned.is_empty() {
                limitation(
                    &mut r,
                    "The scanner did not identify any scanned source files.",
                );
            }
            if v.pointer("/paths/skipped")
                .and_then(Value::as_array)
                .is_some_and(|x| !x.is_empty())
            {
                limitation(&mut r, "Some files were skipped.");
            }
            if format == "opengrep-json-v1" {
                super::opengrep::enrich(&mut r, &v)?;
                if !array(&v, "/skipped_rules")?.is_empty() {
                    limitation(
                        &mut r,
                        "Opengrep skipped rules; the selected analysis is incomplete.",
                    );
                }
                r.facts.insert(
                    "interfile_languages_used".into(),
                    v["interfile_languages_used"].clone(),
                );
                r.limitations.push("The accepted Opengrep mode is intrafile taint analysis. Cross-file support is not inferred; native source/sink paths describe a static potential flow, not a reproduced exploit.".into());
            } else {
                r.limitations.push("Semgrep CE local pattern and function-scoped taint analysis. Native spans, fixes and traces are preserved; no cross-function or cross-file coverage is inferred.".into());
            }
        }
        "betterleaks-json-v1" => {
            // The pinned Go reporter serializes its nil []Finding as JSON null.
            // This is a typed empty list, distinct from absent/empty output.
            let empty = Vec::new();
            let matches = if v.is_null() {
                r.limitations.push("Upstream encoded an empty finding list as JSON null. A usable result still requires captured completion and accepted input coverage.".into());
                &empty
            } else {
                array(&v, "")?
            };
            for (i, m) in matches.iter().enumerate() {
                note(
                    &mut r,
                    text(m, "/RuleID")?,
                    "Possible exposed credential. The matched value is withheld.",
                    Some(text(m, "/File")?),
                    Some(count(m, "/StartLine")?),
                    format!("/{i}"),
                );
            }
            r.limitations.push("Credential candidates were not tested against a remote service. Secret values are never included in normalized observations.".into());
        }
        "osv-json-v1" => {
            let mut packages = 0_u64;
            let mut inventory = Vec::new();
            let mut advisory_ranges = Vec::new();
            for (i, result) in array(&v, "/results")?.iter().enumerate() {
                let path = text(result, "/source/path")?;
                for (j, p) in array(result, "/packages")?.iter().enumerate() {
                    let name = text(p, "/package/name")?;
                    let version = text(p, "/package/version")?;
                    let ecosystem = text(p, "/package/ecosystem")?;
                    packages += 1;
                    inventory.push(json!({"path":path,"name":name,"version":version,
                        "ecosystem":ecosystem,"native_alias_groups":p.get("groups"),
                        "pointer":format!("/results/{i}/packages/{j}")}));
                    // OSV 2.5.1 omits this field for a recognized package with no
                    // matches. Captured completion still gates this interpretation;
                    // the same shape with a missing database exits unsuccessfully.
                    let empty = Vec::new();
                    let vulnerabilities = if p.get("vulnerabilities").is_none() {
                        &empty
                    } else {
                        array(p, "/vulnerabilities")?
                    };
                    for (k, vuln) in vulnerabilities.iter().enumerate() {
                        let id = text(vuln, "/id")?;
                        // Keep the publisher's full interval events. Flattening
                        // fixed versions to one minimum would lose reintroduced
                        // ranges and cannot establish a compatible API upgrade.
                        advisory_ranges.push(json!({"id":id,"package":name,"version":version,
                            "path":path,"aliases":vuln.get("aliases"),"affected":vuln.get("affected"),
                            "summary":vuln.get("summary"),"references":vuln.get("references"),
                            "pointer":format!("/results/{i}/packages/{j}/vulnerabilities/{k}")}));
                        note(
                            &mut r,
                            id,
                            &format!(
                                "{name} {version} ({ecosystem}) matches {id}: {}",
                                vuln.get("summary")
                                    .and_then(Value::as_str)
                                    .unwrap_or("an advisory in the selected database")
                            ),
                            Some(path),
                            None,
                            format!("/results/{i}/packages/{j}/vulnerabilities/{k}"),
                        );
                    }
                }
            }
            r.facts.insert("packages".into(), json!(packages));
            r.facts
                .insert("dependency_inventory".into(), json!(inventory));
            r.facts
                .insert("advisory_ranges".into(), json!(advisory_ranges));
            if packages == 0 {
                limitation(&mut r, "No recognized dependency inventory was examined.");
            }
            r.limitations.push("Coverage is limited to recognized dependencies and the exact vulnerability database snapshot.".into());
            r.limitations.push("Native affected/fixed interval events are advisory data, not a verified upgrade. This Cargo recipe does not apply dependency or API migrations; a proposed upgrade requires a build and independent behavior checks.".into());
        }
        "sarif-2.1.0" => {
            if text(&v, "/version")? != "2.1.0" {
                return Err(bad("unsupported SARIF version"));
            }
            let runs = array(&v, "/runs")?;
            if runs.is_empty() {
                return Err(bad("SARIF contains no runs"));
            }
            for (i, run) in runs.iter().enumerate() {
                text(run, "/tool/driver/name")?;
                for (j, m) in array(run, "/results")?.iter().enumerate() {
                    note(
                        &mut r,
                        text(m, "/ruleId")?,
                        text(m, "/message/text")?,
                        m.pointer("/locations/0/physicalLocation/artifactLocation/uri")
                            .and_then(Value::as_str),
                        m.pointer("/locations/0/physicalLocation/region/startLine")
                            .and_then(Value::as_u64),
                        format!("/runs/{i}/results/{j}"),
                    );
                    if m.get("suppressions")
                        .and_then(Value::as_array)
                        .is_some_and(|a| !a.is_empty())
                    {
                        limitation(&mut r, "SARIF contains suppressed results.");
                    }
                }
                if !run
                    .get("invocations")
                    .and_then(Value::as_array)
                    .is_some_and(|a| {
                        !a.is_empty()
                            && a.iter()
                                .all(|x| x.get("executionSuccessful") == Some(&json!(true)))
                    })
                {
                    limitation(
                        &mut r,
                        "SARIF does not establish successful invocation coverage.",
                    );
                }
            }
        }
        "schemathesis-json-v1" => {
            if text(&v, "/schemathesis_version")? != "4.26.1" {
                return Err(bad("unsupported Schemathesis runtime/report version"));
            }
            let total = count(&v, "/operations/total")?;
            let selected = count(&v, "/operations/selected")?;
            let tested = count(&v, "/operations/tested")?;
            let generated = count(&v, "/test_cases/generated")?;
            let failed = count(&v, "/test_cases/with_failures")?;
            let unchecked = count(&v, "/test_cases/without_checks")?;
            let skipped = count(&v, "/operations/skipped")?;
            if failed > generated || unchecked > generated || selected > total || tested > selected
            {
                return Err(bad("inconsistent Schemathesis counts"));
            }
            r.facts.insert("passed".into(), json!(generated - failed));
            r.facts.insert("failed".into(), json!(failed));
            r.facts.insert("skipped".into(), json!(skipped));
            r.facts
                .insert("scan_coverage".into(), v["operations"].clone());
            r.facts.insert("seed".into(), v["seed"].clone());
            r.facts
                .insert("native_failures".into(), v["failures"].clone());
            if v.get("complete") != Some(&json!(true))
                || text(&v, "/stop_reason")? != "completed"
                || total == 0
                || selected != total
                || tested != selected
                || unchecked > 0
                || skipped > 0
                || count(&v, "/operations/errored")? > 0
                || !array(&v, "/errors")?.is_empty()
            {
                limitation(&mut r,"The API run was interrupted, skipped operations, lacked assertions or encountered setup errors. It cannot settle the full selected contract.");
            }
            for (i, failure) in array(&v, "/failures")?.iter().enumerate() {
                note(
                    &mut r,
                    text(failure, "/type")?,
                    text(failure, "/title")?,
                    None,
                    None,
                    format!("/failures/{i}"),
                );
            }
            if failed > 0 && r.observations.is_empty() {
                return Err(bad("API failure counts have no failure records"));
            }
            r.limitations.push("Generated requests cover only the selected schema, phases and seed. Schema conformance does not prove the business requirement. Sanitized HTTP reproducers, when emitted, remain in the raw report artifact.".into());
        }
        "playwright-json-v1" => {
            array(&v, "/suites")?;
            facts(
                &mut r,
                &v,
                &[
                    ("passed", "/stats/expected"),
                    ("failed", "/stats/unexpected"),
                    ("skipped", "/stats/skipped"),
                    ("flaky", "/stats/flaky"),
                ],
            )?;
            if !array(&v, "/errors")?.is_empty() {
                limitation(&mut r, "The browser test runner reported errors.");
            }
            if count(&v, "/stats/flaky")? > 0 {
                limitation(&mut r, "Some browser tests passed only after retries.");
            }
            browser_observations(&mut r, &v, "")?;
            r.limitations.push(
                "Browser results cover only the asserted workflows and configured browsers.".into(),
            );
        }
        "cargo-mutants-json-v1" => {
            text(&v, "/cargo_mutants_version")?;
            array(&v, "/outcomes")?;
            if v.get("end_time").is_none_or(Value::is_null) {
                limitation(&mut r, "The mutation run has not finished.");
            }
            facts(
                &mut r,
                &v,
                &[
                    ("total_mutants", "/total_mutants"),
                    ("missed", "/missed"),
                    ("caught", "/caught"),
                    ("timeout", "/timeout"),
                    ("unviable", "/unviable"),
                ],
            )?;
            let total = count(&v, "/total_mutants")?;
            let accounted = ["missed", "caught", "timeout", "unviable"]
                .iter()
                .try_fold(0u64, |sum, key| {
                    sum.checked_add(count(&v, &format!("/{key}"))?)
                        .ok_or_else(|| bad("mutation count overflow"))
                })?;
            if total == 0 || accounted != total || count(&v, "/unviable")? > 0 {
                limitation(
                    &mut r,
                    "Mutation coverage is empty, inconsistent, or includes unviable mutations.",
                );
            }
            if count(&v, "/timeout")? > 0 {
                limitation(
                    &mut r,
                    "Timed-out mutants need review; timeout is not a caught mutant.",
                );
            }
            let native = array(&v, "/outcomes")?;
            let baselines: Vec<_> = native
                .iter()
                .filter(|o| o["scenario"] == "Baseline")
                .collect();
            let phase_success = |outcome: &Value, phase: &str| {
                outcome["phase_results"].as_array().is_some_and(|phases| {
                    phases
                        .iter()
                        .any(|p| p["phase"] == phase && p["process_status"] == "Success")
                })
            };
            if baselines.len() != 1
                || !baselines.iter().all(|o| {
                    o["summary"] == "Success"
                        && phase_success(o, "Build")
                        && phase_success(o, "Test")
                })
            {
                limitation(&mut r, "Mutation testing requires exactly one passing original build and test baseline; a skipped or failing baseline cannot qualify mutants.");
            }
            let mutants: Vec<_> = native
                .iter()
                .filter(|o| o["scenario"].get("Mutant").is_some())
                .collect();
            let mut names = std::collections::BTreeSet::new();
            let mut observed = std::collections::BTreeMap::<&str, u64>::new();
            for outcome in &mutants {
                let name = text(outcome, "/scenario/Mutant/name")?;
                if !names.insert(name) {
                    limitation(
                        &mut r,
                        "Duplicate native mutation identities make scope accounting incomplete.",
                    );
                }
                let summary = text(outcome, "/summary")?;
                *observed.entry(summary).or_default() += 1;
                if matches!(summary, "CaughtMutant" | "MissedMutant") {
                    let tested = outcome["phase_results"].as_array().is_some_and(|phases| {
                        phases.iter().any(|p| {
                            p["phase"] == "Test"
                                && if summary == "MissedMutant" {
                                    p["process_status"] == "Success"
                                } else {
                                    p["process_status"].get("Failure").is_some()
                                }
                        })
                    });
                    if !phase_success(outcome, "Build") || !tested {
                        limitation(&mut r, "Caught or missed native outcome lacks the required successful build and matching completed test result.");
                    }
                }
            }
            for (name, key) in [
                ("CaughtMutant", "caught"),
                ("MissedMutant", "missed"),
                ("Timeout", "timeout"),
                ("Unviable", "unviable"),
            ] {
                if observed.get(name).copied().unwrap_or(0) != count(&v, &format!("/{key}"))? {
                    limitation(
                        &mut r,
                        "Native mutation summary counts differ from the retained outcome records.",
                    );
                }
            }
            if mutants.len() as u64 != total {
                limitation(
                    &mut r,
                    "Native mutation outcome count differs from the selected total.",
                );
            }
            r.facts.insert("mutation_outcomes".into(), json!(native));
            r.facts.insert("mutation_baseline".into(), json!(baselines));
            r.limitations.push("Surviving mutations are test-quality candidates; equivalent mutants need human adjudication.".into());
        }
        "criterion-estimates-v1" => {
            for key in ["mean", "median", "std_dev"] {
                let n = v
                    .pointer(&format!("/{key}/point_estimate"))
                    .and_then(Value::as_f64)
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .ok_or_else(|| bad("missing nonnegative benchmark estimate"))?;
                r.facts.insert(format!("{key}_ns"), json!(n));
            }
            r.limitations.push("Timing estimates alone do not establish a regression. Bind workload, baseline, machine and noise budget before comparing.".into());
        }
        "dhat-json-v2" => {
            if count(&v, "/dhatFileVersion")? != 2 {
                return Err(bad("unsupported DHAT file version"));
            }
            let mut total = 0u64;
            for p in array(&v, "/pps")? {
                total = total
                    .checked_add(count(p, "/tb")?)
                    .ok_or_else(|| bad("allocation total overflow"))?;
            }
            r.facts.insert("total_allocated_bytes".into(), json!(total));
            r.limitations.push("Heap allocations are not whole-process RSS. Allocation totals are not retained or peak memory.".into());
        }
        "swe-bench-json-v1" => {
            if count(&v, "/schema_version")? != 2 {
                return Err(bad("unsupported SWE-bench summary version"));
            }
            let total = count(&v, "/total_instances")?;
            if total == 0
                || count(&v, "/completed_instances")? != total
                || !array(&v, "/incomplete_ids")?.is_empty()
                || count(&v, "/infra_failure_instances")? > 0
                || count(&v, "/ambiguous_failure_instances")? > 0
            {
                limitation(
                    &mut r,
                    "The benchmark has missing, ambiguous, or infrastructure-dependent outcomes.",
                );
            }
            facts(
                &mut r,
                &v,
                &[
                    ("total", "/total_instances"),
                    ("resolved", "/resolved_instances"),
                    ("unresolved", "/unresolved_instances"),
                    ("errors", "/error_instances"),
                ],
            )?;
            if count(&v, "/error_instances")? > 0 {
                limitation(&mut r, "Some benchmark instances could not be evaluated.");
            }
            r.limitations.push("Benchmark task outcomes are not a qualification of ThruLine's defect detection precision.".into());
        }
        "inspect-json-v2" => {
            if count(&v, "/version")? != 2 {
                return Err(bad("unsupported Inspect log version"));
            }
            if text(&v, "/status")? != "success" {
                limitation(&mut r, "The evaluation did not finish successfully.");
            }
            array(&v, "/results/scores")?;
            facts(
                &mut r,
                &v,
                &[
                    ("completed_samples", "/results/completed_samples"),
                    ("total_samples", "/results/total_samples"),
                ],
            )?;
            if count(&v, "/results/total_samples")? == 0
                || count(&v, "/results/completed_samples")? != count(&v, "/results/total_samples")?
            {
                limitation(
                    &mut r,
                    "The evaluation did not complete every planned sample.",
                );
            }
            // Model grader scores remain advisory data, never acceptance authority.
            r.limitations.push("Scorer outputs are advisory. Independent labels and class-specific qualification are required.".into());
        }
        "promptfoo-json-v3" => {
            if count(&v, "/results/version")? != 3 {
                return Err(bad("unsupported Promptfoo evaluation version"));
            }
            facts(
                &mut r,
                &v,
                &[
                    ("passed", "/results/stats/successes"),
                    ("failed", "/results/stats/failures"),
                    ("errors", "/results/stats/errors"),
                ],
            )?;
            if count(&v, "/results/stats/errors")? > 0 {
                limitation(&mut r, "Some evaluations returned errors.");
            }
            r.limitations.push("Model-scored evaluations remain advisory and do not establish independent correctness.".into());
        }
        _ => return Err(DecodeError::Format(format.into())),
    }
    // Only scanner formats measure a finding count. Metric-only reports cannot
    // accidentally satisfy `no_observations` merely because this vector is empty.
    if [
        "ast-grep-json-v1",
        "dependency-cruiser-json-v1",
        "knip-json-v1",
        "semgrep-json-v1",
        "opengrep-json-v1",
        "betterleaks-json-v1",
        "osv-json-v1",
        "sarif-2.1.0",
    ]
    .contains(&format)
    {
        r.facts
            .insert("finding_count".into(), json!(r.observations.len()));
    }
    Ok(r)
}
fn deny(mut r: Report, bytes: &[u8]) -> Result<Report, DecodeError> {
    let s = std::str::from_utf8(bytes).map_err(|_| bad("non-UTF8 report"))?;
    let mut records = 0;
    let mut summary_count = None;
    for (i, line) in s.lines().enumerate().filter(|(_, s)| !s.trim().is_empty()) {
        let v = parse_json(line.as_bytes())?;
        records += 1;
        match text(&v, "/type")? {
            "summary" => {
                if summary_count.is_some() {
                    return Err(bad("multiple cargo-deny summaries"));
                }
                let fields = v
                    .get("fields")
                    .and_then(Value::as_object)
                    .filter(|f| !f.is_empty())
                    .ok_or_else(|| bad("empty cargo-deny check summary"))?;
                let mut n = 0u64;
                for (check, stats) in fields {
                    if !["advisories", "bans", "licenses", "sources"].contains(&check.as_str()) {
                        return Err(bad("unknown cargo-deny check kind"));
                    }
                    n = n
                        .checked_add(count(stats, "/errors")?)
                        .and_then(|n| n.checked_add(count(stats, "/warnings").ok()?))
                        .ok_or_else(|| bad("invalid diagnostic count"))?;
                    r.facts
                        .insert(format!("{check}_errors"), json!(count(stats, "/errors")?));
                }
                summary_count = Some(n);
            }
            "diagnostic" => {
                let level = text(&v, "/fields/severity")?;
                if !["error", "warning", "note", "help"].contains(&level) {
                    return Err(bad("unrecognized diagnostic severity"));
                }
                if level == "error" || level == "warning" {
                    note(
                        &mut r,
                        text(&v, "/fields/code")?,
                        text(&v, "/fields/message")?,
                        v.pointer("/fields/labels/0/span/file")
                            .and_then(Value::as_str),
                        None,
                        format!("line:{}", i + 1),
                    );
                }
            }
            "log" => {
                if matches!(
                    v.pointer("/fields/level").and_then(Value::as_str),
                    Some("ERROR" | "WARN")
                ) {
                    limitation(
                        &mut r,
                        "cargo-deny emitted a warning or error outside its diagnostic report.",
                    );
                }
            }
            _ => return Err(bad("unrecognized cargo-deny record type")),
        }
    }
    if records == 0 {
        return Err(bad(
            "empty cargo-deny stream cannot establish which checks ran",
        ));
    }
    if summary_count != Some(r.observations.len() as u64) {
        limitation(
            &mut r,
            "The check summary is missing or does not agree with the captured diagnostics.",
        );
    }
    r.facts
        .insert("finding_count".into(), json!(r.observations.len()));
    r.limitations
        .push("Only configured checks and the supplied advisory database are covered.".into());
    Ok(r)
}
fn decode_text(mut r: Report, bytes: &[u8]) -> Result<Report, DecodeError> {
    let s = std::str::from_utf8(bytes).map_err(|_| bad("non-UTF8 output"))?;
    match r.format.as_str() {
        "cargo-test-text-v1" => {
            let mut found = false;
            let (mut passed, mut failed, mut skipped) = (0u64, 0u64, 0u64);
            for line in s.lines().filter(|l| l.starts_with("test result: ")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let number = |name: &str| -> Result<u64, DecodeError> {
                    let pos = parts
                        .iter()
                        .position(|s| s.trim_end_matches(';') == name)
                        .ok_or_else(|| bad("missing test count"))?;
                    pos.checked_sub(1)
                        .and_then(|p| parts[p].parse::<u64>().ok())
                        .ok_or_else(|| bad("invalid test count"))
                };
                passed = passed
                    .checked_add(number("passed")?)
                    .ok_or_else(|| bad("count overflow"))?;
                failed = failed
                    .checked_add(number("failed")?)
                    .ok_or_else(|| bad("count overflow"))?;
                skipped = skipped
                    .checked_add(number("ignored")?)
                    .and_then(|n| n.checked_add(number("filtered").ok()?))
                    .ok_or_else(|| bad("invalid skip count"))?;
                found = true;
            }
            if !found {
                return Err(bad("no Rust test summaries captured"));
            }
            r.facts.extend([
                ("passed".into(), json!(passed)),
                ("failed".into(), json!(failed)),
                ("skipped".into(), json!(skipped)),
            ]);
            r.limitations.push("Test summaries describe the invoked harness only. Preserve Proptest seeds and Loom model bounds with the run inputs.".into());
        }
        "libfuzzer-text-v1" => {
            let crashes = s
                .lines()
                .filter(|l| {
                    l.contains("ERROR: libFuzzer:") || l.contains("ERROR: AddressSanitizer:")
                })
                .count();
            r.facts.insert("crash_markers".into(), json!(crashes));
            limitation(&mut r,"Fuzzer text is a diagnostic, not an exhaustive proof. Bind the corpus, seed, time budget, and replayable crash artifact.");
            if s.trim().is_empty() {
                return Err(bad("empty fuzzer output"));
            }
        }
        "kani-text-v1" => {
            if s.trim().is_empty() {
                return Err(bad("empty Kani output"));
            }
            r.facts.insert(
                "verification_failed_markers".into(),
                json!(s.matches("VERIFICATION:- FAILED").count()),
            );
            limitation(&mut r,"Kani text requires harness-specific assumptions, unwind bounds and proof artifacts; no proof is inferred from a success phrase.");
        }
        _ => return Err(DecodeError::Format(r.format)),
    }
    Ok(r)
}

fn browser_observations(r: &mut Report, suite: &Value, pointer: &str) -> Result<(), DecodeError> {
    if let Some(specs) = suite.get("specs").and_then(Value::as_array) {
        for (i, spec) in specs.iter().enumerate() {
            for (j, test) in array(spec, "/tests")?.iter().enumerate() {
                let captured = json!({"title":spec["title"],"file":spec["file"],"line":spec["line"],"project":test["projectName"],"expected_status":test["expectedStatus"],"status":test["status"],"results":test["results"],"pointer":format!("{pointer}/specs/{i}/tests/{j}")});
                r.facts
                    .entry("authored_workflows".into())
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .ok_or_else(|| bad("invalid workflow collection"))?
                    .push(captured);
                if test.get("status").and_then(Value::as_str) == Some("unexpected") {
                    for (k, result) in array(test, "/results")?.iter().enumerate() {
                        if result
                            .get("status")
                            .and_then(Value::as_str)
                            .is_some_and(|s| s == "failed" || s == "timedOut")
                        {
                            let message = result
                                .pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("The authored workflow did not satisfy its assertion.");
                            note(
                                r,
                                "authored-workflow",
                                &format!("{}: {}", text(spec, "/title")?, message),
                                spec.get("file").and_then(Value::as_str),
                                spec.get("line").and_then(Value::as_u64),
                                format!("{pointer}/specs/{i}/tests/{j}/results/{k}"),
                            );
                        }
                    }
                }
            }
        }
    }
    if let Some(children) = suite.get("suites").and_then(Value::as_array) {
        for (i, child) in children.iter().enumerate() {
            browser_observations(r, child, &format!("{pointer}/suites/{i}"))?;
        }
    }
    Ok(())
}
