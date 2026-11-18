import Foundation
import CoreText
// Independent Foundation oracle, never linked into the app.
@testable import ExactKit
@testable import ExactCore
// @ref LLP 1044.000 §6 S8 — decode the existing Apple JSON wire once, before
// presentation. Style names/values still come from kernel/tables/schema.json;
// this is a consumer of that wire, not another declaration table.

/// JSON values for style rows and the heterogeneous capability payloads. No
/// Objective-C containers or conditional bridges enter ordinary presentation.
enum OracleBatchValue: Decodable {
    case number(Double), string(String), bool(Bool), array([OracleBatchValue]), object([String: OracleBatchValue]), null

    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let n = try? value.decode(Double.self) { self = .number(n) }
        else if let s = try? value.decode(String.self) { self = .string(s) }
        else if let b = try? value.decode(Bool.self) { self = .bool(b) }
        else if let a = try? value.decode([OracleBatchValue].self) { self = .array(a) }
        else { self = .object(try value.decode([String: OracleBatchValue].self)) }
    }
    var number: Double? { if case .number(let n) = self { return n }; return nil }
    var string: String? { if case .string(let s) = self { return s }; return nil }
    var array: [OracleBatchValue]? { if case .array(let a) = self { return a }; return nil }
    var numbers: [Double]? {
        guard let a = array else { return nil }
        var result: [Double] = []; result.reserveCapacity(a.count)
        for value in a { guard let n = value.number else { return nil }; result.append(n) }
        return result
    }
    var isSchemeColor: Bool { array?.count == 2 && array?.first?.numbers?.count == 4 && array?.last?.numbers?.count == 4 }
    func channels(dark: Bool) -> [Double]? {
        if let c = numbers, c.count == 4 { return c }
        guard let a = array, a.count == 2, let c = a[dark ? 1 : 0].numbers, c.count == 4 else { return nil }
        return c
    }
    /// Only capability/region/collection adapters still take heterogeneous data.
    var any: Any {
        switch self {
        case .number(let n): return NSNumber(value: n)
        case .string(let s): return s
        case .bool(let b): return b
        case .array(let a): return a.map(\.any)
        case .object(let o): return o.mapValues(\.any)
        case .null: return NSNull()
        }
    }
}

typealias OracleNodeStyle = [String: OracleBatchValue]

extension CodingUserInfoKey {
    static let oracleAssets = CodingUserInfoKey(rawValue: "oracleAssets")!
}

struct OracleBatchOp: Decodable {
    enum Kind: String {
        case create, props, style, children, paragraph, frame, content, present, roots, destroy
        case layoutHits, layoutHitsWithdrawn, layoutPromote
        case fieldContent, flow, surface, command, hold, collections, region, router, title, language, unknown
        case svg, animations, canvas2d, reorder, exit, rank, roles
        case sound // LLP 1096 D8, as Batch's: a real session's boot sends its voice table's files
        case heightDrag = "height-drag", transformDrag = "transform-drag", retireMotion = "retire-motion"
    }
    let op: Kind
    let nodeID: UInt32?
    var id: UInt32 { nodeID ?? 0 }
    var kind = "view"
    var props: [String: String] = [:]
    var clear: [String] = []
    var style: OracleNodeStyle = [:]
    var handlers: Set<String> = []
    var ids: [UInt32] = []
    var runs: [OracleInlineText] = []
    var x = 0.0, y = 0.0, w = 0.0, h = 0.0
    var fitHeight: Double?
    var property = ""
    // Rare adapters retain their existing input shape. Common ops never build it.
    var payload: [String: Any] = [:]

    enum CodingKeys: String, CodingKey {
        case op, id, kind, props, set, clear, style, handlers, ids, runs, x, y, w, h, fitHeight, property, roles
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        op = Kind(rawValue: try c.decode(String.self, forKey: .op)) ?? .unknown
        nodeID = try c.decodeIfPresent(UInt32.self, forKey: .id)
        switch op {
        case .create:
            kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "view"
            props = try c.decodeIfPresent([String: String].self, forKey: .props) ?? [:]
            style = try c.decodeIfPresent(OracleNodeStyle.self, forKey: .style) ?? [:]
            handlers = try c.decodeIfPresent(Set<String>.self, forKey: .handlers) ?? []
        case .props:
            props = try c.decodeIfPresent([String: String].self, forKey: .set) ?? [:]
            clear = try c.decodeIfPresent([String].self, forKey: .clear) ?? []
        case .style: style = try c.decodeIfPresent(OracleNodeStyle.self, forKey: .style) ?? [:]
        case .paragraph: runs = try c.decodeIfPresent([OracleInlineText].self, forKey: .runs) ?? []
        case .children, .roots: ids = try c.decodeIfPresent([UInt32].self, forKey: .ids) ?? []
        case .frame, .content, .present:
            x = try c.decodeIfPresent(Double.self, forKey: .x) ?? 0
            y = try c.decodeIfPresent(Double.self, forKey: .y) ?? 0
            w = try c.decodeIfPresent(Double.self, forKey: .w) ?? 0
            h = try c.decodeIfPresent(Double.self, forKey: .h) ?? 0
            if op == .content { fitHeight = try c.decodeIfPresent(Double.self, forKey: .fitHeight) }
            property = try c.decodeIfPresent(String.self, forKey: .property) ?? ""
        case .destroy: break
        case .roles: style = try c.decodeIfPresent(OracleNodeStyle.self, forKey: .roles) ?? [:]
        default: payload = try [String: OracleBatchValue](from: decoder).mapValues(\.any)
        }
    }
}
struct OracleBatch: Decodable {
    let ops: [OracleBatchOp]
    let timers: Bool
    let motion: Bool
    /// The runner's clock after the call, milliseconds (LLP 1012 `clock`).
    let clock: Double?
    let error: String?
    /// @ref LLP 1043.000 §3 D8 — absolute runner deadline, absent without timers.
    var timerDueMs: Double? = nil
    var pending = false
    enum CodingKeys: String, CodingKey {
        case ops, timers, motion, clock, error, pending
        case timerDueMs = "timer_due_ms"
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        ops = try c.decodeIfPresent([OracleBatchOp].self, forKey: .ops) ?? []
        timers = try c.decodeIfPresent(Bool.self, forKey: .timers) ?? false
        motion = try c.decodeIfPresent(Bool.self, forKey: .motion) ?? false
        clock = try c.decodeIfPresent(Double.self, forKey: .clock)
        error = try c.decodeIfPresent(String.self, forKey: .error)
        timerDueMs = try c.decodeIfPresent(Double.self, forKey: .timerDueMs)
        pending = try c.decodeIfPresent(Bool.self, forKey: .pending) ?? false
    }
    init(ops: [OracleBatchOp], timers: Bool, motion: Bool, clock: Double?, error: String?, timerDueMs: Double? = nil, pending: Bool = false) {
        self.ops = ops; self.timers = timers; self.motion = motion; self.clock = clock
        self.error = error; self.timerDueMs = timerDueMs; self.pending = pending
    }
    static func decode(_ data: Data) -> OracleBatch {
        (try? JSONDecoder().decode(OracleBatch.self, from: data))
            ?? OracleBatch(ops: [], timers: false, motion: false, clock: nil, error: "unreadable batch")
    }

}

struct OracleBatchMetadata: Decodable {
    let canvas, waitContinue, frames, canvasOwed, controls, layoutProvisional: Bool?
    let unchanged, storeReadPending, storeActivationPending, busy, requests, spatial: Bool?
    let landing, stored, unsupported, facts, wakelock, mustFinish, presentOnly: Bool?
    let waits: [Double]?
    let canvasImages, alerts, journal, announced: [String]?
    let live: String?
    let design, modes: OracleBatchValue?
    let seq: [Double]?
    let nativeTimers: [Timer]?

    struct Timer: Decodable {
        let index, interval, tolerance: UInt32
        let generation: String
        let due: Double
        let once: Bool
    }
}

// @ref LLP 1044.000 §6 S1 — textual descendants are paragraph values.

struct OracleInlineText: Decodable {
    let id: UInt32
    let parent: UInt32
    let props: [String: String]
    private let lightRun: Run
    private let color: OracleInlineColor?
    let hasSchemeColor: Bool
    let handlers: Set<String>
    let paints: Bool
    var range: NSRange = NSRange(location: 0, length: 0)

    enum CodingKeys: String, CodingKey { case id, parent, props, style, handlers, paint }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UInt32.self, forKey: .id)
        parent = try c.decode(UInt32.self, forKey: .parent)
        props = try c.decodeIfPresent([String: String].self, forKey: .props) ?? [:]
        let style = try c.decodeIfPresent(OracleInlineStyle.self, forKey: .style) ?? OracleInlineStyle()
        var value = style.run
        value.text = props["text"] ?? ""
        value.href = props["href"] ?? ""
        lightRun = value
        color = style.color
        hasSchemeColor = style.color?.paired ?? false
        handlers = try c.decodeIfPresent(Set<String>.self, forKey: .handlers) ?? []
        paints = try c.decodeIfPresent(Bool.self, forKey: .paint) ?? false
    }

    var text: String { lightRun.text }
    func run(dark: Bool) -> Run {
        var value = lightRun
        value.color = color?.channels(dark: dark)
        return value
    }

    static func run(_ text: String, style: OracleNodeStyle, href: String = "", dark: Bool) -> Run {
        func number(_ key: String, _ fallback: Double = 0) -> Double { style[key]?.number ?? fallback }
        let size = Float(number("font_size", 16))
        let height: CGFloat?
        if let ratio = style["line_height"]?.number { height = CGFloat(Float(ratio) * size) }
        else if let px = style["line_height"]?.string, px.hasSuffix("px"), let value = Float(px.dropLast(2)) { height = CGFloat(value) }
        else { height = nil }
        var run = Run(text: text, size: CGFloat(size), weight: Int(number("font_weight", 400)),
                      family: Int(number("font_family")), italic: style["font_style"]?.string == "italic",
                      lineHeight: height, letterSpacing: CGFloat(Float(number("letter_spacing"))),
                      color: style["text_color"]?.channels(dark: dark),
                      decoration: style["text_decoration_line"]?.string ?? "", href: href)
        // A colour that names no system colour is one its author fixed (the app's own asset is).
        if let color = style["text_color"] {
            if case .object(let o) = color, let name = o["sys"]?.string { run.fixedInk = name.hasPrefix("named:") } else { run.fixedInk = true }
        }
        return run
    }
}

// Paragraphs read only text rows, directly into their resolved representation.
// Decoding every style entry through a generic JSON value spends the saved apply
// time in speculative type probes and temporary dictionaries. No wire change.
private struct OracleInlineStyle: Decodable {
    var run = Run(text: "", size: 16, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0)
    var color: OracleInlineColor?
    init() {}
    enum CodingKeys: String, CodingKey {
        case font_size, font_weight, font_family, font_style, line_height, letter_spacing, text_color, text_decoration_line
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let size = Float(try c.decodeIfPresent(Double.self, forKey: .font_size) ?? 16)
        run.size = CGFloat(size)
        run.weight = Int(try c.decodeIfPresent(Double.self, forKey: .font_weight) ?? 400)
        run.family = Int(try c.decodeIfPresent(Double.self, forKey: .font_family) ?? 0)
        run.italic = try c.decodeIfPresent(String.self, forKey: .font_style) == "italic"
        if let height = try c.decodeIfPresent(OracleInlineLineHeight.self, forKey: .line_height) {
            run.lineHeight = height.pixels.map(CGFloat.init) ?? height.ratio.map { CGFloat($0 * size) }
        }
        run.letterSpacing = CGFloat(Float(try c.decodeIfPresent(Double.self, forKey: .letter_spacing) ?? 0))
        run.decoration = try c.decodeIfPresent(String.self, forKey: .text_decoration_line) ?? ""
        color = try c.decodeIfPresent(OracleInlineColor.self, forKey: .text_color)
        run.color = color?.channels(dark: false)
        run.fixedInk = color?.fixedInk ?? false
    }
}

private struct OracleInlineLineHeight: Decodable {
    var pixels: Float?
    var ratio: Float?
    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let n = try? c.decode(Double.self) { ratio = Float(n) }
        else {
            let text = try c.decode(String.self)
            if text.hasSuffix("px") { pixels = Float(text.dropLast(2)) }
        }
    }
}

private struct OracleInlineColor: Decodable {
    let light: [Double]
    let dark: [Double]
    let paired: Bool
    let system: String?
    var fixedInk: Bool { system.map { $0.hasPrefix("named:") } ?? true }
    enum CodingKeys: String, CodingKey { case sys, c, cs }
    init(from decoder: Decoder) throws {
        if let c = try? decoder.container(keyedBy: CodingKeys.self) {
            system = try c.decodeIfPresent(String.self, forKey: .sys)
            let fallback = try c.decodeIfPresent(OracleInlineColor.self, forKey: .c)
            if let system {
                guard let l = SystemColor.channels(system, dark: false, fallback: fallback?.light),
                      let d = SystemColor.channels(system, dark: true, fallback: fallback?.dark) else {
                    throw DecodingError.dataCorruptedError(forKey: .c, in: c, debugDescription: "unresolvable system colour")
                }
                light = fallback?.light ?? l; dark = fallback?.dark ?? d
                paired = true
            } else {
                let spaces = try c.decode([OracleColorSpace].self, forKey: .cs)
                guard (1...2).contains(spaces.count) else {
                    throw DecodingError.dataCorruptedError(forKey: .cs, in: c, debugDescription: "expected one or two colour spaces")
                }
                light = try spaces[0].channels(fallback: fallback?.light)
                dark = try spaces[spaces.count - 1].channels(fallback: fallback?.dark)
                paired = spaces.count == 2
            }
            return
        }
        system = nil
        var c = try decoder.unkeyedContainer()
        paired = c.count == 2
        if paired {
            light = try c.decode([Double].self)
            dark = try c.decode([Double].self)
        } else {
            light = try (0..<4).map { _ in try c.decode(Double.self) }
            dark = light
        }
        guard c.isAtEnd, light.count == 4, dark.count == 4 else {
            throw DecodingError.dataCorruptedError(in: c, debugDescription: "expected RGBA or light/dark RGBA")
        }
    }
    func channels(dark: Bool) -> [Double] {
        let fallback = dark ? self.dark : light
        // Share presentation's platform lookup, but no BatchValue or batch decoder.
        return system.flatMap { SystemColor.channels($0, dark: dark, fallback: fallback) } ?? fallback
    }
}

private struct OracleColorSpace: Decodable {
    let s: String
    let v: [Double]
    let profile: CGColorSpace?
    let intent: CGColorRenderingIntent
    enum CodingKeys: String, CodingKey { case s, v, i }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        s = try c.decode(String.self, forKey: .s)
        v = try c.decode([Double].self, forKey: .v)
        switch try c.decodeIfPresent(String.self, forKey: .i) {
        case "perceptual": intent = .perceptual
        case "saturation": intent = .saturation
        case "absolute-colorimetric": intent = .absoluteColorimetric
        default: intent = .relativeColorimetric
        }
        if s.hasPrefix("cg:") {
            profile = CGColorSpace(name: String(s.dropFirst(3)) as CFString)
        } else if s.hasPrefix("icc:"), let assets = decoder.userInfo[.oracleAssets] as? AssetResolver,
                  let data = assets.bytes(String(s.dropFirst(4))) {
            profile = CGColorSpace(iccData: data as CFData)
        } else { profile = nil }
    }
    func channels(fallback: [Double]?) throws -> [Double] {
        if let code = ["srgb": 0.0, "display-p3": 1, "srgb-linear": 2][s], v.count == 4, let fallback {
            return fallback + [code] + v
        }
        guard let space = profile,
              v.count == space.numberOfComponents + 1,
              let source = CGColor(colorSpace: space, components: v.map { CGFloat($0) }),
              let srgb = CGColorSpace(name: CGColorSpace.sRGB),
              let extended = CGColorSpace(name: CGColorSpace.extendedSRGB) else {
            throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription: "unresolvable colour space"))
        }
        // A CGColor carries no intent; an ICC colour applies it before other conversions (LLP 1100 D3).
        let color = s.hasPrefix("icc:") ? source.converted(to: extended, intent: intent, options: nil) : source
        guard let color,
              let clipped = color.converted(to: srgb, intent: .relativeColorimetric, options: nil)?.components,
              let components = color.converted(to: extended, intent: .relativeColorimetric, options: nil)?.components,
              clipped.count == 4, components.count == 4 else {
            throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription: "unresolvable colour space"))
        }
        let rgba = clipped.map { (Double($0) * 255).rounded() }
        let extendedRGBA = components.map { Double($0) }
        return rgba + [0] + extendedRGBA
    }
}

/// A batch as the library sends it: JSON ops through the same typed decoder,
/// for tests that state a batch as dictionaries.
func wireBatch(_ ops: [[String: Any]]) -> Batch {
    let data = try! JSONSerialization.data(withJSONObject: ["ops": ops])
    let batch = Batch.decode(data)
    precondition(batch.error == nil, "a test batch did not decode: \(ops)")
    return batch
}

/// A batch with no ops that says a control's viewless contents changed, as
/// the runner sends when a native button's text or symbol changes: the
/// presenter reads its buttons' faces again (`ControlsHost.face`).
func facesBatch() -> Batch {
    let batch = Batch.decode(Data(#"{"ops":[],"controls":true}"#.utf8))
    precondition(batch.error == nil && batch.controls)
    return batch
}

/// The `fieldContent` op layout sends a `width` by `height` box with
/// `padding` and `border` (top, right, bottom, left): its content box and the
/// padding it resolved, which the presenter places content and insets by.
/// `button`: a native button's, whose author wrote only its nonzero sides.
func contentOp(_ id: Int, _ width: Double, _ height: Double, padding: [Double], border: [Double] = [0, 0, 0, 0], button: Bool = false) -> [String: Any] {
    let inset = zip(padding, border).map(+)
    let sent: [Any] = button ? padding.map { $0 == 0 ? NSNull() : $0 as Any } : padding
    return ["op": "fieldContent", "id": id, "rect": [inset[3], inset[0], width - inset[1] - inset[3], height - inset[0] - inset[2]], "padding": sent]
}

extension NodeView {
    /// Applies what layout sends a box of these bounds with `padding` and
    /// `border` (`contentOp`), for a view a test makes without a presenter
    func applyContentBox(padding: [Double], border: [Double] = [0, 0, 0, 0]) {
        applyFieldContent(contentOp(Int(id), bounds.width, bounds.height, padding: padding, border: border))
    }
}

// Preserve the pre-record Foundation conversions independently of the schema.
extension OracleBatchOp {
    var contentRect: CGRect? {
        guard let r = payload["rect"] as? [Double], r.count == 4 else { return nil }
        return CGRect(x: r[0], y: r[1], width: r[2], height: r[3])
    }
    var contentPadding: [CGFloat?]? {
        guard let sides = payload["padding"] as? [Any], sides.count == 4 else { return nil }
        return sides.map { ($0 as? NSNumber).map { CGFloat($0.doubleValue) } }
    }
    var layoutBoxes: [LayoutBoxRecord]? {
        guard let boxes = payload["boxes"] as? [[Any]], !boxes.isEmpty else { return nil }
        return boxes.compactMap { box in
            guard box.count >= 3, let f = box[1] as? [NSNumber], f.count == 4, let id = box[2] as? NSNumber else { return nil }
            return LayoutBoxRecord(before: (box[0] as? NSNumber)?.uint32Value,
                frame: CGRect(x: f[0].doubleValue, y: f[1].doubleValue, width: f[2].doubleValue, height: f[3].doubleValue),
                id: id.uint32Value, end: box.count > 3 ? (box[3] as? NSNumber)?.uint32Value : nil)
        }
    }
    var layoutFrame: CGRect? {
        guard let f = payload["frame"] as? [NSNumber], f.count == 4 else { return nil }
        return CGRect(x: f[0].doubleValue, y: f[1].doubleValue, width: f[2].doubleValue, height: f[3].doubleValue)
    }
    var layoutChildren: [LayoutChildRecord]? {
        guard let children = payload["children"] as? [[Any]] else { return nil }
        return children.compactMap { pair in
            guard pair.count == 2, let id = pair[0] as? NSNumber,
                  let f = pair[1] as? [NSNumber], f.count == 4 else { return nil }
            return LayoutChildRecord(id: id.uint32Value,
                frame: CGRect(x: f[0].doubleValue, y: f[1].doubleValue, width: f[2].doubleValue, height: f[3].doubleValue))
        }
    }
}

extension NodeView {
    func applyFieldContent(_ payload: [String: Any]) {
        var object = payload
        object["op"] = "fieldContent"
        let data = try! JSONSerialization.data(withJSONObject: ["ops": [object]])
        applyFieldContent(Batch.decode(data).ops[0])
    }
}

struct OracleCollectionSnapshot {
    struct Row {
        let view: UInt32
        let root: UInt32
        let epoch: UInt64
        /// Logical position; -1 when a snapshot omits it.
        let index: Int
        let size: Double?
        let measured: Bool
    }
    struct Fixed {
        let view: UInt32
        let roots: [UInt32]
        let epoch: UInt64
        let size: Double
    }
    struct Correction {
        let sequence: UInt64
        let offset: Double
        /// An anchor's correction: the offset it was taken at (relative).
        var from: Double? = nil
        /// Animate there (LLP 1070.000 §6.2): a smooth `scrollIntoView`, or
        /// a `scroll-behavior: smooth` list following its end.
        var smooth = false
    }
    let view: UInt32
    /// The main axis (LLP 1070 H1): a row list scrolls on x, and `extent`,
    /// a correction's offset and every measured size run along it.
    let horizontal: Bool
    /// The outer list whose mounted row holds this one (LLP 1070 N1).
    let parent: UInt32?
    let revision: UInt64
    let sequence: UInt64
    let extent: Double
    let rows: [Row]
    let fixed: [Fixed]
    let correction: Correction?
    /// Logical item count, mounted or not.
    let count: Int
    /// A limited report left window rows unbuilt: another report is owed.
    let pending: Bool
    /// A `scrollIntoView` under way (LLP 1070.000): its corrections are the
    /// app's, applied whatever the reader scrolled since, until the reader's
    /// gesture. `waiting`: it waits for the reader's scroll to end (§2.6).
    let seeking: Bool
    let waiting: Bool
    /// Which request that is: a gesture counts against the one it began after.
    let request: UInt64
    /// The latest request the list took, under way or ended (0 for none). A
    /// host compares it before and after an input to tell whether a handler
    /// asked for a row in answer.
    let lastRequest: UInt64
    var underWay: Bool { seeking || waiting }

    init?(_ value: [String: Any]) {
        guard let view = Self.viewID(value["view"]), let revision = Self.uint(value["revision"]),
              let sequence = Self.uint(value["scrollSequence"]), let extent = Self.number(value["totalExtent"]),
              let rawRows = value["rows"] as? [[String: Any]] else { return nil }
        var seen = Set<UInt32>()
        var rows: [Row] = []
        for item in rawRows {
            guard let id = Self.viewID(item["view"]), let root = Self.viewID(item["root"]),
                  let epoch = Self.uint(item["epoch"]), seen.insert(id).inserted else { return nil }
            rows.append(Row(view: id, root: root, epoch: epoch, index: Self.uint(item["index"]).map { Int(clamping: $0) } ?? -1,
                            size: Self.number(item["size"]), measured: item["measured"] as? Bool ?? false))
        }
        var fixed: [Fixed] = []
        if let value = value["fixed"] {
            guard let items = value as? [[String: Any]], items.count == 2 else { return nil }
            for item in items {
                guard let id = Self.viewID(item["view"]), let epoch = Self.uint(item["epoch"]), epoch == 1,
                      let size = Self.number(item["size"]), let roots = item["roots"] as? [Any],
                      seen.insert(id).inserted else { return nil }
                let ids = roots.compactMap(Self.viewID)
                guard ids.count == roots.count else { return nil }
                fixed.append(Fixed(view: id, roots: ids, epoch: epoch, size: size))
            }
        }
        guard fixed.isEmpty || (value["axis"] as? String) != "x" else { return nil }
        var correction: Correction?
        if let raw = value["correction"], !(raw is NSNull) {
            guard let raw = raw as? [String: Any], let seq = Self.uint(raw["scrollSequence"]),
                  let offset = Self.signed(raw["offset"]) else { return nil }
            correction = Correction(sequence: seq, offset: offset, from: Self.signed(raw["from"]), smooth: raw["smooth"] as? Bool == true)
        }
        self.view = view; self.revision = revision; self.sequence = sequence
        self.horizontal = (value["axis"] as? String) == "x"
        self.parent = Self.viewID(value["parent"])
        self.extent = extent; self.rows = rows; self.fixed = fixed; self.correction = correction
        self.count = Self.uint(value["count"]).map { Int(clamping: $0) } ?? rows.count
        self.pending = (value["pending"] as? NSNumber)?.boolValue ?? false
        self.seeking = value["seeking"] as? Bool == true
        self.waiting = value["waiting"] as? Bool == true
        self.request = Self.uint(value["request"]) ?? 0
        self.lastRequest = Self.uint(value["lastRequest"]) ?? 0
    }
    private static func uint(_ value: Any?) -> UInt64? {
        // Integral JSON numbers read directly; a fraction or sign refuses.
        if let n = value as? NSNumber, CFNumberIsFloatType(n) == false {
            return n.int64Value >= 0 ? n.uint64Value : nil
        }
        if let n = value as? NSNumber {
            // An integral double below 2^53 is its own digits; formatting
            // one to parse it back cost a tenth of a built row.
            let d = n.doubleValue
            if d.sign == .plus, d < 9_007_199_254_740_992, d.rounded(.towardZero) == d { return UInt64(d) }
            return UInt64(n.stringValue)
        }
        if let s = value as? String { return UInt64(s) }
        return nil
    }
    private static func viewID(_ value: Any?) -> UInt32? {
        guard let n = uint(value), n > 0 else { return nil }
        return UInt32(exactly: n)
    }
    /// A correction's offset and an anchor's `from`: negative only in the
    /// padding before the first row (LLP 1010 §6.9; a `scrollIntoView` there,
    /// or a report from there that a kept row's correction moves on from).
    private static func signed(_ value: Any?) -> Double? {
        guard let n = value as? NSNumber, n.doubleValue.isFinite else { return nil }
        return abs(n.doubleValue) <= Double(Float.greatestFiniteMagnitude) ? n.doubleValue : nil
    }
    private static func number(_ value: Any?) -> Double? {
        guard let n = value as? NSNumber else { return nil }
        let d = n.doubleValue
        return d.isFinite && d >= 0 && d <= Double(Float.greatestFiniteMagnitude) ? d : nil
    }
}
