import XCTest
import ExactPackage
@testable import ExactKit

/// A package's context names the session's directories and a label the same
/// at every launch (#315): what the host makes a session's module with, which
/// `ExactExtensions.context` hands each package's context.
final class PackageContextTests: XCTestCase {
    func testTheSampleHostsSessionsKeepDistinctLabelsAcrossLaunches() throws {
        // Each round is a launch: the sample host makes `a` and `b` again.
        for _ in 0..<2 {
            let a = ExactApp.shared.makeSession(label: "a"), b = ExactApp.shared.makeSession(label: "b")
            defer { a.destroy(); b.destroy() }
            for (session, label) in [(a, "a"), (b, "b")] {
                let made = NativeViews.moduleContext(session), roots = NativeViews.roots(session: session)
                XCTAssertEqual(made["session"] as? String, label)
                XCTAssertEqual(made["data"] as? String, roots.data)
                XCTAssertEqual(made["cache"] as? String, roots.cache)
                XCTAssertEqual(made["temporary"] as? String, roots.temporary)
            }
        }
    }

    func testUnnamedSessionsTakeTheFirstFreeLabel() {
        let first = ExactApp.shared.makeSession(), second = ExactApp.shared.makeSession()
        defer { second.destroy() }
        XCTAssertNotEqual(first.label, second.label)
        XCTAssertTrue(first.label.hasPrefix("session-"), first.label)
        let label = first.label
        first.destroy()
        let again = ExactApp.shared.makeSession()
        defer { again.destroy() }
        XCTAssertEqual(again.label, label, "the same order of sessions, the same labels")
    }

    func testAPackageContextCarriesTheLabelAndRoots() {
        let base = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true).appendingPathComponent("package-roots")
        let (data, cache, temporary) = (base.appendingPathComponent("data"), base.appendingPathComponent("cache"), base.appendingPathComponent("temporary"))
        let context = ExactPackageContext(agent: true, session: "2", hostSession: "owner", label: "b",
                                          data: data, cache: cache, temporary: temporary,
                                          changed: { _ in }, log: { _ in }, now: { 0 })
        XCTAssertEqual(context.label, "b")
        XCTAssertEqual([context.data, context.cache, context.temporary], [data, cache, temporary])
        // A context made with no label (a test's) is labelled by its session.
        XCTAssertEqual(ExactPackageContext(agent: false, session: "7", changed: { _ in }, log: { _ in }, now: { 0 }).label, "7")
    }
}
