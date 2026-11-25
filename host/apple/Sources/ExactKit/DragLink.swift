// @ref LLP 1047.001 D4 — drags (height and transform handles) are a linked
// capability: their holds and gestures are the `ExactDrag` module, which an
// app's composition links only when its plan has a drag, and installs here.
// The core holds them through these protocols; without the module a node
// never takes a drag gesture, and a plan that has one is refused at boot
// (`Unlinked("drag")`). The module keeps a presenter's handles too
// (`DragHandles`): their bindings, their geometry reports and the packets
// the runtime reads. A list's reorder is the lists family's (`ReorderLink`).
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// What the core asks of the capability.
package protocol DragCapability: AnyObject {
    /// A presenter's transform and height handles.
    func handles(_ presenter: Presenter) -> DragHandles
    #if os(macOS)
    /// A presenter's mouse drags.
    func mouseDrags(_ presenter: Presenter) -> MouseDrags
    #else
    /// A node's transform drag, after its binding changed.
    func updateTransformGesture(_ view: NodeView)
    /// A node's height drag, after its binding changed.
    func updateHeightGesture(_ view: NodeView)
    /// Whether a drag's recognizer on `view` begins; nil when `gesture` is
    /// none of the drags'.
    func shouldBegin(_ view: NodeView, _ gesture: UIGestureRecognizer) -> Bool?
    /// The agent's pinch on a transform handle: why it was refused, or nil.
    func recognizedPinch(_ handle: NodeView, scale: Double, focal: CGPoint) -> String?
    #endif
}

/// A presenter's transform and height handles, bound by the batch's
/// `transformDrag` and `heightDrag` ops.
package protocol DragHandles: AnyObject {
    /// A `transformDrag` or `heightDrag` op: the nodes it names, its handle
    /// first, or none when it binds nothing.
    func bind(_ kind: BatchOp.Kind, _ payload: [String: Any]) -> [UInt32]
    func retire(_ id: UInt32)
    func reset()
    /// What may have moved a transform handle's geometry did.
    func geometryChanged()
    #if EXACT_AGENT
    /// The clip of `id`'s transform handle, if it is one.
    func transformClip(_ id: UInt32) -> UInt32?
    #endif
}

/// An app without the capability: no node is a handle.
final class NoDragHandles: DragHandles {
    func bind(_ kind: BatchOp.Kind, _ payload: [String: Any]) -> [UInt32] { [] }
    func retire(_ id: UInt32) {}
    func reset() {}
    func geometryChanged() {}
    #if EXACT_AGENT
    func transformClip(_ id: UInt32) -> UInt32? { nil }
    #endif
}

#if os(iOS)
/// The height recognizer on a bound scroller shares its one vertical pan with UIKit.
package protocol SheetScrollGesture: AnyObject {
    func receive(_ touch: UITouch, in scroll: UIScrollView) -> Bool
    func takesEdge(in scroll: UIScrollView, direction: CGPoint) -> Bool
    func correctOffset(in scroll: UIScrollView) -> Bool
    func listDrag(in scroll: UIScrollView) -> (start: CGFloat, atStart: Bool)?
}
#endif

/// A drag the session holds while its input lasts.
package protocol DragInput: AnyObject {
    func cancel()
    func cancelIfInputIneligible()
    /// A transform hold's motion the runtime retired.
    func retire(runtime: UInt64, token: UInt64)
}

extension DragInput {
    package func retire(runtime: UInt64, token: UInt64) {}
}

/// The Drag module, installed by the composition.
package enum DragLink {
    package static var installed: DragCapability?
}

#if os(macOS)
/// One of a presenter's mouse drags.
package protocol MouseDrag: MouseRecognizer {
    func retire(_ id: UInt32)
}

/// The mouse transform drag, which also takes a wheel and a magnify.
package protocol MouseTransform: MouseDrag {
    func scroll(_ node: NodeView, event: NSEvent) -> Bool
    func magnify(_ node: NodeView, event: NSEvent) -> Bool
}

/// A presenter's two mouse drags, in the chain's order (the reorder's is
/// `ReorderHosting.mouse`).
package struct MouseDrags {
    package let transform: MouseTransform
    package let height: MouseDrag
    package init(transform: MouseTransform, height: MouseDrag) {
        self.transform = transform; self.height = height
    }
    /// An app without the capability: no mouse drag arms.
    static let none = MouseDrags(transform: NoMouseDrag(), height: NoMouseDrag())
}

/// A mouse drag that never arms.
package final class NoMouseDrag: MouseTransform {
    package init() {}
    package var armed: NodeView? { nil }
    package var engaged: Bool { false }
    package func arm(_ node: NodeView, event: NSEvent) {}
    package func drag(_ event: NSEvent) -> Bool { false }
    package func up(_ event: NSEvent) -> Bool { false }
    package func cancel() {}
    package func retire(_ id: UInt32) {}
    package func scroll(_ node: NodeView, event: NSEvent) -> Bool { false }
    package func magnify(_ node: NodeView, event: NSEvent) -> Bool { false }
}
#endif
