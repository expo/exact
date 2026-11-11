// PROTO (UIKit-first measurement): safe-area modes and cover timing; not landing code
import Darwin
import Foundation
import QuartzCore

extension ExactSession {
    /// PROTO (UIKit-first measurement): how a route's content follows an edge UIKit moves.
    /// `resting` lays it out with the bar at its full height and places the boxes that depend on
    /// the edge between layouts computed ahead (the far pass). `live` lays it out in UIKit's live
    /// safe area on every change, through the general batch pipeline. `geometry` is `live` with
    /// geometry-only batches applying frames and reach; other work waits for the frame after the
    /// edge stops (`GeometryOnlyIOS.swift`).
    package enum ProtoMode: String { case resting, live, geometry }
    /// PROTO (UIKit-first measurement): the mode from `ExactProtoMode`, or `ExactProtoLive` for
    /// `live`, switched by an app's `data-safe-area-mode` on its root (Duo Lab's Edges)
    nonisolated(unsafe) package static var protoMode = ProtoMode(rawValue: UserDefaults.standard.string(forKey: "ExactProtoMode") ?? "")
        ?? (UserDefaults.standard.bool(forKey: "ExactProtoLive") ? .live : .resting)
    /// PROTO (UIKit-first measurement): whether layout follows UIKit's live safe area
    package static var protoLive: Bool { protoMode != .resting }

    /// PROTO (UIKit-first measurement): a cover update's runtime and apply CPU time, wall time,
    /// and wait for in-flight work, measured after the session becomes idle
    func applyMeasuredCovers(_ covers: [(UInt32, HostCover?)], owners: [(UInt32, OwnerValues?)]) {
        let asked = CACurrentMediaTime(), deferred = fillInFlight
        whenIdle(bound: true) { [unowned self] in
            let start = CACurrentMediaTime(), startCPU = Self.prototypeCPU()
            let batch = runtime.covers(covers, owners: owners)
            let ranCPU = Self.prototypeCPU()
            applyUnlessEmpty(batch)
            guard Self.protoLive else { return }
            #if os(iOS) || os(tvOS)
            let phases = ApplyPhases.take()
            #else
            let phases = ""
            #endif
            NSLog("PROTO live covers cpu %.0f µs runtime %.0f apply %.0f wall %.0f ops %d frames %d waited %.0f µs%@ · %@", Self.prototypeCPU() - startCPU, ranCPU - startCPU, Self.prototypeCPU() - ranCPU, (CACurrentMediaTime() - start) * 1e6, batch.ops.count, batch.ops.filter { $0.op == .frame }.count, (start - asked) * 1e6, deferred ? " fill-in-flight" : "", phases)
        }
    }

    /// PROTO (UIKit-first measurement): the main thread's CPU time in microseconds, which
    /// preemption on a loaded machine does not inflate
    static func prototypeCPU() -> Double { Double(clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)) / 1000 }
}

#if os(iOS) || os(tvOS)
/// PROTO (UIKit-first measurement): the main thread's CPU time in microseconds in four apply
/// phases since the last live cover report read and reset them: operations, regrid and flush,
/// reach and placement, and navigation synchronization
enum ApplyPhases {
    static var ops = 0.0, regrid = 0.0, reach = 0.0, navigation = 0.0
    static func cpu() -> Double { ExactSession.prototypeCPU() }
    static func take() -> String {
        defer { ops = 0; regrid = 0; reach = 0; navigation = 0 }
        return String(format: "ops %.0f regrid %.0f reach %.0f navigation %.0f", ops, regrid, reach, navigation)
    }
}
#endif
