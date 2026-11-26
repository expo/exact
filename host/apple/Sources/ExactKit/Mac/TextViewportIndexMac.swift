#if os(macOS)
import AppKit
import ExactCore

/// The text pump admits readers, worker rasters and backing-store bands.
/// A small SDR label draws whole unless a canvas needs its pixels.
final class TextViewport {
    var geometry: TextViewportIndex?
    private var shown: [UInt32: NodeView]?
    private var checkShown = false
    private var indexed: [UInt32: NodeView] = [:]
    private var dirty: Set<UInt32> = []
    private var readers: Set<UInt32> = []
    private var readerGeneration: UInt64?
    private weak var readerEngine: TextEngine?
    private var included: Set<UInt32> = []
    private var zoom: CGFloat = 0

    func reset(checkingOrder: Bool = false) {
        checkShown = checkingOrder
        if checkingOrder { return }
        shown = nil
        geometry = nil
        indexed.removeAll(keepingCapacity: true)
        dirty.removeAll(keepingCapacity: true)
    }

    func changed(_ node: NodeView) {
        if node.isParagraph, shown?[node.id] === node { dirty.insert(node.id) }
    }

    func geometryChanged(_ node: NodeView) {
        if geometry != nil, included.contains(node.id) { geometry = nil }
        changed(node)
    }

    func ancestryChanged(_ node: NodeView) {
        guard shown != nil else { return }
        geometryChanged(node)
        NodeView.visitParagraphs(in: node.subviews, geometryChanged)
    }

    func candidates(in presenter: Presenter, reach: CGFloat) -> [(distance: CGFloat, node: NodeView)] {
        if checkShown, let shown {
            let nodes = presenter.selection.shownParagraphs
            // A style can hide or reparent a native subtree. When the same
            // paragraphs are shown, keep their paint eligibility across the edit.
            if nodes.count != shown.count || !nodes.allSatisfy({ shown[$0.id] === $0 }) { reset() }
        }
        checkShown = false
        if shown == nil {
            indexed = Dictionary(uniqueKeysWithValues: presenter.selection.shownParagraphs.map { ($0.id, $0) })
            shown = indexed
            dirty.formUnion(indexed.keys)
        }
        if zoom != presenter.viewport.magnification, let shown {
            zoom = presenter.viewport.magnification
            dirty.formUnion(shown.keys)
        }
        let engine = presenter.session?.text
        if readerEngine !== engine || readerGeneration != engine?.readerGeneration {
            readerEngine = engine
            let current = engine?.readerViews ?? []
            dirty.formUnion(readers.symmetricDifference(current))
            readers = current
            readerGeneration = engine?.readerGeneration
        }
        var additions: [NodeView] = []
        let pending = dirty
        dirty = []
        for id in pending {
            guard let node = shown?[id] else { continue }
            let direct = node.belowTextRasterThreshold && node.hasKnownSDRText
                && !readers.contains(id) && node.canvasAbove == nil
            if direct {
                indexed.removeValue(forKey: id)
                if node.textRasterKey != nil || node.textRaster != nil || node.textRasterUsesStrips {
                    node.dropTextRaster()
                    node.needsDisplay = true
                }
            } else if indexed[id] !== node {
                indexed[id] = node
                if geometry != nil, included.insert(id).inserted { additions.append(node) }
            }
        }
        guard !indexed.isEmpty else { return [] }
        if geometry == nil {
            geometry = TextViewportIndex(indexed.values)
            included = Set(indexed.keys)
        } else if !additions.isEmpty {
            // Scroll coordinates remain valid when a paragraph changes how it
            // paints. Only a newly admitted paragraph needs its box converted.
            geometry?.include(additions)
        }
        return geometry?.candidates(reach: reach) { self.indexed[$0.id] != nil } ?? []
    }
}

private extension NodeView {
    /// Asking for a spec here would shape or expand text outside raster reach.
    /// A literal without run paint uses at most the four SDR colour channels;
    /// an extended colour carries its space in `cs` (`BatchValue.textChannels`).
    var hasKnownSDRText: Bool {
        if let cachedTextHeadroom { return cachedTextHeadroom <= 1 }
        if case .object(let color) = style["text_color"], color["cs"] != nil { return false }
        return props["text"] != nil && props["markup"] != "markdown" && inlineText.isEmpty
            && RunPaintRows(style) == RunPaintRows()
    }
}

/// Paragraph geometry in each scroll document's own coordinates. A scroll
/// changes the query rectangle, never the index. Layout/structure changes
/// discard it. Selection and copy keep their complete document order.
/// @ref LLP 1033 (long documents), LLP 1010 (native scrolling)
struct TextViewportIndex {
    private struct Entry {
        let node: NodeView
        let rect: NSRect
        var bottom: CGFloat
    }
    private struct Group {
        let document: NSView
        var entries: [Entry]

        mutating func sort() {
            entries.sort { $0.rect.minY < $1.rect.minY }
            var bottom = -CGFloat.infinity
            for i in entries.indices {
                bottom = max(bottom, entries[i].rect.maxY)
                entries[i].bottom = bottom
            }
        }

        mutating func insert(_ entry: Entry) {
            var lo = 0, hi = entries.count
            while lo < hi {
                let mid = lo + (hi - lo) / 2
                if entries[mid].rect.minY <= entry.rect.minY { lo = mid + 1 } else { hi = mid }
            }
            var placed = entry
            if lo > 0 { placed.bottom = max(placed.bottom, entries[lo - 1].bottom) }
            entries.insert(placed, at: lo)
            for i in (lo + 1)..<entries.count {
                if entries[i].bottom >= placed.bottom { break }
                entries[i].bottom = placed.bottom
            }
        }
    }
    private var groups: [ObjectIdentifier: Group] = [:]

    init(_ paragraphs: some Sequence<NodeView>) {
        for node in paragraphs {
            guard let (document, entry) = Self.entry(node) else { continue }
            groups[ObjectIdentifier(document), default: Group(document: document, entries: [])].entries.append(entry)
        }
        for key in groups.keys { groups[key]!.sort() }
    }

    private static func entry(_ node: NodeView) -> (NSView, Entry)? {
        // A paragraph outside a scroller uses its window's content view.
        guard let document = node.enclosingScrollView?.documentView ?? node.window?.contentView else { return nil }
        let rect = node.convert(node.bounds, to: document)
        return (document, Entry(node: node, rect: rect, bottom: rect.maxY))
    }

    mutating func include(_ paragraphs: [NodeView]) {
        for node in paragraphs {
            guard let (document, entry) = Self.entry(node) else { continue }
            groups[ObjectIdentifier(document), default: Group(document: document, entries: [])].insert(entry)
        }
    }

    /// Paragraphs within `reach`, nearest each scroll document's visible rect
    /// first. A tall distant paragraph must not delay a short imminent one.
    func candidates(reach: CGFloat = 0, including: (NodeView) -> Bool = { _ in true }) -> [(distance: CGFloat, node: NodeView)] {
        var result: [(distance: CGFloat, node: NodeView)] = []
        for group in groups.values {
            let shown = group.document.visibleRect
            guard !shown.isEmpty else { continue }
            let visible = shown.insetBy(dx: -reach, dy: -reach)
            // Prefix maxima include tall/overlapping paragraphs that start
            // before the viewport. Binary-searching only minY loses them.
            var lo = 0, hi = group.entries.count
            while lo < hi {
                let mid = lo + (hi - lo) / 2
                if group.entries[mid].bottom <= visible.minY { lo = mid + 1 }
                else { hi = mid }
            }
            var index = lo
            while index < group.entries.count && group.entries[index].rect.minY < visible.maxY {
                let entry = group.entries[index]
                if entry.rect.intersects(visible), including(entry.node) {
                    let dx = max(0, shown.minX - entry.rect.maxX, entry.rect.minX - shown.maxX)
                    let dy = max(0, shown.minY - entry.rect.maxY, entry.rect.minY - shown.maxY)
                    result.append((max(dx, dy), entry.node))
                }
                index += 1
            }
        }
        result.sort {
            $0.distance == $1.distance ? $0.node.id < $1.node.id : $0.distance < $1.distance
        }
        return result
    }
}
#endif
