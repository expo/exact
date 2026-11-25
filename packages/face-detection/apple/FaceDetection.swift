import ExactPackage
import Foundation
import ImageIO
import Vision
#if os(iOS) && targetEnvironment(simulator)
import CoreML
#endif

public final class FaceDetectionModule: ExactPackage {
    public static var requestHatch: ((VNImageBasedRequest) -> Void)?

    private let context: ExactPackageContext
    private let work = DispatchQueue(label: "exact.face-detection", qos: .userInitiated)

    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        guard name == "detectFaces" else { return reply(["error": "NotSupportedError"]) }
        let limit = (args["maxDetectedFaces"] as? NSNumber)?.doubleValue
        if let limit, !(0...65535).contains(limit) || limit.rounded() != limit { return reply(["error": "TypeError"]) }
        guard let path = args["image"] as? String, let url = context.file(path) else { return reply(["error": "NotFoundError"]) }
        let fast = args["fastMode"] as? Bool == true
        let hatch = !context.agent
        work.async {
            let answer = autoreleasepool { Self.read(url, limit: limit.map(Int.init), fast: fast, hatch: hatch) }
            DispatchQueue.main.async { reply(answer) }
        }
    }

    private static func read(_ url: URL, limit: Int?, fast: Bool, hatch: Bool) -> [String: Any] {
        guard FileManager.default.fileExists(atPath: url.path) else { return ["error": "NotFoundError"] }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: fast ? 1024 : 2048,
        ]
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = props[kCGImagePropertyPixelWidth] as? CGFloat, let h = props[kCGImagePropertyPixelHeight] as? CGFloat,
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        else {
            return FileManager.default.fileExists(atPath: url.path) ? ["error": "InvalidStateError", "message": "decode"] : ["error": "NotFoundError"]
        }
        let rotated = (5...8).contains(props[kCGImagePropertyOrientation] as? Int ?? 1)
        let size = CGSize(width: rotated ? h : w, height: rotated ? w : h)
        let handler = VNImageRequestHandler(cgImage: image)
        // The landmarks request's own detector can omit pitch. Supplying the
        // rectangle request's observations keeps its pose with each face.
        let rectangles = VNDetectFaceRectanglesRequest()
        let landmarks = VNDetectFaceLandmarksRequest()
        #if os(iOS) && targetEnvironment(simulator)
        if #available(iOS 17, *) {
            for request: VNRequest in [rectangles, landmarks] {
                for (stage, devices) in (try? request.supportedComputeStageDevices) ?? [:] {
                    if let cpu = devices.first(where: { if case .cpu = $0 { return true }; return false }) {
                        request.setComputeDevice(cpu, for: stage)
                    }
                }
            }
        }
        #endif
        if hatch, let configure = requestHatch {
            configure(landmarks)
            rectangles.regionOfInterest = landmarks.regionOfInterest
            configure(rectangles)
        }
        do {
            try handler.perform([rectangles])
            let sorted = (rectangles.results ?? []).sorted {
                $0.boundingBox.width * $0.boundingBox.height > $1.boundingBox.width * $1.boundingBox.height
            }
            let selected = Array(sorted.prefix(limit ?? sorted.count))
            var observations: [VNFaceObservation] = []
            if !selected.isEmpty {
                landmarks.inputFaceObservations = selected
                try handler.perform([landmarks])
                observations = landmarks.results ?? []
            }
            let byID = Dictionary(observations.map { ($0.uuid, $0) }, uniquingKeysWith: { first, _ in first })
            let faces = selected.map { rect in
                let observation = byID[rect.uuid] ?? observations.first { $0.boundingBox == rect.boundingBox }
                return shape(rect, features: observation?.landmarks, size: size)
            }
            return ["width": size.width, "height": size.height, "faces": faces]
        } catch let error as NSError {
            if #available(iOS 15, macOS 12, tvOS 15, *), error.domain == VNErrorDomain && error.code == VNErrorCode.unsupportedRequest.rawValue {
                return ["error": "NotSupportedError"]
            }
            return ["error": "OperationError", "message": error.localizedDescription]
        }
    }

    private static func shape(_ face: VNFaceObservation, features: VNFaceLandmarks2D?, size: CGSize) -> [String: Any] {
        let b = face.boundingBox
        let regions = [("eye", features?.leftEye), ("eye", features?.rightEye),
                       ("mouth", features?.outerLips), ("nose", features?.nose)]
        let landmarks: [[String: Any]] = regions.compactMap { type, region in
            guard let region, region.pointCount > 0 else { return nil }
            let points = region.pointsInImage(imageSize: size).map { ["x": $0.x, "y": size.height - $0.y] }
            return ["type": type, "locations": points]
        }
        var result: [String: Any] = [
            "boundingBox": ["x": b.minX * size.width, "y": (1 - b.maxY) * size.height,
                            "width": b.width * size.width, "height": b.height * size.height],
            "landmarks": landmarks,
        ]
        if #available(iOS 15, macOS 12, tvOS 15, *), let yaw = face.yaw, let pitch = face.pitch, let roll = face.roll {
            let degrees = 180.0 / Double.pi
            result["pose"] = ["yaw": yaw.doubleValue * degrees, "pitch": -pitch.doubleValue * degrees, "roll": roll.doubleValue * degrees]
        }
        return result
    }
}
