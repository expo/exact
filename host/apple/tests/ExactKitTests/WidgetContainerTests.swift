#if EXACT_TEST_WIDGET_RELOAD
import Foundation
@_cdecl("exact_widget_reload")
public func testWidgetReload(_ bytes: UnsafePointer<UInt8>?, _ length: Int) { precondition(Thread.isMainThread) }
#else
import XCTest
import ExactCore
import ExactSnapshot
import ExactWidgetPublication

@MainActor
final class WidgetContainerTests: XCTestCase {
    func testUpdateLocksStayInTheAppsPrivateContainer() throws {
        let support = try XCTUnwrap(FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first)
        XCTAssertEqual(WidgetContainer.updateBase, support)
    }

    func testPublicationSkipsUnchangedAssetsAndKeepsPinnedGenerations() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let names = ["plans/card.plan"]
        var reads = 0
        func assets(_ generation: String) -> AssetResolver {
            AssetResolver(root: root, names: names, generation: generation) { _ in
                reads += 1
                return Data(generation.utf8)
            }
        }
        func publish(_ assets: AssetResolver) throws -> Bool {
            try WidgetContainer.publish(assets, root: root, kinds: ["card"], compatibility: "cohort")
        }
        XCTAssertThrowsError(try WidgetContainer.pinCurrent(assets("first"), root: root, compatibility: "cohort"))
        XCTAssertEqual(reads, 0, "a send cannot publish assets")
        XCTAssertTrue(try publish(assets("first")))
        XCTAssertNoThrow(try WidgetContainer.pinCurrent(assets("first"), root: root, compatibility: "cohort"))
        XCTAssertThrowsError(try WidgetContainer.pinCurrent(assets("second"), root: root, compatibility: "cohort"))
        XCTAssertEqual(reads, 1, "a generation behind the app refuses without copying")
        let base = root.appendingPathComponent("widget-generations")
        let first = try String(contentsOf: base.appendingPathComponent("current"), encoding: .utf8)
        try WidgetContainer.signal(root: root) {}
        XCTAssertFalse(try publish(assets("first")))
        XCTAssertEqual(reads, 1, "a new resolver for the same generation reads no assets")
        XCTAssertTrue(try publish(assets("second")))
        XCTAssertEqual(reads, 2)
        XCTAssertEqual(try String(contentsOf: base.appendingPathComponent(first + "/plans/card.plan"), encoding: .utf8), "first")
        XCTAssertNotEqual(try String(contentsOf: base.appendingPathComponent("current"), encoding: .utf8), first)
        XCTAssertEqual(try WidgetContainer.pin(root: root, generation: first, compatibility: "cohort").lastPathComponent, first)
        XCTAssertNotEqual(try WidgetContainer.pin(root: root, compatibility: "cohort").lastPathComponent, first)
        XCTAssertThrowsError(try WidgetContainer.pin(root: root, generation: first, compatibility: "another-cohort"))
        XCTAssertThrowsError(try WidgetContainer.pin(root: root, generation: "../current", compatibility: "cohort"))
        // A generation names the binary that published it: the extension of another build (a binary update the app has
        // not yet launched after) falls back to its embedded plan rather than pin the old app's.
        XCTAssertTrue(try WidgetContainer.publish(assets("third"), root: root, kinds: ["card"], compatibility: "cohort", embedded: "build-a"))
        XCTAssertNoThrow(try WidgetContainer.pin(root: root, compatibility: "cohort", embedded: "build-a"))
        XCTAssertThrowsError(try WidgetContainer.pin(root: root, compatibility: "cohort", embedded: "build-b")) { error in
            XCTAssertTrue(String(describing: error).contains("another binary"), "\(error)")
        }
        XCTAssertNoThrow(try WidgetContainer.pin(root: root, compatibility: "cohort"), "a reader with no embedded plan of its own takes what is current")
        XCTAssertThrowsError(try WidgetContainer.pin(root: root, generation: first, compatibility: "cohort", embedded: "build-a"), "a generation published before binaries were named is no binary's")
    }
    /// LLP 1126 D9, amended 2026-11-17: a push-to-start names the generation the app runs, as a start from the app
    /// does, and that generation is published, so the extension renders it and the app's update and end find it.
    func testPushStartsNameTheRunningGenerationAndPublishIt() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let registry = #"[{"kind":"train","shape":"s1","push":true},{"kind":"quiet","shape":"s2","push":false}]"#
        let delivered = AssetResolver(root: root, names: ["plans/card.plan", "plans/activities.json"], generation: "delivered") { name in
            Data((name == "plans/activities.json" ? registry : "plan").utf8)
        }
        let entries = try WidgetContainer.pushStarts(delivered, root: root, kinds: ["card"], compatibility: "cohort", embedded: "build-a")
        let running = WidgetContainer.generationDigest(compatibility: "cohort", publicationID: "delivered")
        XCTAssertEqual(entries, [["kind": "train", "generation": running, "shape": "s1"]])
        XCTAssertNotEqual(running, WidgetContainer.generationDigest(compatibility: "cohort", publicationID: "embedded:build-a"))
        let folder = try WidgetContainer.pin(root: root, generation: running, compatibility: "cohort")
        XCTAssertTrue(FileManager.default.fileExists(atPath: folder.appendingPathComponent("plans/activities.json").path))
    }
    func testTypeScriptPublicationRefusesEitherMissingHalf() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        for half in ["app.hbc", "app.module.json"] {
            let assets = AssetResolver(root: root, names: ["plans/card.plan", "typescript/" + half], generation: half) { _ in Data("bytes".utf8) }
            XCTAssertThrowsError(try WidgetContainer.publish(assets, root: root, kinds: ["card"], compatibility: "cohort", typescript: true))
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("widget-generations/current").path))
    }

    /// LLP 1126.001 WP13: the build writes `ExactWidgetKinds` for an app with an entry of either kind, empty with
    /// activities alone; a bundle without it publishes nothing.
    func testPublicationIsGatedOnAnEntryOfEitherKind() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? fm.removeItem(at: root) }
        func bundle(_ info: [String: Any]) throws -> Bundle {
            let url = root.appendingPathComponent(UUID().uuidString + ".bundle", isDirectory: true)
            let data = try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
            // Foundation reads a flat bundle's Info.plist on iOS and Contents/Info.plist on macOS.
            for path in ["Info.plist", "Contents/Info.plist"] {
                let file = url.appendingPathComponent(path)
                try fm.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
                try data.write(to: file)
            }
            return try XCTUnwrap(Bundle(url: url))
        }
        let assets = AssetResolver(root: root, names: [], generation: "first") { _ in Data() }
        let group = root.appendingPathComponent("group", isDirectory: true)
        let current = group.appendingPathComponent("widget-generations/current").path
        XCTAssertNil(WidgetContainer.entryKinds(of: try bundle([:])))
        XCTAssertFalse(try WidgetContainer.publish(assets, bundle: try bundle([:]), root: group), "no entry of either kind: inert")
        XCTAssertFalse(fm.fileExists(atPath: current))
        XCTAssertEqual(WidgetContainer.entryKinds(of: try bundle(["ExactWidgetKinds": [String]()])), [])
        XCTAssertTrue(try WidgetContainer.publish(assets, bundle: try bundle(["ExactWidgetKinds": [String]()]), root: group), "activities alone publish the generation")
        XCTAssertTrue(fm.fileExists(atPath: current))
        XCTAssertEqual(WidgetContainer.entryKinds(of: try bundle(["ExactWidgetKinds": ["card"]])), ["card"])
        XCTAssertThrowsError(try WidgetContainer.publish(assets, bundle: try bundle(["ExactWidgetKinds": ["card"]]), root: root.appendingPathComponent("widgets", isDirectory: true)), "a widget's plan is required of its generation")
    }

    private enum RefreshFailure: Error { case offline }

    func testAccountSwitchRefusesLastGoodTimelineUntilTheNewAccountRefreshes() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        func cache(_ kind: String = "question", _ generation: String = "one") throws -> WidgetTimelineCache {
            try WidgetTimelineCache(root: root, kind: kind, generation: generation)
        }
        func offline() throws -> Data { throw RefreshFailure.offline }
        let a = Data(#"[{"date":1,"props":{"question":"Account A"}}]"#.utf8)
        let b = Data(#"[{"date":2,"props":{"question":"Account B"}}]"#.utf8)
        let before = try cache()
        XCTAssertEqual(try before.resolve { a }, a)
        XCTAssertEqual(try before.resolve(offline), a, "a failure without a store write keeps the last good answer")
        XCTAssertEqual(try cache().resolve(offline), a, "a new process restores the same account's answer")
        XCTAssertThrowsError(try cache("another-kind").resolve(offline))
        XCTAssertThrowsError(try cache("question", "another-generation").resolve(offline))

        // The app publishes this counter before the Darwin ping for its store write.
        try Data("1".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
        let after = try cache()
        XCTAssertNotEqual(before, after, "a new account must not join the old account's in-flight refresh")
        XCTAssertFalse(before.isCurrent, "an in-memory answer from A must not reach a completion after the write")
        XCTAssertThrowsError(try before.resolve(offline))
        XCTAssertThrowsError(try after.resolve(offline), "the provider must take its Open-app path, never return A's entries")
        XCTAssertThrowsError(try cache().resolve(offline), "restarting the extension must not restore A's persisted entries")

        XCTAssertEqual(try after.resolve { b }, b)
        XCTAssertEqual(try after.resolve(offline), b, "the counter does not invalidate a newly written good answer")
        XCTAssertEqual(try cache().resolve(offline), b)
    }

    func testStoreWriteDuringRefreshDiscardsItsAnswer() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let cache = try WidgetTimelineCache(root: root, kind: "question", generation: "one")
        let a = Data("account A".utf8)
        XCTAssertEqual(try cache.resolve { a }, a)
        XCTAssertThrowsError(try cache.resolve {
            try Data("1".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
            return a
        }, "a refresh that straddles a store write cannot save or return its old account's answer")
        let next = try WidgetTimelineCache(root: root, kind: "question", generation: "one")
        XCTAssertThrowsError(try next.resolve { throw RefreshFailure.offline })
    }

    func testStoreWritesDuringRefreshRestartItUnderTheNewStoreWhileTimeRemains() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        func cache() throws -> WidgetTimelineCache { try WidgetTimelineCache(root: root, kind: "question", generation: "one") }
        func write(_ counter: Int) throws {
            try Data("\(counter)".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
        }
        let a = Data("account A".utf8), b = Data("account B".utf8)
        let begun = try cache()
        XCTAssertEqual(try begun.resolve { a }, a)

        var attempts = 0
        let (bytes, revision) = try begun.resolveAcrossWrites(while: { true }) {
            attempts += 1
            if attempts < 3 { try write(attempts); return a }
            return b
        }
        XCTAssertEqual(attempts, 3, "each write during a refresh runs it again")
        XCTAssertEqual(bytes, b, "an answer computed before a write is never returned")
        XCTAssertTrue(revision.isCurrent)
        XCTAssertFalse(begun.isCurrent)
        XCTAssertEqual(try cache().resolve { throw RefreshFailure.offline }, b, "the answer under the new store is kept")

        attempts = 0
        XCTAssertEqual(try cache().resolveAcrossWrites(while: { true }) {
            attempts += 1
            throw RefreshFailure.offline
        }.bytes, b)
        XCTAssertEqual(attempts, 1, "a failure other than a store write is not retried")

        attempts = 0
        XCTAssertThrowsError(try cache().resolveAcrossWrites(while: { attempts < 2 }) {
            attempts += 1
            try write(10 + attempts)
            return a
        }, "writes that outlast the time fall back")
        XCTAssertEqual(attempts, 2)
        XCTAssertThrowsError(try cache().resolve { throw RefreshFailure.offline }, "no answer computed before a write is kept")
    }

    /// The answers a kind keeps in a generation, one file for each configuration.
    private func kept(_ root: URL, _ generation: String, _ kind: String) -> [URL] {
        let directory = root.appendingPathComponent("Library/Caches/widget-timelines/\(generation)/\(kind)")
        return (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
    }

    func testAnUnknownCounterOrUnversionedCacheIsNeverReusedAndALiveRefreshStillAnswers() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let cache = try WidgetTimelineCache(root: root, kind: "question", generation: "one")
        _ = try cache.resolve { Data("account A".utf8) }
        let file = try XCTUnwrap(kept(root, "one", "question").first)
        try Data(#"[{"date":1,"props":{"question":"Account A"}}]"#.utf8).write(to: file)
        XCTAssertThrowsError(try cache.resolve { throw RefreshFailure.offline }, "an old cache has no proof of its store revision")
        // A build that kept one file for a kind, whatever its configuration, wrote it here: it is not read.
        try FileManager.default.removeItem(at: file)
        try Data(#"{"counter":0,"bytes":"YWNjb3VudCBB"}"#.utf8).write(to: root.appendingPathComponent("Library/Caches/widget-timelines/one/question.json"))
        XCTAssertThrowsError(try cache.resolve { throw RefreshFailure.offline })
        _ = try cache.resolve { Data("account A".utf8) }
        try Data("corrupt".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
        XCTAssertFalse(cache.isCurrent)
        XCTAssertThrowsError(try cache.resolve { throw RefreshFailure.offline })
        // An unreadable counter still lets a refresh answer from the live
        // store; it keeps nothing and reuses nothing.
        let unknown = try WidgetTimelineCache(root: root, kind: "question", generation: "one")
        XCTAssertThrowsError(try unknown.resolve { throw RefreshFailure.offline })
        try FileManager.default.removeItem(at: file)
        XCTAssertEqual(try unknown.resolve { Data("account B".utf8) }, Data("account B".utf8))
        XCTAssertEqual(kept(root, "one", "question"), [])
        // A write that makes the counter readable mid-refresh refuses delivery.
        XCTAssertThrowsError(try unknown.resolve {
            try Data("2".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
            return Data("account A".utf8)
        })
    }

    func testAFailedCacheWriteKeepsTheGoodRefreshAndAnOlderEntryIsRemoved() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let caches = root.appendingPathComponent("Library/Caches/widget-timelines")
        try FileManager.default.createDirectory(at: caches, withIntermediateDirectories: true)
        // A file where the generation's directory goes: the cache cannot be written.
        try Data().write(to: caches.appendingPathComponent("one"))
        let blocked = try WidgetTimelineCache(root: root, kind: "question", generation: "one")
        XCTAssertEqual(try blocked.resolve { Data("account A".utf8) }, Data("account A".utf8))
        try FileManager.default.removeItem(at: caches.appendingPathComponent("one"))
        _ = try WidgetTimelineCache(root: root, kind: "question", generation: "two").resolve { Data("account A".utf8) }
        XCTAssertEqual(kept(root, "two", "question").count, 1)
        try Data("1".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
        let switched = try WidgetTimelineCache(root: root, kind: "question", generation: "two")
        XCTAssertThrowsError(try switched.resolve { throw RefreshFailure.offline })
        XCTAssertEqual(kept(root, "two", "question"), [], "account A's answer is not kept past the switch")
    }

    /// Each configuration has its own in-flight share and last good timeline. A kind keeps
    /// `WidgetTimelineCache.kept` of them: a write past that removes the answer written longest ago.
    func testEachConfigurationKeepsItsOwnTimelineAndAKindKeepsABoundedNumber() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? fm.removeItem(at: root) }
        func cache(_ rows: Int?) throws -> WidgetTimelineCache {
            try WidgetTimelineCache(root: root, kind: "trains", generation: "one", configuration: rows.map { Data(#"{"rows":"\#($0)"}"#.utf8) })
        }
        func offline() throws -> Data { throw RefreshFailure.offline }
        func answer(_ rows: Int) -> Data { Data("rows \(rows)".utf8) }
        XCTAssertEqual(try cache(0), try cache(0))
        XCTAssertNotEqual(try cache(0), try cache(1), "another configuration must not join this one's in-flight refresh")
        XCTAssertNotEqual(try cache(0), try cache(nil))
        var dated = Set<URL>()
        for rows in 0...WidgetTimelineCache.kept {
            XCTAssertThrowsError(try cache(rows).resolve(offline), "no other configuration's answer is this one's last good")
            XCTAssertEqual(try cache(rows).resolve { answer(rows) }, answer(rows))
            // Date each answer as it is written, so their order does not rest on the file system's clock.
            let files = Set(kept(root, "one", "trains"))
            for file in files.subtracting(dated) {
                try fm.setAttributes([.modificationDate: Date(timeIntervalSince1970: Double(rows))], ofItemAtPath: file.path)
            }
            dated = files
        }
        XCTAssertEqual(dated.count, WidgetTimelineCache.kept)
        XCTAssertThrowsError(try cache(0).resolve(offline), "the answer written longest ago is removed")
        for rows in 1...WidgetTimelineCache.kept { XCTAssertEqual(try cache(rows).resolve(offline), answer(rows)) }
        XCTAssertThrowsError(try cache(nil).resolve(offline), "an unconfigured placement has no configured one's answer")
    }


    func testMissingArtifactRemainsUnsignalledAndRetries() throws { try retry(missingSymbol: false) }
    func testMissingSymbolRemainsUnsignalledAndRetries() throws { try retry(missingSymbol: true) }
    private func retry(missingSymbol: Bool) throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? fm.removeItem(at: root) }
        let assets = AssetResolver(root: root, names: ["plans/card.plan"], generation: "pending") { _ in Data("plan".utf8) }
        func publish() throws -> Bool { try WidgetContainer.publish(assets, root: root, kinds: ["card"], compatibility: "cohort") }
        let good = root.appendingPathComponent("reload.dylib")
        let bad = missingSymbol ? URL(fileURLWithPath: "/usr/lib/libSystem.B.dylib") : good
        let base = root.appendingPathComponent("widget-generations")
        XCTAssertTrue(try publish())
        for _ in 0..<2 {
            XCTAssertThrowsError(try WidgetContainer.signal(root: root) { try WidgetContainer.reloadTimelines(artifact: bad) }) { error in
                let message = String(describing: error)
                XCTAssertTrue(message.contains(missingSymbol ? "exact_widget_reload" : "dlopen"), message)
                XCTAssertTrue(message.contains(bad.path), message)
            }
            XCTAssertFalse(fm.fileExists(atPath: base.appendingPathComponent("signalled").path))
            XCTAssertTrue(try publish(), "an unchanged generation must retry a failed signal")
        }
        let fixture = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_WIDGET_RELOAD_TEST"])
        try fm.copyItem(at: URL(fileURLWithPath: fixture), to: good)
        try WidgetContainer.signal(root: root) { try WidgetContainer.reloadTimelines(artifact: good) }
        XCTAssertEqual(try Data(contentsOf: base.appendingPathComponent("signalled")), try Data(contentsOf: base.appendingPathComponent("current")))
        XCTAssertFalse(try publish())
    }
}
#endif
