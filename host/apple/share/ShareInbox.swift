// @ref LLP 1133.024 D4 — the share extension's inbox: what a share leaves in
// the App Group for the app to claim. The extension links no Exact runtime;
// this record's versioned shape is its one contract with the app.
// `EXACT_SHARE_PAGE` compiles in the reading of Safari's page details (D4,
// amended 2026-11-15), for an app that maps `url`.
//
// `<group>/shares/<uuid>/` holds each attachment as `<n>.<ext>` (a counter
// and the type's extension, never the sender's name), streamed by
// `loadFileRepresentation` and never decoded, then `share.json` last, by
// write-and-rename: a directory without it is incomplete and the claim
// ignores it. What is decoded is read from a file up to a cap (`head`): a
// property list small enough to be Safari's page details (`readPage`), and
// the confirmation's text (ShareConfirmation.swift).
import Foundation
import UniformTypeIdentifiers

/// What the bake told the extension in its Info.plist.
public struct ShareTargetInfo {
    /// The App Group both the app and the extension can write.
    public var group: String
    /// The app's id: the Darwin notification is `<id>.shares`.
    public var appId: String
    /// Which text members the app maps (`title`, `text`, `url`).
    public var mapped: Set<String>
    /// Each files member: its name and the UTIs it accepts.
    public var members: [(name: String, types: [String])]

    public init(group: String, appId: String, mapped: Set<String>, members: [(name: String, types: [String])]) {
        self.group = group
        self.appId = appId
        self.mapped = mapped
        self.members = members
    }

    /// Read from the extension's Info.plist (`host/apple/share.mjs` writes these keys).
    public init?(bundle: Bundle) {
        guard let group = bundle.object(forInfoDictionaryKey: "ExactAppGroup") as? String,
              let appId = bundle.object(forInfoDictionaryKey: "ExactAppIdentifier") as? String else { return nil }
        let mapped = Set(bundle.object(forInfoDictionaryKey: "ExactShareParams") as? [String] ?? [])
        let members = (bundle.object(forInfoDictionaryKey: "ExactShareMembers") as? [[String: Any]] ?? []).compactMap { m -> (String, [String])? in
            guard let name = m["name"] as? String else { return nil }
            return (name, m["types"] as? [String] ?? [])
        }
        self.init(group: group, appId: appId, mapped: mapped, members: members)
    }

    /// The first files member that accepts `type`, and the accepted UTI it conforms to.
    func member(for type: UTType) -> (name: String, accepted: String)? {
        for m in members {
            for t in m.types {
                if let accepted = UTType(t), type.conforms(to: accepted) { return (m.name, t) }
            }
        }
        return nil
    }
}

/// One share being written: the sheet's text and what the attachments became.
public final class ShareInboxWriter {
    /// The 64 MB an outgoing share also refuses (LLP 1125 D12).
    public static let cap: Int64 = 64 << 20
    public let info: ShareTargetInfo
    public let directory: URL
    /// The longest title taken from Safari's answer, in UTF-16 units: the page script's bound (SharePage.js), held
    /// here too because a page writes its own answer.
    static let titleCap = 4096
    private var files: [[String: String]] = []
    private var dropped: [String: Int] = [:]
    /// Each link attached, in the order attached, as it loaded: nil for one that did not.
    private var links: [URL?] = []
    #if EXACT_SHARE_PAGE
    /// The largest property list read as Safari's answer, in bytes of its file representation. The answer holds
    /// two URLs and a title, and the page script keeps it under this. A larger list is an attachment like
    /// another, copied and never decoded.
    static let pageCap = 1 << 20
    /// Safari's answer about the open page, as the page script gave it.
    private var answer: [String: Any]?
    /// The attachments that held Safari's answer; nil until `readPage` has looked for them.
    private var answers: [NSItemProvider]?
    /// Whether a property list that may have been Safari's answer was not read: it is over `pageCap`, or did not
    /// load.
    private var unread = false
    #endif
    private let lock = NSLock()

    /// A writer into `container/shares/<uuid>/`; nil when the container cannot be made.
    public init?(info: ShareTargetInfo, container: URL) {
        self.info = info
        directory = container.appendingPathComponent("shares", isDirectory: true).appendingPathComponent(UUID().uuidString, isDirectory: true)
        do { try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true) } catch { return nil }
    }

    /// The App Group's container, or nil when the extension is not entitled to it.
    public static func container(_ info: ShareTargetInfo) -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: info.group)
    }

    /// Takes every attachment of `items`: a web link as `url` (when mapped), each file an accepted type matches,
    /// streamed to disk; what no member accepts, or over the cap, is counted as dropped. `done` runs once all have.
    public func take(_ items: [NSExtensionItem], done: @escaping () -> Void) {
        #if EXACT_SHARE_PAGE
        // Which attachments hold Safari's answer is known only once they have loaded, so they are read first.
        guard let answers else { return readPage(items) { self.take(items, done: done) } }
        #endif
        let group = DispatchGroup()
        var n = 0
        for item in items {
            for provider in item.attachments ?? [] {
                #if EXACT_SHARE_PAGE
                if answers.contains(where: { $0 === provider }) { continue }
                #endif
                // A link: `merge` makes the share's `url` of the web links, which the claim gives to `url`, or to
                // `text` when only that is mapped.
                if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier), !provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                    var slot = 0
                    locked { slot = links.count; links.append(nil) }
                    group.enter()
                    _ = provider.loadObject(ofClass: URL.self) { [weak self] value, _ in
                        self?.locked { self?.links[slot] = value }
                        group.leave()
                    }
                    continue
                }
                // Text that is not a file (neither a file URL nor a name of its own) is the sheet's text, not an attachment.
                if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier), provider.suggestedName == nil,
                   !provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) { continue }
                guard let (member, accepted) = provider.registeredTypeIdentifiers.lazy.compactMap({ UTType($0) }).compactMap({ self.info.member(for: $0).map { m in (m.name, m.accepted) } }).first else {
                    count(dropped: info.members.first?.name ?? "")
                    continue
                }
                let type = provider.registeredTypeIdentifiers.first { UTType($0)?.conforms(to: UTType(accepted) ?? .data) == true } ?? accepted
                n += 1
                let index = n
                group.enter()
                let load = { [weak self] (sender: String?) in
                    _ = provider.loadFileRepresentation(forTypeIdentifier: type) { [weak self] file, _ in
                        defer { group.leave() }
                        guard let self, let file else { self?.count(dropped: member); return }
                        self.store(file, index: index, type: UTType(type) ?? .data, member: member, suggested: provider.suggestedName ?? sender)
                    }
                }
                // A file with no suggested name (a file URL's provider) is named by its URL, as the sender has it;
                // the representation's copy has a name of its own.
                if provider.suggestedName == nil, provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                    _ = provider.loadObject(ofClass: URL.self) { url, _ in load(url?.lastPathComponent) }
                } else {
                    load(nil)
                }
            }
        }
        group.notify(queue: .main, execute: done)
    }

    #if EXACT_SHARE_PAGE
    /// Reads Safari's answer from Exact's page script (SharePage.js, which the bake declares, and compiles this for,
    /// when `url` is mapped), then runs `done` on the main queue. Safari attaches the answer as a property list
    /// with no name of its own, and a sender's own list can look the same, so each such list is read from its file
    /// representation, up to `pageCap`, and only one that holds an answer is taken: any other stays an attachment
    /// for `take`. The list is never loaded as an object, which a provider may answer with another representation
    /// of any size.
    private func readPage(_ items: [NSExtensionItem], done: @escaping () -> Void) {
        let group = DispatchGroup()
        answers = []
        for provider in items.flatMap({ $0.attachments ?? [] }) where provider.hasItemConformingToTypeIdentifier(UTType.propertyList.identifier)
            && provider.suggestedName == nil && !provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
            group.enter()
            _ = provider.loadFileRepresentation(forTypeIdentifier: UTType.propertyList.identifier) { [weak self] file, _ in
                defer { group.leave() }
                guard let self else { return }
                guard let data = Self.head(of: file, cap: Self.pageCap), data.count <= Self.pageCap else { return self.locked { self.unread = true } }
                guard let answer = Self.answer(in: data) else { return }
                self.locked {
                    self.answers?.append(provider)
                    self.answer = self.answer ?? answer
                }
            }
        }
        group.notify(queue: .main, execute: done)
    }

    /// Safari's answer in a property list's bytes: the dictionary under `NSExtensionJavaScriptPreprocessingResultsKey`,
    /// or nil for any other list. An item provider writes a dictionary's file as a keyed archive, which is decoded
    /// securely as property-list classes alone; a sender may also give a plain property list.
    static func answer(in data: Data) -> [String: Any]? {
        let classes: [AnyClass] = [NSDictionary.self, NSArray.self, NSString.self, NSNumber.self, NSData.self, NSDate.self]
        let list = (try? NSKeyedUnarchiver.unarchivedObject(ofClasses: classes, from: data)) ?? (try? PropertyListSerialization.propertyList(from: data, format: nil))
        return (list as? [String: Any])?[NSExtensionJavaScriptPreprocessingResultsKey] as? [String: Any]
    }
    #endif

    /// At most the first `cap + 1` bytes of `file`, or nil when it cannot be read. What the extension decodes in
    /// its memory, whose limit is far below the app's, it decodes from these bytes: a count over `cap` is a file
    /// too large, and no more of it was read.
    static func head(of file: URL?, cap: Int) -> Data? {
        guard let file, let handle = try? FileHandle(forReadingFrom: file) else { return nil }
        defer { try? handle.close() }
        do { return try handle.read(upToCount: cap + 1) ?? Data() } catch { return nil }
    }

    /// The share's `url` and `title`: what the links attached (nil for one that did not load), Safari's answer
    /// about the open page, the item's own `title` and the members the app maps come to.
    ///
    /// The share's own `url` is the first link attached that is a web link, and its title the item's. Safari's
    /// answer describes the open page, so it stands for the share only when the share is of that page: Safari on
    /// iOS 27 shares a page as the answer alone, and a link pressed in a page as the link alone. So the answer
    /// gives the `url`, and the `title` when its own is text, when its `url` is a web link, every link attached
    /// loaded, and each is the page's location (`same`). That is the answer's `location`; the script leaves it out
    /// when it is the `url`. A link that did not load may have been the share itself, so the page does not stand
    /// in for it. No `url` keeps its userinfo, and the answer's title is cut at `titleCap`.
    static func merge(links: [URL?], answer: [String: Any]?, title: String?, mapped: Set<String>) -> (url: String?, title: String?) {
        var url = links.lazy.compactMap { web($0) }.first
        var title = title
        if let answer, let page = web(answer["url"] as? String) {
            // A `location` that is no web link is one no link can be.
            let location: URL? = answer["location"].map { web($0 as? String) } ?? page
            if links.allSatisfy({ same($0, location) }) {
                url = page
                if let own = answer["title"] as? String, !own.isEmpty { title = cut(own) }
            }
        }
        return (url?.absoluteString, mapped.contains("title") && title?.isEmpty == false ? title : nil)
    }

    /// `link` as a web URL without its userinfo, the name and password an address may carry, which would otherwise
    /// reach the App Group and the app's location; nil for any other link.
    static func web(_ link: URL?) -> URL? {
        guard let link, var parts = URLComponents(url: link, resolvingAgainstBaseURL: true), ["http", "https"].contains(parts.scheme?.lowercased()) else { return nil }
        parts.user = nil
        parts.password = nil
        return parts.url
    }

    private static func web(_ link: String?) -> URL? { web(link.flatMap { URL(string: $0) }) }

    /// Whether two links are one URL. The spellings that cannot differ in meaning are one (RFC 3986 §6.2.2.1 and
    /// §6.2.3): the scheme and host in either case, a port left out or its scheme's own, a path left out or "/".
    /// Userinfo is not compared. Everything else is compared as written, since a last slash or a query that
    /// differs may name another resource, and another fragment is another place in the page, which is what the
    /// person chose.
    static func same(_ a: URL?, _ b: URL?) -> Bool {
        func parts(_ url: URL?) -> [String?]? {
            guard let url, let parts = URLComponents(url: url, resolvingAgainstBaseURL: true), let scheme = parts.scheme?.lowercased() else { return nil }
            return [scheme, parts.host?.lowercased(), "\(parts.port ?? (scheme == "http" ? 80 : 443))", parts.percentEncodedPath.isEmpty ? "/" : parts.percentEncodedPath,
                    parts.percentEncodedQuery, parts.percentEncodedFragment]
        }
        guard let a = parts(a), let b = parts(b) else { return false }
        return a == b
    }

    /// `title` within `titleCap` UTF-16 units, cut on a whole character as the page script cuts it.
    private static func cut(_ title: String) -> String {
        let units = title.utf16
        guard units.count > titleCap else { return title }
        let end = units.index(units.startIndex, offsetBy: titleCap)
        return String(title.unicodeScalars[..<(end.samePosition(in: title.unicodeScalars) ?? units.index(before: end))])
    }

    /// Copies `file` (the provider's temporary copy, gone after the callback) into the share under a stored name.
    private func store(_ file: URL, index: Int, type: UTType, member: String, suggested: String?) {
        let size = (try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(Int64.init) ?? 0
        guard size <= Self.cap else { return count(dropped: member) }
        let ext = type.preferredFilenameExtension ?? file.pathExtension
        let stored = ext.isEmpty ? "\(index)" : "\(index).\(ext)"
        do {
            try FileManager.default.copyItem(at: file, to: directory.appendingPathComponent(stored))
        } catch {
            return count(dropped: member)
        }
        let name = suggested.map { s in s.contains(".") || ext.isEmpty ? s : "\(s).\(ext)" } ?? file.lastPathComponent
        locked { files.append(["file": stored, "name": name, "type": type.preferredMIMEType ?? "application/octet-stream", "member": member]) }
    }

    private func count(dropped member: String) { locked { dropped[member, default: 0] += 1 } }
    private func locked(_ body: () -> Void) { lock.lock(); body(); lock.unlock() }

    /// Writes `share.json` last, by write-and-rename, then pings the app. A share with nothing for the app to keep
    /// is not written: it throws.
    public func finish(title: String?, text: String?) throws {
        let ordered = files.sorted { Int($0["file"]!.split(separator: ".")[0]) ?? 0 < Int($1["file"]!.split(separator: ".")[0]) ?? 0 }
        var record: [String: Any] = ["v": 1, "files": ordered, "dropped": dropped]
        #if EXACT_SHARE_PAGE
        let merged = Self.merge(links: links, answer: answer, title: title, mapped: info.mapped)
        #else
        let merged = Self.merge(links: links, answer: nil, title: title, mapped: info.mapped)
        #endif
        if let title = merged.title { record["title"] = title }
        if info.mapped.contains("text"), let text, !text.isEmpty { record["text"] = text }
        if let url = merged.url { record["url"] = url }
        // What did not arrive is left out, and the rest is saved without it: a link or a file that did not load, a
        // file over the cap or one no member accepts, a list too large to read. With no link, text or file left
        // there is nothing for the app to keep, and a request that completed would say the share was taken. A
        // title alone is no share, and the claim gives a link to `url`, or to `text` when only that is mapped.
        let linked = !info.mapped.isDisjoint(with: ["url", "text"])
        if !linked || merged.url == nil, record["text"] == nil, ordered.isEmpty {
            if linked, links.contains(where: { $0 == nil }) { throw Self.failure(3, "The shared link could not be read.") }
            #if EXACT_SHARE_PAGE
            // Safari shares a page as its answer alone.
            if unread { throw Self.failure(4, "The shared page could not be read.") }
            #endif
            throw Self.failure(6, "Nothing in the share could be saved.")
        }
        let data = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
        let partial = directory.appendingPathComponent("share.json.partial")
        try data.write(to: partial)
        try FileManager.default.moveItem(at: partial, to: directory.appendingPathComponent("share.json"))
        CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName("\(info.appId).shares" as CFString), nil, nil, true)
    }

    private static func failure(_ code: Int, _ reason: String) -> NSError {
        NSError(domain: "exact.share", code: code, userInfo: [NSLocalizedDescriptionKey: reason])
    }

    /// Removes what was written (the person cancelled, or the write failed).
    public func discard() { try? FileManager.default.removeItem(at: directory) }
}
