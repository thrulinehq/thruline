#!/usr/bin/env python3
"""A scripted, local-only Anthropic Messages endpoint for the Claude Code rig.

WHY THIS EXISTS
---------------
`scripts/rig/fake_responses.py` did this for Codex and made the whole subscribed
hook set provable at **zero API cost**. This is the same trick for the other
harness, and it is the one that matters for the chain-coverage gate: the gate's
numerator is *hooks folded*, and only a REAL agent session emits `SessionStart`,
a real tool call, and a real `Stop`. Terminal-Bench runs `run-tests.sh` strictly
after the agent returns, so `check`-binding can never produce those. A scripted
endpoint can, and it costs nothing.

This server never reaches the network. It binds 127.0.0.1, it is spawned and
killed by its caller, and it holds no credential: Claude Code is pointed at it
with `ANTHROPIC_BASE_URL`, and the key it sends is a placeholder the caller
invents (`sk-ant-FAKE…`). Nothing reaches a paid API.

THE PLAN
--------
A JSON list of turns, consumed one per *agentic* request:

  ["tool", {"name": "Bash", "input": {"command": "…"}}]  -> a real tool call:
        Claude Code executes it, which fires PreToolUse + PostToolUse and — for
        Bash — a real exec the shim records.
  ["text", "…"]  -> ends the turn, so Stop and SessionEnd follow.

WHAT COUNTS AS AN "AGENTIC" REQUEST, AND WHY IT IS NOT "EVERY REQUEST"
---------------------------------------------------------------------
Claude Code makes side requests that are not part of the conversation — a topic
sniff, a token count, a quota probe. Advancing the plan on those desynchronises
it: the tool call lands on the wrong turn and the transcript ends early with no
tool use at all. The discriminator used here is `tools`: the agentic loop always
offers the tool schemas, the side calls never do. A request with no `tools` gets
a short canned reply and the plan does NOT move.

THE REQUEST LOG IS THE PROOF
----------------------------
Every request is appended to the log file as one JSON line, including its path,
whether it advanced the plan, and what the turn was. The caller asserts that log
is non-empty. **A run that reached no endpoint at all, and a run that reached a
real one, both leave this file empty** — which is exactly the failure that must
never be mistakable for a success (Rule 12).
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

STATE = {"n": 0}
LOCK = threading.Lock()


def sse(kind, payload):
    return f"event: {kind}\ndata: {json.dumps(payload)}\n\n".encode()


def _blocks(turn):
    """The content blocks for one planned turn."""
    kind, payload = turn
    if kind == "tool":
        return [{
            "type": "tool_use",
            "id": "toolu_%s" % payload.get("id", "rig01"),
            "name": payload["name"],
            "input": payload.get("input", {}),
        }], "tool_use"
    if kind == "text":
        return [{"type": "text", "text": payload}], "end_turn"
    raise ValueError("unknown plan turn kind %r" % (kind,))


def _message(blocks, stop_reason, model):
    return {
        "id": "msg_rig_%d" % STATE["n"],
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": blocks,
        "stop_reason": stop_reason,
        "stop_sequence": None,
        "usage": {"input_tokens": 11, "output_tokens": 7},
    }


def _stream(blocks, stop_reason, model):
    """The SSE form. Emitted block-by-block, as the vendor wire format is."""
    out = [sse("message_start", {
        "type": "message_start",
        "message": _message([], None, model),
    })]
    for i, b in enumerate(blocks):
        if b["type"] == "text":
            out.append(sse("content_block_start", {
                "type": "content_block_start", "index": i,
                "content_block": {"type": "text", "text": ""}}))
            out.append(sse("content_block_delta", {
                "type": "content_block_delta", "index": i,
                "delta": {"type": "text_delta", "text": b["text"]}}))
        else:
            out.append(sse("content_block_start", {
                "type": "content_block_start", "index": i,
                "content_block": {"type": "tool_use", "id": b["id"],
                                  "name": b["name"], "input": {}}}))
            out.append(sse("content_block_delta", {
                "type": "content_block_delta", "index": i,
                "delta": {"type": "input_json_delta",
                          "partial_json": json.dumps(b["input"])}}))
        out.append(sse("content_block_stop",
                       {"type": "content_block_stop", "index": i}))
    out.append(sse("message_delta", {
        "type": "message_delta",
        "delta": {"stop_reason": stop_reason, "stop_sequence": None},
        "usage": {"output_tokens": 7}}))
    out.append(sse("message_stop", {"type": "message_stop"}))
    return b"".join(out)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def _record(self, entry):
        with open(self.server.reqlog, "a") as f:
            f.write(json.dumps(entry) + "\n")

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("content-type", ctype)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        # Whatever it is — a health probe, a model list — it is not a turn.
        self._record({"path": self.path, "method": "GET", "advanced": False})
        self._send(200, b'{"data":[]}', "application/json")

    def do_POST(self):
        n = int(self.headers.get("content-length", 0) or 0)
        raw = self.rfile.read(n)
        try:
            req = json.loads(raw)
        except ValueError:
            req = {}

        if self.path.endswith("count_tokens"):
            self._record({"path": self.path, "method": "POST",
                          "advanced": False, "why": "count_tokens"})
            self._send(200, b'{"input_tokens": 11}', "application/json")
            return

        model = req.get("model") or "fake-model"
        agentic = bool(req.get("tools"))
        if not agentic:
            # A side call. Answer it, do not move the plan.
            self._record({"path": self.path, "method": "POST",
                          "advanced": False, "why": "no tools offered",
                          "model": model})
            blocks, stop = [{"type": "text", "text": "ok"}], "end_turn"
        else:
            with LOCK:
                i = STATE["n"]
                STATE["n"] += 1
            plan = self.server.plan
            turn = plan[i] if i < len(plan) else ["text", "done"]
            self._record({"path": self.path, "method": "POST", "advanced": True,
                          "turn_index": i, "turn": turn[0], "model": model,
                          "n_messages": len(req.get("messages", []))})
            blocks, stop = _blocks(turn)

        if req.get("stream"):
            self._send(200, _stream(blocks, stop, model), "text/event-stream")
        else:
            self._send(200,
                       json.dumps(_message(blocks, stop, model)).encode(),
                       "application/json")


def main():
    port, reqlog, planfile = int(sys.argv[1]), sys.argv[2], sys.argv[3]
    with open(planfile) as f:
        plan = [list(x) for x in json.load(f)]
    # 0.0.0.0, not 127.0.0.1, ONLY when asked: the container-side rig reaches
    # this through the Docker gateway, which is not loopback. The default stays
    # loopback so a host-side rig cannot be talked to from off the machine.
    bind = sys.argv[4] if len(sys.argv) > 4 else "127.0.0.1"
    srv = ThreadingHTTPServer((bind, port), Handler)
    srv.plan = plan
    srv.reqlog = reqlog
    srv.serve_forever()


if __name__ == "__main__":
    main()
