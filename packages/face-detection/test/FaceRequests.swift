import ExactPackage
import Foundation
import Vision

@main
enum FaceRequests {
    static func main() {
        let source = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        let folder = Bundle.main.url(forResource: "two", withExtension: "jpg", subdirectory: "fixture")?
            .deletingLastPathComponent().deletingLastPathComponent() ?? source
        let portrait = folder.appendingPathComponent("fixture/two.jpg").path
        let module = FaceDetectionModule(context: ExactPackageContext(
            agent: false, session: "portraits", changed: { _ in }, log: { _ in },
            now: { 0 }, file: { URL(fileURLWithPath: $0) }
        ))
        func detect(_ path: String, limit: Int) -> [String: Any] {
            var answer: [String: Any]?
            module.later("detectFaces", ["image": path, "maxDetectedFaces": limit], preferred: nil) { answer = $0 }
            let deadline = Date().addingTimeInterval(20)
            while answer == nil && Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
            return answer ?? ["error": "timeout"]
        }
        var failures = 0
        func check(_ passed: Bool, _ label: String) {
            if !passed { failures += 1 }
            print("\(passed ? "PASS" : "FAIL") \(label)")
        }
        var frontalEyes = [CGPoint]()
        for name in ["frontal", "exif6"] {
            let answer = detect(folder.appendingPathComponent("fixture/\(name).jpg").path, limit: 1)
            guard let faces = answer["faces"] as? [[String: Any]], faces.count == 1,
                  let b = faces[0]["boundingBox"] as? [String: CGFloat],
                  let x = b["x"], let y = b["y"], let w = b["width"], let h = b["height"],
                  let landmarks = faces[0]["landmarks"] as? [[String: Any]] else {
                check(false, "\(name) face, box and landmarks")
                continue
            }
            let box = CGRect(x: x, y: y, width: w, height: h)
            let regions = landmarks.map { landmark in
                (landmark["locations"] as? [[String: CGFloat]] ?? []).compactMap { p -> CGPoint? in
                    guard let px = p["x"], let py = p["y"] else { return nil }
                    return CGPoint(x: px, y: py)
                }
            }
            let eyes = zip(landmarks, regions).compactMap { mark, points in
                mark["type"] as? String == "eye" ? points.first : nil
            }.sorted { $0.x < $1.x }
            check(eyes.count == 2 && regions.allSatisfy { !$0.isEmpty && $0.allSatisfy(box.contains) },
                  "\(name) landmarks inside the face box")
            // Vision on the iOS 26.5 simulator moves eye points between reads of identical pixels.
            #if !(os(iOS) && targetEnvironment(simulator))
            check(eyes.count == 2 && (120..<185).contains(eyes[0].x) && (230..<290).contains(eyes[1].x)
                  && eyes.allSatisfy { (165..<220).contains($0.y) },
                  "\(name) eye positions in upright pixels")
            if name == "frontal" { frontalEyes = eyes }
            else {
                check(frontalEyes.count == 2 && zip(eyes, frontalEyes).allSatisfy {
                    abs($0.x - $1.x) < 12 && abs($0.y - $1.y) < 12
                }, "EXIF preserves eye positions")
            }
            #endif
        }

        for limit in [0, 1, 2] {
            let faces = detect(portrait, limit: limit)["faces"] as? [[String: Any]]
            check(faces?.count == limit, "face limit \(limit)")
        }
        check(detect(portrait, limit: -1)["error"] as? String == "TypeError", "invalid limit")
        check(detect(folder.appendingPathComponent("missing.jpg").path, limit: 1)["error"] as? String == "NotFoundError", "missing image")
        check(detect(folder.appendingPathComponent("exact.json").path, limit: 1)["error"] as? String == "InvalidStateError", "decode failure")

        var requests = [VNImageBasedRequest]()
        FaceDetectionModule.requestHatch = { request in
            requests.append(request)
            if request is VNDetectFaceLandmarksRequest {
                request.regionOfInterest = CGRect(x: 0.5, y: 0, width: 0.5, height: 1)
            }
        }
        let answer = detect(portrait, limit: 1)
        let face = (answer["faces"] as? [[String: Any]])?.first
        let box = face?["boundingBox"] as? [String: CGFloat]
        check(requests.count == 2 && requests[0] is VNDetectFaceLandmarksRequest && requests[1] is VNDetectFaceRectanglesRequest, "both Vision requests reach the hatch")
        check((box?["x"] ?? 0) > 360, "right-hand portrait inside the search region")
        check(!(face?["landmarks"] as? [[String: Any]] ?? []).isEmpty, "landmarks on the selected portrait")
        exit(failures == 0 ? 0 : 1)
    }
}
