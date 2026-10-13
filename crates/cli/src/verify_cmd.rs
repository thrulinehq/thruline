//! Verify output masks display strings without changing core receipts.

use anyhow::{bail, Result};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use thruline_core::chain::sha256_hex;
use thruline_core::error::CoreError;
use thruline_core::record_verification::{verify_with_report, VerifyReport};
use thruline_core::redact::Masked;
use thruline_core::Ledger;

/// Keep this builder separate from the top-level command's derive frame.
#[derive(Debug, clap::Args)]
pub(crate) struct VerifyArgs {
    #[arg(long, allow_hyphen_values = true)]
    pub(crate) project: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

type CaptureHoldCheck = fn(&Ledger) -> thruline_core::error::Result<u64>;

struct Checks {
    report: VerifyReport,
    error: Option<CoreError>,
    observations: thruline_core::error::Result<u64>,
    permissions: thruline_core::error::Result<u64>,
    capture_holds: Option<thruline_core::error::Result<u64>>,
}

pub(crate) fn run(
    db: &std::path::Path,
    project: Option<&str>,
    json: bool,
    capture_check: Option<CaptureHoldCheck>,
) -> Result<std::process::ExitCode> {
    let Checks {
        report,
        error,
        observations,
        permissions,
        capture_holds,
    } = collect_checks(db, project, capture_check)?;
    let exit = exit_code(
        &report,
        error.as_ref(),
        &observations,
        &permissions,
        capture_holds.as_ref(),
    );
    let state = match primary_exit(&report, error.as_ref()) {
        0 => "INTACT",
        1 => "MISMATCH",
        _ => "UNVERIFIABLE",
    };
    if json {
        crate::json_out::print(
            &crate::json_out::VERIFY,
            serde_json::json!({
                "projects": projects(&report)?,
                "elapsed_ms": report.elapsed_ms,
                "exit_code": exit,
                "verification": {
                    "complete": report.complete,
                    "state": state,
                    "reason": error.as_ref().map(|error| display(&error.to_string())),
                },
                "observations": chain_json(&observations),
                "permissions": chain_json(&permissions),
                "capture_holds": capture_holds.as_ref().map(chain_json),
            }),
        )?;
    } else {
        crate::print_verify(&report);
        if let Some(error) = &error {
            println!(
                "verification   {state} — verification incomplete: {}",
                display(&error.to_string())
            );
        }
        print_chain("observations", &observations);
        print_chain("permissions", &permissions);
        if let Some(check) = &capture_holds {
            print_chain("capture holds", check);
        }
    }
    Ok(std::process::ExitCode::from(exit))
}

fn collect_checks(
    db: &std::path::Path,
    project: Option<&str>,
    capture_check: Option<CaptureHoldCheck>,
) -> thruline_core::error::Result<Checks> {
    let started = std::time::Instant::now();
    match Ledger::open(db) {
        Err(error @ CoreError::UpgradePending { .. }) => Err(error),
        Err(error) => Ok(Checks {
            report: VerifyReport {
                elapsed_ms: started.elapsed().as_millis(),
                ..VerifyReport::default()
            },
            error: Some(error),
            observations: unavailable(),
            permissions: unavailable(),
            capture_holds: capture_check.map(|_| unavailable()),
        }),
        Ok(ledger) => Ok(checks_for_ledger(&ledger, project, capture_check)),
    }
}

fn unavailable() -> thruline_core::error::Result<u64> {
    Err(CoreError::Config("not checked: ledger unavailable".into()))
}

fn checks_for_ledger(
    ledger: &Ledger,
    project: Option<&str>,
    capture_check: Option<CaptureHoldCheck>,
) -> Checks {
    let (report, error) = match verify_with_report(ledger, project) {
        Ok(report) => (report, None),
        Err(failure) => (failure.report, Some(failure.error)),
    };
    // A205, P6-5B and C30R-LEDGER: every supplied machine-wide chain remains
    // checked even when a selected project's source verification aborts.
    Checks {
        report,
        error,
        observations: thruline_core::observation::verify_chain(ledger),
        permissions: thruline_core::run_permission::verify_chain(ledger),
        capture_holds: capture_check.map(|check| check(ledger)),
    }
}

pub(crate) fn display(raw: &str) -> String {
    Masked::mask(raw).into_text()
}

fn mask_strings(value: &mut Value) {
    match value {
        Value::String(text) => *text = display(text),
        Value::Array(items) => items.iter_mut().for_each(mask_strings),
        Value::Object(fields) => fields.values_mut().for_each(mask_strings),
        _ => {}
    }
}

pub(crate) fn projects(report: &VerifyReport) -> Result<Value> {
    let mut keys = Vec::with_capacity(report.projects.len());
    let mut unique = BTreeSet::new();
    let mut collision = false;
    for native in report.projects.keys() {
        let label = display(native);
        let identity = sha256_hex(native.as_bytes());
        let key = if label == *native {
            native.clone()
        } else {
            format!("{label}#{identity}")
        };
        collision |= !unique.insert(key.clone());
        keys.push((key, label, identity));
    }
    let mut out = Map::new();
    for ((_, verdict), (key, label, identity)) in report.projects.iter().zip(keys) {
        let key = if collision {
            format!("project_sha256:{identity}")
        } else {
            key
        };
        let mut value = serde_json::to_value(verdict)?;
        mask_strings(&mut value);
        let Some(fields) = value.as_object_mut() else {
            bail!("verify display projection is not a project object");
        };
        fields.insert("project_label".into(), Value::String(label));
        fields.insert("project_identity_sha256".into(), Value::String(identity));
        if out.insert(key, value).is_some() {
            bail!("verify project identity collision; no display emitted");
        }
    }
    Ok(Value::Object(out))
}

pub(crate) fn chain_state(result: &thruline_core::error::Result<u64>) -> &'static str {
    match result {
        Ok(_) => "INTACT",
        Err(CoreError::Integrity(_)) => "MISMATCH",
        Err(_) => "UNVERIFIABLE",
    }
}

pub(crate) fn chain_json(result: &thruline_core::error::Result<u64>) -> Value {
    serde_json::json!({
        "state": chain_state(result),
        "links_checked": result.as_ref().ok(),
        "reason": result.as_ref().err().map(|error| display(&error.to_string())),
    })
}

pub(crate) fn exit_code(
    report: &VerifyReport,
    error: Option<&CoreError>,
    observations: &thruline_core::error::Result<u64>,
    permissions: &thruline_core::error::Result<u64>,
    capture_holds: Option<&thruline_core::error::Result<u64>>,
) -> u8 {
    let states = [
        Some(chain_state(observations)),
        Some(chain_state(permissions)),
        capture_holds.map(chain_state),
    ];
    if primary_exit(report, error) == 1 || states.contains(&Some("MISMATCH")) {
        1
    } else if primary_exit(report, error) == 2 || states.contains(&Some("UNVERIFIABLE")) {
        2
    } else {
        0
    }
}

#[cfg(test)]
#[path = "verify_capture_holds_tests.rs"]
mod verify_capture_holds_tests;

fn primary_exit(report: &VerifyReport, error: Option<&CoreError>) -> u8 {
    if report.exit_code() == 1 || matches!(error, Some(CoreError::Integrity(_))) {
        1
    } else if error.is_some() || report.exit_code() == 2 {
        2
    } else {
        0
    }
}

pub(crate) fn print_chain(label: &str, result: &thruline_core::error::Result<u64>) {
    match result {
        Ok(n) => println!(
            "{label:<15}chain re-checked, {n} {} add up",
            if *n == 1 { "link" } else { "links" }
        ),
        Err(error) => println!(
            "{label:<15}{} — {}",
            if chain_state(result) == "MISMATCH" {
                "CHAIN DOES NOT ADD UP"
            } else {
                "UNVERIFIABLE"
            },
            display(&error.to_string())
        ),
    }
}
