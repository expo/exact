// The images family (LLP 1047.001 D10, the roster's `Images`): reading,
// decoding and showing image files, animated images and `image-rendering:
// pixelated`. An app's composition links this module when its plan has an
// image source or an icon in text, and installs it before any session is
// made; the core reaches it only through `ImageLink`.
import Foundation
import ExactKit

public enum ExactImages {
    public static func install() {
        ImageLink.makeLoader = { RasterLoader() }
        ImageLink.animated = AnimatedRasters.shared
        ImageLink.pixelated = PixelatedSampling()
        ImageLink.travelling = { RasterWorkers.shared.travelling($0, $1) }
        ImageLink.declineDelays = RasterLoader.declineDelays
        ImageLink.readIcon = { source, resolver in
            let bytes: Data
            if source.hasPrefix("data:") {
                guard source.utf8.count <= RasterInput.dataLimit, let data = RasterInput.dataURL(source) else {
                    throw ImageLink.IconFailure.data(limit: RasterInput.dataLimit)
                }
                bytes = data
            } else {
                do { bytes = try RasterInput.open(source, resolver: resolver!).bytes() } catch { throw ImageLink.IconFailure.read(error) }
            }
            return (bytes, try RasterMetadata.read(prefix: bytes.prefix(RasterMetadata.headerLimit), encodedBytes: bytes.count).svg)
        }
    }
}
