// The kernel's terms for one scroller (LLP 1123 D14, `Kernel::scroll_terms`), each top right bottom
// left, which each host's writer reads: UIKit's (`ScrollInsetsIOS`) and AppKit's (`ScrollInsetsMac`).
import Foundation

/// The kernel's terms for one scroller, each top right bottom left.
package struct ScrollTerms: Equatable {
    package var reach: [CGFloat] = [0, 0, 0, 0]
    package var reserved: [CGFloat?] = [nil, nil, nil, nil]
    package var keyboard: [CGFloat] = [0, 0, 0, 0]
    package var bandReaches: [Bool] = [true, true, true, true]
    package var container: [Bool] = [true, true, true, true]

    package init(reach: [CGFloat] = [0, 0, 0, 0], reserved: [CGFloat?] = [nil, nil, nil, nil],
                 keyboard: [CGFloat] = [0, 0, 0, 0], bandReaches: [Bool] = [true, true, true, true],
                 container: [Bool] = [true, true, true, true]) {
        (self.reach, self.reserved, self.keyboard, self.bandReaches, self.container) = (reach, reserved, keyboard, bandReaches, container)
    }

    /// The terms the runner's `node` reply carries as `scrollTerms`, or `nil` for a box with
    /// none or a reply that does not hold four of each. `reach` is D13's, which reach reads to
    /// set the scroll view's frame, not an inset.
    package init?(reply: [String: Any]) {
        guard let object = reply["scrollTerms"] as? [String: Any] else { return nil }
        let numbers = { (key: String) in (object[key] as? [Any])?.map { ($0 as? NSNumber).map { CGFloat($0.doubleValue) } } }
        guard let reach = numbers("reach"), let reserved = numbers("reserved"), let keyboard = numbers("keyboard"),
              let reaches = object["bandReaches"] as? [Bool], let container = object["container"] as? [Bool],
              [reach.count, reserved.count, keyboard.count, reaches.count, container.count] == [4, 4, 4, 4, 4]
        else { return nil }
        self.init(reach: reach.map { $0 ?? 0 }, reserved: reserved, keyboard: keyboard.map { $0 ?? 0 },
                  bandReaches: reaches, container: container)
    }

    /// D14's container term on each edge: what Exact reserves there, else the scroller's overlap with
    /// its owner's `band` where the band reaches it, and none on an edge it ignores; what a native
    /// layout that clears the band may clear (D15).
    package func container(band: [CGFloat]) -> [CGFloat] {
        (0..<4).map { e in container[e] ? reserved[e] ?? (bandReaches[e] ? band[e] : 0) : 0 }
    }
}
