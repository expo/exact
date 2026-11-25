#if os(iOS)
import UIKit

extension UITraitCollection {
    /// Whether the system places a vertical bar here (the iPhone Duo):
    /// UIKit 27.1's `verticalBarEdge` other than `.unspecified` (raw 0).
    var placesVerticalBar: Bool { (rawVerticalBarEdge ?? 0) != 0 }

    /// UIKit 27.1's `verticalBarEdge` as the kernel's `VerticalBarEdge` takes it: 0 unspecified,
    /// 1 leading, 2 trailing, the system's preferred edge whether or not the bar shows; `nil`
    /// below iOS 27.1 and for a value past those three.
    var verticalBarEdgeRaw: Int? { rawVerticalBarEdge.flatMap { (0...2).contains($0) ? $0 : nil } }

    /// `verticalBarEdge`'s raw value, read by name: the repo's default Xcode 27.0 has no such
    /// symbol and the same Swift 6.4 as Xcode 27.1, so no `#if` tells the SDKs apart
    /// (ReservedRegions.swift); `nil` below iOS 27.1, which has none.
    private var rawVerticalBarEdge: Int? {
        guard responds(to: NSSelectorFromString("verticalBarEdge")) else { return nil }
        return (value(forKey: "verticalBarEdge") as? NSNumber)?.intValue
    }
}
#endif
