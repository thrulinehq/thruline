//! A deliberately small JSON Schema evaluator, used only by the conformance
//! suite that holds `spec/execution-check-v0.1.schema.json` to the producer.
//!
//! # Why this exists rather than a dependency
//!
//! No JSON Schema crate is vendored in this workspace and this program does not
//! reach the network. So the choice was: ship no validation, or implement the
//! subset the published schema actually uses. A schema nothing validates
//! against is documentation, so this is the subset.
//!
//! # The honest limits, stated where they can be read
//!
//! This evaluator implements: `$ref` (local `#/$defs/…` only), `type`, `enum`,
//! `const`, `properties`, `required`, `additionalProperties`, `items`,
//! `minItems`, `minLength`, `minimum`, `minProperties`, `pattern`, `allOf`,
//! `if`/`then`, and `not`. **Anything else in a schema is a hard error, not a silent pass** —
//! an evaluator that ignores the keyword it does not know is an evaluator that
//! reports every document as valid, which is this program's signature defect
//! wearing a validator's name.
//!
//! `pattern` is likewise not a regex engine. Each pattern string the schema uses
//! is matched by a named function, and a pattern outside that table is an error.
//! `spec_conformance::every_pattern_in_the_schema_is_implemented_here` asserts
//! the two sets agree, so a new pattern in the schema cannot silently become a
//! no-op.

#![allow(dead_code)]

use serde_json::Value;

/// Every keyword this evaluator understands. A schema using anything else is
/// rejected by [`check_keywords`] rather than quietly under-validated.
const KNOWN: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "$defs",
    "$ref",
    "title",
    "description",
    "examples",
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "minLength",
    "minimum",
    "minProperties",
    "pattern",
    "allOf",
    "if",
    "then",
    "not",
];

/// A pattern string and the function that matches it exactly.
pub type PatternMatcher = (&'static str, fn(&str) -> bool);

/// The exact `pattern` strings this evaluator can match, each with a matcher
/// that is correct for that string — never an approximation.
pub fn patterns() -> Vec<PatternMatcher> {
    fn sha256_64(s: &str) -> bool {
        s.strip_prefix("sha256:")
            .is_some_and(|h| h.len() == 64 && h.bytes().all(is_hex))
    }
    fn sha256_32_plus(s: &str) -> bool {
        s.strip_prefix("sha256:")
            .is_some_and(|h| h.len() >= 32 && h.bytes().all(is_hex))
    }
    fn hex_40_64(s: &str) -> bool {
        (40..=64).contains(&s.len()) && s.bytes().all(is_hex)
    }
    vec![
        ("^sha256:[0-9a-f]{64}$", sha256_64 as fn(&str) -> bool),
        ("^sha256:[0-9a-f]{32,}$", sha256_32_plus),
        ("^[0-9a-f]{40,64}$", hex_40_64),
    ]
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

/// Walk every subschema and fail on a keyword this evaluator does not
/// implement. Called by the conformance suite before any document is judged.
pub fn check_keywords(schema: &Value, path: &str, out: &mut Vec<String>) {
    let Some(map) = schema.as_object() else {
        return;
    };
    for (k, v) in map {
        // `x-` is the conventional annotation prefix: non-normative by
        // definition, ignored by every validator, and used here to carry the
        // stability contract and the cannot-express list inside the document
        // they are about.
        if !KNOWN.contains(&k.as_str()) && !k.starts_with("x-") {
            out.push(format!("{path}: unimplemented schema keyword {k:?}"));
        }
        match k.as_str() {
            "properties" | "$defs" => {
                if let Some(m) = v.as_object() {
                    for (name, sub) in m {
                        check_keywords(sub, &format!("{path}/{k}/{name}"), out);
                    }
                }
            }
            "additionalProperties" | "items" | "if" | "then" | "not" => {
                if v.is_object() {
                    check_keywords(v, &format!("{path}/{k}"), out)
                }
            }
            "allOf" => {
                if let Some(a) = v.as_array() {
                    for (i, sub) in a.iter().enumerate() {
                        check_keywords(sub, &format!("{path}/allOf/{i}"), out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collect every `pattern` string used anywhere in the schema.
pub fn patterns_used(schema: &Value, out: &mut Vec<String>) {
    match schema {
        Value::Object(m) => {
            if let Some(Value::String(p)) = m.get("pattern") {
                out.push(p.clone());
            }
            for v in m.values() {
                patterns_used(v, out);
            }
        }
        Value::Array(a) => {
            for v in a {
                patterns_used(v, out);
            }
        }
        _ => {}
    }
}

/// Validate `doc` against `schema`. Returns every violation; empty means valid.
pub fn validate(root: &Value, schema: &Value, doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    eval(root, schema, doc, "$", &mut out);
    out
}

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let Some(r) = schema.get("$ref").and_then(Value::as_str) else {
        return schema;
    };
    let mut node = root;
    for seg in r.trim_start_matches("#/").split('/') {
        match node.get(seg) {
            Some(next) => node = next,
            None => return schema,
        }
    }
    node
}

fn eval(root: &Value, schema: &Value, doc: &Value, path: &str, out: &mut Vec<String>) {
    let schema = resolve(root, schema);
    let Some(map) = schema.as_object() else {
        return;
    };

    if let Some(t) = map.get("type") {
        let ok = match t {
            Value::String(s) => type_matches(s, doc),
            Value::Array(a) => a
                .iter()
                .filter_map(Value::as_str)
                .any(|s| type_matches(s, doc)),
            _ => true,
        };
        if !ok {
            out.push(format!("{path}: expected type {t}, found {}", kind_of(doc)));
            return;
        }
    }
    if let Some(Value::Array(vals)) = map.get("enum") {
        if !vals.contains(doc) {
            out.push(format!(
                "{path}: {doc} is not one of {}",
                Value::Array(vals.clone())
            ));
        }
    }
    if let Some(c) = map.get("const") {
        if doc != c {
            out.push(format!("{path}: expected const {c}, found {doc}"));
        }
    }
    if let Some(p) = map.get("pattern").and_then(Value::as_str) {
        if let Some(s) = doc.as_str() {
            match patterns().into_iter().find(|(k, _)| *k == p) {
                Some((_, f)) => {
                    if !f(s) {
                        out.push(format!("{path}: {s:?} does not match {p}"));
                    }
                }
                None => out.push(format!("{path}: pattern {p:?} is not implemented")),
            }
        }
    }
    if let Some(n) = map.get("minLength").and_then(Value::as_u64) {
        if let Some(s) = doc.as_str() {
            if (s.len() as u64) < n {
                out.push(format!("{path}: shorter than minLength {n}"));
            }
        }
    }
    if let Some(n) = map.get("minimum").and_then(Value::as_i64) {
        if let Some(x) = doc.as_i64() {
            if x < n {
                out.push(format!("{path}: {x} is below minimum {n}"));
            }
        }
    }
    if let Some(n) = map.get("minItems").and_then(Value::as_u64) {
        if let Some(a) = doc.as_array() {
            if (a.len() as u64) < n {
                out.push(format!("{path}: fewer than minItems {n}"));
            }
        }
    }
    if let Some(n) = map.get("minProperties").and_then(Value::as_u64) {
        if let Some(o) = doc.as_object() {
            if (o.len() as u64) < n {
                out.push(format!(
                    "{path}: has {} propert(ies), minProperties is {n}",
                    o.len()
                ));
            }
        }
    }
    if let Some(Value::Array(req)) = map.get("required") {
        if let Some(o) = doc.as_object() {
            for r in req.iter().filter_map(Value::as_str) {
                if !o.contains_key(r) {
                    out.push(format!("{path}: required property {r:?} is missing"));
                }
            }
        }
    }
    if let (Some(Value::Object(props)), Some(o)) = (map.get("properties"), doc.as_object()) {
        for (name, sub) in props {
            if let Some(child) = o.get(name) {
                eval(root, sub, child, &format!("{path}.{name}"), out);
            }
        }
    }
    if let (Some(ap), Some(o)) = (map.get("additionalProperties"), doc.as_object()) {
        let declared: Vec<&String> = map
            .get("properties")
            .and_then(Value::as_object)
            .map(|p| p.keys().collect())
            .unwrap_or_default();
        for (name, child) in o {
            if declared.contains(&name) {
                continue;
            }
            match ap {
                Value::Bool(false) => out.push(format!(
                    "{path}: property {name:?} is not declared by the schema"
                )),
                Value::Object(_) => eval(root, ap, child, &format!("{path}.{name}"), out),
                _ => {}
            }
        }
    }
    if let (Some(items), Some(a)) = (map.get("items"), doc.as_array()) {
        for (i, child) in a.iter().enumerate() {
            eval(root, items, child, &format!("{path}[{i}]"), out);
        }
    }
    if let Some(Value::Array(all)) = map.get("allOf") {
        for (i, sub) in all.iter().enumerate() {
            eval(root, sub, doc, &format!("{path}(allOf/{i})"), out);
        }
    }
    if let Some(cond) = map.get("if") {
        if validate(root, cond, doc).is_empty() {
            if let Some(then) = map.get("then") {
                eval(root, then, doc, &format!("{path}(then)"), out);
            }
        }
    }
    if let Some(n) = map.get("not") {
        if validate(root, n, doc).is_empty() {
            out.push(format!("{path}: matched a `not` subschema"));
        }
    }
}

fn type_matches(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
