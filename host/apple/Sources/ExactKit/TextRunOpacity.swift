import CoreText
import Foundation

extension TextLinePaint {
    /// Keep the box's background, decorations, glyphs and shadows inside its
    /// opacity layer. The view already composites the paragraph's own opacity.
    static func drawOpacity(_ line: CTLine, at origin: CGPoint, in ctx: CGContext, scale: CGFloat, pass: Pass) -> Bool {
        drawOpacity([line], at: [origin], in: ctx, scale: scale, pass: pass)
    }

    static func drawOpacity(_ lines: [CTLine], at origins: [CGPoint], in ctx: CGContext, scale: CGFloat, pass: Pass, background: (([CTRun], CGPoint) -> Void)? = nil) -> Bool {
        struct Glyph { let line: Int; let run: Int }
        let runs = lines.map { CTLineGetGlyphRuns($0) as! [CTRun] }
        let opacity = runs.map { $0.map { (CTRunGetAttributes($0) as NSDictionary)[NSAttributedString.Key.exactOpacity] as? [InlineOpacity] ?? [] } }
        guard opacity.contains(where: { $0.contains(where: { !$0.isEmpty }) }) else { return false }
        func offset(_ glyph: Glyph) -> Int { CTRunGetStringRange(runs[glyph.line][glyph.run]).location }
        struct Group {
            var alpha: Double = 1
            var first = Int.max
            var children: [Int] = []
            var runs: [Glyph] = []
            var fills: [Fill] = []
        }
        var groups = [Group()], byId: [UInt32: Int] = [:]
        func group(_ chain: [InlineOpacity], at offset: Int) -> Int {
            var parent = 0
            for step in chain {
                let index: Int
                if let found = byId[step.box] { index = found }
                else {
                    index = groups.count
                    groups.append(Group(alpha: step.alpha))
                    byId[step.box] = index
                    groups[parent].children.append(index)
                }
                groups[index].first = min(groups[index].first, offset)
                parent = index
            }
            return parent
        }
        for (line, glyphs) in runs.enumerated() {
            for (index, run) in glyphs.enumerated() {
                let owner = group(opacity[line][index], at: CTRunGetStringRange(run).location)
                groups[owner].runs.append(Glyph(line: line, run: index))
            }
            if pass.paintsBackgrounds {
                for fill in backgrounds(lines[line], at: origins[line]) {
                    let owner = group(fill.opacity, at: Int.max)
                    groups[owner].fills.append(fill)
                }
            }
        }
        func glyphs(_ selected: [Glyph]) {
            guard !selected.isEmpty else { return }
            let byLine = Dictionary(grouping: selected, by: \.line)
            for line in byLine.keys.sorted() {
                background?(byLine[line]!.map { runs[line][$0.run] }, origins[line])
                guard pass.paintsGlyphs else { continue }
                ctx.saveGState()
                let matrix = ctx.textMatrix
                ctx.translateBy(x: origins[line].x, y: origins[line].y)
                ctx.scaleBy(x: 1, y: -1)
                ctx.textMatrix = .identity
                ctx.textPosition = .zero
                _ = drawShadowed(lines[line], in: ctx, scale: scale, pass: pass, selected: Set(byLine[line]!.map(\.run)))
                ctx.textMatrix = matrix
                ctx.restoreGState()
            }
        }
        func paint(_ index: Int) {
            let g = groups[index]
            guard g.alpha > 0 else { return }
            ctx.saveGState()
            if g.alpha < 1 {
                ctx.setAlpha(g.alpha)
                ctx.beginTransparencyLayer(auxiliaryInfo: nil)
            }
            for fill in g.fills {
                ctx.setFillColor(fill.color)
                if let path = fill.path { ctx.addPath(path); ctx.fillPath() } else { ctx.fill(fill.rect) }
            }
            let children = g.children.sorted { groups[$0].first < groups[$1].first }
            var remaining = g.runs.sorted { offset($0) < offset($1) }
            for child in children {
                let before = remaining.prefix { offset($0) < groups[child].first }
                glyphs(Array(before))
                remaining.removeFirst(before.count)
                paint(child)
            }
            glyphs(remaining)
            if g.alpha < 1 { ctx.endTransparencyLayer() }
            ctx.restoreGState()
        }
        paint(0)
        return true
    }
}
