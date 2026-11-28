#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A search's field never rested on the keys, even after a real tap.
struct SearchFocusFailed: Error, CustomStringConvertible {
    let description: String
}

extension XCTestCase {
    /// Gives `search`'s field the keys and waits until they rest, returning the field that edits: the search bar's
    /// field as it reads at each moment (it may take a new one as the search opens, as on the folded iPhone Duo),
    /// first responder with docked keys, both frames unchanged for half a second. Where keys that come up for a field
    /// focused in code go again, a real tap gives them, as a finger's does: on the field, or, where UIKit shows it
    /// collapsed to a control (the folded Duo's toolbar search), on that control, each only where it takes the touch.
    @discardableResult
    func focusSearch(_ search: UISearchController, within scope: UIView, in window: UIWindow, sampler: KeyboardSampleIOS, touch: TouchRunner?) throws -> UITextField {
        _ = search.searchBar.searchTextField.becomeFirstResponder()
        if let editing = searchRests(search, in: window, sampler: sampler, within: 4) { return editing }
        guard let touch else { throw SearchFocusFailed(description: "the keys did not rest under the search's field focused in code, and no touch runner can tap it") }
        // What a person taps: the field where it shows whole, else the one search control in the search's own
        // navigation that takes the touch, waited for as UIKit lays it out
        var target: UIView?
        let until = Date().addingTimeInterval(2)
        repeat {
            let field = search.searchBar.searchTextField
            target = takesTouch(field, in: window) && field.bounds.width >= 2 * field.bounds.height ? field : searchControl(within: scope, besides: field, named: ["Search", search.searchBar.placeholder ?? "Search"], in: window)
            if target == nil { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        } while target == nil && Date() < until
        guard let target else {
            let field = search.searchBar.searchTextField
            throw SearchFocusFailed(description: "no part of the search takes a touch: its field \(field.convert(field.bounds, to: window)) is collapsed and no one search control shows")
        }
        let shown = target.convert(target.bounds, to: window)
        var tap = try TouchRunner.aim(CGPoint(x: shown.midX, y: shown.midY), in: window)
        tap["op"] = "tap"
        XCTAssertEqual(try touch.ask(tap)["done"] as? Bool, true, "a real tap on the search")
        guard let editing = searchRests(search, in: window, sampler: sampler, within: 15) else {
            throw SearchFocusFailed(description: "the keys did not rest under the search's field after a real tap at \(shown) (field \(search.searchBar.searchTextField.convert(search.searchBar.searchTextField.bounds, to: window)), first responder \(search.searchBar.searchTextField.isFirstResponder), keys \(String(describing: sampler.sample?.frame)))")
        }
        return editing
    }

    /// The search bar's field within `seconds`, once it is first responder in `window` with docked keys and both
    /// stay put for half a second; it is read again on every turn
    private func searchRests(_ search: UISearchController, in window: UIWindow, sampler: KeyboardSampleIOS, within seconds: TimeInterval) -> UITextField? {
        let until = Date().addingTimeInterval(seconds)
        var last: (field: UITextField, keys: CGRect, frame: CGRect)?, since = Date()
        while Date() < until {
            RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02))
            let field = search.searchBar.searchTextField
            guard field.isFirstResponder, field.window === window, let sample = sampler.sample, sample.docked else {
                last = nil
                continue
            }
            let now = (field: field, keys: sample.frame, frame: field.convert(field.bounds, to: window))
            if let last, last.field === now.field, last.keys == now.keys, last.frame == now.frame {
                if Date().timeIntervalSince(since) >= 0.5 { return field }
            } else {
                since = Date()
            }
            last = now
        }
        return nil
    }

    /// Whether a touch at `view`'s centre reaches it or a view inside it
    private func takesTouch(_ view: UIView, in window: UIWindow) -> Bool {
        guard view.window === window, !view.isHidden, view.alpha > 0.01, !view.bounds.isEmpty else { return false }
        let centre = view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: window)
        return window.hitTest(centre, with: nil).map { $0 === view || $0.isDescendant(of: view) } ?? false
    }

    /// The one control in `scope` other than `field` and its ancestors whose accessibility label is one of `names`
    /// (UIKit's "Search", or the search's placeholder) and which takes a touch at its centre; nil for none or several
    private func searchControl(within scope: UIView, besides field: UIView, named names: Set<String>, in window: UIWindow) -> UIView? {
        var found: [UIView] = []
        func walk(_ view: UIView) {
            guard !view.isHidden, view.alpha > 0.01 else { return }
            if view !== field, !field.isDescendant(of: view), names.contains(view.accessibilityLabel ?? ""), takesTouch(view, in: window) {
                found.append(view)
            }
            view.subviews.forEach(walk)
        }
        walk(scope)
        // A control and a view inside it that both say a name are one control
        let outer = found.filter { view in !found.contains { $0 !== view && view.isDescendant(of: $0) } }
        return outer.count == 1 ? outer[0] : nil
    }
}
#endif
