// @exact/photos on Apple (LLP 1133.019): the photo library read and added
// to. Outside the agent PhotoKit answers (Library.swift); under the agent
// it asks PhotoKit for nothing and answers from the agent's library
// (AgentLibrary.swift, §10), the runner holding the permission tickets and
// laying the status it keeps over each answer.
//
// - A photo reaches the app as an id, a picture (`photos:<id>`, which an
//   `img` draws through the host's raster loader, D2) or a copy in
//   `app:/tmp/photos/` (emptied when the module is first made in a launch).
// - A photo that leaves what the app may read (the selection changed, the
//   access narrowed) has its pictures revoked: every `img` showing it is
//   emptied and fires `error` (D2).
// - `managePhotoSelection` is iOS's alone; macOS has no limited picker.
// - Tier 3 (D9): the three hatches below, each called on the main thread
//   for every request, after the module's own settings.
import ExactPackage
import Foundation
import ImageIO
import Photos

public final class PhotosModule: ExactPackage {
    /// Before each `photoAssets` fetch: the app may add predicates and
    /// `includeAssetSourceTypes`; the sort, the limit and the image
    /// predicate are the module's, and a page's key is joined to the
    /// app's predicate after this.
    public static var fetchOptions: ((PHFetchOptions) -> Void)?
    /// Before each picture request: `deliveryMode`, `isNetworkAccessAllowed`
    /// and `resizeMode` are the module's.
    public static var imageRequestOptions: ((PHImageRequestOptions) -> Void)?
    /// Inside an add's change block, after the module adds the photo's
    /// resource: the app may set `creationDate`, `location`, `isFavorite`.
    public static var creationRequest: ((PHAssetCreationRequest) -> Void)?

    let context: ExactPackageContext
    private let library = AgentLibrary()
    private lazy var photoKit = PhotoKitLibrary(context: context)
    /// Emptied once a launch.
    private static var emptied = false

    public init(context: ExactPackageContext) {
        self.context = context
        if !Self.emptied, let tmp = context.file("app:/tmp/photos/x") {
            Self.emptied = true
            try? FileManager.default.removeItem(at: tmp.deletingLastPathComponent())
        }
    }

    private func say(_ line: String) { context.log("photos: \(line)") }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        guard context.agent else {
            let origin = context.origin
            switch name {
            case "photoLibraryStatus": return reply(photoKit.status())
            case "photoAssets": return photoKit.assets(args, origin: origin, reply: reply)
            case "photoChanges": return photoKit.changes(args, origin: origin, reply: reply)
            case "copyPhotoAsset": return photoKit.copy(args, origin: origin, reply: reply)
            case "addToPhotoLibrary": return photoKit.add(args, origin: origin, reply: reply)
            case "managePhotoSelection": return photoKit.manage(origin: origin, reply: reply)
            default: return reply(["error": "NotSupportedError"])
            }
        }
        switch name {
        case "photoLibraryStatus": reply(status())
        case "photoAssets": reply(assets(args))
        case "photoChanges": reply(changes(args))
        case "copyPhotoAsset": copy(args, reply)
        case "addToPhotoLibrary": add(args, reply)
        case "managePhotoSelection":
            #if os(macOS)
            reply(["error": "NotSupportedError"])
            #else
            reply(status())
            #endif
        default: reply(["error": "NotSupportedError"])
        }
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] {
        guard name == "photoLibraryStatusSync" || name == "photoLibraryStatus" else { return ["error": "NotSupportedError"] }
        return context.agent ? status() : photoKit.status()
    }

    public func foreground(_ shown: Bool) {
        if shown, !context.agent { photoKit.recheck() }
    }

    public func destroy(_ done: @escaping () -> Void) {
        if !context.agent { photoKit.stop() }
        done()
    }

    /// Under the agent: the access the runner's status gives, the selection
    /// and the fixtures deleted; a photo that left what the app may read is
    /// revoked.
    public func preferred(_ values: [String: Any]) {
        guard context.agent else { return }
        change { library in
            if let access = (values["photoLibraryStatus"] as? [String: Any])?["access"] as? String { library.access = access }
            library.selection = (values["selection"] as? [String]) ?? AgentLibrary.defaultSelection
            library.deleted = Set((values["deleted"] as? [String]) ?? [])
        }
    }

    public func permissions(_ states: [String: String]) {
        // Denied reading: nothing is readable, whatever the status said before.
        if context.agent, states["photos"] == "denied" { change { $0.access = "none" } }
    }

    /// The agent's library changed by `edit`: a photo that left what the app
    /// may read is revoked, and the readers of what moved are announced.
    private func change(_ edit: (AgentLibrary) -> Void) {
        let before = (library.visible, library.access, library.epoch, library.journal.count)
        edit(library)
        let after = library.visible
        for id in before.0.subtracting(after).sorted() {
            say("revoked photos:\(id)")
            context.revokePicture("photos:\(id)")
        }
        if before.0 != after { context.changed("photos.photoAssets") }
        if before != (after, library.access, library.epoch, library.journal.count) { context.changed("photos.photoChanges") }
    }

    public func image(_ source: String, width: Int, height: Int, fit: String, owned: Bool, reply: @escaping (CGImage?, String?) -> Void) {
        let id = String(source.dropFirst("photos:".count))
        guard context.agent else { return photoKit.picture(id, width: width, height: height, fit: fit, reply: reply) }
        guard let asset = library.asset(id) else {
            say("picture \(id) at \(width)x\(height): NotFoundError")
            return reply(nil, "NotFoundError")
        }
        DispatchQueue.global(qos: .userInitiated).async { [library] in
            let image = library.picture(asset, width: width, height: height, fit: fit)
            reply(image, image == nil ? "UnknownError" : nil)
        }
        say("picture \(id) at \(width)x\(height) \(fit)")
    }

    // MARK: the agent's answers (§10)

    private func status() -> [String: Any] {
        ["available": true, "state": "prompt", "access": library.access, "addState": "prompt", "apple": NSNull(), "android": NSNull()]
    }

    private func assets(_ args: [String: Any]) -> [String: Any] {
        let limit = min(500, max(1, (args["limit"] as? NSNumber)?.intValue ?? 60))
        let after: PageCursor.Key?
        do { after = try PageCursor.key("agent", args["cursor"]) } catch { return PageCursor.refused }
        let screenshots = (args["subtypes"] as? [String])?.contains("screenshot") == true
        let (page, more) = library.page(limit, after: after, screenshots: screenshots)
        let cursor = more ? page.last.map { PageCursor.text("agent", .init(date: $0.created, id: $0.id)) } : nil
        return ["assets": page.map(Self.asset), "cursor": cursor ?? NSNull()]
    }

    private static func asset(_ a: AgentLibrary.Asset) -> [String: Any] {
        ["id": a.id, "mediaType": "image", "width": a.width, "height": a.height, "creationDate": a.created,
         "favorite": false, "picture": "photos:\(a.id)",
         "apple": ["mediaSubtypes": a.screenshot ? ["photoScreenshot"] : [String](), "burstIdentifier": NSNull(), "hasAdjustments": false], "android": NSNull()]
    }

    /// The agent's journal from `since` (LLP 1133.019.000 D1, D5). A walk
    /// fixes its end on its first page; each page after takes up after the
    /// last photo the one before answered.
    private func changes(_ args: [String: Any]) -> [String: Any] {
        let count = library.journal.count
        let here = ChangeToken(backend: "agent", access: library.access, subtypes: ChangeToken.subtypes(args), epoch: library.epoch, at: String(count))
        let given: ChangeToken?
        do { given = try ChangeToken.parse(args["since"], backend: "agent") } catch { return ChangeToken.refused }
        guard let since = given else { return here.answer() }
        // The agent answers no rescan-only token, and each of its tokens names its store and a position.
        let fixed = since.end.map { Int($0) }
        guard since.epoch != nil, let from = since.at.flatMap({ Int($0) }), from >= 0, fixed.map({ ($0 ?? -1) >= from }) ?? true else {
            return ChangeToken.refused
        }
        guard here.sameView(since), since.epoch == library.epoch else { return here.answer(expired: true) }
        // Positions in this store: a walk's end, else now.
        let end = fixed.flatMap { $0 } ?? count
        guard from <= end, end <= count, let page = library.changes(from: from, end: end, after: since.after, limit: ChangeToken.limit(args),
                                                                    screenshots: here.subtypes.contains("screenshot")) else { return ChangeToken.refused }
        var next = here
        next.at = String(page.more ? from : end)
        if page.more { (next.end, next.after) = (String(end), page.last) }
        return next.answer(added: page.upserts.filter(\.inserted).map { Self.asset($0.asset) },
                           updated: page.upserts.filter { !$0.inserted }.map { Self.asset($0.asset) },
                           deleted: page.removed, more: page.more)
    }

    private func copy(_ args: [String: Any], _ reply: @escaping ([String: Any]) -> Void) {
        guard let id = args["asset"] as? String, let asset = library.asset(id) else { return reply(["error": "NotFoundError"]) }
        let ext = asset.file.pathExtension
        let path: String
        if let p = args["path"] as? String, !p.isEmpty { path = p.hasSuffix(".\(ext)") ? p : "\(p).\(ext)" } else { path = "app:/tmp/photos/\(UUID().uuidString).\(ext)" }
        guard let url = context.file(path), let data = try? Data(contentsOf: asset.file) else { return reply(["error": "UnknownError"]) }
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: url)
        } catch { return reply(["error": "UnknownError"]) }
        say("copied \(id) to \(path) sha256 \(AgentLibrary.sha256(data))")
        reply(["path": path, "name": asset.file.lastPathComponent, "type": asset.type, "size": data.count,
               "width": asset.width, "height": asset.height, "duration": NSNull(), "android": NSNull()])
    }

    private func add(_ args: [String: Any], _ reply: @escaping ([String: Any]) -> Void) {
        guard let path = args["path"] as? String, let url = context.file(path), let data = try? Data(contentsOf: url) else {
            return reply(["error": "NotFoundError"])
        }
        guard let directory = context.file("app:/tmp/photos-library/x")?.deletingLastPathComponent(),
              let asset = library.add(data, directory: directory, now: context.now()) else { return reply(["error": "DataError"]) }
        say("added \(asset.id) from \(path) sha256 \(AgentLibrary.sha256(data)) \(asset.width)x\(asset.height) \(asset.type)")
        context.changed("photos.photoAssets")
        context.changed("photos.photoChanges")
        // The new photo's id where the app may read it (D6): not under add-only access.
        reply(["asset": library.visible.contains(asset.id) ? asset.id : NSNull()])
    }
}

/// `photoAssets`' cursor (LLP 1133.019 D1), opaque to the app: the sort key
/// of the last photo a page answered, tagged with the library that sorted
/// it. The next page answers what sorts after the key, so photos added
/// newer do not move it, and a key whose photo is gone still places it.
enum PageCursor {
    struct Key {
        /// The creation date in the library's own unit; nil sorts last.
        let date: Double?
        let id: String
    }

    struct Malformed: Error {}

    static let refused: [String: Any] = ["error": "SyntaxError", "message": "not a cursor photoAssets answered here"]

    static func text(_ library: String, _ key: Key) -> String {
        // A Double's description reads back as the same Double.
        Data("\(library)\n\(key.date.map { "\($0)" } ?? "")\n\(key.id)".utf8).base64EncodedString()
    }

    /// Nil for the first page: no cursor, or an empty one.
    static func key(_ library: String, _ given: Any?) throws -> Key? {
        guard let given = given as? String, !given.isEmpty else { return nil }
        guard let data = Data(base64Encoded: given), let text = String(data: data, encoding: .utf8) else { throw Malformed() }
        let parts = text.split(separator: "\n", maxSplits: 2, omittingEmptySubsequences: false)
        guard parts.count == 3, parts[0] == library, !parts[2].isEmpty else { throw Malformed() }
        if parts[1].isEmpty { return Key(date: nil, id: String(parts[2])) }
        guard let date = Double(parts[1]), date.isFinite else { throw Malformed() }
        return Key(date: date, id: String(parts[2]))
    }
}
