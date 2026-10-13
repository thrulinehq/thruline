//! Validate explicit preview scope before a user accepts native local commands.
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeSet;

fn object<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a serde_json::Map<String, Value>> {
    let obj = value
        .as_object()
        .context("MegaLinter selection object required")?;
    anyhow::ensure!(
        obj.keys().map(String::as_str).collect::<BTreeSet<_>>() == keys.iter().copied().collect(),
        "MegaLinter selection has missing/unknown fields"
    );
    Ok(obj)
}
fn relative(value: &Value) -> Result<String> {
    let name = value
        .as_str()
        .context("Selected command/plugin file required")?;
    anyhow::ensure!(
        !name.is_empty()
            && name.len() <= 512
            && !name.starts_with('/')
            && !name.contains('\\')
            && !name.contains('\0')
            && !name
                .split('/')
                .any(|x| matches!(x, "" | "." | ".." | ".git")),
        "Selected command/plugin path escapes captured source"
    );
    Ok(name.into())
}
fn names(value: &Value, limit: usize) -> Result<Vec<String>> {
    let rows = value
        .as_array()
        .context("MegaLinter configuration list required")?;
    anyhow::ensure!(rows.len() <= limit, "MegaLinter list exceeds bound");
    let rows = rows
        .iter()
        .map(|x| {
            x.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .map(str::to_owned)
                .context("Invalid MegaLinter name")
        })
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(
        rows.iter().collect::<BTreeSet<_>>().len() == rows.len(),
        "Duplicate MegaLinter name"
    );
    Ok(rows)
}
fn integer(value: &Value, max: u64) -> Result<()> {
    anyhow::ensure!(
        value.as_u64().is_some_and(|n| n > 0 && n <= max),
        "MegaLinter positive integer exceeds bound"
    );
    Ok(())
}
pub(super) fn files(value: &Value) -> Result<Vec<String>> {
    object(
        value,
        &[
            "schema",
            "advisor",
            "linters",
            "flavor",
            "filters",
            "parallel",
            "timeouts",
            "plugins",
            "pre_commands",
            "post_commands",
            "environment",
        ],
    )?;
    anyhow::ensure!(
        value["schema"] == "thruline.megalinter-selection.v2" && value["advisor"] == false,
        "Controlled MegaLinter schema requires advisor:false; use the separate Advisor route"
    );
    let linters = names(&value["linters"], 16)?;
    anyhow::ensure!(!linters.is_empty(), "Select at least one analyzer");
    let flavor = value["flavor"].as_str().context("Native flavor required")?;
    anyhow::ensure!(
        !flavor.is_empty()
            && flavor.len() <= 32
            && flavor.bytes().all(|x| x.is_ascii_lowercase() || x == b'_'),
        "Invalid flavor identity"
    );
    object(
        &value["filters"],
        &[
            "include",
            "exclude",
            "excluded_directories",
            "ignore_generated",
        ],
    )?;
    for name in ["include", "exclude"] {
        let text = &value["filters"][name];
        anyhow::ensure!(
            text.is_null() || text.as_str().is_some_and(|s| s.len() <= 512),
            "Filter expression exceeds bound"
        );
    }
    for name in names(&value["filters"]["excluded_directories"], 16)? {
        anyhow::ensure!(
            !name.contains('/') && ![".", "..", ".git"].contains(&name.as_str()),
            "Invalid excluded directory"
        );
    }
    anyhow::ensure!(
        value["filters"]["ignore_generated"].is_boolean(),
        "Explicit generated-file filtering required"
    );
    object(&value["parallel"], &["enabled", "workers"])?;
    anyhow::ensure!(
        value["parallel"]["enabled"].is_boolean(),
        "Explicit parallel boolean required"
    );
    integer(&value["parallel"]["workers"], 4)?;
    object(
        &value["timeouts"],
        &["overall_seconds", "linter_seconds", "per_linter"],
    )?;
    integer(&value["timeouts"]["overall_seconds"], 300)?;
    integer(&value["timeouts"]["linter_seconds"], 120)?;
    for (name, limit) in value["timeouts"]["per_linter"]
        .as_object()
        .context("Per-linter timeout map required")?
    {
        anyhow::ensure!(linters.contains(name), "Timeout names unrequested analyzer");
        integer(limit, 120)?;
    }
    let mut allowed = BTreeSet::from(["PYTHON_RUFF".to_owned(), "YAML_YAMLLINT".to_owned()]);
    let mut files = BTreeSet::new();
    let plugins = value["plugins"]
        .as_array()
        .context("Explicit plugin list required")?;
    anyhow::ensure!(plugins.len() <= 4, "Too many local plugins");
    for plugin in plugins {
        object(plugin, &["descriptor", "sha256", "linters", "script_files"])?;
        let path = relative(&plugin["descriptor"])?;
        anyhow::ensure!(
            format!("/{path}").contains("/mega-linter-plugin-")
                && path.rsplit('/').next().is_some_and(
                    |x| x.starts_with("thruline-") && x.ends_with(".megalinter-descriptor.yml")
                ),
            "Use a distinct thruline-prefixed native plugin descriptor"
        );
        let sha = plugin["sha256"]
            .as_str()
            .context("Plugin descriptor digest required")?;
        anyhow::ensure!(
            sha.len() == 64
                && sha
                    .bytes()
                    .all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x)),
            "Invalid plugin digest"
        );
        files.insert(path);
        let ids = names(&plugin["linters"], 4)?;
        anyhow::ensure!(!ids.is_empty(), "Declare plugin native identities");
        for id in ids {
            anyhow::ensure!(
                id.starts_with("LOCAL_")
                    && id.len() > 6
                    && id
                        .bytes()
                        .all(|x| x.is_ascii_uppercase() || x.is_ascii_digit() || x == b'_')
                    && allowed.insert(id),
                "Local plugin identity collides or is invalid"
            );
        }
        for script in names(&plugin["script_files"], 8)? {
            files.insert(relative(&Value::String(script))?);
        }
    }
    anyhow::ensure!(
        linters.iter().all(|id| allowed.contains(id)),
        "Requested analyzer is not installed or declared"
    );
    for phase in ["pre_commands", "post_commands"] {
        let commands = value[phase]
            .as_array()
            .context("Explicit native command list required")?;
        anyhow::ensure!(commands.len() <= 4, "Too many native commands");
        for command in commands {
            object(command, &["argv", "continue_if_failed"])?;
            anyhow::ensure!(
                command["continue_if_failed"].is_boolean(),
                "Explicit command failure policy required"
            );
            let argv = command["argv"]
                .as_array()
                .context("Explicit command argv required")?;
            anyhow::ensure!(
                (2..=16).contains(&argv.len())
                    && argv[0] == "python"
                    && argv.iter().all(|x| x
                        .as_str()
                        .is_some_and(|s| s.len() <= 512 && !s.contains('\0'))),
                "Native command must use captured Python script and bounded argv"
            );
            files.insert(relative(&argv[1])?);
        }
    }
    let env = value["environment"]
        .as_object()
        .context("Explicit supplied environment map required")?;
    anyhow::ensure!(env.len() <= 8, "Too many environment entries");
    for (key, text) in env {
        anyhow::ensure!(key.starts_with("THRULINE_MEGA_") && key.len()>14 && key.len()<=78 && key.bytes().all(|x|x.is_ascii_uppercase() || x.is_ascii_digit() || x==b'_') && text.as_str().is_some_and(|s|s.len()<=512 && !s.contains('\0')),"Only explicit bounded THRULINE_MEGA_* entries are supported; host environment is not inherited");
    }
    Ok(files.into_iter().collect())
}
