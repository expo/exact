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
extension GroupedListView {
    /// Where a plain list's cells paint on past their frames (`GroupedCell.sideReach`): what the sections stand clear
    /// of at each side, the side safe area the collection reaches over that UIKit does not give it and a fold's half
    /// band. A list of cards stands clear of it, and so does a row at a side where the list's own padding keeps it
    /// clear of the band: the author asked for that room.
    var plainReach: (left: CGFloat, right: CGFloat) {
        guard listAppearance == .plain else { return (0, 0) }
        let band = collection.sideBand
        return (horizontalPadding.left > 0 ? 0 : band.left + foldReach.left, horizontalPadding.right > 0 ? 0 : band.right + foldReach.right)
    }

    /// Places the collection over the sides the list reaches (`reach`): the kernel's reach once it gives one (LLP 1123
    /// D13), else the side safe areas and a fold's half band its edges meet where it stands now (`sideReach`), so a
    /// list mounted out of a window finds its band once it is in one. Called as the list mounts, and as UIKit changes
    /// the collection's safe area or window. The cells' ground follows (`plainReach`), and the sections, their insets
    /// and the rows' lines are laid out again where the band they take changed.
    func placeReach() {
        let fold = foldReach
        if owner.reachGiven { reach = (owner.reach.left, owner.reach.right); foldReach = (0, 0) }
        #if os(iOS)
        if !owner.reachGiven { reach = sideReach() }
        collection.sideSafeArea = reach
        #endif
        var frame = owner.scrollFrame
        if !owner.reachGiven { frame.origin.x -= reach.left; frame.size.width += reach.left + reach.right }
        assign(collection, \.frame, frame)
        let cells = plainReach
        for case let cell as GroupedCell in collection.visibleCells where cell.sideReach != cells { cell.sideReach = cells }
        let band = [collection.sideBand.left, collection.sideBand.right, foldReach.left, foldReach.right]
        if band != placedBand || foldReach != fold {
            placedBand = band
            collection.collectionViewLayout.invalidateLayout()
        }
    }

    #if os(iOS)
    /// A row's lines. Where its author wrote the edge under it, a carried row shows that edge instead of UIKit's line
    /// (LLP 1084 D7), as a hand-built cell hides its separator; a standard cell draws no authored box, so UIKit's line
    /// stays beside one. A custom row's line starts under its content. A plain list's lines run on past the cell over
    /// the side safe area, to the screen's edge (`plainReach`).
    func separator(_ separator: UIListSeparatorConfiguration, at path: IndexPath) -> UIListSeparatorConfiguration {
        var separator = separator
        let rows = section(at: path.section)?.rows ?? []
        func owned(_ i: Int) -> Bool {
            guard i >= 0, i < rows.count else { return false }
            return rows[i].authoredEdge && rows[i].custom || rows[i].nextAuthoredTop && i + 1 < rows.count && rows[i + 1].custom
        }
        if owned(path.item) { separator.bottomSeparatorVisibility = .hidden }
        if owned(path.item - 1) { separator.topSeparatorVisibility = .hidden }
        var aligned = false
        if path.item + 1 < rows.count, rows[path.item].custom, let x = lineStart(rows[path.item].view) {
            separator.bottomSeparatorInsets.leading = x
            aligned = true
        }
        let reach = plainReach
        guard reach != (0, 0) else { return separator }
        let (leading, trailing) = direction == .forceRightToLeft ? (reach.right, reach.left) : (reach.left, reach.right)
        // UIKit's automatic trailing inset stands for the cell's edge in a plain list: the line goes on from there. Its
        // leading end goes on too where it starts at the cell's edge, not under content a custom row aligned it to.
        let automatic = UIListSeparatorConfiguration.automaticInsets.trailing
        for edge in [\UIListSeparatorConfiguration.topSeparatorInsets, \UIListSeparatorConfiguration.bottomSeparatorInsets] where trailing > 0 {
            separator[keyPath: edge].trailing = (separator[keyPath: edge].trailing == automatic ? 0 : separator[keyPath: edge].trailing) - trailing
        }
        if !aligned, separator.bottomSeparatorInsets.leading == 0 { separator.bottomSeparatorInsets.leading -= leading }
        return separator
    }
    #endif
}
#endif
