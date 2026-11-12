#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Authored active properties, independent of the native control's state.
struct NativeButtonState: Equatable {
    var style: NodeStyle = [:]

    init() {}
    init(_ owner: NodeView) {
        for key in ["press_background_color", "press_color", "press_opacity", "press_opacity_to", "press_scale"] {
            style[key] = owner.style[key]
        }
    }

    /// Face rows retain the ownership of viewless text and symbols.
    static func colorOwnership(_ base: ButtonFaceRows) -> (title: Bool, secondary: Bool, image: Bool) {
        func ownsColor(_ rows: NodeStyle) -> Bool { rows["written"]?.array?.contains(.string("text_color")) == true }
        return (ownsColor(base.title), ownsColor(base.subtitle), ownsColor(base.symbol) || base.symbol["tint_color"] != nil)
    }

    func properties(over base: ButtonFaceRows) -> ButtonFaceRows {
        let owns = Self.colorOwnership(base)
        var rows = ButtonFaceRows()
        rows.button["background_color"] = style["press_background_color"]
        if let ink = style["press_color"] {
            if !owns.title { rows.title["text_color"] = ink }
            if !owns.secondary { rows.subtitle["text_color"] = ink }
            if !owns.image { rows.symbol["tint_color"] = ink }
        }
        return rows
    }

    /// Presentation-only overrides keep the native control's tracking geometry.
    func present(on owner: NodeView, down: Bool) {
        #if os(macOS)
        guard let layer = owner.layer else { return }
        #else
        let layer = owner.layer
        #endif
        ActivePresentation.apply(style, to: layer, down: down, origin: owner.transformOriginPoint,
                                 opacityFactor: owner.filterOpacityFactor, baseOpacity: owner.ownOpacity * owner.filterOpacityFactor)
    }
}

extension NodeView {
    /// Reject only at the host projection that knows which native API will draw.
    @discardableResult
    func rejectNativeActiveProperties(on component: String, supporting supported: Set<String> = []) -> Bool {
        let unsupported = NativeButtonState(self).style.keys.filter { !supported.contains($0) }
        let names = Set(unsupported.map { key in
            switch key {
            case "press_color": return "color"
            case "press_background_color": return "background-color"
            case "press_scale": return "-exact-press-scale"
            default: return "opacity"
            }
        }).sorted()
        let message = names.isEmpty ? nil : "node #\(id): :active \(names.joined(separator: ", ")) cannot be represented by \(component). Refusing the active overrides; remove those properties. Keeping \(component)."
        if message != nativeActiveRefusals[component] {
            nativeActiveRefusals[component] = message
            if let message { presenter?.session?.log(message) }
        }
        return names.isEmpty
    }

    package var nativeButtonColorOwnership: (title: Bool, secondary: Bool, image: Bool)? {
        guard isNativeButton, let face else { return nil }
        return NativeButtonState.colorOwnership(face.rows)
    }

    func refreshNativeActivePresentation() {
        #if os(macOS)
        guard let button = presenter?.controls.controls[id] as? NativeButtonMac else { return }
        button.written?.active.present(on: self, down: button.isEnabled && button.cell?.isHighlighted == true)
        #else
        guard let button = presenter?.controls.controls[id] as? NativeButtonIOS else { return }
        button.written?.active.present(on: self, down: button.isEnabled && button.isHighlighted)
        #endif
    }
}

/// An authored opacity or scale adds no timing of its own to native state updates.
package enum ActivePresentation {
    package static func restore(_ layer: CALayer) {
        layer.removeAnimation(forKey: "nativeActiveOpacity")
        layer.removeAnimation(forKey: "nativeActiveScale")
    }
    package static func apply(_ style: NodeStyle, to layer: CALayer, down: Bool, origin: CGPoint? = nil,
                              opacityFactor: Float = 1, baseOpacity: Float? = nil) {
        // A style batch can refresh active state before the filter writes its model opacity.
        let base = baseOpacity ?? layer.opacity
        let opacity = down ? style["press_opacity_to"]?.number.map { Float(min(1, max(0, $0))) * opacityFactor }
            ?? base * Float(min(1, max(0, style["press_opacity"]?.number ?? 1))) : base
        let delta = opacity - base
        if delta == 0 { layer.removeAnimation(forKey: "nativeActiveOpacity") }
        else { hold(layer, key: "nativeActiveOpacity", path: "opacity", value: delta, additive: true) }
        let scale = down ? style["press_scale"]?.number ?? 1 : 1
        if scale == 1 || !scale.isFinite || scale <= 0 { layer.removeAnimation(forKey: "nativeActiveScale") }
        else {
            let origin = origin ?? CGPoint(x: layer.bounds.midX, y: layer.bounds.midY)
            let delta = CGPoint(x: origin.x - layer.bounds.minX - layer.bounds.width * layer.anchorPoint.x,
                                y: origin.y - layer.bounds.minY - layer.bounds.height * layer.anchorPoint.y)
            var transform = CATransform3DTranslate(layer.sublayerTransform, delta.x, delta.y, 0)
            transform = CATransform3DScale(transform, CGFloat(scale), CGFloat(scale), 1)
            transform = CATransform3DTranslate(transform, -delta.x, -delta.y, 0)
            hold(layer, key: "nativeActiveScale", path: "sublayerTransform",
                 value: NSValue(caTransform3D: transform), additive: false)
        }
    }

    private static func hold(_ layer: CALayer, key: String, path: String, value: Any, additive: Bool) {
        if let target = value as? NSObject, let previous = layer.animation(forKey: key) as? CABasicAnimation,
           previous.toValue as? NSObject == target, previous.isAdditive == additive { return }
        let animation = CABasicAnimation(keyPath: path)
        animation.fromValue = value
        animation.toValue = value
        animation.isAdditive = additive
        animation.duration = .greatestFiniteMagnitude
        animation.isRemovedOnCompletion = false
        layer.add(animation, forKey: key)
    }
}
