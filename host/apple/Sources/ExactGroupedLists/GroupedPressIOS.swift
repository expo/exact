import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

extension GroupedCell {
    static let activeRows = ["press_background_color", "press_color", "press_opacity", "press_opacity_to", "press_scale", "press_translate", "press_translate_percent"]

    func restorePressPresentation() {
        ActivePresentation.restore(layer)
    }

    /// Resolve the native state first, then replace only the authored properties.
    func activePaint(_ style: NodeStyle, content: UIListContentConfiguration, enabled: Bool, protected: (title: Bool, secondary: Bool, image: Bool)) {
        refreshActiveOnLayout = ["press_scale", "press_translate", "press_translate_percent"].contains { style[$0] != nil }
        guard Self.activeRows.contains(where: { style[$0] != nil }) else { restorePressPresentation(); return }
        let background = backgroundConfiguration ?? .clear()
        let nativeUpdate = configurationUpdateHandler
        let color = { (value: BatchValue) in UIColor { traits in
            value.channels(dark: traits.userInterfaceStyle == .dark).map(TextEngine.color) ?? .clear
        } }
        configurationUpdateHandler = { cell, state in
            let down = enabled && state.isHighlighted
            nativeUpdate?(cell, state)
            var ground = nativeUpdate == nil ? background.updated(for: state) : cell.backgroundConfiguration ?? .clear()
            var shown = content.updated(for: state)
            if down, let fill = style["press_background_color"] {
                ground.backgroundColor = color(fill)
                ground.backgroundColorTransformer = nil
            }
            if down, let ink = style["press_color"] {
                let ink = color(ink)
                if !protected.title { shown.textProperties.color = ink; shown.textProperties.colorTransformer = nil }
                if !protected.secondary { shown.secondaryTextProperties.color = ink; shown.secondaryTextProperties.colorTransformer = nil }
                if !protected.image {
                    shown.image = shown.image?.withRenderingMode(.alwaysTemplate)
                    shown.imageProperties.tintColor = ink
                    shown.imageProperties.tintColorTransformer = nil
                }
            }
            cell.backgroundConfiguration = ground
            cell.contentConfiguration = shown
            ActivePresentation.apply(style, to: cell.layer, down: down, origin: NodeView.resolveTransformOrigin(style, in: cell.bounds))
        }
        setNeedsUpdateConfiguration()
    }

}

extension GroupedListHost {
    func press(_ node: NodeView, down: Bool) -> Bool {
        #if os(iOS)
        for list in lists.values {
            guard let row = list.rows[node.id] else { continue }
            let cell = list.cell(node.id) as? GroupedCell
            // A custom row with a press of its own shows it, and lets go of a highlight it showed before it took one
            guard GroupedListView.pressIsCells(row, node) else { cell?.forcedPressed = false; return false }
            cell?.forcedPressed = down
            return true
        }
        #endif
        return false
    }

    func cell(carrying node: NodeView) -> UIView? {
        #if os(iOS)
        for list in lists.values {
            guard let row = list.rows[node.id] else { continue }
            return row.custom && GroupedListView.pressIsCells(row, node) ? list.cell(node.id) : nil
        }
        #endif
        return nil
    }

    func select(_ node: NodeView) -> NodeView? {
        #if os(iOS)
        for list in lists.values {
            guard let row = list.rows[node.id] else { continue }
            guard row.custom, GroupedListView.pressIsCells(row, node), list.pressable(node.id),
                  let path = list.source.indexPath(for: node.id) else { return nil }
            list.collection.selectItem(at: path, animated: false, scrollPosition: [])
            return node
        }
        #endif
        return nil
    }

    func ran(_ row: NodeView, on screen: UIViewController?) {
        #if os(iOS)
        guard let list = lists.values.first(where: { $0.rows[row.id] != nil }) else { return }
        if list.sidebar { return presenter.afterBatch { [weak list] in list?.selectDetailRow() } }
        if let screen, let stack = screen.navigationController, stack.viewControllers.contains(screen), stack.topViewController !== screen { return }
        guard let path = list.source.indexPath(for: row.id) else { return }
        list.collection.deselectItem(at: path, animated: !ExactEnv.agentFreezes)
        #endif
    }
}

extension GroupedListView {
    /// Whether row `node`'s press is its cell's highlight: a standard row's
    /// always; a custom row's where the list presses it (a `button` or `link`
    /// row) and its author left the press to the platform (`platformOwnsPress`;
    /// LLP 1084 §4, LLP 1115.000 D16). A custom row with a ground of its own
    /// would cover the highlight, so it dims, as a plain list's row does.
    static func pressIsCells(_ row: GroupedListModel.Row, _ node: NodeView?) -> Bool {
        guard row.custom else { return true }
        guard row.pressable, let node, node.platformOwnsPress else { return false }
        return !node.wrote("background_color") && !node.wrote("background_image")
    }

    func activeSignature(_ id: UInt32) -> BatchValue {
        let style = host.presenter.views[id]?.style ?? [:]
        let owns = pressInkBoundaries(id)
        return .array((GroupedCell.activeRows + ["transform_origin"]).map { style[$0] ?? .null } + [owns.title, owns.secondary, owns.image].map(BatchValue.bool))
    }

    func activePaint(_ cell: GroupedCell, _ id: UInt32, content: UIListContentConfiguration) {
        let row = rows[id]
        cell.activePaint(host.presenter.views[id]?.style ?? [:], content: content,
                         enabled: row?.pressable == true && row?.disabled == false, protected: pressInkBoundaries(id))
    }

    /// The row's colour may be replaced; colours written below it keep their owner.
    private func pressInkBoundaries(_ id: UInt32) -> (title: Bool, secondary: Bool, image: Bool) {
        guard let row = host.presenter.views[id] else { return (false, false, false) }
        if let owns = row.nativeButtonColorOwnership { return owns }
        let symbol = symbolView(of: id)
        var texts: [Bool] = [], image = false
        func walk(_ node: NodeView, protected: Bool) {
            guard node.style["display"]?.string != "none" else { return }
            let owns = protected || node.wrote("text_color")
            if node.kind == "text", node.props["text"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false {
                texts.append(owns)
                return
            }
            if node === symbol { image = owns || node.style["tint_color"] != nil }
            for child in node.container.subviews.compactMap({ $0 as? NodeView }) { walk(child, protected: owns) }
        }
        for child in row.container.subviews.compactMap({ $0 as? NodeView }) { walk(child, protected: false) }
        return (texts.first ?? false, texts.dropFirst().first ?? false, image)
    }
}
#endif
