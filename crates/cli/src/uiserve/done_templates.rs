//! HQ Next — `GET /api/hq/projects/<id>/done-templates`: the templates for
//! what counts as done that this project's `.thruline/policy.yml` holds (C30
//! R-ROUTES "Done-Criteria: templates from policy.yml"). Read-only.
//!
//! The policy bytes come from the project's base branch, never its working
//! tree: a Change cannot loosen its own template. The remote's symbolic
//! default branch wins; without one, exactly one local `main` or `master`
//! must exist. An ambiguous or missing base is reported as unreadable. The
//! bytes are parsed by the same closed-schema reader as `thruline policy`.
//!
//! A Git symlink or anything but a regular blob is not followed, and a file over
//! [`MAX_POLICY_BYTES`] is not read. Git is invoked only for reads, with lazy
//! fetch disabled. No file or record is written; every string served has been
//! through `redact::mask_recorded`.

use super::*;

use std::process::{Command, Output};
use thruline_core::hq as core_hq;
use thruline_core::ledger::Ledger;
use thruline_core::redact::{mask_recorded, MaskRecord};

/// The shape of this document. Raised when a field changes meaning.
pub(super) const SCHEMA_VERSION: u32 = 1;

/// Where the file is, relative to the project's folder.
const POLICY_PATH: &str = ".thruline/policy.yml";

/// The largest policy file this route reads. Far above any file the closed
/// schema can describe; a larger one is said, not read.
const MAX_POLICY_BYTES: u64 = 256 * 1024;

pub(super) fn owns(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/hq/projects/") else {
        return false;
    };
    matches!(
        rest.split('/').collect::<Vec<_>>().as_slice(),
        [_, "done-templates"]
    )
}

fn json<T: Serialize>(stream: &mut TcpStream, status: u16, value: &T) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(value)?,
    )
}

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    json(stream, status, &serde_json::json!({ "error": error }))
}

#[derive(Serialize)]
struct DoneTemplates {
    schema_version: u32,
    project: ProjectRef,
    policy: PolicyFile,
    /// Each template's name and its lines, sorted by name.
    templates: Vec<Template>,
    /// Why the policy supplies no templates: `no_policy_file`,
    /// `no_templates_section` or `policy_not_read`. `null` when it supplies
    /// templates, even if presentation later withholds them by exclusion.
    reason: Option<&'static str>,
    masking: MaskRecord,
}

#[derive(Serialize)]
struct ProjectRef {
    id: String,
}

#[derive(Serialize)]
struct PolicyFile {
    /// Relative to the project's folder.
    path: &'static str,
    /// `absent`, `read`, `unreadable`, `not_a_regular_file` or `too_large`.
    state: &'static str,
    /// Why it could not be read, masked; `null` otherwise.
    detail: Option<String>,
}

#[derive(Serialize)]
struct Template {
    name: String,
    lines: Vec<String>,
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    path: &str,
    query: &str,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    if method != "GET" {
        return refuse(stream, 405, "method_not_allowed");
    }
    let id = path
        .strip_prefix("/api/hq/projects/")
        .and_then(|rest| rest.strip_suffix("/done-templates"))
        .and_then(percent_decode)
        .filter(|id| !id.is_empty() && !id.contains('/'));
    let Some(id) = id else {
        return refuse(stream, 400, "malformed_request");
    };
    let ledger = match Ledger::open(&state.db) {
        Ok(l) => l,
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            return super::store_held(state, stream)
        }
        Err(problem) => {
            return json(
                stream,
                503,
                &serde_json::json!({"error": "store_unavailable", "detail": problem.to_string()}),
            )
        }
    };
    let project = match core_hq::find_project(&ledger, &id) {
        Ok(Some(p)) => p,
        Ok(None) => return refuse(stream, 404, "no_such_project"),
        Err(e) => {
            return json(
                stream,
                500,
                &serde_json::json!({"error": "read_failed", "detail": e.to_string()}),
            )
        }
    };
    let exclusion_policy = match crate::exclusion_serve::policy(&ledger, &project.root) {
        Ok(policy) => policy,
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    drop(ledger);
    let mut masking = MaskRecord::default();
    let (policy, templates, reason) = read_policy(Path::new(&project.root), &mut masking);
    let doc = DoneTemplates {
        schema_version: SCHEMA_VERSION,
        project: ProjectRef { id: project.id },
        policy,
        templates,
        reason,
        masking,
    };
    let raw = serde_json::to_vec(&doc)?;
    match present_templates(&raw, &exclusion_policy) {
        Ok(served) => respond(stream, 200, "application/json; charset=utf-8", &served),
        Err(_) => refuse(stream, 503, "exclusion_policy_unavailable"),
    }
}

fn present_templates<'a>(
    raw: &'a [u8],
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<std::borrow::Cow<'a, [u8]>> {
    if policy.is_empty() {
        return Ok(std::borrow::Cow::Borrowed(raw));
    }
    let mut doc: serde_json::Value = serde_json::from_slice(raw)?;
    if crate::exclusion_serve::checked_apply(&mut doc, policy)? == 0 {
        return Ok(std::borrow::Cow::Borrowed(raw));
    }
    // The policy's fixed source path becomes a marker only when that source
    // is excluded. Its templates remain source content even when their words
    // do not mention the policy path. Keep the factual read state and reason.
    let policy_excluded = is_exclusion_marker(&doc["policy"]["path"]);
    if let Some(templates) = doc
        .get_mut("templates")
        .and_then(serde_json::Value::as_array_mut)
    {
        if policy_excluded {
            templates.clear();
        } else {
            for template in templates {
                if let Some(name) = template.get_mut("name") {
                    hide_template_exclusion_key(name);
                }
                if let Some(lines) = template
                    .get_mut("lines")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for line in lines {
                        hide_template_exclusion_key(line);
                    }
                }
            }
        }
    }
    Ok(std::borrow::Cow::Owned(serde_json::to_vec(&doc)?))
}

fn is_exclusion_marker(value: &serde_json::Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.starts_with("left out by your exclusion (`") && text.ends_with("`)")
    })
}

fn hide_template_exclusion_key(value: &mut serde_json::Value) {
    // A template name or line is content, so its replacement must not repeat
    // the excluded path. Other fields retain the shared marker contract.
    if is_exclusion_marker(value) {
        *value = serde_json::Value::String("left out by your exclusion".into());
    }
}

/// One Git command with user/system configuration and lazy fetch disabled.
/// Every argument is chosen here; no repository text becomes an option.
fn git(root: &Path, args: &[&str]) -> std::result::Result<Output, &'static str> {
    thruline_core::git_isolation::isolate_config(&mut Command::new("git"))
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|_| "Git could not read the base branch")
}

fn commit_at(root: &Path, reference: &str) -> Option<String> {
    let out = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (sha.len() == 40 || sha.len() == 64).then_some(sha)
}

fn base_commit(root: &Path) -> std::result::Result<String, &'static str> {
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if !top.status.success() {
        return Err("This folder is not a Git repository");
    }
    let top =
        String::from_utf8(top.stdout).map_err(|_| "Git returned an invalid repository path")?;
    let actual = Path::new(top.trim())
        .canonicalize()
        .map_err(|_| "Git returned an unreadable repository path")?;
    let expected = root
        .canonicalize()
        .map_err(|_| "The project folder could not be read")?;
    if actual != expected {
        return Err("The project folder is not the Git repository root");
    }
    let remote = git(
        root,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    )?;
    if remote.status.success() {
        let reference =
            String::from_utf8(remote.stdout).map_err(|_| "Git returned an invalid base branch")?;
        return commit_at(root, reference.trim()).ok_or("The remote default branch is unavailable");
    }
    let main = commit_at(root, "refs/heads/main");
    let master = commit_at(root, "refs/heads/master");
    match (main, master) {
        (Some(commit), None) | (None, Some(commit)) => Ok(commit),
        _ => Err("The base branch is missing or ambiguous"),
    }
}

enum BaseFile {
    Absent,
    Text(String),
    NotRegular,
    TooLarge,
}

fn base_policy(root: &Path) -> std::result::Result<BaseFile, &'static str> {
    let commit = base_commit(root)?;
    let tree = git(root, &["ls-tree", &commit, "--", POLICY_PATH])?;
    if !tree.status.success() {
        return Err("Git could not inspect the base policy");
    }
    if tree.stdout.is_empty() {
        return Ok(BaseFile::Absent);
    }
    let listed =
        String::from_utf8(tree.stdout).map_err(|_| "Git returned an invalid policy entry")?;
    let (object, path) = listed
        .trim_end()
        .split_once('\t')
        .ok_or("Git returned an invalid policy entry")?;
    if path != POLICY_PATH {
        return Err("Git returned an unexpected policy entry");
    }
    let fields: Vec<&str> = object.split_whitespace().collect();
    if fields.len() != 3 || fields[1] != "blob" || !fields[2].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Git returned an invalid policy object");
    }
    if fields[0] != "100644" && fields[0] != "100755" {
        return Ok(BaseFile::NotRegular);
    }
    let size = git(root, &["cat-file", "-s", fields[2]])?;
    if !size.status.success() {
        return Err("Git could not size the base policy");
    }
    let size = String::from_utf8(size.stdout).map_err(|_| "Git returned an invalid policy size")?;
    let size: u64 = size
        .trim()
        .parse()
        .map_err(|_| "Git returned an invalid policy size")?;
    if size > MAX_POLICY_BYTES {
        return Ok(BaseFile::TooLarge);
    }
    let content = git(root, &["cat-file", "blob", fields[2]])?;
    if !content.status.success() || content.stdout.len() as u64 != size {
        return Err("Git could not read the base policy");
    }
    let text =
        String::from_utf8(content.stdout).map_err(|_| "The base policy is not UTF-8 text")?;
    Ok(BaseFile::Text(text))
}

/// The policy file's state, templates and reason when none can be served.
fn read_policy(
    root: &Path,
    masking: &mut MaskRecord,
) -> (PolicyFile, Vec<Template>, Option<&'static str>) {
    let mut said = |state: &'static str, detail: Option<String>, reason: &'static str| {
        let detail = detail.map(|d| {
            let (out, rec) = mask_recorded(&d);
            masking.merge(&rec);
            out
        });
        (
            PolicyFile {
                path: POLICY_PATH,
                state,
                detail,
            },
            Vec::new(),
            Some(reason),
        )
    };
    let text = match base_policy(root) {
        Ok(BaseFile::Text(text)) => text,
        Ok(BaseFile::Absent) => return said("absent", None, "no_policy_file"),
        Ok(BaseFile::NotRegular) => {
            return said(
                "not_a_regular_file",
                Some("the base branch entry is a link or another non-regular file".to_string()),
                "policy_not_read",
            )
        }
        Ok(BaseFile::TooLarge) => {
            return said(
                "too_large",
                Some(format!("it is larger than {} KiB", MAX_POLICY_BYTES / 1024)),
                "policy_not_read",
            )
        }
        Err(problem) => return said("unreadable", Some(problem.to_string()), "policy_not_read"),
    };
    match thruline_policy::Policy::parse(&text) {
        Ok(parsed) if parsed.templates.is_empty() => said("read", None, "no_templates_section"),
        Ok(parsed) => {
            let templates = parsed
                .templates
                .into_iter()
                .map(|(name, t)| {
                    let (name, record) = mask_recorded(&name);
                    masking.merge(&record);
                    let lines = t
                        .lines
                        .into_iter()
                        .map(|line| {
                            let (line, record) = mask_recorded(&line);
                            masking.merge(&record);
                            line
                        })
                        .collect();
                    Template { name, lines }
                })
                .collect();
            (
                PolicyFile {
                    path: POLICY_PATH,
                    state: "read",
                    detail: None,
                },
                templates,
                None,
            )
        }
        Err(e) => said("unreadable", Some(e.to_string()), "policy_not_read"),
    }
}

#[cfg(test)]
mod exclusion_template_tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    fn policy_for(key: &str) -> thruline_core::capture_exclusion::ExclusionPolicy {
        thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
            "/synthetic/project",
            &[ExclusionView {
                key: key.into(),
                covers: key.into(),
                note: None,
                built_in: false,
                on: true,
                source: "recorded",
                basis: None,
                since: None,
                by: None,
            }],
        )
        .expect("synthetic policy")
    }

    #[test]
    fn the_policy_file_path_is_left_out_while_its_read_state_stays_visible() {
        let raw = br#"{"schema_version":1,"project":{"id":"synthetic"},"policy":{"path":".thruline/policy.yml","state":"unreadable","detail":"PRIVATE_POLICY_ERROR"},"templates":[]}"#;
        assert!(
            matches!(present_templates(raw, &policy_for("other")), Ok(std::borrow::Cow::Borrowed(bytes)) if bytes == raw)
        );
        let served = present_templates(raw, &policy_for(".thruline/policy.yml"))
            .expect("excluded policy view");
        let doc: serde_json::Value = serde_json::from_slice(&served).expect("served JSON");
        assert_eq!(
            doc["policy"]["path"],
            thruline_core::capture_exclusion::marker(".thruline/policy.yml")
        );
        assert_eq!(doc["policy"]["state"], "unreadable");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert!(!String::from_utf8_lossy(&served).contains("PRIVATE_POLICY_ERROR"));
    }

    #[test]
    fn a_template_line_naming_an_excluded_path_is_not_served() {
        let raw = br#"{"schema_version":1,"project":{"id":"synthetic"},"policy":{"path":".thruline/policy.yml","state":"read","detail":null},"templates":[{"name":"repair","lines":["Edit src/private.txt and keep its contents", "Run the tests"]}],"reason":null}"#;
        let served =
            present_templates(raw, &policy_for("src/private.txt")).expect("excluded template view");
        let doc: serde_json::Value = serde_json::from_slice(&served).expect("served JSON");
        assert!(!String::from_utf8_lossy(&served).contains("src/private.txt"));
        assert_eq!(doc["templates"][0]["lines"][1], "Run the tests");
        assert_eq!(doc["left_out_by_exclusion"], 1);
    }

    #[test]
    fn an_excluded_template_name_does_not_repeat_its_exclusion_key() {
        let raw = br#"{"schema_version":1,"project":{"id":"synthetic"},"policy":{"path":".thruline/policy.yml","state":"read","detail":null},"templates":[{"name":"private_template","lines":["Run the tests"]}],"reason":null}"#;
        let served = present_templates(raw, &policy_for("private_template"))
            .expect("excluded template name");
        let doc: serde_json::Value = serde_json::from_slice(&served).expect("served JSON");
        assert!(!String::from_utf8_lossy(&served).contains("private_template"));
        assert_eq!(doc["templates"][0]["name"], "left out by your exclusion");
        assert_eq!(doc["templates"][0]["lines"][0], "Run the tests");
        assert_eq!(doc["policy"]["path"], ".thruline/policy.yml");
        assert_eq!(doc["left_out_by_exclusion"], 1);
    }

    #[test]
    fn templates_from_an_excluded_policy_file_are_not_served() {
        let raw = br#"{"schema_version":1,"project":{"id":"synthetic"},"policy":{"path":".thruline/policy.yml","state":"read","detail":null},"templates":[{"name":"private_policy_template","lines":["PRIVATE_POLICY_TEMPLATE_CONTENT"]}],"reason":null}"#;
        assert!(
            matches!(present_templates(raw, &policy_for("other")), Ok(std::borrow::Cow::Borrowed(bytes)) if bytes == raw)
        );
        let served = present_templates(raw, &policy_for(".thruline/policy.yml"))
            .expect("excluded policy templates");
        let doc: serde_json::Value = serde_json::from_slice(&served).expect("served JSON");
        assert_eq!(doc["templates"], serde_json::json!([]));
        assert!(!String::from_utf8_lossy(&served).contains("private_policy_template"));
        assert!(!String::from_utf8_lossy(&served).contains("PRIVATE_POLICY_TEMPLATE_CONTENT"));
        assert_eq!(
            doc["policy"]["path"],
            thruline_core::capture_exclusion::marker(".thruline/policy.yml")
        );
        assert_eq!(doc["policy"]["state"], "read");
        assert!(doc["policy"]["detail"].is_null());
        assert!(doc["reason"].is_null());
        assert_eq!(doc["left_out_by_exclusion"], 1);
    }
}
