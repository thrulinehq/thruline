/* GENERATED from crates/core/src/voice.rs — do not hand-edit.
   Regenerate: THRULINE_BLESS_VOICE=1 cargo test -p thruline-core --test voice_first_level
   Doctrine is the source (docs/UX-DOCTRINE.md); the Rust table is the authority. */
export default {
  "verdicts": [
    {
      "slug": "verified",
      "word": "proven",
      "sentence": "Proven — the run itself was captured as it happened."
    },
    {
      "slug": "supported",
      "word": "supported",
      "sentence": "Backed by the record."
    },
    {
      "slug": "unsupported",
      "word": "no proof found",
      "sentence": "No proof of this in the record. It may still be true — we just can't show it."
    },
    {
      "slug": "contradicted",
      "word": "contradicted",
      "sentence": "The AI said this was done — the record shows otherwise."
    },
    {
      "slug": "unknown",
      "word": "insufficient evidence",
      "sentence": "There's evidence here, but it couldn't be read well enough to judge."
    },
    {
      "slug": "stale",
      "word": "out of date",
      "sentence": "This was proven — but the project has changed since."
    },
    {
      "slug": "dropped",
      "word": "abandoned",
      "sentence": "Planned, then never touched again."
    },
    {
      "slug": "pending",
      "word": "unfinished",
      "sentence": "Planned — not finished yet."
    },
    {
      "slug": "recorded",
      "word": "noted",
      "sentence": "This happened."
    },
    {
      "slug": "refused",
      "word": "not evaluated",
      "sentence": "Not evaluated — this one sits outside what we check."
    }
  ],
  "currencies": [
    {
      "slug": "current",
      "word": "current",
      "sentence": "Still current."
    },
    {
      "slug": "superseded",
      "word": "replaced",
      "sentence": "Replaced by later work."
    },
    {
      "slug": "out_of_scope",
      "word": "out of scope",
      "sentence": "No longer part of this project."
    }
  ],
  "ledger_states": [
    {
      "slug": "chain_checked",
      "word": "checked",
      "sentence": "Every link in the record was re-checked and adds up."
    },
    {
      "slug": "chain_damaged",
      "word": "damaged",
      "sentence": "The record does not add up — something changed it after it was written."
    },
    {
      "slug": "sources_swept",
      "word": "sources gone",
      "sentence": "Some original files are no longer on disk. The reading survives here; the originals cannot be re-read."
    },
    {
      "slug": "never_checked",
      "word": "unchecked",
      "sentence": "Not re-checked yet."
    }
  ],
  "proof_currentness": [
    {
      "slug": "current",
      "word": "current",
      "sentence": "Current as of the last check."
    },
    {
      "slug": "stale",
      "word": "out of date",
      "sentence": "This was proven — but the project has changed since."
    },
    {
      "slug": "unknown",
      "word": "unclear",
      "sentence": "No later check to compare this against."
    }
  ],
  "abstentions": [
    {
      "slug": "too_unspecific",
      "word": "just a thought",
      "sentence": "Just a passing thought — nothing concrete to track. Nothing to worry about."
    },
    {
      "slug": "uninterpretable_outcome",
      "word": "unreadable",
      "sentence": "Something ran here, but the output couldn't be read."
    }
  ],
  "disclosures": [
    {
      "slug": "contradiction",
      "word": "contradicted",
      "sentence": "The record holds both the claim and a run that disagrees with it."
    },
    {
      "slug": "staleness",
      "word": "out of date",
      "sentence": "A later check compared the project against the state this proof was taken from, and found it changed."
    },
    {
      "slug": "capture_attention",
      "word": "needs a person",
      "sentence": "This is about how work gets recorded on this machine, not about the work itself."
    },
    {
      "slug": "all_clear",
      "word": "all clear",
      "sentence": "Nothing in the record needs a person right now."
    },
    {
      "slug": "expired",
      "word": "no longer current",
      "sentence": "This no longer needs you — what it described has changed since."
    }
  ],
  "claim_stories": [
    {
      "slug": "verified",
      "word": "proven",
      "sentence": "The run that proves this was captured as it happened, and the record holds it."
    },
    {
      "slug": "supported",
      "word": "supported",
      "sentence": "The record holds work in the same turn that matches what was said."
    },
    {
      "slug": "contradicted",
      "word": "contradicted",
      "sentence": "The record holds both the claim and a run that disagrees with it."
    },
    {
      "slug": "stale",
      "word": "out of date",
      "sentence": "A later check compared the project against the state this proof was taken from, and found it changed."
    },
    {
      "slug": "pending",
      "word": "unfinished",
      "sentence": "It was proposed in this run, and no work in the same turn follows it."
    },
    {
      "slug": "recorded",
      "word": "noted",
      "sentence": "Work happened here. Nothing has checked whether it did what was said."
    },
    {
      "slug": "unknown",
      "word": "insufficient evidence",
      "sentence": "Something was recorded here, but the outcome could not be read well enough to judge."
    },
    {
      "slug": "refused",
      "word": "not evaluated",
      "sentence": "This one sits outside what we check, so no judgment was made."
    },
    {
      "slug": "unsupported",
      "word": "no proof found",
      "sentence": "The record holds the statement, and nothing that would show it was done."
    },
    {
      "slug": "dropped",
      "word": "abandoned",
      "sentence": "It was planned, and nothing later in the record touches it again."
    }
  ],
  "readinesses": [
    {
      "slug": "not_available",
      "title": "Assured Capture unavailable",
      "detail": "This source has no admitted local assurance contract."
    },
    {
      "slug": "bounded_only",
      "title": "Bounded observations only",
      "detail": "This source can record explicit observations, but cannot prove the command boundary."
    },
    {
      "slug": "disabled",
      "title": "Assured Capture disabled",
      "detail": "No new commands are captured; historical proof remains."
    },
    {
      "slug": "available_not_installed",
      "title": "Assured Capture available",
      "detail": "Review the exact local hook changes before installation."
    },
    {
      "slug": "degraded_source_version",
      "title": "Source version not qualified",
      "detail": "Earlier proof is preserved; new commands will not be assured."
    },
    {
      "slug": "degraded_configuration_drift",
      "title": "Capture configuration changed",
      "detail": "New assurance is paused until a fresh plan is reviewed."
    },
    {
      "slug": "degraded_key_or_identity",
      "title": "Collector identity unavailable",
      "detail": "Existing signatures remain recorded; new assurance is paused."
    },
    {
      "slug": "installed_unverified",
      "title": "Installation not verified",
      "detail": "Run the isolated source-native self-test before authorizing this project."
    },
    {
      "slug": "degraded_source_pairing",
      "title": "Source pairing incomplete",
      "detail": "A complete source result link was not accepted; new assurance is paused."
    },
    {
      "slug": "unsupported_profile",
      "title": "Command profile not assured",
      "detail": "This command is outside the measured local foreground contract."
    },
    {
      "slug": "capture_error",
      "title": "Capture needs attention",
      "detail": "The latest capture did not complete; historical proof remains."
    },
    {
      "slug": "assured_ready",
      "title": "Assured Capture ready",
      "detail": "Supported Codex commands can create execution-boundary proof for this authorized project."
    }
  ],
  "jargon": [
    "entailment",
    "fidelity",
    "matched pair",
    "matched-pair",
    "nli",
    "deterministic · local",
    "unverified",
    "confidence"
  ],
  "enums": [
    "unsupported",
    "dropped",
    "pending",
    "recorded",
    "refused",
    "unknown",
    "verified",
    "stale"
  ]
}
