// A scroll view a native view lends the host (LLP 1123 D15, `ExactNativeSafeArea.ScrollView`): D14's
// writer writes its content and indicator insets as it writes Exact's own scrollers, the view's own
// layout told the container clearance it may take and its answer the writer's consumed term; D15's
// list rows set a lent table's or flow layout's properties; and what the host borrowed is given back
// on release.
#if os(iOS) || os(tvOS)
import ExactCore
import UIKit

package final class LentScrollIOS {
    package let view: UIScrollView
    private let layout: ((UIEdgeInsets) -> UIEdgeInsets)?
    /// The properties as the view had them before the host's first write, while the host holds them.
    private var borrowed: Borrowed?
    /// The list properties the host sets, each as the view had it before the host set it, while the
    /// host holds it.
    private var insetsContent: Bool?, followsReadable: Bool?
    private var sectionReference: UICollectionViewFlowLayout.SectionInsetReference?
    /// The flow layout `sectionReference` is held on, which gets its own back even after the view moves to another.
    private weak var sectionLayout: UICollectionViewFlowLayout?

    private struct Borrowed {
        let content, vertical, horizontal: UIEdgeInsets
        let adjustment: UIScrollView.ContentInsetAdjustmentBehavior
        let adjustsIndicators: Bool

        init(_ v: UIScrollView) {
            (content, vertical, horizontal) = (v.contentInset, v.verticalScrollIndicatorInsets, v.horizontalScrollIndicatorInsets)
            (adjustment, adjustsIndicators) = (v.contentInsetAdjustmentBehavior, v.automaticallyAdjustsScrollIndicatorInsets)
        }
    }

    package init(view: UIScrollView, layout: ((UIEdgeInsets) -> UIEdgeInsets)?) {
        self.view = view
        self.layout = layout
    }

    /// Write D14's insets to the lent view, a scroller of the owner whose view is `owner`, as
    /// `ScrollInsetsIOS.write` writes one, borrowing its properties first: its layout is told the
    /// container clearance it may take on each edge, in its own coordinates, and what it says it
    /// cleared, at least 0 and at most that, is the consumed term. Activation supplies the terms and
    /// rows (stage 5); tests give them.
    package func write(in owner: UIView, terms: ScrollInsetsIOS.Terms, rows: ScrollInsetsIOS.Rows) {
        guard view.window != nil, view.isDescendant(of: owner) else { return }
        if borrowed == nil { borrowed = Borrowed(view) }
        let allowed = terms.container(band: ScrollInsetsIOS.band(of: view, in: owner))
        let said = layout.map { ScrollInsetsIOS.edges($0(ScrollInsetsIOS.edgeInsets(allowed))) } ?? [0, 0, 0, 0]
        let consumed = (0..<4).map { e in said[e].isFinite ? min(max(0, said[e]), allowed[e]) : 0 }
        ScrollInsetsIOS.write(view, in: owner, terms: terms, rows: rows, consumed: consumed)
    }

    /// Set D15's list rows that `style` says, the style of the box whose scroll policy the view takes,
    /// on the table or flow layout the view is: a table's `insetsContentViewsToSafeArea` and
    /// `cellLayoutMarginsFollowReadableWidth`, and a flow layout's `sectionInsetReference` from
    /// `-exact-list-section-insets-reference`. A row the box does not say gives the view's own value
    /// back. Returns the words a flow layout refuses, those it has no reference for, which are as
    /// unsaid: its references are `content-inset`, `safe-area` and `layout-margins`. Activation
    /// supplies the style (stage 5); tests give it.
    @discardableResult
    package func list(_ style: NodeStyle) -> [String] {
        if let table = view as? UITableView {
            Self.hold(&insetsContent, \.insetsContentViewsToSafeArea, of: table,
                      at: style["list_insets_content_to_safe_area"]?.number.map { $0 == 0 })
            Self.hold(&followsReadable, \.cellLayoutMarginsFollowReadableWidth, of: table,
                      at: style["list_cell_margins_follow_readable_width"]?.number.map { $0 != 0 })
        }
        let flow = (view as? UICollectionView)?.collectionViewLayout as? UICollectionViewFlowLayout
        // A layout the module replaced gets its own reference back, and the new one is held afresh
        if sectionLayout !== flow {
            if let kept = sectionReference { sectionLayout?.sectionInsetReference = kept }
            (sectionReference, sectionLayout) = (nil, flow)
        }
        guard let flow else { return [] }
        let word = style["list_section_insets_reference"]?.string
        let reference = word.flatMap(UICollectionViewFlowLayout.SectionInsetReference.init(listInsets:))
        Self.hold(&sectionReference, \.sectionInsetReference, of: flow, at: reference)
        return reference == nil ? word.map { [$0] } ?? [] : []
    }

    /// Set `object`'s property at `path` to `value`, keeping the object's own in `own` the first time,
    /// or give its own back where `value` is nil.
    private static func hold<Object: AnyObject, Value: Equatable>(_ own: inout Value?, _ path: ReferenceWritableKeyPath<Object, Value>,
                                                                 of object: Object, at value: Value?) {
        if let value {
            if own == nil { own = object[keyPath: path] }
            if object[keyPath: path] != value { object[keyPath: path] = value }
        } else if let kept = own {
            own = nil
            object[keyPath: path] = kept
        }
    }

    /// Give back what the host set and borrowed: the list properties, and the five properties as the
    /// view had them before the host's first write.
    package func release() {
        list([:])
        guard let b = borrowed else { return }
        borrowed = nil
        view.contentInsetAdjustmentBehavior = b.adjustment
        view.automaticallyAdjustsScrollIndicatorInsets = b.adjustsIndicators
        view.contentInset = b.content
        view.verticalScrollIndicatorInsets = b.vertical
        view.horizontalScrollIndicatorInsets = b.horizontal
    }
}

extension UICollectionViewFlowLayout.SectionInsetReference {
    /// `-exact-list-section-insets-reference`'s word as a flow layout's reference, nil for a word it
    /// has none for.
    init?(listInsets word: String) {
        switch word {
        case "content-inset": self = .fromContentInset
        case "safe-area": self = .fromSafeArea
        case "layout-margins": self = .fromLayoutMargins
        default: return nil
        }
    }
}
#endif
