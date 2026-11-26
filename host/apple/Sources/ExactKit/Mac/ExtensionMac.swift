// A box's picture mirrored past its border box on AppKit (LLP 1123 D13's extension). The extension is a subview of the
// box's own view, behind its children and taking no clicks: on macOS 26 an `NSBackgroundExtensionView` whose content
// view, placed by hand at the border box (`automaticallyPlacesContentView` off), holds the box's picture (its fill,
// gradients, inset shadow and an image's picture), which the box's own layer stops painting meanwhile; before macOS 26
// a strip in the box's background colour around the border box, the picture staying on the box.
#if os(macOS)
import AppKit

/// The views of a box's extension (`NodeView.setExtension`).
package final class BoxExtension {
    /// The view behind the box's children: the extension view, or before macOS 26 the colour strip.
    let view: NSView
    /// The extension view's content view, which holds the box's picture; nil for the strip.
    let picture: NSView?
    /// How far it reaches past the border box, top right bottom left.
    var outsets: NSEdgeInsets

    init(view: NSView, picture: NSView?, outsets: NSEdgeInsets) {
        self.view = view; self.picture = picture; self.outsets = outsets
    }
}

/// What a box asked to extend by and the extension it shows, kept beside its view (`NodeView.boxExtension`).
final class ExtensionState {
    var asked = NSEdgeInsetsZero
    var made: BoxExtension?
}

/// The extension view on macOS 26, which takes no click: a click on its strip is the box's.
@available(macOS 26.0, *)
final class ExtensionMirror: NSBackgroundExtensionView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func isAccessibilityElement() -> Bool { false }
}

/// The extension view's content: the box's picture at the border box, in the box's own orientation.
final class ExtensionPicture: NSView {
    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func isAccessibilityElement() -> Bool { false }
}

/// Before macOS 26: the strip around the border box in the box's background colour, which paints once, the border box
/// left to the box's own fill as an extension view leaves it to its content.
final class ExtensionStrip: NSView {
    /// The strip's paint, which takes the box's `dynamic-range-limit` (`applyColorRanges`).
    let strip = CAShapeLayer()

    var colour: CGColor? {
        get { strip.fillColor }
        set {
            guard newValue != strip.fillColor else { return }
            CATransaction.begin(); CATransaction.setDisableActions(true)
            strip.fillColor = newValue
            CATransaction.commit()
        }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        strip.fillRule = .evenOdd
        layer?.addSublayer(strip)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func isAccessibilityElement() -> Bool { false }

    /// Paints its bounds but `box`, the border box in the strip's coordinates.
    func place(around box: CGRect) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let path = CGMutablePath()
        path.addRect(bounds)
        path.addRect(box)
        strip.frame = bounds
        strip.path = path
    }
}

extension NodeView {
    /// The box's extension while it shows one.
    package var boxExtension: BoxExtension? { Self.extensionStates.object(forKey: self)?.made }

    /// Mirrors the box's picture past its border box by `outsets`, top right bottom left (`Kernel::extension`), or ends
    /// the extension at zero, as far as the box allows; its paint follows.
    package func setExtension(_ outsets: NSEdgeInsets) {
        let state = Self.extensionStates.object(forKey: self) ?? ExtensionState()
        state.asked = outsets
        if NSEdgeInsetsEqual(outsets, NSEdgeInsetsZero), state.made == nil {
            Self.extensionStates.removeObject(forKey: self)
        } else {
            Self.extensionStates.setObject(state, forKey: self)
        }
        applyBoxLayer()
        applyShadow()
    }

    /// Builds, places or ends the extension asked for as the box allows it now, the picture's layers moving to where
    /// the box paints its picture after it (`pictureHost`); `applyBoxLayer` calls it before it paints, since what
    /// allows it can change with no new outsets.
    func updateExtension() {
        guard let state = Self.extensionStates.object(forKey: self) else { return }
        let outsets = state.asked
        guard !NSEdgeInsetsEqual(outsets, NSEdgeInsetsZero), extensionAllowed else {
            clearExtension()
            if NSEdgeInsetsEqual(outsets, NSEdgeInsetsZero) { Self.extensionStates.removeObject(forKey: self) }
            return
        }
        if let made = state.made {
            made.outsets = outsets
        } else {
            let made: BoxExtension
            if #available(macOS 26.0, *), Self.extensionMirrors {
                let view = ExtensionMirror(), picture = ExtensionPicture()
                view.automaticallyPlacesContentView = false
                view.contentView = picture
                made = BoxExtension(view: view, picture: picture, outsets: outsets)
            } else {
                made = BoxExtension(view: ExtensionStrip(), picture: nil, outsets: outsets)
            }
            made.view.setAccessibilityHidden(true)
            rehostPicture { state.made = made }
            addSubview(made.view, positioned: .below, relativeTo: nil)
            made.view.setPaintBehind()
        }
        placeExtension()
    }

    /// Whether the box may extend (D13): a view, a button or an image, whose picture the host paints in layers, with no
    /// authored mask or clip path, no filter of its own or above it, no overflow clip, which would clip the extension
    /// as a view that clips to its bounds clips its subviews, no material the platform draws, and no canvas above it,
    /// which may show it through a capture. Before macOS 26, where only the background colour extends, the box needs
    /// a colour.
    var extensionAllowed: Bool {
        guard ["view", "button", "image"].contains(kind), scroll == nil, layerBoxEligible, !drawsPaint, style["clip_path"] == nil,
              style["mask_image"] == nil, boxFilter == nil, !overflowClips, materialContent == nil, canvasAbove == nil,
              presenter?.boxFilters?.filtered(above: self) != true else { return false }
        return Self.extensionMirrors || (pictureFill?.alpha ?? 0) > 0
    }

    /// Each box's extension state, by its box, so a view's own reads find it without a cast (`heldSubviews`,
    /// `isBoxExtension`): an entry goes with the box.
    nonisolated(unsafe) static let extensionStates = NSMapTable<NSView, ExtensionState>.weakToStrongObjects()

    /// Whether an extension mirrors the box's picture with AppKit's extension view (macOS 26) rather than painting the
    /// strip in its colour; tests clear it to drive the strip.
    nonisolated(unsafe) static var extensionMirrors: Bool = {
        if #available(macOS 26.0, *) { return true }
        return false
    }()

    /// Ends the box's extension: its picture goes back to the box's own layer.
    func clearExtension() {
        guard let state = Self.extensionStates.object(forKey: self), let view = state.made?.view else { return }
        rehostPicture { state.made = nil }
        view.removeFromSuperview()
        PaintOrder.changed(self)
    }

    /// The extension's rectangle in the box's coordinates, the laid-out border box outset by its outsets.
    var extensionFrame: CGRect? {
        guard let o = boxExtension?.outsets else { return nil }
        let box = laidOutBox
        return CGRect(x: box.minX - o.left, y: box.minY - o.top, width: box.width + o.left + o.right, height: box.height + o.top + o.bottom)
    }

    /// The extension's frames: the view's (`extensionFrame`), and the picture or the strip's hole at the border box.
    func placeExtension() {
        guard let e = boxExtension, let frame = extensionFrame else { return }
        let box = CGRect(x: e.outsets.left, y: e.outsets.top, width: laidOutBox.width, height: laidOutBox.height)
        if e.view.frame != frame { e.view.frame = frame }
        e.picture?.frame = box
        if let strip = e.view as? ExtensionStrip {
            strip.place(around: box)
            strip.colour = pictureFill
        }
    }

    /// Runs `change`, which starts or ends the extension, moving the picture's layers to where the box paints its
    /// picture after it (`pictureHost`); the caller paints the box there.
    private func rehostPicture(_ change: () -> Void) {
        let before = pictureHost
        let moving = pictureLayers.filter { $0.superlayer === before }
        change()
        for l in moving { insertBoxSublayer(l) }
    }

    /// Where the box's picture paints, its fill, gradients, inset shadow and an image's picture: the extension's content
    /// view's layer while it is mirrored past its border box and no layout transition's surface paints the box, else
    /// its own layer.
    package var pictureHost: CALayer? { surface == nil ? boxExtension?.picture?.layer ?? layer : layer }

    /// The layers of the box's picture that exist now (`pictureHost`).
    package var pictureLayers: [CALayer] { [boxFill, boxGradient, boxPattern, insetCaster, imageLayer].compactMap { $0 } }
}

extension NSView {
    /// Whether it is a box's extension view, which paints its own frame and holds AppKit's own views.
    package var isBoxExtension: Bool {
        superview.map { NodeView.extensionStates.object(forKey: $0)?.made?.view === self } ?? false
    }
}
#endif
