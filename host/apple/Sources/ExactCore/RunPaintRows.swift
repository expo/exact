import Foundation

package struct RunPaintRows: Equatable {
    /// Offset x, y and blur in points; nil is `none`.
    package var shadow: [Double]?
    /// The shadow's colour; nil is `currentcolor`.
    package var shadowColor: BatchValue?
    package var strokeWidth: Double = 0
    /// The stroke's colour; nil is `currentcolor`.
    package var strokeColor: BatchValue?

    package init() {}
    /// Whether a colour here is a `light-dark()` pair.
    package var paired: Bool { shadowColor?.isSchemeColor == true || strokeColor?.isSchemeColor == true }

    package mutating func readShadow(_ value: BatchValue) {
        guard case .object(let o) = value, let offset = o["o"]?.numbers, offset.count == 2 else { shadow = nil; return }
        shadow = offset + [max(0, o["b"]?.number ?? 0)]
        shadowColor = o["c"]
    }
}
