//! Native agent capture is advisory input. Never infer a product result from
//! natural-language completion, submission, tool count or self-written tests.
use super::{parse_json, DecodeError, Report};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn invalid(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn mini_harness_complete(value: &Value, rid: &str) -> Result<bool, DecodeError> {
    let Some(capture) = value.get("mini_harness") else {
        return Ok(true);
    };
    if rid != "R15"
        || capture["options"]["schema"] != "thruline.mini-harness-options.v1"
        || capture["options"]["mode"] != "owned-swebench-task"
        || capture["options"]["environment"] != "local-in-linux"
    {
        return Err(invalid(
            "mini-SWE harness belongs to the explicit owned Linux task mode",
        ));
    }
    let native = &capture["native"];
    if native.is_null() {
        return Ok(false);
    }
    if native["schema"] != "thruline.mini-owned-harness.v1"
        || native["native_runner"] != "minisweagent.run.benchmarks.swebench.process_instance"
        || native["native_agent"]
            != "minisweagent.run.benchmarks.utils.common.ProgressTrackingAgent"
        || native["environment_type"] != "minisweagent.environments.local.LocalEnvironment"
        || native["dataset_kind"] != "owned_compatibility_format"
        || native["official_benchmark"] != false
        || !native["official_score"].is_null()
        || native["workers"] != 1
    {
        return Err(invalid(
            "unrecognized native mini-SWE harness or unsupported benchmark claim",
        ));
    }
    let identity = capture["source_sha256"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let bound = identity.is_some()
        && value["native_artifacts"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["path"] == capture["source_artifact"]
                    && item["sha256"] == capture["source_sha256"]
            })
        });
    Ok(bound
        && native["instance_ids"]
            .as_array()
            .is_some_and(|ids| ids.len() == 1 && ids[0] == capture["options"]["instance_id"])
        && native["native_exit_status"] == value["agent_termination"]
        && capture["capture_error"].is_null())
}

pub fn decode(format: &str, bytes: &[u8]) -> Result<Report, DecodeError> {
    let value = parse_json(bytes)?;
    let (rid, commit) = match format {
        "aider-json-v1" => ("R12", "a4be6ccd87ebaa59b361f3f028d116ce1761b626"),
        "openhands-sdk-json-v1" => ("R13", "50080b58d35b4824fda25fca2345d80bcd08aeff"),
        "pr-agent-json-v1" => ("R14", "f3b385ea2927247ddcff2fe252472380b9c8f5fc"),
        "mini-swe-agent-json-v1" => ("R15", "a83fcae82d2a08f0ee0c688f9d137b3566c097f8"),
        _ => return Err(invalid("unknown local agent adapter")),
    };
    if value["schema"] != "thruline.local-agent-capture.v1"
        || value["repository_id"] != rid
        || value["tool_source_commit"] != commit
    {
        return Err(invalid(
            "local agent capture does not match the pinned source",
        ));
    }
    for pointer in [
        "/source/selected_files",
        "/source/before",
        "/source/after",
        "/native_artifacts",
        "/native_bytes_omitted",
        "/limitations",
    ] {
        if value.pointer(pointer).and_then(Value::as_array).is_none() {
            return Err(invalid("incomplete native agent capture envelope"));
        }
    }
    for pointer in ["/task_sha256", "/proposal/sha256"] {
        if !value
            .pointer(pointer)
            .and_then(Value::as_str)
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid("missing agent input or proposal identity"));
        }
    }
    let oracle_state = value["acceptance_state"].as_str().unwrap_or("");
    if !["not_evaluated", "oracle_passed", "oracle_failed"].contains(&oracle_state) {
        return Err(invalid(
            "agent submission cannot declare independently accepted product behavior",
        ));
    }
    if oracle_state != "not_evaluated" {
        let oracle = &value["oracle"];
        let after = oracle["after"]["exit_code"]
            .as_i64()
            .ok_or_else(|| invalid("oracle outcome is missing its actual exit code"))?;
        if (oracle_state == "oracle_passed") != (after == 0)
            || oracle["config_sha256"]
                .as_str()
                .is_none_or(|s| s.len() != 64)
            || oracle["before"]["bound_files"]
                .as_array()
                .is_none_or(Vec::is_empty)
            || oracle["before"]["bound_files"] != oracle["after"]["bound_files"]
        {
            return Err(invalid(
                "oracle declaration lacks a consistent frozen before/after identity",
            ));
        }
    }
    let plain_diff = value
        .get("pr_agent_options")
        .filter(|options| !options.is_null());
    if let Some(options) = plain_diff {
        if rid != "R14"
            || options["schema"] != "thruline.pr-agent-plain-diff.v1"
            || options["mode"] != "plain-diff"
            || value["source"]["capture_basis"] != "unified_diff_file"
            || value["source"]["selected_files"] != json!([options["diff_file"]])
            || value["source"]["editable_files"] != json!([])
            || oracle_state != "not_evaluated"
        {
            return Err(invalid(
                "plain-diff review cannot claim editable source or behavior acceptance",
            ));
        }
    }
    let plain_diff_complete = plain_diff.is_none() || {
        let native = &value["pr_agent_plain_diff"];
        native["schema"] == "thruline.pr-agent-native-diff.v1"
            && native["mode"] == "plain-diff"
            && native["working_tree_enrichment"] == false
            && native["commit_history_available"] == false
            && native["hosted_publishing"] == false
            && native["input"]["path"] == value["source"]["before"][0]["path"]
            && native["input"]["sha256"] == value["source"]["before"][0]["sha256"]
            && native["input"]["sha256"]
                .as_str()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && native["native_files"].as_array().is_some_and(|files| {
                !files.is_empty()
                    && files.iter().all(|file| {
                        file["head_file"] == ""
                            && file["base_file"] == ""
                            && file["patch"]
                                .as_str()
                                .is_some_and(|patch| !patch.trim().is_empty())
                    })
            })
            && value["source"]["before"] == value["source"]["after"]
    };
    let mini_complete = mini_harness_complete(&value, rid)?;
    let complete = mini_complete
        && plain_diff_complete
        && value["complete"] == true
        && value["source"]["original_unchanged"] == true
        && value["process"]["exit_code"] == 0
        && value["process"]["state"] == "exited"
        && value["native_bytes_omitted"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && value["source"]["selected_files"]
            .as_array()
            .is_some_and(|files| !files.is_empty())
        && value["native_artifacts"]
            .as_array()
            .is_some_and(|files| !files.is_empty());
    let mut report = Report {
        format: format.into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete,
        limitations: vec![],
    };
    if !complete {
        report.limitations.push("The local agent run was incomplete, failed, interrupted, or lacked complete native artifacts. An agent submission or limit state is not acceptance evidence.".into());
    }
    for key in [
        "source",
        "model",
        "process",
        "agent_termination",
        "proposal",
        "native_artifacts",
        "acceptance_state",
        "independent_oracle",
        "oracle",
        "model_identity_verification",
        "native_output",
        "pr_agent_options",
        "pr_agent_plain_diff",
        "oracle_applicability",
        "mini_harness",
    ] {
        if let Some(v) = value.get(key) {
            report.facts.insert(key.into(), v.clone());
        }
    }
    report.facts.insert("advisory_only".into(), json!(true));
    report.limitations.extend(
        value["limitations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from),
    );
    report.limitations.push("Agent actions, suggestions and self-reported test outcomes remain advisory. Only the frozen independent oracle and a verified patch base can support applying a proposal; this report does not establish that verification.".into());
    Ok(report)
}

#[cfg(test)]
mod mini_harness_tests {
    use super::*;
    fn captured(name: &str) -> Value {
        let bytes = match name {
            "submitted" => include_bytes!("fixtures/mini-harness/submitted.json").as_slice(),
            _ => include_bytes!("fixtures/mini-harness/limited.json").as_slice(),
        };
        serde_json::from_slice(bytes).unwrap()
    }
    #[test]
    fn native_submission_and_limit_remain_separate_without_scores() {
        for name in ["submitted", "limited"] {
            let value = captured(name);
            assert!(mini_harness_complete(&value, "R15").unwrap());
            assert!(value["mini_harness"]["native"]["official_score"].is_null());
        }
        assert_eq!(captured("submitted")["agent_termination"], "Submitted");
        assert_eq!(captured("limited")["agent_termination"], "LimitsExceeded");
    }
    #[test]
    fn unbound_or_missing_native_bytes_cannot_complete() {
        let mut value = captured("submitted");
        value["native_artifacts"] = json!([]);
        assert!(!mini_harness_complete(&value, "R15").unwrap());
        value["mini_harness"]["native"] = Value::Null;
        assert!(!mini_harness_complete(&value, "R15").unwrap());
    }
    #[test]
    fn official_score_wrong_provider_and_foreign_environment_are_refused() {
        let value = captured("submitted");
        assert!(mini_harness_complete(&value, "R14").is_err());
        for (key, bad) in [
            ("official_score", json!(100)),
            ("official_benchmark", json!(true)),
            (
                "environment_type",
                json!("minisweagent.environments.docker.DockerEnvironment"),
            ),
        ] {
            let mut changed = value.clone();
            changed["mini_harness"]["native"][key] = bad;
            assert!(mini_harness_complete(&changed, "R15").is_err());
        }
    }
    #[test]
    fn green_oracle_does_not_turn_limited_native_capture_into_complete_review() {
        for (name, code, state, complete) in [
            ("submitted", 0, "exited", true),
            ("limited", 3, "failed", false),
        ] {
            let mut value = captured(name);
            value["schema"] = json!("thruline.local-agent-capture.v1");
            value["repository_id"] = json!("R15");
            value["tool_source_commit"] = json!("a83fcae82d2a08f0ee0c688f9d137b3566c097f8");
            value["source"] = json!({"selected_files":["ledger.py"],"before":[],"after":[],"original_unchanged":true});
            value["task_sha256"] = json!("1".repeat(64));
            value["proposal"] = json!({"sha256":"2".repeat(64)});
            value["native_bytes_omitted"] = json!([]);
            value["limitations"] = json!([]);
            value["acceptance_state"] = json!("oracle_passed");
            value["oracle"] = json!({"config_sha256":"3".repeat(64),"before":{"bound_files":[{"sha256":"4".repeat(64)}]},"after":{"exit_code":0,"bound_files":[{"sha256":"4".repeat(64)}]}});
            value["complete"] = json!(true);
            value["process"] = json!({"exit_code":code,"state":state});
            let decoded = decode(
                "mini-swe-agent-json-v1",
                &serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
            assert_eq!(decoded.complete, complete);
            assert!(decoded.observations.is_empty());
            assert_eq!(decoded.facts["advisory_only"], true);
            assert_eq!(
                decoded.facts["agent_termination"],
                value["agent_termination"]
            );
        }
    }
}
