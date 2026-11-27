import XCTest
@testable import ExactCore
@testable import ExactKit
@testable import ExactNativeViews

final class ModuleAssetTests: XCTestCase {
    private func callHatchAsset(host: UnsafeMutableRawPointer?, path: String, table: UnsafeRawPointer = hatchHostTable) -> URL? {
        guard table.load(as: UInt32.self) >= 216,
              let raw = table.load(fromByteOffset: 208, as: UnsafeRawPointer?.self) else { return nil }
        let fn = unsafeBitCast(raw, to: HatchAssetFn.self)
        let input = Array(path.utf8)
        var cap = 16 // intentionally small to exercise two-call resize
        for _ in 0..<3 {
            var buf = [UInt8](repeating: 0, count: cap)
            let needed = input.withUnsafeBufferPointer { s in
                buf.withUnsafeMutableBufferPointer { d in
                    Int(fn(host, s.baseAddress, UInt32(s.count), d.baseAddress, UInt32(d.count)))
                }
            }
            guard needed > 0 else { return nil }
            if needed <= cap { return String(bytes: buf.prefix(needed), encoding: .utf8).map(URL.init(fileURLWithPath:)) }
            cap = needed
        }
        return nil
    }

    func testEmbeddedResolverRejectsMissingAndDirectories() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exact-asset-emb-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("assets/sub", isDirectory: true), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("assets/ding.txt")
        try Data("hello".utf8).write(to: file)

        let resolver = AssetResolver(root: dir)
        XCTAssertEqual(resolver.url("assets/ding.txt")?.resolvingSymlinksInPath(), file.resolvingSymlinksInPath())
        XCTAssertNil(resolver.url("assets/missing.txt"))
        XCTAssertNil(resolver.url("assets/sub"))
        XCTAssertNil(resolver.bundledURL("assets/sub"))
        XCTAssertNil(resolver.url("assets/../Info.plist"))
    }

    func testBridgeCanonicalizesAssetsNamespaceAndBlocksInternalFiles() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exact-asset-bridge-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("assets", isDirectory: true), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("typescript", isDirectory: true), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let assetFile = dir.appendingPathComponent("assets/clip.bin")
        try Data("clip".utf8).write(to: assetFile)
        try Data("secret".utf8).write(to: dir.appendingPathComponent("Info.plist"))
        try Data("bytecode".utf8).write(to: dir.appendingPathComponent("typescript/app.hbc"))

        let resolver = AssetResolver(root: dir)
        let host = UnsafeMutableRawPointer(bitPattern: 0x1067_0001)!
        registerHatchAssetResolver(host: host) { name in resolver.url(name, maximumBytes: 1024 * 1024, cacheBytes: false) }
        defer { unregisterHatchAssetResolver(host: host) }

        XCTAssertEqual(callHatchAsset(host: host, path: "clip.bin")?.resolvingSymlinksInPath(), assetFile.resolvingSymlinksInPath())
        XCTAssertEqual(callHatchAsset(host: host, path: "assets/clip.bin")?.resolvingSymlinksInPath(), assetFile.resolvingSymlinksInPath())
        XCTAssertNil(callHatchAsset(host: host, path: "Info.plist"))
        XCTAssertNil(callHatchAsset(host: host, path: "typescript/app.hbc"))
        XCTAssertNil(callHatchAsset(host: host, path: "../Info.plist"))
        XCTAssertNil(callHatchAsset(host: host, path: "assets/missing.bin"))
    }

    func testOTAMaterializationWorksOffMainThreadAndStaysPinnedAcrossSwap() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exact-asset-ota-\(UUID().uuidString)", isDirectory: true)
        var reads = 0
        let payload = Data("verified-ota-bytes".utf8)
        let gen1 = AssetResolver(root: dir, names: ["assets/model.bin"], read: { name in
            reads += 1
            return name == "assets/model.bin" ? payload : nil
        })
        let gen2 = AssetResolver(root: dir, names: ["assets/model.bin"], read: { name in
            return name == "assets/model.bin" ? Data("gen2-bytes".utf8) : nil
        })

        let lock = NSLock()
        var current: AssetResolver = gen1
        var pinned: [ObjectIdentifier: AssetResolver] = [:]
        let host = UnsafeMutableRawPointer(bitPattern: 0x1067_0002)!
        registerHatchAssetResolver(host: host) { name in
            let r = lock.withLock { current }
            guard let url = r.url(name, maximumBytes: 1024 * 1024, cacheBytes: false) else { return nil }
            if r.isComplete { lock.withLock { pinned[ObjectIdentifier(r)] = r } }
            return url
        }
        defer { unregisterHatchAssetResolver(host: host) }

        let exp = expectation(description: "background resolve")
        var resolved1: URL?
        DispatchQueue.global(qos: .userInitiated).async {
            resolved1 = self.callHatchAsset(host: host, path: "model.bin")
            exp.fulfill()
        }
        wait(for: [exp], timeout: 5.0)
        let url1 = try XCTUnwrap(resolved1)
        XCTAssertEqual(try Data(contentsOf: url1), payload)
        XCTAssertEqual(reads, 1)

        // Swap to gen2: url1 stays readable because gen1 is pinned until teardown.
        lock.withLock { current = gen2 }
        XCTAssertEqual(try Data(contentsOf: url1), payload)
        let url2 = try XCTUnwrap(callHatchAsset(host: host, path: "assets/model.bin"))
        XCTAssertEqual(try Data(contentsOf: url2), Data("gen2-bytes".utf8))
    }

    func testHatchesOffTableZeroesUISlotsAndKeepsAssetSlot() throws {
        XCTAssertEqual(hatchHostTableOff.load(as: UInt32.self), 216)
        for offset in stride(from: 8, to: 208, by: 8) {
            XCTAssertNil(hatchHostTableOff.load(fromByteOffset: offset, as: UnsafeRawPointer?.self), "slot \(offset) must be nil when hatches are off")
        }
        XCTAssertNotNil(hatchHostTableOff.load(fromByteOffset: 208, as: UnsafeRawPointer?.self))

        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exact-asset-off-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("assets", isDirectory: true), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("assets/tone.wav")
        try Data("wav".utf8).write(to: file)
        let resolver = AssetResolver(root: dir)
        let host = UnsafeMutableRawPointer(bitPattern: 0x1067_0003)!
        registerHatchAssetResolver(host: host) { resolver.url($0, maximumBytes: 1024 * 1024, cacheBytes: false) }
        defer { unregisterHatchAssetResolver(host: host) }

        XCTAssertEqual(callHatchAsset(host: host, path: "tone.wav", table: hatchHostTableOff)?.resolvingSymlinksInPath(), file.resolvingSymlinksInPath())
    }
}
