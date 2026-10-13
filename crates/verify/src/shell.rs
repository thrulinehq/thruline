//! BUILD-SPEC §13 — parsing `sh -c` payloads instead of pattern-matching them.
//!
//! > *"Obfuscated or split commands → capture is at exec level, not by pattern
//! > on the prompt; parse `sh -c` payloads; record child processes."*
//!
//! # Why a parser and not a `contains("pytest")`
//!
//! A substring match on a command line is wrong in both directions and the
//! wrong direction is the dangerous one:
//!
//! - `echo "pytest passes"` contains `pytest` and ran no tests. A matcher
//!   accepts it and a check appears out of nothing.
//! - `PYTHONPATH=. py.test -q` does not contain `pytest`. A matcher misses a
//!   real run — which is the safe direction, but only by luck.
//!
//! So this module tokenises the payload the way a POSIX shell would, splits it
//! on the operators that separate commands, and hands back one argv per
//! command. [`Runner::from_argv`](crate::parse::Runner::from_argv) then answers
//! the recognition question from an argv, exactly as it does for a directly
//! executed command.
//!
//! # It refuses far more often than it guesses
//!
//! Every construct whose expansion this module cannot compute — `$(…)`,
//! backticks, `${…}`, `$VAR`, globs, subshells, `eval` — returns
//! [`ShellParseError`] rather than a best-effort argv. That is not timidity:
//! a wrong argv here would be *invented evidence*, and the exec recorder is
//! already capturing the real invocation at the exec level. Refusing costs a
//! label on a command whose truth the shim records anyway; guessing costs the
//! difference between what ran and what we say ran.
//!
//! Interpreting this output as authoritative is therefore always wrong: it is
//! an enrichment of a *recorded* exec, never a substitute for one.

/// Why a payload could not be reduced to argv vectors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShellParseError {
    /// A construct whose value depends on the shell's state at run time.
    #[error("{construct} at byte {at} expands at run time, so the real argv cannot be known from the text alone")]
    Unexpandable { construct: &'static str, at: usize },
    /// A quote or escape that never closes.
    #[error("unterminated {what} starting at byte {at}")]
    Unterminated { what: &'static str, at: usize },
}

/// What [`shell_payload`] found in an argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellPayload {
    /// Not a shell-style invocation carrying an inline script.
    NotShell,
    /// A shell payload that parsed into zero or more commands.
    Parsed(Vec<Command>),
    /// A shell payload that could not be parsed exactly. **Not an empty
    /// result**: a caller must be able to tell "no runner in this script" from
    /// "this script could not be read", because only the first is evidence.
    Unparsed { payload: String, reason: String },
}

/// One command from a payload: its argv, plus whether shell syntax was removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    pub argv: Vec<String>,
    /// A redirection was stripped to leave a clean argv (`pytest -q > out.txt`).
    /// Recorded because a redirected run's output did not reach the recorder.
    pub redirected: bool,
    /// `VAR=value` prefixes that were stripped from the front of the command.
    pub env_prefix: Vec<String>,
}

/// Programs whose `-c` argument is a shell script.
const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "ash", "mksh"];

/// Does `argv` invoke a shell (or an npm-family runner) with an inline script,
/// and if so what does the script run?
///
/// `npx -c` / `npx --call` and `npm exec -c` are here because they are
/// **wrapped runners** (§4.4's list) that take a shell payload: without this,
/// `npx -c 'pytest -q'` would be recorded as an `npx` invocation with no
/// recognised runner even though the shim also captured the nested `pytest`.
pub fn shell_payload(argv: &[String]) -> ShellPayload {
    let Some(payload) = inline_script(argv) else {
        return ShellPayload::NotShell;
    };
    match split_commands(payload) {
        Ok(commands) => ShellPayload::Parsed(commands),
        Err(reason) => ShellPayload::Unparsed {
            payload: payload.to_string(),
            reason: reason.to_string(),
        },
    }
}

/// The inline script `argv` carries, if any.
fn inline_script(argv: &[String]) -> Option<&str> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    if SHELLS.contains(&base) {
        return flag_value(&argv[1..], 'c', "--command");
    }
    if base == "npx" {
        return flag_value(&argv[1..], 'c', "--call");
    }
    if base == "npm" && argv.get(1).map(String::as_str) == Some("exec") {
        return flag_value(&argv[2..], 'c', "--call");
    }
    None
}

/// The value of a short flag (possibly inside a cluster like `-lc`) or a long
/// flag, whichever appears first.
fn flag_value<'a>(args: &'a [String], short: char, long: &str) -> Option<&'a str> {
    for (i, arg) in args.iter().enumerate() {
        if arg == long {
            return args.get(i + 1).map(String::as_str);
        }
        if let Some(rest) = arg.strip_prefix(long) {
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value);
            }
        }
        // `-c`, and clusters such as `-lc` / `-ec`. A cluster is only a cluster
        // when every character in it is a flag letter, so `-config` is not one.
        if arg.len() > 1 && arg.starts_with('-') && !arg.starts_with("--") {
            let letters: Vec<char> = arg.chars().skip(1).collect();
            if letters.contains(&short) && letters.iter().all(char::is_ascii_alphabetic) {
                // The script is the next word regardless of where in the
                // cluster `c` sits: a shell requires `c` to be last for the
                // script to follow, and an invocation where it is not is
                // already malformed.
                return args.get(i + 1).map(String::as_str);
            }
        }
    }
    None
}

/// Split a shell script into the commands it runs.
///
/// Recognises the separators `&&`, `||`, `;`, `|`, `&` and newline. Everything
/// whose expansion is not computable from the text is an error — see the module
/// docs for why that is the safe direction.
pub fn split_commands(script: &str) -> Result<Vec<Command>, ShellParseError> {
    let mut p = Parser::default();
    let chars: Vec<char> = script.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                p.end_word();
                i += 1;
            }
            '\n' | ';' => {
                p.end_command();
                i += 1;
            }
            '&' => {
                if chars.get(i + 1) == Some(&'&') {
                    p.end_command();
                    i += 2;
                } else if chars.get(i + 1) == Some(&'>') {
                    // `&>file`
                    p.end_word();
                    p.current.redirected = true;
                    p.expect_redirect_target = true;
                    i += 2;
                } else {
                    p.end_command();
                    i += 1;
                }
            }
            '|' => {
                p.end_command();
                i += if chars.get(i + 1) == Some(&'|') { 2 } else { 1 };
            }
            '<' if chars.get(i + 1) == Some(&'<') => {
                // A here-document. The delimiter and every body line follow on
                // subsequent lines, and this parser has no notion of them — so
                // the terminator and the body were being read as COMMANDS. A
                // heredoc whose body contains `pytest -q` would then produce a
                // command that never ran, which is invented evidence.
                //
                // Refused rather than skipped, for the module's standing
                // reason: a wrong argv costs more than a missing label, and the
                // shim records the real invocation either way.
                return Err(ShellParseError::Unexpandable {
                    construct: "a here-document",
                    at: i,
                });
            }
            '>' | '<' => {
                // A bare `2` or `1` immediately before the operator is a file
                // descriptor, not an argument; it is already in `word`.
                if p.has_word && p.word.chars().all(|c| c.is_ascii_digit()) {
                    p.word.clear();
                    p.has_word = false;
                } else {
                    p.end_word();
                }
                p.current.redirected = true;
                i += 1;
                while matches!(chars.get(i), Some('>') | Some('&')) {
                    i += 1;
                }
                p.expect_redirect_target = true;
            }
            '(' | ')' | '{' | '}' => {
                return Err(ShellParseError::Unexpandable {
                    construct: "a subshell or group",
                    at: i,
                })
            }
            '`' => {
                return Err(ShellParseError::Unexpandable {
                    construct: "a backquoted substitution",
                    at: i,
                })
            }
            '*' | '?' | '[' => {
                return Err(ShellParseError::Unexpandable {
                    construct: "a glob",
                    at: i,
                })
            }
            '$' => {
                return Err(ShellParseError::Unexpandable {
                    construct: "a parameter or command substitution",
                    at: i,
                })
            }
            '\\' => {
                let Some(next) = chars.get(i + 1) else {
                    return Err(ShellParseError::Unterminated {
                        what: "escape",
                        at: i,
                    });
                };
                if *next != '\n' {
                    p.word.push(*next);
                    p.has_word = true;
                }
                i += 2;
            }
            '\'' => {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    p.word.push(chars[i]);
                    i += 1;
                }
                if !closed {
                    return Err(ShellParseError::Unterminated {
                        what: "single quote",
                        at: start,
                    });
                }
                p.has_word = true;
            }
            '"' => {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    match chars[i] {
                        '"' => {
                            closed = true;
                            i += 1;
                            break;
                        }
                        '\\' => {
                            let Some(next) = chars.get(i + 1) else {
                                return Err(ShellParseError::Unterminated {
                                    what: "escape",
                                    at: i,
                                });
                            };
                            // Inside double quotes a backslash is literal
                            // except before these four.
                            if matches!(next, '"' | '\\' | '$' | '`') {
                                p.word.push(*next);
                            } else {
                                p.word.push('\\');
                                p.word.push(*next);
                            }
                            i += 2;
                        }
                        '$' | '`' => {
                            return Err(ShellParseError::Unexpandable {
                                construct: "a parameter or command substitution",
                                at: i,
                            })
                        }
                        other => {
                            p.word.push(other);
                            i += 1;
                        }
                    }
                }
                if !closed {
                    return Err(ShellParseError::Unterminated {
                        what: "double quote",
                        at: start,
                    });
                }
                p.has_word = true;
            }
            '#' if !p.has_word => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            other => {
                p.word.push(other);
                p.has_word = true;
                i += 1;
            }
        }
    }
    p.end_command();
    Ok(p.commands)
}

/// Tokeniser state. A struct rather than two macros: the macro form expanded
/// dead assignments at some call sites, and silencing `unused_assignments`
/// would have silenced a real class of bug in a parser whose whole job is
/// getting state transitions right.
#[derive(Default)]
struct Parser {
    commands: Vec<Command>,
    current: Command,
    word: String,
    has_word: bool,
    expect_redirect_target: bool,
}

impl Parser {
    fn end_word(&mut self) {
        if !self.has_word {
            return;
        }
        let word = std::mem::take(&mut self.word);
        if self.expect_redirect_target {
            // A redirection target is shell syntax, not an argument.
            self.expect_redirect_target = false;
        } else if self.current.argv.is_empty() && is_assignment(&word) {
            self.current.env_prefix.push(word);
        } else {
            self.current.argv.push(word);
        }
        self.has_word = false;
    }

    fn end_command(&mut self) {
        self.end_word();
        if !self.current.argv.is_empty() || !self.current.env_prefix.is_empty() {
            self.commands.push(std::mem::take(&mut self.current));
        }
        self.current = Command::default();
        self.expect_redirect_target = false;
    }
}

/// `NAME=value`, the shell's per-command environment prefix.
fn is_assignment(word: &str) -> bool {
    let Some(eq) = word.find('=') else {
        return false;
    };
    if eq == 0 {
        return false;
    }
    let name = &word[..eq];
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests;
