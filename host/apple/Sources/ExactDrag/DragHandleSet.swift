// A presenter's transform and height handles (`DragHandles`): the bindings
// the batch's ops carry, keyed by handle, and the transform handles'
// geometry reports.
import ExactKit

final class DragHandleSet: DragHandles {
    var height: [UInt32: HeightDragBinding] = [:]
    var transform: [UInt32: TransformDragBinding] = [:]
    private(set) lazy var geometry = TransformGeometryObserver(presenter, self)
    unowned let presenter: Presenter
    init(_ presenter: Presenter) { self.presenter = presenter }

    func bind(_ kind: BatchOp.Kind, _ payload: [String: Any]) -> [UInt32] {
        if kind == .transformDrag, let binding = TransformDragBinding(payload) {
            if binding.target == nil {
                if transform[binding.id]?.handleKey == binding.handleKey && transform[binding.id]?.runtime == binding.runtime {
                    transform.removeValue(forKey: binding.id)
                    geometry.retire(binding.id)
                }
            } else { transform[binding.id] = binding }
            return [binding.id, binding.target, binding.clip].compactMap { $0 }
        }
        if kind == .heightDrag, let binding = HeightDragBinding(payload) {
            if binding.target == nil {
                if height[binding.id]?.handleKey == binding.handleKey { height.removeValue(forKey: binding.id) }
            } else { height[binding.id] = binding }
            return [binding.id, binding.target].compactMap { $0 }
        }
        return []
    }

    func retire(_ id: UInt32) {
        height.removeValue(forKey: id)
        transform.removeValue(forKey: id)
        geometry.retire(id)
    }

    func reset() {
        height.removeAll()
        transform.removeAll()
        geometry.reset()
    }

    func geometryChanged() { geometry.changed() }
    func handles(_ id: UInt32) -> Bool { transform[id]?.target != nil || height[id]?.target != nil }

    #if EXACT_AGENT
    func transformClip(_ id: UInt32) -> UInt32? { transform[id]?.clip }
    #endif
}

extension Presenter {
    /// The Drag module's handles: none on a presenter made while the module
    /// was not installed (a test that unlinks it).
    var handles: DragHandleSet? { drags as? DragHandleSet }
    var heightBindings: [UInt32: HeightDragBinding] { handles?.height ?? [:] }
    var transformBindings: [UInt32: TransformDragBinding] { handles?.transform ?? [:] }
}
