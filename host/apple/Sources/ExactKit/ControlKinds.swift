// @ref LLP 1069.001 D2, D5 — which platform control a `Control` node is, and
// a select's menu as the runtime reads it from the kernel: both hosts'
// presenters build the platform's own control from these.
import Foundation
import MachO

enum ControlKinds {
    /// The chrome index's keys for the controls the presenter projects.
    static let indexed = ["type:checkbox", "type:radio", "type:select", "type:range", "type:date", "type:time", "type:datetime-local", "type:button"]
    static let dates: Set<String> = ["date", "time", "datetime-local"]
    /// The chrome index's key for a `progress`, the platform's activity
    /// indicator (LLP 1069.001, amended 2026-10-07) or, with a `value`, its
    /// progress bar (LLP 1116 D8): a view, not a control, so its own pass,
    /// outside `indexed`.
    static let progress = "type:progress"
    /// `switch`, `checkbox`, or the `type` prop's value (`radio`, a select, …).
    static func kind(_ props: [String: String]) -> String {
        switch props["type"] {
        case "button": return "button" // LLP 1069.011 D3: before the checkbox default
        case "select": return "select"
        case "radio": return "radio" // x2apps survey #2
        case "range": return "range"
        case let t? where dates.contains(t): return t
        default: return props["accessibilityRole"] == "switch" ? "switch" : "checkbox"
        }
    }
}

/// A range's `min`, `max` and `step` by HTML's rules, and its value
/// clamped and snapped as HTML sanitizes it (`exact_kernel::Range`).
struct RangeSpec {
    var min = 0.0, max = 100.0
    var step: Double? = 1

    init(_ props: [String: String]) {
        let number = { (s: String?) in s.flatMap { Double($0.trimmingCharacters(in: .whitespaces)) }.flatMap { $0.isFinite ? $0 : nil } }
        min = number(props["min"]) ?? 0
        max = Swift.max(number(props["max"]) ?? 100, min)
        step = props["step"]?.trimmingCharacters(in: .whitespaces).lowercased() == "any" ? nil : (number(props["step"]).flatMap { $0 > 0 ? $0 : nil } ?? 1)
    }

    func sanitize(_ value: Double) -> Double {
        let clamped = Swift.min(Swift.max(value, min), max)
        guard let step else { return clamped }
        var snapped = min + ((clamped - min) / step + 0.5).rounded(.down) * step
        if snapped > max { snapped -= step }
        return Swift.min(Swift.max((snapped * 1e9).rounded() / 1e9, min), max)
    }

    /// The value it shows: its `value`, or the midpoint as HTML's default.
    func shown(_ props: [String: String]) -> Double {
        sanitize(props["value"].flatMap { Double($0) } ?? (min + (max - min) / 2))
    }

    /// A number as HTML writes one: no trailing `.0`.
    static func format(_ v: Double) -> String {
        v == v.rounded() && abs(v) < 1e15 ? String(Int64(v)) : String(v)
    }
}

/// A determinate progress's `value` and `max` by HTML's rules
/// (`exact_kernel::Progress`, LLP 1116 D8): `max` is 1 unless it reads as a
/// number above 0; `value` is 0 unless it reads as one, held to 0 and `max`.
struct ProgressSpec {
    var value = 0.0, max = 1.0

    init(_ props: [String: String]) {
        let number = { (s: String?) in s.flatMap { Double($0.trimmingCharacters(in: .whitespaces)) }.flatMap { $0.isFinite ? $0 : nil } }
        max = number(props["max"]).flatMap { $0 > 0 ? $0 : nil } ?? 1
        value = Swift.min(Swift.max(number(props["value"]) ?? 0, 0), max)
    }

    /// How much of the bar is filled, 0 to 1.
    var fraction: Double { value / max }
}

/// A date control's value in HTML's format, read and written as the
/// platform's `Date` at UTC: HTML's values carry no zone, so the picker
/// shows the wall time the string names and never converts it.
enum DateValue {
    /// What an empty date control says (LLP 1115.000 D13.1): its author's
    /// `placeholder`, or no words. A date format (`mm/dd/yyyy`) is a
    /// browser's empty field, which the platform's control does not have.
    static func placeholder(_ props: [String: String]) -> String {
        props["placeholder"] ?? ""
    }

    static let utc = TimeZone(identifier: "UTC")!
    private static func formatter(_ pattern: String) -> DateFormatter {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .iso8601)
        f.timeZone = utc
        f.dateFormat = pattern
        return f
    }
    private static let date = formatter("yyyy-MM-dd"), clock = formatter("HH:mm:ss")
    private static let calendar: Calendar = {
        var calendar = Calendar(identifier: .iso8601)
        calendar.timeZone = utc
        return calendar
    }()

    /// `s` by HTML's value grammar for `kind`, as the kernel holds a value to
    /// it (`ControlKind::valid_value`): a date `yyyy-mm-dd` (the year four or
    /// more digits, never 0), a time `hh:mm` with optional `:ss` and one to
    /// three fraction digits, a local date and time the two joined by `T` or
    /// a space, and nothing after. A value HTML's sanitization would empty is
    /// nil.
    static func parse(_ kind: String, _ s: String) -> Date? {
        let digits = { (part: Substring, count: ClosedRange<Int>) -> Int? in
            count.contains(part.count) && part.allSatisfy({ ("0"..."9").contains($0) }) ? Int(part) : nil
        }
        var day: Substring?, clock: Substring?
        switch kind {
        case "date": day = s[...]
        case "time": clock = s[...]
        default:
            guard let at = s.firstIndex(where: { $0 == "T" || $0 == " " }) else { return nil }
            day = s[..<at]
            clock = s[s.index(after: at)...]
        }
        var fields = DateComponents(year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, nanosecond: 0)
        if let day {
            let f = day.split(separator: "-", omittingEmptySubsequences: false)
            guard f.count == 3, let y = digits(f[0], 4...Int.max), y > 0, let m = digits(f[1], 2...2), let d = digits(f[2], 2...2) else { return nil }
            fields.year = y
            fields.month = m
            fields.day = d
        }
        if let clock {
            let f = clock.split(separator: ":", omittingEmptySubsequences: false)
            guard (2...3).contains(f.count), let h = digits(f[0], 2...2), h < 24, let m = digits(f[1], 2...2), m < 60 else { return nil }
            fields.hour = h
            fields.minute = m
            if f.count == 3 {
                let second = f[2].split(separator: ".", omittingEmptySubsequences: false)
                guard second.count <= 2, let whole = digits(second[0], 2...2), whole < 60 else { return nil }
                fields.second = whole
                if second.count == 2 {
                    guard let fraction = digits(second[1], 1...3) else { return nil }
                    fields.nanosecond = fraction * [100_000_000, 10_000_000, 1_000_000][second[1].count - 1]
                }
            }
        }
        // A day the month does not have (February's 30th) is not a date.
        guard fields.isValidDate(in: calendar) else { return nil }
        return calendar.date(from: fields)
    }

    /// `d` as HTML's normalized value for `kind`, as `el.value` reads a local
    /// date and time: seconds only when they or the fraction are not zero,
    /// and the fraction, in whole milliseconds, in as few digits as it needs.
    static func format(_ kind: String, _ d: Date) -> String {
        // Whole milliseconds, rounded: a Date's binary fraction reads .5 as .4999….
        let (q, r) = Int64((d.timeIntervalSince1970 * 1000).rounded()).quotientAndRemainder(dividingBy: 1000)
        let second = Date(timeIntervalSince1970: Double(r < 0 ? q - 1 : q)), fraction = r < 0 ? r + 1000 : r
        if kind == "date" { return date.string(from: second) }
        var time = clock.string(from: second)
        if fraction != 0 {
            time += "." + String(String(format: "%03d", fraction).reversed().drop(while: { $0 == "0" }).reversed())
        } else if time.hasSuffix(":00") {
            time.removeLast(3)
        }
        return kind == "time" ? time : date.string(from: second) + "T" + time
    }

    /// What a picker showing `d` reads after the agent's `type` gave it
    /// `typed`: `typed`, the value the app heard, while the picker still
    /// shows that value, else what an action wrote over it. A picker shows
    /// no finer than a minute and UIKit's may drop the seconds it is given,
    /// so the minute is what is compared.
    static func shown(_ kind: String, typed: String, _ d: Date) -> String {
        let minute = { (d: Date) in (d.timeIntervalSince1970 / 60).rounded(.down) }
        guard let value = parse(kind, typed), minute(value) == minute(d) else { return format(kind, d) }
        return typed
    }

    /// The wall clock's fields in the picker's zone-free representation,
    /// using the same reference day as parsed bounds for a time-only input.
    static func initial(_ kind: String, now: Date = Date(), timeZone: TimeZone = .current,
                        minimum: Date? = nil, maximum: Date? = nil) -> Date {
        let f = formatter(kind == "date" ? "yyyy-MM-dd" : kind == "time" ? "HH:mm" : "yyyy-MM-dd'T'HH:mm")
        f.timeZone = timeZone
        var value = parse(kind, f.string(from: now))!
        if let minimum { value = max(value, minimum) }
        if let maximum { value = min(value, maximum) }
        return value
    }
}

/// A select's options and the one it shows (`exact_select_options`).
struct SelectMenu: Equatable {
    struct Option: Equatable { let value: String; let label: String; let disabled: Bool }
    var options: [Option] = []
    var chosen: Int?
    var chosenValue: String? { chosen.map { options[$0].value } }

    init(options: [Option] = [], chosen: Int? = nil) { self.options = options; self.chosen = chosen }
    init(json: Data) {
        guard let obj = try? JSONSerialization.jsonObject(with: json) as? [String: Any] else { return }
        options = (obj["options"] as? [[String: Any]] ?? []).map {
            Option(value: $0["value"] as? String ?? "", label: $0["label"] as? String ?? "", disabled: $0["disabled"] as? Bool ?? false)
        }
        chosen = (obj["chosen"] as? Int).flatMap { $0 < options.count ? $0 : nil }
    }

    /// The agent's `type <select> <value>` (LLP 1069.001 D9): the enabled
    /// option with that value, else the one option with that label, as the
    /// web host chooses (Playwright's `selectOption`; kanban F17, shop F10);
    /// else the refusal, naming the options.
    func choose(_ text: String, id: UInt32) -> (value: String?, refusal: String?) {
        let enabled = options.filter { !$0.disabled }
        if enabled.contains(where: { $0.value == text }) { return (text, nil) }
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        let labelled = enabled.filter { $0.label.trimmingCharacters(in: .whitespaces) == trimmed }
        if labelled.count == 1 { return (labelled[0].value, nil) }
        let twice = labelled.count > 1 ? " (that label is on more than one option: choose by value)" : ""
        return (nil, "select \(id) has no enabled option \"\(text)\"\(twice) (options: \(enabled.map { "\"\($0.value)\" \"\($0.label)\"" }.joined(separator: ", ")))")
    }
}

/// A button's face (`exact_press_face`, LLP 1069.011.000 D1), custom or
/// native, as the kernel reads it from its children: its title, its symbol as
/// the platform names it (an svg-only icon's path as its `FaceSymbol` token), whether its image is a raster instead, whether the
/// image leads, its `aria-label`, whether its children fit (one text, one
/// image, nothing else); and a native button's `buttonStyles` row (LLP
/// 1069.011 D2) — each platform's draw, a `~` marking a stand-in. `style` is
/// the `buttonStyle` its author wrote, nil when they wrote none: the face
/// then carries no row, and the platform columns keep their defaults here,
/// which are what a button with no style draws (`unsaid`; the Mac's push
/// button).
struct ButtonFace: Equatable {
    /// Whether the node is a button at all; the rest is empty when not.
    var button = true
    var title: String?
    var subtitle: String?
    var placement: String?
    var rows = ButtonFaceRows()
    var symbol: String?
    /// An svg-only icon's token name (`icon.reply`), the agent's name for its path.
    var symbolIcon: String?
    struct Image: Equatable {
        var view: UInt32
        var source: String
        var size: CGSize?
    }
    var image: Image?
    var raster = false
    var leading = true
    var label: String?
    var fits = true
    /// A tab's badge (LLP 1075.003 §9.9): the text of the filled box beside
    /// its face, which the kernel reads from the tab's children.
    var badge: String?
    var style: String?
    var ios = ButtonFace.unsaid
    var iosBefore26 = ButtonFace.unsaid
    var macos = "push"
    var known = true

    init() {}
    init(json: Data) {
        guard let o = try? JSONSerialization.jsonObject(with: json) as? [String: Any] else { return }
        button = o["button"] as? Bool ?? true
        title = o["title"] as? String
        subtitle = o["subtitle"] as? String
        placement = o["placement"] as? String
        rows = ButtonFaceRows(o["rows"] as? [String: Any] ?? [:])
        // An svg-only icon's path as its token (FaceSymbol, stage 9 A2).
        symbol = FaceSymbol.of(o)
        symbolIcon = o["symbolIcon"] as? String
        raster = o["raster"] as? Bool ?? false
        if let i = o["image"] as? [String: Any], let source = i["source"] as? String, let view = i["view"] as? NSNumber {
            let size = (i["size"] as? [Double]).flatMap { $0.count == 2 ? CGSize(width: $0[0], height: $0[1]) : nil }
            image = Image(view: view.uint32Value, source: source, size: size)
        }
        leading = o["leading"] as? Bool ?? true
        label = o["label"] as? String
        fits = o["fits"] as? Bool ?? true
        badge = o["badge"] as? String
        style = o["style"] as? String
        ios = o["ios"] as? String ?? ios
        iosBefore26 = o["iosBefore26"] as? String ?? iosBefore26
        macos = o["macos"] as? String ?? macos
        known = o["known"] as? Bool ?? true
    }

    /// What UIKit draws for a button whose author wrote no `buttonStyle`, by
    /// its name in the table's iOS column (LLP 1115.000 D15). On iOS it is
    /// the plain configuration, tinted text with no fill, as `UIButton(type:
    /// .system)` and SwiftUI's `Button` are; `buttonStyle="bordered"` is what
    /// an author writes for the grey capsule. tvOS keeps the bordered button:
    /// its focus look is drawn on the button's own shape, and a plain button
    /// has none to draw it on. The two are told apart here and nowhere else.
    #if os(iOS)
    static let unsaid = "plain"
    #else
    static let unsaid = "bordered"
    #endif

    /// The text a platform entry shows (a menu row, a confirmation's action):
    /// the title, else the label (a symbol-only button's).
    var shown: String? { title ?? label }

    /// A platform name without its stand-in mark, and whether it had one.
    static func drawn(_ name: String) -> (name: String, standIn: Bool) {
        name.hasPrefix("~") ? (String(name.dropFirst()), true) : (name, false)
    }
}

/// Whether UIKit and AppKit draw this app in their 26 design (Liquid Glass):
/// they key it on the SDK the main executable records in `LC_BUILD_VERSION`,
/// not on the OS, so an app linked to record an older SDK
/// (`designRequiresCompatibility`, `host/apple/build.mjs`) draws as before 26
/// on iOS 27 and macOS 27, where a glass button configuration or bezel draws
/// no glass. Read once; callers also check the OS (`#available(… 26.0, *)`).
enum LinkedDesign {
    static let liquidGlass: Bool = {
        guard let header = _dyld_get_image_header(0) else { return false }
        var at = UnsafeRawPointer(header).advanced(by: MemoryLayout<mach_header_64>.size)
        for _ in 0..<header.pointee.ncmds {
            let command = at.loadUnaligned(as: load_command.self)
            if command.cmd == UInt32(LC_BUILD_VERSION) {
                return at.loadUnaligned(as: build_version_command.self).sdk >= 26 << 16
            }
            at = at.advanced(by: Int(command.cmdsize))
        }
        return false
    }()
}


/// Only authored rows cross this dictionary; absence is UIKit's answer (D1–D2).
struct ButtonFaceRows: Equatable {
    var title: NodeStyle = [:], subtitle: NodeStyle = [:], symbol: NodeStyle = [:], button: NodeStyle = [:]
    var imageGap: CGFloat?
    init() {}
    init(_ object: [String: Any]) {
        func value(_ v: Any) -> BatchValue {
            if let s = v as? String { return .string(s) }
            if let n = v as? NSNumber { return .number(n.doubleValue) }
            if let a = v as? [Any] { return .array(a.map(value)) }
            if let o = v as? [String: Any] {
                let values = o.mapValues(value)
                if let c = CalculatedLength(values) { return .calculation(c) }
                return .object(values)
            }
            return .null
        }
        func rows(_ name: String) -> NodeStyle { (object[name] as? [String: Any] ?? [:]).mapValues(value) }
        title = rows("title"); subtitle = rows("subtitle"); symbol = rows("symbol"); button = rows("button")
        imageGap = (object["imageGap"] as? NSNumber).map { CGFloat($0.doubleValue) }
    }
}
