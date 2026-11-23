#if os(iOS)
import UIKit

/// The resting heights of a UIKit sheet presenting a route or a dialog.
package enum SheetDetents {
    /// Replacing the resting heights also chooses the initial one. Updating a
    /// grabber on its own preserves the height the person dragged the sheet to.
    @MainActor package static func select(on sheet: UISheetPresentationController, node: NodeView, owner: String) {
        let value = node.props["navigationDetent"]
        var refused: [(Substring, DetentRefusal)] = []
        let heights = detents(value, refused: { refused.append(($0, $1)) }) { [weak node] in node?.fitContentHeight }
        sheet.detents = heights.isEmpty ? [.large()] : heights
        if !refused.isEmpty {
            node.presenter?.session?.log(refusal(detent: value ?? "", words: refused, owner: owner, kept: !heights.isEmpty))
        }
        let initial = node.props["navigationInitialDetent"] ?? ""
        let opening = identifier(of: initial, in: heights.isEmpty ? "large" : value)
        if !initial.isEmpty, opening == nil {
            node.presenter?.session?.log("navigationInitialDetent \"\(initial)\" on \(owner) names none of the detents the sheet kept from navigationDetent \"\(value ?? "")\"; the sheet opens at its first")
        }
        sheet.selectedDetentIdentifier = opening ?? sheet.detents.first?.identifier
    }

    @MainActor package static func updateGrabber(_ value: String?, on sheet: UISheetPresentationController) {
        sheet.prefersGrabberVisible = value == "visible" || value != "hidden" && sheet.detents.count > 1
        sheet.prefersScrollingExpandsWhenScrolledToEdge = sheet.detents.count > 1
    }

    /// Why a word of a bound `navigationDetent` is dropped.
    package enum DetentRefusal { case unknown, order }

    /// A `navigationDetent`'s words as UIKit detents, in order. A bound value
    /// the compiler did not see may hold a word that names no detent, or one
    /// that is not taller than the detent before it on every screen (LLP
    /// 1075.003 §9.13, the compiler's `detent_out_of_order`): UIKit takes
    /// detents shortest to tallest, and a point height, a share of the
    /// maximum (`medium` is half) and the content's height cross on some
    /// screen. Each is skipped and handed to `refused`, so what is kept
    /// ascends on any screen; `100%` is `large`. `content` is the sheet's
    /// laid-out content height, nil or zero before it is measured.
    package static func detents(_ value: String?, refused: ((Substring, DetentRefusal) -> Void)? = nil, content: @escaping () -> CGFloat?) -> [UISheetPresentationController.Detent] {
        let kept = keptDetents(value, refused: refused)
        return kept.enumerated().map { place, word in
            let followed = place < kept.count - 1
            switch word.kind {
            case .large: return .large()
            case .share where word.word == "medium": return .medium()
            case .share:
                // @ref LLP 1075.003 §9.13 — a share of the sheet's maximum,
                // as `.custom { $0.maximumDetentValue * 0.45 }` is in UIKit.
                let share = CGFloat(word.height)
                return .custom(identifier: identifier(word)) { context in context.maximumDetentValue * share / 100 }
            case .points:
                // Stopped at the maximum; where a taller detent follows, a
                // height the maximum reaches is inactive (nil), as UIKit's
                // resolver allows, so no two rests tie on a small screen.
                let points = CGFloat(word.height)
                return .custom(identifier: identifier(word)) { context in
                    followed && points >= context.maximumDetentValue ? nil : min(points, context.maximumDetentValue)
                }
            case .content:
                // @ref LLP 1075.003 §9.11 — the sheet's content, which its
                // box (the sheet) does not change; before it is measured,
                // the sheet's maximum.
                return .custom(identifier: identifier(word)) { context in
                    guard let height = content(), height > 0 else { return context.maximumDetentValue }
                    return min(height.rounded(.up), context.maximumDetentValue)
                }
            }
        }
    }

    /// The identifier of the detent `word` names among `value`'s words, as
    /// `detents` makes it; nil when it is none of the words kept.
    private static func identifier(of word: String, in value: String?) -> UISheetPresentationController.Detent.Identifier? {
        keptDetents(value, refused: nil).first { $0.word == word }.map(identifier)
    }

    private enum DetentKind { case points, share, content, large }
    /// A word `detents` keeps: its place among the words, its kind, and its
    /// height within the kind.
    private typealias KeptDetent = (index: Int, word: Substring, kind: DetentKind, height: Double)

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
            if let last = kept.last, last.kind == .large || kind != .large && (last.kind != kind || height <= last.height) {
                refused?(word, .order)
                continue
            }
            kept.append((index, word, kind, height))
        }
        return kept
    }

    private static func identifier(_ word: KeptDetent) -> UISheetPresentationController.Detent.Identifier {
        switch word.kind {
        case .large: return .large
        case .share where word.word == "medium": return .medium
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
