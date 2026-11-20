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
}
