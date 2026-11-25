// Appended to GlobalShortcuts.swift by pairs.test.mjs to test delivery without an Accessibility grant.
private extension HotKeys {
    static func testPairs() -> [String] {
        var failures: [String] = []
        // Key codes and device flags from HIToolbox/Events.h and IOKit/hidsystem/IOLLEvent.h.
        let pairs: [(String, UInt16, UInt16, UInt, UInt, NSEvent.ModifierFlags)] = [
            ("Meta+Meta", 55, 54, 0x8, 0x10, .command),
            ("Shift+Shift", 56, 60, 0x2, 0x4, .shift),
            ("Control+Control", 59, 62, 0x1, 0x2000, .control),
            ("Alt+Alt", 58, 61, 0x20, 0x40, .option),
        ]
        for (keys, left, right, leftMask, rightMask, generic) in pairs {
            for (first, second, firstMask, secondMask) in [(left, right, leftMask, rightMask), (right, left, rightMask, leftMask)] {
                let firstOnly = NSEvent.ModifierFlags(rawValue: generic.rawValue | firstMask)
                let secondOnly = NSEvent.ModifierFlags(rawValue: generic.rawValue | secondMask)
                let both = firstOnly.union(secondOnly)
                func run(_ name: String, _ events: [(UInt16, NSEvent.ModifierFlags, Int)]) {
                    let registry = HotKeys()
                    var received: [String] = []
                    let context = ExactPackageContext(agent: false, session: "pairs", changed: { _ in }, log: { _ in }, now: { 0 },
                        emit: { name, args in
                            let detail = args.last as? [String: String] ?? [:]
                            received.append("\(name) \(args.first as? String ?? "") \(detail["id"] ?? "") \(detail["keys"] ?? "")")
                        })
                    let module = GlobalShortcutsModule(context: context)
                    let chord = Chord(number: 1)
                    registry.chords[keys] = chord
                    registry.add(chord, id: "palette", by: module)
                    var expected: [String] = []
                    var previousPresses = 0
                    var held = false
                    for (index, step) in events.enumerated() {
                        let (code, flags, presses) = step
                        let event = NSEvent.keyEvent(with: .flagsChanged, location: .zero, modifierFlags: flags,
                            timestamp: Double(index), windowNumber: 0, context: nil, characters: "",
                            charactersIgnoringModifiers: "", isARepeat: false, keyCode: code)!
                        registry.flags(event)
                        registry.flags(event)
                        let bothDown = flags.contains(both)
                        if !bothDown || code == left || code == right {
                            if held { expected.append("globalshortcutup palette palette \(keys)") }
                            held = bothDown
                        }
                        if presses > previousPresses { expected.append("globalshortcut palette palette \(keys)") }
                        previousPresses = presses
                        if received != expected {
                            failures.append("\(keys), first=\(first), \(name), event \(index): expected \(expected), got \(received)")
                        }
                    }
                }
                run("first observed event is a release", [
                    (first, [], 0), (second, secondOnly, 0), (second, [], 0),
                    (second, secondOnly, 0), (second, [], 0),
                    (first, firstOnly, 0), (second, both, 1), (first, secondOnly, 1),
                    (first, both, 2), (second, firstOnly, 2), (first, [], 2),
                ])
                run("missed release", [
                    (first, firstOnly, 0), (second, secondOnly, 0),
                    (first, both, 1), (first, secondOnly, 1), (second, [], 1),
                ])
                run("other side already held when monitoring starts", [
                    (second, both, 1), (second, firstOnly, 1), (second, both, 2),
                ])
                run("release missed while other side stays held", [
                    (first, firstOnly, 0), (second, both, 1),
                    (second, both, 2), (second, firstOnly, 2), (first, [], 2),
                ])
                run("unrelated flag reveals a missed release", [
                    (second, both, 1), (57, firstOnly.union(.capsLock), 1), (second, both, 2),
                ])
                run("unrelated modifier changes and releases", [
                    (first, firstOnly, 0), (second, both, 1),
                    (57, both.union(.capsLock), 1), (57, both, 1),
                    (first, secondOnly, 1), (first, [], 1), (second, secondOnly, 1),
                ])
            }
        }
        for (keys, left, right, leftMask, rightMask, _) in pairs where keys == "Meta+Meta" || keys == "Alt+Alt" {
            let side = keys == "Meta+Meta" ? "MetaRight" : "AltRight"
            let registry = HotKeys(), chord = Chord(number: 1)
            var events: [String] = []
            let reader = GlobalShortcutsModule(context: ExactPackageContext(agent: false, session: "side",
                changed: { _ in }, log: { _ in }, now: { 0 }, emit: { name, args in
                    events.append("\(name) \(args[0])")
                }))
            registry.chords[side] = chord
            registry.add(chord, id: "fade", by: reader)
            func flags(_ code: UInt16, _ mask: UInt, _ time: Double) {
                let event = NSEvent.keyEvent(with: .flagsChanged, location: .zero, modifierFlags: .init(rawValue: mask),
                    timestamp: time, windowNumber: 0, context: nil, characters: "", charactersIgnoringModifiers: "",
                    isARepeat: false, keyCode: code)!
                registry.flags(event)
                registry.flags(event)
            }
            flags(left, leftMask, 0)
            flags(left, 0, 1)
            flags(right, rightMask, 2)
            flags(left, leftMask | rightMask, 3)
            flags(left, rightMask, 4)
            flags(right, 0, 5)
            if events != ["globalshortcut fade", "globalshortcutup fade"] {
                failures.append("\(side) edges: \(events)")
            }
            flags(right, rightMask, 6)
            // After a missed release, the next press starts a new hold.
            flags(right, rightMask, 7)
            registry.release(side, id: "fade", by: reader)
            flags(right, 0, 8)
            if events != Array(repeating: ["globalshortcut fade", "globalshortcutup fade"], count: 3).flatMap({ $0 }) {
                failures.append("\(side) withdrawal: \(events)")
            }
        }
        let registry = HotKeys()
        var events: [String] = []
        func reader(_ session: String) -> GlobalShortcutsModule {
            GlobalShortcutsModule(context: ExactPackageContext(agent: false, session: session, changed: { _ in },
                log: { _ in }, now: { 0 }, emit: { name, args in events.append("\(session) \(name) \(args[0])") }))
        }
        let a = reader("a"), b = reader("b")
        let chord = Chord(number: 1), keys = "Meta+Meta"
        registry.chords[keys] = chord
        registry.add(chord, id: "fade", by: a)
        registry.add(chord, id: "show", by: b)
        registry.press(keys, chord)
        chord.readers.reverse()
        registry.lift(keys, chord)
        registry.press(keys, chord)
        registry.release(keys, id: "show", by: b)
        registry.add(chord, id: "show", by: b)
        registry.lift(keys, chord)
        if events != ["a globalshortcut fade", "a globalshortcutup fade", "b globalshortcut show", "b globalshortcutup show"] {
            failures.append("a lift follows its original reader: \(events)")
        }
        return failures
    }
}

@main
private struct PairTests {
    static func main() {
        let failures = HotKeys.testPairs()
        for failure in failures { print("FAIL: \(failure)") }
        print("modifier pairs: \(failures.count) failures")
        exit(failures.isEmpty ? 0 : 1)
    }
}
