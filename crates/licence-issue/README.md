# thruline-licence-issue (T-DIST-6)

Turns a **verified** payment webhook into a ThruLine licence file in T-DIST-4's format (`crates/licence/FORMAT.md`). Prepared, not deployed: no provider account exists, nothing is registered with a provider, and nothing here has seen a real request.

This crate opens no socket and names no network crate (`tests/no_network.rs`). The HTTP shell that receives webhooks is `tools/licence-issuer/`, excluded from the workspace like `services/github-app`.

## Choosing a provider: one file

The founder has not chosen between Lemon Squeezy and Paddle Billing. **Choosing is a change to `products.toml` only**: keep the chosen name on its `providers` line, delete the other name and its `[[product]]` rows, and replace the placeholder ids with the real ones (Lemon Squeezy: the variant id; Paddle: the price id). A provider not on that line is refused at `/webhook/<name>`.

Each provider's webhook rules live in exactly one file:

| Provider | File | Signature |
|---|---|---|
| Lemon Squeezy | `src/lemonsqueezy.rs` | `X-Signature`: hex HMAC-SHA256 of the raw body |
| Paddle Billing | `src/paddle.rs` | `Paddle-Signature: ts=…;h1=…`: hex HMAC-SHA256 of `ts:body`, `ts` within 300 s |

Adding a third provider is one new file beside those two and one line in `PROVIDERS` (`src/lib.rs`).

## The rules it keeps

- The signature is checked over the raw body **before** anything in the body is read (`Provider::receive`).
- A product id not in `products.toml` is **refused, never defaulted** to a plan.
- `accept_test_mode = false` (the committed setting) refuses test-mode payments: a Lemon Squeezy event marked test mode, and any event verified with a Paddle sandbox secret.
- The replay store (`src/store.rs`, one SQLite file, append-only) keys on (provider, event id) **and** (provider, order id). A replayed event, or a second event for an order already paid, gets the byte-identical first licence; no second `licence_id` is minted. A refused event writes nothing.
- A licence is tied to a machine only when the buyer passes a fingerprint (`thruline licence fingerprint`) through checkout's custom data as `machine`. Otherwise it is not machine-bound.

## PROVISIONAL

Plans (`pro`, `team`) and the one-year durations in `products.toml` are T-DIST-4's provisional set. The founder has not ruled plans, prices or terms.

## Fixtures

`fixtures/*.json` are **SYNTHESISED FROM DOCUMENTATION, NOT RECORDED — unverified against a live account.** Tests sign them at run time with a secret generated at run time. No secret, real or synthetic, is committed.
