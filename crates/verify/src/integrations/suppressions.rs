//! Pure source analysis; the CLI supplies and binds bytes. These are coverage
//! limits, never reconstructed findings. The pinned tools have different filters.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceSuppressions {
    pub directives: Vec<SourceDirective>,
    pub unavailable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDirective {
    pub line: u64,
    pub byte_start: usize,
    pub byte_end: usize,
    /// None is a blanket directive; Some is the producer's named set.
    pub rules: Option<Vec<String>>,
    pub basis: String,
}

fn unavailable(reason: &str) -> SourceSuppressions {
    SourceSuppressions {
        unavailable: Some(reason.into()),
        ..Default::default()
    }
}

pub fn source_suppressions(format: &str, path: &str, bytes: &[u8]) -> SourceSuppressions {
    if format == "semgrep-json-v1" {
        return semgrep_line_filter(bytes);
    }
    if format != "ast-grep-json-v1" {
        return SourceSuppressions::default();
    }
    // Absence of the required byte sequence rules out this filter in every
    // language. Presence alone is not a directive or a source violation.
    if !bytes
        .windows(b"ast-grep-ignore".len())
        .any(|w| w == b"ast-grep-ignore")
    {
        return SourceSuppressions::default();
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
        return unavailable("Source with a possible structural suppression is not UTF-8.");
    };
    let lower = path.to_ascii_lowercase();
    let language: tree_sitter::Language = match lower.rsplit('.').next().unwrap_or("") {
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "py" | "pyi" => tree_sitter_python::LANGUAGE.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" | "mts" | "cts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        _ => return unavailable("This source language has no qualified comment parser in this build; suppression completeness is unavailable."),
    };
    let occurrences: Vec<_> = bytes
        .windows(b"ast-grep-ignore".len())
        .enumerate()
        .filter_map(|(i, w)| (w == b"ast-grep-ignore").then_some(i))
        .take(4097)
        .collect();
    if occurrences.len() > 4096 {
        return unavailable("Source contains more than 4096 marker locations; the bounded source audit is incomplete.");
    }
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return unavailable(
            "The structural suppression comment parser could not load its grammar.",
        );
    }
    let mut progress_steps = 0usize;
    let mut stop = |_state: &tree_sitter::ParseState| {
        progress_steps += 1;
        progress_steps > 100_000
    };
    let Some(tree) = parser.parse_with_options(
        &mut |i, _| &bytes[i.min(bytes.len())..],
        None,
        Some(tree_sitter::ParseOptions::new().progress_callback(&mut stop)),
    ) else {
        return unavailable("The structural suppression comment parser did not finish.");
    };
    let mut classified = vec![false; occurrences.len()];
    let mut ambiguous = vec![false; occurrences.len()];
    let mut visited_nodes = 0usize;
    let mut audit = SourceSuppressions::default();
    let mut cursor = tree.walk();
    'walk: loop {
        let node = cursor.node();
        visited_nodes += 1;
        if visited_nodes > 1_000_000 {
            audit.unavailable =
                Some("Source audit exceeded its deterministic syntax-node work ceiling.".into());
            return audit;
        }
        let first_marker = occurrences.partition_point(|i| *i < node.start_byte());
        if node.is_error() {
            for (i, start) in occurrences.iter().enumerate().skip(first_marker) {
                if *start >= node.end_byte() {
                    break;
                }
                ambiguous[i] = true;
            }
        }
        let comment = node.kind().contains("comment");
        let literal = match node.kind() {
            "string_literal" | "raw_string_literal" | "string_content" | "string_fragment" => true,
            // Python f-strings have interpolation children; do not classify
            // their expression regions as harmless literal content.
            "string" => !(0..node.named_child_count()).any(|i| {
                node.named_child(i)
                    .is_some_and(|n| n.kind() == "interpolation")
            }),
            _ => false,
        };
        if !node.has_error() && (comment || literal) {
            for (i, start) in occurrences.iter().enumerate().skip(first_marker) {
                if *start >= node.end_byte() {
                    break;
                }
                if *start + b"ast-grep-ignore".len() <= node.end_byte() {
                    classified[i] = true;
                }
            }
        }
        if comment {
            let text = &source[node.byte_range()];
            // Pinned ast-grep combined.rs:68–78: only comment nodes, with
            // ast-grep-ignore as the first alphabetic text. Prose is excluded.
            if text
                .find(char::is_alphabetic)
                .is_some_and(|i| text[i..].starts_with("ast-grep-ignore"))
            {
                let after = text.split_once("ast-grep-ignore").unwrap().1.trim();
                let rules = after.split_once(':').map(|(_, ids)| {
                    // Match combined.rs:400–419, including its literal-space
                    // token boundary. Do not invent a broader rule-id grammar.
                    ids.split(',')
                        .map(|id| id.trim_start().split(' ').next().unwrap_or("").into())
                        .collect()
                });
                if audit.directives.len() == 4096 {
                    audit.unavailable = Some("Source contains more than 4096 suppression directives; the audit is incomplete.".into());
                    return audit;
                }
                audit.directives.push(SourceDirective {
                    line: node.start_position().row as u64 + 1,
                    byte_start: node.start_byte(), byte_end: node.end_byte(), rules,
                    basis: "ast-grep comment syntax; matching rule coverage must be resolved separately".into(),
                });
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    // Structural tools accept fragments. An unrelated syntax error does not
    // invalidate a complete string/comment node. Only an unresolved marker in
    // an ERROR range makes its classification unavailable.
    if ambiguous
        .iter()
        .enumerate()
        .any(|(i, ambiguous)| *ambiguous && !classified[i])
    {
        audit.unavailable = Some("A possible suppression lies in an ambiguous parse-error region; exact comment/string classification is unavailable.".into());
    }
    audit
}

fn semgrep_line_filter(bytes: &[u8]) -> SourceSuppressions {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return unavailable("Legacy Semgrep source is not UTF-8; use a reviewed --disable-nosem capture to establish suppression completeness.");
    };
    let mut audit = SourceSuppressions::default();
    let mut offset = 0usize;
    for (row, line) in source.split_inclusive('\n').enumerate() {
        let lower = line.to_ascii_lowercase();
        // Nosemgrep.ml:34–79 scans raw lines, not comment tokens. The inline
        // regex requires a literal space and has no identifier boundary. Its
        // preceding-line regex permits every prefix without ASCII letters or
        // digits. These are potential filters; no absent match is fabricated.
        let inline = lower.find(" nosem").map(|i| i + 1);
        let previous = lower
            .find(|c: char| c.is_ascii_alphanumeric())
            .filter(|i| lower[*i..].starts_with("nosem"));
        if let Some(start) = inline.or(previous) {
            if audit.directives.len() == 4096 {
                audit.unavailable = Some("Source contains more than 4096 possible native line filters; the audit is incomplete.".into());
                return audit;
            }
            audit.directives.push(SourceDirective {
                line: row as u64 + 1, byte_start: offset + start,
                byte_end: offset + line.trim_end_matches(['\r', '\n']).len(),
                rules: None,
                basis: "pinned Semgrep raw-line filter may omit matches; default JSON cannot establish whether a matching finding was hidden (including named directives and some strings)".into(),
            });
        }
        offset += line.len();
    }
    audit
}

/// Only recognize a reviewed, unambiguous scan argv. Unknown options do not
/// gain completeness from a token that might instead be another option's value.
pub fn semgrep_suppression_disabled(argv: &[String]) -> bool {
    if argv.get(1).map(String::as_str) != Some("scan") {
        return false;
    }
    let mut disabled = false;
    let mut i = 2;
    while i < argv.len() {
        let arg = argv[i].as_str();
        match arg {
            "--" => break,
            "--disable-nosem" => disabled = true,
            "--enable-nosem" => disabled = false,
            "--config" | "-c" | "--include" | "--exclude" | "--lang" | "-l" | "--pattern"
            | "-e" | "--exclude-rule" => {
                i += 1;
                if i == argv.len() {
                    return false;
                }
            }
            "--json"
            | "--metrics=off"
            | "--disable-version-check"
            | "--strict"
            | "--error"
            | "--no-git-ignore"
            | "--verbose"
            | "--quiet" => {}
            _ if arg.starts_with('-') => return false,
            _ => {}
        }
        i += 1;
    }
    disabled
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structural_comments_strings_prose_and_named_controls() {
        let examples = [
            (
                "x.rs",
                "fn f() { let x = 1; } // ast-grep-ignore: selected\n",
            ),
            ("x.ts", "foo(1); // ast-grep-ignore: selected\n"),
            ("x.tsx", "const a = <b/>; // ast-grep-ignore: selected\n"),
            ("x.js", "foo(1); // ast-grep-ignore: selected\n"),
            ("x.py", "x = 1 # ast-grep-ignore: selected\n"),
            (
                "x.go",
                "package main\n// ast-grep-ignore: selected\nfunc f() {}\n",
            ),
        ];
        for (path, source) in examples {
            let r = source_suppressions("ast-grep-json-v1", path, source.as_bytes());
            assert!(r.unavailable.is_none(), "{path}: {r:?}");
            assert_eq!(r.directives.len(), 1, "{path}");
            assert_eq!(r.directives[0].rules.as_ref().unwrap(), &["selected"]);
        }
        for (path, source) in [
            (
                "x.ts",
                "const a = 'ast-grep-ignore'; // Documentation mentions ast-grep-ignore\n",
            ),
            ("x.rs", "fn f() { let a = r#\"ast-grep-ignore\"#; }\n"),
            (
                "x.py",
                "\"\"\"Documentation says ast-grep-ignore\"\"\"\nx = 'ast-grep-ignore'\n",
            ),
            ("x.rs", "Some(\"ast-grep-ignore\")"),
            (
                "x.rs",
                "Some(123) // Documentation mentions ast-grep-ignore\n",
            ),
            ("x.ts", "Some('ast-grep-ignore')"),
        ] {
            let r = source_suppressions("ast-grep-json-v1", path, source.as_bytes());
            assert!(r.unavailable.is_none());
            assert!(r.directives.is_empty(), "{r:?}");
        }
        let bare = source_suppressions(
            "ast-grep-json-v1",
            "x.ts",
            b"// ast-grep-ignore\n\nfoo(1);\n",
        );
        assert!(bare.directives[0].rules.is_none());
        let unsupported =
            source_suppressions("ast-grep-json-v1", "x.java", b"// ast-grep-ignore\n");
        assert!(unsupported.unavailable.is_some());
        assert!(
            source_suppressions("ast-grep-json-v1", "x.java", b"class X {}\n")
                .unavailable
                .is_none()
        );
    }

    #[test]
    fn semgrep_native_line_filter_is_not_a_comment_parser() {
        for source in [
            "x = eval(y) # nosemgrep: selected\n",
            "# NOSEM\nx = eval(y)\n",
            "x = eval(y); text = ' nosem'\n",
            "x = eval(y) # nosemgrep: unrelated\n",
        ] {
            assert_eq!(
                source_suppressions("semgrep-json-v1", "x.py", source.as_bytes())
                    .directives
                    .len(),
                1
            );
        }
        for source in [
            "x = 'nosemgrep'\n",
            "# prose: nose mgrep is documented here\n",
            "x = 1\n",
        ] {
            assert!(
                source_suppressions("semgrep-json-v1", "x.py", source.as_bytes())
                    .directives
                    .is_empty()
            );
        }
    }

    #[test]
    fn suppression_disable_must_be_a_parsed_option_not_an_argument_value() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert!(semgrep_suppression_disabled(&args(
            "semgrep scan --config rules.yml --json --strict --error --disable-nosem /input"
        )));
        assert!(!semgrep_suppression_disabled(&args(
            "semgrep scan --config --disable-nosem /input"
        )));
        assert!(!semgrep_suppression_disabled(&args(
            "semgrep scan --unknown --disable-nosem /input"
        )));
        assert!(!semgrep_suppression_disabled(&args(
            "semgrep scan --disable-nosem --enable-nosem /input"
        )));
        assert!(!semgrep_suppression_disabled(&args(
            "semgrep scan -- /input/--disable-nosem"
        )));
    }
}
