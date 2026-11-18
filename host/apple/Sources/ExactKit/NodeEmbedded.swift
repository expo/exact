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

extension NodeView {
    /// An iframe's web view, made once the node has its props: the arm then
    /// builds it with the frame's `partition`, and a hatched frame's moment
    /// before construction has the node's word and `data-*`. A hatched one
    /// waits for the module's hatches (LLP 1020.002 §9).
    package func embedFrame(_ presenter: Presenter) {
        guard kind == "iframe", !presenter.elements.waits(self) else { return }
        presenter.leaves.embed(self)
        updateEmbedded()
    }
}
