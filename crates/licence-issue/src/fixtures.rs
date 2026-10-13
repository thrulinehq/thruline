//! One webhook body per provider, **SYNTHESISED FROM DOCUMENTATION, NOT
//! RECORDED — unverified against a live account.** No provider account
//! exists, so no request could be recorded. Each body follows the shape its
//! provider's public webhook documentation gives (the URLs are in each file's
//! `_provenance` key and in the provider's module comment); every id, name and
//! amount is invented, and every UUID is a `cccccccc-` synthetic one.
//!
//! No signature is committed. Tests sign these bytes at run time with a secret
//! they generate at run time (`lemonsqueezy::sign`, `paddle::sign`).

/// A Lemon Squeezy `order_created` for variant `50001`, status `paid`.
pub const LEMONSQUEEZY_ORDER_CREATED: &str =
    include_str!("../fixtures/lemonsqueezy-order_created.json");

/// A Paddle Billing `transaction.completed` for price
/// `pri_01j8synthesisedteam000001`.
pub const PADDLE_TRANSACTION_COMPLETED: &str =
    include_str!("../fixtures/paddle-transaction_completed.json");

/// The label every fixture carries, in the file and in the report.
pub const PROVENANCE: &str = "SYNTHESISED FROM DOCUMENTATION, NOT RECORDED";

/// A mapping for the fixtures' invented ids — Lemon Squeezy variant `50001`
/// to Pro, Paddle's invented price to Team, one year each — with real
/// payments only, as production has. Tests load it; production never does.
pub const FIXTURE_MAPPING: &str = r#"
providers = ["lemonsqueezy", "paddle"]
accept_test_mode = false

[[product]]
provider = "lemonsqueezy"
id = "50001"
plan = "pro"
days = 365

[[product]]
provider = "paddle"
id = "pri_01j8synthesisedteam000001"
plan = "team"
days = 365
"#;
