use super::*;

fn draft() -> serde_json::Value {
    serde_json::json!({
        "version": 1, "id": "project-checks", "revision": 1,
        "objective": "Verify the declared checks for this change",
        "criteria": [{"id": "tests", "description": "The declared Python suite passes", "check_ids": ["pytest"], "human_acceptance_required": false}],
        "checks": [{"id": "pytest", "category": "test", "argv": ["python3", "-m", "pytest", "-q"], "cwd": ".", "runner": "pytest", "mode": "captured", "timeout_seconds": 120, "artifacts": []}],
        "protected_paths": ["tests/test_feature.py"]
    })
}

fn document(value: &serde_json::Value) -> Document {
    Document::parse(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn passed(d: &Document) -> Evidence {
    Evidence {
        check_id: "pytest".into(),
        check_digest: d.contract().checks[0].digest().unwrap(),
        contract_digest: d.digest().into(),
        subject: "tree-a".into(),
        run_id: "run-1".into(),
        outcome: Outcome::Passed,
        admissible: true,
    }
}

#[test]
fn q01_silence_does_not_remove_required_work() {
    let d = document(&draft());
    let result = d.evaluate("tree-a", &[]).unwrap();
    assert_eq!(result.state, State::Unverified);
    assert_eq!(result.checks.len(), 1);
    assert_eq!(result.criteria.len(), 1);
    assert_eq!(result.checks[0].check_id, "pytest");
    assert!(result.checks[0].reason.contains("no evidence"));
}

#[test]
fn q02_invalid_or_empty_contract_cannot_prove_completion() {
    let mut mutations = Vec::new();
    let mut empty = draft();
    empty["criteria"] = serde_json::json!([]);
    mutations.push(empty);
    let mut future = draft();
    future["version"] = 2.into();
    mutations.push(future);
    let mut zero = draft();
    zero["revision"] = 0.into();
    mutations.push(zero);
    let mut no_work = draft();
    no_work["criteria"][0]["check_ids"] = serde_json::json!([]);
    mutations.push(no_work);
    let mut dangling = draft();
    dangling["criteria"][0]["check_ids"] = serde_json::json!(["imaginary"]);
    mutations.push(dangling);
    let mut extra = draft();
    extra["verified"] = true.into();
    mutations.push(extra);
    let mut nested = draft();
    nested["checks"][0]["skip_verification"] = true.into();
    mutations.push(nested);
    let mut duplicate = draft();
    duplicate["checks"]
        .as_array_mut()
        .unwrap()
        .push(draft()["checks"][0].clone());
    mutations.push(duplicate);
    let mut escape = draft();
    escape["checks"][0]["cwd"] = "../other-repo".into();
    mutations.push(escape);
    for value in mutations {
        assert!(
            Document::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
            "accepted {value}"
        );
    }
    assert!(Document::parse(b"{broken").is_err());
    assert!(Document::parse(&vec![b' '; MAX_DOCUMENT_BYTES + 1]).is_err());
}

#[test]
fn q04_changed_contract_cannot_reuse_previous_acceptance() {
    let before = document(&draft());
    let evidence = passed(&before);
    let mut changed = draft();
    changed["revision"] = 2.into();
    changed["objective"] = "A different accepted outcome".into();
    let after = document(&changed);
    assert_ne!(before.digest(), after.digest());
    assert_eq!(
        before
            .evaluate("tree-a", std::slice::from_ref(&evidence))
            .unwrap()
            .state,
        State::Verified
    );
    assert_eq!(
        after.evaluate("tree-a", &[evidence]).unwrap().state,
        State::Unverified
    );
}

#[test]
fn q05_honest_admissible_pass_is_accepted() {
    let d = document(&draft());
    let result = d.evaluate("tree-a", &[passed(&d)]).unwrap();
    assert_eq!(result.state, State::Verified);
    assert_eq!(result.checks[0].run_id.as_deref(), Some("run-1"));
    assert_eq!(result.criteria[0].state, State::Verified);
}

#[test]
fn q06_wrong_subject_or_check_identity_cannot_stay_green() {
    let d = document(&draft());
    assert_eq!(
        d.evaluate("tree-b", &[passed(&d)]).unwrap().state,
        State::Unverified
    );
    assert_eq!(
        d.evaluate("", &[passed(&d)]).unwrap().state,
        State::Unverified
    );
    let mut wrong_command = passed(&d);
    wrong_command.check_digest = "different-command".into();
    assert_eq!(
        d.evaluate("tree-a", &[wrong_command]).unwrap().state,
        State::Unverified
    );
}

#[test]
fn q08_incomplete_execution_is_unknown_and_failure_is_distinct() {
    let d = document(&draft());
    for outcome in [
        Outcome::NotRun,
        Outcome::Unparsed,
        Outcome::TimedOut,
        Outcome::Interrupted,
    ] {
        let mut e = passed(&d);
        e.outcome = outcome;
        assert_eq!(d.evaluate("tree-a", &[e]).unwrap().state, State::Unverified);
    }
    let mut failed = passed(&d);
    failed.outcome = Outcome::Failed;
    assert_eq!(
        d.evaluate("tree-a", &[failed]).unwrap().state,
        State::Failed
    );
    let mut incomplete = passed(&d);
    incomplete.admissible = false;
    assert_eq!(
        d.evaluate("tree-a", &[incomplete]).unwrap().state,
        State::Unverified
    );
}

#[test]
fn q09_duplicate_selected_attempts_do_not_allow_cherry_picking() {
    let d = document(&draft());
    let good = passed(&d);
    let mut bad = passed(&d);
    bad.run_id = "run-2".into();
    bad.outcome = Outcome::Failed;
    assert_eq!(
        d.evaluate("tree-a", &[good, bad]).unwrap().state,
        State::Unverified
    );
}

#[test]
fn an_unrun_second_obligation_prevents_whole_task_green() {
    let mut value = draft();
    let mut build = value["checks"][0].clone();
    build["id"] = "build".into();
    build["category"] = "build".into();
    build["argv"] = serde_json::json!(["npm", "run", "build"]);
    build["runner"] = "npm".into();
    value["checks"].as_array_mut().unwrap().push(build);
    value["criteria"][0]["check_ids"] = serde_json::json!(["pytest", "build"]);
    let d = document(&value);
    let result = d.evaluate("tree-a", &[passed(&d)]).unwrap();
    assert_eq!(result.checks[0].state, State::Verified);
    assert_eq!(result.checks[1].state, State::Unverified);
    assert_eq!(result.state, State::Unverified);
}

#[test]
fn subjective_acceptance_is_not_invented_from_passing_checks() {
    let mut value = draft();
    value["criteria"][0]["human_acceptance_required"] = true.into();
    let d = document(&value);
    assert_eq!(
        d.evaluate("tree-a", &[passed(&d)]).unwrap().state,
        State::Unverified
    );
}

#[test]
fn exact_document_bytes_and_each_check_input_affect_identity() {
    let d = document(&draft());
    let pretty = Document::parse(&serde_json::to_vec_pretty(&draft()).unwrap()).unwrap();
    assert_ne!(d.digest(), pretty.digest());
    for (field, value) in [
        (
            "argv",
            serde_json::json!(["python3", "-m", "pytest", "-k", "one"]),
        ),
        ("cwd", serde_json::json!("subproject")),
        ("runner", serde_json::json!("other")),
        ("mode", serde_json::json!("isolated")),
        ("timeout_seconds", serde_json::json!(60)),
        (
            "artifacts",
            serde_json::json!([{ "path": "junit.xml", "sha256": null }]),
        ),
    ] {
        let mut value_doc = draft();
        value_doc["checks"][0][field] = value;
        let changed = document(&value_doc);
        assert_ne!(
            d.contract().checks[0].digest().unwrap(),
            changed.contract().checks[0].digest().unwrap(),
            "{field} was omitted"
        );
    }
}
