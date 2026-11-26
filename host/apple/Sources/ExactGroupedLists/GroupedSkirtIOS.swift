// LLP 1116.004 D2: a grouped list's background past the edges of a viewport the safe area keeps short of the screen's.
import ExactKit
#if os(iOS)
import UIKit

extension GroupedListView {
    /// LLP 1116.004 D2 (chat-rooms, round 6): where the list's box meets an
    /// edge of a viewport the safe area keeps short of the screen's — a page
    /// with no navigation container keeps its content inside the safe area —
    /// the list's background goes on to the screen's edge, under the home
    /// indicator or the status bar, as a collection view pinned to its view's
    /// edges draws its grouped background there. The canvas there was the
    /// root's white under a grey list. Behind the viewport, so only what lies
    /// past its edges shows. Not while the keyboard ends the viewport (the
    /// keys cover that edge), nor on a page that scrolls, where the list does
    /// not stay at the edge.
    func hem() {
        let presenter = host.presenter, viewport = presenter.viewport
        guard let container = viewport.superview, owner.window != nil, presenter.inDocument(owner),
              presenter.keyboardInset == 0, viewport.contentSize.height <= viewport.bounds.height + 0.5,
              viewport.contentSize.width <= viewport.bounds.width + 0.5 else { skirt?.isHidden = true; return }
        // The page's area, which the viewport is drawn past (LLP 1130).
        let port = presenter.pageFrame, whole = container.bounds, box = owner.convert(owner.bounds, to: container)
        let meets = { (a: CGFloat, b: CGFloat) in abs(a - b) < 0.5 }
        let top = meets(box.minY, port.minY) && port.minY > whole.minY ? whole.minY : box.minY
        let bottom = meets(box.maxY, port.maxY) && port.maxY < whole.maxY ? whole.maxY : box.maxY
        let left = meets(box.minX, port.minX) && port.minX > whole.minX ? whole.minX : box.minX
        let right = meets(box.maxX, port.maxX) && port.maxX < whole.maxX ? whole.maxX : box.maxX
        let reach = CGRect(x: left, y: top, width: right - left, height: bottom - top)
        guard reach != box, !owner.isHidden, !owner.groundIsSidebars else { skirt?.isHidden = true; return }
        let view = skirt ?? {
            let made = UIView()
            made.isUserInteractionEnabled = false
            skirt = made
            return made
        }()
        // The list's own background in each appearance, as its layer paints it: the author's, else the platform's.
        let sides = [owner.cgColor("background_color", dark: false), owner.cgColor("background_color", dark: true)]
        if sides != skirtKey || view.backgroundColor == nil {
            skirtKey = sides
            view.backgroundColor = UIColor { [weak self, weak owner] traits in
                if let colour = owner?.cgColor("background_color", dark: traits.userInterfaceStyle == .dark) { return UIColor(cgColor: colour) }
                // A background written with no colour (`initial`) is none, as the list's box is.
                guard let self, !writesGround else { return .clear }
                return platformGround
            }
        }
        // Only past the list's box, where the list itself paints: a translucent colour is not laid over itself.
        let local = CGRect(origin: .zero, size: reach.size), path = UIBezierPath(rect: local)
        path.append(UIBezierPath(rect: box.offsetBy(dx: -reach.minX, dy: -reach.minY)))
        let mask = view.layer.mask as? CAShapeLayer ?? {
            let made = CAShapeLayer()
            made.fillRule = .evenOdd
            view.layer.mask = made
            return made
        }()
        CATransaction.begin(); CATransaction.setDisableActions(true)
        mask.frame = local
        mask.path = path.cgPath
        CATransaction.commit()
        // Behind the document, where it is mounted: that covers the whole view.
        let document = presenter.root, page = presenter.documentHost
        if view.superview !== page || page.subviews.firstIndex(of: view).map({ $0 > (page.subviews.firstIndex(of: document) ?? 0) }) == true {
            page.insertSubview(view, belowSubview: document)
        }
        assign(view, \.frame, page.convert(reach, from: container))
        assign(view, \.isHidden, false)
    }
}
#endif
