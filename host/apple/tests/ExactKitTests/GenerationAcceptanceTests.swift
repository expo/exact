import Foundation
import CoreText
import CExact
import XCTest
#if os(macOS)
import AppKit
@testable import ExactImages
#endif
@testable import ExactKit

final class GenerationAcceptanceTests: XCTestCase {
    #if os(macOS)
    func testAReplacementReadsTheNewPublicationForARetainedImage() throws {
        _ = NSApplication.shared
        let app = ExactApp.shared
        let previous = app.lastPlan.map { ExactGeneration(plan: $0, assets: app.resolver, token: app.selectedToken, module: app.lastModule) }
        let name = "assets/cover.png"
        let plan = try TableFixture.plan("component App\n  view\n    image \"\(name)\" width=32 height=32\n")
        func assets(width: Int) throws -> AssetResolver {
            let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: 2,
                bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
            let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            return try AssetResolver(root: app.assetRoot, verified: [name: png])
        }
        let makeLoader = ImageLink.makeLoader
        ImageLink.makeLoader = { RasterLoader() }
        defer { ImageLink.makeLoader = makeLoader }
        app.installInitial(ExactGeneration(plan: plan, assets: try assets(width: 3)))
        let session = app.makeSession(label: "published-image")
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 80, height: 80), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer {
            session.destroy(); window.close()
            if let previous { app.installInitial(previous) } else {
                app.installInitial(ExactGeneration(plan: plan, assets: app.resolver, token: 754))
                _ = app.fallBackFromInitial(reason: "fixture finished")
            }
        }
        window.contentView = ExactView(session: session)
        window.orderFront(nil)
        XCTAssertNil(session.boot(size: window.contentLayoutRect.size).error)
        let image = try XCTUnwrap(session.presenter.views.values.first { $0.kind == "image" })
        waitFor("first publication decoded") { image.raster?.image.naturalSize.width == 3 }

        let unreadable = AssetResolver(root: app.assetRoot, names: [name]) { _ in throw CocoaError(.fileReadCorruptFile) }
        let result = app.applyGeneration(ExactGeneration(plan: plan, assets: unreadable)) {
            XCTFail("unreadable image reached durable commit"); return false
        }
        guard case .refused = result else { return XCTFail("expected a refusal, got \(result)") }
        XCTAssertNotNil(unreadable.refusal)
        XCTAssertEqual(image.raster?.image.naturalSize.width, 3)

        let replacement = try assets(width: 7)
        XCTAssertEqual(app.applyGeneration(ExactGeneration(plan: plan, assets: replacement)) { true }, .applied)
        XCTAssertTrue(session.presenter.views[image.id] === image)
        waitFor("replacement publication decoded") { image.raster?.image.naturalSize.width == 7 }
        let generation = image.loadGeneration
        let unchanged = try AssetResolver(root: app.assetRoot, verified: [name: XCTUnwrap(replacement.bytes(name))])
        XCTAssertEqual(app.applyGeneration(ExactGeneration(plan: plan, assets: unchanged)) { true }, .applied)
        XCTAssertEqual(image.loadGeneration, generation, "identical published bytes keep the decoded picture")

        let removed = try TableFixture.plan("component App\n  view\n    text \"Done\"\n")
        let unused = AssetResolver(root: app.assetRoot, names: [name]) { _ in
            XCTFail("a removed picture was read"); return nil
        }
        XCTAssertEqual(app.applyGeneration(ExactGeneration(plan: removed, assets: unused)) { true }, .applied)
        XCTAssertNil(session.presenter.views[image.id])
    }
    #endif

    func testRefusedPreparationDoesNotRegisterItsFontsInTheProcess() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let fixture = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("../../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf").standardized
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("candidate.ttf")
        try FileManager.default.copyItem(at: fixture, to: url)
        var registrationAttempted = false
        defer {
            if registrationAttempted { CTFontManagerUnregisterFontsForURL(url as CFURL, .process, nil) }
            try? FileManager.default.removeItem(at: directory)
        }
        let descriptors = try XCTUnwrap(CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor])
        let descriptor = try XCTUnwrap(descriptors.first)
        let name = CTFontCopyPostScriptName(CTFontCreateWithFontDescriptor(descriptor, 16, nil)) as String
        func available() -> Bool { (CTFontManagerCopyAvailablePostScriptNames() as? [String] ?? []).contains(name) }
        try XCTSkipIf(available(), "the fixture face is already installed in this process")
        registrationAttempted = true
        let app = ExactApp.shared, session = ExactApp.shared.makeSession(label: "refused-font")
        defer { session.destroy() }
        let text = TextEngine.pair(resolve: { _ in url }, read: { _ in try? Data(contentsOf: url) }, bundled: { _ in url })
        let family = Array("Candidate".utf8), source = Array("candidate.ttf".utf8)
        family.withUnsafeBufferPointer { family in source.withUnsafeBufferPointer { source in
            var face = ExactFontFace()
            face.family = family.baseAddress; face.family_len = family.count
            face.source = source.baseAddress; face.source_len = source.count
            face.stack = 8; face.weight = 400
            withUnsafePointer(to: &face) { face in
                var catalog = ExactFontCatalog(); catalog.faces = face; catalog.count = 1
                withUnsafePointer(to: &catalog) { text.install($0) }
            }
        } }
        session.configureCommit(ExactSession.Prepared(text: text, resolver: app.resolver, token: 0))
        session.cancelCommit()
        XCTAssertFalse(available(), "a refused generation must leave process font registration unchanged")
        text.commitFonts()
        XCTAssertTrue(available(), "the fixture registers when accepted")
    }

    func testAChangedSessionRefusesTheWholeCompositionBeforeDurableCommit() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let app = ExactApp.shared, plan = try fixturePlan("HEADLESS")
        let sessions = [app.makeSession(label: "generation-a"), app.makeSession(label: "generation-b")]
        defer { for session in sessions { session.destroy() } }
        for session in sessions {
            session.presenter.viewport.frame = CGRect(x: 0, y: 0, width: 200, height: 400)
            XCTAssertNil(session.boot(size: CGSize(width: 200, height: 400)).error)
            XCTAssertTrue(session.apply(plan))
            XCTAssertNotNil(session.prepare(plan, resolver: app.resolver))
        }
        let session = sessions[0]
        let box = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "box" })
        session.apply(session.runtime.press(box.id, now: session.now()))
        func slots() throws -> [[String: String]] {
            try sessions.map { session in
                let data = Data(session.agent(#"{"op":"state"}"#).utf8)
                let state = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
                return try XCTUnwrap(state["slots"] as? [String: String])
            }
        }
        let before = try slots()
        XCTAssertEqual(before.map { $0["suffix"] }, ["delta epsilon zeta", "beta gamma"])
        var durableCommits = 0
        let refused = Runtime.commitTogether(sessions.map(\.runtime)) { durableCommits += 1; return true }
        XCTAssertNil(refused.batches)
        XCTAssertTrue(refused.pending)
        XCTAssertEqual(durableCommits, 0)
        XCTAssertEqual(try slots(), before)
        for session in sessions { session.runtime.discardPlan() }
        XCTAssertEqual(app.applyGeneration(ExactGeneration(plan: plan, assets: app.resolver)) {
            XCTAssertTrue(Thread.isMainThread)
            durableCommits += 1; return true
        }, .applied)
        XCTAssertEqual(durableCommits, 1)
        XCTAssertEqual(try slots(), before)
    }
}
