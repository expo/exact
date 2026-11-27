import Darwin
import ExactPackage
import Foundation

@main
private struct DestinationFolderTest {
    static func main() {
        let dir = Bundle.main.executableURL!.deletingLastPathComponent()
        let stubPath = dir.appendingPathComponent("libexact_web.dylib").path
        guard dlopen(stubPath, RTLD_NOW | RTLD_LOCAL) != nil else {
            fail("failed to load stub web arm: \(String(cString: dlerror()))")
        }

        let root = FileManager.default.temporaryDirectory.appendingPathComponent("exact-browser-auto-\(UUID().uuidString)")
        let data = root.appendingPathComponent("data", isDirectory: true)
        let cache = root.appendingPathComponent("cache", isDirectory: true)
        let tmp = root.appendingPathComponent("tmp", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for d in [data, cache, tmp] {
            try! FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        }
        let existingDir = data.appendingPathComponent("downloads", isDirectory: true)
        try! FileManager.default.createDirectory(at: existingDir, withIntermediateDirectories: true)
        let existingFile = data.appendingPathComponent("existing.txt")
        try! Data("hello".utf8).write(to: existingFile)
        let symlink = data.appendingPathComponent("link")
        try! FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: existingDir)

        let roots = ["data": data, "cache": cache, "tmp": tmp]
        let resolvePath: (String, (URL, [String]) -> URL?) -> URL? = { path, walk in
            guard path.hasPrefix("app:/") else { return nil }
            let parts = path.dropFirst(5).split(separator: "/", omittingEmptySubsequences: false).map(String.init)
            guard let rootURL = roots[parts[0]] else { return nil }
            return walk(rootURL, Array(parts.dropFirst()))
        }

        let context = ExactPackageContext(
            agent: false, session: "browser-auto-test", data: data, cache: cache, temporary: tmp,
            changed: { _ in }, log: { _ in }, now: { 0 },
            file: { resolvePath($0, AppFileWalk.destination) },
            directory: { resolvePath($0, AppFileWalk.directory) }
        )
        let module = BrowserAutomationModule(context: context)

        func ask(_ name: String, _ args: [String: Any]) -> [String: Any] {
            var answer: [String: Any]?
            module.later(name, args, preferred: nil) { answer = $0 }
            guard let answer else { fail("\(name) did not reply") }
            return answer
        }

        // Existing directory must be accepted for destinationFolder.
        let r1 = ask("setDownloadBehavior", ["id": "frame-1", "destinationFolder": "app:/data/downloads"])
        guard r1["error"] == nil, r1["destinationFolderFile"] as? String == existingDir.path else {
            fail("existing directory rejected: \(r1)")
        }

        // Missing directory must be accepted for destinationFolder.
        let missingDir = data.appendingPathComponent("new-downloads")
        let r2 = ask("setDownloadBehavior", ["id": "frame-1", "destinationFolder": "app:/data/new-downloads"])
        guard r2["error"] == nil, r2["destinationFolderFile"] as? String == missingDir.path else {
            fail("missing directory rejected: \(r2)")
        }

        // Existing regular file must be rejected for destinationFolder.
        let r3 = ask("setDownloadBehavior", ["id": "frame-1", "destinationFolder": "app:/data/existing.txt"])
        guard r3["error"] as? String == "SyntaxError" else {
            fail("regular file admitted as destinationFolder: \(r3)")
        }

        // Symlink must be rejected for destinationFolder.
        let r4 = ask("setDownloadBehavior", ["id": "frame-1", "destinationFolder": "app:/data/link"])
        guard r4["error"] as? String == "SyntaxError" else {
            fail("symlink admitted as destinationFolder: \(r4)")
        }

        // captureScreenshot path still uses context.file (admits files, rejects directories).
        let r5 = ask("captureScreenshot", ["id": "frame-1", "path": "app:/data/shot.png"])
        guard r5["error"] == nil, r5["pathFile"] as? String == data.appendingPathComponent("shot.png").path else {
            fail("screenshot file rejected: \(r5)")
        }
        let r6 = ask("captureScreenshot", ["id": "frame-1", "path": "app:/data/downloads"])
        guard r6["error"] as? String == "SyntaxError" else {
            fail("directory admitted as screenshot file: \(r6)")
        }

        print("destinationFolder ok")
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
