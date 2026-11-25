// A capture session's notifications as the reason a camera track is muted
// (LLP 1133.008 D3): compiled with @exact/media-capture's Apple sources by
// apple.mjs and run on this Mac. AVFoundation posts nothing on cue, so each
// notification is posted by hand on a session with no input, which opens no
// camera and asks for no grant.
//
// - An interruption mutes, and its end unmutes.
// - A runtime error mutes, and the session running again unmutes: AVFoundation
//   posts no interruption's end after one.
// - An interruption that begins while the session restarts after a runtime
//   error stays in force until its own end.
import AVFoundation
import ExactPackage
import Foundation

var failures = 0
func check(_ name: String, _ ok: Bool, _ detail: @autoclosure () -> String = "") {
    if !ok { failures += 1 }
    print("\(ok ? "PASS" : "FAIL") apple-interruptions \(name)\(ok ? "" : ": \(detail())")")
}

/// Runs the main queue until `done` holds or `seconds` pass; whether it held.
func until(_ seconds: Double = 5, _ done: () -> Bool) -> Bool {
    let end = Date().addingTimeInterval(seconds)
    while !done(), Date() < end { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
    return done()
}

@main
struct InterruptionSuite {
    static func main() {
        let session = AVCaptureSession(), queue = DispatchQueue(label: "dev.exact.camera.test.interruptions")
        var told: [String?] = []
        let observers = VideoCapture.listen(session, queue: queue, log: { _ in }, leased: { true }) { told.append($0) }
        let post = { (name: Notification.Name) in NotificationCenter.default.post(name: name, object: session) }
        /// The reason last told, or "none" when none is in force.
        let reason = { told.last.map { $0 ?? "none" } ?? "never told" }

        post(AVCaptureSession.wasInterruptedNotification)
        check("an interruption mutes", until { reason() == "interrupted" }, reason())
        post(AVCaptureSession.interruptionEndedNotification)
        check("its end unmutes, and the session runs", until { reason() == "none" && session.isRunning }, "\(reason()), running=\(session.isRunning)")

        queue.sync { session.stopRunning() }
        told = []
        post(AVCaptureSession.runtimeErrorNotification)
        check("a runtime error mutes", told.first == "interrupted", "\(told)")
        check("the session running again unmutes", until { reason() == "none" && session.isRunning }, "\(reason()), running=\(session.isRunning)")

        queue.sync { session.stopRunning() }
        told = []
        post(AVCaptureSession.runtimeErrorNotification)
        post(AVCaptureSession.wasInterruptedNotification)
        _ = until { session.isRunning }
        _ = until(0.5) { false }
        check("an interruption begun during the restart stays in force", reason() == "interrupted", "\(told)")
        post(AVCaptureSession.interruptionEndedNotification)
        check("until its end", until { reason() == "none" }, reason())

        let admission = CameraStarts()
        queue.sync { session.stopRunning() }
        let app = ExactTrackCause("app")
        queue.suspend()
        admission.update([app], on: queue) { session.startRunning() }
        admission.update([], on: queue) { session.startRunning() }
        queue.resume()
        queue.sync {}
        check("revocation before the capture queue runs leaves the session stopped", !session.isRunning)
        queue.async { session.beginConfiguration(); session.commitConfiguration() }
        admission.update([app], on: queue) { session.startRunning() }
        check("a returning grant starts the configured session", until { session.isRunning })

        var changed = false
        queue.sync {
            VideoCapture.configure(session, stopped: true, admitted: { false }) { _ in changed = true }
        }
        check("a configuration that cannot restart the passive preview leaves it running", !changed && session.isRunning)
        let keptCapture = queue.sync {
            VideoCapture.configure(session, stopped: true, admitted: { !changed }) { _ in changed = true }
        }
        check("revocation during a stopped configuration reports the lost running session", changed && !keptCapture && !session.isRunning)

        for o in observers { NotificationCenter.default.removeObserver(o) }
        queue.sync { session.stopRunning() }
        exit(failures == 0 ? 0 : 1)
    }
}
