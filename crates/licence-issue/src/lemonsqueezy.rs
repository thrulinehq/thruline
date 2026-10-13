//! Lemon Squeezy's webhook rules — the whole of them, in this one file.
//!
//! **Written from Lemon Squeezy's public webhook documentation, not from a
//! recorded request: no account exists, so this is unverified against a live
//! account.** The pages it follows (cited, not fetched by the loop):
//! <https://docs.lemonsqueezy.com/help/webhooks>,
//! <https://docs.lemonsqueezy.com/help/webhooks/signing-requests>,
//! <https://docs.lemonsqueezy.com/help/webhooks/event-types>,
//! <https://docs.lemonsqueezy.com/api/orders/the-order-object>.
//!
//! - **Signature.** Header `X-Signature`: the lowercase hex HMAC-SHA256 of the
//!   raw request body, keyed with the webhook's signing secret.
//! - **Event.** `meta.event_name`; only `order_created` whose order
//!   `data.attributes.status` is `paid` is a payment. Every other event is
//!   [`Event::NotAPayment`].
//! - **Ids.** The order is `data.id` (a JSON:API resource id, a string). The
//!   mapping keys on the **variant**, `data.attributes.first_order_item.variant_id`
//!   (a number in the documented payload; a string is taken as-is too).
//! - **Event id.** `meta.webhook_id` when present. Lemon Squeezy's documented
//!   payload is not certain to carry a per-delivery id, so without one the
//!   event id is `sha256:` + the hex SHA-256 of the raw body: a replay of the
//!   same bytes is the same event, and a different event for the same order is
//!   still caught by the order key.
//! - **Test mode.** `meta.test_mode` or `data.attributes.test_mode`, either true.
//! - **Machine.** `meta.custom_data.machine`, the fingerprint a buyer may pass
//!   through checkout's custom data; absent means an unbound licence.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{hmac_hex, hmac_matches, Event, Headers, PaidOrder, Provider, Refusal};

/// The signature header.
pub const SIGNATURE_HEADER: &str = "X-Signature";

/// The one event that is a payment.
pub const PAID_EVENT: &str = "order_created";

pub struct LemonSqueezy;

impl Provider for LemonSqueezy {
    fn name(&self) -> &'static str {
        "lemonsqueezy"
    }

    fn verify(
        &self,
        secret: &[u8],
        headers: &Headers,
        body: &[u8],
        _now: i64,
    ) -> Result<(), Refusal> {
        let tag = headers
            .get(SIGNATURE_HEADER)
            .ok_or(Refusal::MissingSignature(SIGNATURE_HEADER))?;
        if hmac_matches(secret, &[body], tag) {
            Ok(())
        } else {
            Err(Refusal::BadSignature)
        }
    }

    fn parse(&self, body: &[u8]) -> Result<Event, Refusal> {
        let v: Value = serde_json::from_slice(body)
            .map_err(|_| Refusal::Malformed("the body is not JSON".into()))?;
        let meta = v
            .get("meta")
            .ok_or_else(|| Refusal::Malformed("no meta".into()))?;
        let event_name = meta
            .get("event_name")
            .and_then(Value::as_str)
            .ok_or_else(|| Refusal::Malformed("no meta.event_name".into()))?;
        if event_name != PAID_EVENT {
            return Ok(Event::NotAPayment {
                event_type: event_name.to_string(),
            });
        }
        let data = v
            .get("data")
            .ok_or_else(|| Refusal::Malformed("no data".into()))?;
        let attributes = data
            .get("attributes")
            .ok_or_else(|| Refusal::Malformed("no data.attributes".into()))?;
        let status = attributes
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");
        if status != "paid" {
            return Ok(Event::NotAPayment {
                event_type: format!("{event_name} ({status})"),
            });
        }
        let order_id =
            id_text(data.get("id")).ok_or_else(|| Refusal::Malformed("no data.id".into()))?;
        let product_id = id_text(
            attributes
                .get("first_order_item")
                .and_then(|i| i.get("variant_id")),
        )
        .ok_or_else(|| {
            Refusal::Malformed("no data.attributes.first_order_item.variant_id".into())
        })?;
        let event_id = match meta.get("webhook_id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => format!("sha256:{}", thruline_licence::to_hex(&Sha256::digest(body))),
        };
        let flag = |x: &Value| x.get("test_mode").and_then(Value::as_bool) == Some(true);
        let machine = meta
            .get("custom_data")
            .and_then(|c| c.get("machine"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(Event::Paid(PaidOrder {
            provider: self.name(),
            event_id,
            order_id,
            product_id,
            test_mode: flag(meta) || flag(attributes),
            paid_at: attributes
                .get("created_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            machine,
        }))
    }
}

/// A JSON id, number or string, as text.
fn id_text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The headers Lemon Squeezy would send for `body` under `secret`. For tests
/// and a founder's local dry run; it needs the secret, so it forges nothing.
pub fn sign(secret: &[u8], body: &[u8]) -> Headers {
    Headers::new()
        .with("Content-Type", "application/json")
        .with("X-Event-Name", PAID_EVENT)
        .with(SIGNATURE_HEADER, &hmac_hex(secret, &[body]))
}
