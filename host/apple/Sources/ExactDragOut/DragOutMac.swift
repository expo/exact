// The Mac's drag out of the app (LLP 1094.000 D5): a mouse drag on a node
// with `draggable` is an AppKit dragging session, whose items are file
// promises and whose image is the node as drawn. It is the last candidate of
// the mouse chain on one node (D8): a reorder, transform, height drag, pan
// or swipe that takes the movement first keeps it.
#if os(macOS)
import AppKit
import ExactCore
import ExactKit
import UniformTypeIdentifiers

public enum ExactDragOut {
    public static func install() { DragOutLink.installed = DragOutHost() }
}

final class DragOutHost: DragOutCapability {
    func mouseDrag(_ presenter: Presenter) -> MouseRecognizer { MouseDragOut(presenter) }
}

/// Armed on a down at the innermost draggable node the press reaches
/// without crossing a node that hears `press` or edits text (D8's press
/// boundary), and engaged past the slop, as a dragging session from that
/// node. None arms under the agent: its mouse events are placed in the
/// window, and AppKit's session, which follows the hardware mouse, waits on
/// the main thread for a release that never comes (measured: the drive's
/// `tap … drag` timed out).
final class MouseDragOut: NSObject, MouseRecognizer, NSDraggingSource {
    weak var presenter: Presenter?
    private(set) weak var armed: NodeView?
    /// The node the down landed on, from which the source is found again as
    /// the drag begins.
    private weak var down: NodeView?
    private(set) var engaged = false
    /// The window the down was in, which a move and the source must still be in.
    private weak var downWindow: NSWindow?
    /// What the armed node carried at the down: a list row renewed for another
    /// item keeps its view and id, and the drag carries what was pressed.
    private var carried: [String?] = []
    /// The window a session began in, which its end posts the release to,
    /// whether or not the node is still there.
    private weak var sessionWindow: NSWindow?
    private var origin = CGPoint.zero
    /// AppKit's session, begun from the node; a test's stand-in records it,
    /// since a session follows the hardware mouse.
    var begin: (NodeView, [NSDraggingItem], NSEvent, NSDraggingSource) -> Void = { $0.beginDraggingSession(with: $1, event: $2, source: $3) }

    init(_ presenter: Presenter) { self.presenter = presenter }

    func arm(_ node: NodeView, event: NSEvent) {
        armed = nil; down = nil; engaged = false
        // A down the node is no longer in the window of arms nothing
        guard !ExactEnv.agentMode, let presenter, node.window?.windowNumber == event.windowNumber else { return }
        armed = Self.source(from: node, in: presenter)
        carried = armed.map(Self.carried) ?? []
        down = node
        downWindow = node.window
        origin = event.locationInWindow
    }

    func drag(_ event: NSEvent) -> Bool {
        guard let armed, let presenter, presenter.views[armed.id] === armed, let downWindow, armed.window === downWindow,
              event.windowNumber == downWindow.windowNumber else { return false }
        if engaged { return true }
        // Past the slop on an axis, as the layout pan and the swipe measure
        // it, so a dominant sideways drag reaches an earlier swipe first.
        let point = event.locationInWindow
        guard max(abs(point.x - origin.x), abs(point.y - origin.y)) > Gesture.slop else { return false }
        // A batch since the down may have disabled the node, turned its
        // `draggable` off, or put a press or a disabled node on the way up.
        // A batch since the down may also have hidden it or changed what it carries,
        // or retired the node the down landed on, which an exit keeps mounted.
        // Making the items reads props alone: their image is drawn when AppKit asks.
        guard let down, presenter.views[down.id] === down, Self.source(from: down, in: presenter) === armed,
              Self.carried(armed) == carried, !armed.cssVisibilityHidden, !armed.isHiddenOrHasHiddenAncestor else {
            self.armed = nil
            return false
        }
        let items = DragItemsMac.items(of: armed, in: presenter.session)
        guard !items.isEmpty else { self.armed = nil; return false }
        began(in: downWindow)
        // AppKit tracks the mouse from here; the session's end resets this.
        begin(armed, items, event, self)
        return true
    }

    /// The up `ended(at:)` posts, or a real one: taken while a session ran.
    func up(_ event: NSEvent) -> Bool {
        defer { armed = nil; down = nil; engaged = false }
        return engaged
    }

    func cancel() { armed = nil; down = nil; engaged = false }

    /// A session began from the armed node in `window`.
    func began(in window: NSWindow?) {
        engaged = true; sessionWindow = window
    }

    /// The node a down on `node` drags: the innermost draggable one that
    /// carries something on the way up, found through the presenter's index,
    /// unless a node that hears `press` or edits text, or a disabled or inert
    /// one, comes first.
    static func source(from node: NodeView, in presenter: Presenter) -> NodeView? {
        let draggable = presenter.carrying("draggable").filter { v in v.props["draggable"] == "true" && carried(v).contains { !($0 ?? "").isEmpty } }
        guard draggable.contains(where: { node.isDescendant(of: $0) }) else { return nil }
        let sources = Set(draggable.map(ObjectIdentifier.init))
        let nodes = Dictionary(presenter.views.values.map { (ObjectIdentifier($0), $0) }, uniquingKeysWith: { first, _ in first })
        var at: NSView? = node
        while let view = at {
            if let found = nodes[ObjectIdentifier(view)] {
                if found.disabled || found.inert { return nil }
                if sources.contains(ObjectIdentifier(view)) { return found }
                if found.handlers.contains("press") || found.field != nil || found.textArea != nil { return nil }
            }
            at = view.superview
        }
        return nil
    }

    /// What `node` carries, as its props say it.
    static func carried(_ node: NodeView) -> [String?] {
        ["dataTransferFiles", "dataTransferUrl", "dataTransferText"].map { node.props[$0] }
    }

    // MARK: NSDraggingSource

    func draggingSession(_ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation {
        Self.operations(context)
    }

    /// A copy outside the app; nothing inside it, which takes no drop (D5).
    static func operations(_ context: NSDraggingContext) -> NSDragOperation {
        context == .outsideApplication ? .copy : []
    }

    func draggingSession(_ session: NSDraggingSession, endedAt screenPoint: NSPoint, operation: NSDragOperation) {
        ended(at: screenPoint)
    }

    /// AppKit's session took the button's release, so the node never heard
    /// its `mouseUp`: one is posted where the drag ended, ahead of any later
    /// click, which releases the contact as any up does (the pointer's hold
    /// and hover, a collection's pinned row, the mouse chain), and this
    /// recognizer takes it. To the session's window, though a batch removed
    /// the node meanwhile.
    func ended(at screenPoint: NSPoint) {
        guard engaged, let window = sessionWindow,
              let up = NSEvent.mouseEvent(with: .leftMouseUp, location: window.convertPoint(fromScreen: screenPoint), modifierFlags: [],
                                          timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                          context: nil, eventNumber: 0, clickCount: 1, pressure: 0) else {
            armed = nil; engaged = false
            return
        }
        NSApp.postEvent(up, atStart: true)
    }
}
#endif
