#if os(iOS)
import UIKit

/// The resting heights of a UIKit sheet presenting a route or a dialog.
package enum SheetDetents {
    /// Effective detent state outside a UIKit resolver, computed from the current
    /// content height and sheet maximum (LLP 1075.003 §9.11, amended 2026-11-27).
    package struct ActiveSummary {
        package let activeCount: Int
        package let terminalContentIdentifier: UISheetPresentationController.Detent.Identifier?
        package let droppedIdentifiers: Set<UISheetPresentationController.Detent.Identifier>
        package let droppedWords: Set<String>
        package var hasTerminalContent: Bool { terminalContentIdentifier != nil }
    }

    /// Whether `value` ends with a terminal `fit-content` ceiling after one or
    /// more static stops.
    package static func hasTerminalContent(_ value: String?) -> Bool {
        isTerminalContent(keptDetents(value, refused: nil))
    }

    /// Replacing the resting heights also chooses the initial one. Updating a
    /// grabber on its own preserves the height the person dragged the sheet to.
    /// `above` is what the sheet shows over the node's content: a dialog's bar.
    @MainActor package static func select(
        on sheet: UISheetPresentationController,
        node: NodeView,
        owner: String,
        maximum: CGFloat? = nil,
        observedMaximum: ((CGFloat) -> Void)? = nil,
        above: @escaping () -> CGFloat = { 0 }
    ) {
        let value = node.props["navigationDetent"]
        var refused: [(Substring, DetentRefusal)] = []
        let heights = detents(value, refused: { refused.append(($0, $1)) }, observedMaximum: observedMaximum) { [weak node] in
            node?.fitContentHeight.map { $0 > 0 ? $0 + above() : $0 }
        }
        sheet.detents = heights.isEmpty ? [.large()] : heights
        if !refused.isEmpty {
            node.presenter?.session?.log(refusal(detent: value ?? "", words: refused, owner: owner, kept: !heights.isEmpty))
        }
        let active = activeSummary(for: node, on: sheet, maximum: maximum, above: above())
        let initial = node.props["navigationInitialDetent"] ?? ""
        let opening = resolveIdentifier(of: initial, in: heights.isEmpty ? "large" : value, active: active)
        if !initial.isEmpty, opening == nil {
            node.presenter?.session?.log("navigationInitialDetent \"\(initial)\" on \(owner) names none of the detents the sheet kept from navigationDetent \"\(value ?? "")\"; the sheet opens at its first")
        }
        let firstActive = sheet.detents.first { !active.droppedIdentifiers.contains($0.identifier) }?.identifier ?? sheet.detents.first?.identifier
        sheet.selectedDetentIdentifier = opening ?? firstActive
    }

    /// The route holds the sheet at `word`, one of its detents (`navigationSelectedDetent`, LLP 1123 D7). A word that
    /// names none of them is journaled, and the sheet stays where it is.
    @MainActor package static func hold(
        _ sheet: UISheetPresentationController,
        at word: String,
        node: NodeView,
        owner: String,
        active: ActiveSummary? = nil
    ) {
        guard let held = named(word, by: "navigationSelectedDetent", node: node, owner: owner, active: active, else: "the sheet stays where it is") else { return }
        if sheet.selectedDetentIdentifier != held { sheet.selectedDetentIdentifier = held }
    }

    /// The tallest detent at which the sheet dims nothing (`navigationUndimmedDetent`, LLP 1123 D7), nil unsaid. A
    /// word that names none of them is journaled, and every detent dims.
    @MainActor package static func undimmed(
        _ word: String?,
        node: NodeView,
        owner: String,
        active: ActiveSummary? = nil
    ) -> UISheetPresentationController.Detent.Identifier? {
        word.flatMap { named($0, by: "navigationUndimmedDetent", node: node, owner: owner, active: active, else: "every detent dims") }
    }

    /// The identifier of `word` among the detents the node's sheet keeps, its `attribute`'s; a word naming none is
    /// journaled with what follows.
    @MainActor private static func named(
        _ word: String,
        by attribute: String,
        node: NodeView,
        owner: String,
        active: ActiveSummary?,
        else follows: String
    ) -> UISheetPresentationController.Detent.Identifier? {
        let value = node.props["navigationDetent"]
        let effective = keptDetents(value, refused: nil).isEmpty ? "large" : value
        if let found = resolveIdentifier(of: word, in: effective, active: active) { return found }
        node.presenter?.session?.log("\(attribute) \"\(word)\" on \(owner) names none of the detents the sheet kept from navigationDetent \"\(value ?? "")\"; \(follows)")
        return nil
    }

    @MainActor package static func updateGrabber(
        _ value: String?,
        on sheet: UISheetPresentationController,
        activeCount: Int? = nil,
        scrollExpands: String? = nil
    ) {
        let multi = (activeCount ?? sheet.detents.count) > 1
        sheet.prefersGrabberVisible = value == "visible" || value != "hidden" && multi
        sheet.prefersScrollingExpandsWhenScrolledToEdge = scrollExpands.map { $0 == "true" } ?? multi
    }

    /// Effective detent count and omitted stops for a sheet whose `navigationDetent`
    /// ends in a terminal `fit-content` ceiling, evaluated outside UIKit's resolvers.
    @MainActor package static func activeSummary(
        for node: NodeView,
        on sheet: UISheetPresentationController,
        maximum: CGFloat?,
        above: CGFloat = 0
    ) -> ActiveSummary {
        let kept = keptDetents(node.props["navigationDetent"], refused: nil)
        guard isTerminalContent(kept), let last = kept.last else {
            return ActiveSummary(activeCount: max(1, sheet.detents.count), terminalContentIdentifier: nil, droppedIdentifiers: [], droppedWords: [])
        }
        let fitId = identifier(last, terminalContent: true)
        let resolvedMax = maximum
            ?? sheet.containerView?.safeAreaLayoutGuide.layoutFrame.height
            ?? sheet.presentedViewController.viewIfLoaded?.window?.safeAreaLayoutGuide.layoutFrame.height
        guard let maxH = resolvedMax, maxH > 0 else {
            return ActiveSummary(activeCount: kept.count, terminalContentIdentifier: fitId, droppedIdentifiers: [], droppedWords: [])
        }
        let content = node.fitContentHeight.map { $0 > 0 ? $0 + above : $0 }
        let rawCeiling = rawCeilingHeight(content, maximum: maxH)
        let verticalSizeClass = sheet.containerView?.traitCollection.verticalSizeClass
            ?? sheet.presentedViewController.traitCollection.verticalSizeClass
        var droppedIds: Set<UISheetPresentationController.Detent.Identifier> = []
        var droppedWords: Set<String> = []
        for word in kept.dropLast() {
            let base = resolvedHeight(word, maximum: maxH, verticalSizeClass: verticalSizeClass)
            if base.map({ $0 >= rawCeiling - 0.5 }) ?? true {
                droppedIds.insert(identifier(word, terminalContent: true))
                droppedWords.insert(String(word.word))
            }
        }
        return ActiveSummary(
            activeCount: kept.count - droppedIds.count,
            terminalContentIdentifier: fitId,
            droppedIdentifiers: droppedIds,
            droppedWords: droppedWords
        )
    }

    /// Re-sync grabber, scroll expansion, undimmed detent, and dropped-selection
    /// clamping after content height or container geometry changes on a sheet
    /// with a terminal `fit-content` ceiling. Preserves the user's dragged detent
    /// while it remains active (LLP 1123 D7).
    @MainActor package static func syncTerminalContent(
        on sheet: UISheetPresentationController,
        node: NodeView,
        owner: String,
        maximum: CGFloat?,
        grabber: String?,
        scrollExpands: String? = nil,
        undimmed undimmedWord: String? = nil,
        syncUndimmed: Bool = true,
        above: CGFloat = 0
    ) {
        let active = activeSummary(for: node, on: sheet, maximum: maximum, above: above)
        updateGrabber(grabber, on: sheet, activeCount: active.activeCount, scrollExpands: scrollExpands)
        if syncUndimmed {
            sheet.largestUndimmedDetentIdentifier = undimmed(undimmedWord, node: node, owner: owner, active: active)
        }
        if let current = sheet.selectedDetentIdentifier,
           active.droppedIdentifiers.contains(current),
           let fitId = active.terminalContentIdentifier {
            sheet.selectedDetentIdentifier = fitId
        } else if sheet.selectedDetentIdentifier == nil {
            sheet.selectedDetentIdentifier = sheet.detents.first { !active.droppedIdentifiers.contains($0.identifier) }?.identifier ?? sheet.detents.first?.identifier
        }
    }

    /// Why a word of a bound `navigationDetent` is dropped.
    package enum DetentRefusal { case unknown, order }

    /// A `navigationDetent`'s words as UIKit detents, in order. A bound value
    /// the compiler did not see may hold a word that names no detent, or one
    /// that is not taller than the detent before it on every screen (LLP
    /// 1075.003 §9.13, the compiler's `detent_out_of_order`): UIKit takes
    /// detents shortest to tallest, and a point height, a share of the
    /// maximum (`medium` is half) and the content's height cross on some
    /// screen, unless `fit-content` stands last as a content-height ceiling
    /// (§9.11, amended 2026-11-27). Each invalid word is skipped and handed to
    /// `refused`, so what is kept ascends on any screen; `100%` is `large`.
    /// `content` is the sheet's laid-out content height, nil or zero before it
    /// is measured.
    package static func detents(
        _ value: String?,
        refused: ((Substring, DetentRefusal) -> Void)? = nil,
        observedMaximum: ((CGFloat) -> Void)? = nil,
        content: @escaping () -> CGFloat?
    ) -> [UISheetPresentationController.Detent] {
        let kept = keptDetents(value, refused: refused)
        let terminal = isTerminalContent(kept)
        return kept.enumerated().map { place, word in
            let followed = place < kept.count - 1
            let id = identifier(word, terminalContent: terminal)
            if terminal {
                if word.kind == .content {
                    return .custom(identifier: id) { context in
                        observedMaximum?(context.maximumDetentValue)
                        return ceilingHeight(content(), maximum: context.maximumDetentValue)
                    }
                }
                // Returning `nil` from a UIKit `.custom` resolver marks that detent inactive.
                return .custom(identifier: id) { context in
                    observedMaximum?(context.maximumDetentValue)
                    guard let base = resolvedHeight(
                        word,
                        maximum: context.maximumDetentValue,
                        verticalSizeClass: context.containerTraitCollection.verticalSizeClass
                    ) else { return nil }
                    let rawCeiling = rawCeilingHeight(content(), maximum: context.maximumDetentValue)
                    return base < rawCeiling - 0.5 ? base : nil
                }
            }
            switch word.kind {
            case .large: return .large()
            case .share where word.word == "medium": return .medium()
            case .share:
                // @ref LLP 1075.003 §9.13 — a share of the sheet's maximum,
                // as `.custom { $0.maximumDetentValue * 0.45 }` is in UIKit.
                let share = CGFloat(word.height)
                return .custom(identifier: id) { context in context.maximumDetentValue * share / 100 }
            case .points:
                // Stopped at the maximum; where a taller detent follows, a
                // height the maximum reaches is inactive (nil), as UIKit's
                // resolver allows, so no two rests tie on a small screen.
                let points = CGFloat(word.height)
                return .custom(identifier: id) { context in
                    followed && points >= context.maximumDetentValue ? nil : min(points, context.maximumDetentValue)
                }
            case .content:
                // @ref LLP 1075.003 §9.11 — the sheet's content, which its
                // box (the sheet) does not change; before it is measured,
                // the sheet's maximum.
                return .custom(identifier: id) { context in
                    observedMaximum?(context.maximumDetentValue)
                    return ceilingHeight(content(), maximum: context.maximumDetentValue)
                }
            }
        }
    }

    private static func resolveIdentifier(
        of word: String,
        in value: String?,
        active: ActiveSummary?
    ) -> UISheetPresentationController.Detent.Identifier? {
        let kept = keptDetents(value, refused: nil)
        guard let found = kept.first(where: { $0.word == word }) else { return nil }
        if let active, active.droppedWords.contains(word), let fitId = active.terminalContentIdentifier {
            return fitId
        }
        return identifier(found, terminalContent: isTerminalContent(kept))
    }

    private enum DetentKind { case points, share, content, large }
    /// A word `detents` keeps: its place among the words, its kind, and its
    /// height within the kind.
    private typealias KeptDetent = (index: Int, word: Substring, kind: DetentKind, height: Double)

    private static func isTerminalContent(_ kept: [KeptDetent]) -> Bool {
        kept.count > 1 && kept.last?.kind == .content
    }

    private static func rawCeilingHeight(_ content: CGFloat?, maximum: CGFloat) -> CGFloat {
        guard let height = content, height > 0 else { return maximum }
        return min(height, maximum)
    }

    private static func ceilingHeight(_ content: CGFloat?, maximum: CGFloat) -> CGFloat {
        guard let height = content, height > 0 else { return maximum }
        return min(height.rounded(.up), maximum)
    }

    /// Resolved height of a static stop under a terminal `fit-content` ceiling,
    /// or `nil` when inactive. UIKit's `Detent.medium().resolvedValue(in:)`
    /// invokes a private selector on non-UIKit resolution contexts, so `medium`
    /// uses `maximum * 0.5` in regular height and `nil` in compact vertical
    /// size class, matching Apple's documented `.medium()` omission via public
    /// API (LLP 1075.003 §9.11, amended 2026-11-27).
    private static func resolvedHeight(
        _ word: KeptDetent,
        maximum: CGFloat,
        verticalSizeClass: UIUserInterfaceSizeClass
    ) -> CGFloat? {
        switch word.kind {
        case .large, .content: return maximum
        case .share where word.word == "medium" && verticalSizeClass == .compact: return nil
        case .share: return maximum * CGFloat(word.height) / 100
        case .points: return CGFloat(word.height)
        }
    }

    private static func keptDetents(_ value: String?, refused: ((Substring, DetentRefusal) -> Void)?) -> [KeptDetent] {
        var kept: [KeptDetent] = []
        for (index, word) in (value ?? "").split(separator: " ").enumerated() {
            let kind: DetentKind, height: Double
            switch word {
            case "large": (kind, height) = (.large, 100)
            case "medium": (kind, height) = (.share, 50)
            case "fit-content": (kind, height) = (.content, 0)
            case _ where word.hasSuffix("%"):
                guard let share = Double(word.dropLast()), share.isFinite, share > 0, share <= 100 else { refused?(word, .unknown); continue }
                // All of the maximum is UIKit's `large`, and is the same
                // detent: one rest, and the status bar's (LLP 1105 D2).
                (kind, height) = share == 100 ? (.large, 100) : (.share, share)
            default:
                guard let points = Double(word), points.isFinite, points > 0 else { refused?(word, .unknown); continue }
                (kind, height) = (.points, points)
            }
            if let last = kept.last {
                let ok: Bool
                switch (last.kind, kind) {
                case (.large, _): ok = false
                case (.content, .large): ok = kept.count == 1
                case (.content, _): ok = false
                case (_, .large), (_, .content): ok = true
                case (let a, let b): ok = a == b && height > last.height
                }
                if !ok {
                    refused?(word, .order)
                    continue
                }
            }
            kept.append((index, word, kind, height))
        }
        return kept
    }

    private static func identifier(_ word: KeptDetent, terminalContent: Bool) -> UISheetPresentationController.Detent.Identifier {
        switch word.kind {
        case .large: return .large
        case .share where word.word == "medium" && !terminalContent: return .medium
        case .content: return .init("fit-content-\(word.index)")
        case .share, .points: return .init("authored-\(word.index)")
        }
    }

    /// The journal's line for a bound `navigationDetent` with words dropped:
    /// the value, its owner, each word and why, and what the sheet uses.
    package static func refusal(detent value: String, words: [(Substring, DetentRefusal)], owner: String, kept: Bool) -> String {
        let why = words.map { word, reason in
            switch reason {
            case .unknown: return "\"\(word)\" is not large, medium, fit-content, a positive point height or a percentage above 0% and at most 100%"
            case .order: return "\"\(word)\" is not taller than the detent before it on every screen (UIKit takes them shortest to tallest; a point height, a share of the maximum and the content's height do not mix, and large goes last)"
            }
        }.joined(separator: "; ")
        return "navigationDetent \"\(value)\" on \(owner): \(why); the sheet uses \(kept ? "its other detents" : "the platform's large detent")"
    }
}
#endif
