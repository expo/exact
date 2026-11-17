// A file face uses the file input's app:/tmp/picked/ landing (LLP 1069.002 D4).
// Compiled only for an Apple face with a file parameter. Copies last until
// the next process launch, including after a caller cancels its tool.
import AppIntents
import Foundation
import UniformTypeIdentifiers

enum IntentFiles {
    static let maximumBytes = 64 << 20

    struct Refusal: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    @MainActor static func importFile(_ file: IntentFile, accept: [String]) async throws -> String {
        try Task.checkCancellation()
        let body = NSMutableDictionary(dictionary: ["name": file.filename])
        let answer: [String: Any] = await withCheckedContinuation { done in
            body["reply"] = { (answer: [String: Any]) in done.resume(returning: answer) }
            NotificationCenter.default.post(name: Notification.Name("exact.tools.pickedPath"), object: body)
            if body["taken"] as? Bool != true { done.resume(returning: ["error": "This app has no file landing."]) }
        }
        try Task.checkCancellation()
        guard let path = answer["path"] as? String, let filePath = answer["file"] as? String else {
            throw Refusal(message: answer["error"] as? String ?? "The app has no temporary file storage.")
        }
        return try copy(file, accept: accept) { _ in (path, URL(fileURLWithPath: filePath)) }
    }

    static func copy(_ file: IntentFile, accept: [String], landing: (String) throws -> (String, URL)) throws -> String {
        let types = accept.compactMap { UTType($0) }
        let type = file.type ?? UTType(filenameExtension: (file.filename as NSString).pathExtension)
        guard types.count == accept.count, types.allSatisfy(\.isDeclared), let type,
              types.contains(where: { type.conforms(to: $0) }) else {
            throw Refusal(message: "The file is not one of the accepted content types.")
        }
        let (path, target) = try landing(file.filename)
        do {
            try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let source = file.fileURL {
                let scoped = source.startAccessingSecurityScopedResource()
                defer { if scoped { source.stopAccessingSecurityScopedResource() } }
                let properties = try source.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
                guard properties.isRegularFile == true, let size = properties.fileSize, size <= maximumBytes else {
                    throw Refusal(message: "expected a regular file of at most 64 MiB.")
                }
                let reader = try FileHandle(forReadingFrom: source)
                defer { try? reader.close() }
                guard FileManager.default.createFile(atPath: target.path, contents: nil) else {
                    throw Refusal(message: "could not create a temporary copy.")
                }
                let writer = try FileHandle(forWritingTo: target)
                defer { try? writer.close() }
                var total = 0
                while let bytes = try reader.read(upToCount: 64 << 10), !bytes.isEmpty {
                    total += bytes.count
                    guard total <= maximumBytes else { throw Refusal(message: "file exceeds 64 MiB.") }
                    try writer.write(contentsOf: bytes)
                }
            } else {
                let bytes = file.data
                guard bytes.count <= maximumBytes else { throw Refusal(message: "file exceeds 64 MiB.") }
                try bytes.write(to: target, options: .withoutOverwriting)
            }
            return path
        } catch {
            try? FileManager.default.removeItem(at: target)
            throw error
        }
    }
}
