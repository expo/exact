// The SVG family (LLP 1047.001 D10, the roster's `svg`): an `svg`'s scene,
// SVG files as images and icons, and the island and filter modules,
// installed into `SvgLink` and `CoreExports` by an app whose composition
// links it. Its filters run on box effects' engine (`ExactEffects`), which
// the composition links with it.
import CExact.Svg
import ExactCore
import ExactKit

public enum ExactSvg {
    /// Set `SvgLink`'s slots and SVG's C export, before any session is made.
    public static func install() {
        SvgLink.make = { SvgHost() }
        SvgLink.documents = SvgFiles()
        SvgLink.prepare = SvgPrepare.scene
        SvgLink.prewarm = { svg in
            if svg & 1 != 0 { SvgRasterModule.prewarm() }
            #if os(iOS) || os(tvOS)
            if svg & 2 != 0 { BoxFilterLink.prewarm?() }
            #endif
        }
        SvgLink.drawing = { SvgFilterLive.inFlight }
        SvgLink.waitForDrawing = { SvgFilterLive.waitForDraws(timeout: $0) }
        CoreExports.svgIslands = exact_svg_islands
    }
}
