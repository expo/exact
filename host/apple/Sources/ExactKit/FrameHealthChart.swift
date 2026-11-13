import Foundation
import QuartzCore

/// The card's charts, fed by each frame the app draws (LLP 1079.000 §7.2). The frame-rate chart
/// plots a moving average over the last half second, and the frame-time chart each frame's own frame
/// time and work, or, measuring the screen, the main thread's long spells of work (§7.4), kept as 30
/// seconds of tenth-of-a-second columns. Each plot's gridlines and lines are one layer, wider than the
/// plot by five seconds of axis, whose opaque tiles, about a second of axis each, render themselves as
/// images. The frame hooks only store each frame; `present()`, after them, moves the layers left by
/// whole device pixels, and draws the tiles frames changed once a column has ended since they were
/// drawn, so at most ten times a second. As a layer nears the end of its reach, its tiles' pictures
/// move left by whole tiles, and only the tiles that come in are drawn: the plots scroll rather than
/// draw again, which keeps the card's work in the app's frames small (LLP 1079.000 §7.4). While the card
/// follows the time, the layers move with it between frames too, until 30 seconds pass without one;
/// measuring the screen, they glide, Core Animation moving them at the rate iOS chooses.
/// Held, as the card pauses, the layers keep the picture they showed, and the charts take no frames, so the paused span
/// is a gap.
/// Shared by iOS and macOS.
final class FrameHealthChart {
    /// A tenth of a second of frames.
    struct Column {
        /// Which tenth of a second, counted from the clock's origin; -1 for none
        var index: Int64 = -1
        /// The frame rate's moving average at the column's newest frame; zero without one
        var fps: Float = 0
        /// Milliseconds: the longest frame time, and the slowest work with that frame's budget, or the longest spell of
        /// the main thread's work with one refresh at the display's highest rate
        var frame: Float = 0, work: Float = 0, budget: Float = 0
        /// Its first frame continued a run from an earlier column, so its lines reach back to that
        /// column's across any empty columns between
        var joined = false
    }

    static let columnSeconds = 0.1, span = 30.0, reach = 5.0, average = 0.5
    /// Held only while the card reads, from `start()` to `stop()`
    private(set) var columns: [Column] = []
    /// The newest frame's timestamp
    private(set) var newest = 0.0
    /// The time of the newest frame or the end of the newest long spell, which the plots' marks reach
    private(set) var latest = 0.0
    /// The time at the plots' right edges: the newest frame's, or later while the card follows the time
    private(set) var head = 0.0
    /// The time at the right edges as the layers last showed it
    private var shown = 0.0
    /// Whether the card follows the time, and the timer that moves the plots between frames while it does
    private var following = false
    private(set) var clock: Timer?
    /// The screen's frames feed the charts, so their rate is the frames of the last half second
    private var counting = false
    /// Measuring the screen, the plots glide with the time: Core Animation moves the layers at the rate iOS chooses, so
    /// following the time takes no update of the app's, and the layers are drawn at the present again a second before
    /// the five seconds they reach run out
    var glides = false
    /// Whether the layers glide now
    private(set) var gliding = false
    /// Runs after each move between frames that changed the layers, a change of the card's own
    var moved: (() -> Void)?
    /// Whether the layers keep the picture they showed as the card paused
    private(set) var held = false
    /// This run's frame timestamps over the last half second, a ring of `count` from `first`, oldest
    /// first: the moving average's intervals lie between them
    private(set) var times: [Double] = []
    private var first = 0, count = 0
    /// Whether frames changed what the layers show since they were last drawn, and the newest column then
    private var pending = false, drawnThrough: Int64 = -1
    /// The time from which frames changed what the layers show since they were last drawn
    private var dirty = Double.infinity

    /// A plot's layer. Its tiles render their lines as images, which Core Animation commits in the update that asks for
    /// them; drawn into a backing store by `draw(in:)`, a layer takes UIKit another update a refresh later to show each
    /// drawing (LLP 1079.000 §7.4).
    private final class Lines: CALayer {
        weak var chart: FrameHealthChart?
        var which = 0
        /// Its pictures, left to right, each the same whole number of device pixels wide, fixed in its space
        private(set) var tiles: [CALayer] = []
        override func display() {
            // A layer stays asked to display until its contents are set, though it has none of its own
            contents = nil
            guard let chart else { return }
            let draw = { chart.paint(self.which) }
            if let drawing = chart.drawing { drawing(draw) } else { draw() }
        }

        /// Make `count` tiles `width` points wide, freeing the old ones.
        func tile(_ count: Int, width: CGFloat) {
            tiles.forEach { $0.removeFromSuperlayer() }
            tiles = (0..<count).map { index in
                let tile = CALayer()
                tile.actions = actions
                // A glide's fractional offsets take whole pixels, so the lines stay as sharp as when they step
                (tile.magnificationFilter, tile.minificationFilter) = (.nearest, .nearest)
                tile.isOpaque = true
                tile.contentsScale = contentsScale
                tile.frame = CGRect(x: CGFloat(index) * width, y: 0, width: width, height: bounds.height)
                addSublayer(tile)
                return tile
            }
        }
    }
    /// Each tile is about a second of axis
    private static let tileSeconds = 1.0
    private let windows = [CALayer(), CALayer()], lines = [Lines(), Lines()]
    /// Runs each plot's drawing, which Core Animation asks for as it commits, so the card can count it as its own work
    var drawing: ((() -> Void) -> Void)?
    /// What the layers show: each shown plot, the scale from its points to the host's, the palette, and for each plot
    /// the time its tiles show at its right edge while its layer sits at its origin
    private var plots: [Int: FrameHealthCardDrawing.Plot] = [:]
    private var scale: CGFloat = 1, palette: FrameHealthCardDrawing.Palette?, bases = [0.0, 0.0]
    /// For each plot, whether its next drawing paints every tile, and otherwise the tiles it paints; with neither, as
    /// when asked from outside the chart, it paints every tile
    private var whole = [true, true], repaint = [IndexSet(), IndexSet()]

    init(in host: CALayer) {
        // No implicit animations: a new picture, place or size shows at once, never faded or stretched from the old
        let still: [String: CAAction] = ["contents": NSNull(), "position": NSNull(), "bounds": NSNull(), "hidden": NSNull(),
            "sublayers": NSNull(), "onOrderIn": NSNull(), "onOrderOut": NSNull()]
        for (index, (window, layer)) in zip(windows, lines).enumerated() {
            window.actions = still
            window.masksToBounds = true
            window.isHidden = true
            layer.actions = still
            layer.anchorPoint = .zero
            layer.chart = self
            layer.which = index
            window.addSublayer(layer)
            host.addSublayer(window)
        }
    }

    // MARK: Frames

    /// Hold 30 seconds of columns and half a second of frames, as the card starts reading.
    func start() {
        guard columns.isEmpty else { return }
        columns = [Column](repeating: Column(), count: Int(Self.span / Self.columnSeconds))
        times = [Double](repeating: 0, count: 256)
    }

    /// Free the frames and the layers' pictures, as the card stops reading.
    func stop() {
        follow(false)
        for layer in lines { layer.removeAnimation(forKey: Self.glide) }
        gliding = false
        (columns, times, first, count, newest, latest, head, shown, pending, plots, held) = ([], [], 0, 0, 0, 0, 0, 0, false, [:], false)
        (counting, drawnThrough) = (false, -1)
        for (window, layer) in zip(windows, lines) {
            window.isHidden = true
            layer.tile(0, width: 0)
        }
        (whole, repaint) = ([true, true], [IndexSet(), IndexSet()])
    }

    /// A frame started at `start`; a frame whose work finished in its callback gives `finished`, and a frame of the
    /// screen's the `budget` long spells have. The frame hooks call it, so it only stores the frame.
    func frame(at start: Double, finished: Double? = nil, deadline: Double = 0, budget: Double = 0) {
        guard !columns.isEmpty, !held else { return }
        guard start > newest else { endRun(); return }
        let index = Self.index(start), at = slot(index), previous = latest
        // A column 30 seconds old gives its place to this one
        if columns[at].index != index { columns[at] = Column(index: index, joined: count > 0) }
        if count > 0 {
            let previous = times[(first + count - 1) % times.count]
            if count == times.count { (first, count) = ((first + 1) % times.count, count - 1) }
            times[(first + count) % times.count] = start
            count += 1
            // The intervals that ended within the last half second, and at least the newest
            while count > 2, times[(first + 1) % times.count] <= start - Self.average {
                (first, count) = ((first + 1) % times.count, count - 1)
            }
            columns[at].frame = max(columns[at].frame, Float((start - previous) * 1000))
            columns[at].fps = Float(Double(count - 1) / (start - times[first]))
        } else {
            (first, count) = (0, 1)
            times[0] = start
        }
        if budget > 0 { columns[at].budget = Float(budget * 1000) }
        newest = start
        latest = max(latest, start)
        head = max(head, start)
        if following { startClock() }
        // Redraw from the line into the previous frame's column: once a later frame begins, that column's lines
        // end at its edge, not at its frame
        changed(index, previous: previous)
        if let finished { work(start: start, deadline: deadline, finished: finished) }
    }

    /// A frame's work finished, after its callback or within it.
    func work(start: Double, deadline: Double, finished: Double) {
        guard !columns.isEmpty, !held else { return }
        let index = Self.index(start), at = slot(index)
        guard columns[at].index == index, Float((finished - start) * 1000) > columns[at].work else { return }
        columns[at].work = Float((finished - start) * 1000)
        columns[at].budget = Float((deadline - start) * 1000)
        changed(index, previous: latest)
    }

    /// A frame of the screen's, which UIKit expects on screen at `time`, with its work against its budget, in seconds
    /// (LLP 1079.000 §7.4). Its frame rate is the frames of the last half second, since frames of the screen bound no
    /// frame time; its line reaches back to a column whose frames lie within that half second.
    func screenFrame(at time: Double, work: Double, budget: Double) {
        guard !columns.isEmpty, !held, time > newest else { return }
        counting = true
        let index = Self.index(time), at = slot(index), previous = latest
        if count == times.count { (first, count) = ((first + 1) % times.count, count - 1) }
        times[(first + count) % times.count] = time
        count += 1
        while count > 0, times[first] <= time - Self.average { (first, count) = ((first + 1) % times.count, count - 1) }
        if columns[at].index != index { columns[at] = Column(index: index, joined: count > 1) }
        columns[at].fps = Float(Double(count) / Self.average)
        if Float(work * 1000) > columns[at].work { (columns[at].work, columns[at].budget) = (Float(work * 1000), Float(budget * 1000)) }
        newest = time
        latest = max(latest, time)
        head = max(head, time)
        if following { startClock() }
        changed(index, previous: previous)
    }

    /// A spell of the main thread's work longer than one refresh at the display's highest rate, `budget`, which delays
    /// a frame at that rate: the work of the column it ended in.
    func busy(start: Double, end: Double, budget: Double) {
        guard !columns.isEmpty, !held, end > start else { return }
        let index = Self.index(end), at = slot(index), length = Float((end - start) * 1000), previous = latest
        // A column with no frame yet joins none, so no frame's line reaches into it
        if columns[at].index != index { columns[at] = Column(index: index) }
        guard length > columns[at].work else { return }
        (columns[at].work, columns[at].budget) = (length, Float(budget * 1000))
        latest = max(latest, end)
        head = max(head, end)
        if following { startClock() }
        changed(index, previous: previous)
    }

    /// The run ended, as at an idle callback or a stop: the next frame bounds no interval.
    func endRun() { (first, count) = (0, 0) }

    /// Forget every frame, for a new generation.
    func reset() {
        endRun()
        for i in columns.indices { columns[i] = Column() }
        (newest, latest, head, pending, dirty) = (0, 0, 0, true, .infinity)
        redraw()
    }

    // MARK: Time

    /// Follow the time while the card reads unpaused: between frames, the plots move with it a column at a time, so a
    /// spell without frames widens as it passes. Once its newest mark has left the plots, they are flat and nothing
    /// moves until the next. The timer runs only while the card follows and marks lie within the plots.
    func follow(_ on: Bool) {
        following = on
        if on { startClock() } else { clock?.invalidate(); clock = nil }
    }

    private func startClock() {
        guard clock == nil, following, !columns.isEmpty, latest > 0 else { return }
        let timer = Timer(timeInterval: Self.columnSeconds, repeats: true) { [weak self] timer in
            guard let self else { timer.invalidate(); return }
            self.tick(at: CACurrentMediaTime())
        }
        timer.tolerance = Self.columnSeconds / 5
        RunLoop.main.add(timer, forMode: .common)
        clock = timer
    }

    /// Move the plots to `now`, unless a frame or spell moved them within the last column. Once the newest mark has left
    /// the plots, stop moving them.
    /// Returns whether it changed the layers.
    @discardableResult
    func tick(at now: Double) -> Bool {
        guard !glides else { return glide(at: now) }
        guard latest > 0 else { clock?.invalidate(); clock = nil; return false }
        guard now > head, now - latest >= Self.columnSeconds else { return false }
        head = now
        if counting { recount(at: now) }
        let changed = present(flush: true)
        if changed { moved?() }
        if now - latest >= Self.span { clock?.invalidate(); clock = nil }
        return changed
    }

    /// The screen's frame rate falls as its frames leave the last half second: the column at `now` takes the rate of
    /// those left, until none is, so the line falls as the figures do once motion ends.
    private func recount(at now: Double) {
        guard count > 0 else { return }
        while count > 0, times[first] <= now - Self.average { (first, count) = ((first + 1) % times.count, count - 1) }
        let index = Self.index(now), at = slot(index), previous = latest
        if columns[at].index != index { columns[at] = Column(index: index, joined: true) }
        columns[at].fps = Float(Double(count) / Self.average)
        latest = max(latest, now)
        changed(index, previous: previous)
    }

    /// Gliding, follow the time at `now`: the layers move on their own, so this draws again what frames changed, draws the
    /// layers at the present before their reach runs out, and stops them once the newest mark has left the plots, flat.
    /// Returns whether it changed the layers.
    private func glide(at now: Double) -> Bool {
        guard latest > 0, !plots.isEmpty, !held else { return false }
        guard now - latest < Self.span else { return freeze() }
        if counting, now - latest >= Self.columnSeconds { recount(at: now) }
        return present(flush: true)
    }

    /// Glide from the present to the end of the layers' reach, linearly in time.
    private func startGliding() {
        let now = CACurrentMediaTime()
        guard glides, !held, latest > 0 else { return }
        for (chart, plot) in plots {
            let base = bases[chart], end = base + Self.reach
            guard end > now else { continue }
            let layer = lines[chart], width = (plot.right - plot.left) * scale
            let x = { (t: Double) in -CGFloat((t - base) / Self.span) * width }
            let animation = CABasicAnimation(keyPath: "position.x")
            (animation.fromValue, animation.toValue, animation.duration) = (x(now), x(end), end - now)
            animation.timingFunction = CAMediaTimingFunction(name: .linear)
            layer.position.x = x(end)
            layer.add(animation, forKey: Self.glide)
            gliding = true
        }
    }

    /// Stop the layers where they show, as the newest mark leaves the plots or the card pauses. Returns whether they
    /// glided.
    @discardableResult
    private func freeze() -> Bool {
        guard gliding else { return false }
        gliding = false
        for layer in lines {
            layer.position.x = layer.presentation()?.position.x ?? layer.position.x
            layer.removeAnimation(forKey: Self.glide)
        }
        return true
    }

    private static let glide = "glide"

    /// Hold the picture, as the card pauses, or show the present again from `now`, as it resumes. Held, the charts take
    /// no frames or spells, and the next frame after begins a new run.
    func hold(_ on: Bool, at now: Double) {
        guard on != held else { return }
        if on {
            freeze()
            held = true
            endRun()
            follow(false)
        } else {
            held = false
            head = max(head, now)
            redraw()
        }
    }


    private static func index(_ time: Double) -> Int64 { Int64((time / columnSeconds).rounded(.down)) }

    private func slot(_ index: Int64) -> Int { Int(index % Int64(columns.count)) }

    /// The frames changed what the column at `index` shows, as the newest mark moved on from `previous`. A column's lines
    /// reach back to the end of the column before it with frames, and the newest column's end at the newest mark, so what
    /// changed starts at the earlier of those columns' runs: the column with frames before it.
    private func changed(_ index: Int64, previous: Double) {
        pending = true
        var from = previous > 0 ? min(index, Self.index(previous)) : index, earlier = from
        // At most 30 seconds back, past which no column is held
        for _ in 0..<columns.count {
            earlier -= 1
            guard earlier >= 0 else { break }
            if columns[slot(earlier)].index == earlier { from = earlier; break }
        }
        dirty = min(dirty, Double(from) * Self.columnSeconds)
    }

    // MARK: Layers

    /// Show what changed since the last call: move each layer left by the time the plots advanced, in
    /// whole device pixels, so its lines stay as sharp as they were drawn, and draw the tiles frames
    /// changed once a column has ended since they were drawn, or at `flush`, as the plots follow the
    /// time. Once a layer has moved the five seconds it reaches, move its pictures back by whole tiles.
    /// Called after the frame hooks, and as the plots follow the time. Returns whether it changed the layers.
    @discardableResult
    func present(flush: Bool = false) -> Bool {
        guard !held, !plots.isEmpty else { return false }
        if glides { return presentGliding(flush: flush) }
        let draw = pending && (flush || Self.index(latest) > drawnThrough)
        guard draw || head != shown else { return false }
        shown = head
        guard plots.keys.allSatisfy({ head - bases[$0] <= Self.reach }) else { advance(); return true }
        if draw { takeChanges() }
        var changed = draw
        for (chart, _) in plots {
            let layer = lines[chart], x = offset(chart)
            if layer.position.x != x { (layer.position.x, changed) = (x, true) }
            if draw { paintLater(chart) }
        }
        return changed
    }

    /// Gliding, the layers move on their own: draw the tiles frames changed once a column has ended since they were
    /// drawn, or at `flush`, and move the pictures back once a layer nears the end of its reach, or when they stand still.
    private func presentGliding(flush: Bool) -> Bool {
        let now = CACurrentMediaTime()
        head = max(head, now)
        if latest > 0, !gliding || plots.keys.contains(where: { now - bases[$0] >= Self.reach - 1 }) { advance(); return true }
        guard pending, flush || Self.index(latest) > drawnThrough else { return false }
        takeChanges()
        for (chart, _) in plots { paintLater(chart) }
        return true
    }

    /// Draw now the plots whose layers need it.
    func drawNow() { lines.forEach { $0.displayIfNeeded() } }

    /// Place the shown plots, in the card's points, which `scale` maps to the host's. A change of
    /// place, scale or palette draws the layers whole.
    func configure(_ shown: [(chart: Int, area: CGRect, plot: FrameHealthCardDrawing.Plot)], scale: CGFloat,
                   contentsScale: CGFloat, palette: FrameHealthCardDrawing.Palette, force: Bool = false) {
        let next = Dictionary(uniqueKeysWithValues: shown.map { ($0.chart, $0.plot) })
        let same = !force && scale == self.scale && next.count == plots.count && next.allSatisfy { chart, plot in
            plots[chart].map { $0.left == plot.left && $0.right == plot.right && $0.top == plot.top && $0.bottom == plot.bottom
                && $0.maximum == plot.maximum } ?? false
        }
        guard !same else { return }
        (plots, self.scale, self.palette) = (next, scale, palette)
        for chart in 0..<2 {
            guard let plot = plots[chart] else { windows[chart].isHidden = true; continue }
            let width = plot.right - plot.left, top = plot.top - 8
            windows[chart].isHidden = false
            windows[chart].frame = CGRect(x: plot.left * scale, y: top * scale, width: width * scale, height: (plot.bottom + 1 - top) * scale)
            lines[chart].frame = CGRect(x: 0, y: 0, width: (width + margin(plot)) * scale, height: windows[chart].bounds.height)
            lines[chart].contentsScale = contentsScale
            let tiling = tiling(chart)
            lines[chart].tile(tiling.count, width: CGFloat(tiling.width) / contentsScale)
        }
        redraw()
    }

    /// The axis five seconds take, which the layer reaches past its plot.
    private func margin(_ plot: FrameHealthCardDrawing.Plot) -> CGFloat { (plot.right - plot.left) * CGFloat(Self.reach / Self.span) }

    /// Device pixels a second takes along a plot.
    private func rate(_ chart: Int) -> CGFloat {
        guard let plot = plots[chart] else { return 0 }
        return (plot.right - plot.left) * scale * lines[chart].contentsScale / CGFloat(Self.span)
    }

    /// A plot's tiles' width in device pixels, about a second of axis, and how many cover its layer.
    private func tiling(_ chart: Int) -> (width: Int, count: Int) {
        let width = max(8, Int((rate(chart) * CGFloat(Self.tileSeconds)).rounded(.up)))
        let layer = lines[chart]
        return (width, Int((layer.bounds.width * layer.contentsScale / CGFloat(width)).rounded(.up)))
    }

    /// The layer's place, its right edge's time `head`, moved left in whole device pixels.
    private func offset(_ chart: Int) -> CGFloat {
        guard let plot = plots[chart] else { return 0 }
        let pixel = lines[chart].contentsScale
        return -(CGFloat((head - bases[chart]) / Self.span) * (plot.right - plot.left) * scale * pixel).rounded(.down) / pixel
    }

    /// Repaint, at the plots' next drawing, the tiles frames changed since they were last drawn: from the run the
    /// earliest change continues to the newest mark, and as far as a caret and a line's width reach either side.
    private func takeChanges() {
        defer { (pending, drawnThrough, dirty) = (false, Self.index(latest), .infinity) }
        guard dirty.isFinite else { return }
        for (chart, plot) in plots where !whole[chart] {
            let (width, count) = tiling(chart), pixel = lines[chart].contentsScale, rate = rate(chart)
            let reach = (FrameHealthCardDrawing.overhang + 2) * scale * pixel, right = (plot.right - plot.left) * scale * pixel
            let at = { (time: Double) in right - CGFloat(self.bases[chart] - time) * rate }
            let tile = { (x: CGFloat) in Int(min(CGFloat(count), max(-1, (x / CGFloat(width)).rounded(.down)))) }
            let first = max(0, tile(at(dirty) - reach)), last = min(count - 1, tile(at(latest) + reach))
            if first <= last { repaint[chart].insert(integersIn: first...last) }
        }
    }

    /// Draw a plot's layer at its next drawing, if any of its tiles needs it.
    private func paintLater(_ chart: Int) {
        if whole[chart] || !repaint[chart].isEmpty { lines[chart].setNeedsDisplay() }
    }

    /// Place the plots' right edges at `head`, as the layers reach the end of their reach or stand still: each layer's
    /// pictures move left by whole tiles, and only the tiles that come in, and those frames changed, are drawn.
    private func advance() {
        // Gliding, the layers start from the present, from which they glide on
        if glides, !held { head = max(head, CACurrentMediaTime()) }
        for (chart, _) in plots {
            let (width, count) = tiling(chart), tiles = lines[chart].tiles
            let moved = Int(min(CGFloat(count), (CGFloat(head - bases[chart]) * rate(chart) / CGFloat(width)).rounded(.down)))
            guard moved > 0 else { continue }
            guard !whole[chart], moved < count, tiles.count == count else {
                (bases[chart], whole[chart], repaint[chart]) = (head, true, [])
                continue
            }
            bases[chart] += Double(moved * width) / Double(rate(chart))
            for (index, tile) in tiles.enumerated() { tile.contents = index + moved < count ? tiles[index + moved].contents : nil }
            repaint[chart] = IndexSet(repaint[chart].compactMap { $0 >= moved ? $0 - moved : nil })
            repaint[chart].insert(integersIn: count - moved..<count)
        }
        shown = head
        takeChanges()
        for (chart, _) in plots {
            lines[chart].removeAnimation(forKey: Self.glide)
            lines[chart].position.x = offset(chart)
            paintLater(chart)
        }
        gliding = false
        startGliding()
    }

    /// Draw the layers whole, the plots' right edges at `head`.
    private func redraw() {
        // Gliding, the layers are drawn at the present, from which they glide on
        if glides, !held { head = max(head, CACurrentMediaTime()) }
        (shown, pending, drawnThrough, dirty) = (head, false, Self.index(latest), .infinity)
        for (chart, _) in plots {
            (bases[chart], whole[chart], repaint[chart]) = (head, true, [])
            lines[chart].removeAnimation(forKey: Self.glide)
            lines[chart].position.x = 0
            lines[chart].setNeedsDisplay()
        }
        gliding = false
        startGliding()
    }

    /// Draw the tiles of a plot that need it, or all of them, each a picture of its own device pixels.
    fileprivate func paint(_ chart: Int) {
        guard plots[chart] != nil else { return }
        let layer = lines[chart], pixel = layer.contentsScale, (width, count) = tiling(chart)
        var all = whole[chart] || repaint[chart].isEmpty
        if layer.tiles.count != count || layer.tiles.first.map({ $0.bounds.height != layer.bounds.height
            || ($0.bounds.width * pixel).rounded() != CGFloat(width) || $0.contentsScale != pixel }) == true {
            layer.tile(count, width: CGFloat(width) / pixel)
            all = true
        }
        let height = Int((layer.bounds.height * pixel).rounded(.up))
        // Each run of tiles is one picture, which its tiles show a part of each
        for run in (all ? IndexSet(integersIn: 0..<count) : repaint[chart]).rangeView where run.lowerBound < count {
            let run = run.lowerBound..<min(run.upperBound, count)
            guard let image = image(chart, from: run.lowerBound * width, width: run.count * width, height: height, scale: pixel) else { continue }
            for index in run {
                layer.tiles[index].contents = image.cropping(to: CGRect(x: (index - run.lowerBound) * width, y: 0, width: width, height: height))
            }
        }
        (whole[chart], repaint[chart]) = (false, [])
    }

    /// A plot's lines from device pixel `x` of its layer, `width` × `height` device pixels at `scale` to its points.
    private func image(_ chart: Int, from x: Int, width: Int, height: Int, scale pixel: CGFloat) -> CGImage? {
        guard width > 0, height > 0,
              let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: Self.space,
                  bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue) else { return nil }
        ctx.scaleBy(x: pixel, y: pixel)
        ctx.translateBy(x: -CGFloat(x) / pixel, y: 0)
        draw(chart, ctx)
        return ctx.makeImage()
    }

    private static let space = CGColorSpace(name: CGColorSpace.sRGB)!

    private func draw(_ chart: Int, _ ctx: CGContext) {
        guard var plot = plots[chart], let palette else { return }
        plot.now = bases[chart]
        // Paint the whole tile, as an opaque layer must, including its last pixel column past the plot's margin
        ctx.setFillColor(palette.background)
        ctx.fill(ctx.boundingBoxOfClipPath)
        // In the plot's points with y pointing down, whichever way the layer's context points
        if ctx.ctm.d > 0 { ctx.translateBy(x: 0, y: lines[chart].bounds.height); ctx.scaleBy(x: 1, y: -1) }
        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: -plot.left, y: -(plot.top - 8))
        FrameHealthCardDrawing.plotLines(ctx, chart: chart, plot: plot, columns: columns, latest: latest,
            palette: palette, margin: margin(plot))
    }
}
