// The presenter's context previews (`contextTarget`): each follows its
// source through layout passes and scrolls, and stays above the keyboard.
#if os(iOS) || os(tvOS)
import UIKit
extension Presenter {
    /// Context actions belong to the existing editing session. Resigning
    /// it before dispatch can move the panel out from under the same tap.
    func contextRetainsFocus(_ view: UIView) -> Bool {
        var ancestor: UIView? = view
        while let current = ancestor {
            if let node = current as? NodeView, node.props["retainFocus"] == "true" { return true }
            ancestor = eventParent(of: current)
        }
        return carrying("contextTarget").contains { preview in
            guard preview.props["contextTarget"] != nil, let panel = contextPanel(preview) else { return false }
            return view.isDescendant(of: panel)
        }
    }

    private func contextPanel(_ preview: NodeView) -> NodeView? {
        var parent = preview.superview as? NodeView
        while let node = parent {
            if node.layoutPosition == "absolute" { return node }
            parent = node.superview as? NodeView
        }
        return nil
    }

    struct ContextAnchor {
        let target: String
        let source: NodeView
        /// Its source's box and scroller's box are in `port`'s coordinates: the
        /// source's port when they were captured (LLP 1123 D2)
        weak var port: UIView?
        let box: CGRect
        let viewportWidth: CGFloat
        let scroll: NodeView?
        let scrollBox: CGRect?
    }

    /// Capture before commands in this batch can hide the keyboard and lay
    /// out the source again. The preview owns this bounded presentation state.
    func prepareContexts(_ batch: Batch) {
        for op in batch.ops {
            let kind = op.op
            guard kind == .create || kind == .props,
                  let id = op.nodeID,
                  let target = op.props["contextTarget"],
                  contextAnchors[UInt32(id)]?.target != target,
                  let source = carrying("id").first(where: { $0.props["id"] == target }),
                  source.window != nil else { continue }
            contextAnchors[UInt32(id)] = contextAnchor(target, source)
        }
    }

    /// Locate the source's vertical scroll contents and the owner of its clip.
    /// Projection does not move the composer or write a scroll offset.
    private func contextContent(_ source: NodeView) -> (content: NodeView, scroll: NodeView)? {
        var child: UIView = source
        while let parent = child.superview {
            if let scroll = parent as? ScrollView, scroll.scrollsY,
               let content = child as? NodeView, let owner = scroll.superview as? NodeView {
                return (content, owner)
            }
            child = parent
        }
        return nil
    }

    private func contextAnchor(_ target: String, _ source: NodeView) -> ContextAnchor {
        let scroll = contextContent(source)?.scroll, port = port(of: source)
        return ContextAnchor(target: target, source: source, port: port,
            box: source.convert(source.bounds, to: port), viewportWidth: port.bounds.width,
            scroll: scroll, scrollBox: scroll.map { $0.convert($0.bounds, to: port) })
    }

    /// Magnify the preview without reflowing its text, keeping the source's
    /// outside edge and vertical center. Later content keeps its source-relative
    /// position; clamp the complete projection above the keyboard/safe area.
    /// A scroll whose batch skipped the pass (`ExactSession.applyUnlessEmpty`):
    /// what the pass does that the scroll moved. A context preview follows its
    /// source out of the scrolled box, and the fixed gradients its
    /// compensation moved are aimed again after it, as the pass aims them.
    func scrolledWithoutPass() {
        positionContexts()
        reaimFixedGradients()
    }

    func positionContexts() {
        for id in contextNodes {
            guard let node = views[id] else { continue }
            node.contextTransform = .identity
            moveOrPlace(node) { node.applyTransform() }
        }
        contextAnchors = contextAnchors.filter { id, anchor in
            views[id]?.props["contextTarget"] == anchor.target &&
                views[anchor.source.id] === anchor.source && anchor.source.window != nil
        }
        func project(_ node: NodeView, _ transform: CGAffineTransform) {
            node.contextTransform = transform
            moveOrPlace(node) { node.applyTransform() }
        }
        for preview in carrying("contextTarget") {
            guard let target = preview.props["contextTarget"],
                  let source = carrying("id").first(where: { $0.props["id"] == target }),
                  let panel = contextPanel(preview), let parent = panel.superview,
                  source.window != nil, preview.bounds.width > 0, preview.bounds.height > 0 else { continue }
            let liveSourceBox = source.convert(source.bounds, to: parent)
            // The source is read where it was captured; the preview is kept
            // inside its own port, the viewport or its presentation's mount
            let vp = port(of: preview), insets = vp === viewport ? insets : vp.safeAreaInsets
            if contextAnchors[preview.id] == nil ||
                contextAnchors[preview.id]?.viewportWidth != port(of: source).bounds.width {
                contextAnchors[preview.id] = contextAnchor(target, source)
            }
            let anchor = contextAnchors[preview.id]!
            let sourceBox = (anchor.port ?? viewport).convert(anchor.box, to: parent)
            let content = preview.convert(preview.bounds, to: parent)
            let port = vp.convert(vp === viewport ? vp.bounds.inset(by: pageMargins) : vp.bounds, to: parent)
            // Public UIKit target-preview fixture, iPhone 17 / iOS 26.5.
            let scale = preview.props["contextMagnify"] == "false" ? 1 : min(1.15, 1 + 26 / max(content.width, content.height))
            let extra = content.height * (scale - 1)
            let dx = sourceBox.midX > port.midX
                ? sourceBox.maxX - content.maxX - content.width * (scale - 1) / 2
                : sourceBox.minX - content.minX + content.width * (scale - 1) / 2
            // Top-aligned side slots keep their gap from the enlarged edge.
            // Their zero-height layout boxes do not enlarge the balloon row.
            if let row = preview.superview {
                for sibling in row.subviews.compactMap({ $0 as? NodeView })
                    where sibling !== preview && abs(sibling.frame.minY - preview.frame.minY) < 0.01 {
                    if sibling.frame.maxX <= preview.frame.minX + 0.01 {
                        project(sibling, CGAffineTransform(translationX: dx - content.width * (scale - 1) / 2, y: 0))
                    } else if sibling.frame.minX >= preview.frame.maxX - 0.01 {
                        project(sibling, CGAffineTransform(translationX: dx + content.width * (scale - 1) / 2, y: 0))
                    }
                }
            }
            var child: NodeView? = preview
            while let current = child, current !== panel, let container = current.superview {
                let bottom = current.frame.maxY
                for sibling in container.subviews.compactMap({ $0 as? NodeView })
                    where sibling !== current && sibling.frame.minY >= bottom - 0.01 {
                    project(sibling, CGAffineTransform(translationX: 0, y: extra / 2))
                }
                child = container as? NodeView
            }
            project(preview, CGAffineTransform(translationX: dx, y: extra / 2).scaledBy(x: scale, y: scale))
            // The authored containing region can reserve room for other
            // context content, such as a reaction's participant popover.
            // Static wrappers do not contain the panel. Clamp against its
            // nearest positioned ancestor, expressed in the parent coordinates.
            var containing = parent
            while containing !== root && containing.superview !== root {
                if let node = containing as? NodeView, node.layoutPosition != "static" { break }
                // A route's root, whose container is its controller's view, is as far as a presented one goes
                guard let ancestor = containing.superview, ancestor is NodeView || ancestor === root else { break }
                containing = ancestor
            }
            let region = containing.convert(containing.bounds, to: parent)
            let minimum = max(region.minY, port.minY + insets.top + 8)
            let overflow = max(extra / 2, content.maxY + extra - panel.frame.maxY)
            let maximum = min(region.maxY, port.maxY - insets.bottom - 8) - panel.bounds.height - overflow
            let wanted = sourceBox.midY - (content.minY - panel.frame.minY) - content.height * scale / 2
            let top = max(minimum, min(wanted, maximum))
            if panel.frame.origin.y != top { moveOrPlace(panel) { panel.frame.origin.y = top } }
            // A tall preview can fill the available region. Keep the trailing
            // control group inside it, even when that overlaps the preview (the native 24-line Messages reference does this).
            var branch: UIView = preview
            while let owner = branch.superview, owner !== panel { branch = owner }
            let trailing = panel.subviews.compactMap { $0 as? NodeView }.filter {
                $0 !== branch && $0.frame.minY >= branch.frame.maxY - 0.01
            }
            let bottom = min(region.maxY, port.maxY - insets.bottom - 8)
            if let last = trailing.map({ $0.convert($0.bounds, to: parent).maxY }).max(),
               let first = trailing.map({ $0.convert($0.bounds, to: parent).minY }).min() {
                let overflow = min(max(0, last - bottom), max(0, first - minimum))
                if overflow > 0 {
                    for node in trailing {
                        project(node, node.contextTransform.concatenating(CGAffineTransform(translationX: 0, y: -overflow)))
                    }
                }
            }
            if let context = contextContent(source), !panel.isDescendant(of: context.content) {
                // A centered, content-sized scroller moves when the keyboard
                // closes. Retain its clip position too, or neighbors disappear.
                var scrollDelta: CGFloat = 0
                if context.scroll === anchor.scroll, let box = anchor.scrollBox,
                   !panel.isDescendant(of: context.scroll) {
                    scrollDelta = (anchor.port ?? viewport).convert(box, to: parent).minY -
                        context.scroll.convert(context.scroll.bounds, to: parent).minY
                    project(context.scroll, CGAffineTransform(translationX: 0, y: scrollDelta))
                }
                let displacement = sourceBox.minY + top - wanted - liveSourceBox.minY
                project(context.content, CGAffineTransform(translationX: 0, y: displacement - scrollDelta))
            }
        }
    }
}
#endif
