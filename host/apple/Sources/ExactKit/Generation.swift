import Foundation

/// Prepare every participant before deciding whether the generation can commit.
/// Native image loads must start together, including sessions after a waiter.
func prepareGeneration<Session, Candidate>(_ sessions: [Session], prepare: (Session) -> Candidate?) -> [(Session, Candidate)]? {
    var candidates: [(Session, Candidate)] = []
    var ready = true
    for session in sessions {
        if let candidate = prepare(session) { candidates.append((session, candidate)) }
        else { ready = false }
    }
    return ready ? candidates : nil
}

/// A platform request's weak session owner and the runtime lease it asked
/// under. Validation shares the posted-job door (LLP 1172.005 E11).
package struct SessionLease {
    private weak var owner: ExactSession?
    private let runtime: RuntimeLease
    var generation: Int { runtime.generation }
    package init(_ session: ExactSession) { owner = session; runtime = session.runtime.completionLease }
    package var session: ExactSession? {
        guard let owner, owner.runtime.accepts(runtime) else { return nil }
        return owner
    }
}

extension ExactSession {
    struct Prepared {
        let text: TextEngine
        let resolver: AssetResolver
        let token: UInt64
        var assetChanges: [UInt32: Set<String>] = [:]
        var displacedCompositions: Set<ObjectIdentifier> = []
    }

    /// A reused view can name new bytes without changing any props. Validate
    /// those names before committing, alongside the names in the batch.
    func prepareAssets(_ batch: Batch, resolver: AssetResolver) -> [UInt32: Set<String>] {
        let properties: Set<String> = ["src", "imageSource", "poster", "mediaArtwork"]
        func asset(_ source: String) -> String? {
            guard URL(string: source)?.scheme == nil, !source.hasPrefix("//") else { return nil }
            let path = sourceWithoutQuery(source)
            let name = path.hasPrefix("/") ? String(path.dropFirst()) : path
            return name.isEmpty ? nil : name
        }
        var inherited: [UInt32: [String: String]] = [:]
        if batch.replacesTree && !resolver.hasSameAssets(as: app.resolver) {
            for (id, view) in presenter.views {
                for (key, source) in view.props where properties.contains(key) {
                    if let name = asset(source) { inherited[id, default: [:]][key] = name }
                }
            }
        }
        for op in batch.ops {
            switch op.op {
            case .create, .destroy: inherited[op.id] = nil
            case .props:
                for key in op.clear { inherited[op.id]?[key] = nil }
            default: continue
            }
            for (key, source) in op.props where properties.contains(key) {
                inherited[op.id]?[key] = nil
                if let name = asset(source) { _ = resolver.bytes(name) }
            }
        }
        var changed: [UInt32: Set<String>] = [:]
        for (id, sources) in inherited {
            for (key, name) in sources {
                let bytes = resolver.bytes(name)
                // Documents may refer to other files in the new publication.
                if presenter.views[id]?.kind == "iframe" || !app.resolver.hasSameAsset(name, as: resolver, bytes: bytes) {
                    changed[id, default: []].insert(key)
                }
            }
        }
        return changed
    }

    func reloadAssets(_ changed: [UInt32: Set<String>]) {
        for (id, properties) in changed {
            guard let view = presenter.views[id] else { continue }
            if view.kind == "image", let source = view.props["imageSource"] { view.loadImage(source) }
            if view.kind == "iframe" { webviews.reloadDocument(view) }
            view.video?.reloadAssets(properties)
        }
    }
}
