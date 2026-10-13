//! `claims --phases` and `GET /api/phases` carry no string of the A209 masking
//! corpus — a privacy guard, not a formatting one.
//!
//! # The defect this was written against
//!
//! The phase log is served to the Verdict Panel verbatim (`uiserve.rs`
//! `phases()` masks only stderr lines that are NOT phase objects). Two of its
//! `detail` lines carried recorded text straight past every mask:
//!
//! - `find_claims` quoted the first claim's first 80 characters, cut with a
//!   `chars().take(80)` that knew nothing of `[masked: …]` markers — so a record
//!   holding an unmasked credential would have served it, and a masked one could
//!   be cut through its marker;
//! - `workspace_since` appended a path read from `git diff-tree`, which is not
//!   captured text and was never masked at all.
//!
//! The phase log now carries claim ids and counts, never recorded text.
//!
//! # Two sessions, because there are two ways text reaches the record
//!
//! - **Captured** — through the real `thruline hook claude-code` and `fold`,
//!   with every corpus string delivered in the prompt, a tool's input and
//!   output, a file the agent wrote, and the final message. The hook masks at
//!   capture and keeps a bounded head of each field, so this session exercises
//!   the unmasked `git diff-tree` path, and holds the corpus only as far as that
//!   head reaches.
//! - **Unmasked producer** — the same product-written spool line, with its
//!   `last_assistant_message` put back to the raw text, as a spool written by a
//!   producer that did not mask would hold it. `fold` does not re-mask that
//!   field, so the ledger holds the raw claim text: this session is what makes
//!   the test fail when claim text is put back into a phase detail.
//!
//! # The corpus is the A209 corpus, not a paraphrase of it
//!
//! "The corpus" is exactly the three case lists of
//! `crates/core/tests/a209_masking_corpus.rs`: `must_mask()` (with the
//! canaries it is built from), `must_not_mask()` and `EXPECTED_EXCEPTIONS`.
//! Every string of all three is delivered in both sessions' final message, and
//! in the captured session's prompt, tool input and output and written files
//! too. The unmasked producer's record holds every one of them whole (a premise
//! checks it); the captured record holds only the head its capture keeps. A
//! phase surface fails the test when it carries one of them whole or any line
//! of one, a `TLCANARY` fragment, or the [`PROSE`] sentinel. The bare
//! credential prefixes inside a209's live-gate test are gate inputs, not
//! corpus cases, and are not checked: a prefix such as `sk-` is a substring of
//! ordinary words, including some of the must-not-mask cases.
//!
//! The lists cannot be imported (they live in another crate's integration
//! test), so all three are read from a209's own source when the test runs,
//! and no copy of any corpus string is kept here:
//!
//! - **must-mask** is code (`format!` over canary `const`s and `fn`s), so
//!   [`a209_must_mask_half`] evaluates it, in the little Rust it is written in
//!   and nothing more; the two canaries this file names on its own come from
//!   the same source through [`canary`]. The reader errors on a construct it
//!   does not know, and its count is held to the case id literals of a209's
//!   `must_mask()`.
//! - **must-not-mask and the named exceptions** are plain literals, so
//!   [`a209_unmasked_half`] decodes them line by line. That reader panics on a
//!   case line it cannot decode, and refuses a case that is not on a line of
//!   its own.
//!
//! The must-not-mask strings inside that head are what make the captured
//! session fail on its own: no mask touches them, so they sit whole in that
//! record (a premise checks that at least one does).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};
use thruline_core::redact::mask;
use thruline_ledger::{CheckRow, Ledger};

const CAPTURED: &str = "aaaaaaaa-bbbb-4ccc-8ddd-000000000001";
const UNMASKED: &str = "aaaaaaaa-bbbb-4ccc-8ddd-000000000002";
/// The same record as [`UNMASKED`] with every must-mask string neutralised —
/// the control for "masking changed the words and nothing else".
const CONTROL: &str = "aaaaaaaa-bbbb-4ccc-8ddd-000000000003";
const PLACEHOLDER: &str = "THRULINE-PLACEHOLDER-MESSAGE";
/// Ordinary claim prose that no mask touches. It can reach a phase line only as
/// claim text, so its absence shows the log carries none, masked or not.
const PROSE: &str = "the parcel rate now rounds to two places for every weight band";

/* ── the must-mask half, evaluated from a209's source ────────────────── */

/// One token of the little Rust a209's must-mask half is written in. A char
/// literal is read as the one-character string it formats as.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Str(String),
    Word(String),
    Punct(char),
}

/// Rust source → [`Tok`]s: whitespace and comments dropped, string and char
/// literals decoded, a `\` line continuation included. An escape plain Rust
/// string syntax does not have, or a char literal that is not one plain
/// character, is an error, never a guess. A raw or byte string reads as a name
/// followed by a string, which no expression accepts.
fn rust_tokens(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let word_char = |c: &char| c.is_alphanumeric() || *c == '_';
    let mut toks = Vec::new();
    let mut i = 0;
    while let Some(c) = at(i) {
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && at(i + 1) == Some('/') {
            while at(i).is_some_and(|c| c != '\n') {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == Some('*') {
            i += 2;
            loop {
                match (at(i), at(i + 1)) {
                    (Some('*'), Some('/')) => break,
                    (Some(_), _) => i += 1,
                    (None, _) => return Err("an unterminated block comment".into()),
                }
            }
            i += 2;
        } else if c == '"' {
            let mut s = String::new();
            i += 1;
            loop {
                let ch = at(i).ok_or("an unterminated string literal")?;
                i += 1;
                match ch {
                    '"' => break,
                    '\\' => {
                        let escape = at(i).ok_or("an unterminated escape")?;
                        i += 1;
                        match escape {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            '0' => s.push('\0'),
                            '\\' | '"' | '\'' => s.push(escape),
                            '\n' => {
                                while at(i).is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
                                    i += 1;
                                }
                            }
                            'u' if at(i) == Some('{') => {
                                let len = chars[i..]
                                    .iter()
                                    .position(|c| *c == '}')
                                    .ok_or("an unterminated \\u escape")?;
                                let hex: String = chars[i + 1..i + len].iter().collect();
                                let decoded = u32::from_str_radix(&hex, 16)
                                    .ok()
                                    .and_then(char::from_u32)
                                    .ok_or_else(|| format!("the escape \\u{{{hex}}}"))?;
                                s.push(decoded);
                                i += len + 1;
                            }
                            other => return Err(format!("the escape \\{other}")),
                        }
                    }
                    ch => s.push(ch),
                }
            }
            toks.push(Tok::Str(s));
        } else if c == '\'' {
            match (at(i + 1), at(i + 2)) {
                (Some(ch), Some('\'')) if ch != '\\' => {
                    toks.push(Tok::Str(ch.to_string()));
                    i += 3;
                }
                // A lifetime (`&'static str`): part of a type, which is skipped.
                (Some(ch), _) if ch.is_alphabetic() => {
                    let mut w = String::from("'");
                    i += 1;
                    while let Some(ch) = at(i).filter(word_char) {
                        w.push(ch);
                        i += 1;
                    }
                    toks.push(Tok::Word(w));
                }
                _ => return Err("a char literal this reader cannot read".into()),
            }
        } else if word_char(&c) {
            let mut w = String::new();
            while let Some(ch) = at(i).filter(word_char) {
                w.push(ch);
                i += 1;
            }
            toks.push(Tok::Word(w));
        } else {
            toks.push(Tok::Punct(c));
            i += 1;
        }
    }
    Ok(toks)
}

/// A value a209's must-mask half computes.
#[derive(Clone, Debug, PartialEq)]
enum Val {
    Str(String),
    Tuple(Vec<Val>),
    List(Vec<Val>),
}

type Env = BTreeMap<String, Val>;

fn text(v: Val) -> Result<String, String> {
    match v {
        Val::Str(s) => Ok(s),
        other => Err(format!("{other:?} where text is needed")),
    }
}

fn punct(c: char) -> Tok {
    Tok::Punct(c)
}

fn word(w: &str) -> Tok {
    Tok::Word(w.to_string())
}

struct Cursor<'t> {
    toks: &'t [Tok],
    at: usize,
}

impl<'t> Cursor<'t> {
    fn done(&self) -> bool {
        self.at >= self.toks.len()
    }

    fn peek(&self, ahead: usize) -> Option<&Tok> {
        self.toks.get(self.at + ahead)
    }

    fn bump(&mut self) -> Result<Tok, String> {
        let tok = self.peek(0).cloned().ok_or("source that ends mid-item")?;
        self.at += 1;
        Ok(tok)
    }

    fn is(&self, want: &Tok) -> bool {
        self.peek(0) == Some(want)
    }

    fn eat(&mut self, want: &Tok) -> bool {
        let hit = self.is(want);
        if hit {
            self.at += 1;
        }
        hit
    }

    fn expect(&mut self, want: &Tok) -> Result<(), String> {
        if self.eat(want) {
            Ok(())
        } else {
            Err(format!("{want:?} where the source has {:?}", self.peek(0)))
        }
    }

    fn name(&mut self) -> Result<String, String> {
        match self.bump()? {
            Tok::Word(w) => Ok(w),
            other => Err(format!("a name where the source has {other:?}")),
        }
    }

    /// Past everything up to `stop`: a type, which the reader does not need.
    fn skip_to(&mut self, stop: &[Tok]) -> Result<(), String> {
        while !stop.iter().any(|s| self.is(s)) {
            self.bump()?;
        }
        Ok(())
    }

    /// The tokens inside the `{ … }` that starts here.
    fn braced(&mut self) -> Result<&'t [Tok], String> {
        self.expect(&punct('{'))?;
        let start = self.at;
        let mut depth = 1;
        while depth > 0 {
            match self.bump()? {
                Tok::Punct('{') => depth += 1,
                Tok::Punct('}') => depth -= 1,
                _ => {}
            }
        }
        let toks = self.toks;
        Ok(&toks[start..self.at - 1])
    }
}

/// The `const`s and `fn`s of a span of Rust source, evaluated on request.
#[derive(Default)]
struct RustScope {
    consts: BTreeMap<String, String>,
    fns: BTreeMap<String, (Vec<String>, Vec<Tok>)>,
}

impl RustScope {
    /// `const NAME: &str = …;` and `fn name(arg: T, …) -> T { … }` items; any
    /// other item is an error.
    fn read(src: &str) -> Result<RustScope, String> {
        let toks = rust_tokens(src)?;
        let mut cur = Cursor { toks: &toks, at: 0 };
        let mut scope = RustScope::default();
        while !cur.done() {
            if cur.eat(&word("const")) {
                let name = cur.name()?;
                cur.skip_to(&[punct('=')])?;
                cur.expect(&punct('='))?;
                let value = text(scope.expr(&mut cur, &Env::new())?)?;
                cur.expect(&punct(';'))?;
                scope.consts.insert(name, value);
            } else if cur.eat(&word("fn")) {
                let name = cur.name()?;
                cur.expect(&punct('('))?;
                let mut params = Vec::new();
                while !cur.eat(&punct(')')) {
                    params.push(cur.name()?);
                    cur.expect(&punct(':'))?;
                    cur.skip_to(&[punct(','), punct(')')])?;
                    cur.eat(&punct(','));
                }
                cur.skip_to(&[punct('{')])?;
                let body = cur.braced()?.to_vec();
                scope.fns.insert(name, (params, body));
            } else {
                return Err(format!(
                    "an item this reader cannot read, at {:?}",
                    cur.peek(0)
                ));
            }
        }
        Ok(scope)
    }

    /// `fn must_mask()`'s cases, as (id, input, detector).
    fn must_mask(&self) -> Result<Vec<(String, String, String)>, String> {
        let Val::List(cases) = self.call("must_mask", Vec::new())? else {
            return Err("must_mask() is not a list".into());
        };
        cases
            .into_iter()
            .map(|case| match case {
                Val::Tuple(parts) => match <[Val; 3]>::try_from(parts) {
                    Ok([id, input, detector]) => Ok((text(id)?, text(input)?, text(detector)?)),
                    Err(parts) => Err(format!(
                        "a case that is not (id, input, detector): {parts:?}"
                    )),
                },
                other => Err(format!("a case that is not a tuple: {other:?}")),
            })
            .collect()
    }

    /// One expression, evaluated in this scope: `eval("gh('p')")`.
    fn eval(&self, expr: &str) -> Result<String, String> {
        text(self.block(&rust_tokens(expr)?, &mut Env::new())?)
    }

    fn call(&self, name: &str, args: Vec<Val>) -> Result<Val, String> {
        let (params, body) = self
            .fns
            .get(name)
            .ok_or_else(|| format!("the fn `{name}`, which the source does not define"))?;
        if params.len() != args.len() {
            return Err(format!(
                "`{name}` called with {} argument(s), not {}",
                args.len(),
                params.len()
            ));
        }
        let mut env: Env = params.iter().cloned().zip(args).collect();
        self.block(body, &mut env)
    }

    /// A block's statements (`let`, `for (…) in … { … }`, `name.push(…);`),
    /// then its value: the trailing expression, or `()` when there is none.
    fn block(&self, toks: &[Tok], env: &mut Env) -> Result<Val, String> {
        let mut cur = Cursor { toks, at: 0 };
        while !cur.done() {
            if cur.eat(&word("let")) {
                cur.eat(&word("mut"));
                let name = cur.name()?;
                cur.skip_to(&[punct('=')])?;
                cur.expect(&punct('='))?;
                let value = self.expr(&mut cur, env)?;
                cur.expect(&punct(';'))?;
                env.insert(name, value);
            } else if cur.eat(&word("for")) {
                cur.expect(&punct('('))?;
                let mut names = vec![cur.name()?];
                while cur.eat(&punct(',')) {
                    names.push(cur.name()?);
                }
                cur.expect(&punct(')'))?;
                cur.expect(&word("in"))?;
                let Val::List(items) = self.expr(&mut cur, env)? else {
                    return Err("a for loop over something that is not a list".into());
                };
                let body = cur.braced()?;
                for item in items {
                    let parts = match item {
                        Val::Tuple(parts) if parts.len() == names.len() => parts,
                        other => return Err(format!("{other:?} bound to {names:?}")),
                    };
                    let mut inner = env.clone();
                    inner.extend(names.iter().cloned().zip(parts));
                    self.block(body, &mut inner)?;
                    // What the body pushed onto an outer list stays; the
                    // loop's own bindings do not.
                    for (name, value) in inner {
                        if env.contains_key(&name) && !names.contains(&name) {
                            env.insert(name, value);
                        }
                    }
                }
            } else if matches!(
                (cur.peek(0), cur.peek(1), cur.peek(2)),
                (Some(Tok::Word(_)), Some(Tok::Punct('.')), Some(Tok::Word(m))) if m == "push"
            ) {
                let name = cur.name()?;
                cur.at += 2;
                cur.expect(&punct('('))?;
                let value = self.expr(&mut cur, env)?;
                cur.expect(&punct(')'))?;
                cur.expect(&punct(';'))?;
                match env.get_mut(&name) {
                    Some(Val::List(list)) => list.push(value),
                    _ => return Err(format!("`{name}.push` on something that is not a list")),
                }
            } else {
                let value = self.expr(&mut cur, env)?;
                if !cur.done() {
                    return Err(format!(
                        "a statement this reader cannot read, at {:?}",
                        cur.peek(0)
                    ));
                }
                return Ok(value);
            }
        }
        Ok(Val::Tuple(Vec::new()))
    }

    /// A string or char literal, a name, a call, `format!(…)`, a tuple, `[…]`
    /// or `vec![…]`, then any `.to_string()` or `.clone()` of text.
    fn expr(&self, cur: &mut Cursor<'_>, env: &Env) -> Result<Val, String> {
        let value = match cur.bump()? {
            Tok::Str(s) => Val::Str(s),
            Tok::Punct('(') => Val::Tuple(self.list(cur, env, ')')?),
            Tok::Punct('[') => Val::List(self.list(cur, env, ']')?),
            Tok::Word(name) if cur.eat(&punct('!')) => match name.as_str() {
                "vec" => {
                    cur.expect(&punct('['))?;
                    Val::List(self.list(cur, env, ']')?)
                }
                "format" => {
                    cur.expect(&punct('('))?;
                    let mut args = self.list(cur, env, ')')?.into_iter();
                    let template = text(args.next().ok_or("format!() with no template")?)?;
                    Val::Str(self.format(&template, &mut args, env)?)
                }
                other => return Err(format!("the macro `{other}!`")),
            },
            Tok::Word(name) if cur.eat(&punct('(')) => {
                let args = self.list(cur, env, ')')?;
                self.call(&name, args)?
            }
            Tok::Word(name) => self.lookup(&name, env)?,
            other => return Err(format!("an expression this reader cannot read: {other:?}")),
        };
        while cur.eat(&punct('.')) {
            let method = cur.name()?;
            cur.expect(&punct('('))?;
            cur.expect(&punct(')'))?;
            match (method.as_str(), &value) {
                ("to_string" | "clone", Val::Str(_)) => {}
                _ => return Err(format!("the method `.{method}()` on {value:?}")),
            }
        }
        Ok(value)
    }

    /// Comma-separated expressions up to `close`, a trailing comma allowed.
    fn list(&self, cur: &mut Cursor<'_>, env: &Env, close: char) -> Result<Vec<Val>, String> {
        let mut items = Vec::new();
        while !cur.eat(&punct(close)) {
            items.push(self.expr(cur, env)?);
            if !cur.eat(&punct(',')) {
                cur.expect(&punct(close))?;
                break;
            }
        }
        Ok(items)
    }

    fn lookup(&self, name: &str, env: &Env) -> Result<Val, String> {
        env.get(name)
            .cloned()
            .or_else(|| self.consts.get(name).cloned().map(Val::Str))
            .ok_or_else(|| format!("the name `{name}`, which this reader does not know"))
    }

    /// A `format!` template: `{{`, `}}`, `{name}` and `{}`, with every
    /// argument used. A format spec (`{:?}`) or an index (`{0}`) is an error.
    fn format(
        &self,
        template: &str,
        args: &mut impl Iterator<Item = Val>,
        env: &Env,
    ) -> Result<String, String> {
        let mut out = String::new();
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            if (c == '{' || c == '}') && chars.next_if_eq(&c).is_some() {
                out.push(c);
            } else if c == '{' {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(ch) if ch.is_alphanumeric() || ch == '_' => name.push(ch),
                        _ => return Err(format!("a format spec in {template:?}")),
                    }
                }
                if name.starts_with(|ch: char| ch.is_ascii_digit()) {
                    return Err(format!("an argument index in {template:?}"));
                }
                let value = if name.is_empty() {
                    args.next()
                        .ok_or_else(|| format!("{template:?} with fewer arguments than `{{}}`"))?
                } else {
                    self.lookup(&name, env)?
                };
                out.push_str(&text(value)?);
            } else if c == '}' {
                return Err(format!("a lone `}}` in {template:?}"));
            } else {
                out.push(c);
            }
        }
        if args.next().is_some() {
            return Err(format!("{template:?} with an argument it does not use"));
        }
        Ok(out)
    }
}

/// a209's canary section and `fn must_mask()`: its source from the canary
/// banner up to `fn must_not_mask()`.
fn a209_must_mask_source() -> &'static str {
    let a209 = include_str!("../../core/tests/a209_masking_corpus.rs");
    let start = a209
        .find("/* ── shape-derived canaries")
        .expect("a209 has its canary section");
    let end = a209
        .find("fn must_not_mask()")
        .expect("a209 has must_not_mask()");
    &a209[start..end]
}

fn a209_scope() -> RustScope {
    RustScope::read(a209_must_mask_source()).unwrap_or_else(|e| {
        panic!("a209's canaries and must_mask(), which this reader cannot read: {e}")
    })
}

/// a209's must-mask cases, as (id, input, detector): its `fn must_mask()`,
/// evaluated from source by [`RustScope`]. A construct the reader does not
/// know panics, and the count is held to every case id literal (`"M17",`) in
/// that source, wherever it sits, so a case the evaluation does not reach
/// fails here instead of dropping out of the check unseen.
fn a209_must_mask_half() -> Vec<(String, String, String)> {
    let cases = a209_scope()
        .must_mask()
        .unwrap_or_else(|e| panic!("a209's must_mask(), which this reader cannot evaluate: {e}"));
    assert_eq!(
        cases.len(),
        case_id_literals(a209_must_mask_source(), "M"),
        "the reader evaluated a different number of must-mask cases than a209 defines"
    );
    cases
}

/// A canary of a209's, evaluated from its source (`canary("gh('p')")`): the
/// corpus strings this file uses on its own are a209's too.
fn canary(expr: &str) -> String {
    a209_scope()
        .eval(expr)
        .unwrap_or_else(|e| panic!("the a209 canary {expr}: {e}"))
}

/// Every case id literal with this prefix (`"N17",`, `"M03u",`) in `span`,
/// wherever it sits.
fn case_id_literals(span: &str, prefix: &str) -> usize {
    let quoted = format!("\"{prefix}");
    span.match_indices(&quoted)
        .filter(|(at, _)| {
            let rest = &span[at + quoted.len()..];
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let tail = &rest[digits..];
            let letters = tail.bytes().take_while(u8::is_ascii_lowercase).count();
            digits > 0 && tail[letters..].starts_with("\",")
        })
        .count()
}

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        let fx = Fixture { dir };
        std::fs::create_dir_all(fx.repo()).expect("repo");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "a@e.invalid"],
            vec!["config", "user.name", "A"],
        ] {
            fx.git(&args);
        }
        std::fs::create_dir_all(fx.repo().join("src")).expect("src");
        std::fs::write(fx.repo().join("src/lib.rs"), "pub fn rate() -> u32 { 4 }\n").expect("w");
        fx.git(&["add", "-A"]);
        fx.git(&["commit", "-qm", "baseline"]);
        fx
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }

    fn thruline_home(&self) -> PathBuf {
        self.home().join(".thruline")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(self.repo())
            .args(args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }

    fn env(&self, cmd: &mut Command) {
        cmd.env("HOME", self.home())
            .env("THRULINE_HOME", self.thruline_home())
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.repo());
    }

    /// One delivery through the real hook. Built as JSON, not formatted into a
    /// string, because the corpus holds quotes, backslashes and newlines.
    fn hook(&self, session: &str, event: &str, fields: Value) {
        let mut delivery = json!({
            "hook_event_name": event,
            "session_id": session,
            "transcript_path": "/t",
            "cwd": self.repo().display().to_string(),
        });
        for (k, v) in fields.as_object().expect("fields").clone() {
            delivery[k] = v;
        }
        let mut cmd = Command::new(thruline());
        cmd.args(["hook", "claude-code"]);
        self.env(&mut cmd);
        let mut child = cmd.stdin(Stdio::piped()).spawn().expect("hook");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(delivery.to_string().as_bytes())
            .expect("write");
        assert!(child.wait().expect("wait").success(), "hook exited nonzero");
    }

    fn cli(&self, args: &[&str]) -> (Option<i32>, Vec<u8>, Vec<u8>) {
        let mut cmd = Command::new(thruline());
        cmd.args(args);
        self.env(&mut cmd);
        let out = cmd.output().expect("cli");
        (out.status.code(), out.stdout, out.stderr)
    }

    fn fold(&self) {
        let (code, _, err) = self.cli(&["fold"]);
        assert_eq!(code, Some(0), "fold: {}", String::from_utf8_lossy(&err));
    }

    fn spool(&self) -> PathBuf {
        self.thruline_home().join("spool").join("claude-code.jsonl")
    }

    fn ledger(&self) -> Ledger {
        Ledger::open(&self.thruline_home().join("exec.db")).expect("open ledger")
    }

    fn last_snapshot_tree(&self, session: &str) -> String {
        self.ledger()
            .session_events(session)
            .expect("events")
            .iter()
            .filter(|e| e.kind().as_str().as_str() == "snapshot")
            .filter_map(|e| {
                e.payload()
                    .get("tree_hash")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .next_back()
            .expect("a snapshot")
    }

    fn stop_message(&self, session: &str) -> String {
        self.ledger()
            .session_events(session)
            .expect("events")
            .iter()
            .filter(|e| e.kind().as_str().as_str() == "stop")
            .filter_map(|e| {
                e.payload()
                    .get("last_assistant_message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .next_back()
            .expect("a stop event")
    }

    fn ui(&self) -> Command {
        let mut cmd = Command::new(thruline());
        cmd.args(["ui", "--no-open", "--port", "0", "--dir"])
            .arg(self.dir.path().join("export"));
        self.env(&mut cmd);
        cmd
    }
}

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(mut cmd: Command) -> Server {
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut seen = String::new();
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    seen.push_str(&line);
                    seen.push('\n');
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .unwrap_or_default()
                            .parse()
                            .expect("port in the printed URL");
                        return Server { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut err = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = e.read_to_string(&mut err);
                    }
                    panic!(
                        "`thruline ui` never printed a loopback URL.\nstdout:\n{seen}\nstderr:\n{err}"
                    );
                }
            }
        }
    }

    /// (status, body) of a GET.
    fn get(&self, path: &str) -> (u16, Vec<u8>) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .expect("HTTP status");
        (status, raw[split + 4..].to_vec())
    }
}

/// Every string a JSON value holds, keys included, decoded — a secret escaped
/// by the serializer (`\"`, `\n`) is found here where a raw search would miss it.
fn strings_of(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        Value::Array(a) => a.iter().for_each(|x| strings_of(x, out)),
        Value::Object(o) => {
            for (k, x) in o {
                out.push_str(k);
                out.push('\n');
                strings_of(x, out);
            }
        }
        other => {
            out.push_str(&other.to_string());
            out.push('\n');
        }
    }
}

/// The decoded text of a `--phases` stderr: each line decoded when it is JSON,
/// kept raw when it is not.
fn decoded_stderr(stderr: &[u8]) -> String {
    let mut out = String::new();
    for line in String::from_utf8_lossy(stderr).lines() {
        match serde_json::from_str::<Value>(line) {
            Ok(v) => strings_of(&v, &mut out),
            Err(_) => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// a209's must-not-mask cases and its named exceptions, as (id, input), read
/// from a209's own source: every `("N…", "…"),` line from `fn must_not_mask()`
/// to a209's first `#[test]`, which is where `EXPECTED_EXCEPTIONS` ends. A case
/// line [`decode_case`] cannot read panics, and the count is held to every
/// case id literal (`"N17",`) in that span, wherever it sits, so a case written
/// some other way — split over lines, say — fails here instead of dropping out
/// of the check unseen.
fn a209_unmasked_half() -> Vec<(String, String)> {
    let a209 = include_str!("../../core/tests/a209_masking_corpus.rs");
    let start = a209
        .find("fn must_not_mask()")
        .expect("a209 has must_not_mask()");
    let span = &a209[start..];
    let span = &span[..span
        .find("#[test]")
        .expect("a209's first test ends its corpus")];
    let cases: Vec<(String, String)> = span
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("(\"N"))
        .map(|line| {
            decode_case(line)
                .unwrap_or_else(|| panic!("an a209 case line this reader cannot decode: {line}"))
        })
        .collect();
    assert_eq!(
        cases.len(),
        case_id_literals(span, "N"),
        "an a209 must-not-mask or exception case is not on a line of its own"
    );
    cases
}

/// `("N01", "text"),` → `("N01", text)`, decoding the escapes a plain Rust
/// string literal may hold. A raw string, an unknown escape, an unterminated
/// literal or anything after `),` is `None`.
fn decode_case(line: &str) -> Option<(String, String)> {
    let (id, rest) = line.strip_prefix("(\"")?.split_once("\", \"")?;
    let mut text = String::new();
    let mut chars = rest.chars();
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => match chars.next()? {
                'n' => text.push('\n'),
                't' => text.push('\t'),
                'r' => text.push('\r'),
                '0' => text.push('\0'),
                '\\' => text.push('\\'),
                '"' => text.push('"'),
                '\'' => text.push('\''),
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let hex: String = chars.by_ref().take_while(|c| *c != '}').collect();
                    text.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => text.push(c),
        }
    }
    (chars.as_str() == "),").then(|| (id.to_string(), text))
}

/// Every corpus case, as (id, input): must-mask, then must-not-mask and the
/// named exceptions.
fn every_corpus_case() -> Vec<(String, String)> {
    a209_must_mask_half()
        .into_iter()
        .map(|(id, input, _)| (id, input))
        .chain(a209_unmasked_half())
        .collect()
}

/// A corpus string whole, then each of its lines: a claim is one line of the
/// final message, so a multi-line case can reach a claim only line by line.
fn whole_and_lines(s: &str) -> Vec<&str> {
    let mut parts = vec![s];
    parts.extend(s.lines().filter(|l| !l.trim().is_empty() && *l != s));
    parts
}

/// What a phase surface must never carry, as the ids of what it does: any
/// corpus string whole or any line of one, a `TLCANARY` fragment (a cut secret
/// is not the whole corpus string, and it is still a leak), or [`PROSE`]. The
/// phase log carries no recorded text, masked or not.
fn recorded_text_hits(surface: &str, decoded: &str, raw: &str) -> Vec<String> {
    let found = |s: &str| decoded.contains(s) || raw.contains(s);
    let mut hits: Vec<String> = every_corpus_case()
        .into_iter()
        .filter(|(_, input)| whole_and_lines(input).into_iter().any(found))
        .map(|(id, _)| format!("{surface}: {id}"))
        .collect();
    if found("TLCANARY") {
        hits.push(format!("{surface}: a TLCANARY fragment"));
    }
    if found(PROSE) {
        hits.push(format!("{surface}: claim text (the prose sentinel)"));
    }
    hits
}

/// What masked claim text must never carry: a must-mask string whole, or a
/// `TLCANARY` fragment. Its must-not-mask strings are meant to survive a mask.
fn secret_hits(surface: &str, text: &str) -> Vec<String> {
    let mut hits: Vec<String> = a209_must_mask_half()
        .into_iter()
        .filter(|(_, input, _)| text.contains(input.as_str()))
        .map(|(id, _, _)| format!("{surface}: {id}"))
        .collect();
    if text.contains("TLCANARY") {
        hits.push(format!("{surface}: a TLCANARY fragment"));
    }
    hits
}

/// The final message both sessions stop with: a leading claim whose first 80
/// characters hold a whole canary token, a claim carrying [`PROSE`], then every
/// corpus string, one claim each. The must-mask cases and the unmasked half
/// alternate, because capture keeps only a head of the message
/// (`FIELD_BOUND` in `crates/capture/src/claude_code.rs`): alternating puts
/// cases of both kinds in that head.
///
/// The third line is a PROMISE carrying a canary. `promises[].text` is served
/// by both projections (requests 11 and 16), and without a promise that holds
/// something maskable those two assertions would pass on an empty array.
fn final_message() -> String {
    let token = canary("gh('p')");
    let mut m = format!("All tests pass with {token}.\n");
    m.push_str(&format!("All tests pass and {PROSE}.\n"));
    m.push_str(&format!("I'll rotate {token} in a follow-up.\n"));
    let secrets: Vec<(String, String)> = a209_must_mask_half()
        .into_iter()
        .map(|(id, input, _)| (id, input))
        .collect();
    let unmasked = a209_unmasked_half();
    for i in 0..secrets.len().max(unmasked.len()) {
        for (id, input) in [secrets.get(i), unmasked.get(i)].into_iter().flatten() {
            m.push_str(&format!("{id}: all tests pass with {input}\n"));
        }
    }
    m
}

fn every_corpus_string() -> String {
    every_corpus_case()
        .into_iter()
        .map(|(_, input)| input)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The unmasked-producer session: the product's own spool lines, with the
/// `Stop` line's message put back to the raw text.
fn record_the_unmasked_producer_session(fx: &Fixture) {
    fx.hook(UNMASKED, "SessionStart", json!({"source": "startup"}));
    fx.hook(
        UNMASKED,
        "Stop",
        json!({"last_assistant_message": PLACEHOLDER, "stop_hook_active": false}),
    );
    let spool = std::fs::read_to_string(fx.spool()).expect("spool");
    let mut rewrote = 0;
    let lines: Vec<String> = spool
        .lines()
        .map(|line| {
            let mut v: Value = serde_json::from_str(line).expect("spool line is JSON");
            if v["payload"]["last_assistant_message"] == json!(PLACEHOLDER) {
                v["payload"]["last_assistant_message"] = json!(final_message());
                rewrote += 1;
            }
            v.to_string()
        })
        .collect();
    assert_eq!(rewrote, 1, "the placeholder Stop line was not in the spool");
    std::fs::write(fx.spool(), lines.join("\n") + "\n").expect("rewrite spool");
    fx.fold();
    let stop = fx.stop_message(UNMASKED);
    let missing = every_corpus_case()
        .iter()
        .filter(|(_, input)| !stop.contains(input.as_str()))
        .count();
    assert!(
        stop.contains(&canary("gh('p')")) && missing == 0,
        "premise: this session's record must hold the RAW claim text, every corpus string \
         whole, or it cannot fail ({missing} missing)"
    );
}

/// The captured session: every corpus string through the real hook, and a
/// cited check whose tree differs from the final one, so `workspace_since`
/// has changed paths to describe — two of them named with corpus strings.
fn record_the_captured_session(fx: &Fixture) {
    let corpus = every_corpus_string();
    fx.hook(CAPTURED, "SessionStart", json!({"source": "startup"}));
    fx.hook(CAPTURED, "UserPromptSubmit", json!({ "prompt": corpus }));
    std::fs::write(fx.repo().join("src/lib.rs"), "pub fn rate() -> u32 { 5 }\n").expect("w");
    fx.hook(
        CAPTURED,
        "PostToolUse",
        json!({"tool_name": "Edit", "tool_input": {"file_path": "src/lib.rs"},
               "tool_response": {"success": true}, "tool_use_id": "toolu_1"}),
    );
    fx.fold();
    let tree_at_check = fx.last_snapshot_tree(CAPTURED);

    let named = [
        format!("secrets_{}.txt", canary("gh('p')")),
        format!("config/key_{}.env", canary("AWS")),
    ];
    std::fs::create_dir_all(fx.repo().join("config")).expect("config");
    for (i, path) in named.iter().enumerate() {
        std::fs::write(fx.repo().join(path), &corpus).expect("w");
        fx.hook(
            CAPTURED,
            "PostToolUse",
            json!({"tool_name": "Write", "tool_input": {"file_path": path, "content": corpus},
                   "tool_response": {"success": true}, "tool_use_id": format!("toolu_w{i}")}),
        );
    }
    fx.hook(
        CAPTURED,
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_input": {"command": format!("cat {}", named[0])},
               "tool_response": {"stdout": corpus, "stderr": corpus}, "tool_use_id": "toolu_b"}),
    );
    fx.hook(
        CAPTURED,
        "Stop",
        json!({"last_assistant_message": final_message(), "stop_hook_active": false}),
    );
    fx.fold();
    assert_ne!(
        fx.last_snapshot_tree(CAPTURED),
        tree_at_check,
        "premise: the corpus-named writes must move the tree"
    );
    let stop = fx.stop_message(CAPTURED);
    assert!(
        !stop.contains("TLCANARY"),
        "premise: the captured session is masked at capture"
    );
    // …and still holds recorded text no mask touches, so claim text put back
    // into a phase line fails this session on its own, without the unmasked
    // producer's record or the fragment and prose checks.
    let unmasked_half = a209_unmasked_half();
    let whole = unmasked_half
        .iter()
        .filter(|(_, input)| stop.contains(input.as_str()))
        .count();
    assert!(
        stop.contains(PROSE) && whole > 0,
        "premise: the captured record holds unmasked corpus text \
         ({whole} of {} must-not-mask or exception strings whole)",
        unmasked_half.len()
    );
    // A cited check at the earlier tree, recorded as `thruline reexec` records
    // one (no `exec` event), so the final tree differs from its input tree.
    let row = CheckRow {
        id: "chk_masking_fixture".to_string(),
        session_id: CAPTURED.to_string(),
        exec_event_id: String::new(),
        category: "test".to_string(),
        runner: "pytest".to_string(),
        argv_hash: "sha256:0".to_string(),
        cwd_hash: "sha256:0".to_string(),
        env_hash: "sha256:0".to_string(),
        input_tree_hash: tree_at_check,
        exit_code: 0,
        parsed: json!({"tests_total": 1, "passed": 1, "failed": 0, "skipped": 0, "xfail": 0})
            .to_string(),
    };
    assert_eq!(fx.ledger().insert_check(&row).expect("insert"), 1);
}

#[test]
fn the_unmasked_half_is_read_whole_from_a209() {
    let cases = a209_unmasked_half();
    let mut ids: Vec<&str> = cases.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), cases.len(), "an a209 case id was read twice");
    // a209's own floor for must_not_mask(), plus its two named exceptions.
    assert!(cases.len() >= 76, "read {} a209 cases", cases.len());
    for exception in ["N62", "N63"] {
        assert!(
            ids.contains(&exception),
            "the named exception {exception} was not read"
        );
    }
}

#[test]
fn the_a209_case_reader_decodes_rust_escapes_and_refuses_the_rest() {
    let read = |line: &str| decode_case(line);
    assert_eq!(
        read(r#"("X1", "a\"b\nc\u{2014}d\\e\u{b7}f"),"#),
        Some(("X1".to_string(), "a\"b\nc\u{2014}d\\e\u{b7}f".to_string()))
    );
    assert_eq!(
        read(r#"("X2", "plain, with a comma"),"#).map(|c| c.1),
        Some("plain, with a comma".to_string())
    );
    for refused in [
        r#"("X3", "unknown \q escape"),"#,
        r#"("X4", r"raw string"),"#,
        r#"("X5", "unterminated"#,
        r#"("X6", "trailing"), // note"#,
        r#"("X7", "bad \u2014 escape"),"#,
    ] {
        assert_eq!(read(refused), None, "{refused}");
    }
}

#[test]
fn the_must_mask_half_is_read_whole_from_a209() {
    let cases = a209_must_mask_half();
    let mut ids: Vec<&str> = cases.iter().map(|(id, _, _)| id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), cases.len(), "an a209 case id was read twice");
    // a209's own floor for must_mask().
    assert!(cases.len() >= 76, "read {} a209 cases", cases.len());
    for (id, input, detector) in &cases {
        assert!(
            !input.is_empty() && !detector.is_empty(),
            "{id} was read empty"
        );
    }
}

#[test]
fn the_must_mask_reader_evaluates_a209s_rust_and_refuses_the_rest() {
    let src = r#"
        /* ── a banner ── */
        const BODY: &str = "b\u{b7}d\"y";
        fn wrap(kind: &str, sep: char) -> String {
            format!("{kind}{sep}{BODY}")
        }
        /// Every case.
        fn must_mask() -> Vec<(&'static str, String, &'static str)> {
            let mut v: Vec<(&'static str, String, &'static str)> = vec![
                ("X1", wrap("k", '-'), "d1"),
                (
                    "X2",
                    "a \
                     b\n".to_string(),
                    "d2",
                ),
            ];
            let g = wrap("z", '_');
            for (id, inner, name) in [("X3", BODY.to_string(), "d3"), ("X4", g.clone(), "d4")] {
                v.push((id, format!("{{{}}}/{inner}", 'c'), name));
            }
            v.push(("X5", g, "d5"));
            v
        }
    "#;
    let case = |id: &str, input: &str, detector: &str| {
        (id.to_string(), input.to_string(), detector.to_string())
    };
    assert_eq!(
        RustScope::read(src).and_then(|scope| scope.must_mask()),
        Ok(vec![
            case("X1", "k-b\u{b7}d\"y", "d1"),
            case("X2", "a b\n", "d2"),
            case("X3", "{c}/b\u{b7}d\"y", "d3"),
            case("X4", "{c}/z_b\u{b7}d\"y", "d4"),
            case("X5", "z_b\u{b7}d\"y", "d5"),
        ])
    );
    assert_eq!(case_id_literals(src, "X"), 5);
    assert_eq!(
        RustScope::read(src).and_then(|scope| scope.eval("wrap(\"q\", '.')")),
        Ok("q.b\u{b7}d\"y".to_string())
    );
    for (label, body) in [
        ("an unknown name", r#"vec![("X1", NOPE, "d")]"#),
        ("a raw string", r#"vec![("X1", r"raw", "d")]"#),
        (
            "an unknown escape",
            r#"vec![("X1", "a\qb".to_string(), "d")]"#,
        ),
        (
            "a format spec",
            r#"vec![("X1", format!("{:?}", "a"), "d")]"#,
        ),
        (
            "an argument index",
            r#"vec![("X1", format!("{0}", "a"), "d")]"#,
        ),
        (
            "an unused argument",
            r#"vec![("X1", format!("a", "b"), "d")]"#,
        ),
        ("a missing argument", r#"vec![("X1", format!("{}"), "d")]"#),
        (
            "an unknown method",
            r#"vec![("X1", "a".to_uppercase(), "d")]"#,
        ),
        (
            "an unknown macro",
            r#"vec![("X1", concat!("a", "b"), "d")]"#,
        ),
        (
            "an unknown statement",
            r#"let mut v = vec![]; if true { v.push(("X1", "a".to_string(), "d")); } v"#,
        ),
        ("a case of two", r#"vec![("X1", "a".to_string())]"#),
    ] {
        let got = RustScope::read(&format!("fn must_mask() -> V {{ {body} }}"))
            .and_then(|scope| scope.must_mask());
        assert!(got.is_err(), "{label}: {got:?}");
    }
    assert!(
        RustScope::read(r#"static BODY: &str = "b";"#).is_err(),
        "an item other than a const or a fn"
    );
}

#[test]
fn the_phase_log_carries_no_string_of_the_a209_masking_corpus() {
    let fx = Fixture::new();
    record_the_unmasked_producer_session(&fx);
    record_the_captured_session(&fx);

    let mut hits: Vec<String> = Vec::new();
    let mut cli_phases: Vec<(&str, Vec<Value>)> = Vec::new();
    for session in [CAPTURED, UNMASKED] {
        let (code, _, err) = fx.cli(&["claims", "--session", session, "--json", "--phases"]);
        assert!(matches!(code, Some(0) | Some(3)), "{session}: {code:?}");
        hits.extend(recorded_text_hits(
            &format!("claims --phases stderr ({session})"),
            &decoded_stderr(&err),
            &String::from_utf8_lossy(&err),
        ));
        let phases: Vec<Value> = String::from_utf8_lossy(&err)
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        cli_phases.push((session, phases));
    }

    // The vectors are exercised, or a clean result proves nothing.
    let phase = |session: &str, name: &str| -> Value {
        cli_phases
            .iter()
            .find(|(s, _)| *s == session)
            .and_then(|(_, p)| p.iter().find(|x| x["phase"] == json!(name)).cloned())
            .unwrap_or_else(|| panic!("{session} has no {name} phase"))
    };
    assert!(
        phase(UNMASKED, "find_claims")["count"].as_u64() >= Some(1),
        "premise: the unmasked session's claims were extracted"
    );
    assert!(
        phase(CAPTURED, "workspace_since")["detail"]
            .as_str()
            .is_some_and(|d| d.contains("changed after the last check")),
        "premise: the cited check's tree differs from the final tree: {}",
        phase(CAPTURED, "workspace_since")
    );

    let server = Server::start(fx.ui());
    for path in [
        format!("/api/phases?session={CAPTURED}"),
        format!("/api/phases?session={UNMASKED}"),
        "/api/phases".to_string(),
    ] {
        let (status, body) = server.get(&path);
        assert_eq!(status, 200, "{path}: {}", String::from_utf8_lossy(&body));
        let doc: Value = serde_json::from_slice(&body).expect("JSON body");
        assert_eq!(doc["contract"], "thruline.phases.v1", "{path}");
        let mut decoded = String::new();
        strings_of(&doc, &mut decoded);
        hits.extend(recorded_text_hits(
            &path,
            &decoded,
            &String::from_utf8_lossy(&body),
        ));
    }

    // The panel's own claim text comes from `/api/claims`, masked at capture
    // (`claude_code.rs` `redact_delivery`) and now at serve time too. Pinned
    // for the captured session.
    let (status, body) = server.get(&format!("/api/claims?session={CAPTURED}"));
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let doc: Value = serde_json::from_slice(&body).expect("claims JSON");
    let claims = doc["claims"].as_array().expect("claims[]");
    assert!(
        !claims.is_empty(),
        "premise: the captured session has claims"
    );
    for c in claims {
        let text = c["text"].as_str().unwrap_or_default();
        hits.extend(secret_hits("/api/claims claims[].text", text));
    }

    assert!(
        hits.is_empty(),
        "recorded text reached a phase surface ({} hit(s)): {hits:#?}",
        hits.len()
    );
}

/* ── requests 11 and 16: masking at the serving edge ──────────────────── */

/// A must-mask corpus string with every alphanumeric replaced by `x`.
///
/// Character-local and length-preserving: the same characters in the same
/// places, every punctuation mark and newline kept, and no credential prefix or
/// body left behind. Both halves matter. Length, because a claim's `span` is an
/// offset into the recorded message, so a replacement that moved one character
/// would move every span after it and the control would prove nothing.
/// Punctuation, because the extractor splits the message into sentences on it —
/// replace a `.` and the claims themselves change.
fn neutralised(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { 'x' } else { c })
        .collect()
}

/// [`final_message`] with every must-mask corpus string neutralised: the same
/// sentences, at the same offsets, with nothing left for the masker to find.
fn control_message() -> String {
    let original = final_message();
    let mut m = original.clone();
    for (_, input, _) in a209_must_mask_half() {
        m = m.replace(input.as_str(), &neutralised(&input));
    }
    assert_eq!(
        m.chars().count(),
        original.chars().count(),
        "the control record must be the same length, or every claim span moves"
    );
    assert!(
        !m.contains("TLCANARY"),
        "premise: the control record still holds a canary, so it is not a control"
    );
    m
}

/// The control session: [`control_message`] through the same unmasked-producer
/// path [`record_the_unmasked_producer_session`] uses.
fn record_the_control_session(fx: &Fixture) {
    fx.hook(CONTROL, "SessionStart", json!({"source": "startup"}));
    fx.hook(
        CONTROL,
        "Stop",
        json!({"last_assistant_message": PLACEHOLDER, "stop_hook_active": false}),
    );
    let spool = std::fs::read_to_string(fx.spool()).expect("spool");
    let mut rewrote = 0;
    let lines: Vec<String> = spool
        .lines()
        .map(|line| {
            let mut v: Value = serde_json::from_str(line).expect("spool line is JSON");
            if v["payload"]["last_assistant_message"] == json!(PLACEHOLDER) {
                v["payload"]["last_assistant_message"] = json!(control_message());
                rewrote += 1;
            }
            v.to_string()
        })
        .collect();
    assert_eq!(rewrote, 1, "the placeholder Stop line was not in the spool");
    std::fs::write(fx.spool(), lines.join("\n") + "\n").expect("rewrite spool");
    fx.fold();
    let stop = fx.stop_message(CONTROL);
    assert!(
        !stop.contains("TLCANARY") && stop.contains(PROSE),
        "premise: the control record holds the same prose and no canary"
    );
}

/// Every `text` a document serves under `key`, as (surface, text).
fn served_rows(doc: &Value, key: &str, surface: &str) -> Vec<(String, String)> {
    doc[key]
        .as_array()
        .unwrap_or_else(|| panic!("{surface}: not an array"))
        .iter()
        .map(|row| {
            (
                surface.to_string(),
                row["text"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{surface}: a row with no text: {row}"))
                    .to_string(),
            )
        })
        .collect()
}

/// One `--json` body from the CLI, under §14's exit rule.
fn json_body(fx: &Fixture, args: &[&str]) -> Value {
    let (code, out, err) = fx.cli(args);
    assert!(
        matches!(code, Some(0) | Some(3)),
        "{args:?} exited {code:?}: {}",
        String::from_utf8_lossy(&err)
    );
    serde_json::from_slice(&out).unwrap_or_else(|e| panic!("{args:?}: not JSON: {e}"))
}

/// **Interface requests 11 and 16.** Every `text` the two projections serve is
/// masked on the way out, and nothing else about it changes.
///
/// Read over the unmasked-producer session, whose record holds the RAW claim
/// text — the session request 11 was measured on, and the only one where this
/// can fail, because every producer the product ships masks at capture.
///
/// The four fields are `claims[].text` and `promises[].text` of
/// `claims --json` (request 11) and `claims_vs_proof[].text` and
/// `promises[].text` of `explain --json` (request 16).
/// `docs/UI-SERVER-CONTRACT.md` §2.1 serves both bodies byte for byte, so the
/// command and the route are both read and required to serve the same words.
#[test]
fn the_claims_and_explain_projections_mask_the_text_they_serve() {
    let fx = Fixture::new();
    record_the_unmasked_producer_session(&fx);
    let raw = fx.stop_message(UNMASKED);

    let claims = json_body(&fx, &["claims", "--session", UNMASKED, "--json"]);
    let explain = json_body(&fx, &["explain", "--session", UNMASKED, "--json"]);

    let mut served: Vec<(String, String)> = Vec::new();
    served.extend(served_rows(
        &claims,
        "claims",
        "claims --json claims[].text",
    ));
    served.extend(served_rows(
        &claims,
        "promises",
        "claims --json promises[].text",
    ));
    served.extend(served_rows(
        &explain,
        "claims_vs_proof",
        "explain --json claims_vs_proof[].text",
    ));
    served.extend(served_rows(
        &explain,
        "promises",
        "explain --json promises[].text",
    ));
    for surface in [
        "claims --json claims[].text",
        "claims --json promises[].text",
        "explain --json claims_vs_proof[].text",
        "explain --json promises[].text",
    ] {
        assert!(
            served.iter().any(|(s, _)| s.as_str() == surface),
            "premise: {surface} is empty, so it cannot fail"
        );
    }

    let mut hits: Vec<String> = Vec::new();
    let mut masked_rows = 0usize;
    let mut whole_rows = 0usize;
    for (surface, text) in &served {
        hits.extend(secret_hits(surface, text));
        // A fixpoint at the edge: a second pass finds nothing, so a consumer
        // that masks again changes none of these bytes.
        assert_eq!(
            &mask(text).0,
            text,
            "{surface}: masking is not a fixpoint here: {text:?}"
        );
        if text.contains("[masked: ") {
            masked_rows += 1;
        } else {
            // An unmasked row is the record's own bytes, WHOLE. A mask that ate
            // ordinary prose shows up here rather than as a leak — the worse of
            // the two defects, and the one a leak-only test cannot see.
            assert!(
                raw.contains(text.as_str()),
                "{surface}: served text is not in the record whole: {text:?}"
            );
            whole_rows += 1;
        }
    }
    assert!(
        hits.is_empty(),
        "recorded secrets reached a served field ({} hit(s)): {hits:#?}",
        hits.len()
    );
    assert!(
        masked_rows > 0,
        "premise: nothing was masked, so a masker that did nothing would pass this"
    );
    assert!(
        whole_rows > 0,
        "premise: every served row was masked, so prose survival is untested"
    );
    assert!(
        served
            .iter()
            .any(|(s, t)| s.starts_with("claims --json claims") && t.contains(PROSE)),
        "ordinary prose did not survive the mask whole"
    );

    // The two projections must serve the SAME words. The panel builds its Said
    // row's record index from these rows and reads promise text from
    // `/api/explain` when the claims read fails (`ws/adapter.js`), so a mask on
    // one route and not the other is how a quotation stops matching its record.
    let text_of = |doc: &Value, key: &str| -> Vec<String> {
        served_rows(doc, key, key)
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    };
    assert_eq!(
        text_of(&claims, "claims"),
        text_of(&explain, "claims_vs_proof"),
        "the two projections serve different words for the same claims"
    );
    assert_eq!(
        text_of(&claims, "promises"),
        text_of(&explain, "promises"),
        "the two projections serve different words for the same promises"
    );

    // `explain`'s promises are cloned `Promise`s re-serialised by serde, never
    // rebuilt as a `json!` object: the two optional fields carry
    // `skip_serializing_if = "Option::is_none"`, and a hand-built object would
    // drop that and change the body's shape for every consumer.
    for p in explain["promises"].as_array().expect("promises") {
        for key in ["id", "session_id", "kind", "status", "opened_event_id"] {
            assert!(p.get(key).is_some(), "promises[] lost {key}: {p}");
        }
        assert!(
            p.get("closed_event_id").is_none() && p.get("closed_reason").is_none(),
            "an open promise gained a null field — the masked clone changed the \
             body's shape: {p}"
        );
    }

    // And at the routes, which is what requests 11 and 16 actually ask about.
    let server = Server::start(fx.ui());
    for (path, key, from_cli) in [
        (
            format!("/api/claims?session={UNMASKED}"),
            "claims",
            text_of(&claims, "claims"),
        ),
        (
            format!("/api/explain?session={UNMASKED}"),
            "claims_vs_proof",
            text_of(&explain, "claims_vs_proof"),
        ),
    ] {
        let (status, body) = server.get(&path);
        assert_eq!(status, 200, "{path}: {}", String::from_utf8_lossy(&body));
        let doc: Value = serde_json::from_slice(&body).expect("JSON body");
        let rows = served_rows(&doc, key, &path);
        for (surface, text) in &rows {
            assert!(
                secret_hits(surface, text).is_empty(),
                "{path}: a recorded secret reached {key}[].text: {text:?}"
            );
        }
        assert_eq!(
            rows.into_iter().map(|(_, t)| t).collect::<Vec<_>>(),
            from_cli,
            "{path} and its own command serve different words"
        );
    }
}

/// **Masking changes the words, never which claims exist or how they were
/// judged.**
///
/// Two records with the same sentences at the same offsets: one carrying every
/// must-mask corpus string, one with those strings neutralised in place. The
/// claim kinds, spans and verdicts must be identical across the pair, and only
/// the served words may differ.
///
/// **Why the id SETS are not compared across the pair.** `derive_id` hashes
/// `session_id|event_id|offset|kind` (`crates/claims/src/extract.rs`), so two
/// sessions cannot share a claim id whatever their text — cross-session id
/// equality is unavailable, not merely unasserted. The id question is answered
/// where it can be: against the phase log of the very run being served. Those
/// ids are reported by the pipeline as it extracts, before `emit` masks
/// anything, so if the mask renamed or renumbered a claim the two would
/// disagree.
#[test]
fn masking_changes_the_words_served_and_not_which_claims_exist_or_how_they_were_judged() {
    let fx = Fixture::new();
    record_the_unmasked_producer_session(&fx);
    record_the_control_session(&fx);

    let secrets = json_body(&fx, &["claims", "--session", UNMASKED, "--json"]);
    let control = json_body(&fx, &["claims", "--session", CONTROL, "--json"]);

    let shape = |doc: &Value| -> Vec<(String, String, Value)> {
        doc["claims"]
            .as_array()
            .expect("claims")
            .iter()
            .map(|c| {
                (
                    c["kind"].as_str().unwrap_or_default().to_string(),
                    c["verdict"].as_str().unwrap_or_default().to_string(),
                    c["span"]["offset"].clone(),
                )
            })
            .collect()
    };
    let promise_shape = |doc: &Value| -> Vec<(String, String)> {
        doc["promises"]
            .as_array()
            .expect("promises")
            .iter()
            .map(|p| {
                (
                    p["kind"].as_str().unwrap_or_default().to_string(),
                    p["status"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect()
    };
    assert!(
        !shape(&secrets).is_empty() && !promise_shape(&secrets).is_empty(),
        "premise: the record carries claims and promises"
    );
    assert_eq!(
        shape(&secrets),
        shape(&control),
        "a maskable string changed which claims exist, where they are, or how \
         they were judged"
    );
    assert_eq!(
        promise_shape(&secrets),
        promise_shape(&control),
        "a maskable string changed the promises"
    );

    // The control is the same record WITHOUT the maskable strings, so strictly
    // fewer of its served rows carry a mask. Without this the pair could be two
    // identically-masked records and the comparison above would say nothing.
    let marked = |doc: &Value| -> usize {
        doc["claims"]
            .as_array()
            .expect("claims")
            .iter()
            .filter(|c| c["text"].as_str().unwrap_or_default().contains("[masked: "))
            .count()
    };
    assert!(
        marked(&control) < marked(&secrets),
        "the control record was masked as heavily as the one carrying secrets \
         ({} vs {}) — it is not a control",
        marked(&control),
        marked(&secrets)
    );

    // The ids `emit` served are the ids the pipeline extracted, before it
    // masked anything.
    let (code, out, err) = fx.cli(&["claims", "--session", UNMASKED, "--json", "--phases"]);
    assert!(matches!(code, Some(0) | Some(3)), "{code:?}");
    let doc: Value = serde_json::from_slice(&out).expect("claims JSON");
    let served_ids: BTreeSet<String> = doc["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .map(|c| c["id"].as_str().unwrap_or_default().to_string())
        .collect();
    let detail = String::from_utf8_lossy(&err)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["phase"] == json!("find_claims"))
        .and_then(|v| v["detail"].as_str().map(str::to_string))
        .expect("a find_claims phase");
    let extracted_ids: BTreeSet<String> = detail.split(" \u{b7} ").map(str::to_string).collect();
    assert_eq!(
        served_ids, extracted_ids,
        "the ids served are not the ids extracted — masking renamed a claim"
    );
}
