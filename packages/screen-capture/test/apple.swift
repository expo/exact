import AppKit
import ExactPackage
import ImageIO

@main
struct CaptureTests {
    static func main() throws {
        _ = NSApplication.shared
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("exact-screen-capture-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var pictures: [String] = []
        let context = ExactPackageContext(agent: true, session: "still-test", data: root, cache: root, temporary: root,
            changed: { _ in }, log: { _ in }, now: { 0 }, file: { path in
                AppFileWalk.openRootDestination(path, root: root)
            }, picture: { _, source in pictures.append(source) })
        let module = ScreenCaptureModule(context: context)
        let screens: [[String: Any]] = [
            ["left":0,"top":0,"width":64,"height":40,"devicePixelRatio":1.25],
            ["left":-80,"top":-10,"width":80,"height":40,"devicePixelRatio":2,
             "clientRect":["x":-128,"y":-16,"width":128,"height":64]],
        ]
        module.preferred(["screens":screens])
        var number = 0
        func capture(cancel: Bool = false, args: [String: Any] = [:]) -> [String: Any] {
            number += 1
            context.request = number
            var result: [String: Any]?
            var given: [String: Any] = ["directory":"app:/tmp/freeze"]
            given.merge(args) { _, new in new }
            module.later("captureScreens", given, preferred: nil) { result = $0 }
            if cancel { module.cancel(number) }
            let deadline = Date().addingTimeInterval(10)
            while result == nil && Date() < deadline { _ = RunLoop.main.run(mode: .default, before: deadline) }
            precondition(result != nil, "capture must finish")
            return result!
        }
        var release: (() -> Void)?
        CaptureQueue.add { release = $0 }
        var granted = true, enumerations = 0
        let revoked = ScreenCaptureModule(context: ExactPackageContext(agent: false, session: "revoked",
            data: root, cache: root, temporary: root, changed: { _ in }, log: { _ in }, now: { 0 },
            file: { AppFileWalk.openRootDestination($0, root: root) }), preflight: { granted }, content: { done in
                enumerations += 1
                done(nil, CaptureFailure("EncodingError", "permission revoked during the queue"))
            })
        var refusal: [String: Any]?
        revoked.later("captureScreens", ["directory":"app:/tmp/revoked"], preferred: nil) { refusal = $0 }
        granted = false
        release?()
        let deadline = Date().addingTimeInterval(10)
        while refusal == nil && Date() < deadline { _ = RunLoop.main.run(mode: .default, before: deadline) }
        precondition(refusal?["error"] as? String == "NotAllowedError", "revoked capture: \(String(describing: refusal))")
        precondition(enumerations == 0, "revoked capture must not call ScreenCaptureKit")
        revoked.destroy({})
        let answer = capture()
        guard let captures = answer["captures"] as? [[String: Any]] else { fatalError("\(answer)") }
        precondition(captures.count == 2)
        precondition(captures[0]["width"] as? Int == 160 && captures[0]["height"] as? Int == 80)
        precondition(captures[1]["width"] as? Int == 80 && captures[1]["height"] as? Int == 50)
        precondition(pictures == ["app:/tmp/freeze/screen-0.png", "app:/tmp/freeze/screen-1.png"], "published files must refresh images")
        let first = root.appendingPathComponent("freeze/screen-0.png")
        let bytes = try Data(contentsOf: first)
        let source = CGImageSourceCreateWithData(bytes as CFData, nil)!
        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)! as NSDictionary
        precondition(properties[kCGImagePropertyPixelWidth] as? Int == 160)
        precondition(properties[kCGImagePropertyPixelHeight] as? Int == 80)
        let decoded = CGImageSourceCreateImageAtIndex(source, 0, nil)!
        let rgba = CGContext(data: nil, width: 160, height: 80, bitsPerComponent: 8, bytesPerRow: 160 * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        rgba.draw(decoded, in: CGRect(x: 0, y: 0, width: 160, height: 80))
        let pixels = rgba.data!.assumingMemoryBound(to: UInt8.self)
        precondition(Array(UnsafeBufferPointer(start: pixels, count: 4)) == [32,64,128,255])
        precondition(Array(UnsafeBufferPointer(start: pixels + (8 * 160 + 8) * 4, count: 4)) == [255,255,255,255])
        precondition(capture(cancel: true)["error"] as? String == "AbortError")
        let afterCancel = try Data(contentsOf: first)
        precondition(afterCancel == bytes)
        precondition(pictures.count == 2, "cancelled captures publish no image changes")
        module.preferred(["screens":[["left":0,"top":0,"width":32,"height":32,"devicePixelRatio":1]]])
        precondition(capture()["captures"] != nil)
        let replaced = try Data(contentsOf: first)
        precondition(replaced != bytes)
        precondition(pictures == ["app:/tmp/freeze/screen-0.png", "app:/tmp/freeze/screen-1.png", "app:/tmp/freeze/screen-0.png"])
        module.preferred(["screens":screens])
        let jpeg = capture(args: ["type":"image/jpeg", "quality":0.5])
        precondition((jpeg["captures"] as? [[String: Any]])?[0]["type"] as? String == "image/jpeg")
        module.preferred(["screens":[["width":100000,"height":100000,"devicePixelRatio":1]]])
        precondition(capture()["error"] as? String == "QuotaExceededError")
        module.preferred(["screens":screens])
        let outside = root.appendingPathComponent("outside")
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("linked"), withDestinationURL: outside)
        precondition(capture(args:["directory":"app:/tmp/linked"])["error"] as? String == "NotAllowedError")
        let outsideFiles = try FileManager.default.contentsOfDirectory(atPath: outside.path)
        precondition(outsideFiles.isEmpty)
        let real = ScreenCaptureModule(context: ExactPackageContext(agent: false, session: "status", changed: { _ in }, log: { _ in }, now: { 0 }))
        real.later("screenRecordingStatus", [:], preferred: nil) { status in
            precondition(status["available"] as? Bool == true)
            precondition(["prompt","denied","granted"].contains(status["state"] as? String ?? ""))
            print("PASS macOS permission preflight: \(status["state"]!) (no permission requested)")
        }
        real.destroy({})
        module.destroy({})
        print("PASS macOS fixture pixels, physical sizes, cancellation, JPEG, quota, and link refusal")
    }
}

extension AppFileWalk {
    static func openRootDestination(_ path: String, root: URL) -> URL? {
        guard path.hasPrefix("app:/tmp/") else { return nil }
        return destination(root, path.dropFirst("app:/tmp/".count).split(separator: "/").map(String.init))
    }
}
