#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// `--display` (LLP 1012, "where a driven Mac window opens"): the display a
/// name picks, the refusal that lists the rest, and the frame on it.
final class AgentDisplayMacTests: XCTestCase {
    private let names = ["Built-in Retina Display", "LG UltraFine", "DELL U2720Q", "dell u2720q"]

    func testAnIndexOrOneNamePicksADisplay() throws {
        XCTAssertEqual(try AgentDisplay.pick("1", among: names).get(), 1)
        XCTAssertEqual(try AgentDisplay.pick("lg ultrafine", among: names).get(), 1)
        XCTAssertEqual(try AgentDisplay.pick(" Built-in Retina Display ", among: names).get(), 0)
    }

    func testAMissOrATieIsRefusedWithTheList() {
        let list = #"0 "Built-in Retina Display", 1 "LG UltraFine", 2 "DELL U2720Q", 3 "dell u2720q""#
        for (name, says) in [("4", "names no display here"), ("-1", "names no display here"), ("Studio Display", "names no display here"), ("", "names no display here"),
                             ("DELL U2720Q", "names more than one display; name it by index")] {
            guard case .failure(let refusal) = AgentDisplay.pick(name, among: names) else { return XCTFail("\(name) was picked") }
            XCTAssertEqual(refusal.description, "EXACT_AGENT_DISPLAY \"\(name)\" \(says): \(list)")
        }
    }

    func testTheWindowIsCentredOnTheDisplayAndKeptOnIt() {
        // A second display left of the main one, below the menu bar's row: negative x, its own origin.
        let visible = NSRect(x: -2560, y: 0, width: 2560, height: 1415)
        XCTAssertEqual(AgentDisplay.frame(NSSize(width: 420, height: 928), centredIn: visible), NSRect(x: -1490, y: 244, width: 420, height: 928))
        XCTAssertEqual(AgentDisplay.frame(NSSize(width: 3000, height: 1600), centredIn: visible), NSRect(x: -2560, y: -185, width: 3000, height: 1600))
        // A drive's spread moves it, but not past the display's top or sides.
        XCTAssertEqual(AgentDisplay.frame(NSSize(width: 420, height: 928), centredIn: visible, offset: NSPoint(x: -72, y: 36)), NSRect(x: -1562, y: 280, width: 420, height: 928))
        XCTAssertEqual(AgentDisplay.frame(NSSize(width: 2500, height: 1400), centredIn: visible, offset: NSPoint(x: -120, y: 60)), NSRect(x: -2560, y: 15, width: 2500, height: 1400))
    }
}
#endif
