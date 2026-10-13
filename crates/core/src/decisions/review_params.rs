//! A terminal review may display a public, exact Rust test identity that the
//! JSON quote wrapper alone makes resemble an opaque high-entropy value.
//! Every other value keeps the ordinary full-document mask gate.

use std::path::Path;

use serde_json::Value;

use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::{resembles_known_credential, Masked};

use super::params::CheckParams;
use super::text_exclusions::test_spans;

pub(super) fn safe_for_terminal(
    ledger: &Ledger,
    project_id: &str,
    params: &CheckParams,
    params_json: &str,
    acceptance_version: &str,
) -> Result<bool> {
    if resembles_known_credential(params_json) {
        return Ok(false);
    }
    if Masked::mask(params_json).text() == params_json {
        return Ok(true);
    }
    let CheckParams::TextAbsent(params) = params else {
        return Ok(false);
    };
    let mut selected = Vec::new();
    enum Position {
        Test(usize),
        Support(usize, usize),
    }
    let mut positions = Vec::new();
    let mut selectors = params
        .test_allowlist
        .iter()
        .enumerate()
        .map(|(index, scope)| (scope, Position::Test(index)))
        .collect::<Vec<_>>();
    for (index, span) in params.span_allowlist.iter().enumerate() {
        for (support_index, scope) in span.supports.iter().enumerate() {
            selectors.push((scope, Position::Support(index, support_index)));
        }
    }
    for (scope, position) in selectors {
        if !scope.path.ends_with(".rs") || scope.name_pattern.contains('*') {
            continue;
        }
        let name = &scope.name_pattern;
        let quoted = Masked::mask(&serde_json::to_string(name)?);
        let record = quoted.record();
        if record.complete
            && record.masked == 1
            && record.families.len() == 1
            && record.families.contains("high_entropy")
            && Masked::mask(name).text() == name
            && !resembles_known_credential(name)
        {
            if !selected.contains(scope) {
                selected.push(scope.clone());
            }
            positions.push(position);
        }
    }
    if selected.is_empty()
        || !matches!(acceptance_version.len(), 40 | 64)
        || !acceptance_version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(false);
    }
    let root: String = ledger.conn().query_row(
        "SELECT root_path FROM projects WHERE id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    let Ok(spans) = test_spans(Path::new(&root), acceptance_version, &selected) else {
        return Ok(false);
    };
    for scope in &selected {
        if spans
            .iter()
            .filter(|span| {
                span.path == scope.path
                    && span.receipt.test_name.as_deref() == Some(&scope.name_pattern)
            })
            .count()
            != 1
        {
            return Ok(false);
        }
    }
    // Change only a throwaway mask-check copy. The review and its digest still
    // use the original stored bytes and values. Any other firing stays closed.
    let mut checked: Value = serde_json::from_str(params_json)?;
    for position in positions {
        let name = match position {
            Position::Test(index) => &mut checked["test_allowlist"][index]["name_pattern"],
            Position::Support(index, support_index) => {
                &mut checked["span_allowlist"][index]["supports"][support_index]["name_pattern"]
            }
        };
        *name = Value::String("verified_exact_rust_test_identity".into());
    }
    let checked = serde_json::to_string(&checked)?;
    Ok(Masked::mask(&checked).text() == checked)
}
