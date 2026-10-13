//! Every line an edit added or removed, with its line numbers and context,
//! as a diff viewer shows it — from what the record holds.
//!
//! Three sources, in this order (`FileEdit::source`):
//!
//! 1. **`structured_patch`** — Claude Code's `Edit`/`MultiEdit`/`Write` result
//!    carries `toolUseResult.structuredPatch`: hunks with the file's real line
//!    numbers and context lines. When present it is used, because it is what
//!    the tool reports it did to the file.
//! 2. **`apply_patch`** — a Codex patch text (classic `apply_patch`, or inside
//!    a code-mode `exec` program): `*** Add/Update/Delete File:` sections,
//!    `@@ <header>` hunks of ` `/`+`/`-` lines. Codex patches state no line
//!    numbers, so `old_start`/`new_start` are `null`; `old_lines`/`new_lines`
//!    are counted from the hunk. One patch can touch several files; each file
//!    is one [`FileEdit`].
//! 3. **`request`** — only when neither exists: the hunk is computed from the
//!    request (`old_string` → `new_string` with common leading and trailing
//!    lines dropped, each `MultiEdit` entry, a `Write`'s `content` all added, a
//!    `NotebookEdit`'s `new_source`). No line number and no context is known:
//!    `old_start` is `null` and every line is `added` or `removed`.

use serde::Serialize;
use serde_json::Value;

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// `context` | `added` | `removed`.
    pub kind: &'static str,
    pub text: String,
}

/// One hunk, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    /// 1-based first line in the file before the change; `null` when the
    /// source states none.
    pub old_start: Option<u64>,
    pub old_lines: u64,
    pub new_start: Option<u64>,
    pub new_lines: u64,
    /// The text after `@@` (Codex) — usually the enclosing function.
    pub header: Option<String>,
    pub lines: Vec<Line>,
}

impl Hunk {
    fn new(header: Option<String>) -> Self {
        Hunk {
            old_start: None,
            old_lines: 0,
            new_start: None,
            new_lines: 0,
            header,
            lines: Vec::new(),
        }
    }

    fn push(&mut self, kind: &'static str, text: &str) {
        match kind {
            "context" => {
                self.old_lines += 1;
                self.new_lines += 1;
            }
            "removed" => self.old_lines += 1,
            _ => self.new_lines += 1,
        }
        self.lines.push(Line {
            kind,
            text: text.to_string(),
        });
    }

    pub fn added(&self) -> u64 {
        self.lines.iter().filter(|l| l.kind == "added").count() as u64
    }

    pub fn removed(&self) -> u64 {
        self.lines.iter().filter(|l| l.kind == "removed").count() as u64
    }

    fn changes(&self) -> bool {
        self.lines.iter().any(|l| l.kind != "context")
    }
}

/// What one edit did to one file, before masking and path display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    pub path: String,
    /// `edit` | `write` | `multiedit` | `notebook` | `patch`.
    pub op: &'static str,
    /// For `patch`: `add` | `update` | `delete`; for `write`: `create` |
    /// `update` when the tool said which.
    pub action: Option<String>,
    /// For a patch: the destination of `*** Move to:`.
    pub moved_to: Option<String>,
    pub hunks: Vec<Hunk>,
    /// `structured_patch` | `apply_patch` | `request` (module docs).
    pub source: &'static str,
}

impl FileEdit {
    pub fn lines_added(&self) -> u64 {
        self.hunks.iter().map(Hunk::added).sum()
    }
    pub fn lines_removed(&self) -> u64 {
        self.hunks.iter().map(Hunk::removed).sum()
    }
}

/// The tool names whose rows are edits. Codex's `apply_patch` is stored as
/// `Edit` with `source_tool_name` kept (`codex.rs` `canonical_tool_name`).
pub const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// The edits one `tool_use` row made, from its input and (when present) its
/// result. `result` is the stored `tool_result` JSON (`{content,
/// toolUseResult}`).
pub fn edits_of(tool_name: &str, input: &Value, result: Option<&Value>) -> Vec<FileEdit> {
    let structured = result.and_then(structured_hunks).filter(|h| !h.is_empty());
    let write_type = result
        .and_then(|r| r.pointer("/toolUseResult/type"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let path = input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if tool_name == "Edit" && path.is_none() {
        if let Some(text) = input
            .get("patch")
            .or_else(|| input.get("input"))
            .and_then(Value::as_str)
        {
            return parse_patch(text);
        }
    }
    let Some(path) = path else {
        return Vec::new();
    };
    let from_tool = structured.is_some();
    let source = if from_tool {
        "structured_patch"
    } else {
        "request"
    };
    let pair = |e: &Value| {
        request_hunk(
            e.get("old_string").and_then(Value::as_str).unwrap_or(""),
            e.get("new_string").and_then(Value::as_str).unwrap_or(""),
        )
    };
    let (op, action, hunks) = match tool_name {
        "Edit" => (
            "edit",
            None,
            structured.unwrap_or_else(|| vec![pair(input)]),
        ),
        "MultiEdit" => (
            "multiedit",
            None,
            structured.unwrap_or_else(|| {
                input
                    .get("edits")
                    .and_then(Value::as_array)
                    .map(|edits| edits.iter().map(pair).collect())
                    .unwrap_or_default()
            }),
        ),
        "Write" => (
            "write",
            write_type,
            structured.unwrap_or_else(|| {
                vec![request_hunk(
                    "",
                    input.get("content").and_then(Value::as_str).unwrap_or(""),
                )]
            }),
        ),
        "NotebookEdit" => (
            "notebook",
            input
                .get("edit_mode")
                .and_then(Value::as_str)
                .map(str::to_string),
            vec![request_hunk(
                "",
                input
                    .get("new_source")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )],
        ),
        _ => return Vec::new(),
    };
    vec![FileEdit {
        path,
        op,
        action,
        moved_to: None,
        hunks: hunks.into_iter().filter(Hunk::changes).collect(),
        source: if tool_name == "NotebookEdit" {
            "request"
        } else {
            source
        },
    }]
}

fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    text.lines().collect()
}

/// `old` → `new` as one hunk with no line numbers and no context: the lines
/// both share at the start and the end are dropped.
pub fn request_hunk(old: &str, new: &str) -> Hunk {
    let a = lines(old);
    let b = lines(new);
    let mut head = 0usize;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0usize;
    while tail < a.len() - head
        && tail < b.len() - head
        && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
    {
        tail += 1;
    }
    let mut h = Hunk::new(None);
    for l in &a[head..a.len() - tail] {
        h.push("removed", l);
    }
    for l in &b[head..b.len() - tail] {
        h.push("added", l);
    }
    h
}

/// `toolUseResult.structuredPatch`, one [`Hunk`] per entry, lines in order.
fn structured_hunks(result: &Value) -> Option<Vec<Hunk>> {
    let patch = result
        .pointer("/toolUseResult/structuredPatch")?
        .as_array()?;
    let mut out = Vec::new();
    for entry in patch {
        let Some(lines) = entry.get("lines").and_then(Value::as_array) else {
            continue;
        };
        let mut h = Hunk::new(None);
        for line in lines.iter().filter_map(Value::as_str) {
            let mut chars = line.chars();
            let kind = match chars.next() {
                Some('+') => "added",
                Some('-') => "removed",
                Some('\\') => continue, // "\ No newline at end of file"
                _ => "context",
            };
            h.push(kind, chars.as_str());
        }
        h.old_start = entry.get("oldStart").and_then(Value::as_u64);
        h.new_start = entry.get("newStart").and_then(Value::as_u64);
        // The tool's own counts win when it states them.
        if let Some(n) = entry.get("oldLines").and_then(Value::as_u64) {
            h.old_lines = n;
        }
        if let Some(n) = entry.get("newLines").and_then(Value::as_u64) {
            h.new_lines = n;
        }
        out.push(h);
    }
    Some(out)
}

/// Parse a Codex `apply_patch` text into one [`FileEdit`] per file section.
/// A text with no recognised section yields nothing.
pub fn parse_patch(text: &str) -> Vec<FileEdit> {
    let mut out: Vec<FileEdit> = Vec::new();
    let mut cur: Option<(FileEdit, Option<Hunk>)> = None;
    fn flush(cur: &mut Option<(FileEdit, Option<Hunk>)>) {
        if let Some((edit, hunk)) = cur.as_mut() {
            if let Some(h) = hunk.take() {
                if h.changes() {
                    edit.hunks.push(h);
                }
            }
        }
    }
    for line in text.lines() {
        let header = [
            ("*** Add File: ", "add"),
            ("*** Update File: ", "update"),
            ("*** Delete File: ", "delete"),
        ]
        .iter()
        .find_map(|(p, a)| line.strip_prefix(p).map(|rest| (rest.trim(), *a)));
        if let Some((path, action)) = header {
            flush(&mut cur);
            if let Some((edit, _)) = cur.take() {
                out.push(edit);
            }
            let first = (action == "add").then(|| {
                let mut h = Hunk::new(None);
                h.old_start = Some(0);
                h.new_start = Some(1);
                h
            });
            cur = Some((
                FileEdit {
                    path: path.to_string(),
                    op: "patch",
                    action: Some(action.to_string()),
                    moved_to: None,
                    hunks: Vec::new(),
                    source: "apply_patch",
                },
                first,
            ));
            continue;
        }
        if let Some(dest) = line.strip_prefix("*** Move to: ") {
            if let Some((edit, _)) = cur.as_mut() {
                edit.moved_to = Some(dest.trim().to_string());
            }
            continue;
        }
        if line.starts_with("*** ") {
            // Begin/End Patch, End of File.
            flush(&mut cur);
            continue;
        }
        let Some((_, hunk)) = cur.as_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("@@") {
            flush(&mut cur);
            let header = rest.trim().trim_end_matches("@@").trim();
            if let Some((_, hunk)) = cur.as_mut() {
                *hunk = Some(Hunk::new((!header.is_empty()).then(|| header.to_string())));
            }
            continue;
        }
        let (kind, body) = if let Some(b) = line.strip_prefix('+') {
            ("added", b)
        } else if let Some(b) = line.strip_prefix('-') {
            ("removed", b)
        } else {
            ("context", line.strip_prefix(' ').unwrap_or(line))
        };
        hunk.get_or_insert_with(|| Hunk::new(None)).push(kind, body);
    }
    flush(&mut cur);
    if let Some((edit, _)) = cur.take() {
        out.push(edit);
    }
    out
}

/// The file paths a patch text touches, cheaply — for grouping.
pub fn paths_of_patch(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        for prefix in ["*** Add File: ", "*** Update File: ", "*** Delete File: "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                out.push(rest.trim().to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn kinds(h: &Hunk) -> Vec<&'static str> {
        h.lines.iter().map(|l| l.kind).collect()
    }

    #[test]
    fn a_request_hunk_keeps_only_the_changed_lines() {
        let h = request_hunk("a\nb\nc", "a\nB\nc");
        assert_eq!(kinds(&h), vec!["removed", "added"]);
        assert_eq!(h.old_start, None);
        let h = request_hunk("", "x\ny");
        assert_eq!(h.added(), 2);
        assert_eq!(h.removed(), 0);
    }

    #[test]
    fn the_tools_structured_patch_wins_with_line_numbers_and_context() {
        let input = json!({"file_path": "/r/a.rs", "old_string": "x", "new_string": "y"});
        let result = json!({"content": "ok", "toolUseResult": {"structuredPatch": [
            {"oldStart": 10, "oldLines": 4, "newStart": 10, "newLines": 4,
             "lines": [" ctx", "-old1", "+new1", " mid"]}
        ]}});
        let edits = edits_of("Edit", &input, Some(&result));
        let e = &edits[0];
        assert_eq!(e.source, "structured_patch");
        assert_eq!(e.hunks.len(), 1);
        let h = &e.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (Some(10), 4, Some(10), 4)
        );
        assert_eq!(kinds(h), vec!["context", "removed", "added", "context"]);
        assert_eq!(h.lines[1].text, "old1");
        assert_eq!((e.lines_added(), e.lines_removed()), (1, 1));
        // Without the result, the request is the source and says so.
        let e = &edits_of("Edit", &input, None)[0];
        assert_eq!(e.source, "request");
        assert_eq!(e.hunks[0].old_start, None);
    }

    #[test]
    fn a_codex_patch_yields_one_edit_per_file_with_headers() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+one\n+two\n*** Update File: src/b.rs\n@@ fn x()\n ctx\n-old\n+new\n ctx2\n@@\n+more\n*** Delete File: c.txt\n*** End Patch";
        let edits = edits_of("Edit", &json!({"patch": patch}), None);
        assert_eq!(edits.len(), 3);
        assert_eq!(edits[0].path, "a.txt");
        assert_eq!(edits[0].source, "apply_patch");
        assert_eq!(edits[0].action.as_deref(), Some("add"));
        assert_eq!(edits[0].lines_added(), 2);
        assert_eq!(edits[0].hunks[0].new_start, Some(1));
        let b = &edits[1];
        assert_eq!(b.hunks.len(), 2);
        assert_eq!(b.hunks[0].header.as_deref(), Some("fn x()"));
        assert_eq!(
            kinds(&b.hunks[0]),
            vec!["context", "removed", "added", "context"]
        );
        assert_eq!((b.hunks[0].old_lines, b.hunks[0].new_lines), (3, 3));
        assert_eq!(b.hunks[0].old_start, None);
        assert_eq!(b.hunks[1].header, None);
        assert_eq!(edits[2].action.as_deref(), Some("delete"));
        assert_eq!(paths_of_patch(patch), vec!["a.txt", "src/b.rs", "c.txt"]);
    }

    #[test]
    fn a_write_without_a_patch_is_every_line_added() {
        let edits = edits_of(
            "Write",
            &json!({"file_path": "/r/n.md", "content": "# T\n\nbody\n"}),
            Some(&json!({"toolUseResult": {"type": "create", "structuredPatch": []}})),
        );
        assert_eq!(edits[0].op, "write");
        assert_eq!(edits[0].source, "request");
        assert_eq!(edits[0].action.as_deref(), Some("create"));
        assert_eq!(edits[0].lines_added(), 3);
    }
}
