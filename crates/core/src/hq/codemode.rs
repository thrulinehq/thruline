//! Codex "code mode": the tool calls inside one `exec` script.
//!
//! Recent Codex Desktop sessions do almost all of their work through a single
//! custom tool, `exec`, whose input is a JavaScript program that calls
//! `tools.exec_command({cmd: …})`, `tools.apply_patch("…")` and others. The
//! ingest layer stores that program verbatim as the `exec` row's `tool_input`
//! (`codex.rs` canonicalises only the classic tool names). Measured on the
//! founder's record on 2026-10-07: 45,043 `exec` calls in the ThruLine Codex
//! sessions, holding 37,775 `exec_command` and 3,609 `apply_patch` calls —
//! every Codex edit in that record is inside one of these programs.
//!
//! This module reads the string ARGUMENTS of those calls and nothing else. It
//! is not a JavaScript interpreter: an argument is recovered only when it is a
//! string literal, a template literal, a name bound earlier in the same
//! program by `const|let|var NAME = <string expression>`, or a `+`
//! concatenation of those. Anything else (a function call, a variable
//! assigned twice, a computed value) makes that argument **unresolved**, and
//! the caller says so rather than guessing.

use std::collections::BTreeMap;

/// One call of `tools.<name>(…)` found in a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// Byte offset of `tools.` in the program — the order calls were written.
    pub at: usize,
    /// The recovered string argument, `None` when it could not be recovered.
    pub value: Option<String>,
}

/// Every `tools.exec_command` call's `cmd` (or `command`) argument.
pub fn exec_commands(program: &str) -> Vec<Call> {
    calls(program, "exec_command", &["cmd", "command"])
}

/// Every `tools.apply_patch` call's patch text (a bare string argument, or the
/// `input`/`patch` key of an object argument).
pub fn apply_patches(program: &str) -> Vec<Call> {
    calls(program, "apply_patch", &["input", "patch"])
}

fn calls(program: &str, tool: &str, keys: &[&str]) -> Vec<Call> {
    let bindings = bindings(program);
    let needle = format!("tools.{tool}(");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(pos) = program[from..].find(&needle) {
        let at = from + pos;
        let arg_start = at + needle.len();
        let value = argument(program, arg_start, keys, &bindings);
        out.push(Call { at, value });
        from = arg_start;
    }
    out
}

/// `const|let|var NAME = <string expression>` bindings, in program order. A
/// name bound twice is dropped: which value a later call saw would depend on
/// control flow this module does not follow.
fn bindings(program: &str) -> BTreeMap<String, String> {
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for keyword in ["const ", "let ", "var "] {
        let mut from = 0usize;
        while let Some(pos) = program[from..].find(keyword) {
            let at = from + pos;
            from = at + keyword.len();
            let boundary_ok = at == 0
                || program[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_alphanumeric() && c != '_' && c != '.');
            if boundary_ok {
                starts.push((at, from));
            }
        }
    }
    starts.sort();
    let mut seen: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (_, from) in starts {
        let rest = &program[from..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if name.is_empty() {
            continue;
        }
        let after = rest[name.len()..].trim_start();
        let Some(expr) = after.strip_prefix('=') else {
            continue;
        };
        if expr.starts_with('=') {
            continue;
        }
        let offset = program.len() - expr.len();
        let resolved: BTreeMap<String, String> = seen
            .iter()
            .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v)))
            .collect();
        let value = expression(program, offset, &resolved).map(|(v, _)| v);
        match seen.entry(name) {
            std::collections::btree_map::Entry::Occupied(mut bound) => {
                bound.insert(None);
            }
            std::collections::btree_map::Entry::Vacant(free) => {
                free.insert(value);
            }
        }
    }
    seen.into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

fn argument(
    program: &str,
    start: usize,
    keys: &[&str],
    bindings: &BTreeMap<String, String>,
) -> Option<String> {
    let rest = &program[start..];
    let lead = rest.len() - rest.trim_start().len();
    let start = start + lead;
    if program[start..].starts_with('{') {
        let mut i = start + 1;
        // Walk the object's top-level `key: value` pairs.
        loop {
            i = skip_ws(program, i);
            let c = program[i..].chars().next()?;
            if c == '}' {
                return None;
            }
            let (key, after_key) = if c == '"' || c == '\'' {
                let (k, end) = string_literal(program, i)?;
                (k, end)
            } else {
                let k: String = program[i..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if k.is_empty() {
                    return None;
                }
                let len = k.len();
                (k, i + len)
            };
            let j = skip_ws(program, after_key);
            if !program[j..].starts_with(':') {
                // Shorthand `{cmd}` — a binding of the same name.
                if keys.contains(&key.as_str()) {
                    return bindings.get(&key).cloned();
                }
                i = skip_value(program, j)?;
                continue;
            }
            let value_start = j + 1;
            if keys.contains(&key.as_str()) {
                return expression(program, value_start, bindings).map(|(v, _)| v);
            }
            i = skip_value(program, value_start)?;
        }
    }
    expression(program, start, bindings).map(|(v, _)| v)
}

fn skip_ws(program: &str, mut i: usize) -> usize {
    while let Some(c) = program[i..].chars().next() {
        if c.is_whitespace() {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    i
}

/// Skip one value and its trailing comma. Returns the index after the comma
/// (or at the closing brace).
fn skip_value(program: &str, mut i: usize) -> Option<usize> {
    let mut depth = 0i32;
    while let Some(c) = program[i..].chars().next() {
        match c {
            '"' | '\'' | '`' => {
                let (_, end) = string_literal(program, i)?;
                i = end;
                continue;
            }
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            ',' if depth == 0 => return Some(i + 1),
            _ => {}
        }
        i += c.len_utf8();
    }
    None
}

/// A `+`-concatenation of literals and bound names, starting at `start`.
/// Returns the value and the index just past it.
fn expression(
    program: &str,
    start: usize,
    bindings: &BTreeMap<String, String>,
) -> Option<(String, usize)> {
    let mut value = String::new();
    let mut i = skip_ws(program, start);
    loop {
        let c = program[i..].chars().next()?;
        if c == '"' || c == '\'' || c == '`' {
            let (s, end) = if c == '`' {
                template_literal(program, i, bindings)?
            } else {
                string_literal(program, i)?
            };
            value.push_str(&s);
            i = end;
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let name: String = program[i..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            let end = i + name.len();
            // `name(` or `name.x` is a call or a property — not a string.
            let next = program[end..].chars().next();
            if matches!(next, Some('(') | Some('.') | Some('[')) {
                return None;
            }
            value.push_str(bindings.get(&name)?);
            i = end;
        } else {
            return None;
        }
        let j = skip_ws(program, i);
        match program[j..].chars().next() {
            Some('+') => {
                i = skip_ws(program, j + 1);
            }
            // A method or index on the string: its value is not the literal.
            Some('.') | Some('[') => return None,
            _ => return Some((value, j)),
        }
    }
}

/// A `"…"` or `'…'` literal starting at `start`. Returns the decoded value and
/// the index just past the closing quote.
fn string_literal(program: &str, start: usize) -> Option<(String, usize)> {
    let quote = program[start..].chars().next()?;
    let mut out = String::new();
    let mut chars = program[start + 1..].char_indices();
    while let Some((off, c)) = chars.next() {
        if c == quote {
            return Some((out, start + 1 + off + 1));
        }
        if c == '\\' {
            let (_, e) = chars.next()?;
            match e {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'v' => out.push('\u{b}'),
                'x' => {
                    let h: String = (0..2)
                        .filter_map(|_| chars.next().map(|(_, c)| c))
                        .collect();
                    out.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?)?);
                }
                'u' => {
                    let mut hex = String::new();
                    let (_, first) = chars.next()?;
                    if first == '{' {
                        for (_, h) in chars.by_ref() {
                            if h == '}' {
                                break;
                            }
                            hex.push(h);
                        }
                    } else {
                        hex.push(first);
                        for _ in 0..3 {
                            hex.push(chars.next()?.1);
                        }
                    }
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    // A lone surrogate half cannot be a `char`; it is
                    // replaced, visibly, rather than dropped.
                    out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                }
                '\n' => {}
                other => out.push(other),
            }
            continue;
        }
        if c == '\n' {
            // An unterminated single-line literal.
            return None;
        }
        out.push(c);
    }
    None
}

/// A template literal; `${name}` placeholders resolve through `bindings` and
/// anything more complex makes the literal unresolved.
fn template_literal(
    program: &str,
    start: usize,
    bindings: &BTreeMap<String, String>,
) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = program[start + 1..].char_indices();
    while let Some((off, c)) = chars.next() {
        match c {
            '`' => return Some((out, start + 1 + off + 1)),
            '\\' => {
                let (_, e) = chars.next()?;
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                }
            }
            '$' => {
                let rest = &program[start + 1 + off + 1..];
                if rest.starts_with('{') {
                    let close = rest.find('}')?;
                    let name = rest[1..close].trim();
                    out.push_str(bindings.get(name)?);
                    for _ in 0..close + 1 {
                        chars.next()?;
                    }
                } else {
                    out.push('$');
                }
            }
            other => out.push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_read_from_object_arguments() {
        let p = r#"const r=await Promise.allSettled([
tools.exec_command({cmd:"cargo test -p core\ngit status","workdir":"/x"}),
tools.exec_command({"cmd": 'npm test', max_output_tokens: 10}),
tools.exec_command({workdir: base, cmd: dynamic()})]);"#;
        let got: Vec<Option<String>> = exec_commands(p).into_iter().map(|c| c.value).collect();
        assert_eq!(
            got,
            vec![
                Some("cargo test -p core\ngit status".to_string()),
                Some("npm test".to_string()),
                None
            ]
        );
    }

    #[test]
    fn patches_resolve_bound_names_and_concatenation() {
        let p = "const own=\"/repo\";\ntext(await tools.apply_patch(\"*** Begin Patch\\n*** Add File: \"+own+\"/a.txt\\n+hi\\n*** End Patch\"));";
        let got = apply_patches(p);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].value.as_deref(),
            Some("*** Begin Patch\n*** Add File: /repo/a.txt\n+hi\n*** End Patch")
        );
    }

    #[test]
    fn a_name_bound_twice_is_unresolved() {
        let p = "let f=\"a\"; let f=\"b\"; tools.apply_patch(f);";
        assert_eq!(apply_patches(p)[0].value, None);
    }

    #[test]
    fn template_literals_resolve_simple_placeholders() {
        let p = "const d='/r'; tools.exec_command({cmd:`ls ${d}/src`});";
        assert_eq!(exec_commands(p)[0].value.as_deref(), Some("ls /r/src"));
    }
}
