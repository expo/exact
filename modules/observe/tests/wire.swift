// Feeds wire.json's events through ObserveService and prints the metrics and logs
// bodies it would send, as {"metrics": …, "logs": …}. wire.test.mjs compiles and runs it.
import Foundation
import SQLite3

// Observe loads on the UI thread; record SQLite opens without changing their behavior
private enum SQLiteThreads {
    static let lock = NSLock()
    static var mainOpens = 0
    static var totalOpens = 0
    static let original = sqlite3_vfs_find(nil)!
    static let monitored: UnsafeMutablePointer<sqlite3_vfs> = {
        let copy = UnsafeMutablePointer<sqlite3_vfs>.allocate(capacity: 1)
        copy.initialize(to: original.pointee)
        copy.pointee.zName = UnsafePointer(strdup("observe-wire-threads"))
        copy.pointee.xOpen = { _, name, file, flags, output in
            lock.lock()
            totalOpens += 1
            if Thread.isMainThread { mainOpens += 1 }
            lock.unlock()
            return original.pointee.xOpen!(original, name, file, flags, output)
        }
        return copy
    }()

    static func install() {
        precondition(sqlite3_vfs_register(monitored, 1) == SQLITE_OK)
    }

    static func counts() -> [String: Int] {
        lock.lock(); defer { lock.unlock() }
        return ["mainOpens": mainOpens, "totalOpens": totalOpens]
    }
}

@main
enum WireHarness {
    static func main() throws {
        let args = CommandLine.arguments
        let fixture = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: args[1]))) as! [String: Any]
        let root = URL(fileURLWithPath: args[2], isDirectory: true)
        let app = fixture["app"] as! [String: Any]
        let config: [String: Any] = [
            "session": fixture["session"]!, "sessionStart": fixture["sessionStart"]!, "environment": fixture["environment"]!,
            "app.CFBundleIdentifier": app["id"]!, "app.CFBundleName": app["name"]!,
        ]
        // The handoff is length-prefixed JSON records. The device record holds the state at TTI.
        let record = try JSONSerialization.data(withJSONObject: ["kind": "device", "params": fixture["device"]!])
        var handoff = withUnsafeBytes(of: UInt32(record.count).littleEndian) { Data($0) }
        handoff.append(record)
        SQLiteThreads.install()
        let began = DispatchTime.now().uptimeNanoseconds
        let service = ObserveService(config: config, handoff: handoff, root: root, clientId: UUID(uuidString: fixture["clientId"] as! String)!)
        let constructed = DispatchTime.now().uptimeNanoseconds
        for e in fixture["events"] as! [Any] { service.event(try JSONSerialization.data(withJSONObject: e)) }
        var out: [String: Any] = service.queue.sync {
            var out: [String: Any] = ["metrics": service.chunk("metrics", limit: 200)?.body ?? [:], "logs": service.chunk("logs", limit: 200)?.body ?? [:],
                                     "recent": service.query(["op": "recent", "limit": 3]), "unknown": service.query(["op": "everything"])]
            service.dispatch()
            out["refused"] = service.query(["op": "recent", "limit": 500])
            if let first = service.store?.rows("SELECT MIN(id) FROM metrics", []).first?.first as? Int64 {
                service.store?.acknowledge("metrics", from: first, through: first)
            }
            out["acknowledged"] = service.query(["op": "recent", "limit": 500])
            return out
        }
        let ready = DispatchTime.now().uptimeNanoseconds
        out["initialization"] = ["returnedMs": Double(constructed - began) / 1_000_000, "readyMs": Double(ready - began) / 1_000_000,
                                 "threads": SQLiteThreads.counts()] as [String: Any]
        out["retry"] = try retryState(fixture, config: config, root: root.appendingPathComponent("retry"))
        out["crash"] = try crashState(fixture, config: config, root: root.appendingPathComponent("crash"))
        FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: out))
    }

    /// A pending crash record across a launch whose store refuses the row, then one that stores it.
    private static func crashState(_ fixture: [String: Any], config: [String: Any], root: URL) throws -> [String: Any] {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let record = root.appendingPathComponent("exact-observe-pending-crashed.json")
        try JSONSerialization.data(withJSONObject: ["session": "crashed", "sessionStart": 1, "signal": 11]).write(to: record)
        let clientId = UUID(uuidString: fixture["clientId"] as! String)!
        let crashes = { (service: ObserveService) in
            service.queue.sync { service.store?.rows("SELECT COUNT(*) FROM logs WHERE name = 'native.exception'", []).first?.first as? Int64 ?? -1 }
        }
        let schema = ObserveStore(directory: root)!
        schema.run("CREATE TRIGGER refuse BEFORE INSERT ON logs BEGIN SELECT RAISE(ABORT, 'refused'); END", [])
        let refused = ObserveService(config: config, handoff: Data(), root: root, clientId: clientId)
        let refusedRows = crashes(refused)
        let keptAfterRefusal = FileManager.default.fileExists(atPath: record.path)
        schema.run("DROP TRIGGER refuse", [])
        let stored = ObserveService(config: config, handoff: Data(), root: root, clientId: clientId)
        return ["refusedRows": refusedRows, "keptAfterRefusal": keptAfterRefusal,
                "storedRows": crashes(stored), "keptAfterStore": FileManager.default.fileExists(atPath: record.path)]
    }

    private static func retryState(_ fixture: [String: Any], config: [String: Any], root: URL) throws -> [String: Any] {
        precondition(URLProtocol.registerClass(RetryTransport.self))
        defer { URLProtocol.unregisterClass(RetryTransport.self) }
        var config = config
        config["projectId"] = "fixture"
        config["endpoint"] = "https://observe.example"
        let service = ObserveService(config: config, handoff: Data(), root: root,
                                     clientId: UUID(uuidString: fixture["clientId"] as! String)!)
        for event in fixture["events"] as! [Any] {
            service.event(try JSONSerialization.data(withJSONObject: event))
        }
        // Let the initial flush wake itself; no event arrives after the failed request
        try waitForIdle(service, requests: 1)
        var result = service.queue.sync { ["scheduled": service.scheduled, "requests": RetryTransport.paths] as [String: Any] }
        result["earlyFlushRearms"] = service.queue.sync {
            // Model a flush that woke before the retry deadline
            service.scheduled = false
            service.dispatch()
            return service.scheduled
        }
        service.queue.sync {
            // Model the scheduled retry waking at its deadline without waiting a minute
            service.gate.after = Date.distantPast
            service.scheduled = false
            service.dispatch()
        }
        try waitForIdle(service, requests: 3)
        result["resent"] = service.queue.sync { service.query(["op": "recent", "limit": 500]) }
        result["finalRequests"] = RetryTransport.paths
        return result
    }

    private static func waitForIdle(_ service: ObserveService, requests: Int) throws {
        let finished = DispatchSemaphore(value: 0)
        let observer = DispatchSource.makeTimerSource(queue: service.queue)
        observer.schedule(deadline: .now(), repeating: .milliseconds(1))
        observer.setEventHandler {
            if RetryTransport.paths.count >= requests && !service.sending {
                finished.signal()
            }
        }
        observer.resume()
        defer { observer.cancel() }
        guard finished.wait(timeout: .now() + 20) == .success else {
            throw NSError(domain: "ObserveWireTest", code: 1, userInfo: [NSLocalizedDescriptionKey: "The exporter did not finish its expected requests"])
        }
    }
}

private final class RetryTransport: URLProtocol {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var recorded: [String] = []
    static var paths: [String] {
        lock.lock()
        defer { lock.unlock() }
        return recorded
    }

    override class func canInit(with request: URLRequest) -> Bool { request.url?.host == "observe.example" }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.lock.lock()
        let status = Self.recorded.isEmpty ? 503 : 200
        Self.recorded.append(request.url!.lastPathComponent)
        Self.lock.unlock()
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: ["Retry-After": "60"])!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}
