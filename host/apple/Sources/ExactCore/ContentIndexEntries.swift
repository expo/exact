// @ref LLP 1133.023 D9, LLP 1126 D2 — content-index entries as an entity query reads them.
import Foundation

/// The app answers an entity query from its mirror (`ExactContentIndex`). A widget
/// extension has no mirror: it answers from the list the app published to the App
/// Group for the kinds its widgets' parameters name. Both answer by `answer`.
public enum ContentIndexEntries {
    /// What a query shows of an entry.
    public struct Entry: Codable, Equatable {
        public let id: String
        public let title: String
        public let description: String?
        public init(id: String, title: String, description: String?) {
            self.id = id; self.title = title; self.description = description
        }
    }

    /// D9's answer over one kind's `entries`, which are in the app's order: the `ids`
    /// asked, in their order; else the entries whose title or description contains
    /// `text` (case- and diacritic-insensitive, in the person's locale); else all.
    public static func answer<E>(_ entries: [E], ids: [String]?, matching text: String?, id: (E) -> String, texts: (E) -> [String?]) -> [E] {
        if let ids {
            let byID = Dictionary(entries.map { (id($0), $0) }, uniquingKeysWith: { first, _ in first })
            return ids.compactMap { byID[$0] }
        }
        guard let text, !text.isEmpty else { return entries }
        return entries.filter { entry in
            texts(entry).contains { $0?.range(of: text, options: [.caseInsensitive, .diacriticInsensitive], locale: .current) != nil }
        }
    }

    private static func file(_ root: URL) -> URL { root.appendingPathComponent("widget-entities.json") }

    /// Replace the published list: each named kind's entries, in the app's order.
    package static func publish(_ entries: [String: [Entry]], root: URL) throws {
        try JSONEncoder().encode(entries).write(to: file(root), options: .atomic)
    }

    /// The published entries of `kind` that answer the query; none before the app publishes.
    public static func published(kind: String, ids: [String]? = nil, matching text: String? = nil, root: URL? = WidgetContainer.root) -> [Entry] {
        guard let root, let data = try? Data(contentsOf: file(root)),
              let kinds = try? JSONDecoder().decode([String: [Entry]].self, from: data) else { return [] }
        return answer(kinds[kind] ?? [], ids: ids, matching: text, id: \.id, texts: { [$0.title, $0.description] })
    }

    /// A placement's saved choices: each of `ids` as published, else a stand-in that
    /// keeps the id under `title`. WidgetKit gives a timeline provider no value for an
    /// id its query does not answer, and the refresh decides what such an id shows.
    public static func chosen(kind: String, ids: [String], title: String, root: URL? = WidgetContainer.root) -> [Entry] {
        let known = published(kind: kind, ids: ids, root: root)
        return ids.map { id in known.first { $0.id == id } ?? Entry(id: id, title: title, description: nil) }
    }
}
