#!/usr/bin/env python3
"""A scripted, local-only OpenAI Responses endpoint for the hook-event rig.

WHY THIS EXISTS
---------------
Capture coverage used to look like something only a paid run could demonstrate.
It is not: Codex fires `SessionStart` and `UserPromptSubmit` before its first
model request, and every other subscribed event is driven by what the model
*replies*. A scripted endpoint replies without a model, so the whole subscribed
set can be verified at **zero API cost**.

This server never reaches the network. It binds 127.0.0.1, it is spawned and
killed by `scripts/hook-event-rig.sh`, and it holds no credential: Codex is
pointed at it with `model_providers.*.base_url`, and the key it sends is a
placeholder the script invents.

WHAT IT DRIVES
--------------
The turn script is a list of replies, consumed one per request:

  exec_command(cmd)   -> PreToolUse + PostToolUse   (and, with a non-zero exit,
                         the CONFLICT-6b case: a failure that is present and
                         indistinguishable)
  spawn_agent(...)    -> SubagentStart + SubagentStop
  message(text)       -> ends the turn, so Stop and SessionEnd follow

Tool names are not guessed. They were read off Codex's own request:
`exec_command` is a top-level function; `spawn_agent` lives in the
`multi_agent_v1` namespace.
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SPAWN_TOOL = "multi_agent_v1__spawn_agent"

STATE = {"n": 0}
LOCK = threading.Lock()


def sse(kind, payload):
    return f"event: {kind}\ndata: {json.dumps(payload)}\n\n".encode()


def fn_call(name, args, call_id):
    return {
        "type": "function_call",
        "name": name,
        "arguments": json.dumps(args),
        "call_id": call_id,
        "id": "fc_" + call_id,
    }


def message(text):
    return {
        "type": "message",
        "role": "assistant",
        "id": "msg_1",
        "content": [{"type": "output_text", "text": text}],
    }


def script(i, plan):
    """The i-th reply. `plan` is a list of (kind, payload) pairs."""
    if i < len(plan):
        kind, payload = plan[i]
        if kind == "exec":
            return [fn_call("exec_command", {"cmd": payload}, f"call_{i}")]
        if kind == "spawn":
            # NAMESPACED. A bare `spawn_agent` is rejected by Codex's router
            # with `unsupported call: spawn_agent` — measured, not assumed. The
            # tool lives inside the `multi_agent_v1` namespace and must be
            # called by its qualified name.
            return [fn_call(SPAWN_TOOL, payload, f"call_{i}")]
        if kind == "call":
            return [fn_call(payload[0], payload[1], f"call_{i}")]
        if kind == "text":
            return [message(payload)]
    return [message("done")]


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        raw = self.rfile.read(n)
        with LOCK:
            i = STATE["n"]
            STATE["n"] += 1
        try:
            req = json.loads(raw)
        except ValueError:
            req = {}
        with open(self.server.reqlog, "a") as f:
            f.write(json.dumps({"i": i, "n_input": len(req.get("input", []))}) + "\n")

        items = script(i, self.server.plan)
        body = b"".join(
            [sse("response.created", {"type": "response.created",
                                      "response": {"id": f"resp_{i}"}})]
            + [sse("response.output_item.done",
                   {"type": "response.output_item.done", "item": it}) for it in items]
            + [sse("response.completed",
                   {"type": "response.completed",
                    "response": {"id": f"resp_{i}",
                                 "usage": {"input_tokens": 11, "output_tokens": 7,
                                           "total_tokens": 18}}})]
        )
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    port, reqlog, planfile = int(sys.argv[1]), sys.argv[2], sys.argv[3]
    with open(planfile) as f:
        plan = [tuple(x) for x in json.load(f)]
    srv = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    srv.plan = plan
    srv.reqlog = reqlog
    srv.serve_forever()


if __name__ == "__main__":
    main()
