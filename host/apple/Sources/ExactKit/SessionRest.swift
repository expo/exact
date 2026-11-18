import Foundation

extension ExactSession {
    /// Scrolling has settled (iOS: `ScrollPump.restDelay`): cold pictures
    /// keep a bounded cache, cold shaped text and text pixels go, and the
    /// tree's storage comes back to its live nodes. The allocator returns
    /// the pages they leave.
    func rest() {
        runtime.trim()
        rasters?.trimCold(keeping: nil)
        text.dropColdShaped()
        if let m = text.measurer { Owner.shared.post { m.dropColdShaped() } }
        #if os(iOS) || os(tvOS)
        presenter.textRasters.dropKept()
        #endif
        DispatchQueue.global(qos: .utility).async { malloc_zone_pressure_relief(nil, 0) }
    }

}
