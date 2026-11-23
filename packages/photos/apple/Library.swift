// @exact/photos's PhotoKit half (LLP 1133.019 stage 2): what the module
// does outside the agent, or under it when a drive binds it (`--bind
// photos`). Nothing of PhotoKit is touched before the module's first call
// (D19), and the change observer is registered only once reading is
// granted, since a library call before that may raise the system's prompt.
//
// - The state is a fact (D5): `authorizationStatus(for:)` at each level,
//   read again at each change, at a return to the foreground and after a
//   prompt; a change announces the status and the list.
// - A read while `notDetermined` asks read-write first (D7); an add asks the
//   narrowest level the app declares: add-only where the build carries the
//   add key (`device.photos-add`), else read-write. Only the app's own
//   foreground code or a person asks: any other origin is `person-required`.
// - A picture is `PHCachingImageManager`'s, delivered opportunistically: the
//   quick picture answers the host's ask, and the final one is kept and the
//   picture announced changed, so the host's next ask takes it (D3).
// - A photo the app may no longer read (out of the selection, deleted, or
//   reading withdrawn) has its pictures revoked (D2).
import ExactPackage
import Foundation
import ImageIO
import Photos
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
#else
import PhotosUI
import UIKit
#endif

final class PhotoKitLibrary: NSObject, PHPhotoLibraryChangeObserver {
    private let context: ExactPackageContext
    private var observing = false
    private lazy var images = PHCachingImageManager()
    /// The ids whose pictures were asked, so one that leaves what the app may
    /// read is revoked.
    private var drawn: Set<String> = []
    /// Final pictures that came after a quick one, by id, size and fit,
    /// taken by the host's next ask; the oldest go past `finalsKept`.
    private var finals: [(key: String, image: CGImage)] = []
    private let finalsKept = 32
    /// The status last answered, to announce a change.
    private var said: [String: String]?

    init(context: ExactPackageContext) {
        self.context = context
        super.init()
    }

    private func say(_ line: String) { context.log("photos: \(line)") }

    // MARK: the fact (D5)

    private func level(_ l: PHAccessLevel) -> PHAuthorizationStatus { PHPhotoLibrary.authorizationStatus(for: l) }

    private var readable: Bool { [.authorized, .limited].contains(level(.readWrite)) }

    private static func word(_ s: PHAuthorizationStatus) -> String {
        switch s {
        case .notDetermined: "notDetermined"
        case .restricted: "restricted"
        case .denied: "denied"
        case .limited: "limited"
        default: "authorized"
        }
    }

    private func facts() -> [String: String] {
        let rw = level(.readWrite), add = level(.addOnly)
        let (state, access): (String, String) = switch rw {
        case .authorized: ("granted", "full")
        case .limited: ("granted", "limited")
        case .notDetermined: ("prompt", "none")
        default: ("denied", "none")
        }
        let added: String = switch add {
        case .authorized, .limited: "granted"
        case .notDetermined: "prompt"
        default: "denied"
        }
        // A read-write grant includes adding.
        let addState = state == "granted" ? "granted" : added
        return ["state": state, "access": access, "addState": addState,
                "authorizationStatus": Self.word(rw), "addOnlyAuthorizationStatus": Self.word(add)]
    }

    func status() -> [String: Any] {
        let f = facts()
        said = f
        return ["available": true, "state": f["state"]!, "access": f["access"]!, "addState": f["addState"]!,
                "apple": ["authorizationStatus": f["authorizationStatus"]!, "addOnlyAuthorizationStatus": f["addOnlyAuthorizationStatus"]!],
                "android": NSNull()]
    }

    /// The status changed since it was answered: its readers and the list's
    /// are asked again, and with reading withdrawn every picture is revoked.
    func recheck() {
        guard let said, said != facts() else { return }
        self.said = facts()
        say("status changed: \(self.said!["authorizationStatus"]!), add \(self.said!["addOnlyAuthorizationStatus"]!)")
        context.changed("photos.photoLibraryStatus")
        context.changed("photos.photoAssets")
        context.changed("photos.photoChanges")
        if !readable { revoke(drawn) }
        observe()
    }

    /// `then` with `l`'s answer: asked first while undetermined (D7).
    private func authorize(_ l: PHAccessLevel, _ then: @escaping (PHAuthorizationStatus) -> Void) {
        let now = level(l)
        guard now == .notDetermined else { return then(now) }
        say("asks \(l == .addOnly ? "add-only" : "read-write") access")
        PHPhotoLibrary.requestAuthorization(for: l) { answer in
            DispatchQueue.main.async { [self] in
                recheck()
                then(answer)
            }
        }
    }

    /// `then` while the app may read the library; asked first while
    /// undetermined, by the app's own code or a person only (D7).
    private func whenReadable(origin: String, _ reply: @escaping ([String: Any]) -> Void, _ then: @escaping () -> Void) {
        // A reader of the list or its changes alone learns of a change made in Settings too.
        if said == nil { said = facts() }
        switch level(.readWrite) {
        case .authorized, .limited: then()
        case .notDetermined where origin != "app": reply(["error": "NotAllowedError", "message": "person-required"])
        case .notDetermined:
            authorize(.readWrite) { [self] _ in readable ? then() : reply(["error": "NotAllowedError"]) }
        default: reply(["error": "NotAllowedError"])
        }
    }

    // MARK: changes (D6)

    private func observe() {
        guard !observing, readable else { return }
        observing = true
        PHPhotoLibrary.shared().register(self)
    }

    func stop() {
        if observing { PHPhotoLibrary.shared().unregisterChangeObserver(self) }
        observing = false
    }

    func photoLibraryDidChange(_ change: PHChange) {
        DispatchQueue.main.async { [self] in
            recheck()
            context.changed("photos.photoAssets")
            context.changed("photos.photoChanges")
            guard readable, !drawn.isEmpty else { return }
            let found = PHAsset.fetchAssets(withLocalIdentifiers: Array(drawn), options: nil)
            var still = Set<String>()
            found.enumerateObjects { asset, _, _ in
                still.insert(asset.localIdentifier)
                if change.changeDetails(for: asset)?.assetContentChanged == true {
                    self.finals.removeAll { $0.key.hasPrefix(asset.localIdentifier + " ") }
                    self.context.pictureChanged("photos:\(asset.localIdentifier)")
                }
            }
            revoke(drawn.subtracting(still))
        }
    }

    private func revoke(_ ids: Set<String>) {
        for id in ids.sorted() {
            drawn.remove(id)
            finals.removeAll { $0.key.hasPrefix(id + " ") }
            say("revoked photos:\(id)")
            context.revokePicture("photos:\(id)")
        }
    }

    // MARK: the list

    private static let subtypes: [(PHAssetMediaSubtype, String)] = [
        (.photoPanorama, "photoPanorama"), (.photoHDR, "photoHDR"), (.photoScreenshot, "photoScreenshot"),
        (.photoLive, "photoLive"), (.photoDepthEffect, "photoDepthEffect"),
    ]

    func assets(_ args: [String: Any], origin: String, reply: @escaping ([String: Any]) -> Void) {
        whenReadable(origin: origin, reply) { [self] in
            observe()
            let limit = min(500, max(1, (args["limit"] as? NSNumber)?.intValue ?? 60))
            let after: PageCursor.Key?
            do { after = try PageCursor.key("photokit", args["cursor"]) } catch { return reply(PageCursor.refused) }
            let options = PHFetchOptions()
            let sort = [NSSortDescriptor(key: "creationDate", ascending: false)]
            let images = NSPredicate(format: "mediaType == %d", PHAssetMediaType.image.rawValue)
            options.sortDescriptors = sort
            options.fetchLimit = limit + 1
            options.predicate = images
            var predicates = [images]
            if let only = Self.subtypePredicate(args) { predicates.append(only) }
            // The app adds predicates (D9); the module keeps its own.
            if let hatch = PhotosModule.fetchOptions {
                hatch(options)
                if options.sortDescriptors != sort { say("the fetchOptions hatch changed sortDescriptors, which the module owns (LLP 1133.019 D9)") }
                if options.fetchLimit != limit + 1 { say("the fetchOptions hatch changed fetchLimit, which the module owns (LLP 1133.019 D9)") }
                if let theirs = options.predicate, theirs != images { predicates.append(theirs) }
            }
            DispatchQueue.global(qos: .userInitiated).async {
                let (page, more) = Self.page(limit, after: after, options: options, predicates: predicates)
                let out = page.map(Self.json)
                let cursor = more ? page.last.map {
                    PageCursor.text("photokit", .init(date: $0.creationDate?.timeIntervalSinceReferenceDate, id: $0.localIdentifier))
                } : nil
                DispatchQueue.main.async { reply(["assets": out, "cursor": cursor ?? NSNull()]) }
            }
        }
    }

    /// A `PhotoAsset`, read on the worker that fetched it.
    private static func json(_ a: PHAsset) -> [String: Any] {
        let adjusted = PHAssetResource.assetResources(for: a).contains { $0.type == .adjustmentData }
        return [
            "id": a.localIdentifier, "mediaType": "image", "width": a.pixelWidth, "height": a.pixelHeight,
            "creationDate": a.creationDate.map { $0.timeIntervalSince1970 * 1000 } ?? NSNull(),
            "favorite": a.isFavorite, "picture": "photos:\(a.localIdentifier)",
            "apple": ["mediaSubtypes": subtypes.filter { a.mediaSubtypes.contains($0.0) }.map(\.1),
                      "burstIdentifier": a.burstIdentifier as Any? ?? NSNull(), "hasAdjustments": adjusted],
            "android": NSNull(),
        ]
    }

    /// `subtypes`' predicate (LLP 1133.019.000 D4), nil for every image:
    /// `screenshot` is the `photoScreenshot` bit.
    static func subtypePredicate(_ args: [String: Any]) -> NSPredicate? {
        guard (args["subtypes"] as? [String])?.contains("screenshot") == true else { return nil }
        return NSPredicate(format: "(mediaSubtypes & %ld) != 0", Int(PHAssetMediaSubtype.photoScreenshot.rawValue))
    }

    /// Whether the view walks PhotoKit's persistent history (LLP 1133.019.000
    /// D2): full access with no fetch hatch. Under limited access a selection
    /// can change while the app is closed with no history to recover it, and a
    /// hatch shapes a list the history does not know, so either view is
    /// rescan-only.
    private var history: Bool { level(.readWrite) == .authorized && PhotosModule.fetchOptions == nil }

    /// What changed since `since` (LLP 1133.019.000 D1, D2). A rescan-only
    /// view answers any token `expired` with a fresh one, and the app replaces
    /// its index from `photoAssets`. A history walk fixes its end on its first
    /// page; a continuation finds that end again before it answers. PhotoKit's
    /// tokens are read and written on a worker; the view is checked again on
    /// main before the reply.
    func changes(_ args: [String: Any], origin: String, reply: @escaping ([String: Any]) -> Void) {
        whenReadable(origin: origin, reply) { [self] in
            observe()
            let subtypes = ChangeToken.subtypes(args)
            let view = { [self] in ChangeToken(backend: "photokit", access: facts()["access"]!, subtypes: subtypes) }
            let here = view()
            let given: ChangeToken?
            do { given = try ChangeToken.parse(args["since"], backend: "photokit") } catch { return reply(ChangeToken.refused) }
            // PhotoKit's positions are issued under full access alone, and never with an agent's store.
            if let given, given.epoch != nil || (!given.rescan && given.access != "full") { return reply(ChangeToken.refused) }
            let walking = history
            let (limit, screenshots) = (ChangeToken.limit(args), subtypes.contains("screenshot"))
            DispatchQueue.global(qos: .userInitiated).async {
                let made = Self.answer(since: given, view: here, history: walking, limit: limit, screenshots: screenshots)
                DispatchQueue.main.async { [self] in
                    guard readable else { return reply(["error": "NotAllowedError"]) }
                    // The view the answer was made for is still the app's; if not, the token expires into the new one.
                    let now = view()
                    guard history == walking, now.access == here.access else {
                        guard history else { return reply(now.answer(expired: given != nil)) }
                        return DispatchQueue.global(qos: .userInitiated).async {
                            let made = Self.checkpoint(now, expired: true)
                            DispatchQueue.main.async { [self] in send(made, reply) }
                        }
                    }
                    send(made, reply)
                }
            }
        }
    }

    /// What the walk made: an answer, and what to log of it.
    private struct Made {
        var answer: [String: Any]
        var note: String? = nil
    }

    private func send(_ made: Made, _ reply: ([String: Any]) -> Void) {
        if let note = made.note { say("photoChanges \(note)") }
        reply(made.answer)
    }

    /// `view`'s checkpoint at the library's newest change.
    private static func checkpoint(_ view: ChangeToken, expired: Bool, why: String? = nil) -> Made {
        guard let at = archive(PHPhotoLibrary.shared().currentChangeToken) else { return unarchivable }
        var t = view
        t.at = at
        return Made(answer: t.answer(expired: expired), note: why.map { "expired: \($0)" })
    }

    private static let unarchivable = Made(answer: ["error": "UnknownError"], note: "failed: UnknownError, a change token that does not archive")

    /// The answer for `since` in `view`, made on a worker: a rescan-only view
    /// expires every token; a walk's page otherwise.
    private static func answer(since: ChangeToken?, view: ChangeToken, history: Bool, limit: Int, screenshots: Bool) -> Made {
        // A token's positions are PhotoKit's own archives.
        let start = since?.at.map(unarchive), end = since?.end.map(unarchive)
        guard start != .some(nil), end != .some(nil) else { return Made(answer: ChangeToken.refused) }
        guard history else { return Made(answer: view.answer(expired: since != nil)) }
        guard let since else { return checkpoint(view, expired: false) }
        guard view.sameView(since), let start = start ?? nil else { return checkpoint(view, expired: true) }
        switch walk(from: start, through: end ?? nil, after: since.after, limit: limit, screenshots: screenshots) {
        case let .page(upserts, removed, last, through, more):
            guard let through = archive(through) else { return unarchivable }
            var next = view
            if more { (next.at, next.end, next.after) = (since.at, through, last) } else { next.at = through }
            return Made(answer: next.answer(added: upserts.filter(\.inserted).map(\.json), updated: upserts.filter { !$0.inserted }.map(\.json),
                                            deleted: removed, more: more))
        case let .expired(why): return checkpoint(view, expired: true, why: why)
        case let .failed(name, why): return Made(answer: ["error": name], note: "failed: \(name), \(why)")
        }
    }

    private enum Page {
        case page(upserts: [(json: [String: Any], inserted: Bool)], removed: [String], last: String?, through: NSObject, more: Bool)
        case expired(String)
        case failed(String, String)
    }

    /// One page of the changes after `start` through `saved` (the walk's end;
    /// nil on its first page, which takes the newest change's): D2's fold of
    /// each asset changed into one candidate, by `localIdentifier` after
    /// `after`, at most `limit`, read now with the list's rules. A candidate
    /// the list holds is an upsert, `inserted` when its first change inserted
    /// it; any other is a removal.
    private static func walk(from start: PHPersistentChangeToken, through saved: PHPersistentChangeToken?, after: String?,
                             limit: Int, screenshots: Bool) -> Page {
        let changes: PHPersistentChangeFetchResult
        do { changes = try PHPhotoLibrary.shared().fetchPersistentChanges(since: start) } catch { return failure(error) }
        var first: [String: Bool] = [:], end = start, reached = saved == nil
        for change in changes {
            do {
                let details = try change.changeDetails(for: .asset)
                for (ids, insert) in [(details.insertedLocalIdentifiers, true), (details.updatedLocalIdentifiers, false),
                                      (details.deletedLocalIdentifiers, false)] {
                    for id in ids where first[id] == nil { first[id] = insert }
                }
            } catch { return failure(error) }
            end = change.changeToken
            if let saved, change.changeToken.isEqual(saved) {
                reached = true
                break
            }
        }
        // The saved end, found again by comparing tokens: a count of changes cannot identify it.
        guard reached else { return .expired("the walk's end is not in the history") }
        // `after` only places the slice: PhotoKit leaves a photo deleted since out of the records before its
        // deletion, so a window loses it, and its removal comes in the next walk.
        let ids = first.keys.filter { id in after.map { id > $0 } ?? true }.sorted()
        let page = Array(ids.prefix(limit))
        // The list's own fetch, whose defaults (hidden photos, source types) a fetch by identifiers does not share.
        let options = PHFetchOptions()
        var predicates = [NSPredicate(format: "mediaType == %d", PHAssetMediaType.image.rawValue), NSPredicate(format: "localIdentifier IN %@", page)]
        if screenshots, let only = subtypePredicate(["subtypes": ["screenshot"]]) { predicates.append(only) }
        options.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
        let found = PHAsset.fetchAssets(with: options)
        var listed: [String: PHAsset] = [:]
        found.enumerateObjects { asset, _, _ in listed[asset.localIdentifier] = asset }
        return .page(upserts: page.compactMap { id in listed[id].map { (json($0), first[id]!) } },
                     removed: page.filter { listed[$0] == nil }, last: page.last, through: end, more: ids.count > page.count)
    }

    /// D1's names for a failed fetch: a token PhotoKit can no longer walk from
    /// expires; a cancelled fetch is `AbortError` and a refused one
    /// `NotAllowedError`. The error is named by its domain and code, which
    /// carry no photo or token.
    private static func failure(_ error: Error) -> Page {
        let e = error as NSError, why = "\(e.domain) \(e.code)"
        let photos = e.domain == PHPhotosErrorDomain ? PHPhotosError.Code(rawValue: e.code) : nil
        switch photos {
        case .persistentChangeTokenExpired, .persistentChangeDetailsUnavailable: return .expired(why)
        case .userCancelled: return .failed("AbortError", why)
        case .accessRestricted, .accessUserDenied: return .failed("NotAllowedError", why)
        default: return .failed(e.domain == NSCocoaErrorDomain && e.code == NSUserCancelledError ? "AbortError" : "UnknownError", why)
        }
    }

    private static func archive(_ token: NSObject) -> String? {
        (try? NSKeyedArchiver.archivedData(withRootObject: token, requiringSecureCoding: true))?.base64EncodedString()
    }

    /// A token PhotoKit archived, or nil for text that is not one.
    private static func unarchive(_ text: String) -> PHPersistentChangeToken? {
        guard let data = Data(base64Encoded: text) else { return nil }
        return try? NSKeyedUnarchiver.unarchivedObject(ofClass: PHPersistentChangeToken.self, from: data)
    }

    /// The page after `after` in the list's order: newest first, the
    /// `localIdentifier` ordering photos of one date (descending), and the
    /// photos with no date last. PhotoKit sorts by `creationDate` but not by
    /// `localIdentifier`, so the module orders each date itself: where a page
    /// would end inside a run of photos sharing a date, that run is fetched
    /// whole and the page takes its first ids. Every fetch is the module's
    /// predicates and the app's, `options` carrying the rest of the hatch's.
    private static func page(_ limit: Int, after: PageCursor.Key?, options: PHFetchOptions, predicates: [NSPredicate]) -> ([PHAsset], Bool) {
        /// Newest first, at most `first` (0: every one).
        func fetch(_ more: NSPredicate, first: Int) -> [PHAsset] {
            let o = options.copy() as! PHFetchOptions
            o.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates + [more])
            o.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
            o.fetchLimit = first
            let found = PHAsset.fetchAssets(with: o)
            return found.objects(at: IndexSet(integersIn: 0..<found.count))
        }
        /// The photos of one date (nil: no date), ids descending; only those
        /// below `id` when it is given.
        func run(_ date: Date?, below id: String?) -> [PHAsset] {
            var same = date.map { NSPredicate(format: "creationDate == %@", $0 as NSDate) } ?? NSPredicate(format: "creationDate == nil")
            if let id { same = NSCompoundPredicate(andPredicateWithSubpredicates: [same, NSPredicate(format: "localIdentifier < %@", id)]) }
            return fetch(same, first: 0).sorted { $0.localIdentifier > $1.localIdentifier }
        }
        let afterDate = after?.date.map { Date(timeIntervalSinceReferenceDate: $0) }
        var page: [PHAsset] = []
        var more = false
        if after == nil || afterDate != nil {
            var dated = NSPredicate(format: "creationDate != nil")
            if let after, let afterDate {
                dated = NSPredicate(format: "creationDate < %@ OR (creationDate == %@ AND localIdentifier < %@)", afterDate as NSDate, afterDate as NSDate, after.id)
            }
            // One photo past the page says whether an older page follows.
            let found = fetch(dated, first: limit + 1)
            if found.count > limit, found[limit - 1].creationDate == found[limit].creationDate {
                let date = found[limit - 1].creationDate
                let newer = found.prefix { $0.creationDate != date }
                page = order(newer) + run(date, below: afterDate == date ? after?.id : nil).prefix(limit - newer.count)
                more = true
            } else {
                page = order(found.prefix(limit))
                more = found.count > limit
            }
        }
        if !more {
            let undated = run(nil, below: afterDate == nil ? after?.id : nil)
            let room = limit - page.count
            page += undated.prefix(room)
            more = undated.count > room
        }
        return (page, more)
    }

    /// Dated photos newest first, the id ordering one date.
    private static func order(_ assets: some Sequence<PHAsset>) -> [PHAsset] {
        assets.sorted { a, b in
            let (x, y) = (a.creationDate ?? .distantPast, b.creationDate ?? .distantPast)
            return x != y ? x > y : a.localIdentifier > b.localIdentifier
        }
    }

    // MARK: pictures (D2)

    func picture(_ id: String, width: Int, height: Int, fit: String, reply: @escaping (CGImage?, String?) -> Void) {
        // A draw never asks (D2).
        guard readable else { return reply(nil, "NotAllowedError") }
        let key = "\(id) \(width)x\(height) \(fit)"
        if let at = finals.firstIndex(where: { $0.key == key }) { return reply(finals.remove(at: at).image, nil) }
        guard let asset = PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject else { return reply(nil, "NotFoundError") }
        observe()
        drawn.insert(id)
        let options = PHImageRequestOptions()
        options.deliveryMode = .opportunistic
        options.isNetworkAccessAllowed = true
        options.resizeMode = .fast
        if let hatch = PhotosModule.imageRequestOptions {
            hatch(options)
            if options.deliveryMode != .opportunistic || !options.isNetworkAccessAllowed || options.resizeMode != .fast {
                say("the imageRequestOptions hatch changed deliveryMode, isNetworkAccessAllowed or resizeMode, which the module owns (LLP 1133.019 D9)")
            }
        }
        let mode: PHImageContentMode = fit == "contain" || fit == "scale-down" ? .aspectFit : .aspectFill
        var answered = false
        images.requestImage(for: asset, targetSize: CGSize(width: max(1, width), height: max(1, height)), contentMode: mode, options: options) { image, info in
            let quick = info?[PHImageResultIsDegradedKey] as? Bool ?? false
            let cloud = info?[PHImageResultIsInCloudKey] as? Bool ?? false
            let picture = image.flatMap(Self.upright)
            DispatchQueue.main.async { [self] in
                if !answered {
                    answered = true
                    return reply(picture, picture == nil ? (cloud ? "NetworkError" : "UnknownError") : nil)
                }
                // The final picture after a quick one: kept for the next ask.
                guard !quick, let picture, drawn.contains(id) else { return }
                finals.append((key, picture))
                if finals.count > finalsKept { finals.removeFirst(finals.count - finalsKept) }
                context.pictureChanged("photos:\(id)")
            }
        }
    }

    #if os(macOS)
    private static func upright(_ image: NSImage) -> CGImage? { image.cgImage(forProposedRect: nil, context: nil, hints: nil) }
    #else
    /// PhotoKit's image may carry an orientation its pixels do not have.
    private static func upright(_ image: UIImage) -> CGImage? {
        if image.imageOrientation == .up { return image.cgImage }
        let format = UIGraphicsImageRendererFormat()
        format.scale = image.scale
        return UIGraphicsImageRenderer(size: image.size, format: format).image { _ in image.draw(at: .zero) }.cgImage
    }
    #endif

    // MARK: a copy (D2 item 3)

    func copy(_ args: [String: Any], origin: String, reply: @escaping ([String: Any]) -> Void) {
        whenReadable(origin: origin, reply) { [self] in
            guard let id = args["asset"] as? String, let asset = PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject else {
                return reply(["error": "NotFoundError"])
            }
            let resources = PHAssetResource.assetResources(for: asset)
            // The version Photos shows: the edit's full-size photo where there is one (D3).
            let current = (args["version"] as? String ?? "current") == "current"
            guard let resource = (current ? resources.first { $0.type == .fullSizePhoto } : nil) ?? resources.first(where: { $0.type == .photo }) ?? resources.first else {
                return reply(["error": "NotFoundError"])
            }
            // The photo's own extension (`IMG_0001.HEIC` stays `heic`), else its type's.
            let own = (resource.originalFilename as NSString).pathExtension.lowercased()
            let ext = own.isEmpty ? UTType(resource.uniformTypeIdentifier)?.preferredFilenameExtension ?? "img" : own
            let path: String
            if let p = args["path"] as? String, !p.isEmpty { path = p.hasSuffix(".\(ext)") ? p : "\(p).\(ext)" } else { path = "app:/tmp/photos/\(UUID().uuidString).\(ext)" }
            guard let url = context.file(path) else { return reply(["error": "UnknownError"]) }
            try? FileManager.default.removeItem(at: url)
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            let options = PHAssetResourceRequestOptions()
            options.isNetworkAccessAllowed = ((args["apple"] as? [String: Any])?["isNetworkAccessAllowed"] as? Bool) ?? true
            PHAssetResourceManager.default().writeData(for: resource, toFile: url, options: options) { error in
                DispatchQueue.main.async { [self] in
                    if let error = error as NSError? {
                        say("copy of \(id) failed: \(error.domain) \(error.code)")
                        let network = error.domain == NSURLErrorDomain || (error.domain == PHPhotosErrorDomain && error.code == PHPhotosError.networkAccessRequired.rawValue)
                        return reply(["error": network ? "NetworkError" : "UnknownError"])
                    }
                    let size = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int) ?? 0
                    guard let (w, h, type) = AgentLibrary.size(url) else { return reply(["error": "UnknownError"]) }
                    say("copied \(id) to \(path) (\(size) bytes, \(type))")
                    reply(["path": path, "name": resource.originalFilename, "type": type, "size": size,
                           "width": w, "height": h, "duration": NSNull(), "android": NSNull()])
                }
            }
        }
    }

    // MARK: an add (D6)

    func add(_ args: [String: Any], origin: String, reply: @escaping ([String: Any]) -> Void) {
        // Adding is the app's (D7).
        guard origin == "app" else { return reply(["error": "NotAllowedError", "message": "person-required"]) }
        guard let path = args["path"] as? String, let url = context.file(path), FileManager.default.fileExists(atPath: url.path) else {
            return reply(["error": "NotFoundError"])
        }
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil), let uti = CGImageSourceGetType(source),
              UTType(uti as String)?.conforms(to: .image) == true, CGImageSourceGetCount(source) > 0 else {
            return reply(["error": "DataError"])
        }
        let write = { [self] in
            var created: String?
            PHPhotoLibrary.shared().performChanges({
                let request = PHAssetCreationRequest.forAsset()
                request.addResource(with: .photo, fileURL: url, options: nil)
                created = request.placeholderForCreatedAsset?.localIdentifier
                PhotosModule.creationRequest?(request)
            }) { ok, error in
                DispatchQueue.main.async { [self] in
                    guard ok else {
                        let e = error as NSError?
                        say("add from \(path) failed: \(e?.domain ?? "") \(e?.code ?? 0)")
                        let code = e?.domain == PHPhotosErrorDomain && e?.code == PHPhotosError.accessUserDenied.rawValue ? "NotAllowedError"
                            : e?.domain == PHPhotosErrorDomain && e?.code == PHPhotosError.invalidResource.rawValue ? "DataError" : "UnknownError"
                        return reply(["error": code])
                    }
                    say("added \(created ?? "a photo") from \(path)")
                    // The new photo's id where the app may read it (D6): not under add-only access.
                    reply(["asset": readable ? created as Any? ?? NSNull() : NSNull()])
                }
            }
        }
        if readable { return write() }
        // The narrowest level the app declares (D4): the add key is `device.photos-add`'s alone.
        let addOnly = Bundle.main.object(forInfoDictionaryKey: "NSPhotoLibraryAddUsageDescription") != nil
        authorize(addOnly ? .addOnly : .readWrite) { answer in
            answer == .authorized || answer == .limited ? write() : reply(["error": "NotAllowedError"])
        }
    }

    // MARK: the selection (D6)

    func manage(origin: String, reply: @escaping ([String: Any]) -> Void) {
        #if os(macOS)
        reply(["error": "NotSupportedError"])
        #else
        guard origin == "app" else { return reply(["error": "NotAllowedError", "message": "person-required"]) }
        // Under full access the picker does nothing: the status answers now.
        guard level(.readWrite) == .limited, let shown = Self.topController() else { return reply(status()) }
        PHPhotoLibrary.shared().presentLimitedLibraryPicker(from: shown) { _ in
            DispatchQueue.main.async { [self] in
                recheck()
                reply(status())
            }
        }
        #endif
    }

    #if !os(macOS)
    private static func topController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        var top = scenes.flatMap(\.windows).first { $0.isKeyWindow }?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
    #endif
}
