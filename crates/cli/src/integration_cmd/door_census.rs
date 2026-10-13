//! AI-REVIEW-OUT-OF-LAUNCH-R6 — the door census, the half that reads the clap
//! definitions.
//!
//! Five rounds each fixed the doors the last adversary found and each missed
//! one, because "every door that echoes an id" was a list somebody made by hand
//! (round 5's grep missed `integration evaluate`). This is the list made by the
//! program instead. `Command::augment_subcommands` is the same definition clap
//! parses the command line with, hidden subcommands and hidden arguments
//! included, so the census cannot miss a door the binary has.
//!
//! What it holds is `tests/support/integration_doors.json`: every subcommand and
//! every argument classified, and for each one that reads an id, a pack or a
//! recipe, the drive that hands it an id the build does not ship
//! (`tests/integration_door_census.rs` runs those against the compiled binary).
//! Adding a subcommand or an argument fails here until it is classified; naming
//! one that is gone fails here; classifying an argument as an id and driving
//! nothing at it fails here. The test runs in the `ai-review` build too, where
//! the same doors exist.
use super::Command;
use clap::{ArgAction, Subcommand};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const TABLE: &str = include_str!("../../tests/support/integration_doors.json");

/// The closed vocabulary of `classes` in the table.
const CLASSES: [&str; 10] = [
    "id",
    "id_in_file",
    "word",
    "other_id",
    "text",
    "content",
    "path",
    "number",
    "switch",
    "argv",
];

/// The classes whose value a drive must hand a hostile id.
const DRIVEN: [&str; 4] = ["id", "id_in_file", "word", "other_id"];

/// The placeholders a drive's argv may carry.
const PLACEHOLDERS: [&str; 8] = [
    "{id}",
    "{root}",
    "{exec}",
    "{plan}",
    "{cases}",
    "{results}",
    "{report}",
    "{action}",
];

struct Shape {
    long: Option<String>,
    positional: bool,
    /// Where a positional argument stands on the command line.
    index: Option<usize>,
    takes_value: bool,
}

fn table() -> Value {
    serde_json::from_str(TABLE).expect("the door census table is JSON")
}

/// Every subcommand of `thruline integration` and its arguments, as clap
/// defines them (`help` and `version` are clap's own).
fn defined() -> BTreeMap<String, BTreeMap<String, Shape>> {
    let mut command = Command::augment_subcommands(clap::Command::new("integration"));
    // Built, so that the positional arguments have their places.
    command.build();
    command
        .get_subcommands()
        .filter(|sub| sub.get_name() != "help")
        .map(|sub| {
            let args = sub
                .get_arguments()
                .filter(|a| !matches!(a.get_id().as_str(), "help" | "version"))
                .map(|a| {
                    (
                        a.get_id().to_string(),
                        Shape {
                            long: a.get_long().map(str::to_owned),
                            positional: a.is_positional(),
                            index: a.get_index(),
                            takes_value: !matches!(
                                a.get_action(),
                                ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Count
                            ),
                        },
                    )
                })
                .collect();
            (sub.get_name().to_owned(), args)
        })
        .collect()
}

fn classified(door: &Value) -> BTreeMap<String, String> {
    door["args"]
        .as_object()
        .expect("a door's `args` is an object")
        .iter()
        .map(|(arg, class)| {
            (
                arg.clone(),
                class.as_str().expect("a class is a string").to_owned(),
            )
        })
        .collect()
}

/// What a drive's argv hands each argument: the token at its flag's value, or
/// at its positional slot.
fn handed(name: &str, shapes: &BTreeMap<String, Shape>, argv: &[&str]) -> BTreeMap<String, String> {
    let mut given = BTreeMap::new();
    let mut ordered: Vec<(usize, &String)> = shapes
        .iter()
        .filter(|(_, s)| s.positional)
        .map(|(id, s)| (s.index.unwrap_or(usize::MAX), id))
        .collect();
    ordered.sort();
    let positionals: Vec<&String> = ordered.into_iter().map(|(_, id)| id).collect();
    let mut slot = 0;
    let mut i = 0;
    while i < argv.len() {
        if let Some(long) = argv[i].strip_prefix("--") {
            let (id, shape) = shapes
                .iter()
                .find(|(_, s)| s.long.as_deref() == Some(long))
                .unwrap_or_else(|| panic!("{name}: a drive uses `--{long}`, which is no argument"));
            if shape.takes_value {
                i += 1;
                given.insert(id.clone(), argv[i].to_owned());
            } else {
                given.insert(id.clone(), String::new());
            }
        } else {
            let id = positionals
                .get(slot)
                .unwrap_or_else(|| panic!("{name}: a drive has more positionals than the door"));
            given.insert((*id).clone(), argv[i].to_owned());
            slot += 1;
        }
        i += 1;
    }
    given
}

fn vias(drive: &Value) -> Vec<String> {
    match &drive["via"] {
        Value::String(one) => vec![one.clone()],
        Value::Array(many) => many
            .iter()
            .map(|v| v.as_str().expect("via is a string").to_owned())
            .collect(),
        other => panic!("a drive's `via` is a string or a list: {other}"),
    }
}

#[test]
fn every_integration_subcommand_and_argument_is_classified() {
    let table = table();
    let doors = table["cli"].as_object().expect("cli");
    let defined = defined();
    let mut problems = Vec::new();

    for name in defined.keys() {
        if !doors.contains_key(name) {
            problems.push(format!(
                "`thruline integration {name}` is not in the door census: classify it (and its \
                 arguments) in crates/cli/tests/support/integration_doors.json, and give every \
                 argument that reads an id, a pack or a recipe a drive"
            ));
        }
    }
    for name in doors.keys() {
        if !defined.contains_key(name) {
            problems.push(format!(
                "the door census names `thruline integration {name}`, which clap does not define"
            ));
        }
    }
    for (name, shapes) in &defined {
        let Some(door) = doors.get(name) else {
            continue;
        };
        let known = classified(door);
        for arg in shapes.keys() {
            match known.get(arg) {
                None => problems.push(format!(
                    "`integration {name}` argument `{arg}` is not classified in the door census"
                )),
                Some(class) if !CLASSES.contains(&class.as_str()) => problems.push(format!(
                    "`integration {name}` argument `{arg}` has class {class:?}, not one of {CLASSES:?}"
                )),
                Some(_) => {}
            }
        }
        for arg in known.keys() {
            if !shapes.contains_key(arg) {
                problems.push(format!(
                    "the door census classifies `integration {name}` argument `{arg}`, which clap does not define"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "the door census is out of step with the clap definitions:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn every_argument_that_reads_an_id_is_driven_with_an_id_at_that_argument() {
    let table = table();
    let defined = defined();
    let mut problems = Vec::new();
    for (name, door) in table["cli"].as_object().expect("cli") {
        let Some(shapes) = defined.get(name) else {
            continue; // the test above reports it
        };
        let known = classified(door);
        let operator_files: BTreeSet<String> = door["operator_files"]
            .as_object()
            .map(|files| files.keys().map(|k| format!("env:{k}")).collect())
            .unwrap_or_default();
        let mut reached: BTreeSet<String> = BTreeSet::new();
        for drive in door["drives"].as_array().expect("drives") {
            let via = vias(drive);
            let argv: Vec<&str> = drive["argv"]
                .as_array()
                .expect("argv")
                .iter()
                .map(|t| t.as_str().expect("token"))
                .collect();
            for token in &argv {
                if token.starts_with('{') && !PLACEHOLDERS.contains(token) {
                    problems.push(format!("{name}: unknown placeholder {token}"));
                }
            }
            if !["refused", "counted", "unused"].contains(&drive["outcome"].as_str().unwrap_or(""))
            {
                problems.push(format!(
                    "{name}: a drive's outcome is refused, counted or unused"
                ));
            }
            let given = handed(name, shapes, &argv);
            for via in via {
                if operator_files.contains(&via) {
                    reached.insert(via);
                    continue;
                }
                let Some(class) = known.get(&via) else {
                    problems.push(format!("{name}: a drive is `via` {via:?}, which is no argument or operator file of the door"));
                    continue;
                };
                let want = if class == "id_in_file" {
                    format!("{{{via}}}")
                } else {
                    "{id}".to_owned()
                };
                if !DRIVEN.contains(&class.as_str()) {
                    problems.push(format!(
                        "{name}: a drive is `via` {via:?}, which is classified {class:?}, not an id door"
                    ));
                } else if given.get(&via) != Some(&want) {
                    problems.push(format!(
                        "{name}: the drive `via` {via:?} hands it {:?}, not {want}",
                        given.get(&via)
                    ));
                } else {
                    reached.insert(via);
                }
            }
        }
        for (arg, class) in &known {
            if DRIVEN.contains(&class.as_str()) && !reached.contains(arg) {
                problems.push(format!(
                    "`integration {name}` argument `{arg}` is classified {class:?} but no drive hands it an id the build does not ship"
                ));
            }
        }
        for file in &operator_files {
            if !reached.contains(file) {
                problems.push(format!(
                    "`integration {name}` reads {file} and no drive reaches it"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "the door census names doors nothing drives:\n  {}",
        problems.join("\n  ")
    );
}

/// The four doors the R5 adversary's evaluation repro and the R5 verifier's
/// two observations came through are ones the census must classify as reading
/// an id, or the census is not looking at them.
#[test]
fn the_doors_earlier_rounds_found_are_classified_as_reading_ids() {
    let table = table();
    let cli = &table["cli"];
    for (door, arg, class) in [
        ("evaluate", "cases", "id_in_file"),
        ("evaluate", "results", "id_in_file"),
        ("agent-repair", "recipe", "id"),
        ("decode", "pack", "id"),
        ("plan", "pack", "id"),
        ("run", "plan", "id_in_file"),
        ("workflow", "recipe", "id"),
        ("native-repair", "recipe", "id"),
        ("review", "mode", "word"),
    ] {
        assert_eq!(
            cli[door]["args"][arg], class,
            "`integration {door}` argument `{arg}`"
        );
    }
}
