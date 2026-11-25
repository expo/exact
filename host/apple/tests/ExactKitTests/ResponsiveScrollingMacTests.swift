#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// Responsive scrolling (LLP 1002 D4; LLP 1044 F3): AppKit runs a trackpad
/// gesture on its scrolling thread only for a scroll view whose class, and
/// every scroll view's class around it, is compatible, and whose clip does
/// not override `scrollWheel(with:)`. A gesture's thread needs window-server
/// input to see (`tap … wheel … fling`); this holds the classes AppKit reads.
final class ResponsiveScrollingMacTests: XCTestCase {
    private func scroll(_ id: Int, kind: String = "view", handlers: [String] = [], snap: String? = nil) -> [String: Any] {
        var style: [String: Any] = ["overflow_y": "scroll"]
        if let snap { style["scroll_snap_type"] = snap }
        return ["op": "create", "id": id, "kind": kind, "handlers": handlers, "style": style]
    }

    /// A page in a window: a `scroll` (2), a `list` (3), a `scroll` (4)
    /// holding another (5), a snapping one (6) and one with a drag handler (7).
    private func page() -> (Presenter, NSWindow) {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            scroll(2), scroll(3, kind: "list"), scroll(4), scroll(5),
            scroll(6, snap: "y mandatory"), scroll(7, handlers: ["scrolldragend"]),
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 6, 7]],
            ["op": "roots", "ids": [1]],
        ] + (1...7).map { ["op": "frame", "id": $0, "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0] }))
        return (p, window)
    }

    /// Whether `cls` has a `scrollWheel(with:)` of its own or a subclass's,
    /// rather than `base`'s: what AppKit treats as an override.
    private func overridesScrollWheel(_ cls: AnyClass, base: AnyClass) -> Bool {
        let wheel = #selector(NSResponder.scrollWheel(with:))
        return class_getMethodImplementation(cls, wheel) != class_getMethodImplementation(base, wheel)
    }

    func testEveryScrollerKindInThePageIsCompatibleUpItsWholeChain() throws {
        let (p, window) = page()
        defer { window.close() }
        for (id, what) in [(0, "the page"), (2, "a scroll"), (3, "a list"), (4, "a scroll around another"), (5, "a scroll in a scroll")] {
            let sv: NSScrollView = id == 0 ? p.viewport : try XCTUnwrap(p.views[UInt32(id)]?.scroll, what)
            let around = sequence(first: sv as NSView, next: { $0.superview }).compactMap { $0 as? NSScrollView }
            XCTAssertTrue(around.last === p.viewport, "\(what) is in the page")
            for scroll in around {
                let cls = type(of: scroll)
                XCTAssertTrue(!overridesScrollWheel(cls, base: NSScrollView.self) || cls.isCompatibleWithResponsiveScrolling,
                              "\(what): \(cls) overrides scrollWheel and is not compatible")
            }
            let clip = type(of: sv.contentView)
            XCTAssertFalse(overridesScrollWheel(clip, base: NSClipView.self), "\(what): its clip \(clip) leaves the gesture to AppKit")
            XCTAssertFalse(sv.takesEveryWheelEvent, what)
        }
    }

    func testWhatReadsEachEventKeepsItsOwnScrollerOnTheMainThread() throws {
        let (p, window) = page()
        defer { window.close() }
        for (id, what) in [(6, "a snap eases from the lift and drops AppKit's momentum"), (7, "a drag's end needs its lift")] {
            let sv = try XCTUnwrap(p.views[UInt32(id)]?.scroll)
            XCTAssertTrue(sv.takesEveryWheelEvent, what)
            let clip = type(of: sv.contentView)
            XCTAssertTrue(overridesScrollWheel(clip, base: NSClipView.self) && !clip.isCompatibleWithResponsiveScrolling, "AppKit reads the clip's override")
            XCTAssertTrue(sv.contentView is RangeClipView, "it keeps the clamp to the scroll range")
        }
    }
}
#endif
