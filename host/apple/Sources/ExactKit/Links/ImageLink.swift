// @ref LLP 1047.001 D10 — images are a family the composition selects (the
// roster's `Images`): reading and decoding an `image`'s or a box's
// `background-image` source, the session's loader and budget, animated
// GIF/WebP frames, and `image-rendering: pixelated`. The core reaches it only
// through what this file declares, and `ExactImages.install()` fills its
// slots: a composition without the family leaves them empty, and a plan with
// an image is refused at boot (`Unlinked("images")`). Placing a bitmap in its
// box (`RasterGeometry`) is the core's. Icons in text read through
// `readIcon`, which the roster's `images` rule covers; Surfaces' Canvas 2D
// images fetch and read through the family's decoders, so `ExactSurfaces`
// depends on `ExactImages`.
import CoreGraphics
import Foundation
import QuartzCore

/// A decoded bitmap a view draws.
package protocol ImageBitmap: AnyObject {
    var image: CGImage { get }
    var naturalSize: CGSize { get }
    /// Its headroom over SDR white, else 0.
    var headroom: Float { get }
    var isHDR: Bool { get }
}

/// A view's hold on a decoded bitmap: while a view holds it, the loader
/// keeps its pixels.
package protocol ImageLease: AnyObject {
    /// The lease's identity: a new decode is a new lease.
    var id: UInt64 { get }
    var image: any ImageBitmap { get }
}

/// What the core asks of a session's image loader.
package protocol ImageLoader: AnyObject {
    /// Ask for `view`'s source; false when it was refused at once.
    @discardableResult func load(_ view: NodeView, source: String, resolver: AssetResolver) -> Bool
    /// `view` no longer wants its source.
    func cancel(_ view: UInt32)
    /// `view`'s size or rendering changed: its decode may need another size.
    func resized(_ view: NodeView)
    /// `view`'s dynamic range limit changed.
    func rangeChanged(_ view: NodeView)
    /// The display's scale or colour space changed.
    func displayChanged()
    /// The viewport's size and scale, which set the decode budget.
    func fit(size: CGSize, scale: CGFloat)
    func setPaused(_ paused: Bool)
    /// A source's bytes changed (a dev-loop asset edit).
    func invalidate(_ source: String)
    /// The plan restarted.
    func reset()
    /// The first batch was applied.
    func bootPass()
    var hasRefusedPictures: Bool { get }
    /// A batch ended: pictures it held back may be asked again.
    func readmitPictures()
    /// A bar started drawing an image in a hidden header.
    func visibilityChanged()
    /// The session ends.
    func shutdown()
    /// Frame health's sample of blank images on screen.
    func sampleBlank()
    /// Drop cold decodes down to `keeping` bytes, or the loader's own share.
    func trimCold(keeping bytes: UInt64?)
    /// Whether an image under `root` is still loading.
    func loading(inside root: NodeView) -> Bool
    /// How many images on screen are still loading (the agent's settle).
    var loadingOnScreen: Int { get }
    /// The loader's state, for the agent.
    var diagnostics: [String: Any] { get }
    /// The HTTP cache's use for encoded images, for the agent.
    var encodedHTTPCache: Any { get }
    /// A native view's picture source changed, or was revoked.
    func pictureChanged(_ source: String)
    func revokePicture(_ source: String)
}

/// What the core asks of animated images' players.
package protocol AnimatedImages: AnyObject {
    /// `view` took a new raster: an animated one gets a player.
    func attach(_ view: NodeView)
    /// The frame `view` shows now, or nil for its raster's own pixels.
    func frame(for view: NodeView) -> CGImage?
    /// Something moved: look again at the next turn.
    func poke()
    /// Show every player's frame at the session clock now (the agent's clock).
    func evaluate()
    /// The players' state, for the agent.
    var diagnostics: [String: Any] { get }
}

/// What the core asks of `image-rendering: pixelated`.
package protocol PixelatedImaging: AnyObject {
    /// Whether `view`'s two-pass sampling owns its image layer.
    func apply(_ view: NodeView) -> Bool
    /// Whether `layer` is a pixelated image's.
    func owns(_ layer: CALayer?) -> Bool
    /// `view`'s and its descendants' tiles, after its transform changed.
    func refresh(under view: NodeView)
    /// Every pixelated image's tiles of `presenter`, after a batch or a
    /// scroll moved or resized what clips it.
    func refresh(in presenter: Presenter)
}

/// The slots are set before any session is made. They are read on the main
/// thread, but for `readIcon` and `declineDelays`, which icons in text read on
/// their own queue.
package enum ImageLink {
    /// A session's loader.
    package static var makeLoader: (() -> any ImageLoader)?
    /// Animated images' players.
    package static var animated: (any AnimatedImages)?
    /// Pixelated sampling.
    package static var pixelated: (any PixelatedImaging)?
    /// A scroller moving fast (`true`) or no longer: decode workers yield to it.
    package static var travelling: ((AnyObject, Bool) -> Void)?
    /// An icon in text (`AtomIcons`): `source` read as an `image` reads it,
    /// and whether its bytes are an SVG's. It throws `IconFailure` for a
    /// source it could not read, and the decoder's error for bytes it could
    /// not tell.
    package static var readIcon: ((_ source: String, _ resolver: AssetResolver?) throws -> (bytes: Data, svg: Bool))?
    /// The waits, in seconds, before a decode the system declined is asked
    /// again.
    package static var declineDelays: [Double] = []

    package enum IconFailure: Error {
        /// A `data:` URL over `limit` bytes, or not one.
        case data(limit: Int)
        case read(any Error)
    }
}

/// Whether `source` names a scheme a package may serve: never one a host
/// owns (`HOST_SCHEMES`, contract/cli/src/declaration.rs).
package func packageMayServe(_ source: String) -> Bool {
    guard let colon = source.firstIndex(of: ":") else { return false }
    let scheme = source[..<colon]
    guard let first = scheme.first, first.isLetter, first.isLowercase,
          scheme.allSatisfy({ ($0.isLetter && $0.isLowercase) || $0.isNumber || $0 == "-" }) else { return false }
    return !["app", "doc", "symbol", "data", "http", "https", "blob", "file", "asset"].contains(String(scheme))
}

/// `source` with no query or fragment, as a log names it.
package func sourceWithoutQuery(_ source: String) -> String {
    source.unicodeScalars.firstIndex(where: { $0 == "?" || $0 == "#" }).map { String(source.unicodeScalars[..<$0]) } ?? source
}

extension ExactSession {
    /// The app module's `image` entry and its instance, for a package's
    /// pictures; nil for an artifact without one. Main thread.
    package func packageImageEntry() -> (UnsafeMutableRawPointer, PackageImageFn)? { natives.imageEntry() }
}
