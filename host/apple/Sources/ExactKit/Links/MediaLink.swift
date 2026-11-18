// @ref LLP 1047.001 D10 — media (cut list C5: iframes' web views, video and
// audio elements, sound, MIDI, the audio session and picture in picture) is
// a family the composition selects. Its arms already load on demand; this
// file is how the core reaches their Swift halves. Without the family the
// session holds `NoWebViews` and `NoAudioOutput`, a node gets no player, and
// the core makes none of its four audio-session calls. The `AudioSession`
// enum itself stays in ExactKit: canvases (ExactSurfaces) and hatches hold
// and release through it. The slots' defaults are today's.
import CExact
#if os(macOS)
import AppKit
package typealias ExactWebPlatformView = NSView
package typealias ExactWebImage = NSImage
#else
import UIKit
package typealias ExactWebPlatformView = UIView
package typealias ExactWebImage = UIImage
#endif

/// What the core asks of a session's web views (iframes, LLP 1020).
package protocol WebHosting: AnyObject {
    var session: ExactSession? { get set }
    var status: String { get }
    func create(owner: NodeView) -> ExactWebPlatformView?
    func update(_ owner: NodeView)
    func destroy(id: UInt32)
    func hatched(_ id: UInt32)
    func hasFrame(_ target: String) -> Bool
    func command(_ name: String, _ args: [Any])
    /// The page's `postMessage(text, target)` to an iframe.
    func postMessage(_ target: String, _ text: String)
    func endSession()
    func snapshots() -> [UInt32: ExactWebImage]
    #if EXACT_AGENT
    // What the agent asks: development only, so production strips them.
    func answered(_ reply: [String: Any], request: [String: Any]) -> Bool
    func tree(_ line: String) -> [String: Any]
    func tap(_ owner: NodeView, request values: [String: Any], at: [Double]) -> [String: Any]
    func type(_ owner: NodeView, request values: [String: Any]) -> [String: Any]
    #endif
}

#if EXACT_AGENT
extension WebHosting {
    /// The page's tree, its frames described.
    func tree() -> [String: Any] { tree("{\"op\":\"tree\"}") }
}
#endif

/// No web views: an iframe stays an empty box.
package final class NoWebViews: WebHosting {
    package init() {}
    package weak var session: ExactSession?
    package var status: String { "none" }
    package func create(owner: NodeView) -> ExactWebPlatformView? { nil }
    package func update(_ owner: NodeView) {}
    package func destroy(id: UInt32) {}
    package func hatched(_ id: UInt32) {}
    package func hasFrame(_ target: String) -> Bool { false }
    package func command(_ name: String, _ args: [Any]) {}
    package func postMessage(_ target: String, _ text: String) {}
    package func endSession() {}
    package func snapshots() -> [UInt32: ExactWebImage] { [:] }
    #if EXACT_AGENT
    package func answered(_ reply: [String: Any], request: [String: Any]) -> Bool { false }
    /// The runner's tree, with no frame to describe.
    package func tree(_ line: String) -> [String: Any] {
        guard let session, let data = session.runtime.ask(exact_tree, line).data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return ["error": "unreadable tree"] }
        return root
    }
    package func tap(_ owner: NodeView, request values: [String: Any], at: [Double]) -> [String: Any] { ["error": "no web views"] }
    package func type(_ owner: NodeView, request values: [String: Any]) -> [String: Any] { ["error": "no web views"] }
    #endif
}

/// What the core asks of a video or audio element's player.
package protocol VideoPlaying: AnyObject {
    /// The node's props changed: the player follows them.
    func update()
    func layout()
    /// The platform player (`AVPlayer`), for an element hatch.
    var platform: AnyObject? { get }
    var isFullscreen: Bool { get }
    func togglePlayPause()
    /// `fastSeek(id, seconds)` and `load(id)` (`Presenter.mediaCommand`).
    func command(_ name: String, seconds: Double)
    /// CSS `visibility: hidden` on its node.
    func applyCssHidden(_ hidden: Bool)
    /// The node left: the player stops and lets go.
    func invalidate()
    #if EXACT_AGENT
    /// What the agent reports, and its media session's latest report.
    func state() -> [String: Any]
    var sessionReport: [String: Any] { get }
    /// The agent's media-session action (`play`, `seekto`…); nil: refused.
    func remote(_ action: String, seconds: Double?) -> String?
    #endif
}

/// What the core asks of a session's sound or MIDI output.
package protocol AudioOutputting: AnyObject {
    func apply(_ payload: [String: Any])
    func shutdown()
    #if EXACT_AGENT
    /// What the agent reports.
    var state: [String: Any] { get }
    #endif
}

/// No audio output: nothing plays.
package final class NoAudioOutput: AudioOutputting {
    package init() {}
    package func apply(_ payload: [String: Any]) {}
    package func shutdown() {}
    #if EXACT_AGENT
    package var state: [String: Any] { [:] }
    #endif
}

#if os(iOS) || os(tvOS)
/// What the core asks of the process's audio session (AudioSession.swift).
package protocol AudioSessionControlling: AnyObject {
    /// Around a batch's presentation: claims made while it is applied settle once.
    func beginPresentation()
    func endPresentation()
    /// Whether a background audio claim keeps the app running in the background.
    func backgroundExecutionEligible() -> Bool
    func enteredBackground()
}
#endif

/// The slots are set before any session is made, and read on the main thread.
package enum MediaLink {
    /// A session's web views.
    package static var makeWeb: (() -> any WebHosting)? = { WebViews() }
    /// A video or audio element's player, made when its node is.
    package static var makeVideo: ((NodeView) -> any VideoPlaying)? = { VideoView(owner: $0) }
    /// A session's sound and MIDI output.
    package static var makeSound: ((ExactSession) -> any AudioOutputting)? = { SoundOutput($0) }
    package static var makeMIDI: ((ExactSession) -> any AudioOutputting)? = { MIDIOutput($0) }
    #if os(iOS) || os(tvOS)
    /// The process's audio session.
    package static var audioSession: (any AudioSessionControlling)? = BuiltInAudioSession()
    #endif
    /// An image's picture in picture: the page's `requestPictureInPicture`
    /// and `exitPictureInPicture`; its source changed, or it left the document.
    package static var pictureCommand: ((Presenter, String, [Any]) -> Void)? = { presenter, name, args in
        if name == "requestPictureInPicture" { presenter.pictureInPicture(args) } else { PictureInPictureHost.exit(presenter.session) }
    }
    package static var pictureChanged: ((NodeView) -> Void)? = { PictureInPictureHost.changed($0) }
    package static var pictureGone: ((NodeView) -> Void)? = { PictureInPictureHost.gone($0) }
}

#if os(macOS) && EXACT_AGENT
extension MediaLink {
    /// The agent's view of Exact's picture-in-picture window (LLP 1042 §9.1):
    /// its row in `tree`, a tap on its controls, its screenshot.
    package static var pictureDecorate: ((Agent, [String: Any]) -> [String: Any])? = { $0.decoratePictureInPicture($1) }
    package static var pictureTap: ((Agent, [String: Any], String) -> [String: Any])? = { $0.pictureInPictureTap($1, control: $2) }
    package static var pictureShot: ((Agent, String, UInt32?) -> [String: Any])? = { $0.pictureInPictureShot($1, of: $2) }
}
#endif
