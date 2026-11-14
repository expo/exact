#if os(macOS)
import AppKit
#else
import UIKit
#endif

extension NodeView {
    /// The embedded platform content a node's kind brings: an iframe's web
    /// view (LLP 1020 D3) or a native module's box (LLP 1024 D2).
    func embedPlatformView(_ presenter: Presenter) {
        if kind == "native" { presenter.session?.natives.create(owner: self); return }
        guard kind == "iframe", let w = presenter.session?.webviews.create(owner: self) else { return }
        w.frame = bounds
        #if os(macOS)
        w.autoresizingMask = [.width, .height]
        w.wantsLayer = true
        #else
        w.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        #endif
        w.isHidden = cssVisibilityHidden
        addSubview(w)
        web = w
    }

    func updateEmbedded() {
        if kind == "iframe" { presenter?.session?.webviews.update(self) }
        if kind == "native" { presenter?.session?.natives.update(self) }
    }

    func destroyEmbedded() {
        presenter?.session?.webviews.destroy(id: id)
        presenter?.session?.natives.destroy(id: id)
    }
}
