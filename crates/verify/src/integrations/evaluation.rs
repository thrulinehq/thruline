//! Deterministic evaluation of supplied case labels and measured predictions.
//! This reports sample accounting, never authenticates an oracle or graduates a verifier.
use super::DecodeError;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize)]
struct Counts {
    cases: usize,
    clean: usize,
    violations: usize,
    incomplete: usize,
    detected_violations: usize,
    false_positives: usize,
    missed_failures: usize,
    violation_false_clears: usize,
    violation_abstentions: usize,
    incomplete_false_clears: usize,
    incomplete_overstatements: usize,
    clean_abstentions: usize,
    missing_predictions: usize,
    harness_errors: usize,
}
impl Counts {
    fn add(&mut self, label: &str, actual: &str) {
        self.cases += 1;
        self.missing_predictions += usize::from(actual == "missing");
        self.harness_errors += usize::from(actual == "harness_error");
        match label {
            "clean" => {
                self.clean += 1;
                self.false_positives += usize::from(actual == "failed");
                self.clean_abstentions += usize::from(!matches!(actual, "passed" | "failed"));
            }
            "failure" => {
                self.violations += 1;
                self.detected_violations += usize::from(actual == "failed");
                self.missed_failures += usize::from(actual != "failed");
                self.violation_false_clears += usize::from(actual == "passed");
                self.violation_abstentions += usize::from(!matches!(actual, "passed" | "failed"));
            }
            "incomplete" => {
                self.incomplete += 1;
                self.incomplete_false_clears += usize::from(actual == "passed");
                self.incomplete_overstatements += usize::from(actual == "failed");
            }
            _ => unreachable!("validated label"),
        }
    }
}
fn invalid(s: &str) -> DecodeError {
    DecodeError::Invalid(s.into())
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, DecodeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(&format!("missing {key}")))
}
fn cases(value: &Value) -> Result<&Vec<Value>, DecodeError> {
    let rows = value
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("missing cases"))?;
    if rows.len() > 10_000 {
        return Err(invalid("evaluation case budget exceeded"));
    }
    Ok(rows)
}
pub fn evaluate(labels: &Value, predictions: &Value) -> Result<Value, DecodeError> {
    if !matches!(
        text(labels, "schema")?,
        "thruline.integration-qualification-cases.v1"
            | "thruline.integration-qualification-cases.v2"
    ) || text(predictions, "schema")? != "thruline.upstream-qualification.v1"
    {
        return Err(invalid("unsupported evaluation contract"));
    }
    let labels = cases(labels)?;
    if labels.is_empty() {
        return Err(invalid("empty evaluation is not qualification"));
    }
    let mut measured = BTreeMap::new();
    for row in cases(predictions)? {
        let id = text(row, "id")?;
        let actual = text(row, "actual")?;
        if !matches!(
            actual,
            "passed" | "failed" | "unknown" | "blocked" | "stale" | "reported" | "harness_error"
        ) {
            return Err(invalid("unknown prediction state"));
        }
        if measured.insert(id, row).is_some() {
            return Err(invalid("duplicate prediction ID"));
        }
    }
    let mut seen = BTreeSet::new();
    let mut total = Counts::default();
    let mut adapters = BTreeMap::<String, Counts>::new();
    let mut classes = BTreeMap::<String, Counts>::new();
    let mut rows = vec![];
    for row in labels {
        let id = text(row, "id")?;
        let pack = text(row, "pack")?;
        let label = text(row, "label")?;
        if !seen.insert(id) {
            return Err(invalid("duplicate oracle ID"));
        }
        if !matches!(label, "clean" | "failure" | "incomplete") {
            return Err(invalid("unknown oracle label"));
        }
        let prediction = measured.remove(id);
        if prediction.is_some_and(|p| p.get("pack").and_then(Value::as_str) != Some(pack)) {
            return Err(invalid("prediction belongs to a different adapter"));
        }
        let actual = prediction
            .map(|p| text(p, "actual"))
            .transpose()?
            .unwrap_or("missing");
        let class = row
            .get("case_class")
            .and_then(Value::as_str)
            .unwrap_or(label);
        total.add(label, actual);
        adapters.entry(pack.into()).or_default().add(label, actual);
        classes.entry(class.into()).or_default().add(label, actual);
        rows.push(
            json!({"id":id,"pack":pack,"label":label,"actual":actual,"class":class,
            "oracle":row.get("oracle"),"oracle_origin":row.get("label_origin"),
            "declared_blind":row.get("blind").and_then(Value::as_bool).unwrap_or(false),
            "binding_id":prediction.and_then(|p|p.get("binding_id")),
            "artifact":prediction.and_then(|p|p.get("artifact"))}),
        );
    }
    if !measured.is_empty() {
        return Err(invalid("prediction has no matching oracle"));
    }
    Ok(
        json!({"schema":"thruline.integration-evaluation.v1","qualified":false,"enabled":"advisory_only",
        "authority":"Evaluation of supplied labels and predictions; oracle independence and execution custody are not authenticated by this calculation.",
        "definitions":{"missed_failures":"Labeled violations not detected as failed, including abstentions/missing results; false clears are broken out separately.",
        "false_positives":"Labeled clean scopes classified failed; policy-exclusion cases have their own class and do not prove an unqualified project clean.",
        "incomplete_overstatements":"An incomplete labeled scope classified failed; review whether the condition overstates available evidence."},
        "total":total,"by_adapter":adapters,"by_class":classes,"cases":rows}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn labels() -> Value {
        json!({"schema":"thruline.integration-qualification-cases.v2","cases":[{"id":"a","pack":"tool","label":"clean"},{"id":"b","pack":"tool","label":"failure"},{"id":"c","pack":"tool","label":"incomplete"}]})
    }
    fn predictions() -> Value {
        json!({"schema":"thruline.upstream-qualification.v1","cases":[{"id":"a","pack":"tool","actual":"failed"},{"id":"b","pack":"tool","actual":"unknown"},{"id":"c","pack":"tool","actual":"passed"}]})
    }
    #[test]
    fn abstention_false_alarm_and_false_clear_have_separate_denominators() {
        let r = evaluate(&labels(), &predictions()).unwrap();
        assert_eq!(r["total"]["false_positives"], 1);
        assert_eq!(r["total"]["missed_failures"], 1);
        assert_eq!(r["total"]["violation_false_clears"], 0);
        assert_eq!(r["total"]["violation_abstentions"], 1);
        assert_eq!(r["total"]["incomplete_false_clears"], 1);
        assert_eq!(r["qualified"], false);
    }
    #[test]
    fn missing_predictions_remain_in_the_denominator() {
        let r = evaluate(
            &labels(),
            &json!({"schema":"thruline.upstream-qualification.v1","cases":[]}),
        )
        .unwrap();
        assert_eq!(r["total"]["cases"], 3);
        assert_eq!(r["total"]["missing_predictions"], 3);
        assert_eq!(r["total"]["missed_failures"], 1);
    }
    #[test]
    fn duplicate_unmatched_and_cross_adapter_predictions_are_refused() {
        let mut p = predictions();
        p["cases"][1]["id"] = json!("a");
        assert!(evaluate(&labels(), &p).is_err());
        let mut p = predictions();
        p["cases"][1]["id"] = json!("wrong");
        assert!(evaluate(&labels(), &p).is_err());
        let mut p = predictions();
        p["cases"][1]["pack"] = json!("wrong");
        assert!(evaluate(&labels(), &p).is_err());
    }
}
