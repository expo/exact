// @ref LLP 1047.001 D10 — media is four products the composition selects,
// each the Swift half of a loaded arm: `ExactVideo` (video and audio
// elements' players, and an image's picture in picture), `ExactWebViews`
// (iframes), `ExactSound` and `ExactMIDI`. This file is how the core reaches
// them; each product's `install()` sets its slots. Without one, the session
// holds `NoWebViews` or `NoAudioOutput`, or a node gets no player, or a
// picture-in-picture request is refused. The audio session's code is the
// core's, since players, sound, canvases (ExactSurfaces) and hatches
// (ExactNativeViews) all claim it; each of those installs it.
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
    func reloadDocument(_ owner: NodeView)
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
    package func reloadDocument(_ owner: NodeView) {}
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
    func reloadAssets(_ properties: Set<String>)
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
/// Functions, not a conforming type, so that an app with no claimant links
/// none of the session's code.
package struct AudioSessionCalls {
    /// Around a batch's presentation: claims made while it is applied settle once.
    let beginPresentation: () -> Void
    let endPresentation: () -> Void
    /// Whether a background audio claim keeps the app running in the background.
    let backgroundExecutionEligible: () -> Bool
    let enteredBackground: () -> Void
}
#endif

/// The slots are set before any session is made, and read on the main thread.
package enum MediaLink {
    /// A session's web views.
    package static var makeWeb: (() -> any WebHosting)?
    /// A video or audio element's player, made when its node is.
    package static var makeVideo: ((NodeView) -> any VideoPlaying)?
    /// A session's sound and MIDI output.
    package static var makeSound: ((ExactSession) -> any AudioOutputting)?
    package static var makeMIDI: ((ExactSession) -> any AudioOutputting)?
    #if os(iOS) || os(tvOS)
    /// The process's audio session: nil until a family that claims it
    /// (players, sound, canvases' audio, hatches) is installed, since
    /// without a claim the core's four calls have nothing to do.
    package static var audioSession: AudioSessionCalls?
    /// A family that claims the audio session installs it.
    package static func claimsAudioSession() {
        audioSession = AudioSessionCalls(beginPresentation: AudioSession.beginPresentation, endPresentation: AudioSession.endPresentation,
                                         backgroundExecutionEligible: AudioSession.backgroundExecutionEligible, enteredBackground: AudioSession.enteredBackground)
    }
    #endif
    /// An image's picture in picture: the page's `requestPictureInPicture`
    /// and `exitPictureInPicture`; its source changed, or it left the document.
    package static var pictureCommand: ((Presenter, String, [Any]) -> Void)?
    package static var pictureChanged: ((NodeView) -> Void)?
    package static var pictureGone: ((NodeView) -> Void)?
}


#if os(macOS) && EXACT_AGENT
extension MediaLink {
    /// The agent's view of Exact's picture-in-picture window (LLP 1042 §9.1):
    /// its row in `tree`, a tap on its controls, its screenshot.
    package static var pictureDecorate: ((Agent, [String: Any]) -> [String: Any])?
    package static var pictureTap: ((Agent, [String: Any], String) -> [String: Any])?
    package static var pictureShot: ((Agent, String, UInt32?) -> [String: Any])?
}
#endif
