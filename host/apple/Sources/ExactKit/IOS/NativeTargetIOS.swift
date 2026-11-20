#if os(iOS)
import UIKit

/// A native view whose raw safe area a box reads by name (LLP 1123 D9, "Registration"):
/// `var(--exact-target-<name>-safe-area-inset-<edge>)`. A registration holds its view weakly and
/// reports the view's insets as they are at `register` and `replace(view:)`, none while the view
/// is out of a window or gone; a later change reaches the kernel once publication reports it
/// (stage 5). Its names read their fallbacks once it is disposed of, which releasing it does too.
/// Main thread only.
package final class ExactNativeTarget {
    /// Why a registration is refused.
    package enum Refusal: Error, Equatable, CustomStringConvertible {
        /// D9's names take ASCII lowercase letters, digits and inner hyphens, at most 64.
        case invalidName(String)
        /// A live registration holds the name; replace its view or dispose of it first.
        case taken(String)

        package var description: String {
            switch self {
            case let .invalidName(name):
                "`\(name)` is not a target name: use ASCII lowercase letters, digits and inner hyphens, at most 64"
            case let .taken(name):
                "a live registration holds `\(name)`: replace its view, or dispose of it before registering another"
            }
        }
    }

    package let name: String
    /// The session, held weakly in a box a release can carry to the main thread without taking a
    /// strong reference to the session off it.
    private let owner: WeakSession
    private var session: ExactSession? { owner.session }
    private weak var view: UIView?
    /// The view's identity, kept for when the view is gone (`TargetIds`).
    private var id: UInt64
    private var generation: UInt64
    private var disposed = false

    /// Register `view` under `name` for `session`.
    package static func register(name: String, view: UIView, in session: ExactSession) throws -> ExactNativeTarget {
        guard valid(name) else { throw Refusal.invalidName(name) }
        guard live(name, in: session) == nil else { throw Refusal.taken(name) }
        let target = ExactNativeTarget(name: name, view: view, session: session)
        session.nativeTargets.setObject(target, forKey: name as NSString)
        report(name, in: session)
        return target
    }

    /// Register `controller`'s view under `name` for `session`, loading the view; the registration
    /// follows that view, not the controller.
    package static func register(name: String, controller: UIViewController, in session: ExactSession) throws -> ExactNativeTarget {
        try register(name: name, view: controller.view, in: session)
    }

    private init(name: String, view: UIView, session: ExactSession) {
        self.name = name
        self.view = view
        owner = WeakSession(session)
        id = TargetIds.id(of: view)
        generation = Self.nextGeneration(in: session)
    }

    deinit {
        guard !disposed else { return }
        // Released: the registry's weak entry for this token is gone, so the name reports disposed. The
        // session is read on the main thread only, through the box.
        let (name, owner) = (name, owner)
        let forget = {
            guard let session = owner.session else { return }
            if Self.live(name, in: session) == nil { session.nativeTargets.removeObject(forKey: name as NSString) }
            Self.report(name, in: session)
        }
        if Thread.isMainThread { forget() } else { DispatchQueue.main.async(execute: forget) }
    }

    /// Give the registration another view: its generation rises, so no box takes the old
    /// view's insets for the new one's.
    package func replace(view: UIView) {
        guard !disposed, let session else { return }
        self.view = view
        id = TargetIds.id(of: view)
        generation = Self.nextGeneration(in: session)
        Self.report(name, in: session)
    }

    /// Remove the registration, its names reading their fallbacks from the next commit.
    package func dispose() {
        guard !disposed else { return }
        disposed = true
        guard let session else { return }
        if Self.live(name, in: session) === self {
            session.nativeTargets.removeObject(forKey: name as NSString)
        }
        Self.report(name, in: session)
    }

    /// The target as the kernel takes it now: the view's insets, none while it is out of a window
    /// or gone, under the identity and generation the registration holds.
    private var current: OwnerUpdateIOS.Target {
        var target = OwnerUpdateIOS.target(view)
        target.id = id
        target.generation = generation
        return target
    }

    /// A generation no registration in `session` had: one counter for every name, so a name's
    /// generation never falls, whichever token holds it (the kernel refuses an older one, and
    /// another view at the same one).
    private static func nextGeneration(in session: ExactSession) -> UInt64 {
        session.nativeTargetGeneration += 1
        return session.nativeTargetGeneration
    }

    /// Tell `session`'s kernel what `name` is when the session is idle, read then: the live
    /// registration's target, or a disposal. A report the owner answers `busy` asks again on the
    /// next turn. A registration is the session's, so the work is not bound to the views a reset
    /// ends; a restart's new kernel is told every live registration (`reportAll`).
    private static func report(_ name: String, in session: ExactSession) {
        session.whenIdle { [weak session] in
            // Between a restart's reset and its tree the new kernel is told every live registration
            // as the tree comes in (`treeIsIn`), so a report then would only land before the tree
            guard let session, session.state != .destroyed, !session.awaitingTree else { return }
            // The token is held until its report is applied, so a release elsewhere cannot run its
            // disposal inside this report and send it first
            let live = live(name, in: session)
            defer { withExtendedLifetime(live) {} }
            let batch = session.runtime.ownerReports(registrations: [encode(name: name, target: live?.current)])
            if batch.busy {
                DispatchQueue.main.async { [weak session] in
                    if let session { report(name, in: session) }
                }
                return
            }
            session.applyUnlessEmpty(batch)
        }
    }

    /// Tell `session`'s kernel every live registration, as a new tree comes in after a boot or a
    /// restart, whose new kernel holds none.
    static func reportAll(in session: ExactSession) {
        let names = autoreleasepool { (session.nativeTargets.keyEnumerator().allObjects as? [NSString] ?? []).map { $0 as String } }
        for name in names.sorted() where live(name, in: session) != nil {
            report(name, in: session)
        }
    }

    /// The live registration holding `name` in `session`. The registry hands its object back
    /// autoreleased; read in a pool of its own, the read keeps no token alive past it, so a
    /// released token frees its name at once.
    private static func live(_ name: String, in session: ExactSession) -> ExactNativeTarget? {
        autoreleasepool { session.nativeTargets.object(forKey: name as NSString) as? ExactNativeTarget }
    }

    /// A registration in the kernel's wire form (`encode_registration`): the name's length
    /// and bytes, then the target after a presence byte.
    static func encode(name: String, target: OwnerUpdateIOS.Target?) -> Data {
        var out = Data([UInt8(name.utf8.count)])
        out.append(contentsOf: name.utf8)
        out.append(target == nil ? 0 : 1)
        if let target { out.append(OwnerUpdateIOS.encode(target: target)) }
        return out
    }

    /// Whether D9 takes `name`, byte by byte as the kernel checks it: ASCII lowercase letters,
    /// digits and inner hyphens, at most 64.
    static func valid(_ name: String) -> Bool {
        let bytes = Array(name.utf8), hyphen = UInt8(ascii: "-")
        let inner = bytes.allSatisfy { (97...122).contains($0) || (48...57).contains($0) || $0 == hyphen }
        return !bytes.isEmpty && bytes.count <= 64 && inner && bytes.first != hyphen && bytes.last != hyphen
    }
}
#endif
