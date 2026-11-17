#if os(macOS)
import Darwin
import Foundation
import ImageIO
import UniformTypeIdentifiers

struct CaptureFailure: Error {
    let code: String
    let message: String
    init(_ code: String, _ message: String) { self.code = code; self.message = message }
    var answer: [String: Any] { ["error": code, "message": message] }
}

/// A held parent and a private temporary file: cancellation or an encode
/// failure drops the temporary without replacing the previous still.
final class CaptureFile {
    private let parent: Int32
    private let leaf: String
    private let temporary = ".exact-capture-\(UUID().uuidString)"
    private var published = false

    init(_ destination: URL, root: URL) throws {
        leaf = destination.lastPathComponent
        let prefix = root.path + "/"
        guard destination.path.hasPrefix(prefix) else { throw CaptureFailure("NotAllowedError", "capture path escaped its root") }
        let parts = destination.path.dropFirst(prefix.count).split(separator: "/").map(String.init)
        guard !parts.isEmpty, !parts.contains(where: { $0 == "." || $0 == ".." || $0.contains("\0") }) else {
            throw CaptureFailure("NotAllowedError", "invalid capture path")
        }
        var directory = open(root.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { throw CaptureFailure("NotAllowedError", "app directory is not accessible") }
        for name in parts.dropLast() {
            if mkdirat(directory, name, 0o700) != 0 && errno != EEXIST {
                close(directory)
                throw CaptureFailure("EncodingError", "could not create the capture directory")
            }
            let next = openat(directory, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            close(directory)
            guard next >= 0 else { throw CaptureFailure("NotAllowedError", "capture directory contains a link or non-directory") }
            directory = next
        }
        parent = directory
    }

    deinit {
        if !published { unlinkat(parent, temporary, 0) }
        close(parent)
    }
    func write(_ bytes: Data) throws {
        let fd = openat(parent, temporary, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard fd >= 0 else { throw CaptureFailure("EncodingError", "could not create the capture file") }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        try handle.write(contentsOf: bytes)
        try handle.close()
    }
    func publish() throws {
        var info = stat()
        let exists = fstatat(parent, leaf, &info, AT_SYMLINK_NOFOLLOW) == 0
        guard !exists || info.st_mode & S_IFMT == S_IFREG else {
            throw CaptureFailure("NotAllowedError", "capture destination is not a regular file")
        }
        guard renameat(parent, temporary, parent, leaf) == 0 else {
            throw CaptureFailure("EncodingError", "could not publish the capture file")
        }
        published = true
    }
}

func captureEncoding(_ args: [String: Any]) -> (UTType, Double) {
    let type = (args["type"] as? String ?? "image/png").lowercased()
    let named = type.hasPrefix("image/") ? UTType(mimeType: type) : nil
    let encodable = CGImageDestinationCopyTypeIdentifiers() as? [String] ?? []
    let uti = named.flatMap { encodable.contains($0.identifier) ? $0 : nil } ?? .png
    let quality = args["quality"] as? Double
    return (uti, quality.flatMap { (0...1).contains($0) ? $0 : nil } ?? 0.92)
}

func encodeCapture(_ image: CGImage, _ type: UTType, _ quality: Double) throws -> Data {
    let data = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil) else {
        throw CaptureFailure("EncodingError", "no image encoder")
    }
    let properties: [CFString: Any] = type == .png
        ? [kCGImagePropertyPNGDictionary: [kCGImagePropertyPNGCompressionFilter: IMAGEIO_PNG_FILTER_UP]]
        : [kCGImageDestinationLossyCompressionQuality: quality]
    CGImageDestinationAddImage(destination, image, properties as CFDictionary)
    guard CGImageDestinationFinalize(destination) else { throw CaptureFailure("EncodingError", "image encoding failed") }
    return data as Data
}

/// One queue for publication order across sessions that share app files.
enum CaptureQueue {
    private static var jobs: [(@escaping () -> Void) -> Void] = []
    private static var busy = false
    static func add(_ job: @escaping (@escaping () -> Void) -> Void) {
        jobs.append(job)
        next()
    }
    private static func next() {
        guard !busy, !jobs.isEmpty else { return }
        busy = true
        let job = jobs.removeFirst()
        job {
            busy = false
            next()
        }
    }
}

#endif
