// CSS Masking 1 §5: basic-shape percentages use the box at presentation size.
// The kernel validates the grammar and sends length pairs [px, percent].
import CoreGraphics

public enum ClipGeometry {
    public static func path(_ value: BatchValue?, size: CGSize = .zero) -> CGPath? {
        guard case .object(let o)? = value else { return nil }
        if let shape = o["shape"]?.string, let values = o["values"]?.numbers {
            return basic(shape, values.map { CGFloat($0) }, size)
        }
        guard let commands = o["commands"]?.array, !commands.isEmpty else { return nil }
        let path = CGMutablePath()
        for command in commands {
            guard let kind = command.array?.first?.string, let values = command.array?.last?.numbers else { continue }
            func point(_ index: Int) -> CGPoint { CGPoint(x: values[index], y: values[index + 1]) }
            switch kind {
            case "M" where values.count == 2: path.move(to: point(0))
            case "L" where values.count == 2: path.addLine(to: point(0))
            case "Q" where values.count == 4: path.addQuadCurve(to: point(2), control: point(0))
            case "C" where values.count == 6: path.addCurve(to: point(4), control1: point(0), control2: point(2))
            case "Z": path.closeSubpath()
            default: break
            }
        }
        return path
    }

    private static func basic(_ shape: String, _ v: [CGFloat], _ size: CGSize) -> CGPath? {
        let p = CGMutablePath(), w = size.width, h = size.height
        func length(_ i: Int, _ basis: CGFloat) -> CGFloat { v[i * 2] + v[i * 2 + 1] * basis / 100 }
        switch shape {
        case "polygon" where v.count >= 4 && v.count % 4 == 0:
            for i in stride(from: 0, to: v.count / 2, by: 2) {
                let at = CGPoint(x: length(i, w), y: length(i + 1, h))
                if i == 0 { p.move(to: at) } else { p.addLine(to: at) }
            }
            p.closeSubpath()
        case "inset" where v.count == 24:
            var e = (0..<4).map { length($0, $0 % 2 == 0 ? h : w) }
            for (a, b, extent) in [(0, 2, h), (1, 3, w)] where e[a] + e[b] > extent {
                let f = extent / (e[a] + e[b]); e[a] *= f; e[b] *= f
            }
            let iw = max(0, w - e[1] - e[3]), ih = max(0, h - e[0] - e[2])
            guard iw > 0 && ih > 0 else { return p }
            var rx = (4..<8).map { length($0, w) }, ry = (8..<12).map { length($0, h) }
            let sums = [rx[0] + rx[1], rx[3] + rx[2], ry[0] + ry[3], ry[1] + ry[2]]
            var f: CGFloat = 1
            for (sum, extent) in zip(sums, [iw, iw, ih, ih]) where sum > 0 { f = min(f, extent / sum) }
            rx = rx.map { $0 * f }; ry = ry.map { $0 * f }
            let x = e[3], y = e[0], k: CGFloat = 0.5522848
            p.move(to: CGPoint(x: x + rx[0], y: y))
            let corners: [(CGFloat, CGFloat, CGFloat, CGFloat, CGFloat, CGFloat)] = [
                (x + iw, y, -1, 0, 0, 1), (x + iw, y + ih, 0, -1, -1, 0),
                (x, y + ih, 1, 0, 0, -1), (x, y, 0, 1, 1, 0)
            ]
            for (j, c) in corners.enumerated() {
                let i = (j + 1) % 4
                let start = CGPoint(x: c.0 + c.2 * rx[i], y: c.1 + c.3 * ry[i])
                let end = CGPoint(x: c.0 + c.4 * rx[i], y: c.1 + c.5 * ry[i])
                p.addLine(to: start)
                p.addCurve(to: end,
                    control1: CGPoint(x: start.x - c.2 * rx[i] * k, y: start.y - c.3 * ry[i] * k),
                    control2: CGPoint(x: end.x - c.4 * rx[i] * k, y: end.y - c.5 * ry[i] * k))
            }
            p.closeSubpath()
        case "circle", "ellipse":
            guard v.count == 8 else { return nil }
            let cx = length(0, w), cy = length(1, h)
            func radius(_ i: Int, _ center: CGFloat, _ extent: CGFloat) -> CGFloat {
                switch v[i * 2] {
                case -1: return min(abs(center), abs(extent - center))
                case -2: return max(abs(center), abs(extent - center))
                default: return length(i, extent)
                }
            }
            var rx = radius(2, cx, w), ry = radius(3, cy, h)
            if shape == "circle" {
                rx = v[4] == -1 ? min(rx, ry) : v[4] == -2 ? max(rx, ry) : length(2, hypot(w, h) / sqrt(2))
                ry = rx
            }
            if rx > 0 && ry > 0 { p.addEllipse(in: CGRect(x: cx - rx, y: cy - ry, width: rx * 2, height: ry * 2)) }
        default: return nil
        }
        return p
    }
}
