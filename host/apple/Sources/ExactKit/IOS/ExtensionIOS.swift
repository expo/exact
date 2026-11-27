// A box's picture mirrored past its border box (LLP 1123 D13's extension). The extension is a child of the box's own
// view, behind its children and taking no touches: for an image, Exact's own mirror (`ExtensionView`,
// BackgroundMirrorIOS.swift), which keeps its mirror at the picture's edge through an animation; for another box on
// iOS 26 a `UIBackgroundExtensionView` whose content view, placed
// by hand at the border box, holds the box's picture (its fill, gradients, inset shadow and an image's picture), which
// the box's own layer stops painting meanwhile; before iOS 26 a strip in the box's background colour around the border
// box, the picture staying on the box. UIKit's extension view lays a view of its own over its content and takes every
// hit there, so its content is a picture, never the box's children.
#if os(iOS) || os(tvOS)
import UIKit

/// The views of a box's extension (`NodeView.setExtension`).
package final class BoxExtension {
    /// The view behind the box's children: the extension view, or before iOS 26 the colour strip.
    let view: UIView
    /// The extension view's content view, which holds the box's picture; nil for the strip.
    let picture: UIView?
    /// How far it reaches past the border box, top right bottom left.
    var outsets: UIEdgeInsets

    init(view: UIView, picture: UIView?, outsets: UIEdgeInsets) {
        self.view = view; self.picture = picture; self.outsets = outsets
    }
}

/// Before iOS 26: the strip around the border box in the box's background colour, which paints once, the border box
/// left to the box's own fill as an extension view leaves it to its content.
final class ExtensionStrip: UIView {
    /// The strip's paint, which takes the box's `dynamic-range-limit` (`applyColorRanges`).
    let strip = CAShapeLayer()

    /// The box's background colour as the box paints it (`applyBoxLayer`), nil for none.
    var colour: CGColor? {
        get { strip.fillColor }
        set {
            guard newValue != strip.fillColor else { return }
            CATransaction.begin(); CATransaction.setDisableActions(true)
            strip.fillColor = newValue
            CATransaction.commit()
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        strip.fillRule = .evenOdd
        layer.addSublayer(strip)
    }

    required init?(coder: NSCoder) { nil }

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
    /// Mirrors the box's picture past its border box by `outsets`, top right bottom left (`Kernel::extension`), or ends
    /// the extension at zero, as far as the box allows (`applyExtension`).
    package func setExtension(_ outsets: UIEdgeInsets) {
        if outsets != .zero || extras != nil { more.extensionAsked = outsets }
        if outsets != .zero { Self.askedToExtend.add(self) } else { Self.askedToExtend.remove(self) }
        applyExtension()
    }

    /// The boxes asked to extend. Each checks again whether it may, with no new outsets and no paint of its own, as it or
    /// an ancestor moves or an ancestor's filter comes or goes (`extensionsUnder`), and, for the colour strip, as an agent
    /// moves the clock, which can start or end an animation of the background colour (`animatesBackground`).
    nonisolated(unsafe) static let askedToExtend = NSHashTable<NodeView>.weakObjects()
    /// The boxes that name edges to mirror their picture on (`boxExtendEdges`), which the host sizes while the kernel
    /// gives them no extension (`Presenter.sideExtension`).
    nonisolated(unsafe) static let namesExtension = NSHashTable<NodeView>.weakObjects()
    static func extensionsLookAgain() {
        for view in askedToExtend.allObjects { view.applyExtension() }
    }

    /// `view` moved or changed how it shows its subtree, which can put a box asked to extend in it under a capture
    /// (`shownThroughCapture`) or take it out: each paints again, and looks then, once the batch has settled what holds
    /// it (a canvas records the clip box its overlay moves into after the move).
    static func extensionsUnder(_ view: UIView) {
        guard askedToExtend.count > 0 else { return }
        for box in askedToExtend.allObjects where box.isDescendant(of: view) { box.layer.setNeedsDisplay() }
    }

    /// Builds, places or ends the extension asked for (`setExtension`) as the box allows it now; the box's painting
    /// calls it again (`NodeLayer.display`), since what allows it can change with no new outsets.
    func applyExtension() {
        let outsets = extras?.extensionAsked ?? .zero
        // Most boxes are asked for none and have none
        if outsets == .zero, boxExtension == nil { return }
        // A layout transition's surface paints the box meanwhile, its picture with it: the strip, hidden, waits for its
        // end to be looked at again (`endSurface` paints the box again)
        if surface != nil, let existing = boxExtension, existing.outsets == outsets { return placeExtension() }
        guard outsets != .zero, extensionAllowed else { return clearExtension() }
        if let existing = boxExtension {
            // Cast the outer shadow again, outside the new extension (`applyShadow`)
            if existing.outsets != outsets { existing.outsets = outsets; layer.setNeedsDisplay() }
        } else {
            let made: BoxExtension
            if #available(iOS 26, tvOS 26, *), Self.extensionMirrors, kind == "image" {
                // An image mirrors itself in Exact's own view, placed with the image in one transaction (`placeMirror`)
                made = BoxExtension(view: ExtensionView(), picture: nil, outsets: outsets)
            } else if #available(iOS 26, tvOS 26, *), Self.extensionMirrors {
                let view = UIBackgroundExtensionView()
                let picture = UIView()
                picture.isUserInteractionEnabled = false
                view.automaticallyPlacesContentView = false
                view.contentView = picture
                made = BoxExtension(view: view, picture: picture, outsets: outsets)
            } else {
                made = BoxExtension(view: ExtensionStrip(), picture: nil, outsets: outsets)
            }
            made.view.isUserInteractionEnabled = false
            made.view.accessibilityElementsHidden = true
            rehostPicture { more.boxExtension = made }
            // Last among the box's subviews, so what reads a box's first subview finds its content (a swipe action's
            // icon), and under all of them by depth, so whatever the host inserts later draws over the mirror too;
            // UIKit's hit testing ignores the depth, and the extension takes no touches
            addSubview(made.view)
            Self.extensions.setObject(made, forKey: self)
            made.view.setPaintBehind()
            // Made inside an animation (a fold or a column that brings the clearance the box mirrors into): placed first
            // where the box shows now, reaching nowhere, with no animation, so that the animation grows it from the box's
            // edges as the box moves. Placed by the animation alone it grew from a zero frame at the box's corner, the
            // picture with it, which the mirror draws.
            if UIView.inheritedAnimationDuration > 0, let shown = layer.presentation()?.bounds.size {
                let outsets = made.outsets
                made.outsets = .zero
                UIView.performWithoutAnimation {
                    CATransaction.begin(); CATransaction.setDisableActions(true)
                    placeExtension(shown: shown)
                    CATransaction.commit()
                }
                made.outsets = outsets
            }
        }
        placeExtension()
    }

    /// Whether the box may show a strip (D13): a view, a button or an image, whose picture the host paints in layers,
    /// with no authored mask or clip path, no filter, no paint the host draws into its own backing store, no capture of
    /// an ancestor that shows it (`shownThroughCapture`), no overflow clip, which would clip the strip as a view that
    /// clips to its bounds clips its subviews, and no background colour the platform's vibrancy draws. Before iOS 26,
    /// where only the background colour extends, the box needs a colour, and one Core Animation is not animating. For a
    /// box that may extend, each of these reads the same in a capture as outside one (`drawsPaint` reads the capture only
    /// for a text, a canvas and a guest, none of which extends), so a capture's displays and callbacks decide as any
    /// others do, and the copy, which displays a box before it reads its layers, takes the box as that leaves it.
    var extensionAllowed: Bool {
        // Not a box whose edge is what the host draws or hosts for it: a text's glyphs, whose ground it paints with them
        // wherever it does not raster them (every capture, a lifted preview, a rolling numeral), a field's or a control's
        // native view, a canvas's or an SVG's drawing, an iframe's page, a video or a native module; nor a box that
        // scrolls, a view scrolling on both axes among them, whose overflow clip is its scroll view's; nor a vibrant
        // fill, which is its vibrancy view's (`VibrancyIOS.swift`), no picture to mirror
        guard ["view", "button", "image"].contains(kind), scrollView == nil, !drawsPaint, style["clip_path"] == nil, style["mask_image"] == nil,
              boxFilter == nil, !shownThroughCapture, !overflowClips, vibrancyView == nil else { return false }
        // A menu's stand-in holds the fill meanwhile (`MenuButton.lendFill`), and gives it back with the extension
        if extras?.lentFill != nil { return false }
        // The colour as the box paints it, its own or a field's ground, not one a control's native track takes, and
        // that colour at rest: a strip in one colour cannot follow a CSS animation of it (`animatesBackground`)
        return Self.extensionMirrors || [layer.backgroundColor, boxFill?.backgroundColor].contains { ($0?.alpha ?? 0) > 0 } && !animatesBackground
    }

    /// Whether a CSS animation of the box's background colour is on its layer for Core Animation to play (LLP
    /// 1055.001), which a strip in one colour, before iOS 26, cannot follow: the box shows no strip while it plays, and
    /// the `animations` op and a clock's move check again. One the host samples paints through `applyBoxLayer`, which the
    /// strip follows, and an extension view mirrors the picture as it animates, the animation on it
    /// (`BoxAnimating.setFill`).
    var animatesBackground: Bool {
        layer.animationKeys()?.contains { (layer.animation(forKey: $0) as? CAPropertyAnimation)?.keyPath == "backgroundColor" } == true
    }

    /// Whether the host may show the box through a capture of an ancestor, which holds no UIKit mirror: in a canvas's
    /// overlay, placed by its surface or not, which the surface may draw from a capture (LLP 1014 D5), or under a box
    /// with a filter, which the host may show as a picture of its subtree, and does or not as the box clips (LLP
    /// 1055.000 D14).
    var shownThroughCapture: Bool {
        canvasAbove != nil || presenter?.boxFilters?.filtered(above: self) == true
    }

    /// Puts the box's extension view back at the end of its subviews as another view joins them, so a child's index is
    /// the same among `subviews` and `heldSubviews`, and a saved index puts a child back in its place; the extension
    /// view itself is offered no focus search (`didAddSubview`).
    func keepExtensionLast() {
        if let mirror = extras?.boxExtension?.view, subviews.last !== mirror { bringSubviewToFront(mirror) }
    }

    /// Each box's extension, by its box, so a view's own reads find it without a cast (`heldSubviews`, `ownFill`,
    /// `isBoxExtension`). The box alone holds it (`NodeExtras.boxExtension`): an entry goes as the box or its
    /// extension does, a destroyed box's with it.
    nonisolated(unsafe) static let extensions = NSMapTable<UIView, BoxExtension>.weakToWeakObjects()

    /// Whether an extension mirrors the box's picture with UIKit's extension view (iOS 26) rather than painting the
    /// strip in its colour; tests clear it to drive the strip.
    nonisolated(unsafe) static var extensionMirrors: Bool = {
        if #available(iOS 26, tvOS 26, *) { return true }
        return false
    }()

    /// Ends the box's extension: its picture goes back to the box's own layer.
    func clearExtension() {
        guard let view = boxExtension?.view else { return }
        // An image's own picture layer draws again (`placeMirror`)
        if view is ExtensionView { imageLayer?.isHidden = false }
        rehostPicture { more.boxExtension = nil }
        view.removeFromSuperview()
        Self.extensions.removeObject(forKey: self)
        // The children's depths without the extension below them
        PaintOrder.changed(self)
        overflowClipGeometryChanged()
    }

    /// Keeps a layout transition's surface under the picture's layers that go back on the box while it paints, where
    /// `applySurface` puts it, as they lie without an extension.
    func keepSurfaceUnder() {
        if let s = surface, s.superlayer === layer, layer.sublayers?.first !== s { layer.insertSublayer(s, at: 0) }
    }

    /// The extension's rectangle in the box's coordinates, the border box outset by its bounds.
    var extensionFrame: CGRect? {
        guard let o = boxExtension?.outsets else { return nil }
        return laidOutBox.inset(by: UIEdgeInsets(top: -o.top, left: -o.left, bottom: -o.bottom, right: -o.right))
    }

    /// The extension's frames: the view's (`extensionFrame`), and the picture or the strip's hole at the border box;
    /// with `shown`, for a box of that size, as it shows before an animation moves it.
    func placeExtension(shown: CGSize? = nil) {
        guard let e = boxExtension, var frame = extensionFrame else { return }
        let laid = shown.map { CGRect(origin: .zero, size: $0) } ?? laidOutBox
        if shown != nil {
            let o = e.outsets
            frame = laid.inset(by: UIEdgeInsets(top: -o.top, left: -o.left, bottom: -o.bottom, right: -o.right))
        }
        let box = CGRect(x: e.outsets.left, y: e.outsets.top, width: laid.width, height: laid.height)
        if e.view.frame != frame {
            e.view.frame = frame
            // An ancestor that clips one axis fits its clip to what its children paint, the strip among it
            overflowClipGeometryChanged()
        }
        e.picture?.frame = box
        (e.view as? ExtensionStrip)?.place(around: box)
        if let mirror = e.view as? ExtensionView { placeMirror(mirror, box, shown: shown) }
    }

    /// An image's mirror (`ExtensionView`): its box at `box` drawing its picture as the image does (`applyImageLayer`),
    /// and the box mirrored about each edge the extension reaches past. `UIBackgroundExtensionView` moves its mirror on
    /// SwiftUI's clock, about two frames behind Core Animation's animation of the picture, so while a fold or a column
    /// resizes the picture a cut showed between the picture and its mirror; these layers are placed in the transaction
    /// that places the image, so one animation moves them all and the mirror meets the picture at every frame. The
    /// image's own picture layer stays hidden meanwhile, so no pixel at the edge is drawn twice.
    private func placeMirror(_ mirror: ExtensionView, _ box: CGRect, shown: CGSize? = nil) {
        // A layout transition's surface paints the box meanwhile, the image's own picture with it, the mirror hidden
        guard surface == nil, flightLook == nil, let image = raster?.image else { imageLayer?.isHidden = false; return }
        if imageLayer?.isHidden == false { imageLayer?.isHidden = true }
        // At `shown`, the content box keeps its insets from the border box
        let laid = contentBox(), edge = CGRect(origin: .zero, size: shown ?? bounds.size)
        let inner = shown == nil ? laid : CGRect(x: laid.minX, y: laid.minY, width: max(0, edge.width - laid.minX - (bounds.width - laid.maxX)),
                                                 height: max(0, edge.height - laid.minY - (bounds.height - laid.maxY)))
        let fit = style["object_fit"]?.string ?? "fill"
        let drawn = RasterGeometry.rect(natural: image.naturalSize, content: inner, fit: fit, position: style["object_position"]?.array)
            .offsetBy(dx: -inner.minX, dy: -inner.minY)
        let corners = cornerRadii(in: edge).contains { $0 > 0 } ? roundedPath(in: edge).cgPath : nil
        mirror.place(box, inner: inner, drawn: drawn, pixels: ImageLink.animated?.frame(for: self) ?? image.image,
                     tint: channels("tint_color").map { TextEngine.color($0).cgColor }, corners: corners)
    }

    /// Runs `change`, which starts or ends the extension, moving the picture layers to where the box paints its picture
    /// after it (`pictureHost`), its fill and corners with them.
    private func rehostPicture(_ change: () -> Void) {
        let before = pictureHost
        let moving = pictureLayers.filter { $0.superlayer === before }
        change()
        // A CSS animation of the background colour plays where the fill paints
        presenter?.boxAnimations.setFill(id, boxExtension?.picture?.layer, clock: presenter?.session?.clock)
        for l in moving { insertBoxSublayer(l) }
        keepSurfaceUnder()
        // The fill, and its `dynamic-range-limit`, on the layer that now paints it
        applyBoxLayer()
    }
}
extension UIView {
    /// Its subviews less its extension view, which the host adds behind a box's children and which no reader of what a
    /// box holds counts (LLP 1123 D13, `applyExtension`).
    package var heldSubviews: [UIView] {
        guard let mirror = NodeView.extensions.object(forKey: self)?.view else { return subviews }
        return subviews.filter { $0 !== mirror }
    }

    /// The fill its own picture paints: its layer's, or, while it extends, its extension picture's (LLP 1123 D13), as
    /// a lifted copy that leaves out the view's own fill takes it for its ground.
    package var ownFill: CGColor? {
        NodeView.extensions.object(forKey: self)?.picture?.layer.backgroundColor ?? layer.backgroundColor
    }

    /// Whether it is a box's extension view, which paints its own frame and holds UIKit's own views.
    package var isBoxExtension: Bool {
        superview.map { NodeView.extensions.object(forKey: $0)?.view === self } ?? false
    }
}
#endif
