import Foundation

/// One complete generation's resolver. Stored bytes are verified once and
/// retained; an absent name never falls through to another generation.
public final class AssetResolver {
    package let root: URL
    package let publicationID: String
    private let names: [String]?
    private let read: ((String) throws -> Data?)?
    package var isComplete: Bool { names != nil }
    private var cache: [String: Data] = [:]
    private var files: [String: URL] = [:]
    private var directory: URL?
    private let lock = NSRecursiveLock()
    private var lastRefusal: String?
    /// The root with its symlinks resolved, once: resolving reads each component.
    private lazy var base = root.standardizedFileURL.resolvingSymlinksInPath()
    package var refusal: String? { lock.lock(); defer { lock.unlock() }; return lastRefusal }
    package var encodedCacheBytes: Int { lock.lock(); defer { lock.unlock() }; return cache.values.reduce(0) { $0 + $1.count } }

    /// A complete provider never falls through to the embedded directory.
    public init(root: URL, names: [String], generation: String = UUID().uuidString, read: @escaping (String) throws -> Data?) {
        self.root = root; self.names = names; self.read = read; publicationID = generation
    }

    /// Own verified payloads without retaining texture heap buffers. The private
    /// files live exactly as long as this generation and can be read after loss.
    package init(root: URL, verified: [String: Data]) throws {
        self.root = root; names = Array(verified.keys); read = nil; publicationID = UUID().uuidString
        for (name, bytes) in verified {
            guard Self.validAssetName(name.hasPrefix("assets/") ? String(name.dropFirst(7)) : name) else {
                throw CocoaError(.fileReadInvalidFileName)
            }
            if name.hasSuffix(".tex") { _ = try materialize(name, bytes) }
            else { cache[name] = bytes }
        }
    }

    private func materialize(_ name: String, _ bytes: Data) throws -> URL {
        if directory == nil {
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exact-generation-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
            directory = dir
        }
        let filename = Data(name.utf8).base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let url = directory!.appendingPathComponent(filename)
        try bytes.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o400], ofItemAtPath: url.path)
        files[name] = url
        return url
    }

    package init(root: URL) {
        self.root = root; names = nil; read = nil
        publicationID = "embedded:" + (Bundle.main.object(forInfoDictionaryKey: "ExactWidgetGeneration") as? String ?? root.path)
    }

    deinit { if let directory { try? FileManager.default.removeItem(at: directory) } }

    package func bytes(_ name: String) -> Data? {
        do { return try delivery(name) } catch { refuse(error.localizedDescription); return nil }
    }

    private func refuse(_ why: String) {
        lock.lock(); defer { lock.unlock() }
        lastRefusal = lastRefusal ?? why
    }

    private enum Source {
        case file(URL, embedded: Bool)
        case provider((String) throws -> Data?)
    }

    /// Safe on any thread. The bytes are read outside the lock, from a file or
    /// the provider, so a read that blocks (a texture a GPU canvas reads off the
    /// main thread, a file a privacy check holds) holds up no other lookup.
    package func delivery(_ name: String) throws -> Data? {
        let source: Source
        do {
            lock.lock(); defer { lock.unlock() }
            guard Self.validAssetName(name.hasPrefix("assets/") ? String(name.dropFirst(7)) : name) else { throw CocoaError(.fileReadInvalidFileName) }
            if let bytes = cache[name] { return bytes }
            if let url = files[name] { source = .file(url, embedded: false) }
            else if let read {
                guard names?.contains(name) == true else { return nil }
                source = .provider(read)
            } else {
                guard !isComplete, let url = embeddedURL(name) else { return nil }
                source = .file(url, embedded: true)
            }
        }
        switch source {
        case .provider(let read):
            let bytes: Data?
            do { bytes = try read(name) } catch { refuse(error.localizedDescription); return nil }
            if let bytes, !name.hasSuffix(".tex") { lock.lock(); cache[name] = bytes; lock.unlock() }
            return bytes
        case .file(let file, let embedded):
            do { return try Data(contentsOf: file) }
            catch let error as CocoaError where embedded && error.code == .fileReadNoSuchFile { return nil }
        }
    }

    package func url(_ name: String, maximumBytes: Int? = nil) -> URL? {
        do {
            lock.lock(); defer { lock.unlock() }
            guard isComplete else { return embeddedURL(name) }
            if let url = files[name] { return url }
        }
        // Read outside the lock, as `bytes` does; materialize under it.
        guard let bytes = bytes(name) else { return nil }
        if let maximumBytes, bytes.count > maximumBytes { return nil }
        lock.lock(); defer { lock.unlock() }
        if let url = files[name] { return url } // another thread materialized it meanwhile
        do {
            if directory == nil {
                let dir = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true).appendingPathComponent("exact-generation-\(UUID().uuidString)", isDirectory: true)
                try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
                directory = dir
            }
            let url = directory!.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try bytes.write(to: url, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o400], ofItemAtPath: url.path)
            files[name] = url
            return url
        } catch { lastRefusal = lastRefusal ?? "verified asset materialization failed: \(error)"; return nil }
    }

    /// The asset's file in the signed bundle, when this is the embedded
    /// generation and the file is there; a complete generation has none.
    package func bundledURL(_ name: String) -> URL? {
        lock.lock(); defer { lock.unlock() }
        guard !isComplete, let url = embeddedURL(name), FileManager.default.fileExists(atPath: url.path) else { return nil }
        return url
    }

    /// The complete shader names, without loading the optional GPU module.
    package func shaderSources() -> [String: Data] {
        let names: [String]
        if let completeNames = self.names { names = completeNames.filter { $0.hasPrefix("shaders/") && $0.hasSuffix(".wgsl") } }
        else {
            let fm = FileManager.default
            let prefix = fm.fileExists(atPath: root.appendingPathComponent("shaders").path) ? "shaders" : "gpu/shaders"
            names = ((try? fm.contentsOfDirectory(atPath: root.appendingPathComponent(prefix).path)) ?? []).filter { $0.hasSuffix(".wgsl") }.map { prefix + "/" + $0 }
        }
        var sources: [String: Data] = [:]
        for name in names { if let data = bytes(name) { sources[URL(fileURLWithPath: name).deletingPathExtension().lastPathComponent] = data } }
        return sources
    }

    /// The same cases and byte limit as gpu::asset_name.
    package static func validAssetName(_ name: String) -> Bool {
        let bytes = Array(name.utf8)
        return !bytes.isEmpty && bytes.count <= 128
            && bytes.allSatisfy { $0 >= 32 && $0 < 127 && $0 != 92 }
            && name.split(separator: "/", omittingEmptySubsequences: false).allSatisfy { !$0.isEmpty && $0 != "." && $0 != ".." }
    }

    private func embeddedURL(_ name: String) -> URL? {
        guard Self.validAssetName(name.hasPrefix("assets/") ? String(name.dropFirst(7)) : name) else { return nil }
        let url = base.appendingPathComponent(name).standardizedFileURL.resolvingSymlinksInPath()
        return url.path.hasPrefix(base.path + "/") ? url : nil
    }
}

extension AssetResolver {
    package func widgetNames() throws -> [String] {
        if let names { return names }
        let fm = FileManager.default
        var names: [String] = []
        for tree in ["plans", "assets", "typescript", "activities"] {
            let base = root.appendingPathComponent(tree, isDirectory: true)
            guard let walker = fm.enumerator(at: base, includingPropertiesForKeys: [.isRegularFileKey]) else { continue }
            for case let file as URL in walker where try file.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile == true {
                names.append(tree + "/" + String(file.path.dropFirst(base.path.count + 1)))
            }
        }
        return names
    }
}
