import ObjectiveC
import XCTest
@testable import ExactKit

/// The delegate slot Exact keeps (`ForwardingDelegate`), against a stand-in
/// for the platform's object that reads a delegate's answers as AppKit and
/// UIKit do (measured: ToolbarCustomizationMacTests, NavigationMoreIOSTests):
/// when it is set, in order, keeping the answers of the write that ends last.
/// The install cases give a delegate, or retire the slot, from inside the
/// platform's read of the last one's answers, as an app's `responds(to:)` may,
/// and check that the proxy wrote no slot from inside a write; the others
/// check when a delegate goes and the slot a proxy fills.
final class ForwardingDelegateTests: XCTestCase {
    private static let first = #selector(Probing.first), second = #selector(Probing.second)

    /// The answers kept are those of the delegate the slot holds: a delegate
    /// given while the platform reads the last one's answers is installed
    /// once that read ends.
    func testADelegateGivenWhileThePlatformReadsAnswersIsInstalledAfterTheRead() {
        let (slot, proxy) = filled()
        let app = Probe()
        app.asked = { proxy.give(nil) }
        proxy.give(app)
        XCTAssertNil(proxy.appDelegate, "cleared")
        XCTAssertEqual(slot.unreceived, [], "no answer kept without a receiver")
        XCTAssertTrue(slot.held === proxy, "the slot is Exact's")
        XCTAssertEqual(slot.nested, 0)
    }

    /// A delegate the proxy lets go is kept until the turn ends: what its
    /// `deinit` does meets the host's settled state.
    func testADelegateGoesOnALaterTurn() {
        let (_, proxy) = filled()
        weak var given: Probe?
        autoreleasepool {
            let app = Probe()
            given = app
            proxy.give(app)
            proxy.give(nil)
        }
        XCTAssertNotNil(given, "kept until the turn ends")
        waitForMainQueue()
        XCTAssertNil(given, "gone after it")
    }

    /// A delegate given in place of one not installed yet goes after the
    /// platform's write, and what its `deinit` gives is installed in turn.
    func testADelegateReplacedBeforeItIsInstalledGoesAfterTheWrite() {
        let (slot, proxy) = filled()
        weak var last: Probe?
        var inWrite: Bool?
        let app = Probe()
        app.asked = {
            // Held only by the proxy until the next delegate replaces it
            proxy.give(Probe {
                inWrite = slot.writing
                let successor = Probe()
                last = successor
                proxy.give(successor)
            })
            proxy.give(Probe())
        }
        proxy.give(app)
        waitForMainQueue()
        XCTAssertEqual(inWrite, false, "it went after the platform's write")
        XCTAssertNotNil(last)
        XCTAssertTrue(proxy.appDelegate === last, "the delegate given last")
        XCTAssertEqual(slot.answered, Slot.asked, "its answers")
        XCTAssertEqual(slot.unreceived, [])
        XCTAssertEqual(slot.nested, 0)
    }

    /// The slot leaving Exact during an install drops the delegate waiting
    /// for it and is emptied once the platform's write ends.
    func testARetireDuringAnInstallDropsTheDelegateWaiting() {
        let (slot, proxy) = filled()
        weak var waiting: Probe?
        let app = Probe()
        app.asked = {
            let next = Probe()
            waiting = next
            proxy.give(next)
            proxy.retire()
        }
        autoreleasepool { proxy.give(app) }
        waitForMainQueue()
        XCTAssertNil(waiting, "released")
        XCTAssertNil(proxy.appDelegate)
        XCTAssertNil(slot.held, "the slot is empty")
        XCTAssertEqual(slot.nested, 0, "emptied after the write it was asked in")
        proxy.give(Probe())
        XCTAssertNil(proxy.appDelegate, "a delegate given later is dropped")
    }

    /// A delegate the app replaces goes once the slot holds the proxy again
    /// with the new delegate's answers: its `deinit` finds the slot whole.
    func testAReplacedDelegateGoesAfterTheSwap() {
        let (slot, proxy) = filled()
        var held: Bool?, kept: [Selector]?, answers: Bool?
        autoreleasepool {
            proxy.give(Probe(answers: [Self.first]) {
                held = slot.held === proxy
                kept = slot.answered
                answers = proxy.responds(to: Self.second)
            })
        }
        autoreleasepool { proxy.give(Probe(answers: [Self.second])) }
        waitForMainQueue()
        XCTAssertEqual(held, true, "the slot held the proxy again")
        XCTAssertEqual(kept, [Self.second], "with the answers the platform read of the new delegate")
        XCTAssertEqual(answers, true)
        XCTAssertEqual(slot.nested, 0)
    }

    /// A retire empties the slot before the app's delegate goes.
    func testARetiredSlotIsEmptyAsTheDelegateGoes() {
        let (slot, proxy) = filled()
        var held: Bool?
        autoreleasepool { proxy.give(Probe { held = slot.held != nil }) }
        proxy.retire()
        waitForMainQueue()
        XCTAssertEqual(held, false)
    }

    /// A retire leaves a delegate written into the slot outside Exact, and
    /// lets the app's delegate go.
    func testARetireLeavesADelegateWrittenOutsideExact() {
        let (slot, proxy) = filled()
        weak var given: Probe?
        autoreleasepool {
            let app = Probe()
            given = app
            proxy.give(app)
        }
        let written = Probe()
        slot.slotDelegate = written
        autoreleasepool { proxy.retire() }
        waitForMainQueue()
        XCTAssertTrue(slot.held === written, "the slot keeps it")
        XCTAssertNil(given, "the app's delegate goes")
    }

    /// A proxy whose slot went without a retire lets the app's delegate go
    /// at the next delegate given, and fills no other slot.
    func testAProxyWhoseSlotWentWithoutARetireLetsItsDelegateGo() {
        let proxy = ForwardingDelegate()
        weak var given: Probe?
        autoreleasepool {
            let slot = Slot()
            proxy.fill(slot) { _ in }
            let app = Probe()
            given = app
            proxy.give(app)
        }
        waitForMainQueue()
        XCTAssertNotNil(given, "kept while nothing is given")
        autoreleasepool { proxy.give(nil) }
        waitForMainQueue()
        XCTAssertNil(given, "let go")
        let next = Slot()
        proxy.fill(next) { _ in }
        XCTAssertNil(next.held, "only the slot first given")
    }

    /// A proxy fills only the slot it was first given: another keeps its own
    /// delegate.
    func testAProxyFillsOnlyItsFirstSlot() {
        let (slot, proxy) = filled()
        let other = Slot(), own = Probe()
        other.slotDelegate = own
        proxy.fill(other) { _ in }
        XCTAssertTrue(other.held === own, "the other slot keeps its delegate")
        XCTAssertTrue(slot.held === proxy)
        XCTAssertTrue(proxy.fillsSlot, "the first is still the proxy's")
    }

    private func filled() -> (Slot, ForwardingDelegate) {
        let slot = Slot(), proxy = ForwardingDelegate()
        proxy.fill(slot) { _ in }
        // Keep the slot and the proxy for the test: a slot holds its delegate weakly, as the platform's do, and a proxy
        // whose slot went takes the path for a slot gone without a retire
        proxies.append(proxy)
        slots.append(slot)
        return (slot, proxy)
    }
    private var proxies: [ForwardingDelegate] = []
    private var slots: [Slot] = []
}

@objc private protocol Probing {
    @objc optional func first()
    @objc optional func second()
}

/// A platform object's delegate slot: weak, its delegate's answers for
/// `asked` read when it is set and kept from the write that ends last.
private final class Slot: NSObject, DelegateSlotOwner {
    static let asked = [#selector(Probing.first), #selector(Probing.second)]
    weak var held: AnyObject?
    private(set) var answered: [Selector] = []
    /// Whether a write is running, and how many writes began inside another.
    private(set) var writing = false, nested = 0
    var slotDelegate: AnyObject? {
        get { held }
        set {
            if writing { nested += 1 }
            let outer = writing
            writing = true
            defer { writing = outer }
            held = newValue
            answered = Self.asked.filter { newValue?.responds(to: $0) ?? false }
        }
    }
    /// The methods it kept an answer for that a call would find no receiver
    /// for: the delegate it holds neither implements one nor forwards it to
    /// an object that does (UIKit and AppKit raise "unrecognized selector").
    var unreceived: [Selector] {
        guard let held = held as? NSObject else { return [] }
        return answered.filter { selector in
            if class_respondsToSelector(type(of: held), selector) { return false }
            guard let target = held.forwardingTarget(for: selector) else { return true }
            return !class_respondsToSelector(type(of: target as AnyObject), selector)
        }
    }
}

/// An app's delegate: it answers for `answers`, runs `asked` the first time
/// it is asked whether it answers a method, and runs `goes` in `deinit`.
private final class Probe: NSObject, Probing {
    var asked: (() -> Void)?
    let answers: [Selector]
    let goes: (() -> Void)?
    init(answers: [Selector] = Slot.asked, goes: (() -> Void)? = nil) { (self.answers, self.goes) = (answers, goes) }
    func first() {}
    func second() {}
    override func responds(to selector: Selector!) -> Bool {
        guard Slot.asked.contains(selector) else { return super.responds(to: selector) }
        if let asked { self.asked = nil; asked() }
        return answers.contains(selector)
    }
    deinit { goes?() }
}
