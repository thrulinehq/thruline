//! The committed product mapping, held to its rules: PROVISIONAL and
//! PLACEHOLDER in its header, real payments only, every id a placeholder, and
//! a bad mapping refused at load with its own reason.

use thruline_licence_issue::config::{Config, MAX_DAYS, PRODUCTION};

#[test]
fn the_production_mapping_is_provisional_placeholder_and_real_payments_only() {
    let header: String = PRODUCTION
        .lines()
        .take_while(|l| l.starts_with('#') || l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(header.contains("PROVISIONAL"), "{header}");
    assert!(header.contains("PLACEHOLDER"), "{header}");
    let c = Config::production().expect("the committed mapping loads");
    assert!(
        !c.accept_test_mode(),
        "production accepts test-mode payments"
    );
    let ids: Vec<_> = c.ids().collect();
    assert!(!ids.is_empty());
    for (provider, id, grant) in ids {
        assert!(id.starts_with("PLACEHOLDER-"), "{provider} {id}");
        assert!(grant.days >= 1 && grant.days <= MAX_DAYS);
    }
    for p in c.providers() {
        assert!(c.ids().any(|(q, _, _)| q == *p), "{p} has no product");
    }
}

#[test]
fn a_bad_mapping_is_refused_at_load_with_its_own_reason() {
    let base = "providers = [\"paddle\"]\naccept_test_mode = false\n";
    let row = |p: &str, id: &str, plan: &str, days: u32| {
        format!(
            "[[product]]\nprovider = \"{p}\"\nid = \"{id}\"\nplan = \"{plan}\"\ndays = {days}\n"
        )
    };
    let cases = [
        (format!("{base}{}", row("paddle", "a", "gold", 365)), "gold"),
        (format!("{base}{}", row("paddle", "a", "pro", 0)), "0 days"),
        (
            format!(
                "{base}{}{}",
                row("paddle", "a", "pro", 365),
                row("paddle", "a", "team", 365)
            ),
            "twice",
        ),
        (
            format!("{base}{}", row("lemonsqueezy", "a", "pro", 365)),
            "not in its providers line",
        ),
        (
            "providers = [\"stripe\"]\naccept_test_mode = false\n".to_string(),
            "stripe",
        ),
        // A default plan is not a setting this file can hold.
        (format!("{base}default_plan = \"pro\"\n"), "expected shape"),
    ];
    for (text, needle) in cases {
        let err = Config::from_toml(&text).expect_err(&text).to_string();
        assert!(err.contains(needle), "{err}");
    }
}

#[test]
fn an_unmapped_id_has_no_grant() {
    let c = Config::production().expect("load");
    assert_eq!(c.grant("paddle", "pri_not_in_the_file"), None);
    assert_eq!(c.grant("paddle", ""), None);
    assert_eq!(c.grant("lemonsqueezy", "50001"), None);
}
