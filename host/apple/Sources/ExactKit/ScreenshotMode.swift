// Screenshot mode: what a driver that captures many states needs from a development build, so each state reaches the
// screen at once and in one frame. Set in the launch environment of an agent build (EXACT_AGENT); a production build
// compiles none of it, so every flag reads as off there.
//
//   EXACT_ANIMATION_SPEED=<n>    every window's layer runs n times faster (CALayer.speed), windows UIKit adds later
//                                included: an animation ends in 1/n of its time.
//   EXACT_UIKIT_ANIMATIONS=0     UIKit's view animations are off (UIView.setAnimationsEnabled), and a tab switch
//                                selects without the tab bar's transition.
//   EXACT_AGENT_ONE_COMMIT=1     the agent's tap runs in one CATransaction, so its state change reaches the render
//                                server as one commit instead of several.
//   EXACT_AGENT_TAP_SETTLE=1     the agent's tap settles the clock in the same main-queue turn and answers with the
//                                settle (`settle` in its reply): one request per state instead of a tap and a clock.
//
// Public API only: CALayer.speed, UIView.setAnimationsEnabled, UIView.performWithoutAnimation, CATransaction.
import Foundation
#if os(iOS)
import UIKit
#endif

public enum ScreenshotMode {
    #if EXACT_AGENT
    private static let environment = ProcessInfo.processInfo.environment
    public static let uikitAnimationsOff = environment["EXACT_UIKIT_ANIMATIONS"] == "0"
    public static let oneCommit = environment["EXACT_AGENT_ONE_COMMIT"] == "1"
    public static let tapSettle = environment["EXACT_AGENT_TAP_SETTLE"] == "1"
    static let speed: Float? = environment["EXACT_ANIMATION_SPEED"].flatMap(Float.init).flatMap { $0 > 0 ? $0 : nil }
    #else
    public static let uikitAnimationsOff = false
    public static let oneCommit = false
    public static let tapSettle = false
    #endif

    #if os(iOS) && EXACT_AGENT
    private static var observer: NSObjectProtocol?
    #endif

    /// Applies the animation flags once the app's window is visible. Does nothing unless one is set.
    @MainActor public static func install() {
        #if os(iOS) && EXACT_AGENT
        if uikitAnimationsOff { UIView.setAnimationsEnabled(false) }
        guard let speed, observer == nil else { return }
        apply(speed)
        observer = NotificationCenter.default.addObserver(forName: UIWindow.didBecomeVisibleNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { apply(speed) }
        }
        #endif
    }

    #if os(iOS) && EXACT_AGENT
    @MainActor private static func apply(_ speed: Float) {
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            for window in scene.windows where window.layer.speed != speed { window.layer.speed = speed }
        }
    }
    #endif
}
