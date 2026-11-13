// @ref LLP 1044.000 §6 S1 — textual descendants are paragraph values.
import Foundation
import CoreText

package struct InlineText {
    package let id: UInt32
    package let parent: UInt32
    package let props: [String: String]
    package let lightRun: Run
    package let darkColor: [Double]?
    package let darkBackground: [Double]?
    package let colorRef: BatchValue?
    package let backgroundRef: BatchValue?
    /// Its `color` is one its author fixed (`BatchValue.fixesInk`).
    package let fixedInk: Bool
    package let paint: RunPaintRows
    package let hasSchemeColor: Bool

    package let handlers: Set<String>
    package let paints: Bool
    /// Its presented `opacity` times its enclosing runs': it has no layer,
    /// so its paint is drawn at this alpha (CSS Color 3 §3.2).
    package let opacity: Double
    /// CSS `user-select: all` on the run: a selection that takes part of its
    /// text takes all of it (`TextSelection.atom`).
    package let selectsAll: Bool
    /// CSS `outline: none` on the run (LLP 1104 D6): a focused link or
    /// `tabindex` run draws no ring.
    package let outlineNone: Bool
    /// An image's box (LLP 1011.002) with `visibility: visible`: its label is
    /// read. The box itself draws no ink either way.
    package let boxShown: Bool
    /// Its computed `visibility` is not `visible`: its text is no target
    /// (`inlineTarget`), though it hears a visible run's click inside it.
    package var hidden: Bool { lightRun.hidden }
    package var range: NSRange = NSRange(location: 0, length: 0)

    package init(id: UInt32, parent: UInt32, props: [String: String], style: InlineStyle,
         handlers: Set<String>, paints: Bool, opacity: Double = 1) {
        self.id = id; self.parent = parent; self.props = props
        var value = style.run
        value.text = props["text"] ?? ""
        if value.background != nil || style.backgroundRef != nil { value.backgroundBox = id } else { value.backgroundRadii = [] }
        value.href = props["href"] ?? ""
        // The room its inline boxes take beside its text, as the kernel measured it ("start end").
        if let edges = props["inlineEdges"]?.split(separator: " ").compactMap({ Float($0) }), edges.count == 2 {
            value.inlineStart = CGFloat(edges[0]); value.inlineEnd = CGFloat(edges[1])
        }
        // Its padding box's reach past its text, which its background covers.
        if let pad = props["inlinePad"]?.split(separator: " ").compactMap({ Float($0) }), pad.count == 2 {
            value.backgroundBefore = CGFloat(pad[0]); value.backgroundAfter = CGFloat(pad[1])
        }
        // An image in the text is its box, with no ink or paint of its own (LLP 1011.002).
        boxShown = props["inlineBoxHidden"] != "true"
        if let box = InlineBox(prop: props["inlineBox"]) {
            value.box = box; value.hidden = true
            value.background = nil; value.backgroundBox = 0; value.backgroundRadii = []
            value.decoration = ""; value.shadow = nil; value.stroke = nil
        }
        lightRun = value
        darkColor = style.darkColor
        darkBackground = style.darkBackground
        colorRef = style.colorRef; backgroundRef = style.backgroundRef
        fixedInk = style.fixedInk
        paint = style.paint
        hasSchemeColor = style.paired || style.paint.paired
        selectsAll = style.selectsAll
        outlineNone = style.outlineNone
        self.handlers = handlers; self.paints = paints; self.opacity = opacity
    }

    package var text: String { lightRun.text }
    /// A run with an `href`: the web's `<a>` in its paragraph.
    package var isLink: Bool { !(props["href"] ?? "").isEmpty }

}

/// A box with a background around a run's own (`Run.background`), which
/// paints under it: its colour, node, `border-radius` and padding box's
/// reach past its first and last characters, as the run's own fields.
package struct OuterBackground: Hashable {
    package var color: [Double]
    package var box: UInt32
    package var radii: [InlineRadius]
    package var before: CGFloat, after: CGFloat
    package init(color: [Double], box: UInt32, radii: [InlineRadius], before: CGFloat, after: CGFloat) {
        self.color = color; self.box = box; self.radii = radii; self.before = before; self.after = after
    }
}

/// One corner of an inline box's `border-radius`: px plus a percentage, or
/// a `calc()`, of the fragment's width across and its height down.
package struct InlineRadius: Hashable {
    package var px: Double = 0
    package var pct: Double = 0
    package var calc: CalculatedLength?
    package init(px: Double = 0, pct: Double = 0) { self.px = px; self.pct = pct }
    /// A radius row as the wire carries it.
    package init(_ value: BatchValue) {
        switch value {
        case .number(let n): px = n
        case .calculation(let c): calc = c
        case .object(let o):
            if let c = CalculatedLength(o) { calc = c } else { px = o["px"]?.number ?? 0; pct = o["pct"]?.number ?? 0 }
        default: break
        }
    }
    package var isZero: Bool { px == 0 && pct == 0 && calc == nil }
    package func length(basis: CGFloat) -> CGFloat {
        max(0, calc?.length(basis: basis) ?? CGFloat(px) + CGFloat(pct) * basis / 100)
    }
    package static func == (a: InlineRadius, b: InlineRadius) -> Bool { a.px == b.px && a.pct == b.pct && a.calc == b.calc }
    package func hash(into hasher: inout Hasher) {
        hasher.combine(px); hasher.combine(pct); hasher.combine(calc?.expression); hasher.combine(calc?.environment)
    }
}

// Resolved while reading a paragraph; no dictionary of unused layout rows.
package struct InlineStyle {
    package init() {}
    package var run = Run(text: "", size: 16, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0)
    package var darkColor: [Double]?
    package var darkBackground: [Double]?
    /// A platform colour by name, resolved when the run is read (LLP 1095 D5).
    package var colorRef: BatchValue?
    package var backgroundRef: BatchValue?
    /// Whether `color` is one its author fixed, not a platform colour by name.
    package var fixedInk = false
    /// `text-shadow` and `-webkit-text-stroke`, computed for the run (LLP 1077).
    package var paint = RunPaintRows()
    package var paired = false
    /// `user-select: all` (`InlineText.selectsAll`).
    package var selectsAll = false
    /// `outline: none` (`InlineText.outlineNone`).
    package var outlineNone = false
}

