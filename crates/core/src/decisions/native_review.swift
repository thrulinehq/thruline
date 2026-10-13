// Native review is a separate transport. A browser request carries no authority.
// No successful result is accepted until a native Confirm callback followed by
// fresh LocalAuthentication for this exact nonce and action tuple.
import AppKit
import Foundation
import LocalAuthentication
import CryptoKit
import Darwin

let env = ProcessInfo.processInfo.environment
let markers = ["CODEX_THREAD_ID", "CODEX_RUN_ID", "CODEX_SANDBOX", "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "OPENAI_AGENT", "THRULINE_AGENT_CONTEXT", "THRULINE_MODEL_CONTEXT", "GITHUB_ACTIONS", "CI"]
guard !markers.contains(where: { env[$0] != nil }),
      env["THRULINE_ACTOR_KIND"] == nil || env["THRULINE_ACTOR_KIND"] == "person",
      getuid() != 0, getuid() == geteuid(),
      isatty(STDIN_FILENO) == 1, isatty(STDOUT_FILENO) == 1,
      tcgetsid(STDIN_FILENO) == getsid(0),
      tcgetpgrp(STDIN_FILENO) == getpgrp(),
      let physical = ttyname(STDIN_FILENO) else { exit(3) }
var input = stat(), output = stat(), named = stat()
guard fstat(STDIN_FILENO, &input) == 0, fstat(STDOUT_FILENO, &output) == 0,
      input.st_rdev == output.st_rdev,
      lstat(physical, &named) == 0, (named.st_mode & S_IFMT) == S_IFCHR,
      named.st_uid == getuid(), named.st_rdev == input.st_rdev else { exit(3) }
let physicalFD = open(physical, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
var opened = stat()
guard physicalFD >= 0, fstat(physicalFD, &opened) == 0,
      opened.st_ino == named.st_ino, opened.st_dev == named.st_dev,
      opened.st_rdev == input.st_rdev, opened.st_uid == getuid() else { exit(3) }
close(physicalFD)
// macOS /dev/tty is an alias. Its ownership/rdev is not the physical device.
let aliasFD = open("/dev/tty", O_RDONLY | O_CLOEXEC)
guard aliasFD >= 0 else { exit(3) }
close(aliasFD)

func privateFile(_ path: String, _ flags: Int32) -> FileHandle? {
    var before = stat(), after = stat()
    guard lstat(path, &before) == 0, (before.st_mode & S_IFMT) == S_IFREG,
          before.st_uid == getuid(), before.st_mode & 0o077 == 0 else { return nil }
    let fd = open(path, flags | O_NOFOLLOW | O_CLOEXEC)
    guard fd >= 0 else { return nil }
    guard fstat(fd, &after) == 0, before.st_dev == after.st_dev,
          before.st_ino == after.st_ino, after.st_uid == getuid() else {
        close(fd); return nil
    }
    return FileHandle(fileDescriptor: fd, closeOnDealloc: true)
}
guard let payloadPath = env["THRULINE_NATIVE_REVIEW_FILE"],
      let payloadSHA = env["THRULINE_NATIVE_REVIEW_SHA256"],
      let resultPath = env["THRULINE_NATIVE_RESULT_FILE"],
      let payloadFile = privateFile(payloadPath, O_RDONLY),
      let resultFile = privateFile(resultPath, O_WRONLY),
      let bytes = try? payloadFile.read(upToCount: 2 * 1024 * 1024 + 1),
      bytes.count <= 2 * 1024 * 1024,
      SHA256.hash(data: bytes).map({ String(format: "%02x", $0) }).joined() == payloadSHA,
      let payload = (try? JSONSerialization.jsonObject(with: bytes)) as? [String: Any],
      payload["schema_version"] as? Int == 1,
      let review = payload["review"] as? [String: Any],
      let expected = payload["expected"] as? [String: Any],
      expected["schema_version"] as? Int == 1,
      expected["transport"] as? String == "native_review_click",
      expected["result"] as? String == "confirmed",
      expected["account_uid"] as? UInt32 == getuid(),
      let nonce = expected["nonce"] as? String, !nonce.isEmpty,
      let tuple = expected["tuple_sha256"] as? String, tuple.count == 64,
      let reviewSHA = expected["review_sha256"] as? String, reviewSHA.count == 64,
      let reason = payload["reason"] as? String, !reason.isEmpty,
      reason.utf8.count <= 4096,
      let shown = try? JSONSerialization.data(withJSONObject: review, options: [.prettyPrinted, .sortedKeys]),
      let shownText = String(data: shown, encoding: .utf8) else { exit(2) }
try? payloadFile.close()

final class ReviewController: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var window: NSWindow!
    var confirm: NSButton!
    var context: LAContext?
    var waiting = false
    let nonce: String
    let reviewSHA: String
    let tuple: String
    let shownText: String
    let expected: [String: Any]
    let reason: String
    let resultFile: FileHandle
    init(nonce: String, reviewSHA: String, tuple: String, shownText: String, expected: [String: Any], reason: String, resultFile: FileHandle) {
        self.nonce = nonce; self.reviewSHA = reviewSHA; self.tuple = tuple
        self.shownText = shownText; self.expected = expected; self.reason = reason
        self.resultFile = resultFile
        super.init()
    }
    func applicationDidFinishLaunching(_ note: Notification) {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 700), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.title = "ThruLine — Review this decision"
        window.delegate = self
        window.center()
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 900, height: 700))
        window.contentView = content
        let scroll = NSScrollView(frame: NSRect(x: 20, y: 84, width: 860, height: 596))
        scroll.autoresizingMask = [.width, .height]
        scroll.hasVerticalScroller = true
        let text = NSTextView(frame: scroll.bounds)
        text.isEditable = false
        text.isSelectable = true
        text.isRichText = false
        text.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        text.string = "Only this native click and fresh macOS authentication can confirm this action.\nOS account UID: \(getuid())\nNonce: \(nonce)\nReview SHA-256: \(reviewSHA)\nTuple SHA-256: \(tuple)\n\n\(shownText)"
        text.autoresizingMask = [.width]
        text.textContainer?.widthTracksTextView = true
        scroll.documentView = text
        content.addSubview(scroll)
        let cancel = NSButton(title: "Cancel", target: self, action: #selector(cancelled))
        cancel.frame = NSRect(x: 20, y: 20, width: 120, height: 44)
        cancel.bezelStyle = .rounded
        content.addSubview(cancel)
        confirm = NSButton(title: "Confirm this exact decision…", target: self, action: #selector(clicked))
        confirm.frame = NSRect(x: 570, y: 20, width: 310, height: 44)
        confirm.autoresizingMask = [.minXMargin]
        confirm.bezelStyle = .rounded
        // No default/Return key or browser callback can activate this action.
        content.addSubview(confirm)
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        Timer.scheduledTimer(withTimeInterval: 110, repeats: false) { _ in self.context?.invalidate(); exit(5) }
    }
    func windowShouldClose(_ sender: NSWindow) -> Bool { cancelled(); return false }
    @objc func cancelled() { context?.invalidate(); exit(1) }
    @objc func clicked() {
        guard !waiting else { return }
        waiting = true
        confirm.isEnabled = false
        let fresh = LAContext()
        fresh.touchIDAuthenticationAllowableReuseDuration = 0
        context = fresh
        var error: NSError?
        guard fresh.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) else { exit(1) }
        fresh.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { success, _ in
            DispatchQueue.main.async {
                guard success else { exit(1) }
                // Only this framework callback writes the exact challenge result.
                guard let output = try? JSONSerialization.data(withJSONObject: self.expected, options: [.sortedKeys]) else { exit(2) }
                do { try self.resultFile.write(contentsOf: output); try self.resultFile.synchronize(); try self.resultFile.close() } catch { exit(2) }
                exit(0)
            }
        }
    }
}
let controller = ReviewController(nonce: nonce, reviewSHA: reviewSHA, tuple: tuple, shownText: shownText, expected: expected, reason: reason, resultFile: resultFile)
let app = NSApplication.shared
app.setActivationPolicy(.regular)
app.delegate = controller
app.run()
exit(1)
