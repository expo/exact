// @ref LLP 1115.002 D4, LLP 1127 D1 — which stacks show a list beside its
// detail, by the runner's rule (`Runner::stack_split`), read by every host
// that splits: the Mac (SplitMac.swift) and iOS (SplitIOS.swift).
import Foundation
import ExactKit
import CExact

/// A stack that shows its root route as a list beside its detail, as the
/// runner finds it (`exact_stack_split`; `Runner::stack_split`), over the
/// kernel's own tree.
struct StackSplit: Equatable {
    /// The list whose row pushes; nil where only `navigationSplit` asked.
    let list: UInt32?
    /// The first visible row the app marks current.
    let current: UInt32?
    /// The list column's width as `navigationSplit` names it.
    let width: Double?
    /// Each route above the root that is a row's detail, by the route's id,
    /// with that row's press.
    let rows: [UInt32: UInt32]
    /// Each route above the root, by its id, with its location.
    let locations: [UInt32: String]
    /// Whether the press of the view asked about writes the router; nil
    /// where none was asked about.
    let pressWrites: Bool?

    init(list: UInt32?, width: Double?, rows: [UInt32: UInt32], locations: [UInt32: String] = [:], current: UInt32? = nil, pressWrites: Bool? = nil) {
        self.current = current
        (self.list, self.width, self.rows, self.locations, self.pressWrites) = (list, width, rows, locations, pressWrites)
    }

    init?(json: Data) {
        guard let o = try? JSONSerialization.jsonObject(with: json) as? [String: Any] else { return nil }
        let pairs = (o["rows"] as? [[Int]] ?? []).filter { $0.count == 2 }.map { (UInt32($0[0]), UInt32($0[1])) }
        let located = (o["locations"] as? [[Any]] ?? []).compactMap { pair -> (UInt32, String)? in
            guard pair.count == 2, let route = pair[0] as? Int, let url = pair[1] as? String else { return nil }
            return (UInt32(route), url)
        }
        self.init(list: (o["list"] as? Int).map { UInt32($0) }, width: (o["width"] as? NSNumber)?.doubleValue,
                  rows: Dictionary(pairs, uniquingKeysWith: { first, _ in first }),
                  locations: Dictionary(located, uniquingKeysWith: { first, _ in first }), current: (o["current"] as? Int).map { UInt32($0) }, pressWrites: o["pressWrites"] as? Bool)
    }
}

extension Runtime {
    /// Whether the stack whose root route is `view` splits, by the runner's
    /// rule; nil for one that pushes, and while the runner is busy.
    /// `retaining` keeps a list that split the stack while it has no row to
    /// act on (a search that matches nothing). `pressed`: a view being
    /// pressed, whose press is said to write the router or not.
    func stackSplit(_ view: UInt32, retaining list: UInt32?, pressed: UInt32? = nil) -> StackSplit? {
        stackSplitAnswer(view, retaining: list, pressed: pressed) ?? nil
    }

    /// `stackSplit`'s answer, or nil while the runner is busy: from a callback
    /// it waits on, no runtime call runs (`Owner.sync`), and a nil there says
    /// nothing about the stack.
    func stackSplitAnswer(_ view: UInt32, retaining list: UInt32?, pressed: UInt32? = nil) -> StackSplit?? {
        on(busy: nil) {
            let len = exact_stack_split(rt, view, list ?? 0, pressed ?? 0)
            return .some(StackSplit(json: Data(bytes: exact_out(rt), count: Int(len))))
        }
    }
}
