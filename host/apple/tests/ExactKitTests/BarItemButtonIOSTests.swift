#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

extension NavigationBarIOSTests {
    func testTheDefaultTapReachesOnlyTheControlUIKitPlacedForTheItem() throws {
        let session = try fixture("tabbed-bar-item", module: false), agent = Agent(session: session)
        let node = try node(session, "compose-home")
        let (bar, item, _) = try XCTUnwrap(session.presenter.navigationHost.barButtonItem(node))
        let window = try XCTUnwrap(bar.window)
        let frame = try XCTUnwrap(item.frame(in: window))
        let middle = CGPoint(x: frame.midX, y: frame.midY)
        let button = try XCTUnwrap(window.hitTest(middle, with: nil) as? UIControl)
        if bar.traitCollection.placesVerticalBar { XCTAssertFalse(button.isDescendant(of: bar)) }
        let cover = UIControl(frame: button.bounds)
        cover.accessibilityIdentifier = item.accessibilityIdentifier
        var presses = [UInt32]()
        session.presenter.onPress = { presses.append($0) }
        let tap = { agent.tap(["id": Int(node.id)]) }
        func assertCovered(file: StaticString = #filePath, line: UInt = #line) {
            XCTAssertIdentical(window.hitTest(middle, with: nil), cover, file: file, line: line)
            let reply = tap()
            XCTAssertTrue((reply["error"] as? String)?.contains("covers its middle") == true, "\(reply)", file: file, line: line)
            XCTAssertTrue(presses.isEmpty, file: file, line: line)
        }

        window.addSubview(cover)
        cover.frame = frame
        defer { cover.removeFromSuperview() }
        assertCovered()
        let holder = try XCTUnwrap(button.superview)
        holder.addSubview(cover)
        cover.frame = holder.convert(frame, from: window)
        var writes = 0
        let observation = cover.layer.observe(\.position) { _, _ in writes += 1 }
        assertCovered()
        observation.invalidate()
        XCTAssertEqual(writes, 0)
        button.addSubview(cover)
        cover.frame = button.bounds
        assertCovered()

        cover.isHidden = true
        button.accessibilityIdentifier = item.accessibilityIdentifier
        XCTAssertIdentical(window.hitTest(middle, with: nil), button)
        XCTAssertEqual(tap()["native"] as? String, "bar-button-item")
        XCTAssertEqual(presses, [node.id])
    }

    /// Stand in for the view UIKit vends when the button is hosted outside its bar.
    private final class ProjectedItem: UIBarButtonItem {
        let button = UIControl()
        override func value(forKey key: String) -> Any? {
            key == "view" ? button : super.value(forKey: key)
        }
    }

    func testTheAgentFollowsTheSameButtonIntoGlassOrASiblingStrip() throws {
        let session = try fixture("bar-glass", module: false), agent = Agent(session: session)
        let compose = try node(session, "compose-home")
        let (bar, item, owner) = try XCTUnwrap(session.presenter.navigationHost.barButtonItem(compose))
        let window = try XCTUnwrap(bar.window)
        let projected = ProjectedItem(title: "Compose", style: .plain, target: item.target, action: item.action)
        projected.accessibilityIdentifier = item.accessibilityIdentifier
        owner.setRightBarButtonItems([projected], animated: false)
        let button = projected.button, strip = UIView()
        defer { strip.removeFromSuperview() }
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        for host in [try XCTUnwrap(session.view) as UIView, try XCTUnwrap(nav.view)] {
            host.addSubview(strip)
            strip.frame = CGRect(x: 100, y: 200, width: 48, height: 44)
            strip.addSubview(button)
            button.frame = strip.bounds
            let center = button.convert(CGPoint(x: button.bounds.midX, y: button.bounds.midY), to: window)
            XCTAssertIdentical(window.hitTest(center, with: nil), button)
            XCTAssertNil(agent.drawnCovered(compose))
            let cover = UIControl(frame: button.frame)
            cover.accessibilityIdentifier = item.accessibilityIdentifier
            strip.addSubview(cover)
            XCTAssertTrue((agent.drawnCovered(compose) ?? "").contains("covers its middle"))
            cover.removeFromSuperview()
        }
    }

}
#endif
