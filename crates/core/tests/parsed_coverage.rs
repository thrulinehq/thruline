//! A27 — parsed-coverage metric, by volume, per tool family.
//!
//! `#[ignore]`d: needs the real ledger. Run:
//!
//! ```text
//! cargo test --release --test parsed_coverage -- --ignored --nocapture
//! ```
//!
//! **By volume, not by shape.** What matters is the share of evidence rows the
//! scorer will actually be handed that we understood — a rare unparsed family
//! costs nothing, a common one costs everything. Coverage is simultaneously the
//! ceiling on how much the product can confirm (`done` requires parsed
//! fidelity) and the floor under how often it must abstain.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[path = "support/live_store.rs"]
mod live_store;

use thruline_core::ledger::Ledger;
use thruline_core::premise::{render_tagged, Fidelity};

const PROJECTS: [&str; 3] = [
    "-Users-dev-thruline",
    "-Users-dev-Documents-ATLAS",
    "-Users-dev-Developer-market",
];

/// The family an evidence row belongs to. Bash rows are split by executable,
/// because "Bash" is not one shape — `cargo test` and `mkdir -p` have nothing
/// in common from the renderer's point of view.
fn family(tool: &str, command: Option<&str>) -> String {
    if tool != "Bash" {
        return tool.to_string();
    }
    let Some(cmd) = command else {
        return "bash:(none)".to_string();
    };
    // First token that is not an env assignment, a `.`-source or a `cd`.
    let mut exe = "";
    for tok in cmd.split_whitespace() {
        if tok.contains('=') || tok == "." || tok == "cd" || tok == "sudo" || tok == "&&" {
            continue;
        }
        exe = tok;
        break;
    }
    let exe = exe.rsplit('/').next().unwrap_or(exe);
    let exe: String = exe
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
        .collect();
    if exe.is_empty() {
        "bash:(other)".to_string()
    } else {
        format!("bash:{exe}")
    }
}

#[derive(Default, Clone, Copy)]
struct Cov {
    parsed: u64,
    total: u64,
}

impl Cov {
    fn pct(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            100.0 * self.parsed as f64 / self.total as f64
        }
    }
}

#[test]
#[ignore]
fn parsed_coverage_by_family() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let db = PathBuf::from(home).join(".thruline/ledger.db");
    if !db.exists() {
        eprintln!("SKIP: no ledger");
        return;
    }
    live_store::refuse_live_store(&db);
    let ledger = Ledger::open(&db).expect("open");

    let mut grand: BTreeMap<String, Cov> = BTreeMap::new();
    let mut grand_total = Cov::default();

    for p in PROJECTS {
        let g = thruline_core::derive::derive_project(&ledger, p).expect("derive");
        let mut per_family: BTreeMap<String, Cov> = BTreeMap::new();
        let mut proj = Cov::default();
        for node in &g.nodes {
            for row in &node.evidence {
                let f = family(&row.tool_name, row.command.as_deref());
                let r = render_tagged(row);
                let is_parsed = r.fidelity == Fidelity::Parsed;
                for slot in [
                    per_family.entry(f.clone()).or_default(),
                    grand.entry(f.clone()).or_default(),
                ] {
                    slot.total += 1;
                    slot.parsed += u64::from(is_parsed);
                }
                for slot in [&mut proj, &mut grand_total] {
                    slot.total += 1;
                    slot.parsed += u64::from(is_parsed);
                }
            }
        }
        eprintln!(
            "\n=== {p} ===\nPARSED {}/{} = {:.1}%   (fallback {:.1}%)",
            proj.parsed,
            proj.total,
            proj.pct(),
            100.0 - proj.pct()
        );
        let mut fams: Vec<(&String, &Cov)> = per_family.iter().collect();
        fams.sort_by_key(|(_, c)| std::cmp::Reverse(c.total));
        for (f, c) in fams.iter().take(14) {
            eprintln!(
                "  {f:<26} {:>6} rows ({:>4.1}% of project)  parsed {:>5.1}%  UNPARSED {:>5}",
                c.total,
                100.0 * c.total as f64 / proj.total.max(1) as f64,
                c.pct(),
                c.total - c.parsed
            );
        }
    }

    eprintln!(
        "\n=== ALL THREE PROJECTS ===\nPARSED {}/{} = {:.1}%",
        grand_total.parsed,
        grand_total.total,
        grand_total.pct()
    );
    // Ranked by how much unparsed volume each family is responsible for: the
    // work list, in the order that actually moves the number.
    let mut fams: Vec<(&String, &Cov)> = grand.iter().collect();
    fams.sort_by_key(|(_, c)| std::cmp::Reverse(c.total - c.parsed));
    eprintln!("\nUNPARSED VOLUME, ranked (the work list):");
    for (f, c) in fams.iter().take(20) {
        let un = c.total - c.parsed;
        if un == 0 {
            continue;
        }
        eprintln!(
            "  {f:<26} {un:>6} unparsed of {:>6}  ({:>4.1}% of ALL unparsed)",
            c.total,
            100.0 * un as f64 / (grand_total.total - grand_total.parsed).max(1) as f64
        );
    }
}
