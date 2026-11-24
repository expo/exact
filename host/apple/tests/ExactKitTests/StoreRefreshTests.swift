import XCTest
import ExactCore
import ExactSnapshot
import ExactWidgetPublication
@testable import ExactKit
@testable import ExactCollections
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

@MainActor
final class StoreRefreshTests: XCTestCase {
    #if os(iOS)
    func testPersistedSlotsReadAndWriteTheAppsDefaultsAtBoot() throws {
        let receipt = try XCTUnwrap(try JSONSerialization.jsonObject(with: Runtime.bakedCompat()) as? [String: Any])
        let appID = try XCTUnwrap((receipt["inputs"] as? [String: Any])?["app"] as? String)
        let defaults = try XCTUnwrap(UserDefaults(suiteName: appID))
        let keys = ["exact.state.favourite", "exact.state.marker", "exact.state.delay"]
        let previous = keys.map { ($0, defaults.object(forKey: $0)) }
        let variables = ["EXACT_AGENT", "EXACT_STORE", "EXACT_APP_GROUP_ROOT", "EXACT_APP_GROUP_APP"]
        let environment = variables.map { ($0, getenv($0).map { String(cString: $0) }) }
        for key in variables { unsetenv(key) }
        setenv("EXACT_STORE", "real", 1)
        defer {
            for (key, value) in previous { defaults.set(value, forKey: key) }
            defaults.synchronize()
            for (key, value) in environment { if let value { setenv(key, value, 1) } else { unsetenv(key) } }
        }
        defaults.set("true", forKey: keys[0])
        defaults.set("\"not a boolean\"", forKey: keys[1])
        defaults.set("10000", forKey: keys[2])
        XCTAssertTrue(defaults.synchronize())
        let plan = try fixturePlan("STORE_REFRESH")
        func boot() throws -> ExactSession {
            let session = ExactApp.shared.makeSession(label: "defaults-boot")
            let batch = session.boot(plan: plan, size: CGSize(width: 300, height: 400))
            XCTAssertNil(batch.error)
            return session
        }
        func slots(_ session: ExactSession) throws -> [String: Any] {
            let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent("{\"op\":\"state\"}").utf8)) as? [String: Any])
            return try XCTUnwrap(state["slots"] as? [String: Any])
        }
        let first = try boot()
        defer { first.destroy() }
        XCTAssertEqual(try slots(first)["favourite"] as? Bool, true, "a preference is present before data activation")
        XCTAssertEqual(try slots(first)["marker"] as? Bool, false, "an incompatible saved value uses the initializer")
        let button = try XCTUnwrap(first.presenter.views.values.first { $0.props["testId"] == "favourite" })
        first.apply(first.runtime.press(button.id, now: 0))
        XCTAssertEqual(defaults.string(forKey: keys[0]), "false", "the commit writes the preference immediately")
        first.destroy()
        let second = try boot()
        defer { second.destroy() }
        XCTAssertEqual(try slots(second)["favourite"] as? Bool, false, "a fresh runner reads the committed value")
    }
    #endif

    func testRestoredWindowBootsWithItsOwnStateAndPublishesSubsequentChanges() throws {
        let plan = try fixturePlan("STORE_REFRESH")
        let first = ExactApp.shared.makeSession(label: "restored-window")
        let second = ExactApp.shared.makeSession(label: "fresh-window")
        defer { first.destroy(); second.destroy() }
        first.restoreWindowState(["local": "17", "favourite": "true", "retired": "42"])
        let size = CGSize(width: 300, height: 400)
        XCTAssertNil(first.boot(plan: plan, size: size).error)
        XCTAssertNil(second.boot(plan: plan, size: size).error)
        XCTAssertEqual(first.windowState, ["local": "17"])
        XCTAssertEqual(second.windowState, ["local": "0"])
        let increment = try XCTUnwrap(first.presenter.views.values.first { $0.props["testId"] == "local" })
        first.apply(first.runtime.press(increment.id, now: 0))
        XCTAssertEqual(first.windowState, ["local": "18"])
        XCTAssertEqual(second.windowState, ["local": "0"])
    }

    func testABurstOfStoreWritesReloadsTheWidgetsOnceItIsQuiet() {
        let refresh = SharedStoreRefresh()
        refresh.quiet = 0.2
        var reloads = 0
        refresh.reloadWhenQuiet { reloads += 1 }
        refresh.firstPixel()
        refresh.reloadWhenQuiet { reloads += 1 }
        refresh.reloadWhenQuiet { reloads += 1 }
        XCTAssertEqual(reloads, 0, "no reload starts while the burst is still writing")
        let settled = expectation(description: "the burst settles")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { settled.fulfill() }
        wait(for: [settled], timeout: 5)
        XCTAssertEqual(reloads, 1, "one reload after the last write of the burst")
        refresh.reloadWhenQuiet { reloads += 1 }
        let next = expectation(description: "a later write")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { next.fulfill() }
        wait(for: [next], timeout: 5)
        XCTAssertEqual(reloads, 2, "a write after the quiet period reloads again")
    }

    func testProductionObserverRefreshesEverySessionAndRecoversAtActivation() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let foreground = NSApplication.didBecomeActiveNotification
        #else
        let foreground = UIApplication.didBecomeActiveNotification
        #endif
        let fixture = try isolatedStore()
        let (app, appID, root) = (fixture.app, fixture.appID, fixture.root)
        defer { fixture.restore() }
        let counter = root.appendingPathComponent("widget-store-generation")
        try Data("1000000".utf8).write(to: counter, options: .atomic)
        var reloads = 0
        var expected: XCTestExpectation?
        app.observeSharedStore(root: root, appID: appID, kinds: ["wp12-fixture"]) { kinds in
            XCTAssertEqual(kinds, ["wp12-fixture"])
            reloads += 1
            expected?.fulfill()
        }
        WidgetContainer.configure()
        XCTAssertEqual(String(cString: getenv("EXACT_APP_GROUP_ROOT")), root.path, "configure preserves the explicitly endowed test container")
        func foregroundAndDrain() {
            let delivered = expectation(description: "foreground notification delivered")
            NotificationCenter.default.post(name: foreground, object: nil)
            DispatchQueue.main.async { delivered.fulfill() }
            wait(for: [delivered], timeout: 5)
        }
        foregroundAndDrain()
        XCTAssertEqual(reloads, 0, "the installed counter is the baseline, including before first pixel")
        let plan = try fixturePlan("STORE_REFRESH")
        let readers = (0..<2).map { app.makeSession(label: "store-reader-\($0)") }
        defer { readers.forEach { $0.destroy() } }
        let booting = ExactEnv.wall()
        for session in readers {
            XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 300, height: 400)).error)
            session.apply(session.runtime.dataReady())
        }
        let booted = ExactEnv.wall()
        // Due 10 s after its window booted, however long the process ran
        // before: a boot starts on the live clock, not at its zero. Read
        // here, before the waits below, which under load can outlast it. A
        // task's timer is Foundation's: its row's due time.
        let due = try XCTUnwrap(readers[0].taskTimers.nextDue, "the short task's timer is armed")
        XCTAssertTrue((booting + 10000...booted + 10000).contains(due), "due \(due), booted between \(booting) and \(booted)")
        app.sharedStore.firstPixel()
        func text(_ session: ExactSession, _ value: String) -> Bool {
            #if os(macOS)
            session.presenter.controls.controls.values.contains { ($0 as? NSButton)?.title == value }
            #else
            session.presenter.controls.controls.values.contains { ($0 as? UIButton)?.titleLabel?.text == value }
            #endif
        }
        func tap(_ session: ExactSession, _ testID: String) throws {
            let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testID })
            session.apply(session.runtime.press(node.id, now: 0))
        }
        /// The other process's write, as an interactive widget's intent makes
        /// one (LLP 1126.001 WP11): a headless session booted on the store as
        /// it stands presses the node. A write of one of the app's own
        /// sessions reaches its other sessions at once (`relayStore`); a
        /// peer's reaches them only through the observer.
        func peerTap(_ testID: String) throws {
            let peer = try HeadlessSession(plan: plan, size: CGSize(width: 300, height: 400), facts: SessionFacts(displayScale: 1, preferences: [], clock: 0))
            defer { peer.close() }
            try peer.refresh()
            let node = try XCTUnwrap(try peer.snapshot().nodes.values.first { $0.props["testId"] == testID })
            try peer.press(node.id)
        }
        try tap(readers[0], "local")
        try peerTap("favourite")
        XCTAssertTrue(readers.allSatisfy { text($0, "Not a favourite") })
        expected = expectation(description: "the production Darwin observer refreshes both readers before reload")
        try Data("1000001".utf8).write(to: counter, options: .atomic)
        let ping = appID + ".widget-store"
        CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName(ping as CFString), nil, nil, true)
        wait(for: [try XCTUnwrap(expected)], timeout: 5)
        expected = nil
        XCTAssertTrue(readers.allSatisfy { text($0, "Favourite") })
        XCTAssertEqual(reloads, 1)
        try peerTap("favourite")
        try Data("1000002".utf8).write(to: counter, options: .atomic)
        XCTAssertTrue(readers.allSatisfy { text($0, "Favourite") })
        expected = expectation(description: "didBecomeActive recovers a ping missed by a prewarmed app")
        NotificationCenter.default.post(name: foreground, object: nil)
        wait(for: [try XCTUnwrap(expected)], timeout: 5)
        expected = nil
        XCTAssertEqual(reloads, 2)
        XCTAssertTrue(readers.allSatisfy { text($0, "Not a favourite") })
        XCTAssertTrue(text(readers[0], "1"))
        XCTAssertTrue(text(readers[1], "0"))
        foregroundAndDrain()
        XCTAssertEqual(reloads, 2)

        setenv("EXACT_WIDGET_WRITER", appID, 1)
        expected = expectation(description: "an app-side commit refreshes sibling windows before reload")
        try tap(readers[0], "favourite")
        wait(for: [try XCTUnwrap(expected)], timeout: 5)
        expected = nil
        XCTAssertTrue(text(readers[0], "Favourite"))
        XCTAssertTrue(text(readers[1], "Favourite"), "the sibling adopts the app-side write")
        XCTAssertEqual(reloads, 3)
        foregroundAndDrain()
        XCTAssertEqual(reloads, 3)

        // Main stays in this turn until both publications exist, so the
        // observer first sees the app's counter over the peer's new value.
        setenv("EXACT_WIDGET_WRITER", "", 1)
        try peerTap("favourite")
        let peerCounter = UInt64(try String(contentsOf: counter, encoding: .utf8))! + 1
        try Data(String(peerCounter).utf8).write(to: counter, options: .atomic)
        CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName(ping as CFString), nil, nil, true)
        setenv("EXACT_WIDGET_WRITER", appID, 1)
        try peerTap("marker")
        let deadline = Date().addingTimeInterval(5)
        while (UInt64((try? String(contentsOf: counter, encoding: .utf8)) ?? "") ?? 0) <= peerCounter && Date() < deadline { usleep(50000) }
        XCTAssertGreaterThan(UInt64(try String(contentsOf: counter, encoding: .utf8))!, peerCounter)
        expected = expectation(description: "an app publication cannot hide a peer value")
        wait(for: [try XCTUnwrap(expected)], timeout: 5)
        expected = nil
        XCTAssertTrue(readers.allSatisfy { text($0, "Not a favourite") && text($0, "Marked") })

        setenv("EXACT_WIDGET_WRITER", "", 1)
        try peerTap("wait")
        try Data(String(peerCounter + 2).utf8).write(to: counter, options: .atomic)
        var timerBatch: Batch?
        let refreshing = ExactEnv.wall()
        // The long task's timer is armed by the refresh: 20 s after it, not the short one's.
        let rearmed = { (at: Double?) in at.map { $0 != due && $0 >= refreshing + 20000 } ?? false }
        readers[0].runtime.observeBatch = { _, batch in
            if timerBatch == nil, rearmed(batch.nativeTimers?.map(\.due).min()) { timerBatch = batch }
        }
        defer { readers[0].runtime.observeBatch = nil }
        expected = expectation(description: "a timer-only store commit reaches apply")
        NotificationCenter.default.post(name: foreground, object: nil)
        wait(for: [try XCTUnwrap(expected)], timeout: 5)
        expected = nil
        let refreshed = ExactEnv.wall()
        let landed = try XCTUnwrap(timerBatch)
        XCTAssertTrue(landed.ops.isEmpty, "this commit changes only the timer")
        let landedDue = try XCTUnwrap(landed.nativeTimers?.map(\.due).min())
        XCTAssertTrue((refreshing + 20000...refreshed + 20000).contains(landedDue))
        XCTAssertEqual(readers[0].taskTimers.nextDue, landedDue)
        XCTAssertTrue(rearmed(readers[1].taskTimers.nextDue), "the sibling's long task is armed too")

    }

    private let unchangedStoreReply = Batch.decode(Data(#"{"ops":[],"unchanged":true,"stored":true,"storeReadPending":false,"storeActivationPending":false}"#.utf8))

    private func enqueueReply(_ reply: Batch, to session: ExactSession) {
        session.repliesInFlight += 1
        session.replySubmissions.append(0)
        session.publish(.reply, RuntimeCompletion(lease: session.runtime.completionLease, batch: reply))
    }

    private func publishReply(_ reply: Batch, to session: ExactSession) {
        enqueueReply(reply, to: session)
        session.landPublished()
    }

    func testAnUnchangedStoreReplyRelaysWithFramesRunning() throws {
        try checkUnchangedStoreRelay(staged: false)
    }

    func testAnUnchangedStoreReplyRelaysBesideAStageWithFramesRunning() throws {
        try checkUnchangedStoreRelay(staged: true)
    }

    private func checkUnchangedStoreRelay(staged: Bool) throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let fixture = try isolatedStore()
        defer { fixture.restore() }
        let writer = fixture.app.makeSession(label: "unchanged-store-writer")
        let reader = fixture.app.makeSession(label: "unchanged-store-reader")
        defer { writer.destroy(); reader.destroy() }
        let size = CGSize(width: 300, height: 400)
        let plan = try fixturePlan("STORE_REFRESH")
        reader.clock = 0
        XCTAssertNil(reader.boot(plan: plan, size: size).error)
        reader.apply(reader.runtime.dataReady())
        let button = try XCTUnwrap(reader.presenter.views.values.first { $0.props["testId"] == "favourite" })
        func title() -> String? {
            #if os(macOS)
            (reader.presenter.controls.controls[button.id] as? NSButton)?.title
            #else
            (reader.presenter.controls.controls[button.id] as? UIButton)?.titleLabel?.text
            #endif
        }
        XCTAssertEqual(title(), "Not a favourite")
        let peer = try HeadlessSession(plan: plan, size: size, facts: SessionFacts(displayScale: 1, preferences: [], clock: 0))
        defer { peer.close() }
        try peer.refresh()
        let node = try XCTUnwrap(try peer.snapshot().nodes.values.first { $0.props["testId"] == "favourite" })
        try peer.press(node.id)

        #if os(macOS)
        let window = staged ? NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false) : nil
        window?.isReleasedWhenClosed = false
        window?.contentView = writer.presenter.viewport
        defer { window?.close() }
        #endif
        var frames = wireBatch([])
        frames.frames = true
        writer.apply(frames)
        if staged {
            let parent: UInt32 = 9000
            let children = (1...StagePlan.minimumOps).map { parent + UInt32($0) }
            var ops: [[String: Any]] = children.map { ["op": "create", "id": $0, "kind": "view"] }
            ops.append(["op": "children", "id": parent, "ids": children])
            var batch = wireBatch(ops)
            batch.frames = true
            #if os(macOS)
            writer.apply(wireBatch([["op": "create", "id": parent, "kind": "view"], ["op": "roots", "ids": [parent]]]))
            writer.apply(batch)
            #else
            // UIKit does not open stages. Put one in the shared session queue.
            writer.stage = try XCTUnwrap(StagePlan(batch, joins: { $0 == parent }, holds: { _, _ in false }))
            #endif
            XCTAssertNotNil(writer.stage)
        }
        XCTAssertTrue(writer.frames.tasks)
        let applied = writer.appliedBatches
        let originalReplication = fixture.app.replicateStore
        var replications = 0
        fixture.app.replicateStore = { replications += 1 }
        defer { fixture.app.replicateStore = originalReplication }
        publishReply(unchangedStoreReply, to: writer)
        XCTAssertEqual(replications, 1)
        XCTAssertEqual(writer.appliedBatches, applied + 1)
        XCTAssertTrue(writer.frames.tasks)
        XCTAssertEqual(writer.stage != nil, staged)
        waitFor("the sibling window reads the committed preference", cap: 5) { title() == "Favourite" }
        XCTAssertEqual(replications, 1)
    }

    func testAnUnchangedStoreReplyCanDrainWithAnotherReplyPublished() {
        let app = ExactApp.shared
        let session = app.makeSession(label: "unchanged-store-drain")
        defer { session.destroy() }
        var frames = wireBatch([])
        frames.frames = true
        session.apply(frames)
        let applied = session.appliedBatches
        var next = frames
        let due = session.now() + 60_000
        next.timerDueMs = due
        let originalReplication = app.replicateStore
        var drained = false
        app.replicateStore = {
            guard session.appliedBatches == applied + 1 else {
                return XCTFail("the intake callback would spin on the second publication")
            }
            XCTAssertTrue(session.hasPublished(), "the callback runs before the second reply lands")
            session.drainFill()
            drained = true
        }
        defer { app.replicateStore = originalReplication }
        enqueueReply(unchangedStoreReply, to: session)
        enqueueReply(next, to: session)
        session.landPublished()
        XCTAssertTrue(drained)
        XCTAssertFalse(session.hasPublished())
        XCTAssertEqual(session.clockDue, due)
    }

    func testAnUnchangedStoreReplyPreservesAnArmedScrollEndWake() throws {
        let app = ExactApp.shared
        let session = app.makeSession(label: "unchanged-store-timer")
        defer { session.destroy() }
        let plan = try fixturePlan("PULL_REFRESH")
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 300, height: 200)).error)
        settleViewlessSession(session)
        let button = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "wait-for-rest" })
        session.apply(session.runtime.press(button.id, now: session.now()))
        let waits = session.timerAdmissions.waits
        XCTAssertTrue(waits.contains { $0 >= 0 }, "the scroll-end wake is armed")

        let originalReplication = app.replicateStore
        var replications = 0
        app.replicateStore = { replications += 1 }
        defer { app.replicateStore = originalReplication }
        publishReply(unchangedStoreReply, to: session)
        XCTAssertEqual(replications, 0, "main skips a receipt beside a scroll-end wait alone")
        XCTAssertEqual(session.timerAdmissions.waits, waits)
        waitFor("the unchanged receipt leaves the wake armed", cap: 5) {
            session.presenter.views.values.contains { $0.props["testId"] == "rests" && $0.props["text"] == "1" }
        }
    }

    #if os(macOS)
    func testAnEmptyTextInputRelaysItsStoreWriteAfterLanding() throws {
        _ = NSApplication.shared
        let fixture = try isolatedStore()
        defer { fixture.restore() }
        let plan = try TableFixture.plan("""
        component StoreEdit
          state saved = "" persist
          action edited(value: string)
            saved = value
          view
            textarea testId="editor" value="" input=edited
        """)
        let writer = fixture.app.makeSession(label: "store-edit-writer")
        let rect = NSRect(x: 0, y: 0, width: 300, height: 200)
        let window = NSWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { writer.destroy(); window.close() }
        XCTAssertNil(writer.boot(plan: plan, size: rect.size).error)
        writer.apply(writer.runtime.dataReady())
        settleViewlessSession(writer)
        writer.presenter.viewport.frame = rect
        window.contentView = writer.presenter.viewport
        let node = try XCTUnwrap(writer.presenter.views.values.first { $0.props["testId"] == "editor" })
        let editor = try XCTUnwrap(node.textArea)
        XCTAssertTrue(window.makeFirstResponder(editor))
        writer.followOffset()

        let originalReplication = fixture.app.replicateStore
        var receipts: [Batch] = []
        fixture.app.replicateStore = { [unowned writer] in
            if let batch = writer.newestApplied { receipts.append(batch) }
        }
        defer { fixture.app.replicateStore = originalReplication }
        let applied = writer.appliedBatches
        editor.insertText("a", replacementRange: editor.selectedRange())
        XCTAssertEqual(editor.string, "a")
        XCTAssertEqual(writer.appliedBatches, applied, "the native edit skips presentation")
        XCTAssertEqual(receipts.count, 1)
        let receipt = try XCTUnwrap(receipts.first)
        XCTAssertTrue(receipt.stored, "replication sees the landed store receipt")
        XCTAssertTrue(receipt.ops.isEmpty)
    }

    func testAnEmptyReplyRefreshesTheOtherWindow() throws {
        _ = NSApplication.shared
        let fixture = try isolatedStore()
        defer { fixture.restore() }
        let writer = fixture.app.makeSession(label: "store-reply-writer")
        let reader = fixture.app.makeSession(label: "store-reply-reader")
        defer { writer.destroy(); reader.destroy() }
        let plan = try fixturePlan("STORE_REFRESH")
        let size = CGSize(width: 300, height: 400)
        reader.clock = 0
        XCTAssertNil(reader.boot(plan: plan, size: size).error)
        reader.apply(reader.runtime.dataReady())
        let button = try XCTUnwrap(reader.presenter.views.values.first { $0.props["testId"] == "favourite" })
        let control = try XCTUnwrap(reader.presenter.controls.controls[button.id] as? NSButton)
        XCTAssertEqual(control.title, "Not a favourite")

        let peer = try HeadlessSession(plan: plan, size: size, facts: SessionFacts(displayScale: 1, preferences: [], clock: 0))
        defer { peer.close() }
        try peer.refresh()
        let node = try XCTUnwrap(try peer.snapshot().nodes.values.first { $0.props["testId"] == "favourite" })
        try peer.press(node.id)

        let applied = writer.appliedBatches
        var write = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
        write.stored = true
        publishReply(write, to: writer)
        XCTAssertEqual(writer.appliedBatches, applied)
        waitFor("the other window displays the saved preference") { control.title == "Favourite" }

        let listener = fixture.app.makeSession(label: "store-topic-listener")
        defer { listener.destroy() }
        XCTAssertNil(listener.boot(plan: try fixturePlan("NOTIFICATION_ACTIONS"), size: size).error)
        for value in ["reply", "tick"] {
            var topic = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
            topic.announced = [#"exact-state:{"extension":"notifications","state":{"source":"\#(value)"}}"#]
            if value == "reply" {
                publishReply(topic, to: writer)
                XCTAssertEqual(writer.appliedBatches, applied)
            } else {
                writer.publishTickForTests(topic)
                writer.landPublished()
            }
            waitFor("the other window handles the topic from a \(value)", cap: 5) {
                let data = Data(listener.agent(#"{"op":"state"}"#).utf8)
                let state = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
                let extensions = state?["extensions"] as? [String: [String: Any]]
                let implementation = extensions?["notifications"]?["implementation"] as? [String: String]
                return implementation?["source"] == value
            }
        }
    }

    func testWindowStateCallbackCanDestroyTheSessionWithFramesRunning() {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "store-callback")
        defer { session.destroy() }
        var batch = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
        batch.frames = true
        session.apply(batch)
        XCTAssertNotNil(session.frames.link)
        let applied = session.appliedBatches
        batch.windowState = ["tab": "A"]
        session.windowStateDidChange = { [unowned session] in
            XCTAssertEqual(session.newestApplied?.windowState, ["tab": "A"])
            session.destroy()
        }
        publishReply(batch, to: session)
        XCTAssertEqual(session.windowState, ["tab": "A"])
        XCTAssertEqual(session.state, .destroyed)
        XCTAssertNil(session.frames.link, "the callback's teardown is final")
        XCTAssertEqual(session.appliedBatches, applied)
    }

    func testAClockOnlyReplyArmsItsDeadline() {
        let session = ExactApp.shared.makeSession(label: "reply-deadline")
        defer { session.destroy() }
        let due = session.now() + 60_000
        let applied = session.appliedBatches
        var reply = Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due)
        publishReply(reply, to: session)
        XCTAssertEqual(session.clockDue, due)
        XCTAssertTrue(session.clockTimer?.isValid == true)
        XCTAssertEqual(session.appliedBatches, applied + 1)
        reply.landing = true
        publishReply(reply, to: session)
        XCTAssertTrue(session.timerLanding)
        XCTAssertEqual(session.appliedBatches, applied + 2)
    }

    func testAReplyThatOnlyChangesTimerWaitsArmsItsWake() throws {
        let plan = try TableFixture.plan("""
        component App
          state enabled = false
          state fired = false
          action arm
            enabled = true
          action fire
            fired = true
          task delayed when enabled
            after(100, fire, wait=scrollend)
          view
            column
              button press=arm testId="arm"
                text "Arm"
              text fired ? "fired" : "waiting"
        """)
        let session = ExactApp.shared.makeSession(label: "reply-timer-wait")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 300, height: 200)).error)
        settleViewlessSession(session)
        let button = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "arm" })
        let reply = session.runtime.press(button.id, now: session.now())
        XCTAssertTrue(reply.ops.isEmpty)
        XCTAssertEqual(reply.timerDueMs, session.timerDue)
        XCTAssertTrue(session.taskTimers.matches(reply.nativeTimers))
        XCTAssertFalse(reply.timerWaits.isEmpty)
        XCTAssertNotEqual(reply.timerWaits, session.timerAdmissions.waits)
        publishReply(reply, to: session)
        XCTAssertEqual(session.timerAdmissions.waits, reply.timerWaits)
        waitFor("the idle window's uncapped scrollend timer fires", cap: 5) {
            session.presenter.views.values.contains { $0.props["text"] == "fired" }
        }
    }

    func testAnEmptyProvisionalReplyStillApplies() {
        let session = ExactApp.shared.makeSession(label: "reply-provisional")
        defer { session.destroy() }
        var reply = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
        reply.layoutProvisional = true
        let applied = session.appliedBatches
        publishReply(reply, to: session)
        XCTAssertEqual(session.appliedBatches, applied + 1)
    }

    func testOnlyAnEmptyBatchThatWouldHavePresentedRetriesTheReplacement() {
        let app = ExactApp.shared
        let session = app.makeSession(label: "replacement-retry")
        defer { session.destroy() }
        defer { app.replacements.withdraw(self, "test ended") }
        var attempts = 0
        XCTAssertEqual(app.replacements.submit(token: 1, now: true, producer: self, attempt: {
            attempts += 1
            return .held("requests")
        }, finished: { _ in }), .held("requests"))
        waitFor("the candidate's initial retry") { attempts == 2 }

        let applied = session.appliedBatches
        var batch = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
        session.applyUnlessEmpty(batch, scrolled: true)
        session.applyUnlessEmpty(batch, resized: true)
        session.applyUnlessEmpty(batch.made(by: .advance))
        let turn = expectation(description: "the replacement observer had a turn")
        let observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, false, 1) { _, _ in
            DispatchQueue.main.async { turn.fulfill() }
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .defaultMode)
        defer { CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .defaultMode) }
        wait(for: [turn], timeout: 5)
        XCTAssertEqual(attempts, 2, "scroll, measurements and timers leave the candidate held")

        batch.windowState = ["tab": "A"]
        publishReply(batch, to: session)
        waitFor("the skipped reply retries the candidate", cap: 5) { attempts >= 3 }
        XCTAssertEqual(attempts, 3)
        XCTAssertEqual(session.appliedBatches, applied)
    }
    #endif

    func testBusyRefreshRetriesAndDestroyedFillDoesNotHoldCompletion() throws {
        let wasAsync = ExactSession.asyncFills
        ExactSession.asyncFills = true
        defer { ExactSession.asyncFills = wasAsync }
        let fixture = try isolatedStore()
        defer { fixture.restore() }
        let app = fixture.app
        let session = app.makeSession(label: "store-busy")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("STORE_REFRESH"), size: CGSize(width: 300, height: 400)).error)
        session.apply(session.runtime.dataReady())
        let due = session.now() + 10000
        session.apply(Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due))
        let completed = expectation(description: "a busy refresh is retried after the owner callback")
        Owner.shared.sync {
            Owner.shared.callMain {
                session.refreshSharedStore { _ in completed.fulfill() }
            }
        }
        wait(for: [completed], timeout: 5)
        XCTAssertEqual(session.timerDue, due, "an unchanged store refresh leaves the existing clock armed")
        let bounded = expectation(description: "a persistently busy owner ends the refresh attempt")
        Owner.shared.sync {
            Owner.shared.callMain {
                session.refreshSharedStore { success in XCTAssertFalse(success); bounded.fulfill() }
                // Keep the owner in this callback while main serves retries.
                self.wait(for: [bounded], timeout: 5)
            }
        }
        let destroyed = app.makeSession(label: "store-destroyed")
        XCTAssertNil(destroyed.boot(plan: try fixturePlan("STORE_REFRESH"), size: CGSize(width: 300, height: 400)).error)
        // The real collection feedback starts an owner slice; its waiter is
        // cancelled synchronously when the window is destroyed.
        let node = try XCTUnwrap(destroyed.presenter.views.values.first)
        destroyed.presenter.listHost.onFill?(node.id, Data())
        XCTAssertTrue(destroyed.fillInFlight)
        let released = expectation(description: "destroying a queued reader releases the reload")
        destroyed.refreshSharedStore { _ in released.fulfill() }
        destroyed.destroy()
        wait(for: [released], timeout: 5)
    }
    func testForegroundDuringAnInflightRefreshIsRemembered() throws {
        let fixture = try isolatedStore()
        defer { fixture.restore() }
        let foreground = Notification.Name("wp12-test-foreground")
        let counter = fixture.root.appendingPathComponent("widget-store-generation")
        try Data("10".utf8).write(to: counter)
        var finish: ((Bool) -> Void)?
        var calls = 0
        var retried = expectation(description: "the notification during the refused refresh was remembered")
        let observer = try XCTUnwrap(WidgetContainer.observeStore(root: fixture.root, appID: fixture.appID, kinds: ["fixture"], foreground: [foreground]) { _, done in
            calls += 1
            if calls % 2 == 1 { finish = done } else { done(true); retried.fulfill() }
        })
        try Data("11".utf8).write(to: counter, options: .atomic)
        observer.check()
        NotificationCenter.default.post(name: foreground, object: nil)
        try XCTUnwrap(finish)(false)
        wait(for: [retried], timeout: 5)
        XCTAssertEqual(calls, 2)
        retried = expectation(description: "the notification during a successful refresh was remembered")
        try Data("12".utf8).write(to: counter, options: .atomic)
        observer.check()
        NotificationCenter.default.post(name: foreground, object: nil)
        try XCTUnwrap(finish)(true)
        wait(for: [retried], timeout: 5)
        XCTAssertEqual(calls, 4)
    }

    private func isolatedStore() throws -> (app: ExactApp, appID: String, root: URL, restore: () -> Void) {
        let app = ExactApp.shared
        let receipt = try XCTUnwrap(try JSONSerialization.jsonObject(with: Runtime.bakedCompat()) as? [String: Any])
        let appID = try XCTUnwrap((receipt["inputs"] as? [String: Any])?["app"] as? String)
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let variables = ["EXACT_APP_GROUP_ROOT": root.path, "EXACT_APP_GROUP_APP": appID, "EXACT_STORE": "real", "EXACT_WIDGET_WRITER": ""]
        let saved = variables.keys.map { key in (key, getenv(key).map { String(cString: $0) }) }
        for (key, value) in variables { setenv(key, value, 1) }
        let restore = {
            app.sharedStore.observer = nil
            for (key, value) in saved { if let value { setenv(key, value, 1) } else { unsetenv(key) } }
            app.observeSharedStore()
            try? FileManager.default.removeItem(at: root)
        }
        return (app, appID, root, restore)
    }
}
