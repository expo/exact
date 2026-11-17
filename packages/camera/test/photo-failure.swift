// A capture that fails answers `takePhoto` with the error's name (LLP
// 1133.008 D1): compiled with @exact/camera's Apple sources by apple.mjs and
// run on this Mac. The track's source is a session with no input, so its
// photo output's connection is not active and the capture fails
// `InvalidStateError`; the reply must be a JSON object, as the host's
// `ExactReply.send` requires. The camera is made through the Objective-C
// runtime, so the test needs no camera and asks for no grant; it answers only
// what `takePhoto` reads of it.
import AVFoundation
import ExactPackage
import Foundation

final class NoFormat: AVCaptureDevice.Format {}
final class NoCamera: AVCaptureDevice {
    override var activeFormat: AVCaptureDevice.Format {
        get { class_createInstance(NoFormat.self, 0) as! AVCaptureDevice.Format }
        set {}
    }
}

final class Source: ExactVideoSource {
    let session = AVCaptureSession()
    let device: AVCaptureDevice
    private let queue = DispatchQueue(label: "dev.exact.camera.test.photo")
    private var objects: [String: AnyObject] = [:]
    init(_ device: AVCaptureDevice) { self.device = device }
    func configure(stopped: Bool, _ change: @escaping (AVCaptureSession) -> Void) {
        queue.async { [session] in session.beginConfiguration(); change(session); session.commitConfiguration() }
    }
    func run(_ work: @escaping (AVCaptureSession) -> Void) { queue.async { [session] in work(session) } }
    func shared<T: AnyObject>(_ key: String, _ make: () -> T) -> T {
        if let o = objects[key] as? T { return o }
        let o = make()
        objects[key] = o
        return o
    }
    func apply(_ constraints: [String: Any]) {}
    func focus(at point: CGPoint) {}
}

@main
struct PhotoFailureSuite {
    static func main() {
        // `supportedMaxPhotoDimensions` is bridged from an Objective-C array, so it is replaced at that level.
        let none: @convention(block) (AnyObject) -> NSArray = { _ in NSArray() }
        class_replaceMethod(NoFormat.self, NSSelectorFromString("supportedMaxPhotoDimensions"), imp_implementationWithBlock(none), "@@:")
        let tracks = ExactTracks(), entry = ExactTrackEntry(id: "t", kind: "video")
        let source = Source(class_createInstance(NoCamera.self, 0) as! AVCaptureDevice)
        entry.state = { ["readyState": "live", "enabled": true, "muted": false] }
        entry.video = { source }
        tracks.add(entry)
        let module = CameraModule(context: ExactPackageContext(agent: false, session: "s", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: tracks))
        var reply: [String: Any]?
        module.later("takePhoto", ["track": "t"], preferred: nil) { reply = $0 }
        let end = Date().addingTimeInterval(5)
        while reply == nil, Date() < end { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
        let ok = reply.map(JSONSerialization.isValidJSONObject) == true && reply?["error"] as? String == "InvalidStateError"
        print("\(ok ? "PASS" : "FAIL") apple-photo-failure an inactive connection answers {\"error\":\"InvalidStateError\"}\(ok ? "" : ": \(String(describing: reply))")")
        exit(ok ? 0 : 1)
    }
}
