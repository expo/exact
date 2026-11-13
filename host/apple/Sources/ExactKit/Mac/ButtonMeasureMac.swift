// @ref LLP 1069.011.001 D11: AppKit fitting answers precede publication.
#if os(macOS)
import AppKit
import CExact

final class ButtonMeasureCache: @unchecked Sendable {
    struct Key: Hashable {
        let face: Data
        let widthKind: UInt8
        let width: Float
        let traits: FieldChromeCache.Traits
    }
    private let lock = NSLock()
    private var appearance = NSAppearance(named: .aqua)!
    private var traits = FieldChromeCache.Traits(appearance: NSAppearance.Name.aqua.rawValue, scale: 1)
    /// Each title, offer and kind at the current traits, within a budget: a
    /// session's history of faces is not kept.
    private var entries = RecentMeasures<Key, ExactButtonMeasure>(entries: 256, bytes: 128 << 10) { key, _ in key.face.count }
    private(set) var misses = 0
    var retained: (entries: Int, bytes: Int) { lock.lock(); defer { lock.unlock() }; return entries.retained }
    @discardableResult
    func configure(_ next: NSAppearance, scale: CGFloat) -> Bool {
        precondition(Thread.isMainThread)
        let nextTraits = FieldChromeCache.Traits(appearance: next.name.rawValue, scale: max(1, scale))
        lock.lock(); defer { lock.unlock() }
        let changed = traits != nextTraits
        appearance = next; traits = nextTraits
        // Measurements at the old traits answer no key from now on.
        if changed { entries.removeAll() }
        return changed
    }
    /// A miss is measured on main before it is answered, so the answer is
    /// exact and the kernel keeps it: the cache's budget then costs only a
    /// measurement, never a layout pass (a root with more faces than the budget
    /// would otherwise stay provisional through every pass). Only when the
    /// traits change between the key and the measurement is the answer
    /// provisional, and the host lays out again under the new key.
    func answer(face data: Data, widthKind: UInt8, width: Float) -> ExactButtonMeasure {
        lock.lock()
        let key = Key(face: data, widthKind: widthKind, width: width, traits: traits)
        if let exact = entries.value(key) { lock.unlock(); return exact }
        misses += 1
        lock.unlock()
        return Owner.shared.callMain {
            self.lock.lock()
            let current = self.appearance, currentTraits = self.traits
            self.lock.unlock()
            let now = Key(face: data, widthKind: widthKind, width: width, traits: currentTraits)
            var measured = self.measure(ButtonFace(json: data), widthKind: widthKind, width: CGFloat(width), appearance: current, scale: currentTraits.scale)
            self.lock.lock(); self.entries.insert(now, measured); self.lock.unlock()
            measured.provisional = now == key ? 0 : 1
            return measured
        }
    }
    func measure(_ face: ButtonFace, widthKind: UInt8, width: CGFloat, appearance: NSAppearance, scale: CGFloat) -> ExactButtonMeasure {
        HostPhases.shared.count(.nativeControlMeasurements)
        precondition(Thread.isMainThread)
        var result = ExactButtonMeasure()
        appearance.performAsCurrentDrawingAppearance {
            let button = NSButton(title: "", target: nil, action: nil)
            ButtonConfigurationMac.apply(face, to: button, appearance: appearance, accent: nil)
            if widthKind == 1 {
                var minimum = face
                minimum.title = TextEngine.widestButtonRun(face.title, whiteSpace: face.rows.title["white_space"]?.string) { run in
                    var candidate = face; candidate.title = run
                    ButtonConfigurationMac.apply(candidate, to: button, appearance: appearance, accent: nil)
                    return button.fittingSize.width
                }
                // AppKit's declared single-line stand-in truncates in this box;
                // it must not give the flex item a max-content auto minimum.
                ButtonConfigurationMac.apply(minimum, to: button, appearance: appearance, accent: nil)
            }
            // Push buttons do not wrap at the offered width. A required width
            // constraint makes fittingSize answer the real constrained frame;
            // window/Auto Layout ground truth is covered by the macOS tests.
            let image = ButtonConfigurationMac.imageSizing(face, button: button)
            if widthKind == 0, image == nil {
                button.translatesAutoresizingMaskIntoConstraints = false
                button.widthAnchor.constraint(equalToConstant: max(0, width)).isActive = true
            }
            var size = image?.size ?? button.fittingSize
            if widthKind == 0, image != nil { size.width = max(0, width) }
            result = ExactButtonMeasure(width: Float(ceil(max(0, size.width) * scale) / scale),
                height: Float(ceil(max(0, size.height) * scale) / scale), provisional: 0)
        }
        return result
    }
}

extension TextEngine {
    static let buttonMeasure: ExactButtonMeasureFn = { ctx, pointer in
        guard let ctx, let request = pointer?.pointee, let bytes = request.face else { return ExactButtonMeasure() }
        let engine = Unmanaged<TextEngine>.fromOpaque(ctx).takeUnretainedValue()
        return engine.buttonMeasurements?.answer(face: Data(bytes: bytes, count: request.face_len),
            widthKind: request.width_kind, width: request.width) ?? ExactButtonMeasure()
    }
}
#endif
