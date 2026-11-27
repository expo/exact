import ExactPackage
import Foundation

@main
private struct RetireTest {
    static func main() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("exact-transfer-retire-\(UUID().uuidString)")
        let data = root.appendingPathComponent("data", isDirectory: true)
        let cache = root.appendingPathComponent("cache", isDirectory: true)
        let tmp = root.appendingPathComponent("tmp", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for dir in [data, cache, tmp] {
            try! FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        let roots = ["data": data, "cache": cache, "tmp": tmp]
        let resolvePath: (String, (URL, [String]) -> URL?) -> URL? = { path, walk in
            guard path.hasPrefix("app:/") else { return nil }
            let parts = path.dropFirst(5).split(separator: "/", omittingEmptySubsequences: false).map(String.init)
            guard let rootURL = roots[parts[0]] else { return nil }
            return walk(rootURL, Array(parts.dropFirst()))
        }

        var records: [String: [String: Any]] = [:]
        let transfers = ExactTransfers { op, args in
            let id = args["id"] as? String ?? ""
            switch op {
            case "record": return records[id]
            case "retire": records[id] = nil; return nil
            case "ids": return Array(records.keys)
            default: return nil
            }
        }
        let context = ExactPackageContext(
            agent: true, session: "retire-test", data: data, cache: cache, temporary: tmp,
            changed: { _ in }, log: { _ in }, now: { 0 },
            file: { resolvePath($0, AppFileWalk.destination) },
            directory: { resolvePath($0, AppFileWalk.directory) },
            transfers: transfers
        )
        let module = BackgroundTransferModule(context: context)

        func ask(_ name: String, _ args: [String: Any]) -> [String: Any] {
            var answer: [String: Any]?
            module.later(name, args, preferred: nil) { answer = $0 }
            let deadline = Date(timeIntervalSinceNow: 2)
            while answer == nil, Date() < deadline {
                RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
            }
            guard let answer else { fail("\(name) did not reply") }
            return answer
        }

        // 1. Place a completed transfer body into app:/data/transfers/u1/0 and its .placed mark.
        let body = tmp.appendingPathComponent("u1-body.txt")
        try! Data("hello u1".utf8).write(to: body)
        records["u1"] = [
            "id": "u1", "result": "success", "failureReason": "",
            "records": [["index": 0, "url": "https://example.test/u1", "status": 200, "contentType": "text/plain", "bodyLost": false, "path": body.path]],
        ]
        let placed = ask("backgroundFetchRegistration", ["id": "u1"])["exact:answer"] as? [String: Any]
        let placedRecord = (placed?["records"] as? [[String: Any]])?.first
        guard placedRecord?["path"] as? String == "app:/data/transfers/u1/0" else {
            fail("expected placed path, got \(String(describing: placedRecord))")
        }
        let folder = data.appendingPathComponent("transfers/u1")
        guard FileManager.default.fileExists(atPath: folder.appendingPathComponent("0").path),
              FileManager.default.fileExists(atPath: folder.appendingPathComponent("0.placed").path) else {
            fail("expected placed file and marker on disk")
        }

        // Keep a sibling folder to verify empty-id retirement leaves transfers/ intact.
        let sibling = data.appendingPathComponent("transfers/keep")
        try! FileManager.default.createDirectory(at: sibling, withIntermediateDirectories: true)
        try! Data("keep".utf8).write(to: sibling.appendingPathComponent("0"))

        // 2. Retiring u1 removes app:/data/transfers/u1 and its files.
        _ = ask("retireBackgroundFetch", ["id": "u1"])
        guard !FileManager.default.fileExists(atPath: folder.path) else {
            fail("retireBackgroundFetch leaked \(folder.path)")
        }

        // 3. Reusing id "u1" for an aborted transfer with no response body reports no path.
        let saved = data.appendingPathComponent("saved")
        try! FileManager.default.createDirectory(at: saved, withIntermediateDirectories: true)
        try! Data("stale".utf8).write(to: saved.appendingPathComponent("u1.txt"))
        records["u1"] = [
            "id": "u1", "result": "failure", "failureReason": "aborted",
            "records": [["index": 0, "url": "https://example.test/u1", "status": 0, "contentType": "", "bodyLost": false, "saveTo": "app:/data/saved/u1.txt"]],
        ]
        let reused = ask("backgroundFetchRegistration", ["id": "u1"])["exact:answer"] as? [String: Any]
        let reusedRecord = (reused?["records"] as? [[String: Any]])?.first
        guard reusedRecord?["path"] == nil else {
            fail("reused id reported stale path: \(String(describing: reusedRecord?["path"]))")
        }

        // 4. Empty id does not remove app:/data/transfers or sibling folders.
        _ = ask("retireBackgroundFetch", ["id": ""])
        guard FileManager.default.fileExists(atPath: sibling.appendingPathComponent("0").path) else {
            fail("empty id retirement deleted sibling transfer folder")
        }
        print("retire ok")
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
