// The system camera UI's answer on iOS (`takePhotoWithCameraApp`): compiled
// with @exact/camera's Apple sources for the iOS simulator by apple.mjs and run
// there with `simctl spawn`, since the simulator has no camera to show the UI
// with. The picker's `info` for a photo taken with the phone held upright (the
// sensor's pixels landscape, the image `.right`) becomes a JPEG whose `Photo`
// is portrait; `info` with no image is `UnknownError`. With a camera and its
// permission stood in for (`CameraModule.cameraAppPlatform`), a call let go
// or a session ended while the permission is asked shows and answers nothing
// when the answer comes, and keeps no module alive.
import ExactPackage
import Foundation
import ImageIO
import UIKit

@main
struct CameraAppTest {
    static func main() {
        var failed = false
        func check(_ name: String, _ ok: Bool, _ detail: @autoclosure () -> String) {
            if !ok { failed = true }
            print("\(ok ? "PASS" : "FAIL") apple-camera-app \(name)\(ok ? "" : ": \(detail())")")
        }

        let sensor = UIGraphicsImageRenderer(size: CGSize(width: 40, height: 30), format: {
            let f = UIGraphicsImageRendererFormat(); f.scale = 1; return f
        }()).image { ctx in UIColor.orange.setFill(); ctx.fill(CGRect(x: 0, y: 0, width: 40, height: 30)) }.cgImage!
        let held = UIImage(cgImage: sensor, scale: 1, orientation: .right)
        let metadata: [String: Any] = [kCGImagePropertyExifDictionary as String: [kCGImagePropertyExifLensModel as String: "back camera"]]
        switch CameraAppDelegate.kept([.originalImage: held, .mediaMetadata: metadata]) {
        case .success(let data):
            let photo = CameraModule.photo(path: "app:/tmp/camera/1.jpg", data: data)
            check("a kept photo is a JPEG", photo["type"] as? String == "image/jpeg", "\(photo)")
            check("sized as it is shown, upright", photo["width"] as? Int == 30 && photo["height"] as? Int == 40, "\(photo)")
            let props = CGImageSourceCreateWithData(data as CFData, nil).flatMap { CGImageSourceCopyPropertiesAtIndex($0, 0, nil) } as? [CFString: Any]
            let exif = props?[kCGImagePropertyExifDictionary] as? [CFString: Any]
            check("the camera's metadata is kept", exif?[kCGImagePropertyExifLensModel] as? String == "back camera", "\(String(describing: props))")
        case .failure(let f):
            check("a kept photo is a JPEG", false, f.code)
        }
        if case .failure(let f) = CameraAppDelegate.kept([:]) { check("no image is UnknownError", f.code == "UnknownError", f.code) } else { check("no image is UnknownError", false, "an image") }

        var answers: [String] = []
        let delegate = CameraAppDelegate { result in
            if case .failure(let f) = result { answers.append(f.code) } else { answers.append("photo") }
        }
        delegate.finish(.failure(PhotoFailure("AbortError")))
        delegate.finish(.failure(PhotoFailure("UnknownError")))
        check("a cancel is AbortError, answered once", answers == ["AbortError"], "\(answers)")

        // A call let go, or a session ended, while the camera's permission is asked: its late answer shows nothing
        // and answers nothing, the module is not kept alive by it, and the next call asks again.
        var asked: [(Bool) -> Void] = []
        CameraModule.cameraAppPlatform = CameraAppPlatform(available: { true }, described: { true }, status: { .notDetermined }, ask: { asked.append($0) })
        var replies: [[String: Any]] = []
        weak var released: CameraModule?
        do {
            let context = ExactPackageContext(agent: false, session: "s", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: ExactTracks())
            let module = CameraModule(context: context)
            context.request = 7
            module.later("takePhotoWithCameraApp", [:], preferred: nil) { replies.append($0) }
            module.cancel(7)
            context.request = 8
            module.later("takePhotoWithCameraApp", [:], preferred: nil) { replies.append($0) }
            check("after a cancel the next call asks again", asked.count == 2 && replies.isEmpty, "\(asked.count) asked, \(replies)")
            var ended = false
            module.destroy { ended = true }
            check("the session's end lets the asked call go", ended, "destroy did not finish")
            released = module
        }
        for answer in asked { answer(true) }
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        check("a permission answered after the call was let go shows nothing and answers nothing", replies.isEmpty, "\(replies)")
        check("and keeps no module alive", released == nil, "the module is retained")
        exit(failed ? 1 : 0)
    }
}
