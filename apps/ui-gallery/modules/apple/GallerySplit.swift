#if os(iOS)
import UIKit

final class GallerySplit: UISplitViewController, UISplitViewControllerDelegate {
    private let contents: ExactTabContents
    private let stacks: [UIViewController]
    private let sidebar: GallerySidebar
    private var selectedFromSidebar = false

    init(_ contents: ExactTabContents) {
        self.contents = contents
        stacks = contents.tabs.map { $0.controller }
        sidebar = GallerySidebar(contents)
        super.init(style: .doubleColumn)
        delegate = self
        preferredDisplayMode = .oneBesideSecondary
        setViewController(UINavigationController(rootViewController: sidebar), for: .primary)
        setViewController(stacks[contents.selected], for: .secondary)
        sidebar.choose = { [weak self] index in self?.choose(index) }
        contents.onSelect = { [weak self] index in
            guard let self else { return }
            selectedFromSidebar = true
            setViewController(stacks[index], for: .secondary)
            sidebar.refreshSelection()
            show(.secondary)
        }
    }

    required init?(coder: NSCoder) { nil }

    /// The sidebar's navigation bar height with its title inline while the sidebar shows beside the page, else 0:
    /// the least height of the page's range bar, so the two columns' headers end level once both are scrolled
    private(set) var besideBarHeight: CGFloat = 0
    /// Called on the main thread when `besideBarHeight` changes
    var besideBarHeightChanged: (() -> Void)?
    /// The sidebar's inline bar height, and the bar width and size classes it was measured at
    private var inlineBarHeight: CGFloat = 0
    private var measuredAt: [CGFloat]?

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        updateBesideBarHeight(displayMode)
    }

    /// Sets `besideBarHeight` for the split shown in `mode`, and says so when it changes
    private func updateBesideBarHeight(_ mode: DisplayMode) {
        let beside = !isCollapsed && mode != .secondaryOnly
        // Measure once the bar is laid out in a window, and again only where its width or the size classes changed:
        // measuring sets the sidebar item's mode twice
        if beside, let bar = sidebar.navigationController?.navigationBar, bar.window != nil, bar.bounds.width > 0 {
            let at = [bar.bounds.width, CGFloat(traitCollection.horizontalSizeClass.rawValue), CGFloat(traitCollection.verticalSizeClass.rawValue)]
            if at != measuredAt, let measured = sidebar.measureInlineBarHeight() { inlineBarHeight = measured; measuredAt = at }
        }
        let height = beside ? inlineBarHeight : 0
        if height != besideBarHeight {
            besideBarHeight = height
            besideBarHeightChanged?()
        }
    }

    private func choose(_ index: Int) {
        selectedFromSidebar = true
        // Reveal the current section too: its router selection does not change
        if contents.selected == index {
            contents.select(index)
            show(.secondary)
        } else {
            contents.select(index)
        }
    }

    func splitViewController(_ svc: UISplitViewController,
                             topColumnForCollapsingToProposedTopColumn proposedTopColumn: UISplitViewController.Column) -> UISplitViewController.Column {
        selectedFromSidebar ? .secondary : .primary
    }

    // Hiding or showing the sidebar, collapsing and expanding need not lay out the split's own view
    func splitViewController(_ svc: UISplitViewController, willChangeTo displayMode: UISplitViewController.DisplayMode) {
        updateBesideBarHeight(displayMode)
    }
    func splitViewControllerDidCollapse(_ svc: UISplitViewController) { updateBesideBarHeight(displayMode) }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) { updateBesideBarHeight(displayMode) }
}

final class GallerySidebar: UITableViewController {
    private let contents: ExactTabContents
    private let groups = [("Text", [0, 1]), ("Color & images", [2, 3, 4, 5]), ("UI & media", [6, 7, 8])]
    var choose: ((Int) -> Void)?

    init(_ contents: ExactTabContents) {
        self.contents = contents
        super.init(style: .insetGrouped)
        title = "UI Gallery"
        navigationItem.largeTitleDisplayMode = .always
    }

    required init?(coder: NSCoder) { nil }

    /// The navigation bar's height with the title inline, whichever title it shows now: what the bar says it
    /// fits while this item asks for no large title, the item's mode set back at once without animation. Nil
    /// while the bar shows another item.
    func measureInlineBarHeight() -> CGFloat? {
        guard let bar = navigationController?.navigationBar, bar.topItem === navigationItem else { return nil }
        let mode = navigationItem.largeTitleDisplayMode
        var height: CGFloat = 0
        UIView.performWithoutAnimation {
            navigationItem.largeTitleDisplayMode = .never
            height = bar.sizeThatFits(CGSize(width: bar.bounds.width, height: 0)).height
            navigationItem.largeTitleDisplayMode = mode
        }
        return height
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        navigationController?.navigationBar.prefersLargeTitles = true
        tableView.accessibilityIdentifier = "gallery-sidebar"
        tableView.register(UITableViewCell.self, forCellReuseIdentifier: "section")
        clearsSelectionOnViewWillAppear = false
        refreshSelection()
    }

    func refreshSelection() {
        guard isViewLoaded else { return }
        // Keep the cells so UIKit updates separators with its selection highlight
        for (section, group) in groups.enumerated() {
            if let row = group.1.firstIndex(of: contents.selected) {
                let indexPath = IndexPath(row: row, section: section)
                if tableView.indexPathForSelectedRow != indexPath {
                    tableView.selectRow(at: indexPath, animated: false, scrollPosition: .none)
                }
            }
        }
    }

    override func numberOfSections(in tableView: UITableView) -> Int { groups.count }
    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { groups[section].1.count }
    override func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? { groups[section].0 }

    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let index = groups[indexPath.section].1[indexPath.row]
        let tab = contents.tabs[index]
        let cell = tableView.dequeueReusableCell(withIdentifier: "section", for: indexPath)
        var configuration = cell.defaultContentConfiguration()
        configuration.text = tab.item.title ?? tab.name
        configuration.image = tab.item.image
        cell.contentConfiguration = configuration
        cell.accessoryType = .disclosureIndicator
        cell.accessibilityIdentifier = "sidebar-\(tab.name)"
        return cell
    }

    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        choose?(groups[indexPath.section].1[indexPath.row])
    }
}
#endif
