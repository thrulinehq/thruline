//! Explicit owned-service extension mode; legacy OpenAPI runner remains separate.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Extensions {
    checks: Vec<String>,
    hooks: Vec<String>,
    scalars: Vec<String>,
    serializers: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema: String,
    specification: String,
    schema_file: String,
    service_file: String,
    extension_file: String,
    required_extensions: Extensions,
    operations: Vec<String>,
    seed: u32,
    max_examples: u32,
}
fn strings(values: &[String], minimum: usize) -> Result<()> {
    anyhow::ensure!(
        values.len() >= minimum && values.len() <= 16,
        "Extension/operation list outside bounded scope"
    );
    anyhow::ensure!(
        values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "Duplicate extension/operation selection"
    );
    anyhow::ensure!(
        values
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)),
        "Malformed extension/operation name"
    );
    Ok(())
}
fn selected_file(root: &Path, name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    anyhow::ensure!(
        !name.is_empty()
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "Selected API inputs need relative ordinary file paths"
    );
    let mut current = root.to_path_buf();
    for part in path.components() {
        current.push(part);
        anyhow::ensure!(
            !std::fs::symlink_metadata(&current)?
                .file_type()
                .is_symlink(),
            "Selected API inputs cannot follow symbolic links"
        );
    }
    anyhow::ensure!(
        current.is_file(),
        "Selected API input is not a regular file"
    );
    bounded_file(&current, 262144)?;
    Ok(current)
}
pub(super) fn inputs(root: &Path) -> Result<Option<(String, Vec<PathBuf>)>> {
    let path = root.join(".thruline/schemathesis.json");
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let policy = selected_file(root, ".thruline/schemathesis.json")?;
    let selection: Selection =
        serde_json::from_value(engine::parse_json(&bounded_file(&policy, 65536)?)?)?;
    anyhow::ensure!(
        selection.schema == "thruline.schemathesis-extensions.v1",
        "Unknown Schemathesis extension selection"
    );
    anyhow::ensure!(
        matches!(selection.specification.as_str(), "graphql" | "openapi"),
        "Only the explicit GraphQL and OpenAPI extension modes are implemented"
    );
    anyhow::ensure!(
        selection.seed <= 2147483647 && (1..=20).contains(&selection.max_examples),
        "Selected seed or example count exceeds native qualification scope"
    );
    let ext = &selection.required_extensions;
    strings(&selection.operations, 1)?;
    for (names, minimum) in [
        (&ext.checks, 1),
        (&ext.hooks, 0),
        (&ext.scalars, 0),
        (&ext.serializers, 0),
    ] {
        strings(names, minimum)?;
    }
    anyhow::ensure!(
        ext.checks
            .iter()
            .all(|name| name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')),
        "Custom check names must be Python identifiers"
    );
    anyhow::ensure!(selection.schema_file.ends_with(if selection.specification == "graphql" { ".graphql" } else { ".json" }) && selection.service_file.ends_with(".py") && selection.extension_file.ends_with(".py"), "This mode needs GraphQL SDL or OpenAPI JSON, plus an owned Python service and Python extension module");
    let runner = selected_file(root, ".thruline/checks/api-run.py")?;
    anyhow::ensure!(bounded_file(&runner, 262144)? == include_bytes!("../../../../scripts/integrations/schemathesis-extensions.py"), "Install the exact candidate scripts/integrations/schemathesis-extensions.py as .thruline/checks/api-run.py before reviewing this mode");
    let files = vec![
        policy,
        runner,
        selected_file(root, &selection.schema_file)?,
        selected_file(root, &selection.service_file)?,
        selected_file(root, &selection.extension_file)?,
    ];
    Ok(Some((selection.specification, files)))
}
