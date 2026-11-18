// The Numerals capability (LLP 1077 D15, LLP 1115.003 D3): a paragraph under
// `-exact-content-transition: numeric` is drawn by SwiftUI's `Text` with its
// numeric content transition, per glyph and blurred as iOS's own numbers roll,
// where the core would push its whole raster. An app's composition links this
// module, and SwiftUI with it, only when its plan binds the row.
#if os(iOS) || os(tvOS)
import ExactKit
import ObjectiveC
import SwiftUI
import UIKit

public enum ExactNumerals {
    public static func install() { NumeralLink.installed = NumeralHost() }
}

final class NumeralHost: NumeralCapability {
    private static var slot = 0

    func show(_ node: NodeView, attributed: NSAttributedString, box: CGRect, baseline: CGFloat?, align: Int, mode: String) {
        let roll: NumeralRoll
        if let existing = objc_getAssociatedObject(node, &Self.slot) as? NumeralRoll {
            roll = existing
        } else {
            roll = NumeralRoll()
            objc_setAssociatedObject(node, &Self.slot, roll, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            node.addSubview(roll.view)
        }
        roll.view.frame = box
        roll.show(attributed, baseline: baseline, align: align, mode: mode)
    }

    func hide(_ node: NodeView) {
        guard let roll = objc_getAssociatedObject(node, &Self.slot) as? NumeralRoll else { return }
        roll.view.removeFromSuperview()
        objc_setAssociatedObject(node, &Self.slot, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
    }
}

/// One paragraph's rolling text: what SwiftUI shows, and the last number it
/// showed, which says which way the next one rolls.
final class NumeralModel: ObservableObject {
    @Published var text = AttributedString()
    @Published var down = false
    @Published var version = 0
    @Published var alignment: HorizontalAlignment = .leading
    /// Where the line's baseline is, down from the view's top; nil: the
    /// line is centred down the view.
    @Published var baseline: CGFloat?
    var plain = ""
}

private struct NumeralBody: View {
    @ObservedObject var model: NumeralModel

    var body: some View {
        Text(model.text)
            .lineLimit(1)
            .fixedSize()
            .contentTransition(.numericText(countsDown: model.down))
            // Apple's smooth spring, without bounce; none under Reduce Motion,
            // where the number changes in place.
            .animation(UIAccessibility.isReduceMotionEnabled ? nil : .spring(duration: 0.4, bounce: 0), value: model.version)
            // The baseline where Exact laid the line out, as the backgrounds under it are.
            .alignmentGuide(.top) { $0[.firstTextBaseline] - (model.baseline ?? 0) }
            // The frame keeps the view's size: one that grew to a text a fraction of a
            // point larger would be centred on the view, off the line's start and baseline.
            // A line with a baseline comes in its own box and starts where the box does.
            .frame(minWidth: 0, maxWidth: .infinity, minHeight: 0, maxHeight: .infinity,
                   alignment: model.baseline == nil ? Alignment(horizontal: model.alignment, vertical: .center) : .topLeading)
            // The node is the accessibility element; it reads the value once.
            .accessibilityHidden(true)
    }
}

final class NumeralRoll {
    let model = NumeralModel()
    let view: UIView
    private let host: UIHostingController<NumeralBody>
    private var lastAttributed: NSAttributedString?

    init() {
        host = UIHostingController(rootView: NumeralBody(model: model))
        host.safeAreaRegions = []
        view = host.view
        view.backgroundColor = .clear
        view.isUserInteractionEnabled = false
    }

    func show(_ attributed: NSAttributedString, baseline: CGFloat? = nil, align: Int, mode: String) {
        model.alignment = align == 1 ? .center : align == 2 ? .trailing : .leading
        if model.baseline != baseline { model.baseline = baseline }
        guard lastAttributed?.isEqual(to: attributed) != true else { return }
        lastAttributed = NSAttributedString(attributedString: attributed)
        let plain = attributed.string
        model.text = (try? AttributedString(attributed, including: \.uiKit)) ?? AttributedString(plain)
        // A style change repaints without starting another numeric transition.
        guard plain != model.plain || model.version == 0 else { return }
        // `numeric-countdown` always rolls down; `numeric` rolls the way the
        // number moved, as SwiftUI's own callers say.
        model.down = mode == "numeric-countdown" || leadingNumber(plain) < leadingNumber(model.plain)
        model.plain = plain
        model.version += 1
    }
}
#else
import ExactKit

public enum ExactNumerals {
    /// AppKit draws the paragraph's raster, which does not roll.
    public static func install() {}
}
#endif
