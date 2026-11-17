#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// `layout agree` on AppKit (LLP 1080.001 D3): a content region's surface,
/// this session's by its controller, is accounted for and its interior is
/// the region's; an unrelated view beside it is still a stray.
final class AgreementMacTests: XCTestCase {
    func testOnlyTheParagraphsOwnInlineKeyViewsAreAccountedFor() throws {
        _ = NSApplication.shared
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text"],
            ["op": "paragraph", "id": 1, "runs": [
                ["id": 2, "parent": 1, "paint": true, "props": ["text": "Link", "href": "/next"], "handlers": []],
            ]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 30],
        ]))
        let owner = try XCTUnwrap(p.views[1])
        let keyView = try XCTUnwrap(owner.syncInlineKeyViews().first)
        let kernel = KernelFrames([KernelFrame(id: 1, parent: nil, rect: owner.frame, bits: 0)])
        func agree() -> AgreementReport {
            let report = AgreementReport(tolerance: 1)
            p.inspectAgreement(roots: [(p.root, "document")], kernel: kernel, inFlight: false, report: report) { $0.frame }
            return report
        }
        XCTAssertEqual(agree().counts, [:])
        let dump = p.inspectionDump(owner, kind: "view", depth: 2, limit: 20) { $0.frame }
        let entries = try XCTUnwrap(dump["entries"] as? [[String: Any]])
        XCTAssertEqual(entries.first { $0["class"] as? String == "InlineKeyView" }?["role"] as? String, "editor")
        owner.addSubview(InlineKeyView(owner: owner, id: keyView.id))
        let report = agree()
        XCTAssertEqual(report.counts["stray"], 1, "an unregistered key view is still a stray: \(report.found)")
        XCTAssertEqual(report.found.first?["class"] as? String, "InlineKeyView")
    }

    func testThisSessionsRegionSurfaceIsAccountedForAndASiblingIsNot() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "agreement-region")
        let p = session.presenter
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0],
        ]))
        let owner = try XCTUnwrap(p.views[2])
        owner.addSubview(RegionSurfaceMac(controller: session.regions), positioned: .above, relativeTo: nil)
        let kernel = KernelFrames([KernelFrame(id: 1, parent: nil, rect: CGRect(x: 0, y: 0, width: 400, height: 400), bits: 0),
                                   KernelFrame(id: 2, parent: 1, rect: CGRect(x: 0, y: 0, width: 400, height: 200), bits: 0)])
        func agree() -> AgreementReport {
            let report = AgreementReport(tolerance: 1)
            p.inspectAgreement(roots: [(p.root, "document")], kernel: kernel, inFlight: false, report: report) { $0.frame }
            return report
        }
        let clean = agree()
        XCTAssertEqual(clean.counts, [:], "the session's surface is accounted for: \(clean.found)")
        XCTAssertGreaterThan(clean.opaque, 0, "its interior is the region's, counted, not judged")
        owner.addSubview(NSView(frame: NSRect(x: 0, y: 0, width: 10, height: 10)))
        let r = agree()
        XCTAssertEqual(r.counts["stray"], 1, "\(r.found)")
        XCTAssertEqual(r.found.first?["class"] as? String, "NSView")
        XCTAssertEqual(r.found.first?["under"] as? Int, 2)
    }
}
#endif
