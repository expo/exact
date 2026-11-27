// The window's aspect-fit guide (LLP 1123 D16, `-exact-safe-area-aspect-fit`): the first root of a
// standalone session whose view spans its window claims the guide at the root's ratio, and the
// kernel lays the root out in the guide's rectangle. Exact saves the ratio the guide had and gives it
// back as the claim ends, unless something else wrote the guide since. A window has one claim: a
// second is refused before it writes anything. A hidden view constrained to the guide keeps UIKit
// laying it out.
#if os(iOS) || os(tvOS)
import UIKit

final class AspectFitIOS {
    /// The claim each window holds.
    private static let claims = NSMapTable<UIWindow, AspectFitIOS>.weakToWeakObjects()
    private weak var window: UIWindow?
    /// The ratio the guide had before the claim, the one Exact asked for, and UIKit's reading of it.
    private var saved: CGFloat = 1, asked: CGFloat?, written: CGFloat?
    private var proxy: UIView?

    /// The guide's rectangle in the document's space while `view`'s first root claims it, else `nil`:
    /// it claims, keeps or gives back the guide of the window `view` spans as the root and the mount
    /// say. A view the standalone adapter did not mount, or that does not span its window, claims none.
    func rectangle(for view: ExactView) -> CGRect? {
        let presenter = view.session.presenter
        guard view.standaloneWindow, let ratio = presenter.documentRoot.flatMap(Self.ratio), let window = view.window,
              view.convert(view.bounds, to: nil).insetBy(dx: -0.5, dy: -0.5).contains(window.bounds) else {
            release()
            return nil
        }
        if self.window !== window {
            release()
            if let other = Self.claims.object(forKey: window), other !== self {
                view.session.log("aspect fit: the window's guide is already claimed by another session's first root, so this one keeps the whole view")
                return nil
            }
            claim(window)
        }
        let guide = window.safeAreaAspectFitLayoutGuide
        if asked != ratio {
            asked = ratio
            guide.aspectRatio = ratio
            written = guide.aspectRatio
            // The guide's frame follows the ratio at the window's next layout, which lays this view out again
            window.setNeedsLayout()
            DispatchQueue.main.async { [weak view] in view?.setNeedsLayout() }
        }
        let frame = guide.layoutFrame
        guard frame.width > 0, frame.height > 0 else { return nil }
        return presenter.viewportPoints(presenter.viewport.convert(frame, from: window))
    }

    /// End the claim: the guide's ratio as it was, unless something else wrote it since, and the proxy gone.
    func release() {
        guard let window else { return }
        let guide = window.safeAreaAspectFitLayoutGuide
        if let written, guide.aspectRatio == written { guide.aspectRatio = saved }
        if Self.claims.object(forKey: window) === self { Self.claims.removeObject(forKey: window) }
        proxy?.removeFromSuperview()
        (proxy, asked, written, self.window) = (nil, nil, nil, nil)
    }

    private func claim(_ window: UIWindow) {
        self.window = window
        Self.claims.setObject(self, forKey: window)
        let guide = window.safeAreaAspectFitLayoutGuide
        saved = guide.aspectRatio
        let proxy = UIView()
        proxy.isHidden = true
        proxy.isUserInteractionEnabled = false
        proxy.translatesAutoresizingMaskIntoConstraints = false
        window.addSubview(proxy)
        NSLayoutConstraint.activate([
            proxy.leadingAnchor.constraint(equalTo: guide.leadingAnchor), proxy.trailingAnchor.constraint(equalTo: guide.trailingAnchor),
            proxy.topAnchor.constraint(equalTo: guide.topAnchor), proxy.bottomAnchor.constraint(equalTo: guide.bottomAnchor),
        ])
        self.proxy = proxy
    }

    /// The root's `-exact-safe-area-aspect-fit`, width over height, or `nil` for `none` and unsaid.
    static func ratio(_ root: NodeView) -> CGFloat? {
        guard let word = root.style["safe_area_aspect_fit"]?.string, word != "none" else { return nil }
        let parts = word.split(separator: "/").map { Double($0.trimmingCharacters(in: .whitespaces)) }
        guard let width = parts.first ?? nil else { return nil }
        let height = parts.count == 2 ? parts[1] : 1
        guard let height, width > 0, height > 0 else { return nil }
        return CGFloat(width / height)
    }
}
#endif
