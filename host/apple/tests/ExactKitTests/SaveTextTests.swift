#if os(iOS) || os(macOS)
#if os(iOS)
import UIKit
#else
import AppKit
#endif
import XCTest
@testable import ExactKit

/// `saveFile(id, text=…, suggestedName=…)` writes the text to a scratch file
/// for the panel or picker to copy from; that file holds the app's text, so
/// it is gone once the save ends, saved or cancelled.
///   bun host/apple/build.mjs --test --only SaveTextTests   (--ios for the picker)
final class SaveTextTests: XCTestCase {
    private var session: ExactSession?
    #if os(iOS)
    private var window: UIWindow?
    #else
    private var window: NSWindow?
    #endif

    override func tearDown() {
        session?.destroy()
        session = nil
        #if os(iOS)
        window?.isHidden = true
        #else
        window?.close()
        #endif
        window = nil
        super.tearDown()
    }

    private func booted() throws -> ExactSession {
        let plan = try fixturePlan("SAVE_TEXT")
        let session = ExactApp.shared.makeSession(label: "save-text")
        self.session = session
        let size = CGSize(width: 400, height: 300)
        #if os(iOS)
        let window = UIWindow(frame: CGRect(origin: .zero, size: size))
        let host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        let view = ExactView(session: session)
        view.frame = host.view.bounds
        host.view.addSubview(view)
        #else
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = ExactView(session: session)
        #endif
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        return session
    }

    /// The scratch directories `save` writes text into.
    private func scratch() -> Set<String> {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: FileManager.default.temporaryDirectory.path)) ?? []
        return Set(names.filter { $0.hasPrefix("exact-text-") })
    }

    private func save(_ session: ExactSession) {
        session.picker.save(["export-file", NSNull(), "notes.txt", "the exported notes"])
    }

    func testAnExportCopiesARegularFileAndRefusesLinks() throws {
        let session = try booted(), files = FileManager.default
        let ruling = session.runtime.command(["command": "saveFile", "agent": false, "id": "export-file",
                                              "text": "", "suggestedName": "saved.txt"])
        let view = try XCTUnwrap((ruling["view"] as? NSNumber)?.uint32Value)
        let reply = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"appRoots"}"#).utf8)) as? [String: Any])
        let roots = try XCTUnwrap(reply["roots"] as? [String: String])
        let name = UUID().uuidString
        let root = URL(fileURLWithPath: try XCTUnwrap(roots["tmp"]))
        let dir = root.appendingPathComponent(name)
        let outside = files.temporaryDirectory.appendingPathComponent("exact-export-test-\(name)")
        try files.createDirectory(at: dir.appendingPathComponent("plain"), withIntermediateDirectories: true)
        try files.createDirectory(at: outside, withIntermediateDirectories: true)
        defer { try? files.removeItem(at: dir); try? files.removeItem(at: outside) }
        let bytes = Data("exported contents".utf8)
        try bytes.write(to: dir.appendingPathComponent("plain/file.txt"))
        try Data("outside contents".utf8).write(to: outside.appendingPathComponent("file.txt"))
        for (link, target) in [("leaf-in", dir.appendingPathComponent("plain/file.txt")),
                               ("leaf-out", outside.appendingPathComponent("file.txt")),
                               ("ancestor-in", dir.appendingPathComponent("plain")), ("ancestor-out", outside)] {
            try files.createSymbolicLink(at: dir.appendingPathComponent(link), withDestinationURL: target)
        }
        let destination = outside.appendingPathComponent("saved.txt")
        func export(_ suffix: String) {
            session.picker.saveAnswered(["capability": "export", "node": view,
                                         "request": ["from": "app:/tmp/\(name)/\(suffix)"]],
                                        request: ["text": destination.path])
        }
        export("plain/file.txt")
        XCTAssertEqual(try Data(contentsOf: destination), bytes)
        try files.removeItem(at: destination)
        let opened = try XCTUnwrap(session.picker.appFile("app:/tmp/\(name)/plain/file.txt"))
        let original = dir.appendingPathComponent("plain/file.txt")
        try files.removeItem(at: original)
        try files.createSymbolicLink(at: original, withDestinationURL: outside.appendingPathComponent("file.txt"))
        session.picker.copyOut(view: view, file: opened, to: destination)
        XCTAssertEqual(try Data(contentsOf: destination), bytes, "the file opened before the panel showed")
        try files.removeItem(at: destination)
        try files.removeItem(at: original)
        try bytes.write(to: original)
        for suffix in ["leaf-in", "leaf-out", "ancestor-in/file.txt", "ancestor-out/file.txt"] {
            export(suffix)
            XCTAssertFalse(files.fileExists(atPath: destination.path), suffix)
        }
    }

    #if os(macOS)
    /// The panel's sheet holds the scratch file while it shows, and a
    /// cancel takes it.
    func testACancelledPanelLeavesNoScratchCopy() throws {
        let session = try booted()
        let before = scratch()
        save(session)
        let sheet = try XCTUnwrap(window?.attachedSheet, "the save panel's sheet")
        let made = scratch().subtracting(before)
        XCTAssertEqual(made.count, 1, "the text is written for the panel to copy")
        window?.endSheet(sheet, returnCode: .cancel)
        let deadline = Date().addingTimeInterval(5)
        while !scratch().isDisjoint(with: made), Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.01))
        }
        XCTAssertTrue(scratch().isDisjoint(with: made), "the scratch copy outlived the panel")
        XCTAssertTrue(session.agent(#"{"op":"logs"}"#).contains("saveFile: cancelled"))
    }
    #else
    /// The exporting picker reads only its own staged copy, so the text's
    /// scratch file goes as the picker is shown.
    func testTheExportingPickerKeepsNoScratchCopyOfTheText() throws {
        let session = try booted()
        let before = scratch()
        save(session)
        let staged = session.picker.exports.values.map(\.1)
        defer {
            staged.forEach { try? FileManager.default.removeItem(at: $0) }
            session.picker.exports.removeAll()
            window?.rootViewController?.presentedViewController?.dismiss(animated: false)
        }
        XCTAssertEqual(staged.count, 1, "the exporting picker is shown: \(session.agent(#"{"op":"logs"}"#))")
        XCTAssertTrue(staged.allSatisfy { FileManager.default.fileExists(atPath: $0.path) }, "its staged copy")
        XCTAssertEqual(scratch(), before, "the text's scratch copy outlived the picker's staging")
    }
    #endif
}
#endif
