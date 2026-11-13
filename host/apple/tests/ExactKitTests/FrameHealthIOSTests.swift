#if os(iOS)
import UIKit
import QuartzCore
import XCTest
import ExactCore
@testable import ExactKit

/// Runs `change` at each display link callback, counting them.
private final class Ticker: NSObject {
    private let change: () -> Void
    private(set) var ticks = 0
    init(_ change: @escaping () -> Void) { self.change = change }
    @objc func tick() { ticks += 1; change() }
}

final class FrameHealthIOSTests: XCTestCase {
    func testTheCardOffersAnAccessibleMinimizeAction() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the frame-health card's minimize control")
        }
        let health = FrameHealth(policy: .onDemand)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        let window = hosted(overlay, in: scene, size: .zero)
        overlay.frame = window.bounds
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        overlay.layoutIfNeeded()
        let elements = overlay.card.accessibilityElements?.compactMap { $0 as? UIAccessibilityElement } ?? []
        let minimize = try XCTUnwrap(elements.first { $0.isAccessibilityElement && $0.accessibilityLabel == "Minimize frame health" })
        let full = overlay.card.frame
        let generation = health.snapshot().generation, collector = health.handle
        XCTAssertTrue(minimize.accessibilityActivate())
        overlay.layoutIfNeeded()
        XCTAssertLessThan(overlay.card.frame.width, full.width)
        XCTAssertLessThan(overlay.card.frame.height, full.height / 4)
        XCTAssertTrue(overlay.desiredVisible && health.hudVisible && health.hudEffectiveVisible)
        XCTAssertTrue(health.enabled, "a minimized card keeps on-demand collection active")
        let start = CACurrentMediaTime()
        for i in 0...60 {
            let time = start + Double(i) / 120
            health.record(arrival: time, target: time + 1 / 120, period: 1 / 120, finished: time + 0.001)
        }
        overlay.refresh(now: start + 0.501)
        XCTAssertEqual(overlay.card.state.rows[0].1, "120", "the minimized status takes new samples")
        XCTAssertEqual(health.handle, collector)
        XCTAssertEqual(health.snapshot().generation, generation, "minimizing does not start a new measurement")
        XCTAssertEqual(health.snapshot().callbacks, 61)
        XCTAssertGreaterThan(overlay.card.chart.latest, start, "the charts retain frames while minimized")
        XCTAssertNil(overlay.card.chart.clock, "hidden plots need no redraw timer")
        let restore = try XCTUnwrap(elements.first { $0.isAccessibilityElement && $0.accessibilityLabel == "Restore frame health" })
        XCTAssertEqual(elements.filter(\.isAccessibilityElement).count, 1, "only the reduced control is exposed")
        XCTAssertTrue(restore.accessibilityValue?.contains("120 FPS") == true)
        XCTAssertTrue(restore.accessibilityActivate())
        overlay.layoutIfNeeded()
        XCTAssertEqual(overlay.card.frame, full)
        XCTAssertEqual(health.snapshot().callbacks, 61)
        // Details and the user's pause survive a minimize/restore cycle too.
        overlay.card.activate?(.details)
        overlay.card.activate?(.status)
        XCTAssertTrue(minimize.accessibilityActivate())
        XCTAssertTrue(restore.accessibilityActivate())
        XCTAssertTrue(overlay.card.state.showsDetails)
        XCTAssertFalse(overlay.card.detailsScroll.isHidden)
        XCTAssertEqual(overlay.card.state.status, .paused)
    }

    func testTheCompactCardFitsAndRestoresAboveTheKeyboardWithoutLosingSamples() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the compact frame-health card above the keyboard")
        }
        let health = FrameHealth(policy: .onDemand)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        let window = hosted(overlay, in: scene, size: CGSize(width: 393, height: 216))
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        overlay.layoutIfNeeded()
        let card = overlay.card
        XCTAssertFalse(card.isHidden, "the original 176-point compact face fits")
        XCTAssertTrue(card.compact)
        XCTAssertEqual(card.bounds.height, FrameHealthCardFace.compactHeight)
        XCTAssertTrue(card.bounds.contains(try XCTUnwrap(card.minimizeElement).accessibilityFrameInContainerSpace))
        XCTAssertFalse(card.layout.pacingArea.intersects(card.layout.minimize))
        let start = CACurrentMediaTime(), generation = health.snapshot().generation
        func sample(_ first: Int, through last: Int) {
            for i in first...last {
                let time = start + Double(i) / 120
                health.record(arrival: time, target: time + 1 / 120, period: 1 / 120, finished: time + 0.001)
            }
            overlay.refresh(now: start + Double(last) / 120 + 0.001)
        }
        sample(0, through: 60)
        let history = card.chart.latest
        overlay.frame.size.height = 300
        NotificationCenter.default.post(name: UIResponder.keyboardWillChangeFrameNotification, object: nil,
            userInfo: [UIResponder.keyboardFrameEndUserInfoKey: window.convert(CGRect(x: 0, y: 200, width: 600, height: 300), to: nil)])
        overlay.layoutIfNeeded()
        XCTAssertFalse(card.isHidden, "184 points above the keyboard still fit the compact face")
        XCTAssertTrue(health.enabled && health.hudEffectiveVisible)
        XCTAssertEqual(card.chart.latest, history, "fitting changes keep the chart's history")
        sample(61, through: 120)
        XCTAssertEqual(card.state.rows[0].1, "120")
        XCTAssertTrue(card.minimizeElement?.accessibilityActivate() == true)
        XCTAssertTrue(card.minimized)
        sample(121, through: 180)
        XCTAssertTrue(card.minimizeElement?.accessibilityActivate() == true)
        XCTAssertFalse(card.minimized, "the compact card restores above the keyboard")
        XCTAssertFalse(card.isHidden)
        XCTAssertTrue(overlay.bounds.insetBy(dx: 8, dy: 8).contains(card.frame))
        XCTAssertLessThanOrEqual(card.frame.maxY, 192)
        XCTAssertEqual(card.state.rows[0].1, "120")
        XCTAssertGreaterThan(card.chart.latest, history)
        XCTAssertEqual(health.snapshot().callbacks, 181)
        XCTAssertEqual(health.snapshot().generation, generation)
    }

    private final class CardTouch: UITouch {
        var point: CGPoint
        var time: TimeInterval = 1
        init(_ point: CGPoint) { self.point = point; super.init() }
        override var timestamp: TimeInterval { time }
        override func location(in view: UIView?) -> CGPoint { view.map { $0.convert(point, from: nil) } ?? point }
    }

    func testTheMinimizedCardTakesOnlyItsBoundsAndDragsWithoutRestoring() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "touches on and beside the minimized frame-health card")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        let window = hosted(overlay, in: scene, size: .zero)
        overlay.frame = window.bounds
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        overlay.layoutIfNeeded()
        let card = overlay.card
        func tap(_ point: CGPoint) {
            let touch = CardTouch(card.convert(point, to: nil))
            card.touchesBegan([touch], with: nil)
            card.touchesEnded([touch], with: nil)
            overlay.layoutIfNeeded()
        }
        let full = card.frame
        let minimize = card.layout.minimize
        tap(CGPoint(x: minimize.midX * card.scale, y: minimize.midY * card.scale))
        XCTAssertTrue(card.minimized, "the visible Minimize control takes a tap")
        let bar = scene.statusBarManager?.statusBarFrame ?? .zero
        XCTAssertEqual(card.frame.origin, CGPoint(x: full.minX, y: max(full.minY, overlay.convert(bar, from: nil).maxY)),
            "at the same edge, below the status bar")
        let free = CGPoint(x: full.midX, y: full.maxY - 60)
        let button = UIButton(type: .system)
        button.frame = CGRect(x: free.x - 40, y: free.y - 22, width: 80, height: 44)
        var pressed = 0
        button.addAction(UIAction { _ in pressed += 1 }, for: .touchUpInside)
        let root = try XCTUnwrap(window.rootViewController?.view)
        root.insertSubview(button, belowSubview: overlay)
        XCTAssertTrue(window.hitTest(free, with: nil) === button, "the released area routes touches to the app")
        (window.hitTest(free, with: nil) as? UIControl)?.sendActions(for: .touchUpInside)
        XCTAssertEqual(pressed, 1)
        let center = CGPoint(x: card.bounds.midX, y: card.bounds.midY)
        XCTAssertTrue(window.hitTest(card.convert(center, to: window), with: nil) === card)
        let touch = CardTouch(card.convert(center, to: nil))
        card.touchesBegan([touch], with: nil)
        touch.point = overlay.convert(CGPoint(x: overlay.bounds.maxX - 8 - card.frame.width / 2,
            y: overlay.bounds.maxY - 8 - card.frame.height / 2), to: nil)
        touch.time += 1
        card.touchesMoved([touch], with: nil)
        touch.time += 0.1
        card.touchesEnded([touch], with: nil)
        waitFor("the minimized drag settles") { !overlay.isSettling }
        overlay.layoutIfNeeded()
        XCTAssertTrue(card.minimized, "a drag is not a restore tap")
        XCTAssertEqual(card.frame.maxX, overlay.bounds.maxX - 8, accuracy: 0.5)
        XCTAssertEqual(card.frame.maxY, overlay.bounds.maxY - 8, accuracy: 0.5)
        let restore = try XCTUnwrap(card.minimizeElement)
        XCTAssertTrue(card.bounds.contains(restore.accessibilityFrameInContainerSpace))
        let move = try XCTUnwrap(restore.accessibilityCustomActions?.first { $0.name == "Move up" })
        XCTAssertTrue(move.actionHandler?(move) == true)
        waitFor("the accessibility move settles") { !overlay.isSettling }
        tap(center)
        XCTAssertFalse(card.minimized)
        XCTAssertEqual(card.frame.size, full.size)
        XCTAssertEqual(card.frame.maxX, overlay.bounds.maxX - 8, accuracy: 0.5)
        XCTAssertEqual(card.frame.minY, 8, accuracy: 0.5)
        // A shorter window uses the compact face and still offers a visible minimize control.
        overlay.frame.size = CGSize(width: full.height + 100, height: 300)
        overlay.layoutIfNeeded()
        XCTAssertTrue(card.compact)
        XCTAssertTrue(card.bounds.contains(try XCTUnwrap(card.minimizeElement).accessibilityFrameInContainerSpace))
        card.activate?(.minimize)
        overlay.layoutIfNeeded()
        XCTAssertTrue(card.minimized)
        XCTAssertTrue(card.accessibilityMove?(CGVector(dx: 0, dy: 1)) == true)
        waitFor("the card rests above the keyboard's future position") { !overlay.isSettling }
        NotificationCenter.default.post(name: UIResponder.keyboardWillChangeFrameNotification, object: nil,
            userInfo: [UIResponder.keyboardFrameEndUserInfoKey: window.convert(CGRect(x: 0, y: 200, width: 600, height: 300), to: nil)])
        overlay.layoutIfNeeded()
        XCTAssertEqual(card.frame.maxY, 192, accuracy: 0.5)
        card.activate?(.minimize)
        XCTAssertFalse(card.minimized, "the compact card restores in 184 points above the keyboard")
        XCTAssertFalse(card.isHidden)
        XCTAssertTrue(health.hudEffectiveVisible)
        card.activate?(.minimize)
        NotificationCenter.default.post(name: UIResponder.keyboardWillChangeFrameNotification, object: nil,
            userInfo: [UIResponder.keyboardFrameEndUserInfoKey: window.convert(CGRect(x: 0, y: 150, width: 600, height: 300), to: nil)])
        overlay.layoutIfNeeded()
        card.activate?(.minimize)
        XCTAssertTrue(card.minimized && !card.isHidden, "less than 176 points keeps the status available")
        let attachment = XCTAttachment(image: UIGraphicsImageRenderer(bounds: card.bounds).image { context in
            card.drawNow()
            card.layer.render(in: context.cgContext)
        })
        attachment.name = "Frame health — minimized"
        attachment.lifetime = .keepAlways
        add(attachment)
        NotificationCenter.default.post(name: UIResponder.keyboardWillChangeFrameNotification, object: nil,
            userInfo: [UIResponder.keyboardFrameEndUserInfoKey: window.convert(CGRect(x: 0, y: 1000, width: 600, height: 300), to: nil)])
        overlay.layoutIfNeeded()
        card.activate?(.minimize)
        XCTAssertFalse(card.minimized, "with the keyboard dismissed, the compact card restores")
    }

    func testTheMinimizedCardKeepsBelowTheStatusBar() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the minimized frame-health card beside the status bar")
        }
        let bar = try XCTUnwrap(scene.statusBarManager?.statusBarFrame)
        guard bar.height > 0 else { throw XCTSkip("this host shows no status bar") }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        let window = hosted(overlay, in: scene, size: .zero)
        overlay.frame = window.bounds
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        overlay.layoutIfNeeded()
        let card = overlay.card
        XCTAssertEqual(card.frame.minY, 8, accuracy: 0.5, "the whole card rests past the safe area")
        // Touches within the status bar's frame reach the system, not the app: all one control, the minimized card
        // rests below it, and restored, the card returns to its edge
        card.activate?(.minimize)
        overlay.layoutIfNeeded()
        XCTAssertGreaterThanOrEqual(card.frame.minY, overlay.convert(bar, from: nil).maxY)
        XCTAssertEqual(card.frame.minX, 8, accuracy: 0.5)
        card.activate?(.minimize)
        overlay.layoutIfNeeded()
        XCTAssertEqual(card.frame.minY, 8, accuracy: 0.5)
    }

    func testTheMinimizedCardKeepsItsCornerThroughRotation() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the minimized frame-health card rotating with its window")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        let window = hosted(overlay, in: scene, size: .zero)
        overlay.frame = window.bounds
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        overlay.layoutIfNeeded()
        overlay.card.activate?(.minimize)
        XCTAssertTrue(overlay.card.accessibilityMove?(CGVector(dx: 1, dy: 1)) == true)
        waitFor("the card reaches the bottom-right corner") { !overlay.isSettling }
        let original = overlay.bounds.size
        // The test host supports portrait only; exercise the bounds change delivered by rotation.
        for size in [CGSize(width: original.height, height: original.width), original] {
            overlay.frame.size = size
            overlay.layoutIfNeeded()
            XCTAssertTrue(overlay.card.minimized)
            XCTAssertTrue(overlay.bounds.contains(overlay.card.frame))
            XCTAssertEqual(overlay.card.frame.maxX, overlay.bounds.maxX - 8, accuracy: 0.5)
            XCTAssertEqual(overlay.card.frame.maxY, overlay.bounds.maxY - 8, accuracy: 0.5)
        }
        overlay.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        overlay.setNeedsLayout()
        overlay.layoutIfNeeded()
        XCTAssertGreaterThan(overlay.card.scale, 1)
        XCTAssertTrue(overlay.bounds.contains(overlay.card.frame))
    }

    /// A window for an overlay: the card reads only while it can be seen.
    private func hosted(_ overlay: FrameHealthOverlay, in scene: UIWindowScene, size: CGSize) -> UIWindow {
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        overlay.frame = CGRect(origin: .zero, size: size)
        window.rootViewController?.view.addSubview(overlay)
        addTeardownBlock { window.isHidden = true }
        return window
    }

    func testPresentedMenuAndCardKeepAppGeometryAndSupportAccessibilityType() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the developer sheet and frame-health card in a real host window")
        }
        let session = ExactApp.shared.makeSession(label: "frame-health-ui")
        let window = UIWindow(windowScene: scene)
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        let app = ExactView(session: session)
        root.view.addSubview(app)
        app.frame = root.view.bounds
        app.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        XCTAssertNil(session.boot(size: app.bounds.size).error)
        app.fit(); window.layoutIfNeeded()
        defer {
            root.dismiss(animated: false)
            window.isHidden = true
            session.destroy()
        }
        DevMenu.install(on: window, session: session, controller: root, planPath: nil, frameHealthContainer: root.view)
        let before = app.frame
        let overlay = try XCTUnwrap(DevMenu.frameHealthOverlay())
        XCTAssertTrue(overlay.isHidden)
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        // In a window of its own above the app's, never key, so it stays above whatever the app presents later
        let cardWindow = try XCTUnwrap(overlay.window as? FrameHealthWindow)
        XCTAssertTrue(cardWindow.windowScene === window.windowScene)
        XCTAssertGreaterThan(cardWindow.windowLevel, window.windowLevel)
        XCTAssertFalse(cardWindow.isKeyWindow)
        XCTAssertTrue(window.isKeyWindow, "the app's window keeps the keyboard and key commands")
        XCTAssertFalse(root.view.isDescendant(of: cardWindow))
        XCTAssertTrue(root.view.subviews.first === app)
        let open = overlay.card.convert(CGPoint(x: 160, y: 100), to: cardWindow)
        XCTAssertTrue(cardWindow.hitTest(open, with: nil)?.isDescendant(of: overlay) == true, "the card takes the touches on it")
        let cardFrame = overlay.card.convert(overlay.card.bounds, to: cardWindow)
        let beside = CGPoint(x: cardWindow.bounds.midX, y: cardFrame.midY < cardWindow.bounds.midY ? cardWindow.bounds.maxY - 4 : 4)
        XCTAssertNil(cardWindow.hitTest(beside, with: nil), "and touches beside it pass to the app's window")
        XCTAssertEqual(cardFrame.origin, CGPoint(x: 8, y: 8), "the card rests 8 points from the window's edges, past its safe area")
        // The app's appearance, which overrides the system's on the app's window, reaches the card's window
        let style = window.overrideUserInterfaceStyle
        for scheme in [UIUserInterfaceStyle.dark, .light] {
            window.overrideUserInterfaceStyle = scheme
            waitFor("the card takes the app's appearance") { cardWindow.traitCollection.userInterfaceStyle == scheme }
        }
        window.overrideUserInterfaceStyle = style
        XCTAssertEqual(app.frame, before, "host controls cannot resize the app")

        // The app's window and the card's above it, as the screen shows them
        func screen() -> UIImage {
            UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
                window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
                cardWindow.drawHierarchy(in: cardWindow.frame, afterScreenUpdates: true)
            }
        }

        let navigation = DevMenuController.navigation()
        let shown = expectation(description: "native developer sheet presented")
        root.present(navigation, animated: false) { shown.fulfill() }
        wait(for: [shown], timeout: 5)
        // Attached to the bottom edge in regular height, where a sheet can rest at the medium detent
        if window.traitCollection.horizontalSizeClass == .compact && window.traitCollection.verticalSizeClass == .regular,
           let sheet = navigation.sheetPresentationController {
            XCTAssertGreaterThan(navigation.view.frame.height, window.bounds.height * 0.8, "the sheet opens at full height, not half")
            sheet.selectedDetentIdentifier = .medium
            // UIKit moves the sheet at a later layout pass
            waitFor("the sheet rests at half height") { navigation.view.frame.height < window.bounds.height * 0.7 }
            sheet.selectedDetentIdentifier = .large
        }
        let menu = try XCTUnwrap(navigation.viewControllers.first as? DevMenuController)
        menu.tableView.layoutIfNeeded()
        XCTAssertEqual(menu.groups[1].first?.title, "Frame Health")
        XCTAssertEqual(menu.groups[1].first?.checked, true, "the shown card's row is checked")
        XCTAssertEqual(menu.tableView.cellForRow(at: IndexPath(row: 0, section: 1))?.accessoryType, .checkmark)
        // The card stays above the menu, so it can close itself there: the row follows
        overlay.setVisible(false)
        waitFor("closing the card unchecks its row") { menu.groups[1].first?.checked == false }
        overlay.setVisible(true)
        waitFor("showing it checks the row again") { menu.groups[1].first?.checked == true }
        XCTAssertFalse(menu.groups[1].contains { $0.title.contains("Details") || $0.title.contains("Share") }, "the card offers these itself")
        let light = XCTAttachment(image: screen())
        light.name = "Developer menu — native sheet, under the card"
        light.lifetime = .keepAlways
        add(light)
        // A sheet opened after the card stays below it
        let cardPoint = overlay.card.convert(CGPoint(x: 160, y: 100), to: cardWindow)
        XCTAssertTrue(cardWindow.hitTest(cardPoint, with: nil)?.isDescendant(of: overlay) == true,
            "the card stays above a sheet the app presents after it")
        XCTAssertTrue(DevMenu.frameHealthOverlay() === overlay, "the menu's row finds the same card")
        let observer = overlay.screen, frames = overlay.screenFrames
        overlay.card.activate?(.minimize)
        overlay.layoutIfNeeded()
        XCTAssertTrue(overlay.card.minimized)
        XCTAssertTrue(overlay.screen === observer && overlay.observing, "minimizing keeps the screen observer")
        XCTAssertEqual(menu.groups[1].first?.checked, true, "a minimized card is still shown")
        XCTAssertNil(cardWindow.hitTest(cardPoint, with: nil), "the sheet receives the area the card released")
        let sheetPoint = navigation.view.convert(CGPoint(x: navigation.view.bounds.midX, y: 120), to: window)
        XCTAssertTrue(window.hitTest(sheetPoint, with: nil)?.isDescendant(of: navigation.view) == true)
        menu.tableView.reloadData()
        waitFor("the screen keeps observing the app's sheet") { overlay.screenFrames > frames }
        let minimized = XCTAttachment(image: screen())
        minimized.name = "Frame health — minimized above a sheet"
        minimized.lifetime = .keepAlways
        add(minimized)
        XCTAssertTrue(overlay.card.minimizeElement?.accessibilityActivate() == true)
        overlay.layoutIfNeeded()
        XCTAssertTrue(cardWindow.hitTest(cardPoint, with: nil)?.isDescendant(of: overlay) == true)

        navigation.setOverrideTraitCollection(UITraitCollection(preferredContentSizeCategory: .accessibilityExtraExtraExtraLarge), forChild: menu)
        navigation.overrideUserInterfaceStyle = .dark
        menu.tableView.reloadData()
        menu.view.layoutIfNeeded()
        let row = IndexPath(row: 0, section: 0)
        let cell = try XCTUnwrap(menu.tableView.cellForRow(at: row))
        XCTAssertGreaterThan(cell.bounds.height, 56)
        XCTAssertEqual((cell.contentConfiguration as? UIListContentConfiguration)?.textProperties.numberOfLines, 0)
        // Build Information's text clears the sheet's side insets, as the menu's rows do: on the iPhone Duo the sheet's
        // bar items stand in a strip at its side
        menu.tableView(menu.tableView, didSelectRowAt: IndexPath(row: 0, section: 2))
        let pushed = expectation(description: "Build Information pushed")
        if let transition = navigation.transitionCoordinator {
            transition.animate(alongsideTransition: nil) { _ in pushed.fulfill() }
        } else {
            pushed.fulfill()
        }
        wait(for: [pushed], timeout: 5)
        let information = try XCTUnwrap(navigation.topViewController?.view as? UITextView)
        navigation.view.layoutIfNeeded()
        let whole = try XCTUnwrap(information.textRange(from: information.beginningOfDocument, to: information.endOfDocument))
        let text = information.selectionRects(for: whole).map(\.rect).filter { $0.width > 0 }.reduce(CGRect.null) { $0.union($1) }
        let safe = information.safeAreaInsets
        XCTAssertGreaterThanOrEqual(text.minX, safe.left + 20 - 0.5, "the text starts clear of the left inset (\(safe))")
        XCTAssertLessThanOrEqual(text.maxX, information.bounds.width - safe.right - 20 + 0.5, "and ends clear of the right one (\(safe))")
        let buildInformation = XCTAttachment(image: UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        })
        buildInformation.name = "Developer menu — Build Information"
        buildInformation.lifetime = .keepAlways
        add(buildInformation)
        let dismissed = expectation(description: "native developer sheet dismissed")
        root.dismiss(animated: false) { dismissed.fulfill() }
        wait(for: [dismissed], timeout: 5)
        // Details takes the place of the card's charts at the same size, and Share asks what to share,
        // then opens the share sheet with that one item. The card presents from its own window's root, over itself
        let cardRoot = try XCTUnwrap(cardWindow.rootViewController)
        func presented<Controller: UIViewController>(_: Controller.Type) throws -> Controller {
            wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in cardRoot.presentedViewController is Controller }, object: nil)], timeout: 10)
            return try XCTUnwrap(cardRoot.presentedViewController as? Controller)
        }
        func close() {
            let closed = expectation(description: "presentation dismissed")
            cardRoot.dismiss(animated: false) { closed.fulfill() }
            wait(for: [closed], timeout: 5)
        }
        let size = overlay.card.frame.size
        overlay.card.activate?(.details)
        XCTAssertTrue(overlay.card.state.showsDetails)
        XCTAssertFalse(overlay.card.detailsScroll.isHidden)
        XCTAssertEqual(overlay.card.frame.size, size, "the card keeps its size")
        XCTAssertEqual(overlay.card.presentation.details.map(\.name), ["Cadence", "Target rate", "Content latency", "Frames",
            "Requested slots", "Missed frames", "Skipped refreshes", "Late frames", "Limited by the system"])
        XCTAssertNil(cardRoot.presentedViewController, "and opens no sheet")
        if #available(iOS 26.0, *) { XCTAssertTrue(overlay.card.detailsScroll.topEdgeEffect.isHidden, "no edge effect blends the rows") }
        XCTAssertEqual(overlay.card.detailsScroll.contentInsetAdjustmentBehavior, .never)
        // Details draws as the face does, a picture its update commits, timed as the card's work, and only when it
        // changes
        let rows = try XCTUnwrap(overlay.card.detailsScroll.subviews.first { $0 is FrameHealthCardView.DetailsView })
        XCTAssertTrue(rows.layer is FrameHealthCardView.Pictured.Picture)
        rows.layer.displayIfNeeded()
        XCTAssertNotNil(rows.layer.contents)
        // The picture is opaque, so it fills its own ground: the card's, not black
        let picture = try XCTUnwrap(rows.layer.contents.map { $0 as! CGImage })
        var corner = [UInt8](repeating: 0, count: 4)
        let probe = try XCTUnwrap(CGContext(data: &corner, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        probe.draw(picture, in: CGRect(x: -2, y: 2 - CGFloat(picture.height) + 1, width: CGFloat(picture.width), height: CGFloat(picture.height)))
        var ground = [UInt8](repeating: 0, count: 4)
        let fill = try XCTUnwrap(CGContext(data: &ground, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        fill.setFillColor(try XCTUnwrap(overlay.card.palette).background)
        fill.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
        XCTAssertEqual(corner, ground, "Details' ground")
        let timing = overlay.card.drawing
        var drawn = 0
        overlay.card.drawing = { drawn += 1; $0() }
        overlay.card.update(overlay.card.presentation)
        rows.layer.displayIfNeeded()
        XCTAssertEqual(drawn, 0, "the same rows draw nothing")
        rows.layer.setNeedsDisplay()
        rows.layer.displayIfNeeded()
        XCTAssertEqual(drawn, 1)
        overlay.card.drawing = timing
        let details = XCTAttachment(image: screen())
        details.name = "Frame health — Details"
        details.lifetime = .keepAlways
        add(details)
        overlay.card.activate?(.details)
        XCTAssertTrue(overlay.card.detailsScroll.isHidden, "a second tap returns the charts")
        overlay.card.activate?(.share)
        let choices = try presented(UIAlertController.self)
        // Measuring the screen, Share gives Exact's frames as an agent report of their own too
        XCTAssertEqual(choices.actions.map(\.title), overlay.observing
            ? ["Agent Report: Screen", "Agent Report: Exact's Frames", "Summary", "Image", "Cancel"]
            : ["Agent Report", "Summary", "Image", "Cancel"])
        if #available(iOS 18.0, *) { XCTAssertTrue(overlay.observing) }
        close()
        overlay.share(.image)
        // On a phone the sheet rises from the bottom edge, and UIKit replaces the popover's source
        XCTAssertEqual(try presented(UIActivityViewController.self).excludedActivityTypes, [.assignToContact])
        close()
        // The title opens the definitions, over the card
        overlay.card.activate?(.title)
        XCTAssertTrue(try presented(UINavigationController.self).viewControllers.first is FrameHealthDefinitionsController)
        close()
        root.overrideUserInterfaceStyle = .dark
        overlay.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        overlay.setNeedsLayout(); overlay.layoutIfNeeded()
        XCTAssertTrue(overlay.bounds.contains(overlay.card.frame))
        let card = XCTAttachment(image: screen())
        card.name = "Frame health — native card"
        card.lifetime = .keepAlways
        add(card)
        XCTAssertEqual(app.frame, before)
        overlay.hideReadout()
        XCTAssertFalse(overlay.desiredVisible)
        overlay.setVisible(true)
        session.destroy()
        XCTAssertTrue(overlay.isHidden)
        XCTAssertNil(overlay.superview, "destroying the session removes its host readout")
    }

    func testCardUsesExistingClockTakesTouchesWithinItsBoundsAndPreservesItsPosition() throws {
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let links = FrameClock.shared.linksMade
        let overlay = FrameHealthOverlay(health: health)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        _ = hosted(overlay, in: scene, size: CGSize(width: 430, height: 900))
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        XCTAssertEqual(FrameClock.shared.linksMade, links, "showing the readout never starts a sampler")
        XCTAssertFalse(overlay.card.frame.isEmpty)
        XCTAssertTrue(overlay.bounds.contains(overlay.card.frame))
        XCTAssertEqual(overlay.accessibilityIdentifier, "exact.host.frame-health-overlay")
        // The card is opaque: it takes the touches within its bounds, and the rest reach the app
        let body = overlay.card.convert(CGPoint(x: 160, y: 100), to: overlay)
        XCTAssertTrue(overlay.hitTest(body, with: nil) === overlay.card)
        let outside = CGPoint(x: overlay.card.frame.maxX + 20, y: overlay.card.frame.maxY + 20)
        XCTAssertNil(overlay.hitTest(outside, with: nil), "outside the card, touches reach the app")
        let layout = FrameHealthCardFace.Layout(width: overlay.card.bounds.width)
        for (control, rect) in [(FrameHealthCardFace.Control.close, layout.close), (.title, layout.title), (.status, layout.status),
                                (.details, layout.details), (.share, layout.share)] {
            // Each button's touch target is 44 points tall and at least as wide, out to the card's edge or
            // halfway to the next button
            XCTAssertEqual(layout.control(at: CGPoint(x: rect.midX, y: rect.midY - 21)), control)
            XCTAssertEqual(layout.control(at: CGPoint(x: rect.midX + (control == .share ? 18 : -18), y: rect.midY)), control)
        }
        // The card measures the screen while it shows, so it is live from the start
        XCTAssertEqual(overlay.card.state.status, .live)
        XCTAssertEqual(overlay.card.state.request.0, "This app's frames")
        XCTAssertEqual(overlay.card.state.unit, "late")

        let elements = overlay.card.accessibilityElements?.compactMap { $0 as? UIAccessibilityElement } ?? []
        XCTAssertEqual(elements.first?.accessibilityLabel, "Close frame health", "Close comes first, in its corner")
        let title = try XCTUnwrap(elements.first { $0.accessibilityLabel == "Frame health" })
        XCTAssertTrue(title.accessibilityTraits.contains(.button), "the title opens the definitions")
        // The card rests in a corner, 8 points from the window's edges, past its safe area; moving it down
        // takes it to the bottom one, where it stays
        let action = try XCTUnwrap(title.accessibilityCustomActions?.first { $0.name == "Move down" })
        let bottom = overlay.bounds.maxY - 8
        XCTAssertTrue(action.actionHandler?(action) == true)
        XCTAssertEqual(overlay.card.frame.maxY, bottom, accuracy: 0.5)
        overlay.setVisible(false)
        XCTAssertNil(health.changed)
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        XCTAssertEqual(overlay.card.frame.maxY, bottom, accuracy: 0.5)
        // A window that leaves the card its 304-point minimum width keeps it, on its edge, and one too short for the
        // whole card leaves out its toolbar and chart area; a narrower window hides it
        overlay.frame.size = CGSize(width: 330, height: 400)
        overlay.layoutIfNeeded()
        XCTAssertFalse(overlay.card.isHidden)
        XCTAssertTrue(overlay.bounds.contains(overlay.card.frame), "resizing clamps the card into its own window")
        XCTAssertEqual(overlay.card.frame.maxY, overlay.bounds.maxY - 8, accuracy: 0.5)
        XCTAssertTrue(overlay.card.compact)
        overlay.frame.size = CGSize(width: 250, height: 400)
        overlay.layoutIfNeeded()
        XCTAssertTrue(overlay.card.isHidden, "a window narrower than the card's minimum width hides it")
        overlay.hideReadout()
        XCTAssertFalse(overlay.desiredVisible)
        XCTAssertFalse(health.hudVisible)
    }

    func testPublicationIsBoundedAndIdleReloadVirtualAndDisabledStayExplicit() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        var publications = 0
        health.changed = { publications += 1 }
        func drain(until count: Int) {
            let deadline = Date().addingTimeInterval(2)
            while publications < count, Date() < deadline { RunLoop.main.run(mode: .default, before: deadline) }
        }
        for i in 0..<120 {
            let t = 10 + Double(i) / 120
            health.record(arrival: t, target: t + 1 / 120, period: 1 / 120, finished: t + 0.001)
            health.maintain(at: t + 0.001)
            if i == 0 { drain(until: 1) }
            if i == 60 { drain(until: 2) }
        }
        XCTAssertEqual(publications, 2)
        XCTAssertEqual(health.snapshot().callbacks, 120)
        XCTAssertEqual(health.snapshot().intervals, 119)
        XCTAssertTrue(health.reply(virtual: false)["presentedFrames"] is NSNull)
        XCTAssertEqual(health.reply(virtual: true)["status"] as? String, "virtual")
        health.stop()
        drain(until: 3)
        XCTAssertEqual(publications, 3)
        health.record(arrival: 100, target: 100.01, period: 1 / 120, finished: 100.001)
        XCTAssertEqual(health.snapshot().intervals, 119, "stopped time does not become a gap")
        health.reset()
        XCTAssertEqual(health.snapshot().callbacks, 0)
        XCTAssertEqual(health.snapshot().generation, 1)
        let disabled = FrameHealth(enabled: false)
        defer { disabled.destroy() }
        disabled.record(arrival: 1, target: 2, period: 1, finished: 1.1)
        XCTAssertEqual(disabled.snapshot().callbacks, 0)
        let overlay = FrameHealthOverlay(health: disabled)
        _ = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852))
        overlay.setVisible(true)
        XCTAssertFalse(disabled.enabled, "showing the UI cannot override collection policy")
        XCTAssertTrue(overlay.card.state.request.0.contains("disabled"))
    }

    func testChartsRenderTheReadersSamplesWithoutStartingAFrameSource() throws {
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        health.requestedRate(120)
        let made = FrameClock.shared.linksMade
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        _ = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852))
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        let start = CACurrentMediaTime()
        var time = start
        for bucket in 1...10 {
            while time < start + Double(bucket) * 0.5 {
                // One 60 ms hitch in the sixth half second
                if bucket == 6, time - start > 2.7, time - start < 2.71 { time += 0.060 }
                health.record(arrival: time, target: time + 1 / 120, period: 1 / 120, finished: time + 0.002)
                time += 1.0 / 120
            }
            overlay.refresh(now: start + Double(bucket) * 0.5 + 0.001)
        }
        XCTAssertEqual(FrameClock.shared.linksMade, made)
        XCTAssertEqual(overlay.card.state.status, .live)
        XCTAssertEqual(overlay.card.state.rows[0].1, "120")
        XCTAssertEqual(overlay.card.state.pacing.0, "Uneven", "the hitch is in the last 5 seconds")
        // A pause keeps the figures and the charts' picture, and the card reads nothing until it resumes, while the
        // collector takes each frame
        overlay.card.activate?(.status)
        XCTAssertEqual(overlay.card.state.status, .paused)
        XCTAssertTrue(overlay.card.chart.held)
        health.record(arrival: time, target: time + 0.1, period: 0.1, finished: time + 0.001)
        overlay.refresh(now: start + 6)
        XCTAssertEqual(overlay.card.state.rows[0].1, "120")
        overlay.card.activate?(.status)
        XCTAssertEqual(overlay.card.state.status, .live)
        overlay.refresh(now: start + 6.5)
        XCTAssertEqual(overlay.card.state.rows[0].0, "Last 1.5s", "resumed, its first read covers the paused span")
        // Hidden and shown again, the card starts unpaused
        overlay.card.activate?(.status)
        overlay.setVisible(false)
        overlay.setVisible(true)
        XCTAssertNotEqual(overlay.card.state.status, .paused)
        // Hidden, the card reads nothing and releases what it drew from
        overlay.setVisible(false)
        XCTAssertNil(overlay.card.presentation.input)
        let attachment = XCTAttachment(image: UIGraphicsImageRenderer(bounds: overlay.card.bounds).image { context in
            overlay.card.layer.render(in: context.cgContext)
        })
        attachment.name = "Frame health — charts with a hitch"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testScrollingKeepsTheAppChartLiveAndPublishesInTrackingMode() throws {
        let health = FrameHealth(enabled: true)
        let scroll = FrameHealth(enabled: true, source: "native-scroll-ui-update")
        health.nativeHealth = scroll
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        _ = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852))
        overlay.setVisible(true)
        health.nativeScrollActive = true
        var published = false
        health.changed = {
            overlay.refresh()
            published = overlay.card.state.rows[0].1 != "—"
        }
        CFRunLoopAddCommonMode(CFRunLoopGetMain(), CFRunLoopMode(rawValue: RunLoop.Mode.tracking.rawValue as CFString))
        let ticks = Timer(timeInterval: 1 / 120, repeats: true) { _ in
            let time = CACurrentMediaTime()
            health.record(arrival: time, target: time + 1 / 120, period: 1 / 120, finished: time + 0.001)
            scroll.record(arrival: time, target: time + 0.01, period: 0, finished: time + 0.001)
            health.maintain(at: time + 0.001)
        }
        RunLoop.main.add(ticks, forMode: .tracking)
        defer { ticks.invalidate() }
        let end = Date().addingTimeInterval(5)
        while !published, Date() < end { RunLoop.main.run(mode: .tracking, before: end) }
        XCTAssertTrue(published, "the app chart refreshes before tracking ends, even with an active unknown-cadence scroll observer")
    }

    func testAnIdleAppShowsNoFramesThoughTheCardRedrawsItself() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        let window = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852)), dot = CALayer()
        window.layer.addSublayer(dot)
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        overlay.setVisible(true)
        // The card reads twice a second and sends its changes alone, which take no UI update
        RunLoop.main.run(until: Date().addingTimeInterval(3))
        XCTAssertEqual(overlay.screenFrames, 0, "\(overlay.card.state.rows)")
        XCTAssertEqual(overlay.card.state.rows.map(\.1), ["—", "—"])
        // While the charts move with the time, each of the app's own changes is one of its frames, and the card's are
        // none
        for x in 1...6 {
            dot.position.x = CGFloat(x)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        }
        XCTAssertEqual(overlay.screenFrames, 6)
        // Under the app's sustained updates, the card's reads and Details' changing rows go out with them, adding none
        func sustained() -> (ticks: Int, frames: Int) {
            let ticker = Ticker {
                dot.position.x += 1
                let t = CACurrentMediaTime()
                health.record(arrival: t, target: t + 1 / 60, period: 1 / 60, finished: t + 0.001)
            }
            let link = CADisplayLink(target: ticker, selector: #selector(Ticker.tick)), before = overlay.screenFrames
            link.add(to: .main, forMode: .common)
            RunLoop.main.run(until: Date().addingTimeInterval(1.5))
            link.invalidate()
            // The frames while the link ran: after it stops, UIKit can run a few updates of its own
            let frames = Int(overlay.screenFrames - before)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
            return (ticker.ticks, frames)
        }
        let closed = sustained()
        XCTAssertGreaterThan(closed.ticks, 10)
        XCTAssertEqual(closed.frames, closed.ticks, "one frame for each display link callback's change")
        overlay.card.activate?(.details)
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        let open = sustained()
        XCTAssertEqual(open.frames, open.ticks, "Details open, its rows changing with each read")
        overlay.card.activate?(.details)
        // The charts glide with the time, Core Animation moving them; a pause stops them, and resuming starts them again
        XCTAssertTrue(overlay.card.chart.gliding)
        overlay.card.activate?(.status)
        XCTAssertFalse(overlay.card.chart.gliding)
        overlay.card.activate?(.status)
        XCTAssertTrue(overlay.card.chart.gliding)
        // Hidden with no change of its own, as under a hidden ancestor, it stops measuring within half a second, paused
        // too
        overlay.card.activate?(.status)
        overlay.superview?.isHidden = true
        RunLoop.main.run(until: Date().addingTimeInterval(0.8))
        XCTAssertFalse(health.hudEffectiveVisible)
        XCTAssertFalse(overlay.observing)
        overlay.setVisible(false)
    }

    /// Measuring the screen, the card's Share gives the screen's report and Exact's frames' as two, each naming the other:
    /// an app drawn by GPU surfaces has frame times only on Exact's clock, in a production bake too (LLP 1079.000 §7.4)
    func testMeasuringTheScreenShareGivesExactsFramesAsAReportOfTheirOwn() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let health = FrameHealth(enabled: true), scroll = FrameHealth(enabled: true, source: "native-scroll-ui-update")
        health.nativeHealth = scroll
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        let window = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852)), dot = CALayer()
        window.layer.addSublayer(dot)
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        XCTAssertTrue(overlay.observing)
        // The app draws 45 frames, on Exact's clock too, the 30th late
        let ticker = Ticker {
            dot.position.x += 1
            let t = CACurrentMediaTime()
            health.record(arrival: t, target: t + 1 / 60, period: 1 / 60, finished: t + (dot.position.x == 30 ? 0.03 : 0.002))
        }
        let link = CADisplayLink(target: ticker, selector: #selector(Ticker.tick))
        link.add(to: .main, forMode: .common)
        waitFor("45 of the app's frames") { ticker.ticks >= 45 }
        link.invalidate()
        // Then a native scroll's late frame, newer than any of Exact's: still none of Exact's frames
        scroll.setHUDVisible(true)
        let late = CACurrentMediaTime()
        scroll.record(arrival: late, target: late + 1 / 120, period: 1 / 120, finished: late + 0.03)
        XCTAssertTrue(health.reportSource === scroll, "the scroll's late frame is the newest")
        func report(_ kind: FrameHealthShare) throws -> [String: Any] {
            var data: Data?
            overlay.report(kind) { data = $0 }
            waitFor("the report is encoded") { data != nil }
            return try XCTUnwrap(JSONSerialization.jsonObject(with: try XCTUnwrap(data)) as? [String: Any])
        }
        let clock = try report(.clockReport)
        XCTAssertEqual(clock["source"] as? String, health.source)
        XCTAssertEqual(clock["selection"] as? String, "one-source; the card's Share gives ui-update as a report of its own")
        XCTAssertTrue((clock["otherSources"] as? [[String: Any]])?.contains { $0["source"] as? String == "ui-update" } == true)
        let core = try XCTUnwrap(clock["coreSinceReload"] as? [String: Any])
        XCTAssertGreaterThan(Int(core["frameIntervals"] as? String ?? "") ?? 0, 30, "its frame times")
        XCTAssertNotNil((core["frameTimeBoundsNs"] as? [String: Any])?["p99"])
        XCTAssertEqual(((core["workBoundsNs"] as? [String: Any])?["maximum"] as? [UInt64])?.first, 24_000_000, "and the work of every frame, in its bin")
        XCTAssertFalse((clock["hitches"] as? [[Any]] ?? []).isEmpty, "with its late frame")
        let screen = try report(.report)
        XCTAssertEqual(screen["source"] as? String, "ui-update")
        XCTAssertEqual(screen["selection"] as? String, "one-source; the card's Share gives \(health.source) as a report of its own")
    }

    /// The face draws each band as a picture of its own, and a read draws again only the bands whose figures changed, so
    /// the card's drawing within the app's frames stays small (LLP 1079.000 §7.4)
    func testAReadDrawsOnlyTheBandsWhoseFiguresChanged() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        _ = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852))
        overlay.setVisible(true)
        defer { overlay.setVisible(false) }
        let card = overlay.card
        card.drawNow()
        let content = try XCTUnwrap(card.subviews.first)
        let bands = content.subviews.filter { $0.layer is FrameHealthCardView.Pictured.Picture && $0.frame.width == card.bounds.width }
        XCTAssertEqual(bands.map { $0.frame.minY / card.scale }, [0, 44, 82, 126, 170, 206], "header, context, figures, pacing, toolbar, charts")
        func drawn(_ change: (inout FrameHealthCardPresentation) -> Void) -> [Bool] {
            let before = bands.map { $0.layer.contents as AnyObject? }
            var next = card.presentation
            change(&next)
            card.update(next)
            card.drawNow()
            return zip(before, bands.map { $0.layer.contents as AnyObject? }).map { $0 !== $1 }
        }
        XCTAssertTrue(bands.allSatisfy { $0.layer.contents != nil })
        XCTAssertEqual(drawn { $0.rows = [("Last 0.5s", "59", "1"), ("Last 5s", "60", "2")] }, [false, false, true, false, false, false],
            "new figures draw their band alone")
        XCTAssertEqual(drawn { $0.pacing = ("Even", "300 frames at 16.7ms") }, [false, false, false, true, false, false])
        XCTAssertEqual(drawn { $0.status = .paused }, [true, false, false, false, false, false])
        XCTAssertEqual(drawn { _ in }, [false, false, false, false, false, false], "the same figures draw nothing")
    }

    /// A card measuring the screen holds two readers' places; with one free, it shows Exact's frames until a second is
    func testACardWithOnePlaceFreeShowsExactsFramesUntilASecondIs() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let others = (0..<3).map { _ in FrameHealthCardReader() }
        XCTAssertEqual(others.map { $0.start() }, [true, true, true])
        defer { others.forEach { $0.stop() } }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        _ = hosted(overlay, in: scene, size: CGSize(width: 393, height: 852))
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        overlay.setVisible(true)
        XCTAssertFalse(overlay.observing, "one place is free")
        others[0].stop()
        overlay.refresh()
        XCTAssertTrue(overlay.observing, "with two places free, at its next read")
        XCTAssertFalse(FrameHealthCardReader().start(), "which it holds, with the two others")
        overlay.setVisible(false)
    }

    /// A late frame of the screen's names who asked for its update: the app session's workload, and each of Exact's
    /// frame clock's users asking for frames
    func testTheScreensLateFrameNamesWhoAskedForIt() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the screen's frames")
        }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        addTeardownBlock { window.isHidden = true }
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        let screen = FrameHealthScreen()
        screen.workload = { 1 << 23 | 1 << 19 }
        screen.start(scene)
        defer { screen.stop() }
        screen.health.setHUDVisible(true)
        defer { screen.health.setHUDVisible(false) }
        // A live SVG filter's frames, the first held up past its deadline in one of Exact's host phases
        let filter = NSObject()
        var stalled = false
        FrameClock.shared.want(filter, .svgFilter) { _ in
            guard !stalled else { return }
            stalled = true
            HostPhases.shared.measure(.draw) { usleep(60_000) }
            screen.card { usleep(5_000) }
        }
        defer { FrameClock.shared.drop(filter) }
        func asked() -> UInt64? {
            let retained = UInt32(min(screen.health.historyStatus().retained, 16))
            return (0..<retained).map { screen.health.hitch($0).flags }.first { $0 & (1 << 45) != 0 }
        }
        let end = Date().addingTimeInterval(3)
        while asked() == nil, Date() < end { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        let flags = try XCTUnwrap(asked(), "the stalled frame was late, and a live SVG filter asked for it")
        XCTAssertEqual(flags & (1 << 23 | 1 << 19), 1 << 23 | 1 << 19, "with the session's workload")
        XCTAssertEqual(flags & (1 << 40), 0, "and no other user of the frame clock")
        XCTAssertEqual(flags & (1 << 35 | 1 << 36), 1 << 35, "while the card shows, not dragged")
        // Its work split: the stall in Exact's phases, and the card's own work within the update
        let report = FrameHealthReport.snapshot(health: screen.health, session: nil)
        let columns = try XCTUnwrap(report["columns"] as? [String])
        let row = try XCTUnwrap((report["hitches"] as? [[Any]])?.first { (($0[4] as? UInt64) ?? 0) & (1 << 45) != 0 })
        let value = { (name: String) in row[columns.firstIndex(of: name)!] }
        XCTAssertGreaterThanOrEqual(value("exact_us") as? Int ?? 0, 55_000)
        XCTAssertGreaterThanOrEqual(value("card_us") as? Int ?? 0, 4_500)
        XCTAssertLessThan(value("card_us") as? Int ?? .max, 20_000)
        XCTAssertNotNil(value("layouts") as? Int)
    }

    /// UIKit lets the main thread wait between an update's scheduling and its commit: the wait is none of the frame's
    /// work, which is taken to end at the commit, so the frame is late as before
    func testAWaitWithinAnUpdateIsNoneOfItsWork() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the screen's frames")
        }
        let screen = startedScreen(in: scene)
        defer { screen.stop() }
        screen.health.setHUDVisible(true)
        defer { screen.health.setHUDVisible(false) }
        let t = CACurrentMediaTime()
        screen.scheduled(presented: t + 0.02, deadline: t + 0.004, now: t)
        screen.wait(at: t + 0.001)
        screen.wake(at: t + 0.006)
        screen.completed(presented: t + 0.02, now: t + 0.009)
        let report = FrameHealthReport.snapshot(health: screen.health, session: nil, now: t + 0.01)
        let columns = try XCTUnwrap(report["columns"] as? [String])
        let row = try XCTUnwrap((report["hitches"] as? [[Any]])?.last)
        let value = { (name: String) in row[columns.firstIndex(of: name)!] }
        XCTAssertEqual(value("waited_us") as? Int, 5_000)
        XCTAssertEqual(Int(value("callback_us") as? UInt32 ?? 0), 4_000, "the 9 ms update less its 5 ms wait")
        XCTAssertEqual(value("target_offset_us") as? Int, -1_000, "the deadline, from the work's start, which it ran past")
        // The collector counts it, though its effective start came after its deadline
        let snapshot = screen.health.snapshot()
        XCTAssertEqual(snapshot.worked, 1)
        XCTAssertEqual(snapshot.late, 1)
    }

    /// A card measuring the screen holds the screen's reader, Exact's over 5 seconds for Details, the charts and the
    /// screen's work splits, in two readers' places
    func testACardMeasuringTheScreenHoldsItsReadersChartsAndSplits() {
        func heap<T>(_ array: [T]) -> Int { malloc_good_size(32 + array.capacity * MemoryLayout<T>.stride) }
        let health = FrameHealth(enabled: true), screen = FrameHealth(policy: .always, source: FrameHealth.screenSource)
        defer { health.destroy(); screen.destroy() }
        let face = FrameHealthCardReader(), clock = FrameHealthCardReader(), chart = FrameHealthChart(in: CALayer())
        XCTAssertTrue(clock.start())
        XCTAssertTrue(face.start())
        defer { clock.stop(); face.stop() }
        clock.keep(5)
        chart.start()
        for index in 0..<4000 {
            let time = 1 + Double(index) / 120
            health.record(arrival: time, target: time + 1 / 120, period: 1 / 120, finished: time + 0.003)
            screen.record(arrival: time, target: time + 1 / 120, period: 0, finished: time + 0.003)
            screen.split(at: time, waited: 0.001, exact: 0.001, card: 0, layouts: 1)
            chart.screenFrame(at: time, work: 0.003, budget: 1 / 120)
            if index % 60 == 59 {
                face.sample(screen, now: time + 0.001, force: true, main: { (0.003, 0) })
                clock.sample(health, now: time + 0.001, force: true)
            }
        }
        XCTAssertEqual(face.samples.count, 60)
        XCTAssertEqual(clock.samples.count, 10)
        let read = FrameHealthMetrics.read(health)
        let samples = { (reader: FrameHealthCardReader) in heap(reader.samples) + reader.samples.reduce(0) { $0 + heap($1.bins) } }
        let previous = heap(read.intervals) + heap(read.work) + heap(read.refreshes) + heap(read.lateBy)
        let charts = heap(chart.columns) + heap(chart.times)
        let splits = heap(screen.splits)
        let objects = [face, clock].reduce(0) { $0 + malloc_size(Unmanaged.passUnretained($1).toOpaque()) }
            + malloc_size(Unmanaged.passUnretained(chart).toOpaque())
        let card = samples(face) + samples(clock) + 2 * previous + charts + splits + objects
        XCTAssertLessThan(card, 60 * 1024, "face \(samples(face)), clock \(samples(clock)), charts \(charts), splits \(splits)")
        XCTAssertLessThan(card, 2 * 51 * 1024, "within the two places it holds, each a card's of Exact's frames")
    }

    /// A frame's work, and its split, end at its last commit: Exact's host phases after it are none of its work
    func testAFramesSplitEndsAtItsCommit() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the screen's frames")
        }
        let screen = startedScreen(in: scene)
        defer { screen.stop() }
        screen.health.setHUDVisible(true)
        defer { screen.health.setHUDVisible(false) }
        // Times are given, the host phases' clock too
        let t = CACurrentMediaTime(), phases = HostPhases.shared, clock = phases.clock
        var now = t
        phases.clock = { now }
        defer { phases.clock = clock }
        screen.scheduled(presented: t + 0.05, deadline: t + 0.001, now: t)
        phases.measure(.apply) { now = t + 0.002 }
        screen.committed(at: t + 0.002)
        phases.measure(.apply) { now = t + 0.012 }
        screen.completed(presented: t + 0.05, now: t + 0.012)
        let report = FrameHealthReport.snapshot(health: screen.health, session: nil, now: t + 0.013)
        let columns = try XCTUnwrap(report["columns"] as? [String])
        let row = try XCTUnwrap((report["hitches"] as? [[Any]])?.last)
        let value = { (name: String) in row[columns.firstIndex(of: name)!] }
        XCTAssertEqual(Int(value("callback_us") as? UInt32 ?? 0), 2_000, "the work ends at the commit")
        XCTAssertEqual(value("exact_us") as? Int, 2_000, "and the 10 ms after it are none of its host phases")
    }

    /// A screen observer started on `scene`, after a window's first frames there.
    private func startedScreen(in scene: UIWindowScene) -> FrameHealthScreen {
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        addTeardownBlock { window.isHidden = true }
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        let screen = FrameHealthScreen()
        screen.start(scene)
        return screen
    }

    func testTheScreenSeesEachFrameAndTheMainThreadsWorkOnlyWhileItRuns() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the screen's frames")
        }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        addTeardownBlock { window.isHidden = true }
        let scroll = UIScrollView(frame: window.bounds), dot = CALayer()
        scroll.contentSize = CGSize(width: window.bounds.width, height: 20_000)
        window.rootViewController?.view.addSubview(scroll)
        window.layer.addSublayer(dot)
        func run(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }
        run(0.3)
        let screen = FrameHealthScreen(), chart = FrameHealthChart(in: CALayer())
        chart.start()
        screen.chart = chart
        screen.start(scene)
        defer { screen.stop() }
        // A spell is the app's work less the card's own time within it, so an app stall the card's timers fire after
        // still counts, and the card's work alone does not; times are given, as the run loop's observers give them
        let t = CACurrentMediaTime()
        screen.wake(at: t)
        screen.card(took: 0.02)
        screen.wait(at: t + 0.08)
        var spell = screen.takeMainThread()
        XCTAssertEqual(spell.longest, 0.06, accuracy: 1e-9)
        XCTAssertEqual(spell.long, 1)
        screen.wake(at: t + 0.1)
        screen.card(took: 0.03)
        screen.wait(at: t + 0.13)
        spell = screen.takeMainThread()
        XCTAssertEqual(spell.longest, 0, accuracy: 1e-9)
        XCTAssertEqual(spell.long, 0)
        // Scrolling, each update is a frame with its work, and the frames of the last half second give the rate; a frame
        // of the screen's bounds no frame time
        scroll.setContentOffset(CGPoint(x: 0, y: 3000), animated: true)
        run(0.6)
        let snapshot = screen.health.snapshot()
        XCTAssertGreaterThan(snapshot.callbacks, 10)
        XCTAssertEqual(snapshot.worked, snapshot.callbacks, "each frame's commit is its work")
        XCTAssertEqual(snapshot.intervals, 0)
        XCTAssertTrue(chart.columns.contains { $0.fps >= 20 && $0.budget > 0 })
        XCTAssertFalse(chart.columns.contains { $0.frame > 0 }, "the charts plot no frame time")
        _ = screen.takeMainThread()
        // A stall within a scroll is the main thread's long spell, and the charts' work
        run(0.3)
        scroll.setContentOffset(.zero, animated: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { usleep(60_000) }
        run(0.6)
        let main = screen.takeMainThread()
        XCTAssertGreaterThanOrEqual(main.longest, 0.06)
        XCTAssertGreaterThanOrEqual(main.long, 1)
        XCTAssertTrue(chart.columns.contains { $0.work >= 60 }, "the stall is the work chart's")
        // Stopped, it observes nothing
        screen.stop()
        let callbacks = screen.health.snapshot().callbacks
        scroll.setContentOffset(CGPoint(x: 0, y: 3000), animated: true)
        run(0.4)
        XCTAssertEqual(screen.health.snapshot().callbacks, callbacks)
    }

    func testTheCardKeepsItsPositionAlongEveryEdgeAfterLayout() throws {
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health, measuresScreen: false)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the card in a real host window")
        }
        let window = hosted(overlay, in: scene, size: .zero)
        overlay.frame = window.bounds
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        let area = overlay.bounds.insetBy(dx: 8, dy: 8)
        let right = area.maxX - overlay.card.frame.width
        let bottom = area.maxY - overlay.card.frame.height
        let x = area.minX + (right - area.minX) * 0.37, y = area.minY + (bottom - area.minY) * 0.61
        for target in [CGPoint(x: area.minX, y: y), CGPoint(x: right, y: y), CGPoint(x: x, y: area.minY), CGPoint(x: x, y: bottom)] {
            overlay.card.grabbed?()
            overlay.card.moved?(target)
            overlay.card.released?(.zero)
            XCTAssertEqual(overlay.card.frame.minX, target.x, accuracy: 0.001)
            XCTAssertEqual(overlay.card.frame.minY, target.y, accuracy: 0.001)
        }
        let limit = Date(timeIntervalSinceNow: 10)
        while overlay.movesFast, Date() < limit { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05)) }
        XCTAssertFalse(overlay.movesFast)
        overlay.setVisible(false)
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        XCTAssertEqual(overlay.card.frame.minX, x, accuracy: 0.001)
        overlay.frame.size.width += 100
        overlay.setNeedsLayout()
        overlay.layoutIfNeeded()
        let wider = overlay.bounds.insetBy(dx: 8, dy: 8)
        XCTAssertEqual(overlay.card.frame.minX, wider.minX + (wider.width - overlay.card.frame.width) * 0.37, accuracy: 0.001)
        XCTAssertEqual(overlay.card.frame.maxY, wider.maxY, accuracy: 0.001)
        let before = overlay.card.frame.minX
        XCTAssertTrue(overlay.card.accessibilityMove?(CGVector(dx: 0, dy: -1)) == true)
        XCTAssertEqual(overlay.card.frame.minX, before, accuracy: 0.001, "moving to another edge keeps the position on the other axis")
        XCTAssertEqual(overlay.card.frame.minY, wider.minY, accuracy: 0.001)
        overlay.setVisible(false)
    }

    func testADragGoesPastTheEdgesAndTheCardSpringsBackToAScreenCorner() throws {
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the card in a real host window, whose corners it follows")
        }
        let window = hosted(overlay, in: scene, size: .zero)
        overlay.frame = window.bounds
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        let corner = CGPoint(x: 8, y: 8)
        XCTAssertEqual(overlay.card.frame.origin, corner, "a new card rests in the top-left corner, past the safe area")
        // Held and dragged, the card asks for the display's highest rates
        overlay.card.grabbed?()
        if #available(iOS 18.0, *) { XCTAssertTrue(overlay.movesFast) }
        overlay.card.moved?(CGPoint(x: -92, y: corner.y))
        XCTAssertLessThan(overlay.card.frame.minX, corner.x, "a drag carries the card past the edge")
        XCTAssertGreaterThan(overlay.card.frame.minX, -92, "but not as far as the press")
        overlay.card.released?(.zero)
        XCTAssertEqual(overlay.card.frame.origin, corner, "on release it springs back to its corner")
        if #available(iOS 18.0, *) { XCTAssertTrue(overlay.movesFast, "and asks for them as it springs") }
        let limit = Date(timeIntervalSinceNow: 10)
        while overlay.movesFast, Date() < limit { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05)) }
        XCTAssertFalse(overlay.movesFast, "and no longer once the spring settles")
        if #available(iOS 26.0, *) {
            // The card's corners are concentric with the window's, never under 12 points: in a rounded screen corner
            // the top-left grows with the screen's rounding, and a square one, as the folded iPhone Duo's, leaves 12
            XCTAssertEqual(overlay.card.cornerConfiguration, .corners(radius: .containerConcentric(minimum: 12)))
            overlay.card.layoutIfNeeded()
            XCTAssertEqual(overlay.card.effectiveRadius(corner: .bottomRight), 12, accuracy: 0.5)
            XCTAssertGreaterThanOrEqual(overlay.card.effectiveRadius(corner: .topLeft), 12)
        }
    }

    func testTheCardsHairlineAndHeaderKeepToItsRoundedCorners() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("a card's corners follow the screen's from iOS 26") }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the card in a real host window, drawn as the screen shows it")
        }
        let window = hosted(overlay, in: scene, size: .zero)
        window.overrideUserInterfaceStyle = .dark
        window.rootViewController?.view.backgroundColor = .black
        overlay.frame = window.bounds
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        // Beside the card, two views of its corners and hairline, the hairline their own layer's border: one that
        // draws nothing, whose hairline is whole, and one that draws, as the card did
        let card = overlay.card, below = card.frame.maxY + 8
        let whole = bordered(UIView(frame: CGRect(x: 8, y: below, width: 120, height: 120)))
        let fading = bordered(DrawsBlack(frame: CGRect(x: 136, y: below, width: 120, height: 120)))
        let scale = window.traitCollection.displayScale
        for view in [whole, fading] {
            window.rootViewController?.view.addSubview(view)
            view.layer.borderWidth = 1 / scale
        }
        window.layoutIfNeeded()
        XCTAssertTrue(window.bounds.contains(fading.frame), "both fit under the card")
        XCTAssertEqual(card.effectiveRadius(corner: .bottomRight), whole.effectiveRadius(corner: .bottomRight), accuracy: 0.5)
        // Captured only across the three bottom-right corners
        let region = CGRect(x: 0, y: card.frame.maxY - 16, width: window.bounds.width, height: whole.frame.maxY - card.frame.maxY + 20)
        let image = try XCTUnwrap(UIGraphicsImageRenderer(bounds: region).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }.cgImage)
        var pixels = [UInt8](repeating: 0, count: image.width * image.height * 4)
        let ctx = try XCTUnwrap(CGContext(data: &pixels, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        // The hairline's light summed across the bottom-right corner's diagonal, on black: a sum, not the brightest
        // pixel, since a card that grows with the text size can end between pixels, which share its hairline
        func hairline(_ view: UIView) -> Double {
            let r = view.effectiveRadius(corner: .bottomRight), frame = view.frame
            return stride(from: 0.5 * r, to: 1.6 * r, by: 0.25 / scale).reduce(0) { sum, s in
                let x = Int((frame.maxX - r + s / 2.squareRoot() - region.minX) * scale)
                let y = Int((frame.maxY - r + s / 2.squareRoot() - region.minY) * scale)
                guard x >= 0, y >= 0, x < image.width, y < image.height else { return sum }
                let i = (y * image.width + x) * 4
                return sum + Double(Int(pixels[i]) + Int(pixels[i + 1]) + Int(pixels[i + 2])) / 3 / 4
            }
        }
        let (drawn, reference, faded) = (hairline(card), hairline(whole), hairline(fading))
        let figures = "card \(Int(drawn)), a view that draws nothing \(Int(reference)), one that draws \(Int(faded))"
        XCTAssertGreaterThan(reference, 20, "the hairline shows: \(figures)")
        XCTAssertGreaterThanOrEqual(drawn, 0.9 * reference, "the card's is as strong around the corner: \(figures)")
        // Where this fails, this runtime keeps the border of a layer that draws, and the card needs no content view
        XCTAssertLessThan(faded, 0.4 * reference, "a layer that draws and clips fades its own border here: \(figures)")

        // Dragged right and released, the card rests in the screen's top-right corner; where that corner rounds, Share
        // moves in and Close, whose corner stays 12 points, keeps its place
        card.moved?(CGPoint(x: overlay.bounds.maxX, y: 8))
        card.released?(.zero)
        overlay.layoutIfNeeded()
        card.layoutIfNeeded()
        let radius = card.effectiveRadius(corner: .topRight)
        guard radius > 12.5 else { return }
        let moved = card.layout, square = FrameHealthCardFace.Layout(width: moved.width)
        XCTAssertEqual(moved.corners.topRight, (radius / card.scale).rounded())
        XCTAssertEqual(moved.corners.topLeft, 12, "the card's top-left corner is away from the screen's")
        XCTAssertLessThan(moved.share.maxX, square.share.maxX, "Share moves in from the rounder corner")
        XCTAssertEqual(moved.close, square.close)
        // Moved down into the screen's bottom-right corner, the card's bottom-right corner rounds instead, and the text
        // beside it moves in (FrameHealthMetricsTests)
        card.moved?(CGPoint(x: overlay.bounds.maxX, y: overlay.bounds.maxY))
        card.released?(.zero)
        overlay.layoutIfNeeded()
        card.layoutIfNeeded()
        let low = card.layout
        XCTAssertEqual(low.corners.bottomRight, (card.effectiveRadius(corner: .bottomRight) / card.scale).rounded())
        XCTAssertGreaterThan(low.corners.bottomRight, 12)
        XCTAssertEqual(low.corners.topRight, 12)
    }

    func testTheCardsFaceShowsItsTitleAndButtons() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        let window = hosted(overlay, in: scene, size: CGSize(width: 430, height: 900))
        window.overrideUserInterfaceStyle = .light
        overlay.setVisible(true)
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        // The face renders itself as an image; what it shows is what the card's layers hold
        let card = overlay.card, layout = FrameHealthCardFace.Layout(width: card.bounds.width / card.scale)
        let width = Int(card.bounds.width), height = Int(card.bounds.height)
        var bytes = [UInt8](repeating: 255, count: width * height * 4)
        bytes.withUnsafeMutableBytes { buffer in
            let ctx = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
            ctx.translateBy(x: 0, y: CGFloat(height))
            ctx.scaleBy(x: 1, y: -1)
            card.layer.render(in: ctx)
        }
        // Text and symbols draw in the label's colours on the card's white ground in light mode
        func inked(_ rect: CGRect) -> Int {
            let r = rect.applying(CGAffineTransform(scaleX: card.scale, y: card.scale)).integral
            var n = 0
            for y in max(0, Int(r.minY))..<min(height, Int(r.maxY)) {
                for x in max(0, Int(r.minX))..<min(width, Int(r.maxX)) where bytes[(y * width + x) * 4] < 160 { n += 1 }
            }
            return n
        }
        XCTAssertGreaterThan(inked(layout.title), 30, "the title shows")
        XCTAssertGreaterThan(inked(layout.close), 5, "Close shows its symbol")
        // At the display's scale, so its text is as sharp as UIKit's
        func picture(in view: UIView) -> CALayer? {
            "\(type(of: view.layer))".hasSuffix("Picture") ? view.layer : view.subviews.lazy.compactMap(picture).first
        }
        let face = try XCTUnwrap(picture(in: card))
        let image = try XCTUnwrap(face.contents as! CGImage?)
        XCTAssertEqual(face.contentsScale, card.traitCollection.displayScale)
        XCTAssertEqual(CGFloat(image.width), face.bounds.width * card.traitCollection.displayScale, accuracy: 1)
        overlay.setVisible(false)
    }

    func testTheCardFollowsDarkModeWithoutBeingMoved() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window whose appearance changes")
        }
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        let window = hosted(overlay, in: scene, size: CGSize(width: 430, height: 900))
        window.overrideUserInterfaceStyle = .light
        overlay.setVisible(true)
        overlay.layoutIfNeeded()
        // The card's ground, systemBackground, is white in light mode and black in dark
        let dark = { (overlay.card.palette.background.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent,
            options: nil)?.components?.first ?? 1) < 0.5 }
        XCTAssertFalse(dark())
        // Only the appearance changes: nothing lays the card out or moves it
        window.overrideUserInterfaceStyle = .dark
        let followed = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in dark() }, object: nil)
        wait(for: [followed], timeout: 2)
        XCTAssertEqual(overlay.card.backgroundColor?.cgColor, overlay.card.palette.background, "the card's ground follows too")
    }

    func testTheMenusOneFrameHealthRowIsDimmedAndExplainsWhereTheCardCannotRead() {
        let on = FrameHealth(enabled: true), off = FrameHealth(enabled: false)
        defer { on.destroy(); off.destroy() }
        let available = DevMenuController.frameHealth(on, container: true)
        XCTAssertEqual(available.title, "Frame Health")
        XCTAssertNil(available.unavailable)
        XCTAssertFalse(available.checked, "the card starts hidden")
        let disabled = DevMenuController.frameHealth(off, container: true)
        XCTAssertEqual(disabled.title, "Frame Health Disabled", "the same one row, dimmed")
        XCTAssertEqual(disabled.unavailable?.title, "Frame Health Is Off")
        XCTAssertTrue(disabled.unavailable?.message.hasPrefix("This app turns frame health off") == true, "it says what turned it off")
        XCTAssertEqual(DevMenuController.frameHealth(on, container: false).unavailable?.title, "Frame Health Is Unavailable")
        // A production build's collector, which collects only while the card shows, offers the row as available
        let demand = FrameHealth(policy: .onDemand)
        defer { demand.destroy() }
        XCTAssertEqual(DevMenuController.frameHealth(demand, container: true).title, "Frame Health")
    }

    func testMenuUsesNativeRowsAndBuildDetailsHaveTheirOwnScreen() throws {
        let navigation = DevMenuController.navigation()
        navigation.view.frame = CGRect(x: 0, y: 0, width: 430, height: 900)
        navigation.view.layoutIfNeeded()
        let menu = try XCTUnwrap(navigation.viewControllers.first as? DevMenuController)
        menu.loadViewIfNeeded()
        menu.view.frame = navigation.view.bounds
        menu.view.layoutIfNeeded()
        XCTAssertEqual(menu.tableView.style, .insetGrouped)
        XCTAssertEqual(menu.tableView.numberOfSections, 3)
        XCTAssertNotNil(menu.navigationItem.rightBarButtonItem)
        let row = IndexPath(row: 0, section: 0)
        let cell = try XCTUnwrap(menu.tableView.cellForRow(at: row))
        XCTAssertGreaterThan(cell.bounds.width, 350, "commands use the native sheet width")
        XCTAssertGreaterThanOrEqual(cell.bounds.height, 44)
        XCTAssertEqual((cell.contentConfiguration as? UIListContentConfiguration)?.text, "Reload")
        let frameHealth = try XCTUnwrap(menu.groups[1].first)
        XCTAssertEqual(frameHealth.symbol, "waveform.path.ecg")
        let content = try XCTUnwrap(menu.tableView.cellForRow(at: IndexPath(row: 0, section: 1))?.contentConfiguration as? UIListContentConfiguration)
        XCTAssertEqual(content.text, frameHealth.title)
        XCTAssertNil(content.secondaryText, "the row has no subtext")
        XCTAssertFalse(menu.groups.flatMap { $0 }.contains { $0.title == "Copy Info" })
        menu.tableView(menu.tableView, didSelectRowAt: IndexPath(row: 0, section: 2))
        XCTAssertTrue(navigation.viewControllers.last?.view is UITextView)
        XCTAssertEqual(navigation.viewControllers.last?.title, "Build Information")
    }
}

/// `view` on black with the card's corners and, as its own layer's border, the card's hairline color; the caller
/// sets the width once the view is in its window
@available(iOS 26.0, *)
private func bordered<View: UIView>(_ view: View) -> View {
    view.cornerConfiguration = .corners(radius: .containerConcentric(minimum: 12))
    view.layer.masksToBounds = true
    view.backgroundColor = .black
    view.layer.borderColor = UIColor.separator.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)).cgColor
    return view
}

/// A view that draws its bounds black, so its layer has drawn contents, as the card's did
private final class DrawsBlack: UIView {
    override func draw(_ rect: CGRect) {
        UIGraphicsGetCurrentContext()?.setFillColor(UIColor.black.cgColor)
        UIGraphicsGetCurrentContext()?.fill(bounds)
    }
}
#endif
