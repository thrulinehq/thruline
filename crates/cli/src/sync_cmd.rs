//! `thruline sync` — §14's verbs, behind a feature that is **off**.
//!
//! §18 asks whether the hosted sync server ships in v1 or stays a
//! feature-flagged reference implementation. The answer given is the second, so
//! this command exists in two forms and the shipped one refuses.
//!
//! # Why the refusal is a command rather than a missing command
//!
//! A verb §14 lists that simply is not there reads as an oversight, and the
//! next person implements it. A verb that refuses and says *why* — the decision,
//! the flag, and the fact that no server exists to sync with — is a decision
//! the reader can see was made. The docs census in
//! `crates/cli/tests/m6_docs_are_true.rs` also requires that a command the book
//! names exists, and this is the honest way to satisfy it.

use std::process::ExitCode;

use anyhow::Result;

pub struct Args {
    pub verb: String,
    pub json: bool,
}

/// The reason, in one place, so the two builds cannot drift apart in what they
/// say.
const NOT_SHIPPED: &str = "\
`thruline sync` is a reference implementation and v1 does not ship it \
(BUILD-SPEC §18). The client half is built and tested in `crates/sync` — \
XChaCha20-Poly1305 under an Argon2id-derived key, with the object id as \
associated data so a server that shuffles objects cannot make a client read \
the wrong one — and **no server exists to sync with**. Build with \
`--features sync` to use the reference client against your own transport.";

#[cfg(not(feature = "sync"))]
pub fn run(args: Args) -> Result<ExitCode> {
    if args.json {
        crate::json_out::print_compact(
            &crate::json_out::SYNC,
            serde_json::json!({
                "ok": false,
                "verb": args.verb,
                "reason": NOT_SHIPPED,
                "feature": "sync",
            }),
        )?;
    } else {
        println!("{NOT_SHIPPED}");
    }
    // 1, not 3: this is not a policy failure, it is a command that cannot do
    // what was asked.
    Ok(ExitCode::from(1))
}

#[cfg(feature = "sync")]
pub fn run(args: Args) -> Result<ExitCode> {
    // Even with the feature on there is no server, so every verb that needs one
    // says so. `status` is the exception: it can report the local half.
    match args.verb.as_str() {
        "status" => {
            let msg = "the sync client is compiled in; no transport is configured, and \
                       this build ships no server (BUILD-SPEC §18)";
            if args.json {
                crate::json_out::print_compact(
                    &crate::json_out::SYNC,
                    serde_json::json!({ "ok": true, "client": "built", "transport": null, "reason": msg }),
                )?;
            } else {
                println!("{msg}");
            }
            Ok(ExitCode::SUCCESS)
        }
        verb => {
            let msg = format!("`{verb}` needs a transport and none is configured. {NOT_SHIPPED}");
            if args.json {
                crate::json_out::print_compact(
                    &crate::json_out::SYNC,
                    serde_json::json!({ "ok": false, "verb": verb, "reason": msg }),
                )?;
            } else {
                println!("{msg}");
            }
            Ok(ExitCode::from(1))
        }
    }
}
