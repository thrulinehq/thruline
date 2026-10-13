//! PRICING-TRUTHS-IN-BINARY — the pricing census, over everything the binary
//! itself can say about plans.
//!
//! The product names one plan anyone can buy, Pro (founder ruling 2, D-298:
//! "Pro is the only purchasable plan. Team licence code may stay dormant, but
//! Team must not be buyable or shown anywhere a user sees."), states no amount
//! anywhere (a price compiled into a binary cannot follow a price the website
//! changes), and calls its free third view Workbench (ruling #5,
//! R-2026-09-26-3), so "Pro" is the plan's name and nothing else's.
//!
//! The census reads four sources:
//!  1. every hq-copy golden value (`ui/src/workspace/hq-copy.golden.json`, the
//!     rendered copy file — the screen's words);
//!  2. every Problem, Verdict and refusal sentence `crates/licence` can
//!     produce, enumerated against its own `SLUGS` so a new one cannot hide;
//!  3. every `--help` the shipping binary prints, walked from `thruline --help`
//!     down through every listed subcommand, plus the hidden ones by name;
//!  4. the licence status document (`licence status --json`, the body
//!     `GET /api/licence` serves) and its one-line form, in every state a
//!     process can be put in here.
//!
//! It fails on:
//!  - an amount: `[$€£]` then a digit, `USD`, `per month`, `per year`,
//!    `per seat`, `/mo`;
//!  - a tier nobody can buy: `Team` or `Enterprise` (singular or plural) as a
//!    capitalised word, which is the card's rule and how a plan is named, in
//!    the words a person reads (the golden, the licence sentences, every
//!    `--help`). The status document is a wire format whose values are
//!    lowercase (`"plan": "team"`), so there the rule folds case — except text
//!    reachable only when such a licence is installed: its `plan` field reads
//!    `"team"` for an installed Team licence, and nothing else is exempt;
//!  - `Pro` (case-sensitive word) anywhere except as the plan name — the plan on
//!    its own, a Pro licence, "Get Pro at" the pricing page, or "Licence: Pro,"
//!    — and, in the golden, only under a `licence.` key.
//!
//! The screen's half of the same rules runs under `npm test` as
//! `ui/src/workspace/pricing-census.test.js`.
//!
//! Nothing here opens a socket or reads the pricing page: the website is not a
//! source of this census, and the plans link is only ever a string.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::Value;
use thruline_licence::{decide, Licence, Plan, Problem, Source, Surface, Verdict};

/* ── the rules ─────────────────────────────────────────────────────────── */

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Every byte offset where `word` stands as a whole word in `text`.
fn word_at(text: &str, word: &str, fold: bool) -> Vec<usize> {
    let (hay, needle) = if fold {
        (text.to_lowercase(), word.to_lowercase())
    } else {
        (text.to_string(), word.to_string())
    };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(&needle) {
        let at = from + i;
        let end = at + needle.len();
        let before = hay[..at].chars().next_back();
        let after = hay[end..].chars().next();
        if !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char) {
            out.push(at);
        }
        from = at + needle.len().max(1);
    }
    out
}

fn has_word(text: &str, word: &str, fold: bool) -> bool {
    !word_at(text, word, fold).is_empty()
}

/// The amount rule.
fn amount(text: &str) -> Option<&'static str> {
    let chars: Vec<char> = text.chars().collect();
    if chars
        .windows(2)
        .any(|w| matches!(w[0], '$' | '€' | '£') && w[1].is_ascii_digit())
    {
        return Some("a currency sign before a digit");
    }
    if has_word(text, "USD", false) {
        return Some("USD");
    }
    for phrase in ["per month", "per year", "per seat"] {
        if has_word(text, phrase, true) {
            return Some(phrase);
        }
    }
    let lower = text.to_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("/mo") {
        let end = from + i + 3;
        if !lower[end..].chars().next().is_some_and(is_word_char) {
            return Some("/mo");
        }
        from = end;
    }
    None
}

/// How the tier rule reads one text.
#[derive(Clone, Copy, PartialEq)]
enum TierRule {
    /// Words a person reads: the capitalised plan names.
    Named,
    /// A wire document: any case.
    Folded,
    /// Reachable only with such a licence installed.
    Exempt,
}

/// The tier rule: a plan nobody can buy, named.
fn tier(text: &str, rule: TierRule) -> Option<&'static str> {
    let words: &[&'static str] = match rule {
        TierRule::Named => &["Team", "Teams", "Enterprise", "Enterprises"],
        TierRule::Folded => &["team", "teams", "enterprise", "enterprises"],
        TierRule::Exempt => &[],
    };
    words
        .iter()
        .copied()
        .find(|w| has_word(text, w, rule == TierRule::Folded))
}

/// The Pro rule: true when `Pro` is left once the plan-name forms are taken out.
fn pro_not_as_plan(text: &str, plan_forms_allowed: bool) -> bool {
    let mut rest = text.to_string();
    if plan_forms_allowed {
        if rest == "Pro" {
            return false;
        }
        rest = rest.replace("ThruLine Pro licence", "");
        rest = rest.replace("Pro licence", "");
        rest = rest.replace("Get Pro at ", "");
        if let Some(tail) = rest.strip_prefix("Licence: Pro,") {
            rest = tail.to_string();
        }
    }
    has_word(&rest, "Pro", false)
}

/// Every reason one text breaks the census.
fn hits(text: &str, plan_forms_allowed: bool, rule: TierRule) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(a) = amount(text) {
        out.push(format!("amount ({a})"));
    }
    if let Some(t) = tier(text, rule) {
        out.push(format!("tier ({t})"));
    }
    if pro_not_as_plan(text, plan_forms_allowed) {
        out.push("\"Pro\" not as the plan name".into());
    }
    out
}

#[test]
fn the_rules_are_a_live_instrument() {
    for s in [
        "$20 a month",
        "€5",
        "£9",
        "20 USD",
        "10 per month",
        "100 Per Year",
        "30 per seat",
        "$9/mo",
        "9/mo.",
    ] {
        assert!(
            hits(s, true, TierRule::Named)
                .iter()
                .any(|h| h.starts_with("amount")),
            "{s}"
        );
    }
    for s in [
        "Team",
        "the Teams plan",
        "Enterprise plan",
        "ThruLine Enterprise",
    ] {
        assert!(
            hits(s, true, TierRule::Named)
                .iter()
                .any(|h| h.starts_with("tier")),
            "{s}"
        );
    }
    for s in ["team", "\"plans\": [\"pro\", \"team\"]", "Enterprise"] {
        assert!(
            hits(s, true, TierRule::Folded)
                .iter()
                .any(|h| h.starts_with("tier")),
            "{s}"
        );
    }
    // A common noun is not a plan name in the words a person reads; the wire
    // document folds case, so there it is.
    assert!(hits(
        "the enterprise managed-settings snippet",
        true,
        TierRule::Named
    )
    .is_empty());
    assert!(!hits("enterprise", true, TierRule::Folded).is_empty());
    assert!(
        hits("team", true, TierRule::Exempt).is_empty(),
        "the exemption exempts"
    );
    for s in ["Pro", "Pro: every control", "Buttons in Pro now"] {
        assert_eq!(
            hits(s, false, TierRule::Named),
            vec!["\"Pro\" not as the plan name"],
            "{s}"
        );
    }
    assert_eq!(
        hits("Pro is the free view", true, TierRule::Named),
        vec!["\"Pro\" not as the plan name"]
    );
    for s in [
        "Pro",
        "This machine has a ThruLine Pro licence, valid until 17 Sep 2026.",
        "Approving a version needs a Pro licence.",
        "Get Pro at https://thrulinehq.com/pricing, then install your licence.",
        "Licence: Pro, until 17 Sep 2026",
    ] {
        assert!(hits(s, true, TierRule::Named).is_empty(), "{s}");
    }
    for s in [
        "Proven",
        "Prove it",
        "a project",
        "protect",
        "$ thruline ui",
        "$HOME/.thruline",
        "~/.thruline/models",
        "steam",
        "pro",
    ] {
        assert!(
            hits(s, false, TierRule::Named).is_empty(),
            "{s}: {:?}",
            hits(s, false, TierRule::Named)
        );
    }
}

/* ── 1. every hq-copy golden value ─────────────────────────────────────── */

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_hq_copy_golden_value_passes_the_census() {
    let path = repo().join("ui/src/workspace/hq-copy.golden.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let golden: serde_json::Map<String, Value> = serde_json::from_str(&text).expect("golden json");
    assert!(golden.len() > 1500, "found {} golden values", golden.len());
    let mut found = Vec::new();
    for (key, value) in &golden {
        let v = value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string());
        for h in hits(&v, key.starts_with("licence."), TierRule::Named) {
            found.push(format!("{key}: {h}: {v:?}"));
        }
    }
    assert!(
        found.is_empty(),
        "golden values that break the census:\n{}",
        found.join("\n")
    );
}

/* ── 2. every sentence the licence crate can say ───────────────────────── */

fn every_problem() -> Vec<Problem> {
    vec![
        Problem::Absent,
        Problem::Unreadable,
        Problem::NotALicence,
        Problem::NewerFormat,
        Problem::NoTrustedKey,
        Problem::UnknownKey,
        Problem::Tampered,
        Problem::WrongProduct,
        Problem::UnknownPlan,
        Problem::Expired {
            on: "1 Sep 2026".into(),
        },
        Problem::WrongMachine,
        Problem::MachineUnknown,
    ]
}

fn licence(plan: Plan) -> Licence {
    Licence {
        licence_id: "01J8ZQ4Y6T3M2N1P0R9S8T7V6W".into(),
        plan,
        issued_at: "2026-09-01T00:00:00Z".into(),
        expires_at: "2099-01-01T00:00:00Z".into(),
        machine_bound: false,
        key_id: "k1".into(),
    }
}

#[test]
fn every_licence_sentence_passes_the_census() {
    let problems = every_problem();
    let slugs: Vec<&str> = problems.iter().map(Problem::slug).collect();
    assert_eq!(slugs, Problem::SLUGS, "a Problem the census does not read");
    let mut said: Vec<(String, String)> = Vec::new();
    for p in &problems {
        said.push((format!("{p:?} problem"), p.problem()));
        said.push((format!("{p:?} fix"), p.fix()));
        said.push((
            format!("{p:?} verdict"),
            Verdict::Refused(p.clone()).sentence(),
        ));
    }
    // A valid licence of every plan, the dormant one included: ruling 2 says
    // Team is shown nowhere, so its own sentence gets no exemption either.
    for plan in [Plan::Pro, Plan::Team] {
        said.push((
            format!("valid {plan:?}"),
            Verdict::Valid(licence(plan)).sentence(),
        ));
    }
    said.push((
        "what needs a licence".into(),
        Surface::ApproveVersion.needs().to_string(),
    ));
    // Every refusal the one decision can give, with and without a file installed.
    let mut sources: Vec<Vec<Source>> = Vec::new();
    for p in &problems {
        for installed in [true, false] {
            let l = Source::Licence {
                verdict: Verdict::Refused(p.clone()),
                installed,
            };
            sources.push(vec![l]);
        }
    }
    for s in &sources {
        if let thruline_licence::Entitlement::Refused { reason, enable, .. } =
            decide(Surface::ApproveVersion, s)
        {
            said.push(("refusal reason".into(), reason));
            said.push(("refusal enable".into(), enable));
        }
    }
    let mut found = Vec::new();
    for (what, text) in &said {
        for h in hits(text, true, TierRule::Named) {
            found.push(format!("{what}: {h}: {text:?}"));
        }
    }
    assert!(
        found.is_empty(),
        "licence sentences that break the census:\n{}",
        found.join("\n")
    );
}

/* ── 3. every --help ───────────────────────────────────────────────────── */

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A throwaway HOME so a `--help` can write nothing that matters.
struct Home {
    dir: tempfile::TempDir,
}

const SEAM: &str = "THRULINE_LICENCE_DEV_PUBLIC_KEY";
const GATE: &str = "THRULINE_PAID_GATE_DEV";
const NO_BUILT_IN_KEYS: &str = "THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS";

impl Home {
    fn new() -> Home {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo")).expect("repo");
        Home { dir }
    }
    fn thruline_home(&self) -> PathBuf {
        self.dir.path().join("h/.thruline")
    }
    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(bin());
        cmd.args(args)
            .env("HOME", self.dir.path().join("h"))
            .env("THRULINE_HOME", self.thruline_home())
            .env_remove(SEAM)
            .env_remove(GATE)
            .env_remove(NO_BUILT_IN_KEYS)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .env("NO_COLOR", "1")
            .current_dir(self.dir.path().join("repo"))
            .stdin(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().expect("run thruline")
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// The subcommand names a `--help` page lists under `Commands:`.
fn listed(help: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in help.lines() {
        if line.trim_end() == "Commands:" {
            inside = true;
            continue;
        }
        if inside {
            if !line.starts_with("  ") {
                if line.trim().is_empty() {
                    continue;
                }
                break;
            }
            // A command line is indented by exactly two spaces; a wrapped
            // description is indented further.
            if line.starts_with("   ") {
                continue;
            }
            if let Some(name) = line.split_whitespace().next() {
                if name != "help" {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

/// Hidden subcommands are not listed, and print a `--help` all the same.
const HIDDEN: &[&[&str]] = &[&["shim", "exec"]];

#[test]
fn every_help_page_passes_the_census() {
    let home = Home::new();
    let mut queue: Vec<Vec<String>> = vec![vec![]];
    queue.extend(
        HIDDEN
            .iter()
            .map(|h| h.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
    );
    let mut seen = 0usize;
    let mut found = Vec::new();
    while let Some(path) = queue.pop() {
        let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
        args.push("--help");
        let o = home.run(&args, &[]);
        let page = text(&o);
        assert!(
            o.status.success(),
            "`thruline {}` exited {:?}: {page}",
            args.join(" "),
            o.status
        );
        seen += 1;
        for (n, line) in page.lines().enumerate() {
            for h in hits(line, true, TierRule::Named) {
                found.push(format!(
                    "`thruline {}` line {}: {h}: {line:?}",
                    args.join(" "),
                    n + 1
                ));
            }
        }
        for sub in listed(&page) {
            let mut next = path.clone();
            next.push(sub);
            queue.push(next);
        }
    }
    eprintln!("walked {seen} help pages");
    assert!(seen > 60, "walked only {seen} help pages");
    assert!(
        found.is_empty(),
        "help text that breaks the census:\n{}",
        found.join("\n")
    );
}

/* ── 4. the licence status document ────────────────────────────────────── */

struct Issuer {
    signing: SigningKey,
}

impl Issuer {
    fn new() -> Issuer {
        Issuer {
            signing: SigningKey::generate(&mut OsRng),
        }
    }
    fn public_hex(&self) -> String {
        thruline_licence::to_hex(&self.signing.verifying_key().to_bytes())
    }
    fn licence(&self, product: &str, plan: &str, expires: &str) -> String {
        let payload = format!(
            "{{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"{product}\",\"plan\":\"{plan}\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"{expires}\"}}"
        );
        thruline_licence::seal(payload.as_bytes(), &self.signing).expect("seal")
    }
}

/// Every string value in a JSON document, with its path.
fn strings(v: &Value, at: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::String(s) => out.push((at.to_string(), s.clone())),
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                strings(x, &format!("{at}[{i}]"), out);
            }
        }
        Value::Object(m) => {
            for (k, x) in m {
                strings(x, &format!("{at}.{k}"), out);
            }
        }
        _ => {}
    }
}

/// One state: its name, the installed licence's text, whether a v0.1.0 trial
/// file is left behind, the extra environment, and whether Team is installed.
type Case<'a> = (&'a str, Option<String>, bool, Vec<(&'a str, &'a str)>, bool);

#[test]
fn the_licence_status_document_passes_the_census_in_every_state() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let far = "2099-01-01T00:00:00Z";
    let past = "2020-01-01T00:00:00Z";
    // (name, installed licence text, legacy trial file left behind,
    //  extra environment, whether a Team licence is installed)
    let pro = issuer.licence("thruline", "pro", far);
    let team = issuer.licence("thruline", "team", far);
    let cases: Vec<Case> = vec![
        ("nothing installed", None, false, vec![], false),
        ("old trial ignored", None, true, vec![], false),
        ("valid Pro", Some(pro.clone()), false, vec![], false),
        (
            "valid Team (dormant)",
            Some(team.clone()),
            false,
            vec![],
            true,
        ),
        (
            "expired Pro",
            Some(issuer.licence("thruline", "pro", past)),
            true,
            vec![],
            false,
        ),
        (
            "expired Team",
            Some(issuer.licence("thruline", "team", past)),
            false,
            vec![],
            true,
        ),
        (
            "unknown plan",
            Some(issuer.licence("thruline", "enterprise", far)),
            false,
            vec![],
            false,
        ),
        (
            "wrong product",
            Some(issuer.licence("other", "pro", far)),
            false,
            vec![],
            false,
        ),
        (
            "tampered",
            Some(pro.replacen("\"plan\":\"pro\"", "\"plan\":\"prn\"", 1)),
            true,
            vec![],
            false,
        ),
        ("not a licence", Some("hello".into()), false, vec![], false),
        ("gate off", None, false, vec![(GATE, "off")], false),
        (
            "gate off, valid Team",
            Some(team),
            false,
            vec![(GATE, "off")],
            true,
        ),
        ("keyless", None, false, vec![(NO_BUILT_IN_KEYS, "1")], false),
    ];
    let mut found = Vec::new();
    for (name, installed, old_trial, extra, team_installed) in cases {
        let home = Home::new();
        if let Some(l) = &installed {
            std::fs::write(home.thruline_home().join("licence.txt"), l).expect("place licence");
        }
        if old_trial {
            std::fs::write(
                home.thruline_home().join("trial.json"),
                br#"{"format":1,"started_on":"2099-01-01","created_at":"2026-10-07T00:00:00Z"}"#,
            )
            .expect("place old trial");
        }
        let mut env = extra.clone();
        env.push((SEAM, key.as_str()));
        let o = home.run(&["licence", "status", "--json"], &env);
        assert!(o.status.success(), "{name}: {}", text(&o));
        let doc: Value = serde_json::from_slice(&o.stdout)
            .unwrap_or_else(|e| panic!("{name}: not json ({e}): {}", text(&o)));
        assert!(
            doc.get("trial").is_none(),
            "{name}: obsolete trial field: {doc}"
        );
        if name == "old trial ignored" {
            assert_eq!(doc["headline"], "free", "{doc}");
            assert_eq!(doc["entitlements"]["approve_version"]["allowed"], false);
        }
        if name == "expired Pro" {
            assert_eq!(doc["status"], "expired", "{doc}");
            assert_eq!(doc["entitlements"]["approve_version"]["allowed"], false);
        }
        let mut said = Vec::new();
        strings(&doc, "", &mut said);
        assert!(!said.is_empty(), "{name}: an empty document");
        for (at, s) in &said {
            // Reachable only with such a licence installed: the wire name in
            // the `plan` field of an installed Team licence. Nothing else.
            let exempt = team_installed && at == ".plan" && s == "team";
            for h in hits(
                s,
                true,
                if exempt {
                    TierRule::Exempt
                } else {
                    TierRule::Folded
                },
            ) {
                found.push(format!("{name}: {at}: {h}: {s:?}"));
            }
        }
        if team_installed && doc["status"] == "valid" {
            assert_eq!(doc["plan"], "team", "{name}: the exemption is still needed");
        }
        // The one-line form of the same document.
        let o = home.run(&["licence", "status"], &env);
        let line = text(&o);
        for h in hits(line.trim(), true, TierRule::Named) {
            found.push(format!("{name}: `licence status`: {h}: {line:?}"));
        }
    }
    assert!(
        found.is_empty(),
        "status documents that break the census:\n{}",
        found.join("\n")
    );
}
