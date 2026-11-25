// @fixture/swatch on Apple: a box filled with its `tint`, which sends
// `message` "tap" when clicked or tapped, made by the session's one instance,
// which keeps what it was told of its views and answers it, with what it
// reads of its configuration, as `swatchReports`. On iOS the box is a view
// controller's view, which the host contains.
import ExactPackage
import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public final class SwatchModule: ExactPackage {
    private let context: ExactPackageContext
    private var heard: [String] = []

    public init(context: ExactPackageContext) { self.context = context }

    private func say(_ line: String) {
        heard.append(line)
        context.changed("swatch.swatchReports")
    }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        guard name == "swatchReports" else { return reply(["error": "NotSupportedError"]) }
        reply(["text": heard.joined(separator: "; "), "count": heard.count, "config": config])
    }

    /// What it reads of its configuration: the label, and the key by its
    /// length and last four characters only.
    private var config: String {
        let key = context.config["apiKey"] as? String ?? ""
        return "label=\(context.config["label"] as? String ?? "none") apiKey=\(key.isEmpty ? "none" : "\(key.count) chars ending \(key.suffix(4))") development=\(context.development)"
    }

    public func view(_ tag: String, id: String, props: [String: String], events: ExactPackageEvents) throws -> ExactPackageView {
        guard tag == "swatch-box" else { throw ExactPackageRefusal("no view \(tag)") }
        let box = SwatchBox(events: events) { [weak self] held in self?.say("held \(id) \(held)") }
        try box.setProps(props)
        say("made \(id)")
        return box
    }

    public func displayed(_ id: String, _ shown: Bool, origin: String) { say("displayed \(id) \(shown) \(origin)") }
    public func bound(_ id: String, _ prop: String, origin: String) { say("bound \(id) \(prop) \(origin)") }
}

#if os(macOS)
private final class Fill: NSView {
    var tapped: () -> Void = {}
    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func mouseDown(with event: NSEvent) { tapped() }
}
#else
private final class Fill: UIView {}

/// The box's controller. Each `didMove(toParent:)` reports whether the
/// parent's view holds the box, as UIKit requires.
private final class Holder: UIViewController {
    var moved: (Bool) -> Void = { _ in }
    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)
        if let parent { moved(view.isDescendant(of: parent.view)) }
    }
}
#endif

private final class SwatchBox: NSObject, ExactPackageView {
    private let fill = Fill()
    private let events: ExactPackageEvents
    #if !os(macOS)
    private let holder = Holder()
    #endif

    init(events: ExactPackageEvents, held: @escaping (Bool) -> Void) {
        self.events = events
        super.init()
        #if os(macOS)
        fill.wantsLayer = true
        fill.layer?.cornerRadius = 8
        fill.tapped = { [weak self] in self?.events.message("tap") }
        #else
        holder.view = fill
        holder.moved = held
        fill.layer.cornerRadius = 8
        fill.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tap)))
        #endif
    }

    #if !os(macOS)
    @objc private func tap() { events.message("tap") }
    #endif

    var view: ExactPackagePlatformView { fill }
    #if !os(macOS)
    var controller: UIViewController? { holder }
    #endif

    func setProps(_ props: [String: String]) throws {
        let color = Self.color(props["tint"] ?? "")
        #if os(macOS)
        fill.layer?.backgroundColor = color?.cgColor
        #else
        fill.backgroundColor = color
        #endif
    }

    /// `#rgb` or `#rrggbb`.
    private static func color(_ hex: String) -> PlatformColor? {
        var digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        if digits.count == 3 { digits = digits.map { "\($0)\($0)" }.joined() }
        guard digits.count == 6, let v = UInt32(digits, radix: 16) else { return nil }
        let c = { (shift: UInt32) in CGFloat((v >> shift) & 0xff) / 255 }
        return PlatformColor(red: c(16), green: c(8), blue: c(0), alpha: 1)
    }
}

#if os(macOS)
private typealias PlatformColor = NSColor
#else
private typealias PlatformColor = UIColor
#endif
