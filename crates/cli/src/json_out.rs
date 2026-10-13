//! The `--json` contract: one registry, one envelope, one place to look.
//!
//! # What `schema_version` already meant here, and why this is not a second convention
//!
//! Before this module, `schema_version` appeared 77 times across `crates/`, and
//! **not one of them versioned a CLI output shape.** They are all persistence
//! versions: `capture::CAPSULE_SCHEMA_VERSION` on a capture capsule row,
//! `capture_product::M5R2_PROFILE_SCHEMA_VERSION` on a profile row,
//! `observation::SCHEMA_VERSION` inside a hashed preimage, `evidence_binding::
//! SCHEMA_VERSION`, and `ledger::schema::SCHEMA_VERSION`, which is the SQLite
//! `user_version` that `thruline upgrade` migrates. One more,
//! `mind_grounded::SCHEMA_VERSION`, is a *string* (`"grounded-drafts-v1"`) and
//! is served on the `ui` server's `/mind/status` body.
//!
//! So there was no CLI convention to conflict with — but there was a trap. The
//! `ui` body's `schema_version` is a **string** at the top level of a JSON
//! document. If the CLI's were also a bare `schema_version` with an integer,
//! a consumer that learned the field from one surface would mis-type it on the
//! other. The rule adopted here, and enforced by the census, is:
//!
//! * a CLI `--json` document's top-level `schema_version` is an **integer**,
//!   independent per shape, starting at 1;
//! * the shape is identified by `shape`, a stable dotted id, so a consumer that
//!   logs a document can say what it was without knowing which flag produced it;
//! * versions are **per shape**, not global — 28 surfaces evolve at different
//!   rates, and a global integer would force every consumer to re-verify every
//!   pin whenever any one shape changed.
//!
//! # Why an envelope, and what it cost
//!
//! Four shapes used to print a bare JSON **array**. A top-level field cannot be
//! read out of an array, so `--json` on those commands could not be versioned
//! without wrapping it. That is a breaking change and it is recorded as one in
//! `docs/STATUS.md`; the alternative was to leave the four highest-traffic
//! shapes unversioned, which is the failure this whole exercise exists against.
//!
//! # What is deliberately *not* versioned
//!
//! Two `--json` documents this binary prints are somebody else's schema.
//! Injecting a ThruLine field into one is a defect, not a version:
//!
//! * `thruline receipt --json` prints a **DSSE envelope** whose bytes are
//!   hashed into the `ThruLine-Receipt:` commit trailer on the very next line.
//!   A foreign field changes that hash and breaks the binding it exists to make.
//!   The receipt is already versioned, by in-toto's own mechanism, in
//!   `predicateType`.
//! * `thruline verify-receipt --format check-run` prints a payload posted
//!   verbatim to GitHub's check-runs API.
//!
//! Both are listed in `SHAPES` with `Envelope::External` and a reason, because a
//! surface that is exempt but unlisted is indistinguishable from one that was
//! forgotten. (A third foreign document, `hook-install
//! --managed-settings-snippet`, is a fragment pasted into Claude Code's
//! `settings.json`. It reaches stdout through a different flag, is not a
//! `--json` surface, and is named here only so nobody adds a version to it.)

use anyhow::{bail, Result};
use serde_json::{Map, Value};

/// How a shape's body becomes a versioned document.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Envelope {
    /// The body is already an object; `schema_version` and `shape` are added.
    Object,
    /// The body is an array; it is placed under this key beside the version.
    Items(&'static str),
    /// Somebody else's schema. Printed untouched; see the module docs.
    External,
}

impl Envelope {
    fn as_str(self) -> String {
        match self {
            Envelope::Object => "object".to_string(),
            Envelope::Items(key) => format!("items:{key}"),
            Envelope::External => "external".to_string(),
        }
    }
}

/// One `--json` document this binary can emit.
pub struct Shape {
    /// Stable dotted id, emitted as `shape` in the document itself.
    pub id: &'static str,
    /// The subcommand path exactly as `thruline --help` spells it.
    pub command: &'static str,
    /// `"--json"`, `"--format <name>"`, or `"always"` for a command that has no
    /// text mode at all.
    pub surface: &'static str,
    /// Independent per shape. Starts at 1. Bumping it is a promise to consumers.
    pub version: u64,
    pub envelope: Envelope,
    /// Required, and load-bearing, for `Envelope::External`.
    pub note: &'static str,
}

macro_rules! shapes {
    ($( $konst:ident = ($id:expr, $cmd:expr, $surface:expr, $ver:expr, $env:expr, $note:expr); )*) => {
        $( pub const $konst: Shape = Shape {
            id: $id, command: $cmd, surface: $surface,
            version: $ver, envelope: $env, note: $note,
        }; )*
        /// The registry. `thruline json-shapes` prints it; the census compares
        /// it against clap's own account of which commands take `--json`.
        pub const SHAPES: &[&Shape] = &[ $( &$konst ),* ];
    };
}

shapes! {
    ACCEPTANCE_INIT = ("thruline.acceptance.init", "acceptance init", "--json", 1, Envelope::Object, "unaccepted draft and exact document digest");
    ACCEPTANCE_ACCEPT = ("thruline.acceptance.accept", "acceptance accept", "--json", 1, Envelope::Object, "local acceptance; never a completion result");
    ACCEPTANCE_STATUS = ("thruline.acceptance.status", "acceptance status", "--json", 1, Envelope::Object, "accepted requirements and outstanding checks");
    ACCEPTANCE_RUN = ("thruline.acceptance.run", "acceptance run", "--json", 1, Envelope::Object, "one explicitly authorized local attempt and current requirement evaluation");
    ACCEPTANCE_RECEIPT = ("thruline.acceptance.receipt", "acceptance receipt", "--json", 1, Envelope::Object, "detached v0.2 local acceptance receipt location and identity");
    VERIFY_SESSION_RECEIPT = ("thruline.verify-receipt.session", "verify-receipt", "--json", 2, Envelope::Object, "v0.2 session provenance, integrity and D22 conclusion");
    VERIFY_ACCEPTANCE_RECEIPT = ("thruline.verify-receipt.acceptance", "verify-receipt", "--json", 2, Envelope::Object, "v0.2 acceptance branch; signature and current local evidence are distinct");
    ACCEPTANCE_RESUME = ("thruline.acceptance.resume", "acceptance resume", "--json", 1, Envelope::Object, "explicit local handoff acknowledgement after identity and evidence comparison");
    ACCEPTANCE_HANDOFF = ("thruline.acceptance.handoff", "acceptance handoff", "--json", 1, Envelope::Object, "manual packet; export is not confirmed recovery");
    NOW = ("thruline.now", "now", "--json", 1, Envelope::Items("items"),
        "");
    SHADOW_SESSIONS = ("thruline.shadow-sessions", "shadow-sessions", "--json", 1, Envelope::Items("sessions"),
        "");
    ENTITY_LIST = ("thruline.entity.list", "entity list", "--json", 1, Envelope::Items("records"),
        "");
    VERIFY_CLAIMS = ("thruline.verify-claims", "verify-claims", "--json", 1, Envelope::Items("claims"),
        "");
    VERIFY = ("thruline.verify", "verify", "--json", 1, Envelope::Object,
        "recorded-prefix, accepted sealed-copy and chain integrity; 0 intact, 1 mismatch, 2 unverifiable");
    WHY_RECORD = ("thruline.why.record", "why", "--json", 1, Envelope::Object,
        "the truth-loop rendering of one recorded finding");
    WHY_ATTENTION = ("thruline.why.attention", "why", "--json", 1, Envelope::Object,
        "the attention-queue view of one item");
    EVIDENCE_ONE = ("thruline.evidence.one", "evidence", "--json", 1, Envelope::Object,
        "one binding, named by --id");
    EVIDENCE_LIST = ("thruline.evidence.list", "evidence", "--json", 1, Envelope::Object,
        "every binding for the repository at the working directory");
    DOCTOR = ("thruline.doctor", "doctor", "--json", 2, Envelope::Object,
        "every check doctor ran, and `ok` over them. Version 2 adds `note` to \
         each finding: true for a check that did not apply where doctor ran \
         (the working-tree check outside a git repository), which is reported \
         and not counted against `ok`");
    SESSIONS = ("thruline.sessions", "sessions", "--json", 2, Envelope::Object,
        "the execution store's sessions, ranked; `total` and `returned` are \
         both present because the list is bounded and a bounded list that does \
         not say so implies it showed everything. Version 2 adds limit_scope: \
         store for the default global bound, project for --per-project");
    CLAIMS = ("thruline.claims", "claims", "--json", 1, Envelope::Object, "");
    EXPLAIN = ("thruline.explain", "explain", "--json", 1, Envelope::Object, "");
    PROMISES = ("thruline.promises", "promises", "--json", 1, Envelope::Object, "");
    RECEIPT_DSSE = ("thruline.receipt.dsse", "receipt", "--json", 1, Envelope::External,
        "a DSSE envelope (in-toto Statement, BUILD-SPEC §8.1). Its exact bytes \
         are hashed into the `ThruLine-Receipt:` commit trailer printed beside \
         it, so any added field breaks the binding the receipt exists to make. \
         It is versioned by in-toto's own `predicateType`.");
    VERIFY_RECEIPT = ("thruline.verify-receipt", "verify-receipt", "--json", 1, Envelope::Object, "");
    VERIFY_RECEIPT_CHECK_RUN = ("thruline.verify-receipt.check-run", "verify-receipt", "--format check-run", 1, Envelope::External,
        "GitHub's check-runs API request body, posted verbatim by the Action. \
         The shape belongs to GitHub; a ThruLine field in it is sent to a \
         third party's endpoint and versions nothing we own.");
    REEXEC = ("thruline.reexec", "reexec", "--json", 1, Envelope::Object, "");
    SHIM_INSTALL = ("thruline.shim.install", "shim install", "--json", 1, Envelope::Object, "");
    SHIM_UNINSTALL = ("thruline.shim.uninstall", "shim uninstall", "--json", 1, Envelope::Object, "");
    SHIM_STATUS = ("thruline.shim.status", "shim status", "--json", 1, Envelope::Object, "");
    FOLD = ("thruline.fold", "fold", "--json", 1, Envelope::Object, "");
    RUN = ("thruline.run", "run", "--json", 1, Envelope::Object, "");
    IMPORT_CODEX = ("thruline.import-codex", "import-codex", "--json", 1, Envelope::Object, "");
    IMPORT_CODEX_DRY_RUN = ("thruline.import-codex.dry-run", "import-codex", "--json", 1, Envelope::Object,
        "`--dry-run`: a survey of first lines, which never touches the store");
    IMPORT_LEGACY = ("thruline.import-legacy", "import-legacy", "--json", 1, Envelope::Object, "");
    SYNC = ("thruline.sync", "sync", "--json", 1, Envelope::Object, "");
    POLICY_INIT = ("thruline.policy.init", "policy", "--json", 1, Envelope::Object,
        "`policy --init`: whether a template was written");
    POLICY_CHECK = ("thruline.policy.check", "policy", "--json", 1, Envelope::Object,
        "the evaluated conclusion, including the no-policy-is-not-a-failure case");
    BENCH = ("thruline.bench", "bench", "--json", 1, Envelope::Object, "");
    DEMO = ("thruline.demo", "demo", "--json", 1, Envelope::Object, "");
    HOOK_INSTALL = ("thruline.hook-install", "hook-install", "--json", 1, Envelope::Object, "");
    HOOK_INSTALL_REFUSED = ("thruline.hook-install.refused", "hook-install", "--json", 1, Envelope::Object,
        "the refusal when another ThruLine hook pack already owns the settings file");
    CAPTURE_INSTALL = ("thruline.capture.install", "capture install", "--json", 1, Envelope::Object, "");
    CAPTURE_STATUS = ("thruline.capture.status", "capture status", "--json", 1, Envelope::Object, "");
    CAPTURE_DOCTOR = ("thruline.capture.doctor", "capture doctor", "--json", 1, Envelope::Object, "");
    CHANGE_NEW = ("thruline.change.new", "change new", "--json", 1, Envelope::Object,
        "one Change, opened: the id it will be known by, the project it is \
         about, what was asked for and who asked. Nothing about what counts as \
         done, because opening a Change decides none of it");
    CHANGE_SUGGEST = ("thruline.change.suggest", "change suggest", "--json", 1, Envelope::Object,
        "proposed session links with the basis and the evidence for each, plus \
         what the suggester could not decide: ties, undated sessions and scope \
         lines it does not understand. Nothing here is a confirmed link");
    CHANGE_ASSOCIATE = ("thruline.change.associate", "change associate", "--json", 1, Envelope::Object,
        "one person's decision about one link, appended beside the suggestion \
         it answers; never an edit of it");
    CHANGE_RESULTS = ("thruline.change.results", "change results", "--json", 1, Envelope::Object,
        "every session linked to a Change with either its recorded verdict \
         counts or the reason they are not counted here. A session that fails \
         the scope gate is listed with a sentence and no numbers");
    CHANGE_LINKS = ("thruline.change.links", "change links", "--json", 1, Envelope::Object,
        "one session's side of the same fact, including the case where it \
         belongs to no Change at all");
    CHANGE_CANCEL_RUN = ("thruline.change.cancel-run", "change cancel-run", "--json", 1, Envelope::Object,
        "what became of one running check after a person asked for it to stop: \
         whether it was cancelled at all, who asked, and what was found when \
         its process group was looked at afterwards. \"Nothing looked\" is its \
         own answer and is never reported as a clean stop");
    CHANGE_WAIVE = ("thruline.change.waive", "change waive", "--json", 1, Envelope::Object,
        "what became of one request to set an obligation aside: whether it was \
         recorded at all, and when it was, who set it aside, the accepted \
         policy's authority it rests on and the reason they gave. A refusal is \
         reported as a refusal and exits non-zero; nothing here is ever a \
         result about the obligation");
    CHANGE_FINDING = ("thruline.change.finding", "change finding", "--json", 1, Envelope::Object,
        "one person's (or one labelled synthetic import's) change to where one \
         thing a review found stands: the appended row's from and to, or the \
         refusal code, the parts that were missing, and the sentence the \
         screen would show. A resolved mark never reads as fixed");
    CHANGE_PREPARE = ("thruline.change.prepare", "change prepare", "--json", 1, Envelope::Object,
        "what exactly will be examined for one Change: the project folder, the \
         starting point, whether unsaved changes are included, the tools that \
         would run with their versions, and the running application this Change \
         is about. Every field a probe could not answer is unknown, and unknown \
         is never a value");
    CHANGE_PROPOSE = ("thruline.change.propose", "change propose", "--json", 1, Envelope::Object,
        "proposed criteria cut from the request and the confirmed work, each \
         with the request it came from and the reason it is here; the sentences \
         that could not be cut into three parts, each naming the part it was \
         missing; and a suggested check profile in which an unavailable check is \
         still listed. Nothing here is accepted and nothing here has run");
    CHANGE_ACCEPT = ("thruline.change.accept", "change accept", "--json", 1, Envelope::Object,
        "one person writing down what counts as done: a new scope version, its \
         acceptance record, and whose words each line is in. Accepting is not \
         approving, and the document says so in both registers");
    DECIDE_LIST = ("thruline.decide.list", "decide list", "--json", 1, Envelope::Object,
        "stored proposed, verified accepted, and retired decisions for one project");
    DECIDE_PROPOSE = ("thruline.decide.propose", "decide propose", "--json", 1, Envelope::Object,
        "source-anchored proposals from one recorded person event; no acceptance");
    DECIDE_RULES = ("thruline.decide.rules", "decide rules", "--json", 1, Envelope::Object,
        "exact-version tracked rule proposals and parameter notes; no person authority");
    DECIDE_REVISE_PARAMS = ("thruline.decide.revise-params", "decide revise-params", "--json", 1, Envelope::Object,
        "exact-parent complete parameter proposal with verified native sources; no person act");
    DECIDE_ACCEPT = ("thruline.decide.accept", "decide accept", "--json", 1, Envelope::Object,
        "one exact recorded person acceptance");
    DECIDE_RETIRE = ("thruline.decide.retire", "decide retire", "--json", 1, Envelope::Object,
        "one exact recorded person retirement");
    DECIDE_CONFIRM = ("thruline.decide.confirm", "decide confirm", "--json", 1, Envelope::Object,
        "bridge receipt identity and verified derived copy after an independent person confirmation");
    DECIDE_PREPARE = ("thruline.decide.prepare", "decide prepare", "--json", 1, Envelope::Object,
        "one inert action row awaiting independent person confirmation");
    DECIDE_REPAIR_DERIVED = ("thruline.decide.repair-derived", "decide repair-derived", "--json", 1, Envelope::Object,
        "rebuilt and verified derived decision copy; no new person confirmation");
    DECIDE_DEFAULTS = ("thruline.decide.defaults", "decide defaults", "--json", 1, Envelope::Object,
        "eighteen advisory Family K proposals with explicit checker coverage");
    DECIDE_CHECK = ("thruline.decide.check", "decide check", "--json", 1, Envelope::Object,
        "bounded exact-version decision verdicts and their typed findings");
    DECIDE_INLINE = ("thruline.decide.inline", "decide inline", "--json", 1, Envelope::Object,
        "bounded read-only exact-version cheap sweep; no verdict or person act is appended");
    DECIDE_GATE = ("thruline.decide.gate", "decide gate", "--json", 1, Envelope::Object,
        "project policy conclusion and exact-version receipt reasons");
    LICENCE_STATUS = ("thruline.licence.status", "licence status", "--json", 4, Envelope::Object,
        "whether this machine has a licence that works, in one sentence, with \
         what would fix it and the one paid surface. Since v4, an unlicensed \
         machine reports Free, and only a valid Pro licence permits approving \
         a version. The former time-limited evaluation field is absent. The \
         same document `GET /api/licence` serves. It carries no fingerprint");
    JSON_SHAPES = ("thruline.json-shapes", "json-shapes", "always", 1, Envelope::Object,
        "the registry listing itself");
}

/// Wrap `body` in this shape's envelope.
///
/// Fails loudly rather than emitting an unversioned document: a body that does
/// not match its declared envelope is a programming error, and the one thing it
/// must never do is fall back to printing the bare body.
pub fn envelope(shape: &Shape, body: Value) -> Result<Value> {
    let mut out = Map::new();
    match shape.envelope {
        Envelope::External => bail!(
            "{} is an external schema and must be printed untouched",
            shape.id
        ),
        Envelope::Items(key) => {
            if !body.is_array() {
                bail!(
                    "{} declares a collection envelope but its body is not an array",
                    shape.id
                );
            }
            out.insert("schema_version".into(), Value::from(shape.version));
            out.insert("shape".into(), Value::from(shape.id));
            out.insert(key.to_string(), body);
        }
        Envelope::Object => {
            let Value::Object(fields) = body else {
                bail!(
                    "{} declares an object envelope but its body is not an object",
                    shape.id
                );
            };
            for reserved in ["schema_version", "shape"] {
                if fields.contains_key(reserved) {
                    bail!(
                        "{} already carries a `{reserved}` field of its own",
                        shape.id
                    );
                }
            }
            out.insert("schema_version".into(), Value::from(shape.version));
            out.insert("shape".into(), Value::from(shape.id));
            out.extend(fields);
        }
    }
    Ok(Value::Object(out))
}

/// Pretty-printed, the way most of these surfaces already printed.
pub fn print(shape: &Shape, body: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&envelope(shape, body)?)?);
    Ok(())
}

/// Single-line, for the surfaces that already printed compact.
pub fn print_compact(shape: &Shape, body: Value) -> Result<()> {
    println!("{}", serde_json::to_string(&envelope(shape, body)?)?);
    Ok(())
}

/// `thruline json-shapes` — the contract, readable by the teams that pin it.
pub fn registry_document() -> Result<Value> {
    let shapes: Vec<Value> = SHAPES
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "command": s.command,
                "surface": s.surface,
                "version": s.version,
                "envelope": s.envelope.as_str(),
                "note": s.note,
            })
        })
        .collect();
    envelope(
        &JSON_SHAPES,
        serde_json::json!({
            "thruline_version": env!("CARGO_PKG_VERSION"),
            "count": shapes.len(),
            "shapes": shapes,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_object_body_keeps_its_fields_and_gains_exactly_two() {
        let out = envelope(&DOCTOR, serde_json::json!({ "findings": [], "ok": true })).unwrap();
        let obj = out.as_object().unwrap();
        assert_eq!(obj["schema_version"], 2);
        assert_eq!(obj["shape"], "thruline.doctor");
        assert_eq!(obj["ok"], true);
        assert_eq!(obj.len(), 4);
    }

    #[test]
    fn an_array_body_is_placed_under_its_declared_key() {
        let out = envelope(&NOW, serde_json::json!([1, 2])).unwrap();
        assert_eq!(out["items"], serde_json::json!([1, 2]));
        assert_eq!(out["schema_version"], 1);
    }

    #[test]
    fn a_body_that_contradicts_its_envelope_refuses_rather_than_printing_unversioned() {
        assert!(envelope(&NOW, serde_json::json!({})).is_err());
        assert!(envelope(&DOCTOR, serde_json::json!([])).is_err());
        assert!(envelope(&RECEIPT_DSSE, serde_json::json!({})).is_err());
    }

    #[test]
    fn a_body_that_already_has_the_reserved_names_refuses_instead_of_shadowing_one() {
        let clash = serde_json::json!({ "schema_version": "grounded-drafts-v1" });
        assert!(envelope(&DOCTOR, clash).is_err());
    }
}
