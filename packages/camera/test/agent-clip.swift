import AVFoundation
import ExactPackage
import Foundation
import ImageIO

final class AgentClipTrack: ExactTrackOwner {
    func subscribe(_ id: String, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
        consumer.live(video: nil)
        return ExactSubscription {}
    }
}

@main
struct AgentClipSuite {
    static func colors(_ image: CGImage) -> [Int] {
        var pixels = [UInt8](repeating: 0, count: 16)
        pixels.withUnsafeMutableBytes { bytes in
            let canvas = CGContext(data: bytes.baseAddress, width: 2, height: 2, bitsPerComponent: 8, bytesPerRow: 8,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            canvas.draw(image, in: CGRect(x: 0, y: 0, width: 2, height: 2))
        }
        return stride(from: 0, to: 16, by: 4).map { at in (0..<3).max { pixels[at + $0] < pixels[at + $1] }! }
    }

    static func main() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
        let rotatedURL = root.appendingPathComponent("fixture/rotated.mp4")
        let reference = AVAssetImageGenerator(asset: AVURLAsset(url: rotatedURL))
        reference.appliesPreferredTrackTransform = true
        let expected = try reference.copyCGImage(at: .zero, actualTime: nil)
        precondition(expected.width == 32 && expected.height == 16 && colors(expected) == [0, 0, 1, 1])
        let rotated = try AgentClip(url: rotatedURL)
        for frame in rotated.frames {
            let source = CGImageSourceCreateWithData(frame.jpeg as CFData, nil)!
            let photo = CGImageSourceCreateImageAtIndex(source, 0, nil)!
            for image in [frame.image, photo] {
                precondition(image.width == expected.width && image.height == expected.height)
                precondition(colors(image) == colors(expected), "rotated frame: \(colors(image)); expected \(colors(expected))")
            }
        }
        let clip = try AgentClip(url: root.appendingPathComponent("fixture/frames.mp4"))
        precondition(clip.frames.count == 3 && abs(clip.duration - 300) < 0.01)
        precondition([0.0, 99, 100, 200, 300, 1100].map(clip.index) == [0, 0, 1, 2, 0, 2])
        precondition(clip.frames.allSatisfy { $0.image.width == 16 })
        var now = 500.0
        let owner = AgentClipTrack()
        let tracks = ExactTracks(), entry = ExactTrackEntry(id: "t", kind: "video", owner: owner)
        entry.startedAt = now
        entry.state = { ["readyState": "live", "enabled": true, "muted": false] }
        tracks.add(entry)
        var logs: [String] = []
        let out = URL(fileURLWithPath: CommandLine.arguments[1])
        var pending: (() -> Void)?
        let module = CameraModule(context: ExactPackageContext(agent: true, session: "s", changed: { _ in },
            log: { logs.append($0) }, now: { now }, tracks: tracks, file: { _ in out }, after: { _, coalescing, body in
                precondition(coalescing)
                pending = body
                return { pending = nil }
            }))
        setenv("EXACT_ASSETS", root.path, 1)
        module.preferred(["clip": ["path": "fixture/frames.mp4"]])
        let card = try Data(contentsOf: root.appendingPathComponent("fixture/card.jpg"))
        precondition(module.agentPictureData() == card)
        let view = try module.view("exact-camera", id: "finder", props: ["srcObject": "t"],
            events: ExactPackageEvents(send: { _, _ in }, size: { _ in }))
        tracks.commitViewBindings(["exact-camera": ["finder": ["srcObject": ExactViewBinding(value: "t", origin: "app")]]])?()
        var changes = 0
        let observation = entry.observePicture { changes += 1 }
        var images: [CGImage] = []
        var hashes: [Data] = []
        for at in [500.0, 599, 600, 700, 800, 1600] {
            now = at
            pending?()
            images.append(entry.picture!)
            var answer: [String: Any] = [:]
            module.later("takePhoto", ["track": "t"], preferred: nil) { answer = $0 }
            precondition(answer["width"] as? Int == 16)
            hashes.append(try Data(contentsOf: out))
        }
        precondition(changes == 4)
        precondition(images[0] === images[1] && images[0] === images[4] && images[3] === images[5])
        precondition(images[0] !== images[2] && images[2] !== images[3])
        precondition(hashes[0] == hashes[1] && hashes[0] == hashes[4] && hashes[3] == hashes[5])
        precondition(hashes[0] != hashes[2] && hashes[2] != hashes[3])
        module.preferred(["clip": ["path": "fixture/frames.mp4"], "picture": ["path": "fixture/card.jpg"]])
        precondition(logs.contains { $0.contains("camera.picture and camera.clip") })
        module.preferred(["picture": ["path": "fixture/card.jpg"]])
        precondition(module.agentPicture()?.width == 1280)
        observation.cancel()
        withExtendedLifetime(view) { module.destroy {} }
        precondition(pending == nil)
        print("PASS apple-agent-clip display transform, clock boundaries, looping, current photo, conflict and still")
    }
}
