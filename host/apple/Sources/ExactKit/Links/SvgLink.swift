// @ref LLP 1047.001 D10 — SVG is a family the composition selects: an
// `svg`'s scene, an SVG document an image or an icon draws, and the island
// and filter modules. The core reaches it only through what this file
// declares, and `ExactSvg.install()` sets its slots; without it no file reads
// as SVG, and a plan with an `svg` is refused at boot (`Unlinked("effects, svg")`). Box
// animations are the core's (`BoxAnimations`). SVG's filters run on box
// effects' filter engine (`BoxFilterLink`), so the SVG product depends on
// that one, never the other way, and a composition that links SVG links it.
import CoreGraphics
import CoreText
import Foundation
import QuartzCore

/// What the core asks of a presenter's SVG scenes. Without SVG the presenter
/// holds none, and a plan with an `svg` is refused before it boots (D5).
package protocol SvgScenes: AnyObject {
    /// The session's fonts for SVG text; nil where a face is not known
    /// answers the system font.
    func useFonts(_ session: @escaping (CGFloat, Int, Int, Bool) -> CTFont?)
    /// An `svg` op: `id`'s scene in `layer`.
    func scene(_ id: UInt32, _ payload: [String: Any], layer: CALayer?, dark: Bool, clock: Double?, limit: String?, tint: PlatformColor?)
    /// `id`'s scene at its view's content box.
    func place(_ id: UInt32, in box: CGRect)
    /// `id`'s view changed appearance.
    func reappear(_ id: UInt32, dark: Bool, clock: Double?, tint: PlatformColor?)
    /// The page zoom or the display's scale changed.
    func rescale(clock: Double?)
    /// The agent's clock moved.
    func seek(clock: Double?)
    /// The SVG element a press at `point` in `id`'s view goes to.
    func target(_ id: UInt32, at point: CGPoint) -> UInt32?
    /// `id`'s view is gone.
    func forget(_ id: UInt32)
}

/// What the core asks of SVG documents: an `image`'s file or an icon's.
/// Without SVG, no file reads as one. Raster workers and the icon queue call
/// it concurrently, off the main thread, so an implementation keeps no
/// mutable state; a scene with text is drawn on the main thread, which the
/// caller arranges.
package protocol SvgDocuments: AnyObject {
    /// The natural size of the SVG document `prefix` starts, or nil when it
    /// is not one.
    func size(prefix: Data) -> CGSize?
    /// `bytes` resolved at `width` × `height` CSS pixels: the scene's JSON
    /// and its flags (1: it has text; 2: the JSON is the refusal's message),
    /// or nil when no scene was made.
    func scene(_ bytes: Data, width: Double, height: Double) -> (json: Data, flags: UInt32)?
    /// A scene's elements drawn into `width` × `height` pixels at `scale`,
    /// flipped, not dark. `fonts` is `.none` for the system font, and
    /// otherwise the document's font stacks (its `fonts` value, which may be
    /// absent).
    func draw(_ elements: [Any], rect: CGRect, width: Int, height: Int, scale: CGFloat, transform: CGAffineTransform, fonts: Any??) -> CGContext?
}

/// The slots are set before any session is made, and read from any thread.
package enum SvgLink {
    /// A presenter's scenes.
    package static var make: (() -> any SvgScenes)?
    /// SVG documents.
    package static var documents: (any SvgDocuments)?
    /// An `svg` op's scene prepared on the owner as the batch is decoded.
    package static var prepare: ((inout [String: Any]) -> Void)?
    /// A plan that can show an island (bit 1) or a filter (bit 2) opens their
    /// modules off the main thread before its first draw needs them.
    package static var prewarm: ((UInt8) -> Void)?
    /// How many SVG pictures are still drawing, which a settle waits for.
    package static var drawing: (() -> Int)?
    /// On the main thread: wait at most the interval for those pictures to
    /// show.
    package static var waitForDrawing: ((TimeInterval) -> Void)?
    /// A scene's root layer's name, by which its node re-asks its range
    /// (LLP 1100 D8).
    package static let rootName = "exact-svg"
}
