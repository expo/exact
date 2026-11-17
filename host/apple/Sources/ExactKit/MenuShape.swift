// @ref LLP 1021 D3 — which popovers an Apple host may present as the
// platform's own menu.

extension NodeView {
    /// Whether this popover is menu-shaped (LLP 1021 D3): `button` rows
    /// (a native one, one whose face fits a menu item, or text with at most
    /// one image), `hr` separators and `role="group"` sections of button
    /// rows, nothing else, so it may present as the platform's menu
    /// (`NSMenu`, `UIMenu`). A popover inside it (a submenu authored in its
    /// parent) is no row of it: closed, it is `display: none` on the web.
    var menuShaped: Bool {
        let rows = menuRows
        func textOnly(_ node: NodeView) -> Bool {
            node.kind == "text" && node.container.subviews.compactMap { $0 as? NodeView }.allSatisfy(textOnly)
        }
        func item(_ row: NodeView) -> Bool {
            guard row.isButton else { return false }
            if row.isNativeButton { return true }
            if let face = row.face, face.fits, !face.raster { return true }
            // Text, and at most one image: the item's title and its image.
            let content = row.container.subviews.compactMap { $0 as? NodeView }
            let images = content.filter { $0.kind == "image" && $0.container.subviews.isEmpty }
            return images.count <= 1 && content.allSatisfy { textOnly($0) || images.contains($0) }
        }
        return rows.contains { !$0.isMenuSection || !$0.menuRows.isEmpty } && rows.allSatisfy { row in
            if row.props["semanticTag"] == "hr" { return true }
            if row.isMenuSection { return row.menuRows.allSatisfy(item) }
            return item(row)
        }
    }

    /// A menu's rows: its children less a context menu's preview and a
    /// popover written inside it; a section's less what titles it (a child
    /// its `aria-labelledby` names, which the web draws as written).
    var menuRows: [NodeView] {
        let titles = isMenuSection ? Set(props["accessibilityLabelledBy"]?.split(separator: " ").map(String.init) ?? []) : []
        return container.subviews.compactMap { $0 as? NodeView }
            .filter { $0.props["contextPreview"] != "true" && $0.props["popover"] == nil && !titles.contains($0.props["id"] ?? "") }
    }

    /// Its rows with each section's in its place.
    var menuRowsThroughSections: [NodeView] { menuRows.flatMap { $0.isMenuSection ? $0.menuRows : [$0] } }

    /// Whether this row of a menu is a section of rows (`role="group"`, as
    /// WAI-ARIA's menu pattern groups items), titled by its accessible name
    /// (LLP 1021 D3, amended 2026-11-12).
    var isMenuSection: Bool { !isButton && props["accessibilityRole"] == "group" }

    /// A section's title: the text its `aria-labelledby` names, else its
    /// `aria-label`; nil where it has neither.
    var menuSectionTitle: String? { authoredLabel }
}
