// LLP 1123 D15: the regions a grouped list's layout, its sections and their headers and footers
// are inset from, and its rows' content margins and background insets, as the list's node and each
// row's say; a list or row that says nothing keeps UIKit's own choice, as a hand-built list does.
import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

extension GroupedCollectionView {
    /// The regions the list's node names for the whole layout, its sections and their boundary
    /// items, in that order; `nil` where it says none.
    var insetsReferences: [UIContentInsetsReference?] {
        let style = listNode?.style
        let rows = ["list_insets_reference", "list_section_insets_reference", "list_supplementary_insets_reference"]
        return rows.map { style?[$0]?.string.flatMap(UIContentInsetsReference.init(listInsets:)) }
    }

    /// Gives `section` the regions its insets and its boundary items' are measured from.
    func referInsets(of section: NSCollectionLayoutSection, _ references: [UIContentInsetsReference?]) {
        if let r = references[1] { section.contentInsetsReference = r }
        if let r = references[2] { section.supplementaryContentInsetsReference = r }
    }

    /// Gives the layout the region its content is inset from, UIKit's default again where the
    /// list stops saying one; whether it changed.
    func referLayoutInsets(_ reference: UIContentInsetsReference?) -> Bool {
        guard let layout = collectionViewLayout as? UICollectionViewCompositionalLayout else { return false }
        let want = reference ?? .automatic
        guard layout.configuration.contentInsetsReference != want else { return false }
        let configuration = layout.configuration
        configuration.contentInsetsReference = want
        layout.configuration = configuration
        return true
    }
}

extension GroupedListView {
    /// The row `id`'s margins and background insets as it says them, which configure its cell again as
    /// they change.
    func listInsetsLook(_ id: UInt32) -> BatchValue {
        let style = host.presenter.views[id]?.style
        let rows = ["list_cell_margins", "list_background_insets"].flatMap { g in ["top", "right", "bottom", "left"].map { "\(g)_\($0)" } }
        return .array(rows.map { style?[$0] ?? .null })
    }
}

extension UIContentInsetsReference {
    /// `-exact-list-insets-reference`'s word as UIKit's region; `content-inset`, a flow layout's
    /// `.fromContentInset`, measures from no region here.
    init?(listInsets word: String) {
        switch word {
        case "automatic": self = .automatic
        case "none", "content-inset": self = .none
        case "safe-area": self = .safeArea
        case "layout-margins": self = .layoutMargins
        case "readable-content": self = .readableContent
        default: return nil
        }
    }
}

extension GroupedCell {
    /// The row `node`'s `-exact-list-cell-margins` and `-exact-list-background-insets`, the start
    /// and end edges resolved against the list's direction (`rtl`); `nil` for one it does not say.
    static func listInsets(of node: NodeView?, rtl: Bool) -> (margins: NSDirectionalEdgeInsets?, background: NSDirectionalEdgeInsets?) {
        func edges(_ group: String) -> NSDirectionalEdgeInsets? {
            let values = ["top", "right", "bottom", "left"].map { node?.style["\(group)_\($0)"]?.number }
            guard values.contains(where: { $0 != nil }) else { return nil }
            let v = values.map { CGFloat($0 ?? 0) }
            return NSDirectionalEdgeInsets(top: v[0], leading: rtl ? v[1] : v[3], bottom: v[2], trailing: rtl ? v[3] : v[1])
        }
        return (edges("list_cell_margins"), edges("list_background_insets"))
    }
}
#endif
