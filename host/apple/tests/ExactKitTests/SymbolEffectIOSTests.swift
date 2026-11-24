// LLP 1077 D12: `symbolEffectValue`'s presence picks a symbol effect's form, as SwiftUI's `value:` and `isActive:` do.
#if os(iOS)
import Symbols
import UIKit
import XCTest
@testable import ExactKit

/// The effects UIKit is asked to add and remove, at the public Objective-C methods its Swift `addSymbolEffect`
/// and `removeAllSymbolEffects` call, each added one with whether it plays once (`.nonRepeating`) or in the
/// effect's default form.
private final class RecordingImageView: UIImageView {
    var added: [String] = []
    private static func archive(_ options: __NSSymbolEffectOptions) -> Data? {
        try? NSKeyedArchiver.archivedData(withRootObject: options, requiringSecureCoding: true)
    }
    override func __add(_ effect: __NSSymbolEffect, options: __NSSymbolEffectOptions, animated: Bool, completion: __UISymbolEffectCompletion?) {
        let name = "\(type(of: effect))".replacingOccurrences(of: "NSSymbol", with: "").replacingOccurrences(of: "Effect", with: "")
        var forms = [("once", __NSSymbolEffectOptions.withNonRepeating()), ("", __NSSymbolEffectOptions())]
        if #available(iOS 18.0, *) {
            forms += [("periodic", __NSSymbolEffectOptions().withRepeatBehavior(.behaviorPeriodic())),
                      ("continuous", __NSSymbolEffectOptions().withRepeatBehavior(.behaviorContinuous()))]
        }
        let form = forms.first { Self.archive($0.1) == Self.archive(options) }.map { $0.0 } ?? "\(options)"
        added.append(form.isEmpty ? name : "\(name) \(form)")
    }
    override func __add(_ effect: __NSSymbolEffect, options: __NSSymbolEffectOptions, animated: Bool) {
        __add(effect, options: options, animated: animated, completion: nil)
    }
    override func __add(_ effect: __NSSymbolEffect, options: __NSSymbolEffectOptions) { __add(effect, options: options, animated: true, completion: nil) }
    override func __add(_ effect: __NSSymbolEffect) { __add(effect, options: __NSSymbolEffectOptions(), animated: true, completion: nil) }
    /// Each time all effects were removed: what ends one still in flight.
    var removals = 0
    override func __removeAllSymbolEffects(with options: __NSSymbolEffectOptions, animated: Bool) { removals += 1 }
    func take() -> [String] { defer { added = [] }; return added }
}

final class SymbolEffectIOSTests: XCTestCase {
    private var window: UIWindow!
    override func tearDown() { window?.isHidden = true; window = nil }

    /// A bell symbol with `-exact-symbol-effect: <effect>`, drawn into a recording image view, and the
    /// value its `symbolEffectValue` starts with.
    private func symbol(_ effect: String, value: String?) throws -> (Presenter, RecordingImageView) {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": [:], "handlers": [], "style": [:]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0],
            ["op": "create", "id": 2, "kind": "image", "props": ["imageSource": "symbol:sf/bell.fill", "symbolName": "bell.fill"],
             "handlers": [], "style": ["font_size": 40]],
            ["op": "frame", "id": 2, "x": 20.0, "y": 20.0, "w": 50.0, "h": 50.0],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
        ]))
        let node = try XCTUnwrap(p.views[2]), leaf = RecordingImageView()
        node.symbolView?.removeFromSuperview()
        node.symbolView = leaf
        node.addSubview(leaf)
        if let value { p.apply(wireBatch([["op": "props", "id": 2, "set": ["symbolEffectValue": value], "clear": []]])) }
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["font_size": 40, "symbol_effect": effect]]]))
        return (p, leaf)
    }

    private func value(_ p: Presenter, _ value: String?) {
        p.apply(wireBatch([["op": "props", "id": 2, "set": value.map { ["symbolEffectValue": $0] } ?? [:],
                            "clear": value == nil ? ["symbolEffectValue"] : []]]))
    }

    /// With a value, a pulse plays once each time it changes, and not for the value it first shows; without one,
    /// it runs while set (expo/exact#1423).
    func testAPulseWithAValuePlaysOncePerChange() throws {
        let (p, leaf) = try symbol("pulse", value: "0")
        XCTAssertEqual(leaf.take(), [], "nothing plays for the first value")
        value(p, "1")
        XCTAssertEqual(leaf.take(), ["Pulse once"])
        value(p, "1")
        XCTAssertEqual(leaf.take(), [], "an unchanged value plays nothing")
        value(p, "2")
        XCTAssertEqual(leaf.take(), ["Pulse once"])
        value(p, nil)
        XCTAssertEqual(leaf.take(), ["Pulse"], "without the value, the pulse runs while set")
        value(p, "3")
        XCTAssertEqual(leaf.take(), [], "a value stops it; its first plays nothing")
    }

    func testAPulseWithoutAValueRunsWhileSet() throws {
        let (_, leaf) = try symbol("pulse", value: nil)
        XCTAssertEqual(leaf.take(), ["Pulse"])
    }

    /// Scale has no discrete form, so it holds while set with a value too.
    func testScaleRunsWhileSetEvenWithAValue() throws {
        let (p, leaf) = try symbol("scale", value: "0")
        XCTAssertEqual(leaf.take(), ["Scale"])
        value(p, "1")
        XCTAssertEqual(leaf.take(), [])
    }

    /// Without a value, iOS 18's indefinite forms run while set, where UIKit's default options would play them once.
    func testIndefiniteFormsRepeatWhileSet() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("wiggle, rotate and breathe are iOS 18's") }
        for (effect, added) in [("wiggle", "Wiggle periodic"), ("rotate", "Rotate continuous"), ("breathe", "Breathe continuous"),
                                ("variable-color", "VariableColor")] {
            let (_, leaf) = try symbol(effect, value: nil)
            XCTAssertEqual(leaf.take(), [added])
        }
    }

    /// Clearing or changing the effect ends a once-per-change play still in flight; the new effect waits for a
    /// change of the value.
    func testANewEffectEndsThePlayInFlight() throws {
        let (p, leaf) = try symbol("pulse", value: "0")
        value(p, "1")
        XCTAssertEqual(leaf.take(), ["Pulse once"])
        let removals = leaf.removals
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["font_size": 40, "symbol_effect": "bounce"]]]))
        XCTAssertEqual(leaf.removals, removals + 1)
        XCTAssertEqual(leaf.take(), [])
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["font_size": 40]]]))
        XCTAssertEqual(leaf.removals, removals + 2)
        XCTAssertEqual(leaf.take(), [])
    }

    /// A list row's view parked for reuse ends its effect, and the next row's first value plays nothing.
    func testARecycledSymbolStartsAfresh() throws {
        let (p, leaf) = try symbol("pulse", value: "0")
        value(p, "5")
        XCTAssertEqual(leaf.take(), ["Pulse once"])
        let removals = leaf.removals
        try XCTUnwrap(p.views[2]).recycle()
        XCTAssertEqual(leaf.removals, removals + 1)
        value(p, "0")
        XCTAssertEqual(leaf.take(), [], "the next row's first value")
    }

    /// A bounce plays once per change with a value, and from iOS 18, where it has an indefinite form, runs
    /// while set without one: UIKit's default options would play it once, so it repeats.
    func testABounceRunsWhileSetWithoutAValue() throws {
        let (p, leaf) = try symbol("bounce", value: "0")
        value(p, "1")
        XCTAssertEqual(leaf.take(), ["Bounce once"])
        value(p, nil)
        if #available(iOS 18.0, *) { XCTAssertEqual(leaf.take(), ["Bounce periodic"]) } else { XCTAssertEqual(leaf.take(), []) }
    }
}
#endif
