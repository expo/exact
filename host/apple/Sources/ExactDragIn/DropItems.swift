// What a drop's items carry (LLP 1094.001 D4), decided from the types each
// item registers, best first as its source lists them, and the system's type
// tree. Exact keeps no list of file types, so a type iOS adds needs no change
// here, and every call is iOS 14 or earlier, below Exact's iOS 17 floor.
#if os(iOS)
import Foundation
import UniformTypeIdentifiers

enum DropItems {
    /// What an item is, from its registered types alone, so every load
    /// starts in `performDrop`, as UIKit requires.
    enum Kind: Equatable {
        /// A file, by the type to load it as.
        case file(String)
        /// A link that is not a file, plain text, or both, each read alone.
        case words(url: Bool, text: Bool)
        /// Nothing usable: a bare `file:` URL, which another app's is not
        /// readable, or only types the system cannot name.
        case none
    }

    /// 1. A file the item offers to open in place is a file. 2. An item that
    /// registers a URL that is not a file URL, or a type in
    /// `public.plain-text`'s tree, gives its link and its plain text: a link
    /// card its link and title, rich text its plain-text form. Text asks the
    /// tree because `loadObject(ofClass: NSString.self)` answers a link-only
    /// item with the link's own string. 3. Otherwise the item is a file, by
    /// its first type the system declares that is not a URL, else its first
    /// dynamic one, as Exact's own file card of an unknown extension
    /// registers. UIKit's pictures lead with a type the system cannot name
    /// (`com.apple.uikit.image`), so one arrives by its first declared type,
    /// such as `public.heic`.
    static func kind(of provider: NSItemProvider) -> Kind {
        if let offered = provider.registeredTypeIdentifiers(fileOptions: [.openInPlace]).first { return .file(offered) }
        let types = provider.registeredTypeIdentifiers
        let url = types.contains(UTType.url.identifier) && !types.contains(UTType.fileURL.identifier)
        let text = types.contains { UTType($0)?.conforms(to: .plainText) == true }
        if url || text { return .words(url: url, text: text) }
        let named = types.compactMap { id in UTType(id).flatMap { $0.conforms(to: .url) ? nil : (id, $0) } }
        return (named.first { $0.1.isDeclared } ?? named.first { $0.1.isDynamic }).map { Kind.file($0.0) } ?? .none
    }

    /// What the items drop, in their order: each file copied out of UIKit's
    /// temporary one into `folder`, made for this drop; each item's plain
    /// text, joined by line breaks; and the first link. `done` runs on the
    /// main queue once every item has loaded or failed; a failed item, and a
    /// folder, are journaled (`log`) and left out. Nothing is minted here:
    /// the copies become the session's only when the drop is delivered.
    static func read(_ providers: [NSItemProvider], log: @escaping (String) -> Void,
                     done: @escaping (_ files: [URL], _ text: String, _ url: String, _ folder: URL) -> Void) {
        let say = { (line: String) in DispatchQueue.main.async { log(line) } }
        var files = [URL?](repeating: nil, count: providers.count), texts = files.map { _ in String?.none }, urls = texts
        let group = DispatchGroup()
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("exact-dropped", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        for (index, provider) in providers.enumerated() {
            switch kind(of: provider) {
            case .file(let type):
                group.enter()
                _ = provider.loadFileRepresentation(forTypeIdentifier: type) { url, error in
                    // UIKit removes its file once this returns, so it is copied here
                    let copy = url.flatMap { copied($0, into: folder, say: say) }
                    if url == nil { say("drop: an item's file did not arrive: \(error?.localizedDescription ?? "UIKit gave none")") }
                    DispatchQueue.main.async { files[index] = copy; group.leave() }
                }
            case .words(let url, let text):
                if url {
                    group.enter()
                    _ = provider.loadObject(ofClass: URL.self) { link, _ in
                        DispatchQueue.main.async { urls[index] = link.flatMap { $0.isFileURL ? nil : $0.absoluteString }; group.leave() }
                    }
                }
                if text {
                    group.enter()
                    _ = provider.loadObject(ofClass: String.self) { words, _ in
                        DispatchQueue.main.async { texts[index] = words; group.leave() }
                    }
                }
            case .none:
                say("drop: an item offers nothing to take (\(provider.registeredTypeIdentifiers.joined(separator: ", ")))")
            }
        }
        group.notify(queue: .main) {
            done(files.compactMap { $0 }, texts.compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: "\n"), urls.compactMap { $0 }.first ?? "", folder)
        }
    }

    /// A copy of the file `url` in a folder of its own under `folder`, so two
    /// items of one name both arrive; none for a folder (D4) or a failed copy.
    private static func copied(_ url: URL, into folder: URL, say: (String) -> Void) -> URL? {
        if (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true {
            say("drop: \(url.lastPathComponent) is a folder; iOS drops files, not folders")
            return nil
        }
        let place = folder.appendingPathComponent(UUID().uuidString, isDirectory: true), copy = place.appendingPathComponent(url.lastPathComponent)
        do {
            try FileManager.default.createDirectory(at: place, withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: url, to: copy)
            return copy
        } catch {
            say("drop: \(url.lastPathComponent) could not be copied: \(error.localizedDescription)")
            return nil
        }
    }
}
#endif
