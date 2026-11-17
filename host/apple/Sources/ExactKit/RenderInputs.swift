// What `CALayer.render(in:)` draws a layer tree from: every property of the
// model layers that it reads, and their contents. It draws from nothing
// else, so a tree whose inputs are equal draws the same pixels again,
// whatever changed in it meanwhile and whether or not an op said so (a
// native scroll, a decode, an animated image's frame, a re-aimed gradient).
// `BoxFilter` keeps its last drawing while they are equal.
//
// A layer that draws itself (`draw(in:)`: a drawn surface, a recorded
// canvas) is no exception. `render(in:)` displays one that waits for
// display before drawing it, and a display's contents are Core Animation's
// own buffer, not a `CGImage`, so such a tree has no inputs (below). One
// that does not wait is drawn from the contents it has, without
// `draw(in:)`. (Both measured on iOS 27, the first also on macOS 27.)
// Marking a tree by an override of `draw(in:)` instead would mark every
// tree on the Mac: AppKit's `NSViewBackingLayer`, each view's layer, has one.
//
// Some inputs cannot be compared: contents that change in place, keeping
// their identity (a view's backing store, measured on iOS 27; an IOSurface,
// which a writer can reuse, as the live SVG filter does), and Core Animation
// classes that draw from properties of their own beyond the shape's and the
// gradient's (text, replicator, emitter, tiled, Metal, backdrop layers). A
// tree holding one has no inputs (`init` fails), and is drawn whenever a
// batch touches it.
import CoreGraphics
import Foundation
import QuartzCore

struct RenderInputs: Equatable {
    private var numbers: [CGFloat] = []
    /// Layers and contents, compared by identity; colours, paths, arrays and
    /// names, by value (`isEqual`, which is `CFEqual` for a colour or path).
    private var objects: [AnyObject?] = []

    /// `root`'s inputs, without its own position, anchor, depth, transform,
    /// opacity and mask: a drawing of it in its own bounds space reads none
    /// of them (a filter's picture mirrors them instead).
    init?(_ root: CALayer) {
        numbers.reserveCapacity(512)
        objects.reserveCapacity(128)
        guard add(root, root: true) else { return nil }
    }

    static func == (a: Self, b: Self) -> Bool {
        a.numbers == b.numbers && a.objects.count == b.objects.count
            && zip(a.objects, b.objects).allSatisfy { $0 === $1 || ($0 as? NSObject)?.isEqual($1) == true }
    }

    private mutating func add(_ layer: CALayer, root: Bool = false) -> Bool {
        guard let kind = Self.kind(of: layer) else { return false }
        if let contents = layer.contents {
            guard CFGetTypeID(contents as CFTypeRef) == CGImage.typeID else { return false }
        }
        objects.append(layer)
        numbers.append(layer.isHidden ? 1 : 0)
        // Nothing of a hidden layer is drawn.
        if layer.isHidden { return true }
        objects.append(layer.contents as AnyObject?)
        if !root {
            numbers += [layer.position.x, layer.position.y, layer.anchorPoint.x, layer.anchorPoint.y, layer.anchorPointZ,
                        layer.zPosition, CGFloat(layer.opacity)]
            add(layer.transform)
        }
        let b = layer.bounds, r = layer.contentsRect, c = layer.contentsCenter
        numbers += [b.minX, b.minY, b.width, b.height, r.minX, r.minY, r.width, r.height, c.minX, c.minY, c.width, c.height,
                    layer.contentsScale, CGFloat(layer.minificationFilterBias), layer.cornerRadius, CGFloat(layer.maskedCorners.rawValue),
                    layer.borderWidth, CGFloat(layer.shadowOpacity), layer.shadowOffset.width, layer.shadowOffset.height, layer.shadowRadius,
                    CGFloat(layer.edgeAntialiasingMask.rawValue), layer.allowsEdgeAntialiasing ? 1 : 0, layer.allowsGroupOpacity ? 1 : 0,
                    layer.masksToBounds ? 1 : 0, layer.isDoubleSided ? 1 : 0, layer.isGeometryFlipped ? 1 : 0]
        add(layer.sublayerTransform)
        objects += [layer.contentsGravity.rawValue as NSString, layer.minificationFilter.rawValue as NSString,
                    layer.magnificationFilter.rawValue as NSString, layer.cornerCurve.rawValue as NSString,
                    layer.backgroundColor, layer.borderColor, layer.shadowColor, layer.shadowPath,
                    layer.compositingFilter.map { $0 as AnyObject }, layer.filters as NSArray?]
        switch kind {
        case .plain: break
        case .shape:
            let s = layer as! CAShapeLayer
            numbers += [s.strokeStart, s.strokeEnd, s.lineWidth, s.miterLimit, s.lineDashPhase]
            objects += [s.path, s.fillColor, s.strokeColor, s.fillRule.rawValue as NSString, s.lineCap.rawValue as NSString,
                        s.lineJoin.rawValue as NSString, s.lineDashPattern as NSArray?]
        case .gradient:
            let g = layer as! CAGradientLayer
            numbers += [g.startPoint.x, g.startPoint.y, g.endPoint.x, g.endPoint.y]
            objects += [g.colors as NSArray?, g.locations as NSArray?, g.type.rawValue as NSString]
        }
        // The root's mask is the filter's own: what hides the box, or its clip, which the picture takes.
        if !root, let mask = layer.mask {
            numbers.append(1)
            guard add(mask) else { return false }
        } else {
            numbers.append(0)
        }
        let sublayers = layer.sublayers ?? []
        numbers.append(CGFloat(sublayers.count))
        for sublayer in sublayers { guard add(sublayer) else { return false } }
        return true
    }

    private mutating func add(_ t: CATransform3D) {
        numbers += [t.m11, t.m12, t.m13, t.m14, t.m21, t.m22, t.m23, t.m24, t.m31, t.m32, t.m33, t.m34, t.m41, t.m42, t.m43, t.m44]
    }

    /// What a layer draws from besides `CALayer`'s own properties: nothing,
    /// a shape's or a gradient's, or what is not compared (nil). A subclass
    /// from outside Core Animation (UIKit's, AppKit's, this host's) draws as
    /// the Core Animation class it is built on.
    private enum Kind { case plain, shape, gradient }
    nonisolated(unsafe) private static var kinds: [ObjectIdentifier: Kind?] = [:]
    private static let quartz = Bundle(for: CALayer.self)
    private static func kind(of layer: CALayer) -> Kind? {
        let type: AnyClass = Swift.type(of: layer)
        if let known = kinds[ObjectIdentifier(type)] { return known }
        var base: AnyClass = type
        while Bundle(for: base) != quartz, let next = class_getSuperclass(base) { base = next }
        let kind: Kind? = switch ObjectIdentifier(base) {
        case ObjectIdentifier(CALayer.self), ObjectIdentifier(CAScrollLayer.self), ObjectIdentifier(CATransformLayer.self): .plain
        case ObjectIdentifier(CAShapeLayer.self): .shape
        case ObjectIdentifier(CAGradientLayer.self): .gradient
        default: nil
        }
        kinds[ObjectIdentifier(type)] = kind
        return kind
    }
}
