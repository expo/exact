import XCTest
@testable import ExactKit
@testable import ExactWakeCarriers

// Which background tasks an abort of a registration cancels: its own, read
// by the description `start` gives them, and none of a registration whose id
// begins with the aborted one and a space.
final class TransferCarrierTests: XCTestCase {
    func testAnAbortSelectsOnlyItsOwnRegistrationsTasks() {
        XCTAssertTrue(TransferCarrier.belongs("album 0", to: "album"))
        XCTAssertTrue(TransferCarrier.belongs("album 1", to: "album"))
        XCTAssertFalse(TransferCarrier.belongs("album 2 0", to: "album"))
        XCTAssertTrue(TransferCarrier.belongs("album 2 0", to: "album 2"))
        XCTAssertFalse(TransferCarrier.belongs("album 0", to: "album 2"))
        XCTAssertFalse(TransferCarrier.belongs(nil, to: "album"))
        XCTAssertFalse(TransferCarrier.belongs("album", to: "album"))
    }

    func testMultipartWireKeepsFieldOrderAndStreamsTheFile() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("clip.bin"), body = root.appendingPathComponent("snapshot")
        let binary = Data([0, 255, 13, 10])
        try binary.write(to: file)
        try TransferCarrier.writeForm([["name": "tag", "value": "one\ntwo"], ["name": "q\"\n雪", "from": "app:/data/clip.bin"],
                                       ["name": "tag", "value": ""]], boundary: "fixed-boundary", roots: ["data": root], to: body)
        var expected = Data("--fixed-boundary\r\nContent-Disposition: form-data; name=\"tag\"\r\n\r\none\r\ntwo\r\n".utf8)
        expected += Data("--fixed-boundary\r\nContent-Disposition: form-data; name=\"q%22%0D%0A雪\"; filename=\"clip.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".utf8)
        expected += binary
        expected += Data("\r\n--fixed-boundary\r\nContent-Disposition: form-data; name=\"tag\"\r\n\r\n\r\n--fixed-boundary--\r\n".utf8)
        try Data("changed".utf8).write(to: file)
        XCTAssertEqual(try Data(contentsOf: body), expected)
    }

    // A restart that is refused leaves the finished registration's record and
    // bodies as they were, on disk for the next launch.
    func testARefusedRestartKeepsTheFinishedRecordAndItsBodies() throws {
        let id = "restart-\(UUID().uuidString)"
        let folder = TransferCarrier.directory.appendingPathComponent(id)
        defer { try? FileManager.default.removeItem(at: folder) }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var item = TransferCarrier.Item(index: 0, method: "GET", url: "https://example.com/a", upload: false)
        item.ended = true
        item.status = 200
        let record = TransferCarrier.Record(id: id, package: "test", discretionary: false, requests: [item])
        let kept = try JSONEncoder().encode(record)
        try kept.write(to: folder.appendingPathComponent("record.json"))
        try Data("body".utf8).write(to: folder.appendingPathComponent("0"))
        TransferCarrier.install()

        func start(_ request: [String: Any]) -> String? {
            let box = NSMutableDictionary(dictionary: ["op": "start", "package": "test", "id": id, "requests": [request]])
            NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: box)
            return box["reply"] as? String
        }
        let form: [[String: Any]] = [["name": "empty", "value": ""]]
        for invalid: [String: Any] in [["method": "HEAD", "form": form], ["headers": ["cOnTeNt-TyPe": "text/plain"], "form": form],
                                        ["form": form, "bodyFrom": "also"], ["form": [["name": "a", "value": "", "from": "app:/data/a"]]]] {
            XCTAssertEqual(start(invalid.merging(["url": "https://example.com/a"]) { _, new in new }), "TypeError")
        }
        XCTAssertEqual(start(["url": "https://example.com/a", "form": [["name": "a", "from": "app:/data/missing"]]]), "NotAllowedError")
        XCTAssertEqual(start(["url": "ftp://example.com/a"]), "TypeError")
        XCTAssertEqual(try Data(contentsOf: folder.appendingPathComponent("record.json")), kept)
        XCTAssertEqual(try Data(contentsOf: folder.appendingPathComponent("0")), Data("body".utf8))
        let missing = FileManager.default.temporaryDirectory.appendingPathComponent("\(id)-missing").path
        XCTAssertEqual(start(["url": "https://example.com/b", "bodyFrom": missing]), "NotAllowedError")
        XCTAssertEqual(try Data(contentsOf: folder.appendingPathComponent("record.json")), kept)
        XCTAssertEqual(try Data(contentsOf: folder.appendingPathComponent("0")), Data("body".utf8))
    }

    func testRetireCancelsLiveTasksAndIgnoresStaleCallbacksAfterRestart() throws {
        TransferCarrier.install()
        let pkg = "pkg-\(UUID().uuidString.prefix(6))", id = "sync"
        let key = TransferCarrier.Key(package: pkg, id: id)
        var cancelled: [(Bool, String)] = []
        let saved = TransferCarrier.cancelSessionTasks
        TransferCarrier.cancelSessionTasks = { cancelled.append(($0, $1)) }
        defer {
            TransferCarrier.cancelSessionTasks = saved
            let box = NSMutableDictionary(dictionary: ["op": "retire", "package": pkg, "id": id])
            NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: box)
        }

        let g1 = "gen-1-\(UUID().uuidString)"
        let item1 = TransferCarrier.Item(index: 0, method: "GET", url: "https://example.com/1", upload: false)
        TransferCarrier.records[key] = TransferCarrier.Record(id: id, package: pkg, discretionary: false, requests: [item1], generation: g1)

        // Retiring cancels G1's tasks.
        let retireBox = NSMutableDictionary(dictionary: ["op": "retire", "package": pkg, "id": id])
        NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: retireBox)
        XCTAssertEqual(cancelled.map(\.1), [g1])
        XCTAssertNil(TransferCarrier.records[key])

        // Restart the same id with generation G2.
        let g2 = "gen-2-\(UUID().uuidString)"
        let item2 = TransferCarrier.Item(index: 0, method: "GET", url: "https://example.com/2", upload: false)
        TransferCarrier.records[key] = TransferCarrier.Record(id: id, package: pkg, discretionary: false, requests: [item2], generation: g2)

        let ephemeral = URLSession(configuration: .ephemeral)
        defer { ephemeral.invalidateAndCancel() }
        let staleTask = ephemeral.dataTask(with: URL(string: "https://example.com/1")!)
        staleTask.taskDescription = "\(g1) 0"
        TransferCarrier.received(staleTask, Data("stale-body".utf8))
        TransferCarrier.completed(staleTask, NSError(domain: NSURLErrorDomain, code: NSURLErrorCancelled))

        let live = try XCTUnwrap(TransferCarrier.records[key])
        XCTAssertEqual(live.generation, g2)
        XCTAssertFalse(live.ended, "stale completion must not end restarted registration")
        XCTAssertEqual(live.failureReason, "")
        XCTAssertNil(live.requests[0].received)
        XCTAssertFalse(FileManager.default.fileExists(atPath: TransferCarrier.folder(key).appendingPathComponent("0").path))

        let liveTask = ephemeral.dataTask(with: URL(string: "https://example.com/2")!)
        liveTask.taskDescription = "\(g2) 0"
        TransferCarrier.received(liveTask, Data("fresh".utf8))
        XCTAssertEqual(TransferCarrier.records[key]?.requests[0].received, 5)
    }

    func testTwoPackagesShareTransferIdWithoutClobberingRecordsOrFolders() throws {
        TransferCarrier.install()
        let pkgA = "pkgA-\(UUID().uuidString.prefix(6))", pkgB = "pkgB-\(UUID().uuidString.prefix(6))"
        let id = "shared-\(UUID().uuidString.prefix(6))"
        let keyA = TransferCarrier.Key(package: pkgA, id: id), keyB = TransferCarrier.Key(package: pkgB, id: id)
        let saved = TransferCarrier.cancelSessionTasks
        TransferCarrier.cancelSessionTasks = { _, _ in }
        defer {
            TransferCarrier.cancelSessionTasks = saved
            for p in [pkgA, pkgB] {
                let box = NSMutableDictionary(dictionary: ["op": "retire", "package": p, "id": id])
                NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: box)
            }
        }

        var itemA = TransferCarrier.Item(index: 0, method: "GET", url: "https://example.com/a", upload: false)
        itemA.ended = true; itemA.status = 200
        let recA = TransferCarrier.Record(id: id, package: pkgA, discretionary: false, requests: [itemA], result: "success", generation: "gen-a")
        let dirA = TransferCarrier.folder(keyA)
        try FileManager.default.createDirectory(at: dirA, withIntermediateDirectories: true)
        try JSONEncoder().encode(recA).write(to: dirA.appendingPathComponent("record.json"))
        try Data("body-a".utf8).write(to: dirA.appendingPathComponent("0"))
        TransferCarrier.records[keyA] = recA

        // Package B creates and retires its own registration under the same id.
        var itemB = TransferCarrier.Item(index: 0, method: "GET", url: "https://example.com/b", upload: false)
        itemB.ended = true; itemB.status = 200
        let recB = TransferCarrier.Record(id: id, package: pkgB, discretionary: false, requests: [itemB], result: "success", generation: "gen-b")
        let dirB = TransferCarrier.folder(keyB)
        try FileManager.default.createDirectory(at: dirB, withIntermediateDirectories: true)
        try JSONEncoder().encode(recB).write(to: dirB.appendingPathComponent("record.json"))
        try Data("body-b".utf8).write(to: dirB.appendingPathComponent("0"))
        TransferCarrier.records[keyB] = recB

        XCTAssertNotEqual(dirA, dirB)
        let recBoxA = NSMutableDictionary(dictionary: ["op": "record", "package": pkgA, "id": id])
        NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: recBoxA)
        XCTAssertEqual((recBoxA["reply"] as? [String: Any])?["result"] as? String, "success")

        let retireB = NSMutableDictionary(dictionary: ["op": "retire", "package": pkgB, "id": id])
        NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: retireB)
        XCTAssertNil(TransferCarrier.records[keyB])
        XCTAssertEqual(TransferCarrier.records[keyA]?.generation, "gen-a")
        XCTAssertEqual(try Data(contentsOf: dirA.appendingPathComponent("0")), Data("body-a".utf8))
    }

    func testDownloadTotalOverflowCancelsAllTasksInRegistration() throws {
        TransferCarrier.install()
        let pkg = "ceil-\(UUID().uuidString.prefix(6))", id = "multi"
        let key = TransferCarrier.Key(package: pkg, id: id)
        var cancelled: [(Bool, String)] = []
        let saved = TransferCarrier.cancelSessionTasks
        TransferCarrier.cancelSessionTasks = { cancelled.append(($0, $1)) }
        defer {
            TransferCarrier.cancelSessionTasks = saved
            let box = NSMutableDictionary(dictionary: ["op": "retire", "package": pkg, "id": id])
            NotificationCenter.default.post(name: Notification.Name("exact.transfers"), object: box)
        }

        let gen = "gen-ceil-\(UUID().uuidString)"
        let items = [
            TransferCarrier.Item(index: 0, method: "GET", url: "https://example.com/0", upload: false),
            TransferCarrier.Item(index: 1, method: "GET", url: "https://example.com/1", upload: false),
        ]
        TransferCarrier.records[key] = TransferCarrier.Record(id: id, package: pkg, discretionary: true, requests: items, downloadTotal: 100, generation: gen)

        let ephemeral = URLSession(configuration: .ephemeral)
        defer { ephemeral.invalidateAndCancel() }
        let t0 = ephemeral.dataTask(with: URL(string: "https://example.com/0")!)
        let t1 = ephemeral.dataTask(with: URL(string: "https://example.com/1")!)
        t0.taskDescription = "\(gen) 0"
        t1.taskDescription = "\(gen) 1"

        TransferCarrier.progressed(t0) { $0.received = 60 }
        XCTAssertTrue(cancelled.isEmpty)
        TransferCarrier.progressed(t1) { $0.received = 50 }
        XCTAssertEqual(TransferCarrier.records[key]?.failureReason, "download-total-exceeded")
        XCTAssertEqual(cancelled.count, 1, "exceeding downloadTotal cancels the whole registration's tasks")
        XCTAssertEqual(cancelled.first?.0, true)
        XCTAssertEqual(cancelled.first?.1, gen)
    }
}
