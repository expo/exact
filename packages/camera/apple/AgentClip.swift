import AVFoundation
import CoreImage
import CryptoKit
import Foundation
import ImageIO

/// Decoded once; presentation timestamps, rather than a player's wall clock, select the image.
final class AgentClip {
    struct Frame {
        let at: Double
        let image: CGImage
        let jpeg: Data
    }
    let frames: [Frame], duration: Double, sha256: String

    init(url: URL) throws {
        let data = try Data(contentsOf: url)
        sha256 = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        let asset = AVURLAsset(url: url)
        guard let track = asset.tracks(withMediaType: .video).first else {
            throw NSError(domain: "camera.clip has no video track", code: 1)
        }
        let reader = try AVAssetReader(asset: asset)
        let output = AVAssetReaderTrackOutput(track: track,
            outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        reader.add(output)
        guard reader.startReading() else { throw reader.error ?? NSError(domain: "camera.clip did not decode", code: 1) }
        let context = CIContext()
        // Track coordinates have y down; Core Image has y up. createCGImage crops the transformed extent.
        let flip = CGAffineTransform(scaleX: 1, y: -1)
        let transform = flip.concatenating(track.preferredTransform).concatenating(flip)
        var decoded: [Frame] = []
        while let sample = output.copyNextSampleBuffer() {
            guard let buffer = CMSampleBufferGetImageBuffer(sample) else { continue }
            let image = CIImage(cvPixelBuffer: buffer).transformed(by: transform)
            guard let cg = context.createCGImage(image, from: image.extent), let jpeg = Self.jpeg(cg) else {
                reader.cancelReading()
                throw NSError(domain: "camera.clip frame did not decode", code: 1)
            }
            decoded.append(Frame(at: CMSampleBufferGetPresentationTimeStamp(sample).seconds * 1000, image: cg, jpeg: jpeg))
        }
        guard reader.status == .completed, !decoded.isEmpty else {
            throw reader.error ?? NSError(domain: "camera.clip has no decoded frames", code: 1)
        }
        decoded.sort { $0.at < $1.at }
        let first = decoded[0].at
        frames = decoded.map { Frame(at: $0.at - first, image: $0.image, jpeg: $0.jpeg) }
        duration = asset.duration.seconds * 1000
        guard duration.isFinite, duration > 0 else { throw NSError(domain: "camera.clip has no duration", code: 1) }
    }

    func index(_ elapsed: Double) -> Int {
        let at = max(0, elapsed).truncatingRemainder(dividingBy: duration)
        var lo = 0, hi = frames.count
        while lo + 1 < hi {
            let mid = (lo + hi) / 2
            if frames[mid].at <= at { lo = mid } else { hi = mid }
        }
        return lo
    }

    func next(_ elapsed: Double) -> Double {
        let at = max(0, elapsed).truncatingRemainder(dividingBy: duration), i = index(elapsed)
        return (i + 1 < frames.count ? frames[i + 1].at : duration) - at
    }

    private static func jpeg(_ image: CGImage) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, "public.jpeg" as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.92] as CFDictionary)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }
}
