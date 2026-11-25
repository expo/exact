// The user's display preferences, as the platform's accessibility settings
// hold them (LLP 1061 D5; LLP 1069.000 D1): what a browser reports as
// `prefers-reduced-motion`, `prefers-reduced-transparency`,
// `prefers-contrast` and `prefers-color-scheme`. The runner answers them
// through `exactViewport()`; the app decides what they mean. Press feedback
// stays visible under reduced motion (LLP 1061's 2026-09-27 ruling).
// An agent's `prefer` stands in for the settings, for this process only.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

#if os(macOS)
extension ExactEnv {
    /// The manifest's fixed `userInterfaceStyle` (`ExactUserInterfaceStyle`,
    /// host/apple/plist.mjs `interfaceStyle`), or `nil` to follow the Mac.
    /// AppKit has no layer beneath the application's appearance, so the app
    /// sets it before its first window and `setScheme("system")` returns to it.
    nonisolated(unsafe) public static let fixedAppearance: NSAppearance? = switch appMetadata["ExactUserInterfaceStyle"] as? String {
    case "dark": NSAppearance(named: .darkAqua)
    case "light": NSAppearance(named: .aqua)
    default: nil
    }
}
#endif

package enum DisplayPreferences {
    /// The agent starts without a detection, independent of the machine's setting.
    nonisolated(unsafe) static var agentScreenReader: String? = ExactEnv.agentMode ? "unknown" : nil {
        didSet { changed() }
    }
    static var screenReader: String {
        if let agentScreenReader { return agentScreenReader }
        #if os(macOS)
        return NSWorkspace.shared.isVoiceOverEnabled ? "on" : "off"
        #else
        return UIAccessibility.isVoiceOverRunning ? "on" : "off"
        #endif
    }
    /// What an agent's `prefer` set, in place of the platform's settings.
    /// Under the agent it starts at `no-preference` for both, never the
    /// machine's (LLP 1069.007 D2: facts are fixed at launch).
    nonisolated(unsafe) static var agent: (reducedMotion: Bool, reducedTransparency: Bool)? = ExactEnv.agentMode ? (false, false) : nil {
        didSet { changed() }
    }
    /// What an agent's `prefer` set for `prefers-contrast`, in CSS's words;
    /// `no-preference` from launch under the agent (LLP 1069.007 D2).
    nonisolated(unsafe) static var agentContrast: String? = ExactEnv.agentMode ? "no-preference" : nil {
        didSet { changed() }
    }
    /// What an agent's `prefer` set for `color-gamut`; `srgb` from launch
    /// under the agent (LLP 1069.007 D2).
    nonisolated(unsafe) static var agentGamut: String? = ExactEnv.agentMode ? "srgb" : nil {
        didSet { changed() }
    }
    /// What an agent's `prefer` (or a test) set for Bold Text; off from
    /// launch under the agent (LLP 1069.007 D2).
    nonisolated(unsafe) static var agentBoldText: Bool? = ExactEnv.agentMode ? false : nil {
        didSet { changed() }
    }
    /// What an agent's `prefer` set for `pointer` and `hover` (LLP 1132 D10);
    /// the device class's from launch under the agent, so a drive never reads
    /// whether the machine running it has a mouse (LLP 1069.007 D2).
    nonisolated(unsafe) static var agentInput: Preferences? = ExactEnv.agentMode ? .platformInput : nil {
        didSet { changed() }
    }
    private static let agentChanged = Notification.Name("ExactDisplayPreferencesChanged")
    private static var center: NotificationCenter {
        #if os(macOS)
        return NSWorkspace.shared.notificationCenter
        #else
        return NotificationCenter.default
        #endif
    }
    /// Something a reading depends on changed outside a platform
    /// notification (an agent's `prefer`, the agent's system scheme): every
    /// session tells its runner again.
    static func changed() { center.post(name: agentChanged, object: nil) }

    package static var reducedMotion: Bool {
        if let agent { return agent.reducedMotion }
        #if os(macOS)
        return NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        #else
        return UIAccessibility.isReduceMotionEnabled
        #endif
    }
    static package var reducedTransparency: Bool {
        if let agent { return agent.reducedTransparency }
        #if os(macOS)
        return NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
        #else
        return UIAccessibility.isReduceTransparencyEnabled
        #endif
    }
    /// `prefers-contrast`: Increase Contrast (macOS) or Increase Contrast /
    /// Darker System Colors (iOS) is `more`; neither platform has `less`.
    static package var contrast: String {
        if let agentContrast { return agentContrast }
        #if os(macOS)
        return NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast ? "more" : "no-preference"
        #else
        return UIAccessibility.isDarkerSystemColorsEnabled ? "more" : "no-preference"
        #endif
    }
    /// Bold Text (LLP 1115.000 D12.3): the person's setting, which the
    /// kernel weights the system font by. The Mac has none.
    static var boldText: Bool {
        if let agentBoldText { return agentBoldText }
        #if os(macOS)
        return false
        #else
        return UIAccessibility.isBoldTextEnabled
        #endif
    }
    /// `pointer` and `hover` (LLP 1132 D10): the device class's, except that
    /// an iPad with a trackpad or mouse attached points with it, as Safari on
    /// iPadOS answers.
    static var input: Preferences {
        if let agentInput { return agentInput }
        #if os(iOS)
        return touchScreenInput(mouse: mouse.flatMap { ($0 as AnyObject).perform(NSSelectorFromString("current"))?.takeUnretainedValue() })
        #else
        return .platformInput
        #endif
    }
    /// A touch screen's input given the mouse GameController reports
    /// attached (`GCMouse.current`, a trackpad too), or none.
    static func touchScreenInput(mouse: AnyObject?) -> Preferences {
        .primaryInput(touchScreen: true, pointingDevice: mouse != nil)
    }
    /// `color-gamut` (LLP 1100 D9). Neither platform reports a rec2020 panel.
    static var gamut: String {
        if let agentGamut { return agentGamut }
        #if os(macOS)
        return NSScreen.main?.canRepresent(.p3) == true ? "p3" : "srgb"
        #else
        return UIScreen.main.traitCollection.displayGamut == .P3 ? "p3" : "srgb"
        #endif
    }
    /// `dynamic-range: high` (LLP 1100 D9): the main display's potential headroom.
    static var highDynamicRange: Bool {
        if let pinned = DisplayRange.pinned { return pinned > 1 }
        #if os(macOS)
        return (NSScreen.main?.maximumPotentialExtendedDynamicRangeColorComponentValue ?? 1) > 1
        #else
        return UIScreen.main.potentialEDRHeadroom > 1
        #endif
    }
    /// `prefers-color-scheme: dark` — the *system's* appearance, beneath any
    /// `setScheme` the app chose (LLP 1034 D3 as amended by LLP 1069.000 D1).
    /// AppKit says it publicly only as the application's effective
    /// appearance, which an appearance the app sets (`setScheme`'s
    /// `NSApp.appearance`) replaces. So it is read while the app sets none,
    /// and while it sets one it is the system's as last read: a change the
    /// person makes in System Settings then is seen once the app follows
    /// the system again.
    /// An app whose manifest fixes `userInterfaceStyle` sees it as the
    /// system's, as UIKit's `UIUserInterfaceStyle` makes it on iOS.
    #if os(macOS)
    static package var systemDark: Bool {
        if let fixed = ExactEnv.fixedAppearance { return fixed.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
        #if EXACT_AGENT
        if let agent = Agent.systemAppearance { return agent.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
        #endif
        guard let app = NSApp else { return false }
        let dark = app.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        guard app.appearance == nil else { return followedDark ?? dark }
        followedDark = dark
        return dark
    }
    /// The system's appearance as last read while the app set none.
    nonisolated(unsafe) private static var followedDark: Bool?
    #endif
    /// Encode this window's readings with the core's shared wire layout.
    static func bits(systemDark: Bool, view: PlatformView? = nil) -> UInt32 {
        #if os(macOS)
        let displayGamut = agentGamut ?? ((view?.window?.screen ?? NSScreen.main)?.canRepresent(.p3) == true ? "p3" : "srgb")
        #else
        let displayGamut = agentGamut ?? ((view?.window?.screen ?? UIScreen.main).traitCollection.displayGamut == .P3 ? "p3" : "srgb")
        #endif
        let high = view.map { DisplayRange.headroom($0) > 1 } ?? highDynamicRange
        return Preferences(reducedMotion: reducedMotion, reducedTransparency: reducedTransparency,
            contrast: contrast, dark: systemDark, input: input, gamut: displayGamut, highDynamicRange: high,
            displayMode: Preferences.DisplayMode(rawValue: displayMode(view)) ?? .standalone, boldText: boldText,
            screenReader: screenReader).rawValue | orientationBits(view)
    }

    /// Bits 20–24 carry the screen reading (runner/src/viewport.rs).
    private static func orientationBits(_ view: PlatformView?) -> UInt32 {
        #if os(macOS)
        guard let screen = view?.window?.screen ?? NSScreen.main else { return 0 }
        let id = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID
        let degrees = id.map(CGDisplayRotation) ?? 0
        let turns = UInt32(truncatingIfNeeded: Int((degrees / 90).rounded())) & 3
        return desktopOrientationBits(screen.frame.size, turns)
        #elseif os(iOS)
        guard let scene = view?.window?.windowScene else { return fixedOrientation(UIScreen.main.bounds) }
        let kind: UInt32
        switch OrientationLock.bit(scene.effectiveGeometry.interfaceOrientation) {
        case 1: kind = 0
        case 4: kind = 1
        case 2: kind = 2
        case 8: kind = 3
        default: return fixedOrientation(scene.screen.bounds)
        }
        // Safari measures from landscape-primary on iPad, portrait-primary on iPhone.
        let turns = UIDevice.current.userInterfaceIdiom == .pad ? kind ^ 1 : kind
        return (16 | turns << 2 | kind) << 20
        #else
        return fixedOrientation((view?.window?.screen ?? UIScreen.main).bounds)
        #endif
    }

    #if os(macOS)
    static func desktopOrientationBits(_ size: CGSize, _ turns: UInt32) -> UInt32 {
        let naturalPortrait = turns.isMultiple(of: 2) ? size.height >= size.width : size.width >= size.height
        let kind = (turns + (naturalPortrait ? 0 : 1)) % 4
        return (16 | turns << 2 | kind) << 20
    }
    #endif

    private static func fixedOrientation(_ bounds: CGRect) -> UInt32 {
        (bounds.height >= bounds.width ? 16 : 17) << 20
    }

    // Same order as runner/src/viewport.rs's DisplayMode (media bits 11–13).
    static let displayModes = ["standalone", "browser", "fullscreen", "minimal-ui", "window-controls-overlay", "picture-in-picture", "tabbed"]
    static func displayMode(_ view: PlatformView?) -> UInt32 {
        #if os(macOS)
        if view?.window?.styleMask.contains(.fullScreen) == true { return 2 }
        return (view as? ExactView)?.controlsOverlay == true ? 4 : 0
        #else
        return 0
        #endif
    }

    /// Calls `changed` on the main queue when any setting changes; the
    /// caller holds the tokens and removes them.
    package static func observe(_ changed: @escaping () -> Void) -> [NSObjectProtocol] {
        #if os(macOS)
        let names = [NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, NSWorkspace.didWakeNotification, agentChanged]
        var tokens: [NSObjectProtocol] = names.map { center.addObserver(forName: $0, object: nil, queue: .main) { _ in changed() } }
        // The system appearance, as the application's effective appearance
        // (`systemDark`), which AppKit changes on the main thread.
        if let app = NSApp { tokens.append(app.observe(\.effectiveAppearance) { _, _ in changed() }) }
        tokens.append(NSWorkspace.shared.observe(\.isVoiceOverEnabled) { _, _ in
            DispatchQueue.main.async { changed() }
        })
        tokens.append(NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { _ in changed() })
        return tokens
        #else
        // A system appearance change reaches an app as a trait change (the
        // view reports it, ExactViewIOS) or, when made elsewhere, on return.
        let names = [UIAccessibility.voiceOverStatusDidChangeNotification, UIAccessibility.reduceMotionStatusDidChangeNotification, UIAccessibility.reduceTransparencyStatusDidChangeNotification,
                     UIAccessibility.darkerSystemColorsStatusDidChangeNotification, UIAccessibility.boldTextStatusDidChangeNotification,
                     UIApplication.didBecomeActiveNotification, agentChanged] + pointingDevices
        return names.map { center.addObserver(forName: $0, object: nil, queue: .main) { _ in changed() } }
        #endif
    }
    #if os(iOS)
    /// Whether a trackpad or mouse can point in this app: on an iPad, or on
    /// a Mac (an iPad app there, or Catalyst in either idiom). An iPhone's
    /// (AssistiveTouch) only sends touches, as WebKit answers there, and
    /// GameController, asked, starts its keyboard and mouse service (about
    /// 0.3 MB), so an iPhone never asks.
    private static let pointable: Bool = {
        #if targetEnvironment(macCatalyst)
        return true
        #else
        let idiom = UIDevice.current.userInterfaceIdiom
        return idiom == .pad || idiom == .mac || ProcessInfo.processInfo.isiOSAppOnMac
        #endif
    }()
    /// GameController's `GCMouse`, where a pointer can point. Loaded here,
    /// not linked: a link maps the framework and the six images it brings
    /// beyond UIKit's, and runs their initializers, before `main` in every
    /// iPhone app, which never asks (3 to 11 ms in the iOS 27 simulator).
    private static let mouse: AnyClass? = {
        guard pointable, dlopen("/System/Library/Frameworks/GameController.framework/GameController", RTLD_LAZY) != nil else { return nil }
        return NSClassFromString("GCMouse")
    }()
    /// A trackpad or mouse attached or removed changes `pointer` and `hover`
    /// (`GCMouseDidConnect` and `GCMouseDidDisconnect`, by their values).
    private static let pointingDevices: [Notification.Name] = mouse == nil ? [] : [Notification.Name("GCMouseDidConnectNotification"), Notification.Name("GCMouseDidDisconnectNotification")]
    #elseif !os(macOS)
    private static let pointingDevices: [Notification.Name] = []
    #endif
    package static func forget(_ tokens: [NSObjectProtocol]) {
        #if os(macOS)
        tokens.forEach { token in
            if let observation = token as? NSKeyValueObservation { observation.invalidate() } else {
                center.removeObserver(token)
                NotificationCenter.default.removeObserver(token)
            }
        }
        #else
        tokens.forEach(center.removeObserver)
        #endif
    }
}

extension TextEngine {
    /// Whether Bold Text weights a run in stack `family` (LLP 1115.000
    /// D12.3), as the kernel weights its runs: the setting on, and the
    /// stack the system's, a generic one or a list whose first member is a
    /// generic family (the catalog's first face, as a compiled or runtime
    /// list registered it). A Markdown piece's own weight takes the step its
    /// run's took.
    package func boldTextSteps(_ family: Int) -> Bool {
        guard DisplayPreferences.boldText else { return false }
        guard let faces = catalog[family] else { return family < 8 }
        return faces.first?.generic != nil
    }

    /// The kernel's step (`exact_kernel::bolder`), UIKit's as measured: 100
    /// to 200, 200 to 300, 300 to 400, 400 to 600, 500 to 700, 600 to 800,
    /// 700 and on to 900; between two hundreds, between their steps.
    package static func bolder(_ weight: Int) -> Int {
        let steps = [100, 200, 300, 400, 600, 700, 800, 900, 900, 900, 1000]
        let w = min(max(weight, 1), 1000), at = w / 100, rest = w % 100
        if at >= 10 { return 1000 }
        return steps[at] + (steps[at + 1] - steps[at]) * rest / 100
    }
}
