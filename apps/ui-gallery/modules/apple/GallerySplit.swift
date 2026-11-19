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
        sidebar.laidOut = { [weak self] in self?.updateSidebarBarHeight() }
        contents.onSelect = { [weak self] index in
            guard let self else { return }
            selectedFromSidebar = true
            setViewController(stacks[index], for: .secondary)
            sidebar.refreshSelection()
            show(.secondary)
        }
    }

    required init?(coder: NSCoder) { nil }

    /// The sidebar's navigation bar height with its title inline while the split is expanded, else 0: the least
    /// height of the page's range bar, so the two columns' headers end level once both are scrolled. It holds
    /// whether or not the sidebar shows, so hiding the sidebar, showing it or dragging it in moves nothing.
    private(set) var sidebarBarHeight: CGFloat = 0
    /// Called on the main thread when `sidebarBarHeight` changes
    var sidebarBarHeightChanged: (() -> Void)?
    /// The sidebar's inline bar height, and the bar width and vertical size class it was measured at
    private var inlineBarHeight: CGFloat = 0
    private var measuredAt: [CGFloat]?

    /// Sets `sidebarBarHeight`, and says so when it changes
    private func updateSidebarBarHeight() {
        if !isCollapsed, let navigation = sidebar.navigationController {
            let bar = navigation.navigationBar
            // Measure only in a window, where the bar has the height it shows (laid out apart from one it reports
            // UIKit's bare 44 points), and again only where its width or the vertical size class changed: measuring
            // sets the sidebar item's mode twice. A sidebar hidden since launch is first measured as it shows.
            let at = [bar.bounds.width, CGFloat(traitCollection.verticalSizeClass.rawValue)]
            // Note the key first, so a layout the measuring starts does not measure again
            if bar.window != nil, bar.bounds.width > 0, at != measuredAt {
                measuredAt = at
                if let measured = sidebar.measureInlineBarHeight() { inlineBarHeight = measured }
            }
        }
        let height = isCollapsed ? 0 : inlineBarHeight
        if height != sidebarBarHeight {
            sidebarBarHeight = height
            sidebarBarHeightChanged?()
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

    // Update when the split collapses or expands, which need not lay out the sidebar
    func splitViewControllerDidCollapse(_ svc: UISplitViewController) { updateSidebarBarHeight() }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) { updateSidebarBarHeight() }
}

final class GallerySidebar: UITableViewController {
    private let contents: ExactTabContents
    private let groups = [("Text", [0, 1]), ("Color & images", [2, 3, 4, 5]), ("UI & media", [6, 7, 8])]
    var choose: ((Int) -> Void)?
    /// Called after the sidebar lays out, once its bar is in a window: a sidebar hidden since launch first does as it shows
    var laidOut: (() -> Void)?

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

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        laidOut?()
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
