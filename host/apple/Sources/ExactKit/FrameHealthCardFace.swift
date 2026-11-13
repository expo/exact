import CExact
import CoreGraphics
import CoreText
import Foundation

/// The card's face above its chart area, drawn by hand on one opaque surface so nothing in it
/// blends (LLP 1079.000 §7.2). iOS and macOS share this layout and drawing; each maps its own
/// touches or clicks to `Layout`'s regions and gives each region an accessibility element.
enum FrameHealthCardFace {
    enum Status { case live, paused, idle }
    enum Control: Equatable {
        case close, title, status, details, share, charts, table
        #if os(iOS)
        case minimize
        #endif
    }
    /// The face's bands, top to bottom, which change apart: a card can draw each as a picture of its own, so a read
    /// that changes the figures draws only theirs
    enum Band: CaseIterable {
        case header, context, figures, pacing, toolbar, charts
        /// Where it lies, from the top of the face, in points
        var span: ClosedRange<CGFloat> {
            switch self {
            case .header: 0...44
            case .context: 44...82
            case .figures: 82...126
            case .pacing: 126...170
            case .toolbar: 170...206
            case .charts: 206...height
            }
        }
    }

    /// What the face shows; the platform view fills it from the reader on each refresh.
    struct State {
        var status = Status.idle
        var request = ("No rate requested", "")
        var display = ""
        /// Two rows, each a window's name, frame rate in whole frames per second, and missed frames, or late ones where
        /// the card measures the screen, which `unit` names
        var rows = [("Last 0.5s", "—", "—"), ("Last 5s", "—", "—")], unit = "missed"
        var pacing = ("—", "")
        /// What the line under the figures gives, pacing or, where the card measures the screen, the main thread's work,
        /// and the window it covers: "last 5s", or the span it actually covers
        var pacingName = "Pacing", pacingWindow = "last 5s"
        var table = false
        /// Details replaces the figures and charts.
        var showsDetails = false
        var pressed: Control?
        var hovered: Control?

        var minimizedText: String {
            let word = status == .paused ? "Paused" : status == .live ? "Live" : "Idle"
            return "\(word) · \(rows.first?.1 ?? "—") FPS"
        }
    }

    static let width: CGFloat = 320, height: CGFloat = 416
    /// Narrower, the header's title, status and buttons would overlap
    static let minimumWidth: CGFloat = 304
    /// Without the toolbar and chart area, where the card would not fit
    static let compactHeight: CGFloat = 176
    static let minimizedWidth: CGFloat = 192, minimizedHeight: CGFloat = 44

    static let body = CTFontCreateUIFontForLanguage(.system, 12, nil)!
    static let digits = FrameHealthCardDrawing.tabular(body)
    static let strong = FrameHealthCardDrawing.tabular(FrameHealthCardDrawing.semibold(12))
    static let title = FrameHealthCardDrawing.semibold(15)

    /// Every region at a card width; sizes are points.
    struct Layout {
        let width: CGFloat
        /// Without the chart area or Charts/Table control
        var compact = false
        var minimized = false
        /// iOS keeps Minimize in the toolbar, beside compact pacing, or below Details.
        var minimizable = false
        /// With Details in place of the figures and charts
        var showsDetails = false
        /// The radii of the card's corners, top left first and clockwise, which round with the screen's in a corner of it
        var corners: (topLeft: CGFloat, topRight: CGFloat, bottomRight: CGFloat, bottomLeft: CGFloat) = (12, 12, 12, 12)
        /// Close and Share sit in the card's top corners, and Details beside Share. Close moves in from a rounder
        /// top-left corner as far as the room the title leaves it.
        var close: CGRect { CGRect(x: 6 + min(Self.adaptation(corners.topLeft), closeRoom), y: 6, width: 32, height: 32) }
        var share: CGRect { CGRect(x: width - 38 - trailingInset, y: 6, width: 32, height: 32) }
        var details: CGRect { CGRect(x: width - 74 - trailingInset, y: 6, width: 32, height: 32) }
        /// The title is the definitions button, with 4 points of fill on either side of its text. It stands 2 points plus
        /// Close's room from Close, so Close can move in without moving it.
        var title: CGRect { CGRect(x: 40 + closeRoom, y: 6, width: Self.titleWidth, height: 32) }
        private static let titleWidth = ceil(FrameHealthCardDrawing.width("Frame health", FrameHealthCardFace.title)) + 8
        /// How far Close can move in from a rounded corner before it would come within 2 points of the title: 6 points of
        /// the header's slack, or as much as a narrower card has
        var closeRoom: CGFloat { min(6, slack) }
        /// How far Details and Share move in from a rounder top-right corner, within the slack Close's room leaves
        var trailingInset: CGFloat { min(Self.adaptation(corners.topRight), slack - closeRoom) }
        /// The header's spare width, with the title 2 points from Close and the status 2 points from Details
        private var slack: CGFloat { max(0, (width - 74) - (40 + Self.titleWidth + 4 + 79) - 2) }
        /// How far a corner rounder than 12 points moves the controls beside it in: 0.29 of the difference, as UIKit's
        /// corner adaptation moves content (`edgeInsets(for: .margins(cornerAdaptation:))`), since Apple's curve
        /// crosses a corner's diagonal 0.29 radii in from each edge
        fileprivate static func adaptation(_ radius: CGFloat) -> CGFloat { max(0, radius - 12) * 0.2915 }
        var status: CGRect { CGRect(x: title.maxX + 4, y: 8.5, width: 79, height: 27) }
        var segments: (CGRect, CGRect) {
            let charts = ceil(FrameHealthCardDrawing.width("Charts", body)) + 18, table = ceil(FrameHealthCardDrawing.width("Table", body)) + 18
            return (CGRect(x: 15, y: 175, width: charts, height: 21), CGRect(x: 15 + charts, y: 175, width: table, height: 21))
        }
        var track: CGRect { let (a, b) = segments; return a.union(b).insetBy(dx: -3, dy: -3) }
        var chartArea: CGRect { CGRect(x: 12, y: 206, width: width - 24, height: 200) }
        /// Rows that change at each refresh, through the toolbar, whose chosen segment can change
        var figures: CGRect { CGRect(x: 0, y: 44, width: width, height: 162) }
        var cardHeight: CGFloat {
            if minimized { return minimizedHeight }
            return compact ? compactHeight : height
        }
        /// Below Details or beside compact pacing, Minimize sits in the bottom-right corner and moves in from a rounder one
        var minimize: CGRect {
            let bottom = showsDetails || compact
            return CGRect(x: width - 100 - (bottom ? Self.adaptation(corners.bottomRight) : 0), y: bottom ? cardHeight - 38 : 170, width: 88, height: 32)
        }
        var pacingArea: CGRect {
            let right = compact && minimizable ? minimize.minX - 8 : width - 12
            return CGRect(x: 12, y: 126, width: right - 12, height: 40)
        }
        /// Details scrolls below the header, leaving room for Minimize where offered.
        var detailsArea: CGRect { CGRect(x: 0, y: 44, width: width, height: max(0, cardHeight - 44 - (minimizable ? 44 : 0))) }

        /// The card's outline 2 points in, which a button's fill keeps within
        var inner: CGPath {
            FrameHealthCardDrawing.continuous(CGRect(x: 2, y: 2, width: width - 4, height: cardHeight - 4),
                radii: [corners.topLeft, corners.topRight, corners.bottomRight, corners.bottomLeft].map { $0 - 2 })
        }
        /// Where a bottom corner rounds more than 12 points, `inner`, which the text beside it keeps within by moving in
        /// or up; elsewhere nil, since the face's 12-point margins clear a 12-point curve
        var bottomOutline: CGPath? { max(corners.bottomRight, corners.bottomLeft) > 12 ? inner : nil }

        /// The control under a point, with each button's 44-point touch target; where two targets
        /// overlap, the nearer button.
        func control(at point: CGPoint) -> Control? {
            func target(_ rect: CGRect) -> CGRect { rect.insetBy(dx: min(0, (rect.width - 44) / 2), dy: min(0, (rect.height - 44) / 2)) }
            func distance(_ rect: CGRect) -> CGFloat { max(rect.minX - point.x, point.x - rect.maxX, 0) + max(rect.minY - point.y, point.y - rect.maxY, 0) }
            #if os(iOS)
            if minimized { return CGRect(x: 0, y: 0, width: width, height: cardHeight).contains(point) ? .minimize : nil }
            if minimizable, target(minimize).contains(point) { return .minimize }
            #endif
            var buttons = [(Control.close, close), (.title, title), (.status, status), (.details, details), (.share, share)]
            if !compact, !showsDetails { buttons += [(.charts, segments.0), (.table, segments.1)] }
            return buttons.filter { target($0.1).contains(point) }.min { distance($0.1) < distance($1.1) }?.0
        }
    }

    /// A press on the card: anywhere on it, a button included, it drags once it moves 4 points, and
    /// one that does not move is a tap. Points are in the card's container.
    struct Drag {
        let start: CGPoint, origin: CGPoint, control: Control?
        var moved = false
        /// The press's latest samples, each its time, x and y, oldest first: as many as the engine's velocity estimator
        /// reads at release (`exact_velocity`; LLP 1057.001 §3)
        private var samples: [Double] = []
        init(at point: CGPoint, time: TimeInterval, origin: CGPoint, control: Control?) {
            (start, self.origin, self.control) = (point, origin, control)
            samples.reserveCapacity(16 * 3)
            track(point, at: time)
        }
        mutating func track(_ point: CGPoint, at time: TimeInterval) {
            if samples.count == 16 * 3 { samples.removeFirst(3) }
            samples.append(time)
            samples.append(Double(point.x))
            samples.append(Double(point.y))
        }
        /// The velocity a release at `time` carries, in points a second: a press held still for 100 ms has none.
        func velocity(releasedAt time: TimeInterval) -> CGVector {
            var velocity = [0.0, 0.0]
            exact_velocity(samples, samples.count / 3, time, &velocity)
            return CGVector(dx: velocity[0], dy: velocity[1])
        }
    }

    /// A 0.4-second spring response with a damping ratio of 0.8 and unit mass
    static let spring = (stiffness: pow(2 * CGFloat.pi / 0.4, 2), damping: 2 * 0.8 * (2 * CGFloat.pi / 0.4))

    /// The area along whose edges the card rests: 8 points inside its window, ignoring safe-area insets (LLP 1079.000
    /// §7.2). On iOS the overlay also keeps it above the keyboard.
    static func restingArea(in bounds: CGRect) -> CGRect { bounds.insetBy(dx: 8, dy: 8) }

    /// Store fractions of the card's available travel to preserve its position along an edge when the window
    /// or card resizes.
    struct Position {
        var x: CGFloat, y: CGFloat
        init(x: CGFloat = 0, y: CGFloat = 0) { (self.x, self.y) = (x, y) }

        init(nearest origin: CGPoint, velocity: CGVector, size: CGSize, in area: CGRect) {
            // Project the release using normal scroll deceleration: 0.998 of the velocity each millisecond
            let glide = 0.998 / (1 - 0.998) / 1000
            let width = max(0, area.width - size.width), height = max(0, area.height - size.height)
            let dx = min(width, max(0, origin.x + velocity.dx * glide - area.minX))
            let dy = min(height, max(0, origin.y + velocity.dy * glide - area.minY))
            x = width > 0 ? dx / width : 0
            y = height > 0 ? dy / height : 0
            let horizontal = min(dx, width - dx), vertical = min(dy, height - dy)
            if horizontal <= vertical {
                x = dx <= width / 2 ? 0 : 1
            } else {
                y = dy <= height / 2 ? 0 : 1
            }
        }

        func origin(of size: CGSize, in area: CGRect) -> CGPoint {
            CGPoint(x: area.minX + x * max(0, area.width - size.width),
                y: area.minY + y * max(0, area.height - size.height))
        }
    }

    /// Follow the press within `area` and resist movement past its edges like a scroll view. On release, the card
    /// springs back to an edge.
    static func resisted(_ origin: CGPoint, size: CGSize, in area: CGRect) -> CGPoint {
        func axis(_ value: CGFloat, _ low: CGFloat, _ high: CGFloat, _ span: CGFloat) -> CGFloat {
            let high = max(low, high)
            // Past an edge by `x`, the card goes (1 - 1 / (0.55 x / span + 1)) span, never a whole span
            func resist(_ x: CGFloat) -> CGFloat { (1 - 1 / (0.55 * x / span + 1)) * span }
            return value < low ? low - resist(low - value) : value > high ? high + resist(value - high) : value
        }
        return CGPoint(x: axis(origin.x, area.minX, area.maxX - size.width, area.width),
            y: axis(origin.y, area.minY, area.maxY - size.height, area.height))
    }

    /// Draw the header, context, figures, pacing and, unless compact, the toolbar into the layout's width × 206 points,
    /// or only those of `bands`.
    static func draw(_ ctx: CGContext, _ layout: Layout, state: State, palette p: FrameHealthCardDrawing.Palette,
                     bands: Set<Band> = Set(Band.allCases)) {
        let (width, compact) = (layout.width, layout.compact)
        ctx.setFillColor(p.background)
        ctx.fill(CGRect(x: 0, y: 0, width: width, height: 206))
        typealias D = FrameHealthCardDrawing
        if layout.minimized {
            if state.pressed != nil || state.hovered != nil {
                ctx.setFillColor(state.pressed != nil ? p.iconPressed : p.iconHover)
                ctx.fill(CGRect(x: 0, y: 0, width: width, height: minimizedHeight))
            }
            let c = layout.corners
            let left = 12 + Layout.adaptation(max(c.topLeft, c.bottomLeft)), right = 12 + Layout.adaptation(max(c.topRight, c.bottomRight))
            D.text(ctx, state.minimizedText, x: left, y: 22, strong, p.label, .left, limit: width - left - right - 24)
            Symbol.restore.draw(ctx, in: CGRect(x: width - right - 16, y: 14, width: 16, height: 16), color: p.secondary)
            return
        }
        if bands.contains(.header) { header(ctx, layout, state: state, palette: p) }
        guard !state.showsDetails else { return }
        if bands.contains(.context) {
            // Context: the request and any system limit, then the display
            let request = state.request.0 + (state.request.1.isEmpty ? "" : " ")
            D.text(ctx, request, x: 12, y: 52, body, p.label, .left, limit: width - 24)
            if !state.request.1.isEmpty {
                D.text(ctx, "· " + state.request.1, x: 12 + D.width(request, body), y: 52, body, p.secondary, .left)
            }
            D.text(ctx, state.display, x: 12, y: 68, body, p.secondary, .left, limit: width - 24)
            separator(ctx, y: 80, width: width, p)
        }
        if bands.contains(.figures) {
            // One row per window, digits right-aligned in reserved tabular columns so nothing moves
            let digits = D.width("000", strong), count = D.width("0000", strong)
            for (index, row) in state.rows.enumerated() {
                let y = 94 + CGFloat(index) * 18
                D.text(ctx, row.0, x: 12, y: y, body, p.secondary, .left)
                D.text(ctx, row.1, x: 78 + digits, y: y, strong, p.label, .right)
                D.text(ctx, "FPS", x: 78 + digits + 3.6, y: y, body, p.label, .left)
                D.text(ctx, row.2, x: 162 + count, y: y, strong, p.label, .right)
                D.text(ctx, state.unit, x: 162 + count + 3.6, y: y, body, p.label, .left)
            }
            separator(ctx, y: 125, width: width, p)
        }
        if bands.contains(.pacing) {
            // A compact card ends with these lines, whose starts move in together where a bottom corner rounds
            let outline = layout.bottomOutline, area = layout.pacingArea
            let x = outline.map { D.fit(D.box(state.pacing.1, x: 12, y: 154, Self.digits, .left), .left, limit: area.width, within: $0).x } ?? 12
            let label = "\(state.pacingName) (\(state.pacingWindow))", verdict = x + D.width(label, body) + 3.6
            D.text(ctx, label, x: x, y: 138, body, p.secondary, .left)
            D.text(ctx, state.pacing.0, x: verdict, y: 138, strong, p.label, .left, limit: area.maxX - verdict, within: outline)
            D.text(ctx, state.pacing.1, x: x, y: 154, Self.digits, p.label, .left, limit: area.maxX - x, within: outline)
        }
        guard !compact, bands.contains(.toolbar) else { return }
        // The segmented control: a track, and the chosen segment a chip of the card's ground
        let track = layout.track
        ctx.setFillColor(p.track)
        ctx.addPath(D.continuous(track, radius: 8))
        ctx.fillPath()
        let (charts, table) = layout.segments
        for (rect, name, control, chosen) in [(charts, "Charts", Control.charts, !state.table), (table, "Table", .table, state.table)] {
            // The chosen segment is a chip of the card's ground; the other washes when hovered or pressed
            let fill = chosen ? p.chip : state.pressed == control ? p.statusPressed : state.hovered == control ? p.statusHover : nil
            if let fill {
                ctx.setFillColor(fill)
                ctx.addPath(D.continuous(rect, radius: 6))
                ctx.fillPath()
            }
            let w = D.width(name, body)
            D.text(ctx, name, x: rect.midX - w / 2, y: rect.midY, body, chosen || state.hovered == control ? p.label : p.secondary, .left)
        }
    }

    #if os(iOS)
    /// Minimize, at `layout.minimize`. Its fill keeps 2 points within the card's outline, as the header buttons' do, so
    /// in a rounder bottom-right corner its outer corner follows the card's.
    static func minimize(_ ctx: CGContext, _ layout: Layout, state: State, palette p: FrameHealthCardDrawing.Palette) {
        typealias D = FrameHealthCardDrawing
        let rect = layout.minimize
        if let fill = state.pressed == .minimize ? p.iconPressed : state.hovered == .minimize ? p.iconHover : nil {
            ctx.saveGState()
            ctx.addPath(layout.inner)
            ctx.clip()
            ctx.setFillColor(fill)
            ctx.addPath(D.continuous(rect, radius: 8))
            ctx.fillPath()
            ctx.restoreGState()
        }
        D.text(ctx, "Minimize", x: rect.midX - D.width("Minimize", body) / 2, y: rect.midY, body, p.label, .left)
    }
    #endif

    /// The header: Close, the title, the status, Details and Share.
    private static func header(_ ctx: CGContext, _ layout: Layout, state: State, palette p: FrameHealthCardDrawing.Palette) {
        typealias D = FrameHealthCardDrawing
        // A button's fill keeps 2 points within the card's outline, so in a rounded corner its outer corner follows the card's
        ctx.saveGState()
        ctx.addPath(layout.inner)
        ctx.clip()
        for (control, rect) in [(Control.close, layout.close), (.title, layout.title), (.details, layout.details), (.share, layout.share)] {
            // Details keeps the status's fill while it shows, as a toggled button does
            let shown = control == .details && state.showsDetails
            let fill = state.pressed == control ? shown ? p.statusPressed : p.iconPressed
                : state.hovered == control ? shown ? p.statusHover : p.iconHover : shown ? p.statusFill : nil
            guard let fill else { continue }
            ctx.setFillColor(fill)
            ctx.addPath(D.continuous(rect, radius: 8))
            ctx.fillPath()
        }
        ctx.restoreGState()
        Symbol.close.draw(ctx, in: layout.close.insetBy(dx: 6.6, dy: 6.6), color: p.secondary)
        D.text(ctx, "Frame health", x: layout.title.minX + 4, y: 22, title, p.label, .left)
        Symbol.details.draw(ctx, in: layout.details.insetBy(dx: 6.6, dy: 6.6), color: p.secondary)
        Symbol.share.draw(ctx, in: layout.share.insetBy(dx: 6.6, dy: 6.6), color: p.secondary)
        // The status is the pause button: its symbol and word say the state
        let status = layout.status
        ctx.setFillColor(state.pressed == .status ? p.statusPressed : state.hovered == .status ? p.statusHover : p.statusFill)
        ctx.addPath(CGPath(roundedRect: status, cornerWidth: status.height / 2, cornerHeight: status.height / 2, transform: nil))
        ctx.fillPath()
        let symbol = CGRect(x: status.minX + 9, y: status.midY - 6.6, width: 13.2, height: 13.2)
        switch state.status {
        case .paused: Symbol.paused.draw(ctx, in: symbol, color: p.gray)
        case .live: Symbol.live.draw(ctx, in: symbol, color: p.green)
        case .idle: Symbol.live.draw(ctx, in: symbol, color: p.gray)
        }
        let word = state.status == .paused ? "Paused" : state.status == .live ? "Live" : "Idle"
        D.text(ctx, word, x: symbol.maxX + 4.8, y: status.midY, strong, p.label, .left)
    }

    /// The table: for the last 5s and 30s, the figures the face does not show, then the deadline.
    static func table(_ ctx: CGContext, _ rect: CGRect, columns: [String], rows: [(String, [String])], caption: String,
                      palette p: FrameHealthCardDrawing.Palette, within outline: CGPath? = nil) {
        typealias D = FrameHealthCardDrawing
        ctx.setFillColor(p.background)
        ctx.fill(rect)
        let first = rect.width * 0.46, column = (rect.width - first) / CGFloat(max(1, columns.count))
        var y = rect.minY
        for (index, row) in ([("", columns)] + rows).enumerated() {
            D.text(ctx, row.0, x: rect.minX, y: y + 12.5, body, p.secondary, .left)
            for (i, value) in row.1.enumerated() {
                D.text(ctx, value, x: rect.minX + first + CGFloat(i + 1) * column, y: y + 12.5, index == 0 ? body : strong,
                    index == 0 ? p.secondary : p.label, .right)
            }
            y += 25
            separator(ctx, y: y - 1, from: rect.minX, to: rect.maxX, p)
        }
        D.text(ctx, caption, x: rect.minX, y: y + 6 + 8, body, p.secondary, .left, limit: rect.width, within: outline)
    }

    /// Details' rows, then its caption, from the top of a surface `width` points wide: a result with its
    /// reason under it takes 36 points, a count 25.
    static func details(_ ctx: CGContext, width: CGFloat, rows: [FrameHealthCardText.Detail], caption: String,
                        palette p: FrameHealthCardDrawing.Palette) {
        typealias D = FrameHealthCardDrawing
        var y: CGFloat = 0
        for row in rows {
            let height = height(row), right = width - 12
            D.text(ctx, row.name, x: 12, y: y + height / 2, body, p.secondary, .left)
            let name = 12 + D.width(row.name, body) + 12
            if let reason = row.reason {
                D.text(ctx, row.value, x: right, y: y + 11, strong, p.label, .right, limit: right - name)
                D.text(ctx, reason, x: right, y: y + 25, body, p.secondary, .right, limit: right - name)
            } else {
                D.text(ctx, row.value, x: right, y: y + height / 2, strong, p.label, .right, limit: right - name)
            }
            y += height
            separator(ctx, y: y - 1, width: width, p)
        }
        for line in D.lines(caption, width: width - 24, body) {
            y += 16
            D.text(ctx, line, x: 12, y: y, body, p.secondary, .left)
        }
    }

    /// The height Details' rows and caption take, which scroll where it is more than the area's.
    static func detailsHeight(width: CGFloat, rows: [FrameHealthCardText.Detail], caption: String, clearing layout: Layout? = nil) -> CGFloat {
        let total = rows.reduce(0) { $0 + height($1) } + CGFloat(FrameHealthCardDrawing.lines(caption, width: width - 24, body).count) * 16 + 10
        guard let layout, let outline = layout.bottomOutline else { return total }
        // Scrolled to its end, the last line, 10 points above it, rises clear of a rounder bottom corner
        let line = CTFontGetAscent(body) + CTFontGetDescent(body)
        return total + FrameHealthCardDrawing.rise(CGRect(x: 12, y: layout.detailsArea.maxY - 10 - line / 2, width: width - 24, height: line),
            within: outline)
    }

    private static func height(_ row: FrameHealthCardText.Detail) -> CGFloat { row.reason == nil ? 25 : 36 }

    private static func separator(_ ctx: CGContext, y: CGFloat, width: CGFloat, _ p: FrameHealthCardDrawing.Palette) {
        separator(ctx, y: y, from: 12, to: width - 12, p)
    }

    private static func separator(_ ctx: CGContext, y: CGFloat, from: CGFloat, to: CGFloat, _ p: FrameHealthCardDrawing.Palette) {
        ctx.setFillColor(p.separator)
        ctx.fill(CGRect(x: from, y: y, width: to - from, height: 1))
    }

    /// Exact's symbol roles as 24-unit paths, stroked at 1.85 with round caps and joins.
    enum Symbol {
        case live, paused, close, details, share, restore

        func draw(_ ctx: CGContext, in rect: CGRect, color: CGColor) {
            let path = CGMutablePath()
            func lines(_ points: [(CGFloat, CGFloat)]) {
                path.move(to: CGPoint(x: points[0].0, y: points[0].1))
                for point in points.dropFirst() { path.addLine(to: CGPoint(x: point.0, y: point.1)) }
            }
            var solid = false
            switch self {
            case .live: lines([(2, 12), (6, 12), (9, 4), (15, 20), (18, 12), (22, 12)])
            case .paused:
                solid = true
                path.addRect(CGRect(x: 6, y: 4, width: 4, height: 16))
                path.addRect(CGRect(x: 14, y: 4, width: 4, height: 16))
            case .close:
                lines([(6, 6), (18, 18)])
                lines([(18, 6), (6, 18)])
            case .restore:
                lines([(4, 10), (4, 4), (10, 4)])
                lines([(14, 20), (20, 20), (20, 14)])
                lines([(4, 4), (10, 10)])
                lines([(14, 14), (20, 20)])
            case .details:
                // Rows of text with a bullet before each, as the system's list.bullet
                for y: CGFloat in [6, 12, 18] {
                    lines([(5, y), (5.01, y)])
                    lines([(9, y), (20, y)])
                }
            case .share:
                // A box open at the top and an arrow up out of it, as the system's square.and.arrow.up
                lines([(8, 9), (5, 9), (5, 21), (19, 21), (19, 9), (16, 9)])
                lines([(12, 3), (12, 15)])
                lines([(8, 7), (12, 3), (16, 7)])
            }
            ctx.saveGState()
            ctx.translateBy(x: rect.minX, y: rect.minY)
            ctx.scaleBy(x: rect.width / 24, y: rect.height / 24)
            ctx.addPath(path)
            if solid {
                ctx.setFillColor(color)
                ctx.fillPath()
            } else {
                ctx.setStrokeColor(color)
                ctx.setLineWidth(1.85)
                ctx.setLineCap(.round)
                ctx.setLineJoin(.round)
                ctx.strokePath()
            }
            ctx.restoreGState()
        }
    }
}

