import XCTest
@testable import ExactWatchSync

/// `WatchReceive` and `WatchSend` searched over every order of events a small world
/// allows, and named cases for single orderings.
///
/// A world holds a machine's state and what the machine cannot see: WCSession, the
/// store, the app. A move is one outside event. Effects are performed as the adapters
/// perform them (`run`), and an effect that can fail branches the search. Counters
/// bound the world (contexts, store versions, faults), so the search ends by itself.
final class WatchSyncProtocolTests: XCTestCase {
    // MARK: Search

    /// Answers each choice an effect asks for from a script; a choice past its end is
    /// recorded so the search can try every answer.
    struct Chooser {
        let script: [Int]
        var asked = 0
        var wanted: Int?
        mutating func pick(_ options: Int) -> Int {
            defer { asked += 1 }
            if asked < script.count { return script[asked] }
            if wanted == nil { wanted = options }
            return 0
        }
    }

    /// Every world `body` reaches from `world`, over every answer to its choices.
    static func branches<World>(_ world: World, _ body: (inout World, inout Chooser) -> Void) -> [World] {
        var reached: [World] = [], scripts: [[Int]] = [[]]
        while let script = scripts.popLast() {
            var next = world, chooser = Chooser(script: script)
            body(&next, &chooser)
            if let options = chooser.wanted {
                scripts.append(contentsOf: (0..<options).map { script + [$0] })
            } else {
                reached.append(next)
            }
        }
        return reached
    }

    /// Visits every reachable world once, breadth first, until `check` names a
    /// violation. Answers how many worlds it visited.
    static func search<World: Hashable>(from start: World, limit: Int, moves: (World) -> [World], check: (World) -> String?) -> (worlds: Int, violation: String?) {
        var seen: Set<World> = [start], queue = [start], next = 0
        while next < queue.count {
            let world = queue[next]
            next += 1
            if let violation = check(world) { return (next, "\(violation): \(world)") }
            guard seen.count < limit else { return (next, "the search did not end within \(limit) worlds") }
            for reached in moves(world) where seen.insert(reached).inserted { queue.append(reached) }
        }
        return (next, nil)
    }

    // MARK: The watch

    enum Seen: Hashable {
        case nothing
        /// The read overlapped an unfinished import and kept no value.
        case incomplete
        case store(Int?)
    }

    struct Watch: Hashable {
        var s = WatchReceive.State()
        var activating = false
        var activated = false
        /// The newest context WCSession holds, by number.
        var retained: Int?
        var contexts = 0
        /// Contexts this watch can never open (another installation's key).
        var unopenable: Set<Int> = []
        /// The context the store holds whole, including that snapshot's removals.
        var imported: Int?
        /// An import began and did not finish.
        var journal = false
        var launched = false
        var opened = false
        /// What the live session last read.
        var session = Seen.nothing
        var subscribed = false
        /// WatchConnectivity took the last publication of the key.
        var published = false
        var faults = 0
        var restarts = 0
        var violation: String?

        static let contextLimit = 3, faultLimit = 3, restartLimit = 1

        var importable: Bool { retained.map { $0 != imported && !unopenable.contains($0) } ?? false }
        var read: Seen { journal ? .incomplete : .store(imported) }

        mutating func require(_ condition: Bool, _ message: @autoclosure () -> String) {
            if !condition, violation == nil { violation = message() }
        }

        mutating func feed(_ event: WatchReceive.Event, _ chooser: inout Chooser) {
            var state = s
            WatchReceive.run(&state, event) { effect in
                switch effect {
                case .activate:
                    require(!activating, "activated twice at once")
                    switch chooser.pick(faults < Self.faultLimit ? 3 : 2) {
                    case 0: activating = true
                    case 1:
                        activated = true
                        return .activated
                    default:
                        faults += 1
                        return .activationFailed
                    }
                case let .reconcile(context):
                    require(activated || !context, "the retained context was read before activation completed")
                    let fault = faults < Self.faultLimit
                    if context, importable {
                        switch chooser.pick(fault ? 4 : 1) {
                        case 0:
                            imported = retained; journal = false
                            return .reconciled(.applied, ready: true)
                        case 1:
                            // The store refused before the journal was written.
                            faults += 1
                            return .reconciled(.failed, ready: !journal)
                        case 2:
                            // A value write failed: the journal stands.
                            faults += 1; journal = true
                            return .reconciled(.failed, ready: false)
                        default:
                            // Imported, but the readiness request failed.
                            faults += 1; imported = retained; journal = false
                            return .reconciled(.applied, ready: false)
                        }
                    }
                    switch chooser.pick(fault ? 2 : 1) {
                    case 0: return .reconciled(.settled, ready: !journal)
                    default:
                        faults += 1
                        return .reconciled(.failed, ready: false)
                    }
                case .open:
                    require(!opened, "readers opened twice")
                    require(!journal, "readers opened over an unfinished import")
                    opened = true
                case .fail: require(!opened, "an open store was reported unreadable")
                case .refresh:
                    require(subscribed, "a refresh had no session to reach")
                    session = read
                case .reloadComplications: break
                case .publishKey:
                    require(activated, "the key was published before activation")
                    published = chooser.pick(faults < Self.faultLimit ? 2 : 1) == 0
                    if !published { faults += 1 }
                    return .published(published)
                case .schedule: break
                }
                return nil
            }
            s = state
        }

        /// Each outside event that can happen next, with every outcome of its effects.
        func moves() -> [Watch] {
            var all: [Watch] = []
            func move(_ body: @escaping (inout Watch, inout Chooser) -> Void) {
                all += WatchSyncProtocolTests.branches(self, body)
            }
            if !launched { move { w, c in w.launched = true; w.feed(.launch, &c) } }
            move { w, c in w.feed(.scene(active: !w.s.sceneActive), &c) }
            if activating {
                move { w, c in w.activating = false; w.activated = true; w.feed(.activated, &c) }
                if faults < Self.faultLimit {
                    move { w, c in w.activating = false; w.faults += 1; w.feed(.activationFailed, &c) }
                }
            }
            if contexts < Self.contextLimit {
                for openable in [true, false] {
                    move { w, c in
                        w.contexts += 1; w.retained = w.contexts
                        if !openable { w.unopenable.insert(w.contexts) }
                        // WCSession delivers to an activated session; otherwise it keeps the context.
                        if w.activated { w.feed(.context, &c) }
                    }
                }
            }
            if opened, session == .nothing { move { w, _ in w.session = w.read } }
            if session != .nothing, !subscribed { move { w, c in w.subscribed = true; w.feed(.subscribed, &c) } }
            if s.timer { move { w, c in w.feed(.retry, &c) } }
            if restarts < Self.restartLimit {
                // The process ends. The store and WCSession's retained context outlive it.
                move { w, _ in
                    var next = Watch()
                    (next.retained, next.contexts, next.unopenable) = (w.retained, w.contexts, w.unopenable)
                    (next.imported, next.journal, next.faults, next.restarts) = (w.imported, w.journal, w.faults, w.restarts + 1)
                    w = next
                }
            }
            return all
        }

        mutating func check() {
            require(activating == (s.link == .activating), "the link disagrees with WCSession")
            require(opened == (s.readers == .open), "readers disagree with the boot")
            require(!s.sceneActive || !WatchReceive.owed(s) || s.timer, "owed work has no retry scheduled")
            require(!activated || !importable || s.held, "a delivered context was forgotten before it was imported")
            require(journal || session == .nothing || session == .store(imported) || (!subscribed && s.refreshOwed),
                    "readers older than the last applied snapshot, including its removals, were not invalidated")
            require(!activated || published || s.keyOwed, "a refused publication of the key was forgotten")
        }

        /// Local readers make progress even if activation never answers. Before
        /// activation they see only what the last applied context left in the store.
        mutating func settleOffline() {
            guard !activated else { return }
            var chooser = Chooser(script: Array(repeating: 0, count: 64))
            let before = opened
            faults = Self.faultLimit
            for _ in 0..<8 {
                if !launched { launched = true; feed(.launch, &chooser) }
                else if !s.sceneActive { feed(.scene(active: true), &chooser) }
                else if s.timer { feed(.retry, &chooser) }
                else { break }
            }
            if journal {
                require(opened == before, "offline readers opened over an unfinished import")
            } else {
                require(opened, "local readers waited for activation")
                if session == .nothing { session = read }
                if !subscribed { subscribed = true; feed(.subscribed, &chooser) }
                require(session == .store(imported), "offline readers did not read the last applied snapshot")
            }
        }

        /// With no more faults and no new context, everything owed finishes.
        mutating func settle() {
            var chooser = Chooser(script: Array(repeating: 0, count: 64))
            let before = opened
            faults = Self.faultLimit
            for _ in 0..<32 {
                if !launched { launched = true; feed(.launch, &chooser) }
                else if !s.sceneActive { feed(.scene(active: true), &chooser) }
                else if activating { activating = false; activated = true; feed(.activated, &chooser) }
                else if s.timer { feed(.retry, &chooser) }
                else if opened, session == .nothing { session = read }
                else if session != .nothing, !subscribed { subscribed = true; feed(.subscribed, &chooser) }
                else { break }
            }
            require(published, "the key was never published")
            guard !journal else {
                // Only a context this watch can open finishes the import.
                require(!importable, "an openable context did not finish the import")
                require(opened == before, "readers opened over an unfinished import")
                return
            }
            require(!importable && !s.held, "the retained context was never imported")
            require(opened, "the store is whole and readers never opened")
            require(session == .store(imported), "the live session never read the last import")
            require(!s.timer && !WatchReceive.owed(s), "a retry is still scheduled with nothing owed")
        }
    }

    func testEveryOrderOfWatchEventsKeepsReadersOnTheLastAppliedSnapshot() {
        let found = Self.search(from: Watch(), limit: 2_000_000, moves: { $0.moves() }) { world in
            var world = world
            world.check()
            var offline = world
            if offline.violation == nil { offline.settleOffline() }
            if let violation = offline.violation { return violation }
            if world.violation == nil { world.settle() }
            return world.violation
        }
        XCTAssertNil(found.violation)
        // A search that stops early proves nothing.
        XCTAssertGreaterThan(found.worlds, 30_000)
    }

    private func step(_ s: inout WatchReceive.State, _ events: WatchReceive.Event...) -> [WatchReceive.Effect] {
        events.flatMap { WatchReceive.step(&s, $0) }
    }

    /// The local store is readable before activation, even if WCSession never answers.
    /// Only the retained context waits for activation.
    func testReadersOpenOnTheLocalStoreBeforeActivation() {
        var s = WatchReceive.State()
        XCTAssertEqual(step(&s, .launch), [.reconcile(context: false), .activate])
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: true)), [.open])
        XCTAssertEqual(step(&s, .scene(active: true), .subscribed, .retry), [])
        XCTAssertEqual(step(&s, .activated), [.publishKey, .reconcile(context: true)])
        XCTAssertEqual(step(&s, .reconciled(.applied, ready: true)), [.reloadComplications, .refresh])
        XCTAssertEqual(s.readers, .open)
        // The scene's event may also arrive first.
        s = WatchReceive.State()
        XCTAssertEqual(step(&s, .scene(active: true), .launch), [.activate, .reconcile(context: false)])
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: true)), [.open])
        XCTAssertEqual(step(&s, .activated), [.publishKey, .reconcile(context: true)])
    }

    func testLocalReadinessIsRetriedWhileActivationIsPending() {
        var s = WatchReceive.State()
        _ = step(&s, .scene(active: true), .launch)
        XCTAssertEqual(step(&s, .reconciled(.failed, ready: false)), [.fail, .schedule(seconds: 1)])
        XCTAssertEqual(step(&s, .retry), [.reconcile(context: false)])
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: true)), [.open])
        XCTAssertEqual(s.link, .activating)
        XCTAssertFalse(s.timer || WatchReceive.owed(s))
    }

    /// Activation can answer synchronously on a retry or foreground. A local
    /// readiness reply must be consumed before activation starts a retained import.
    func testSynchronousReactivationKeepsAFailedImportOwed() {
        var failed = Watch()
        failed.launched = true
        failed.retained = 1
        failed.contexts = 1
        failed.s.link = .failed
        failed.s.readers = .waiting
        failed.s.sceneActive = true
        failed.s.timer = true
        for event in [WatchReceive.Event.retry, .scene(active: true)] {
            let outcomes = Self.branches(failed) { world, chooser in world.feed(event, &chooser) }
            let unfinished = outcomes.filter { $0.activated && $0.journal }
            XCTAssertFalse(unfinished.isEmpty, "the search did not exercise a partial import")
            for var world in unfinished {
                world.check()
                XCTAssertNil(world.violation)
                XCTAssertTrue(world.s.held, "an unfinished import was forgotten")
                XCTAssertTrue(world.s.timer, "the unfinished import has no retry")
                world.settle()
                XCTAssertNil(world.violation)
            }
        }
    }

    /// An import that fails at launch does not strand the process, and one that fails
    /// after boot is tried again while the scene is active.
    func testAFailedImportIsRetriedWithoutARestart() {
        var s = WatchReceive.State()
        _ = step(&s, .scene(active: true), .launch, .activated)
        XCTAssertEqual(step(&s, .reconciled(.failed, ready: false)), [.fail, .schedule(seconds: 1)])
        XCTAssertEqual(step(&s, .retry), [.reconcile(context: true)])
        XCTAssertEqual(step(&s, .reconciled(.failed, ready: true)), [.open, .schedule(seconds: 2)],
                       "a failed delivery does not hide the last applied store")
        XCTAssertEqual(step(&s, .retry, .reconciled(.applied, ready: true)), [.reconcile(context: true), .reloadComplications])
        XCTAssertFalse(s.timer)

        // After boot: the journal stands, so the session keeps what it read.
        _ = step(&s, .subscribed)
        XCTAssertEqual(step(&s, .context), [.reconcile(context: true)])
        XCTAssertEqual(step(&s, .reconciled(.failed, ready: false)), [.schedule(seconds: 1)])
        XCTAssertEqual(step(&s, .retry, .reconciled(.applied, ready: true)), [.reconcile(context: true), .reloadComplications, .refresh])

        // A scene that left stops the retries; coming back starts again at once.
        _ = step(&s, .context, .reconciled(.failed, ready: false), .scene(active: false))
        XCTAssertEqual(step(&s, .retry), [])
        XCTAssertEqual(step(&s, .scene(active: true)), [.publishKey, .reconcile(context: true)])
    }

    /// An import that lands after the session booted and before it subscribed is
    /// refreshed at subscription, and only then.
    func testAnImportAfterBootRefreshesTheSessionOnceItSubscribes() {
        var s = WatchReceive.State()
        _ = step(&s, .launch, .activated, .reconciled(.settled, ready: true))
        XCTAssertEqual(s.readers, .open)
        var idle = s
        XCTAssertEqual(step(&idle, .subscribed), [], "nothing was imported after boot")
        XCTAssertEqual(step(&s, .context, .reconciled(.applied, ready: true)), [.reconcile(context: true), .reloadComplications])
        XCTAssertEqual(step(&s, .subscribed), [.refresh])
        XCTAssertEqual(step(&s, .subscribed), [], "one import refreshes once")
        XCTAssertEqual(step(&s, .context, .reconciled(.applied, ready: true)), [.reconcile(context: true), .reloadComplications, .refresh])
        // A context this watch already holds changes nothing.
        XCTAssertEqual(step(&s, .context, .reconciled(.settled, ready: true)), [.reconcile(context: true)])
    }

    /// A context the watch refuses, or a session that cannot activate, opens the last
    /// accepted store instead of refusing every launch.
    func testARefusedContextOrAFailedActivationOpensTheLastAcceptedStore() {
        var s = WatchReceive.State()
        _ = step(&s, .launch, .activated)
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: true)), [.open])

        s = WatchReceive.State()
        _ = step(&s, .scene(active: true), .launch)
        XCTAssertEqual(step(&s, .activationFailed), [.reconcile(context: false), .schedule(seconds: 1)])
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: true)), [.open])
        // The activation is tried again, and its context refreshes the open session.
        XCTAssertEqual(step(&s, .retry), [.activate])
        XCTAssertEqual(step(&s, .activated, .reconciled(.applied, ready: true)), [.publishKey, .reconcile(context: true), .reloadComplications])
        XCTAssertTrue(s.refreshOwed)
    }

    /// While an import is unfinished, a context that cannot finish it leaves readers
    /// closed and the retries running; the one that can opens them.
    func testAnUnfinishedImportKeepsReadersClosedUntilAContextFinishesIt() {
        var s = WatchReceive.State()
        _ = step(&s, .scene(active: true), .launch, .activated)
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: false)), [.fail, .schedule(seconds: 1)])
        XCTAssertEqual(s.readers, .waiting)
        XCTAssertEqual(step(&s, .retry, .reconciled(.settled, ready: false)), [.reconcile(context: true), .fail, .schedule(seconds: 2)])
        XCTAssertEqual(step(&s, .context, .reconciled(.applied, ready: true)), [.reconcile(context: true), .reloadComplications, .open])
    }

    /// A publication of the key that WatchConnectivity refuses is tried again while
    /// the scene is active, and readers open without it.
    func testARefusedKeyPublicationIsRetriedAndReadersOpenWithoutIt() {
        var s = WatchReceive.State()
        _ = step(&s, .scene(active: true), .launch, .activated)
        XCTAssertEqual(step(&s, .published(false)), [.schedule(seconds: 1)])
        XCTAssertEqual(step(&s, .reconciled(.settled, ready: true)), [.open], "a refused publication kept readers closed")
        XCTAssertEqual(step(&s, .retry, .published(false)), [.publishKey, .schedule(seconds: 2)])
        XCTAssertEqual(step(&s, .retry, .published(true)), [.publishKey])
        XCTAssertFalse(s.timer || WatchReceive.owed(s))
        XCTAssertEqual(step(&s, .retry), [], "a key the phone holds was published again")

        // A scene that left stops the retries; coming back publishes at once.
        _ = step(&s, .scene(active: false), .scene(active: true), .published(false), .scene(active: false))
        XCTAssertEqual(step(&s, .retry), [])
        XCTAssertEqual(step(&s, .scene(active: true)), [.publishKey])
    }

    // MARK: The phone

    struct Phone: Hashable {
        var s = WatchSend.State()
        var activating = false
        var activated = false
        /// A paired watch has announced its key.
        var watch = false
        /// The store's version, and whether its selected values exceed the transport.
        var content = 0
        var oversized = false
        /// The revision the store has spent.
        var spent = 0
        /// The content each handed revision carried, by revision.
        var handed: [Int] = []
        /// The content the current watch was handed.
        var delivered: Int? = 0
        /// The refusal was reported, and nothing has been handed over since.
        var reported = false
        var observers = 0
        var changes = 0
        var faults = 0
        var restarts = 0
        var violation: String?

        static let contentLimit = 3, changeLimit = 3, faultLimit = 3, restartLimit = 1

        mutating func require(_ condition: Bool, _ message: @autoclosure () -> String) {
            if !condition, violation == nil { violation = message() }
        }

        mutating func feed(_ event: WatchSend.Event, _ chooser: inout Chooser) {
            var state = s
            WatchSend.run(&state, event) { effect in
                switch effect {
                case .observeForeground:
                    observers += 1
                    require(observers == 1, "a second foreground observer was registered")
                case .activate:
                    require(!activating, "activated twice at once")
                    activating = true; activated = false
                case .enqueue: break
                case .submit:
                    require(activated, "a send began before activation")
                    guard watch else { return .submitted(.refused(.noWatch)) }
                    // Measured before any revision is spent.
                    guard !oversized else { return .submitted(.refused(.oversized)) }
                    switch chooser.pick(faults < Self.faultLimit ? 3 : 1) {
                    case 0:
                        spent += 1; handed.append(content); delivered = content; reported = false
                        return .submitted(.accepted)
                    case 1:
                        // The store failed before the reserve.
                        faults += 1
                        return .submitted(.failed)
                    default:
                        // WatchConnectivity threw: the reserved revision is released.
                        faults += 1
                        return .submitted(.failed)
                    }
                case let .report(refusal):
                    require(refusal == .oversized && oversized, "a refusal was reported that did not happen")
                    require(!reported, "one refusal was reported twice")
                    reported = true
                case .schedule: break
                }
                return nil
            }
            s = state
        }

        func moves() -> [Phone] {
            var all: [Phone] = []
            func move(_ body: @escaping (inout Phone, inout Chooser) -> Void) {
                all += WatchSyncProtocolTests.branches(self, body)
            }
            if content < Self.contentLimit {
                for oversized in [false, true] {
                    move { p, c in p.content += 1; p.oversized = oversized; p.feed(.changed, &c) }
                }
            }
            // First pixel in a process that has sent nothing yet.
            if !s.observing { move { p, c in p.feed(.changed, &c) } }
            move { p, c in p.feed(.foreground, &c) }
            if activating {
                move { p, c in p.activating = false; p.activated = true; p.feed(.activated, &c) }
                if faults < Self.faultLimit {
                    move { p, c in p.activating = false; p.faults += 1; p.feed(.activationFailed, &c) }
                }
            }
            if changes < Self.changeLimit {
                // The watch announces a key, or is unpaired.
                move { p, c in p.changes += 1; p.watch.toggle(); p.delivered = nil; p.feed(.peer, &c) }
                if activated {
                    // The person switched watches: WCSession deactivated.
                    move { p, c in p.changes += 1; p.watch = false; p.delivered = nil; p.feed(.deactivated, &c) }
                }
            }
            if s.queued { move { p, c in p.feed(.send, &c) } }
            if s.timer { move { p, c in p.feed(.retry, &c) } }
            if restarts < Self.restartLimit {
                // The process ends. The store, its spent revisions and the watch outlive it.
                move { p, _ in
                    (p.s, p.activating, p.activated, p.observers) = (WatchSend.State(), false, false, 0)
                    (p.reported, p.restarts) = (false, p.restarts + 1)
                }
            }
            return all
        }

        /// The newest content reached the current watch, or was reported as too large.
        var current: Bool { delivered == content || (oversized && reported) }

        mutating func check() {
            require(activating == (s.link == .activating), "the link disagrees with WCSession")
            require(spent == handed.count, "a revision was spent on a send that never left")
            require(handed == handed.sorted(), "a later revision carried older values")
            // A process owes nothing before its first pixel.
            require(!s.observing || current || s.owed, "a change was forgotten before it was sent")
            if s.owed {
                switch s.link {
                case .active: require(s.queued || s.timer || s.noWatch, "an owed send is neither queued nor scheduled")
                case .failed: require(s.timer, "a failed activation has no retry scheduled")
                case .idle: require(false, "a send is owed and nothing started")
                case .activating: break
                }
            }
        }

        /// With a watch present and no more faults, the newest values are handed over.
        mutating func settle() {
            var chooser = Chooser(script: Array(repeating: 0, count: 64))
            faults = Self.faultLimit
            if !watch { watch = true; delivered = nil; feed(.peer, &chooser) }
            for _ in 0..<32 {
                if activating { activating = false; activated = true; feed(.activated, &chooser) }
                else if s.queued { feed(.send, &chooser) }
                else if s.timer { feed(.retry, &chooser) }
                else { break }
            }
            require(spent == handed.count, "a revision was spent on a send that never left")
            // Before first pixel nothing has asked for a send.
            guard s.observing else { return }
            require(current, "the newest values never left and no refusal was reported")
            require(!s.owed && !s.queued && !s.timer, "work is still owed with nothing to send")
        }
    }

    func testEveryOrderOfPhoneEventsSendsTheNewestValuesOrReportsWhy() {
        let found = Self.search(from: Phone(), limit: 2_000_000, moves: { $0.moves() }) { world in
            var world = world
            world.check()
            if world.violation == nil { world.settle() }
            return world.violation
        }
        XCTAssertNil(found.violation)
        XCTAssertGreaterThan(found.worlds, 100_000)
    }

    private func step(_ s: inout WatchSend.State, _ events: WatchSend.Event...) -> [WatchSend.Effect] {
        events.flatMap { WatchSend.step(&s, $0) }
    }

    /// Every node's redraw asks for a send. Changes that arrive before the queued
    /// send's turn share one snapshot, and a later change sends again.
    func testChangesBeforeASendShareOneSnapshot() {
        var s = WatchSend.State()
        XCTAssertEqual(step(&s, .changed), [.observeForeground, .activate])
        XCTAssertEqual(step(&s, .changed, .changed), [], "nothing is sent before activation")
        XCTAssertEqual(step(&s, .activated), [.enqueue])
        XCTAssertEqual((0..<20).flatMap { _ in step(&s, .changed) }, [])
        XCTAssertEqual(step(&s, .send), [.submit])
        XCTAssertEqual(step(&s, .submitted(.accepted), .send, .foreground), [], "nothing changed since the accepted send")
        XCTAssertEqual(step(&s, .changed), [.enqueue])
    }

    /// A send WatchConnectivity refuses is tried again without another edit, and
    /// values that cannot fit are reported once, not retried.
    func testAFailedSendIsRetriedAndAnOversizedOneReported() {
        var s = WatchSend.State()
        _ = step(&s, .changed, .activated, .send)
        XCTAssertEqual(step(&s, .submitted(.failed)), [.schedule(seconds: 1)])
        XCTAssertTrue(s.owed)
        XCTAssertEqual(step(&s, .retry, .send), [.enqueue, .submit])
        XCTAssertEqual(step(&s, .submitted(.failed)), [.schedule(seconds: 2)])
        XCTAssertEqual(step(&s, .retry, .send, .submitted(.accepted)), [.enqueue, .submit])
        XCTAssertFalse(s.owed || s.timer)

        _ = step(&s, .changed, .send)
        XCTAssertEqual(step(&s, .submitted(.refused(.oversized))), [.report(.oversized)])
        XCTAssertEqual(step(&s, .foreground, .retry), [], "the same values cannot fit")
        // The next change may fit. Until one does, the refusal is not reported again.
        XCTAssertEqual(step(&s, .changed, .send), [.enqueue, .submit])
        XCTAssertEqual(step(&s, .submitted(.refused(.oversized))), [])
        XCTAssertEqual(step(&s, .changed, .send, .submitted(.accepted)), [.enqueue, .submit])
        XCTAssertEqual(step(&s, .changed, .send, .submitted(.refused(.oversized))), [.enqueue, .submit, .report(.oversized)])
        // A watch that announces its key is sent what an earlier submit could not.
        _ = step(&s, .changed, .send)
        XCTAssertEqual(step(&s, .submitted(.refused(.noWatch))), [])
        XCTAssertEqual(step(&s, .peer, .send), [.enqueue, .submit])
    }

    /// The foreground observer is registered once, however many activations fail.
    func testOneForegroundObserverSurvivesFailedActivations() {
        var s = WatchSend.State()
        var effects = step(&s, .changed)
        for _ in 0..<5 {
            effects += step(&s, .activationFailed, .changed, .retry, .foreground)
        }
        XCTAssertEqual(effects.filter { $0 == .observeForeground }.count, 1)
        XCTAssertEqual(effects.filter { $0 == .activate }.count, 6, "each failure is followed by one new activation")
        // A switched watch activates again and is owed the values.
        _ = step(&s, .activated, .send, .submitted(.accepted))
        XCTAssertEqual(step(&s, .deactivated), [.activate])
        XCTAssertEqual(step(&s, .activated), [.enqueue])
    }
}
