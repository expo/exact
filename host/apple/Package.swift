// swift-tools-version:5.9
// ExactCore owns the extension-safe runtime, batches and text measurement.
// ExactKit adds the presenters and app lifecycle; capabilities and executables
// compose it. build.mjs selects the app's Rust archive with EXACT_LIB_DIR/EXACT_LIB.
import PackageDescription
import Foundation

let libDir = ProcessInfo.processInfo.environment["EXACT_LIB_DIR"] ?? (Context.packageDirectory + "/../../target/host-dev")
let libName = ProcessInfo.processInfo.environment["EXACT_LIB"] ?? "caltrain_apple"

// The app's module launch parts. `build.mjs` generates this target when the
// manifest's `launch` names any; otherwise the empty default is used.
let launchParts = ProcessInfo.processInfo.environment["EXACT_LAUNCH_PARTS"] ?? "Sources/ExactLaunchParts"
// Under SwiftPM's native build system (the Linux cross build), the app's launch modules and ExactIOS, which imports
// them, compile at the app's own minimum, where Swift checks the modules' availability; the rest of the package
// compiles at the floor every app shares (EXACT_IOS_DEPLOYMENT). swiftc takes the last `-target`, and a module may
// import one compiled for an earlier target. Set by build.mjs for iOS and tvOS on Linux.
let launchPartsTarget = ProcessInfo.processInfo.environment["EXACT_LAUNCH_PARTS_TARGET"].flatMap { $0.isEmpty ? nil : $0 }

let composition = ProcessInfo.processInfo.environment["EXACT_APP_COMPOSITION"] ?? "embedded"
precondition(["embedded", "updating"].contains(composition), "EXACT_APP_COMPOSITION must be embedded or updating")

// The linked capabilities with a Swift half (LLP 1047.001 D4): each is its
// own target, which the composition depends on, and installs, only when the
// app links it. `build.mjs` names the app's (`EXACT_APPLE_LINK`, the
// capabilities' names, comma-separated, or `all`); a build it doesn't drive
// links them all, as development does.
let linkNames = ProcessInfo.processInfo.environment["EXACT_APPLE_LINK"] ?? "all"
func links(_ capability: String) -> Bool {
    linkNames == "all" || linkNames.split(separator: ",").contains { $0 == Substring(capability) }
}
let capabilities: [(name: String, target: String, define: String)] = [
    ("grouped_lists", "ExactGroupedLists", "EXACT_LINK_GROUPED_LISTS"),
    ("markdown", "ExactMarkdown", "EXACT_LINK_MARKDOWN"),
    ("surfaces", "ExactSurfaces", "EXACT_LINK_SURFACES"),
    ("drag", "ExactDrag", "EXACT_LINK_DRAG"),
    ("media_picker", "ExactMediaPicker", "EXACT_LINK_MEDIA_PICKER"),
    ("auth_session", "ExactAuthSession", "EXACT_LINK_AUTH_SESSION"),
    ("numerals", "ExactNumerals", "EXACT_LINK_NUMERALS"),
    ("tips", "ExactTips", "EXACT_LINK_TIPS"),
].filter { links($0.name) }

// `swift test` builds every target a package declares, and the two UIKit
// executables cannot build for macOS. EXACT_TESTS=1 narrows the package to
// what the tests need — the same environment-driven shape the composition
// above already uses — so a normal build is unchanged and a test build is
// quick. `bun host/apple/build.mjs --test` sets it.
let testing = ProcessInfo.processInfo.environment["EXACT_TESTS"] == "1"
// Inspect mode (LLP 1122 T2c, Inspect.swift) is a development build's: a
// production one (`EXACT_UPDATE_TRUST=production`) compiles none of it, nor
// the agent's private API (LLP 1035.003 §3): UIKit's alert views
// (`Agent.shownButton`), AppKit's events made in a window (`AgentMouseMac`),
// Core Animation's filter keys (`backdropRects`).
let devTools: [SwiftSetting] = ProcessInfo.processInfo.environment["EXACT_UPDATE_TRUST"] == "production" ? [] : [.define("EXACT_INSPECT")]

let gallery = ProcessInfo.processInfo.environment["EXACT_UPDATE_TRUST"] != "production"
    && (testing || ProcessInfo.processInfo.environment["EXACT_WIDGET_GALLERY"] == "1")
let gallerySettings: [SwiftSetting] = gallery ? [.define("EXACT_WIDGET_GALLERY")] : []

// Some tests call Caltrain's data functions, which only its archive answers.
let caltrainSettings: [SwiftSetting] = libName == "caltrain_apple" ? [.define("EXACT_LINKS_CALTRAIN")] : []

let widgetCore = ProcessInfo.processInfo.environment["EXACT_WIDGET_CORE"] == "1"
let sharedCore = ProcessInfo.processInfo.environment["EXACT_CORE_FRAMEWORK"]
let core: [Target] = [
    .systemLibrary(name: "CExact", path: "Sources/CExact"),
    // C, so its constructor runs before `main` and can read the process start and prewarm flag.
    .target(name: "CExactLaunch", path: "Sources/CExactLaunch"),
    sharedCore.map { .binaryTarget(name: "ExactCore", path: $0) } ?? .target(
        name: "ExactCore",
        dependencies: ["CExact", "CExactLaunch"],
        path: "Sources/ExactCore",
        swiftSettings: [.unsafeFlags(["-application-extension"])] + devTools,
        linkerSettings: (widgetCore ? [.unsafeFlags(["-Xlinker", "-force_load", "-Xlinker", libDir + "/lib" + libName + ".a"])] : [.unsafeFlags(["-L", libDir]), .linkedLibrary(libName)]) + [.linkedLibrary("c++"), .linkedLibrary("sqlite3"), .linkedFramework("ImageIO")]
    ),
    // The snapshot driver (LLP 1126 D4, D5): SwiftUI over a headless
    // session's node model, for WidgetKit's archive. Linked by an app with
    // entries and its extension, never by a presenter.
    .target(name: "ExactSnapshot", dependencies: ["ExactCore", "CExact"], path: "Sources/ExactSnapshot", swiftSettings: [.unsafeFlags(["-application-extension"])]),
    .target(name: "ExactKit", dependencies: [.target(name: "ExactCore"), .target(name: "CExact"), .target(name: "CExactLaunch")] + (gallery ? [.target(name: "ExactSnapshot")] : []), path: "Sources/ExactKit", swiftSettings: devTools + gallerySettings),
    .target(name: "ExactUpdates", dependencies: ["ExactKit", "CExact"], path: "Sources/ExactUpdates"),
    .target(name: "ExactGroupedLists", dependencies: ["ExactKit", "CExact"], path: "Sources/ExactGroupedLists"),
    .target(name: "ExactMarkdown", dependencies: ["ExactKit", "CExact"], path: "Sources/ExactMarkdown"),
    .target(name: "ExactSurfaces", dependencies: ["ExactKit", "CExact"], path: "Sources/ExactSurfaces"),
    .target(name: "ExactDrag", dependencies: ["ExactKit", "CExact"], path: "Sources/ExactDrag"),
    .target(name: "ExactMediaPicker", dependencies: ["ExactKit"], path: "Sources/ExactMediaPicker"),
    .target(name: "ExactAuthSession", dependencies: ["ExactKit"], path: "Sources/ExactAuthSession"),
    .target(name: "ExactNumerals", dependencies: ["ExactKit"], path: "Sources/ExactNumerals"),
    .target(name: "ExactTips", dependencies: ["ExactKit"], path: "Sources/ExactTips"),
    .target(name: "ExactLaunchParts", dependencies: ["ExactKit"], path: launchParts,
            swiftSettings: launchPartsTarget.map { [.unsafeFlags(["-target", $0])] } ?? []),
    .target(name: "ExactComposition", dependencies: [.target(name: "ExactKit")] + (composition == "updating" ? [.target(name: "ExactUpdates")] : [])
                + capabilities.map { .target(name: $0.target) },
            path: composition == "updating" ? "Sources/ExactUpdating" : "Sources/ExactEmbedded",
            swiftSettings: capabilities.map { .define($0.define) }),
]

let executables: [Target] = [
    .executableTarget(name: "ExactWatch", dependencies: ["ExactCore", "ExactSnapshot"], path: "Sources/ExactWatch", swiftSettings: devTools),
    .executableTarget(name: "ExactMac", dependencies: ["ExactKit", "ExactComposition", "ExactLaunchParts"], path: "Sources/ExactMac"),
    .executableTarget(name: "ExactIOS", dependencies: ["ExactKit", "ExactComposition", "ExactLaunchParts"], path: "Sources/ExactIOS",
                     swiftSettings: launchPartsTarget.map { [.unsafeFlags(["-target", $0])] } ?? []),
    .executableTarget(name: "ExactHostMac", dependencies: ["ExactKit", "ExactComposition"], path: "Sources/ExactHostMac"),
    .executableTarget(name: "ExactHostIOS", dependencies: ["ExactKit", "ExactComposition"], path: "Sources/ExactHostIOS"),
]

// Host behaviour no other check can see: a wheel event's routing, asserted
// as a decision rather than a drawn frame, so it needs no window, no run
// loop, and no clock (LLP 1033 D4a).
let tests: [Target] = [
    .target(name: "ExactPackage", path: "modules", exclude: ["ExactExtensions.swift", "ExactNativeModule.swift", "ExactNativeViews.swift", "ExactServices.swift", "ExactTamber.swift"], sources: ["ExactPackage.swift"]),
    .target(name: "SignInImplementation", dependencies: ["ExactPackage"], path: "tests/SignInImplementation"),
    .target(name: "ExactWidgetTimeline", dependencies: ["ExactCore"], path: "widget", exclude: ["Timeline.swift"], sources: ["WidgetTimelineWaiters.swift"]),
    // The sound arm's C mixer (LLP 1096 D8), rendered offline by the tests,
    // and its AVAudioEngine calls (ARC, cleaning up on a caught exception, as
    // the arm's build); the arm itself is a dylib build.mjs makes, never a product.
    .target(name: "ExactSoundRender", path: "soundarm", exclude: ["SoundArm.swift"], sources: ["sound_mixer.c", "sound_render.c", "sound_engine.m"], publicHeadersPath: ".", cSettings: [.unsafeFlags(["-fobjc-arc", "-fobjc-arc-exceptions"])]),
    // The video arm's media session coordinator (LLP 1098 D7), driven by the
    // tests over stand-in players; the arm itself is build.mjs's dylib.
    .target(name: "ExactNowPlaying", path: "videoarm", exclude: ["VideoArm.swift", "PictureInPicture.swift"], sources: ["NowPlaying.swift"]),
    // The share extension's Swift (LLP 1133.024 D4), which links no Exact runtime; build.mjs compiles the appex itself.
    .target(name: "ExactShare", path: "share"),
    .testTarget(name: "ExactKitTests", dependencies: ["ExactPackage", "SignInImplementation", "ExactWidgetTimeline", "ExactCore", "ExactSnapshot", "ExactKit", "ExactUpdates", "ExactGroupedLists", "ExactMarkdown", "ExactSurfaces", "ExactDrag", "ExactMediaPicker", "ExactAuthSession", "ExactNumerals", "ExactTips", "ExactSoundRender", "ExactNowPlaying", "ExactShare"], path: "tests/ExactKitTests",
                resources: [.copy("ColorFixtures"), .copy("SnapshotFixtures")], swiftSettings: devTools + gallerySettings + caltrainSettings),
]

// The iOS and tvOS deployment target every target here compiles for. SwiftPM takes it from `platforms`, not from
// `swift build --triple`'s OS version, so build.mjs passes an app's compiled target (`compiledTargets`: 17.0, or the
// app's own minimum when its launch modules compile inside this package as ExactLaunchParts) in
// EXACT_IOS_DEPLOYMENT. Never below the host's floor, 17.0.
let iosFloor: String = {
    let floor = [17, 0]
    guard let asked = ProcessInfo.processInfo.environment["EXACT_IOS_DEPLOYMENT"], !asked.isEmpty else { return "17.0" }
    let parts = asked.split(separator: ".").map { Int($0) ?? 0 }
    // `.iOS(_:)` takes two or three components; app.json may write "18".
    return parts.lexicographicallyPrecedes(floor) ? "17.0" : parts.count == 1 ? "\(asked).0" : asked
}()

let package = Package(
    name: "Exact",
    platforms: [.macOS(.v14), .iOS(iosFloor), .tvOS(iosFloor), .watchOS(.v10), .visionOS(.v1)],
    products: [.library(name: "ExactCore", type: widgetCore ? .dynamic : nil, targets: ["ExactCore"]), .library(name: "ExactSnapshot", type: widgetCore ? .static : nil, targets: ["ExactSnapshot"])] + (testing ? [.library(name: "ExactKit", targets: ["ExactKit"])] : [
        .library(name: "ExactKit", targets: ["ExactKit"]),
        .library(name: "ExactUpdates", targets: ["ExactUpdates"]),
        .library(name: "ExactGroupedLists", targets: ["ExactGroupedLists"]),
        .library(name: "ExactMarkdown", targets: ["ExactMarkdown"]),
        .library(name: "ExactSurfaces", targets: ["ExactSurfaces"]),
        .library(name: "ExactDrag", targets: ["ExactDrag"]),
        .library(name: "ExactMediaPicker", targets: ["ExactMediaPicker"]),
        .library(name: "ExactAuthSession", targets: ["ExactAuthSession"]),
        .library(name: "ExactNumerals", targets: ["ExactNumerals"]),
        .library(name: "ExactTips", targets: ["ExactTips"]),
        .executable(name: "ExactWatch", targets: ["ExactWatch"]),
        .executable(name: "ExactMac", targets: ["ExactMac"]),
        .executable(name: "ExactIOS", targets: ["ExactIOS"]),
        .executable(name: "ExactClip", targets: ["ExactIOS"]),
        .executable(name: "ExactHostMac", targets: ["ExactHostMac"]),
        .executable(name: "ExactHostIOS", targets: ["ExactHostIOS"]),
    ]),
    targets: core + (testing ? tests : executables)
)
