//! Paddle Billing's webhook rules — the whole of them, in this one file.
//!
//! **Written from Paddle's public webhook documentation, not from a recorded
//! request: no account exists, so this is unverified against a live
//! account.** The pages it follows (cited, not fetched by the loop):
//! <https://developer.paddle.com/webhooks/signature-verification>,
//! <https://developer.paddle.com/webhooks/overview>,
//! <https://developer.paddle.com/webhooks/transactions/transaction-completed>,
//! <https://developer.paddle.com/api-reference/transactions/overview>.
//!
//! - **Signature.** Header `Paddle-Signature: ts=<unix seconds>;h1=<hex>`. The
//!   signed payload is `<ts>:<raw body>`; `h1` is its lowercase hex
//!   HMAC-SHA256 keyed with the notification destination's secret. During a
//!   secret rotation more than one `h1` may be sent; any one matching passes.
//! - **Timestamp tolerance.** `ts` must be within [`TOLERANCE_SECS`] of `now`
//!   either way, so a captured request cannot be replayed later with a valid
//!   signature. (Replays inside the window are the store's job.)
//! - **Event.** `event_type`; only `transaction.completed` is a payment.
//! - **Ids.** The event is `event_id` (`evt_…`), the order is `data.id`
//!   (`txn_…`), and the mapping keys on the first item's **price**,
//!   `data.items[0].price.id` (`pri_…`).
//! - **Test mode.** Paddle keeps sandbox and live as separate environments,
//!   and its payload carries no test-mode flag, so this adapter reports
//!   `test_mode = false` and the issuer marks every event verified with a
//!   *sandbox* secret as test mode ([`crate::WebhookSecret::sandbox`]).
//! - **Machine.** `data.custom_data.machine`, passed through checkout; absent
//!   means an unbound licence.

use serde_json::Value;

use crate::{hmac_hex, hmac_matches, Event, Headers, PaidOrder, Provider, Refusal};

/// The signature header.
pub const SIGNATURE_HEADER: &str = "Paddle-Signature";

/// How far `ts` may be from `now`, either way, in seconds.
pub const TOLERANCE_SECS: i64 = 300;

/// The one event that is a payment.
pub const PAID_EVENT: &str = "transaction.completed";

pub struct Paddle;

impl Provider for Paddle {
    fn name(&self) -> &'static str {
        "paddle"
    }

    fn verify(
        &self,
        secret: &[u8],
        headers: &Headers,
        body: &[u8],
        now: i64,
    ) -> Result<(), Refusal> {
        let header = headers
            .get(SIGNATURE_HEADER)
            .ok_or(Refusal::MissingSignature(SIGNATURE_HEADER))?;
        let mut ts: Option<&str> = None;
        let mut h1: Vec<&str> = Vec::new();
        for part in header.split(';') {
            match part.trim().split_once('=') {
                Some(("ts", v)) if ts.is_none() => ts = Some(v),
                Some(("h1", v)) => h1.push(v),
                _ => {}
            }
        }
        let ts = ts.ok_or(Refusal::BadSignature)?;
        let at: i64 = ts.parse().map_err(|_| Refusal::BadSignature)?;
        if (now - at).abs() > TOLERANCE_SECS {
            return Err(Refusal::StaleSignature(TOLERANCE_SECS));
        }
        let signed: [&[u8]; 3] = [ts.as_bytes(), b":", body];
        if h1.iter().any(|tag| hmac_matches(secret, &signed, tag)) {
            Ok(())
        } else {
            Err(Refusal::BadSignature)
        }
    }

    fn parse(&self, body: &[u8]) -> Result<Event, Refusal> {
        let v: Value = serde_json::from_slice(body)
            .map_err(|_| Refusal::Malformed("the body is not JSON".into()))?;
        let event_type = v
            .get("event_type")
            .and_then(Value::as_str)
            .ok_or_else(|| Refusal::Malformed("no event_type".into()))?;
        if event_type != PAID_EVENT {
            return Ok(Event::NotAPayment {
                event_type: event_type.to_string(),
            });
        }
        let text = |x: Option<&Value>, what: &str| {
            x.and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| Refusal::Malformed(format!("no {what}")))
        };
        let data = v
            .get("data")
            .ok_or_else(|| Refusal::Malformed("no data".into()))?;
        let status = data.get("status").and_then(Value::as_str).unwrap_or("");
        if status != "completed" {
            return Ok(Event::NotAPayment {
                event_type: format!("{event_type} ({status})"),
            });
        }
        let event_id = text(v.get("event_id"), "event_id")?;
        let order_id = text(data.get("id"), "data.id")?;
        let product_id = text(
            data.get("items")
                .and_then(|i| i.get(0))
                .and_then(|i| i.get("price"))
                .and_then(|p| p.get("id")),
            "data.items[0].price.id",
        )?;
        let paid_at = data
            .get("billed_at")
            .or_else(|| v.get("occurred_at"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let machine = data
            .get("custom_data")
            .and_then(|c| c.get("machine"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(Event::Paid(PaidOrder {
            provider: self.name(),
            event_id,
            order_id,
            product_id,
            test_mode: false,
            paid_at,
            machine,
        }))
    }
}

/// The headers Paddle would send for `body` at `ts` under `secret`. For tests
/// and a founder's local dry run; it needs the secret, so it forges nothing.
pub fn sign(secret: &[u8], body: &[u8], ts: i64) -> Headers {
    let ts = ts.to_string();
    let h1 = hmac_hex(secret, &[ts.as_bytes(), b":", body]);
    Headers::new()
        .with("Content-Type", "application/json")
        .with(SIGNATURE_HEADER, &format!("ts={ts};h1={h1}"))
}
