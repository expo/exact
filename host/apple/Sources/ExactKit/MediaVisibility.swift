// What media and animated images share: the platform view type, which part
// of a view shows (`VideoVisibilityHost`), where an on-demand module's
// library is embedded, the page's commands to a video or audio element, and
// where its `postMessage` goes (an iframe or a canvas). These are the core's
// (LLP 1047.001 D10); the media family is `MediaLink`'s.
import Foundation
#if os(macOS)
import AppKit
package typealias MediaPlatformView = NSView
#else
import UIKit
package typealias MediaPlatformView = UIView
#endif

/// A player that pauses or plays as its view shows.
package protocol VisibilityRefreshing: AnyObject {
    /// The node whose view it shows in.
    var owner: NodeView? { get }
    func refreshVisibility()
}

/// Optional media policy. Scroll/layout notifications coalesce without app actions
/// or a frame clock; only a threshold crossing changes the player's paused request.
package final class VideoVisibilityHost {
    package init() {}
    #if os(iOS) || os(tvOS)
    private let scrolls = ScrollMembership()
    package func changed(in scroll: UIScrollView) {
        guard !videos.isEmpty, scrolls.contains(scroll, members: videos.values.compactMap { $0.value?.owner }) else {
            ImageLink.animated?.poke()
            return
        }
        changed()
    }
    #endif
    private final class WeakVideo {
        weak var value: (any VisibilityRefreshing)?
        init(_ value: any VisibilityRefreshing) { self.value = value }
    }
    private var videos: [ObjectIdentifier: WeakVideo] = [:]
    private var queued = false
    package func track(_ video: any VisibilityRefreshing) {
        let key = ObjectIdentifier(video)
        if videos[key] == nil { videos[key] = WeakVideo(video) }
        #if os(iOS) || os(tvOS)
        ScrollMembership.epoch &+= 1
        #endif
        changed()
    }
    package func remove(_ video: any VisibilityRefreshing) {
        videos.removeValue(forKey: ObjectIdentifier(video))
        #if os(iOS) || os(tvOS)
        ScrollMembership.epoch &+= 1
        #endif
    }
    package func reset() {
        videos.removeAll()
        #if os(iOS) || os(tvOS)
        ScrollMembership.epoch &+= 1
        #endif
    }
    package func changed() {
        ImageLink.animated?.poke()
        guard !videos.isEmpty, !queued else { return }
        queued = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.queued = false
            for (key, entry) in self.videos {
                if let video = entry.value { video.refreshVisibility() }
                else { self.videos.removeValue(forKey: key) }
            }
        }
    }
    /// Rectangular intersection, as IntersectionObserver without trackVisibility:
    /// ancestor clipping and the viewport count; sibling occlusion/opacity do not.
    package static func fraction(_ view: MediaPlatformView) -> CGFloat {
        guard let window = view.window, view.bounds.width > 0, view.bounds.height > 0 else { return 0 }
        #if os(macOS)
        guard let root = window.contentView else { return 0 }
        let box = view.convert(view.bounds, to: root)
        var clipped = box.intersection(root.bounds)
        var ancestor: NSView? = view
        while let current = ancestor {
            if current.isHidden { return 0 }
            if current !== view && (current is NSClipView || current.clipsToBounds || current.layer?.masksToBounds == true) {
                clipped = clipped.intersection(current.convert(current.bounds, to: root))
            }
            ancestor = current.superview
        }
        #else
        let box = view.convert(view.bounds, to: window)
        var clipped = box.intersection(window.bounds)
        var ancestor: UIView? = view
        while let current = ancestor {
            if current.isHidden { return 0 }
            if current !== view && current.clipsToBounds {
                clipped = clipped.intersection(current.convert(current.bounds, to: window))
            }
            ancestor = current.superview
        }
        #endif
        guard !clipped.isNull, box.width > 0, box.height > 0 else { return 0 }
        return min(1, max(0, clipped.width * clipped.height / (box.width * box.height)))
    }
}

#if !os(macOS)
/// An optional module in the app's Frameworks: wrapped as `<Name>.framework`
/// when the bundle is built for distribution (the App Store refuses loose
/// dylibs, ITMS-90171), otherwise the loose `lib….dylib` a development build
/// places there.
package func embeddedModule(framework: String, dylib: String) -> String {
    let directory = Bundle.main.privateFrameworksPath ?? Bundle.main.bundlePath
    let wrapped = directory + "/" + framework + ".framework/" + framework
    return FileManager.default.fileExists(atPath: wrapped) ? wrapped : directory + "/" + dylib
}
#endif

extension Presenter {
    /// `fastSeek(id, seconds)` and `load(id)` on the video or audio with that
    /// HTML `id`, as `focus(id)` names one.
    func mediaCommand(_ name: String, _ args: [Any]) {
        if name == "requestPictureInPicture" || name == "exitPictureInPicture" {
            guard let picture = MediaLink.pictureCommand else { return pictureUnlinked(name, args) }
            return picture(self, name, args)
        }
        let id = args.first as? String ?? ""
        let node = views.values.sorted(by: { $0.id < $1.id }).first(where: { $0.props["id"] == id })
        guard let video = node?.video else {
            session?.log("\(name) \"\(id)\" refused: \(node == nil ? "no live node with that id" : "not a video or audio")")
            return
        }
        video.command(name, seconds: (args.count > 1 ? args[1] as? Double : nil) ?? .nan)
    }
}

extension ExactSession {
    /// `postMessage(text, target)`: the compiler marks an iframe target; a name that is both a
    /// live frame's id and a canvas surface, or a canvas post to a frame's id, is refused.
    func post(_ text: String, to target: String, iframe: Bool) {
        if webviews.hasFrame(target) && canvases.hasSurface(target) {
            fputs("exact: postMessage: ambiguous iframe id and canvas surface \(target)\n", stderr)
        } else if iframe {
            webviews.postMessage(target, text)
        } else if webviews.hasFrame(target) {
            fputs("exact: postMessage: canvas name collides with an iframe id \(target)\n", stderr)
        } else {
            canvases.post(target, text)
        }
    }
}

extension Presenter {
    /// Without picture in picture, a request is refused as an unavailable
    /// one is (LLP 1042 §9): logged, and the image hears
    /// `pictureinpictureerror`. An exit finds no image to leave.
    fileprivate func pictureUnlinked(_ name: String, _ args: [Any]) {
        guard name == "requestPictureInPicture" else { session?.log("exitPictureInPicture ignored: no image is in the picture-in-picture window"); return }
        let id = args.first as? String ?? ""
        guard let node = views.values.sorted(by: { $0.id < $1.id }).first(where: { $0.props["id"] == id }) else {
            session?.log("requestPictureInPicture \"\(id)\" refused: no live node with that id"); return
        }
        let why = "picture in picture is not linked"
        session?.log("requestPictureInPicture \"\(id)\" refused: \(why)")
        guard node.handlers.contains("pictureinpictureerror"), let session else { return }
        session.apply(session.runtime.media(node.id, event: "pictureinpictureerror", payload: why, now: session.now()))
    }
}
