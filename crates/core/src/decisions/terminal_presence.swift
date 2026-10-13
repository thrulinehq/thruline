// Creation-time OS user presence for one reviewed Decisions action.
// No credential is read or returned by this process. The enclosing bridge
// owns the nonce and exact action tuple; no authentication result is reusable.
import Foundation
import LocalAuthentication
import Darwin

let environment = ProcessInfo.processInfo.environment
guard let reason = environment["THRULINE_DECISION_REASON"],
      !reason.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
      reason.utf8.count <= 4096 else {
    exit(2)
}

let agentMarkers = [
    "CODEX_THREAD_ID", "CODEX_RUN_ID", "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT",
    "OPENAI_AGENT", "THRULINE_AGENT_CONTEXT", "THRULINE_MODEL_CONTEXT"
]
if agentMarkers.contains(where: { environment[$0] != nil }) {
    exit(3)
}
if let origin = environment["THRULINE_ACTOR_KIND"],
   origin != "person" {
    exit(3)
}
guard getuid() == geteuid() else {
    exit(3)
}
guard isatty(STDIN_FILENO) == 1 else {
    exit(4)
}

let context = LAContext()
context.touchIDAuthenticationAllowableReuseDuration = 0
var availabilityError: NSError?
guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &availabilityError) else {
    exit(1)
}

// Apple's reply runs on a private framework queue. Exit directly from it;
// there is no caller-supplied success flag or mutable result shared with Rust.
context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { success, _ in
    exit(success ? 0 : 1)
}
Thread.sleep(forTimeInterval: 90)
context.invalidate()
exit(5)
