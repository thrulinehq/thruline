# Parser fixtures

Captured 2026-07-29 during M1 step 0 from this machine's real
`~/.claude/projects/` transcripts (CLI 2.1.205 and 2.1.219, `entrypoint`
`cli` and `claude-desktop`).

## `claude_code_sessions.jsonl` — 12 real lines, redacted

One specimen per parser path; see `claude_code_sessions.manifest.json` for the
line-by-line mapping.

**Redaction.** Values only — structure is untouched. Every key path and JSON
type is byte-for-byte identical to the source line (asserted at capture time
across all 12 specimens), so the fixtures exercise the same parsing branches as
production data.

| what | how |
|---|---|
| prose and payloads (`text`, `thinking`, `content`, `command`, `description`, `lastPrompt`, `aiTitle`, …) | replaced with `[redacted <key> <hash8>] `-repeating filler of **the same character length**, so size classes and offsets stay realistic |
| all UUIDs (`uuid`, `parentUuid`, `sessionId`, `session_id`, `promptId`, `leafUuid`, `toolUseID`, `messageId`, …) | rewritten through a consistent bijection to `00000000-0000-4000-8000-…` — **parent/child and tool_use↔tool_result linkage is preserved** |
| `toolu_*` / `msg_*` / `req_*` ids | rewritten consistently to zero-padded synthetic ids |
| absolute paths | `/Users/<real>` → `/Users/dev`; MCP server UUIDs in tool names zeroed |
| `thinking.signature` | truncated to 96 chars of filler (the real value is ~5 KB of base64 and adds nothing to parsing) |

Scanned clean for: usernames, email addresses, `sk-`/`ghp_`/`AKIA` prefixes,
`Bearer` tokens, private-key headers, and the strings `password`/`secret`/`api_key`.
The only absolute path remaining is `/Users/dev`.

Timestamps, `version`, `entrypoint`, `gitBranch`, `model`, `usage` counters and
all structural flags are **not** redacted — they carry no personal data and the
parser depends on them.

## Synthetic fixtures

* `malformed.jsonl` — well-formed lines surrounding: unterminated JSON, an empty
  line, a whitespace-only line, an unknown `type`, a non-JSON line, and a final
  line truncated mid-object with no trailing newline (interrupted session).
* `bad_utf8.jsonl` — valid lines around one line containing invalid UTF-8 bytes
  (`ff fe 80`).

Neither is derived from real transcripts.
