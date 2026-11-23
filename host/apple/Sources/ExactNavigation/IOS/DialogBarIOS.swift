// @ref LLP 1141.004 D6 — a dialog's header is its sheet's navigation bar.
// A hand-built UIKit form sheet is a navigation controller whose bar holds
// the form's title, Cancel leading and the action trailing. A `dialog` that
// is UIKit's sheet (DialogSheetIOS.swift) and whose first child is a
// `header` the shared rule makes a bar of (`HeaderShape`, with the dialog in
// a route's place) shows that bar: the heading is the title, large at level
// 1 as a route's is, the first text after it the subtitle, and the header's
// buttons the items, those before the heading leading and those after it
// trailing, each made and pressed as a route's are (`barItems`). The
// closing button (`command="close"`) is an item as any button that commands
// is, and the platform's look for it is left unsaid.
//
// A sheet whose header is the bar when it opens is presented in a
// navigation controller (`DialogSheetController.sheet`); a header that stops
// being one hides the bar, and one that becomes one after the sheet opened
// stays in the page until it opens again.
//
// The bar shows the header only where it shows all of it. A header that
// also holds a search field, a tablist, a readout, a title drawn from the
// heading's group (an avatar, a line of glyphs, a tap) or anything else the
// bar has no object for stays in the page as written, as does one under the
// agent's own chrome (`--chrome agent`). Each takes no room while the bar
// stands for it (`HostCover.bar`).
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// What keeps the items a bar shows of a header, kept across projections so
/// that one whose kind is unchanged is changed in place: a route's
/// controller, or a dialog's sheet (`DialogBar`).
protocol BarItemStore: AnyObject {
    var barPresses: [BarPress] { get set }
    var barItemsMade: [UInt32: (item: HeaderShape.Item, bar: UIBarButtonItem, press: BarPress, menuFrom: UInt64?)] { get set }
    var barSpaces: [UInt32: UIBarButtonItem] { get set }
    var navigationController: UINavigationController? { get }
    #if EXACT_INSPECT
    var itemNodes: [ObjectIdentifier: UInt32] { get set }
    #endif
}

extension RouteController: BarItemStore {}
#endif

#if os(iOS)
import UIKit
import ExactKit

/// A dialog's sheet bar: the controller whose navigation item it fills,
/// the header it stands for, and its items.
final class DialogBar: BarItemStore {
    weak var controller: UIViewController?
    /// The header the bar stands for, hidden in the page while it does.
    weak var lifted: NodeView?
    var barPresses: [BarPress] = []
    var barItemsMade: [UInt32: (item: HeaderShape.Item, bar: UIBarButtonItem, press: BarPress, menuFrom: UInt64?)] = [:]
    var barSpaces: [UInt32: UIBarButtonItem] = [:]
    #if EXACT_INSPECT
    var itemNodes: [ObjectIdentifier: UInt32] = [:]
    #endif
    /// What was last projected, and that with each item's enabling left out.
    var source: BarSource?, faces: BarSource?
    var subtitle: String?
    /// The tint last given the bar: the dialog's own accent.
    var tint: UIColor?
    let titleInk = NavigationTitleInk()
    var navigationController: UINavigationController? { controller?.navigationController }
    init(_ controller: UIViewController) { self.controller = controller }
}

extension HeaderShape {
    /// `dialog`'s header as its sheet's bar shows it, or nil where the
    /// header stays in the page: the bar must show all of it.
    static func dialogBar(of dialog: NodeView) -> HeaderShape? {
        guard let shape = HeaderShape(route: dialog, back: nil, backIsUIKits: false, projected: true),
              shape.authored.isEmpty, shape.search == nil, shape.segments == nil, shape.hosted.isEmpty,
              shape.group?.drawn != true else { return nil }
        let buttons = shape.leading + shape.trailing
        // A box that paints nothing of its own.
        func plain(_ node: NodeView) -> Bool {
            node.kind == "view" && !node.pressable && !DialogShape.paintsOwnSurface(node) && !node.wrote("box_shadow")
                && !["top", "right", "bottom", "left"].contains { node.wrote("border_style_\($0)") }
        }
        // A button whose face the item draws whole: a native button's, a badge, or one symbol and one text.
        func drawn(_ button: NodeView) -> Bool {
            if button.isNativeButton || BadgeFace(button) != nil { return true }
            var symbols = 0, texts = 0
            func face(_ node: NodeView) -> Bool {
                HeaderShape.Item.shown(node).allSatisfy { child in
                    if child.kind == "image", child.faceSymbol != nil { symbols += 1; return symbols == 1 }
                    if child.isParagraph { texts += 1; return texts == 1 }
                    return plain(child) && face(child)
                }
            }
            return face(button)
        }
        // Every shown node is one the bar places (a button drawn whole), inside one, or a plain box holding only such.
        func whole(_ node: NodeView) -> Bool {
            node.childNodes.allSatisfy { child in
                if child.style["display"]?.string == "none" || child.props["popover"] != nil || child.props["semanticTag"] == "dialog" { return true }
                if shape.placed.contains(where: { $0 === child }) { return !buttons.contains { $0.id == child.id } || drawn(child) }
                return plain(child) && whole(child)
            }
        }
        return whole(shape.header) ? shape : nil
    }
}

extension NavigationHost {
    func wantsDialogBar(_ dialog: NodeView) -> Bool { !ExactEnv.authoredChrome && HeaderShape.dialogBar(of: dialog) != nil }

    /// Project `dialog`'s header onto `controller`'s navigation item, the
    /// root of the navigation controller its sheet presents; false where the
    /// header stays in the page and the bar is hidden.
    func projectDialogBar(_ dialog: NodeView, on controller: UIViewController) -> Bool {
        guard !ExactEnv.authoredChrome, let nav = controller.navigationController, let shape = HeaderShape.dialogBar(of: dialog) else {
            endDialogBar(dialog)
            controller.navigationController?.setNavigationBarHidden(true, animated: false)
            return false
        }
        let kept = dialogBars[dialog.id].flatMap { $0.controller === controller ? $0 : nil }
        let bar = kept ?? DialogBar(controller)
        if kept == nil { titleInk(nav) }
        dialogBars[dialog.id] = bar
        func barSource(_ key: (HeaderShape.Item) -> String) -> BarSource {
            BarSource(bar: .header, authored: [], placed: shape.placed.map(\.id), header: shape.header.id, title: shape.title, level: shape.level,
                      leading: shape.leading.map(key), trailing: shape.trailing.map(key), group: shape.group?.source ?? "", canGoBack: false)
        }
        let item = controller.navigationItem
        let source = barSource(\.source)
        if bar.source != source {
            bar.source = source
            let faces = barSource(\.face)
            // An item that only turned enabled or disabled changes on its kept item.
            if !(bar.faces == faces && enable(shape, in: bar)) {
                bar.faces = faces
                item.title = shape.title
                item.largeTitleDisplayMode = shape.level == 1 ? .always : .never
                bar.barPresses = []
                let leading = barItems(shape.leading, bar), trailing = barItems(Array(shape.trailing.reversed()), bar)
                let ids = Set((shape.leading + shape.trailing).map(\.id))
                bar.barItemsMade = bar.barItemsMade.filter { ids.contains($0.key) }
                bar.barSpaces = bar.barSpaces.filter { ids.contains($0.key) }
                let animated = controller.viewIfLoaded?.window != nil && !ExactEnv.agentFreezes
                if !Self.same(item.leftBarButtonItems, leading) { item.setLeftBarButtonItems(leading, animated: animated) }
                if !Self.same(item.rightBarButtonItems, trailing) { item.setRightBarButtonItems(trailing, animated: animated) }
            }
        }
        if #available(iOS 26.0, *) {
            let subtitle = shape.group?.subtitle
            if subtitle != bar.subtitle {
                if item.subtitle == bar.subtitle { item.subtitle = subtitle }
                bar.subtitle = subtitle
            }
        }
        // The dialog's accent is its bar's tint, as a route's is (`showBar`); nil, the sheet's.
        if bar.tint !== dialog.ownAccent {
            bar.tint = dialog.ownAccent
            nav.navigationBar.tintColor = bar.tint
        }
        bar.titleInk.update(shape.heading?.ownUIColor("text_color"), fonts: NavigationTitleInk.fonts(of: shape.heading, level: shape.level, traits: controller.traitCollection),
                            item: item, bar: nav.navigationBar, transparent: false)
        if bar.lifted !== shape.header {
            if let old = bar.lifted { old.isHidden = false; coverDialogHeader(old.id, nil) }
            shape.header.isHidden = true
            bar.lifted = shape.header
            presenter.controls.syncShown()
        }
        // Again each time: a routeless app's navigation clears its covers at every sync (`reset`).
        coverDialogHeader(shape.header.id, .bar)
        if nav.isNavigationBarHidden { nav.setNavigationBarHidden(false, animated: false) }
        return true
    }

    /// `dialog`'s sheet has gone, or its header is no bar's: the header
    /// shows in the page again, and the item holds nothing of it.
    func endDialogBar(_ dialog: NodeView) {
        guard let bar = dialogBars.removeValue(forKey: dialog.id) else { return }
        if let controller = bar.controller {
            let item = controller.navigationItem
            item.leftBarButtonItems = nil
            item.rightBarButtonItems = nil
            item.title = nil
            if #available(iOS 26.0, *), item.subtitle == bar.subtitle { item.subtitle = nil }
            bar.titleInk.update(nil, item: item, bar: controller.navigationController?.navigationBar, transparent: false)
        }
        guard let lifted = bar.lifted else { return }
        lifted.isHidden = false
        presenter.controls.syncShown()
        coverDialogHeader(lifted.id, nil)
    }

    /// The header a dialog's bar stands for takes no room (`cover`), or
    /// takes its room again, told to the runtime as a lifted route header is
    /// (`coverLifted`).
    private func coverDialogHeader(_ id: UInt32, _ cover: HostCover?) {
        guard covers[id] != cover else { return }
        covers[id] = cover
        countReport()
        presenter.afterBatch { [weak self] in self?.deliverCovers() }
    }

    /// The headers the dialogs' bars stand for: each takes no room.
    var dialogBarCovers: [UInt32: HostCover] {
        var out: [UInt32: HostCover] = [:]
        for bar in dialogBars.values { if let lifted = bar.lifted, presenter.views[lifted.id] === lifted { out[lifted.id] = .bar } }
        return out
    }

    /// The dialog bars on screen, for the agent's aim and taps.
    var shownDialogBars: [DialogBar] {
        dialogBars.values.filter { $0.controller?.viewIfLoaded?.window != nil && $0.navigationController?.isNavigationBarHidden == false }
    }
}
#endif
