//! Device grants: one line admits a device, and every host's spelling of it
//! is derived from this table.
//!
//! @ref LLP 1069.008 D1 (`device.<web permission name> <strings key>`) / D2
//! (one table maps a name to every host's spelling) / D6 (the served web's
//! `Permissions-Policy`)
//!
//! A device grant is an exact2 grant, like `surface.read`: [`io_grants`]
//! strips it before `ibex2` parses the rest. A purpose is a strings key
//! (LLP 1060), never prose; CloudKit instead names an optional container
//! (LLP 1133.001.000 D3).
//!
//! `auth.*` lines (LLP 1069.006) are stripped the same way and checked by
//! `crate::auth`; the bake derives their associated domains (`reach.auth`).
//!
//! [`io_grants`]: crate::io_grants

use crate::sandbox::{self, Sandbox};

/// HealthKit on the Mac: its entitlement is restricted there, and Exact signs none (LLP 1030.005 D2).
const HEALTH: Sandbox = Sandbox::Refused(
    "HealthKit's entitlement is restricted on the Mac, and Exact signs none there",
);

/// One device a grant can name, and what each host calls it.
#[derive(Debug, PartialEq, Eq)]
pub struct Device {
    /// The web Permissions Registry name where one exists; a name the web
    /// lacks is a declared deviation (`web` is then `None`).
    pub name: &'static str,
    /// iOS `Info.plist` usage keys, each given the purpose text.
    pub ios: &'static [&'static str],
    /// iOS `Info.plist` keys set to true.
    pub ios_flags: &'static [&'static str],
    /// Android's permissions (`<uses-permission>`): the runtime ones a module
    /// asks for in one request (LLP 1133.002 D6), and the normal ones granted
    /// at install (a foreground service's, LLP 1133.033 D16). One the platform retired at an API level
    /// is written `<name> maxSdkVersion=<level>`, which the manifest
    /// declares as `android:maxSdkVersion` (LLP 1133.019 D4), omitting it
    /// when the app's minimum API is newer.
    pub android: &'static [&'static str],
    /// Android permissions derived beside `android` only where the app uses
    /// a module name that writes under the line (`"writes": true` in its
    /// declaration; LLP 1133.018 D11: `WRITE_CALENDAR` for `saveEvent`).
    pub android_writes: &'static [&'static str],
    /// macOS `Info.plist` usage keys.
    pub macos: &'static [&'static str],
    /// The macOS hardened-runtime entitlement: without it a notarised build is
    /// silently refused, whatever `Info.plist` says.
    pub hardened: Option<&'static str>,
    /// macOS signing entitlements only a provisioning profile authorizes
    /// (Apple TN3125), apart from `hardened`, which any signature carries:
    /// the Mac build signs them under a profile that allows them, and
    /// without one signs none (`host/apple/presence.mjs`, `macProvisioning`).
    pub macos_profile: &'static [&'static str],
    /// What the Mac App Store's sandbox asks for the row (LLP 1030.005 D2):
    /// its entitlements, the profile's keys, or a refusal. Unset only in a
    /// row that has not been audited, which a store build refuses.
    pub sandbox: Option<Sandbox>,
    /// The web `Permissions-Policy` feature, where the feature is policy-controlled.
    pub web: Option<&'static str>,
    /// Further `Permissions-Policy` features the row turns on beside `web`:
    /// motion's three sensors are three features (LLP 1133.013 D11).
    pub policy: &'static [&'static str],
    /// iOS signing entitlements the device needs beside its usage strings
    /// (LLP 1069.008.000 D2): HealthKit authorizes nothing without its own.
    pub ios_entitlements: &'static [&'static str],
    /// The value of each of `ios_entitlements` and `macos_profile` that is no
    /// boolean, by name: NFC's reader-session formats are an array (LLP
    /// 1133.035 D16 item 2).
    pub entitlement_values: &'static [(&'static str, &'static [&'static str])],
    /// A usage key written with this row's text when no granted row writes it
    /// (LLP 1069.008.000 D1): App Store Connect wants both HealthKit keys
    /// beside the entitlement, and iOS shows only the prompt an app asks for.
    pub companion: Option<&'static str>,
    /// Whether the row applies on tvOS, which bakes the iOS plan (D5).
    pub tvos: bool,
    /// An app granting it keeps no answers across launches (D7): its data
    /// must not sit in `exact.kept.*` outside the platform's protection.
    pub keeps_no_answers: bool,
    /// The row this one asks for less of (a declared deviation, as
    /// `geolocation-approximate` is of `geolocation`, LLP 1133.002 D7): an app
    /// grants one or the other, never both (`grant-twice`).
    pub narrows: Option<&'static str>,
    /// A row the platforms will not give beside this one (D13: no region is
    /// watched under approximate location): refused together (`grant-conflict`).
    pub conflicts: Option<&'static str>,
    /// Whether the line names a purpose: false for a row no platform asks
    /// the person about, written `device.<name>` alone (LLP 1133.017 D14).
    pub purpose: bool,
    /// iOS `UIBackgroundModes` entries the row needs (LLP 1069.008 D4).
    pub ios_modes: &'static [&'static str],
    /// The `BGTaskScheduler` identifiers the row permits, each after the
    /// bundle id and a dot in `BGTaskSchedulerPermittedIdentifiers`.
    pub ios_tasks: &'static [&'static str],
    /// The row another must stand beside (`grant-needs`, LLP 1133.033 D3):
    /// a capture in the background is a capture first, and background
    /// location is asked of an app with foreground location (LLP 1133.021 D13).
    pub needs: Option<&'static str>,
    /// The Android reader's foreground services the row declares in the
    /// manifest, each `<kind>:<type>` (`capture:microphone`; LLP 1133.033
    /// D2): a kind's `<service>` is declared with every type its rows give
    /// it, so a later row adds a type to a kind (LLP 1133.032 D11 item 3).
    pub android_services: &'static [&'static str],
    /// Android features by full name, required unless `required=false`
    /// follows the name (LLP 1133.022 D18; LLP 1133.037 D17).
    pub android_features: &'static [&'static str],
}

/// What a row has unless it says otherwise.
pub(crate) const ROW: Device = Device {
    name: "",
    ios: &[],
    ios_flags: &[],
    android: &[],
    android_writes: &[],
    macos: &[],
    hardened: None,
    macos_profile: &[],
    sandbox: None,
    web: None,
    policy: &[],
    ios_entitlements: &[],
    entitlement_values: &[],
    companion: None,
    tvos: true,
    keeps_no_answers: false,
    narrows: None,
    conflicts: None,
    purpose: true,
    ios_modes: &[],
    ios_tasks: &[],
    needs: None,
    android_services: &[],
    android_features: &[],
};

/// Every device grant. Adding a row is one entry here and no host code.
pub const DEVICES: &[Device] = &[
    // Permission metadata for an app's native module (LLP 1067). The
    // platform keys, permissions and features follow LLP 1133.022 D18;
    // the module owns the Bluetooth calls.
    Device {
        name: "bluetooth",
        sandbox: Some(Sandbox::Keys(&["device.bluetooth"])),
        ios: &["NSBluetoothAlwaysUsageDescription"],
        macos: &["NSBluetoothAlwaysUsageDescription"],
        web: Some("bluetooth"),
        android: &[
            "android.permission.BLUETOOTH maxSdkVersion=30",
            "android.permission.BLUETOOTH_CONNECT",
        ],
        android_features: &[
            "android.hardware.bluetooth_le required=false",
            "android.software.companion_device_setup",
        ],
        ..ROW
    },
    Device {
        name: "bluetooth-background",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        needs: Some("bluetooth"),
        ios_modes: &["bluetooth-central"],
        tvos: false,
        ..ROW
    },
    Device {
        name: "microphone",
        sandbox: Some(Sandbox::Keys(&["device.audio-input"])),
        ios: &["NSMicrophoneUsageDescription"],
        macos: &["NSMicrophoneUsageDescription"],
        hardened: Some("com.apple.security.device.audio-input"),
        web: Some("microphone"),
        android: &["android.permission.RECORD_AUDIO"],
        ..ROW
    },
    // @ref LLP 1133.033 D3 — a microphone capture keeps running while the app
    // is in the background. iOS keeps the process for the `audio` mode, and
    // the capture reads `ExactCapturesInBackground`, since other lines derive
    // `audio` too; Android runs it in the reader's `capture` service, whose
    // notification's text is the purpose. A Mac and the web need nothing,
    // and tvOS's recording cell is later.
    Device {
        name: "microphone-background",
        sandbox: Some(sandbox::NONE),
        ios_flags: &["ExactCapturesInBackground"],
        ios_modes: &["audio"],
        android: &[
            "android.permission.FOREGROUND_SERVICE",
            "android.permission.FOREGROUND_SERVICE_MICROPHONE",
        ],
        android_services: &["capture:microphone"],
        needs: Some("microphone"),
        tvos: false,
        ..ROW
    },
    // @ref LLP 1133.033 D16 — `@exact/calls`. A call keeps the process for
    // its active audio session on iOS (`audio`), and CallKit refuses a
    // transaction as `unentitled` without `voip` (measured on an iOS 27
    // simulator, 2026-10-27). On Android it runs in the reader's `call` service,
    // which needs `MANAGE_OWN_CALLS`; the purpose names the call
    // notification's channel. Every Android permission here is normal but
    // `USE_FULL_SCREEN_INTENT`, a special access the person can turn off.
    // No call service on a Mac, the web or tvOS.
    Device {
        name: "calls",
        sandbox: Some(sandbox::NONE),
        ios_modes: &["audio", "voip"],
        android: &[
            "android.permission.MANAGE_OWN_CALLS",
            "android.permission.FOREGROUND_SERVICE",
            "android.permission.FOREGROUND_SERVICE_PHONE_CALL",
            "android.permission.FOREGROUND_SERVICE_MICROPHONE",
            "android.permission.USE_FULL_SCREEN_INTENT",
        ],
        android_services: &["call:phoneCall", "call:microphone"],
        needs: Some("microphone"),
        tvos: false,
        ..ROW
    },
    // The camera, `@exact/camera`'s (LLP 1133.008 D6). Android's `CAMERA`
    // comes from opening one in the app (any use of `getUserMedia` under
    // this line, its `derives`), not from the line alone: the system camera
    // app needs none, and declared without being held it refuses the app the
    // camera app (`takePhotoWithCameraApp`), which iOS's usage key, the
    // line's, asks for. Its Android features are declared not required where
    // the permission is (app/build.gradle.kts).
    Device {
        name: "camera",
        sandbox: Some(Sandbox::Keys(&["device.camera"])),
        ios: &["NSCameraUsageDescription"],
        macos: &["NSCameraUsageDescription"],
        hardened: Some("com.apple.security.device.camera"),
        web: Some("camera"),
        ..ROW
    },
    Device {
        name: "geolocation",
        sandbox: Some(Sandbox::Keys(&["personal-information.location"])),
        ios: &["NSLocationWhenInUseUsageDescription"],
        macos: &[
            "NSLocationUsageDescription",
            "NSLocationWhenInUseUsageDescription",
        ],
        hardened: Some("com.apple.security.personal-information.location"),
        web: Some("geolocation"),
        android: &[
            "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.ACCESS_COARSE_LOCATION",
        ],
        ..ROW
    },
    // Approximate location (LLP 1133.002 D7): the same keys, and on iOS the
    // prompt opens with precise off (macOS has no such key).
    Device {
        name: "geolocation-approximate",
        sandbox: Some(Sandbox::Keys(&["personal-information.location"])),
        ios: &["NSLocationWhenInUseUsageDescription"],
        ios_flags: &["NSLocationDefaultAccuracyReduced"],
        // Coarse alone: a ceiling on Android, where fine can then never be asked (D7).
        android: &["android.permission.ACCESS_COARSE_LOCATION"],
        macos: &[
            "NSLocationUsageDescription",
            "NSLocationWhenInUseUsageDescription",
        ],
        hardened: Some("com.apple.security.personal-information.location"),
        web: Some("geolocation"),
        narrows: Some("geolocation"),
        ..ROW
    },
    // Background location, as geofencing needs it (LLP 1133.021 D13): iOS
    // asks with its own key beside the When In Use one; a Mac's location
    // authorization is one grant; Android adds the background permission and
    // the boot broadcast its geofences are re-armed by. Neither platform
    // watches a region under approximate location. No tvOS cell.
    Device {
        name: "geolocation-background",
        sandbox: Some(Sandbox::Keys(&["personal-information.location"])),
        ios: &["NSLocationAlwaysAndWhenInUseUsageDescription"],
        macos: &[
            "NSLocationUsageDescription",
            "NSLocationWhenInUseUsageDescription",
        ],
        hardened: Some("com.apple.security.personal-information.location"),
        android: &[
            "android.permission.ACCESS_BACKGROUND_LOCATION",
            "android.permission.RECEIVE_BOOT_COMPLETED",
        ],
        tvos: false,
        needs: Some("geolocation"),
        conflicts: Some("geolocation-approximate"),
        ..ROW
    },
    // iOS's prompt text is fixed and the web's is not policy-controlled; the
    // grant still gates the request. Android asks `POST_NOTIFICATIONS` from 13
    // and grants it at install before; a timed post is armed again after a
    // reboot, whose broadcast reaches only an app holding the boot permission,
    // a normal one (LLP 1133.004.000 D5).
    Device {
        name: "notifications",
        sandbox: Some(sandbox::NONE),
        ios: &[],
        macos: &[],
        hardened: None,
        web: None,
        android: &[
            "android.permission.POST_NOTIFICATIONS",
            "android.permission.RECEIVE_BOOT_COMPLETED",
        ],
        ..ROW
    },
    // A visible push (LLP 1133.004 §9 P2): the Push API's name. Nothing asks
    // about push itself (the prompt is notifications', which it needs), so no
    // purpose. The environment's value is the build's, by the signing
    // (`host/apple/presence.mjs`). A Mac spells the key with Apple's prefix
    // and takes it only under a provisioning profile.
    Device {
        name: "push",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        needs: Some("notifications"),
        ios_entitlements: &["aps-environment"],
        macos_profile: &["com.apple.developer.aps-environment"],
        tvos: false,
        ..ROW
    },
    // Deviation: the web has no name; its speech API is gated by `microphone`.
    Device {
        name: "speech-recognition",
        sandbox: Some(Sandbox::Keys(&[sandbox::CLIENT])),
        ios: &["NSSpeechRecognitionUsageDescription"],
        macos: &["NSSpeechRecognitionUsageDescription"],
        hardened: None,
        web: None,
        ..ROW
    },
    // Deviation, same reason: saving to the library. PhotoKit under the
    // hardened runtime needs the library entitlement for a write too. Apple
    // lists the add key for iOS alone and the read key for macOS too, so
    // macOS writes both (LLP 1133.019 D4).
    Device {
        name: "photos-add",
        sandbox: Some(Sandbox::Keys(&["personal-information.photos-library"])),
        ios: &["NSPhotoLibraryAddUsageDescription"],
        macos: &[
            "NSPhotoLibraryAddUsageDescription",
            "NSPhotoLibraryUsageDescription",
        ],
        hardened: Some("com.apple.security.personal-information.photos-library"),
        web: None,
        ..ROW
    },
    // Reading the library, `@exact/photos` (LLP 1133.019 D4): a deviation,
    // the web has no library. iOS never shows its automatic limited-access
    // alert, which a tool's or a wake's read would otherwise raise (D6). An
    // app reading it keeps no answers (D6, ruling 6). Android's read is a
    // permission per API range: `READ_EXTERNAL_STORAGE` to 32,
    // `READ_MEDIA_IMAGES` from 33, `READ_MEDIA_VISUAL_USER_SELECTED` (a
    // partial grant) from 34; and `ACCESS_MEDIA_LOCATION`, without which
    // another app's photo reads with its location redacted (D4). A level
    // ignores a permission it does not define, so only a range's end is
    // written; the implementation asks those its level has. Adding needs no
    // permission from API 29, the host's oldest.
    Device {
        name: "photos",
        sandbox: Some(Sandbox::Keys(&["personal-information.photos-library"])),
        ios: &["NSPhotoLibraryUsageDescription"],
        ios_flags: &["PHPhotoLibraryPreventAutomaticLimitedAccessAlert"],
        android: &[
            "android.permission.READ_EXTERNAL_STORAGE maxSdkVersion=32",
            "android.permission.READ_MEDIA_IMAGES",
            "android.permission.READ_MEDIA_VISUAL_USER_SELECTED",
            "android.permission.ACCESS_MEDIA_LOCATION",
        ],
        macos: &["NSPhotoLibraryUsageDescription"],
        hardened: Some("com.apple.security.personal-information.photos-library"),
        web: None,
        keeps_no_answers: true,
        ..ROW
    },
    // HealthKit (LLP 1069.008.000): Apple only, a deviation (the web has no
    // Health permission). Read and write are HealthKit's own share/update
    // prompts, one key each; each carries the other's key as its companion,
    // and the HealthKit entitlement on iOS. macOS writes the keys and signs
    // with no entitlement, which is restricted there (D5). Not on tvOS.
    Device {
        name: "health-read",
        sandbox: Some(HEALTH),
        ios: &["NSHealthShareUsageDescription"],
        macos: &["NSHealthShareUsageDescription"],
        ios_entitlements: &["com.apple.developer.healthkit"],
        companion: Some("NSHealthUpdateUsageDescription"),
        tvos: false,
        keeps_no_answers: true,
        ..ROW
    },
    Device {
        name: "health-write",
        sandbox: Some(HEALTH),
        ios: &["NSHealthUpdateUsageDescription"],
        macos: &["NSHealthUpdateUsageDescription"],
        ios_entitlements: &["com.apple.developer.healthkit"],
        companion: Some("NSHealthShareUsageDescription"),
        tvos: false,
        keeps_no_answers: true,
        ..ROW
    },
    // The person's address book, `@exact/contacts` (LLP 1133.018 D11): a
    // deviation, the web has no store. Choosing contacts needs no line (D4).
    // An app reading it keeps no answers (D8).
    Device {
        name: "contacts",
        sandbox: Some(Sandbox::Keys(&["personal-information.addressbook"])),
        ios: &["NSContactsUsageDescription"],
        macos: &["NSContactsUsageDescription"],
        hardened: Some("com.apple.security.personal-information.addressbook"),
        android: &["android.permission.READ_CONTACTS"],
        tvos: false,
        keeps_no_answers: true,
        ..ROW
    },
    // The person's calendars, `@exact/calendar` (D11): full access, and
    // `WRITE_CALENDAR` on Android only where the app writes (`android_writes`).
    // Neither calendar line narrows the other: Apple's path between them is
    // an upgrade, so an app may write both.
    Device {
        name: "calendar",
        sandbox: Some(Sandbox::Keys(&["personal-information.calendars"])),
        ios: &["NSCalendarsFullAccessUsageDescription"],
        macos: &["NSCalendarsFullAccessUsageDescription"],
        hardened: Some("com.apple.security.personal-information.calendars"),
        android: &["android.permission.READ_CALENDAR"],
        android_writes: &["android.permission.WRITE_CALENDAR"],
        tvos: false,
        keeps_no_answers: true,
        ..ROW
    },
    // Apple's write-only calendar access (iOS 17, macOS 14). Android has no
    // narrower grant: an insert names a calendar row, which only a read finds,
    // so the line derives both permissions there (D11).
    Device {
        name: "calendar-write-only",
        sandbox: Some(Sandbox::Keys(&["personal-information.calendars"])),
        ios: &["NSCalendarsWriteOnlyAccessUsageDescription"],
        macos: &["NSCalendarsWriteOnlyAccessUsageDescription"],
        hardened: Some("com.apple.security.personal-information.calendars"),
        android: &[
            "android.permission.READ_CALENDAR",
            "android.permission.WRITE_CALENDAR",
        ],
        tvos: false,
        keeps_no_answers: true,
        ..ROW
    },
    // @ref LLP 1133.036 D7 — local network privacy (TN3179): macOS 15 and
    // iOS 14 ask before the app first browses or reaches a LAN address, and
    // show this text. The `net.dns-sd` and `local-network` lines it requires
    // derive `NSBonjourServices`, `NSAllowsLocalNetworking` and Android's
    // multicast permission (`bake/src/reach.rs`). Deviation: the web's Local
    // Network Access prompt is the browser's own, and Android asks nothing
    // until the host targets API 37, so the row writes Apple keys only.
    Device {
        name: "local-network",
        sandbox: Some(Sandbox::Keys(&[sandbox::CLIENT])),
        ios: &["NSLocalNetworkUsageDescription"],
        macos: &["NSLocalNetworkUsageDescription"],
        ..ROW
    },
    // @ref LLP 1133.010 D12 — local authentication. iOS shows the purpose the
    // first time Face ID is used, and refuses Face ID without the key; Touch ID
    // and macOS ask nothing. A declared deviation: the web has no name (its
    // cell is `later`, over passkeys). Not on tvOS, which has no biometry.
    Device {
        name: "local-auth",
        sandbox: Some(sandbox::NONE),
        ios: &["NSFaceIDUsageDescription"],
        android: &["android.permission.USE_BIOMETRIC"],
        tvos: false,
        ..ROW
    },
    // @ref LLP 1125 D6 — Web MIDI's permission and policy feature. Apple asks
    // for nothing, so no usage key; tvOS has no CoreMIDI (`unavailable`).
    Device {
        name: "midi",
        sandbox: Some(sandbox::NONE),
        web: Some("midi"),
        tvos: false,
        ..ROW
    },
    // @ref LLP 1133.013 D11 — `@exact/motion`. A declared deviation: the
    // Registry has three names and one line grants all three, the
    // magnetometer's whatever the app reads. iOS and Android ask nothing for
    // these sensors (`NSMotionUsageDescription` is the activity and
    // pedometer APIs'), and tvOS has none of Core Motion's streams.
    Device {
        name: "motion",
        sandbox: Some(sandbox::NONE),
        web: Some("accelerometer"),
        policy: &["gyroscope", "magnetometer"],
        tvos: false,
        ..ROW
    },
    // Background work (LLP 1133.017 D14): nothing asks the person, so no
    // line names a purpose. The WICG specifications name the first two
    // (the Permissions Registry lists none of the three). The host keeps
    // one refresh identifier and two processing ones, by constraints (D8);
    // a `sync` waiting asks for both kinds. tvOS's refresh is later (D17):
    // its build writes none.
    Device {
        name: "background-sync",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        ios_modes: &["fetch", "processing"],
        ios_tasks: &["refresh", "processing.network", "processing.network-power"],
        ..ROW
    },
    Device {
        name: "periodic-background-sync",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        ios_modes: &["fetch"],
        ios_tasks: &["refresh"],
        ..ROW
    },
    Device {
        name: "background-fetch",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        ..ROW
    },
    // A login item and global shortcuts (LLP 1133.025 D10): nothing asks the
    // person, so the lines name no purpose; each is the app's statement of
    // what it may do, checked at bake. The installed Windows build's
    // `desktop:StartupTask` derives from `login-item`. Desktop only.
    Device {
        name: "login-item",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        tvos: false,
        ..ROW
    },
    Device {
        name: "global-shortcuts",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        tvos: false,
        ..ROW
    },
    // @ref LLP 1178 D4 — the displays and the pointer. Only the web asks:
    // the browser's own prompt, which shows no purpose, behind the
    // `window-management` policy feature. Natively nothing asks, and the line
    // is the app's statement that it reads where the displays are.
    Device {
        name: "window-management",
        sandbox: Some(sandbox::NONE),
        purpose: false,
        web: Some("window-management"),
        tvos: false,
        ..ROW
    },
    // @ref LLP 1178 D4 — a still of every display without a picker (LLP
    // 1133.032 D9's row). macOS asks once (Screen Recording, TCC), which
    // ScreenCaptureKit needs no sandbox entitlement for; Windows asks nothing.
    // The web has no standing capture permission, so the row has no policy
    // feature: `display-capture` is `getDisplayMedia`'s, a row of its own.
    Device {
        name: "screen-recording",
        sandbox: Some(sandbox::NONE),
        macos: &["NSScreenCaptureUsageDescription"],
        tvos: false,
        ..ROW
    },
    // App Attest and Play Integrity (LLP 1133.028 D6): nothing is shown or
    // asked. The entitlement's value, `development` or `production`, is the
    // build's to choose (`host/apple/build.mjs`); a Mac takes it only with a
    // profile (D17).
    Device {
        name: "attestation",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        ios_entitlements: &["com.apple.developer.devicecheck.appattest-environment"],
        macos_profile: &["com.apple.developer.devicecheck.appattest-environment"],
        ..ROW
    },
    // The stores' age signal (D10): the platform's sheet says what it asks,
    // and neither platform has a usage string. Not on tvOS.
    Device {
        name: "age-range",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        ios_entitlements: &["com.apple.developer.declared-age-range"],
        macos_profile: &["com.apple.developer.declared-age-range"],
        tvos: false,
        ..ROW
    },
    // Group Activities, for a package's SharePlay session (LLP 1133.001.000
    // D3): nothing is asked here, since SharePlay's own interface does the
    // asking. The entitlement is a Boolean (Apple, "Group Activities
    // Entitlement"); a Mac takes it only with a profile. Not on tvOS.
    Device {
        name: "shareplay",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        ios_entitlements: &["com.apple.developer.group-session"],
        macos_profile: &["com.apple.developer.group-session"],
        tvos: false,
        ..ROW
    },
    // The iCloud key-value store, for an app's own Swift module (expo/exact#663):
    // nothing is asked, and without the entitlement `NSUbiquitousKeyValueStore`
    // keeps values on the one device. Its value is the team prefix and the
    // bundle id, as `application-identifier` is (Apple, "Synchronizing app
    // preferences with iCloud"), which the Apple build derives at signing
    // (`host/apple/presence.mjs`); a Mac takes it only with a profile.
    Device {
        name: "icloud-kv",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        ios_entitlements: &["com.apple.developer.ubiquity-kvstore-identifier"],
        macos_profile: &["com.apple.developer.ubiquity-kvstore-identifier"],
        ..ROW
    },
    // LLP 1133.001.000 D3; expo/exact#531: one CloudKit container, with
    // values derived by the bake and Apple signing, without silent pushes.
    Device {
        name: "cloudkit",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        ios_entitlements: &[
            "com.apple.developer.icloud-services",
            "com.apple.developer.icloud-container-identifiers",
            "com.apple.developer.icloud-container-environment",
        ],
        macos_profile: &[
            "com.apple.developer.icloud-services",
            "com.apple.developer.icloud-container-identifiers",
            "com.apple.developer.icloud-container-environment",
        ],
        entitlement_values: &[("com.apple.developer.icloud-services", &["CloudKit"])],
        tvos: false,
        ..ROW
    },
    // Sensitive Content Analysis, for an app's own module that calls
    // `SCSensitivityAnalyzer` (expo/exact#708): nothing is asked and the
    // framework has no usage key. The analyzer works only in an app signed
    // with the client entitlement, an array holding `analysis` (Apple,
    // "com.apple.developer.sensitivecontentanalysis.client"); whether it
    // analyses is the person's own setting (`analysisPolicy`). iOS 17, and
    // macOS 14 under a profile that authorizes it; not on tvOS.
    Device {
        name: "sensitive-content-analysis",
        sandbox: Some(Sandbox::Profile),
        purpose: false,
        ios_entitlements: &["com.apple.developer.sensitivecontentanalysis.client"],
        macos_profile: &["com.apple.developer.sensitivecontentanalysis.client"],
        entitlement_values: &[(
            "com.apple.developer.sensitivecontentanalysis.client",
            &["analysis"],
        )],
        tvos: false,
        ..ROW
    },
    // App Tracking Transparency (D13): iOS and tvOS alone; Apple lists the
    // key for no Mac.
    Device {
        name: "tracking",
        sandbox: Some(sandbox::NONE),
        ios: &["NSUserTrackingUsageDescription"],
        ..ROW
    },
    // Alarms, `@exact/alarms`'s (LLP 1133.030 D7), a declared deviation: no
    // web name. AlarmKit schedules nothing without its key. On Android the
    // package owns the app's one exact-alarm permission, needs notifications
    // for the ring's Stop, and runs receivers and a ringing service at a
    // boot and at the alarm's time; macOS, tvOS and the web have no alarm.
    Device {
        name: "alarms",
        sandbox: Some(sandbox::NONE),
        ios: &["NSAlarmKitUsageDescription"],
        android: &[
            "android.permission.SCHEDULE_EXACT_ALARM",
            "android.permission.POST_NOTIFICATIONS",
            "android.permission.RECEIVE_BOOT_COMPLETED",
            "android.permission.WAKE_LOCK",
            "android.permission.FOREGROUND_SERVICE",
            "android.permission.FOREGROUND_SERVICE_SYSTEM_EXEMPTED",
        ],
        tvos: false,
        ..ROW
    },
    // NFC, `@exact/nfc`'s (LLP 1133.035 D15): iOS asks nothing, yet ends an
    // app that reads a tag without the usage key, and reads none without the
    // reader-session entitlement, whose value lists the formats (`TAG`, the
    // one Apple documents). Android's permission is a normal one, granted at
    // install. Web NFC has no policy feature; Core NFC is on neither the Mac
    // nor the TV.
    Device {
        name: "nfc",
        sandbox: Some(sandbox::NONE),
        ios: &["NFCReaderUsageDescription"],
        android: &["android.permission.NFC"],
        ios_entitlements: &["com.apple.developer.nfc.readersession.formats"],
        entitlement_values: &[("com.apple.developer.nfc.readersession.formats", &["TAG"])],
        tvos: false,
        ..ROW
    },
    // @ref LLP 1133.037 D17 — Web Serial, WebHID and WebUSB. No platform
    // shows a purpose for any of them, so no line names one. The web's
    // policy features are the APIs' names. Android declares USB host mode,
    // not required, for serial and USB; HID is absent there (D11). macOS
    // derives nothing while the app is not sandboxed. No device API exists
    // for apps on iOS or tvOS (D21).
    Device {
        name: "serial",
        sandbox: Some(Sandbox::Keys(&["device.serial"])),
        web: Some("serial"),
        purpose: false,
        tvos: false,
        android_features: &["android.hardware.usb.host required=false"],
        ..ROW
    },
    Device {
        name: "hid",
        sandbox: Some(Sandbox::Keys(&["device.usb", "device.bluetooth"])),
        web: Some("hid"),
        purpose: false,
        tvos: false,
        ..ROW
    },
    Device {
        name: "usb",
        sandbox: Some(Sandbox::Keys(&["device.usb"])),
        web: Some("usb"),
        purpose: false,
        tvos: false,
        android_features: &["android.hardware.usb.host required=false"],
        ..ROW
    },
];

/// One device line, with its purpose key or CloudKit container.
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceGrant<'a> {
    /// The table's row.
    pub device: &'static Device,
    /// The strings key whose text is the purpose; empty for a row that
    /// names none.
    pub purpose: &'a str,
    /// CloudKit's explicit container, or none to derive `iCloud.<bundle id>`.
    pub container: Option<&'a str>,
}

/// Whether a grant line is a device grant (the runner's own family).
pub fn is_device_line(line: &str) -> bool {
    line.trim_start().starts_with("device.")
}

/// Every device grant in `spec`, or every malformed one, each reported.
pub fn device_grants(spec: &str) -> Result<Vec<DeviceGrant<'_>>, Vec<String>> {
    checked(spec).map(|all| {
        (all.into_iter())
            .map(|g| DeviceGrant {
                device: &DEVICES[g.row],
                purpose: g.purpose,
                container: g.container,
            })
            .collect()
    })
}

/// What the runner reads of a row: its name and what checking a line asks
/// of it. An app links this, not the table's build facts, which are read
/// where a build or a server derives a host's spelling (the bake, the CLI,
/// the served web's `Permissions-Policy`).
struct Rule {
    name: &'static str,
    purpose: bool,
    keeps_no_answers: bool,
    needs: Option<&'static str>,
    conflicts: Option<&'static str>,
    narrows: Option<&'static str>,
}

static RULES: [Rule; DEVICES.len()] = {
    let mut rules = [const {
        Rule {
            name: "",
            purpose: false,
            keeps_no_answers: false,
            needs: None,
            conflicts: None,
            narrows: None,
        }
    }; DEVICES.len()];
    let mut i = 0;
    while i < DEVICES.len() {
        let d = &DEVICES[i];
        rules[i] = Rule {
            name: d.name,
            purpose: d.purpose,
            keeps_no_answers: d.keeps_no_answers,
            needs: d.needs,
            conflicts: d.conflicts,
            narrows: d.narrows,
        };
        i += 1;
    }
    rules
};

/// A checked device line: its row in [`DEVICES`] and [`RULES`].
struct Checked<'a> {
    row: usize,
    purpose: &'a str,
    container: Option<&'a str>,
}

/// [`device_grants`] over [`RULES`].
fn checked(spec: &str) -> Result<Vec<Checked<'_>>, Vec<String>> {
    let (mut grants, mut errors) = (Vec::<Checked>::new(), Vec::new());
    for line in spec.lines().map(str::trim).filter(|l| is_device_line(l)) {
        let mut words = line.split_whitespace();
        let name = words
            .next()
            .unwrap_or_default()
            .trim_start_matches("device.");
        let Some(row) = RULES.iter().position(|d| d.name == name) else {
            let known: Vec<_> = RULES.iter().map(|d| d.name).collect();
            errors.push(format!(
                "`device.{name}` is not a device grant; the table has {}",
                known.join(", ")
            ));
            continue;
        };
        if name == "cloudkit" {
            let container = words.next();
            let valid = container.is_none_or(|c| {
                c.strip_prefix("iCloud.").is_some_and(|id| {
                    id.split('.').all(|label| {
                        !label.is_empty()
                            && !label.starts_with('-')
                            && !label.ends_with('-')
                            && label
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    })
                })
            });
            if !valid || words.next().is_some() {
                errors.push(format!(
                    "`{line}`: write `device.cloudkit` or `device.cloudkit iCloud.<name>`; the container is one identifier, with no purpose string"
                ));
            } else if let Some(have) = grants.iter().find(|g| RULES[g.row].name == name) {
                if have.container != container {
                    errors.push(format!(
                        "`device.cloudkit` names two containers, `{}` and `{}`; only one container is supported",
                        have.container.unwrap_or("iCloud.<bundle id>"),
                        container.unwrap_or("iCloud.<bundle id>")
                    ));
                }
            } else {
                grants.push(Checked {
                    row,
                    purpose: "",
                    container,
                });
            }
            continue;
        }
        if line.contains(['"', '\'']) {
            errors.push(format!(
                "`{line}`: the purpose is a strings key (LLP 1060), not a literal: `device.{name} purpose.{name}`, with `\"purpose.{name}\": \"…\"` in strings/en.json"
            ));
            continue;
        }
        if !RULES[row].purpose {
            match words.next() {
                Some(word) => errors.push(format!(
                    "`{line}`: nothing asks the person for `device.{name}`, so its line names no purpose: write `device.{name}` (not `{word}`)"
                )),
                None if grants.iter().any(|g| RULES[g.row].name == name) => {}
                None => grants.push(Checked {
                    row,
                    purpose: "",
                    container: None,
                }),
            }
            continue;
        }
        let (Some(purpose), None) = (words.next(), words.next()) else {
            errors.push(format!(
                "`{line}`: a device grant names one strings key, as in `device.{name} purpose.{name}`, whose text in strings/en.json (`\"purpose.{name}\": \"…\"`) is the purpose the person is shown"
            ));
            continue;
        };
        match grants.iter().find(|g| RULES[g.row].name == name) {
            Some(have) if have.purpose != purpose => errors.push(format!(
                "`device.{name}` names two purposes, `{}` and `{purpose}`",
                have.purpose
            )),
            Some(_) => {}
            None => grants.push(Checked {
                row,
                purpose,
                container: None,
            }),
        }
    }
    for g in &grants {
        let rule = &RULES[g.row];
        if let Some(needed) = rule.needs {
            if !grants.iter().any(|h| RULES[h.row].name == needed) {
                errors.push(format!(
                    "`device.{}` needs `device.{needed} <purposeKey>` beside it (grant-needs)",
                    rule.name
                ));
            }
        }
        if let Some(other) = rule.conflicts {
            if grants.iter().any(|h| RULES[h.row].name == other) {
                errors.push(format!(
                    "`device.{}` and `device.{other}` both: no platform gives the first beside the second (grant-conflict)",
                    rule.name
                ));
            }
        }
        if let Some(wider) = rule.narrows {
            if grants.iter().any(|h| RULES[h.row].name == wider) {
                errors.push(format!(
                    "`device.{}` and `device.{wider}` both: write the one the app needs (grant-twice)",
                    rule.name
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(grants)
    } else {
        Err(errors)
    }
}

/// Whether `spec` grants the device `name`: what a first-party capability
/// reads before it acts, refusing by name without it (D3).
pub fn granted(spec: &str, name: &str) -> bool {
    checked(spec).is_ok_and(|all| all.iter().any(|g| RULES[g.row].name == name))
}

/// Whether `spec` grants a device whose app keeps no answers
/// (LLP 1069.008.000 D7).
pub fn keeps_no_answers(spec: &str) -> bool {
    checked(spec).is_ok_and(|all| all.iter().any(|g| RULES[g.row].keeps_no_answers))
}

/// The served web's `Permissions-Policy` (D6): `feature=(self)` for each
/// granted row with a web name, `feature=()` for every other one the table
/// has. Built from the table's own names, so no app text reaches the header.
pub fn permissions_policy(spec: &str) -> String {
    let granted = device_grants(spec).unwrap_or_default();
    // A feature two rows share (`geolocation`) is named once: on when either is granted.
    let mut features: Vec<&str> = Vec::new();
    let of = |d: &'static Device| d.web.into_iter().chain(d.policy.iter().copied());
    for feature in DEVICES.iter().flat_map(of) {
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    features
        .into_iter()
        .map(|feature| {
            let on = granted.iter().any(|g| of(g.device).any(|f| f == feature));
            format!("{feature}=({})", if on { "self" } else { "" })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_background_needs_bluetooth_and_has_no_purpose_of_its_own() {
        let spec = "device.bluetooth purpose.strap\ndevice.bluetooth-background";
        let grants = device_grants(spec).unwrap();
        assert_eq!(grants[0].purpose, "purpose.strap");
        assert_eq!(
            sandbox::device(grants[0].device),
            Sandbox::Keys(&["device.bluetooth"])
        );
        assert!(permissions_policy(spec).contains("bluetooth=(self)"));
        assert_eq!(grants[1].device.ios_modes, ["bluetooth-central"]);
        assert!(grants[1].purpose.is_empty());
        for bad in [
            "device.bluetooth",
            "device.bluetooth purpose.strap extra",
            "device.bluetooth-background purpose.strap",
            "device.bluetooth-background",
        ] {
            assert!(device_grants(bad).is_err(), "{bad}");
        }
        assert!(
            device_grants("device.bluetooth-background").unwrap_err()[0].contains("grant-needs")
        );
    }

    /// LLP 1133.002 D7: the approximate line is a row of its own, granting
    /// the same web feature; an app writes it or the precise one, not both.
    #[test]
    fn approximate_location_is_its_own_line_and_never_beside_the_precise_one() {
        let one = "device.geolocation-approximate locationPurpose";
        assert!(granted(one, "geolocation-approximate") && !granted(one, "geolocation"));
        let policy = permissions_policy(one);
        assert_eq!(policy.matches("geolocation=").count(), 1, "{policy}");
        assert!(policy.contains("geolocation=(self)"), "{policy}");
        let both = format!("{one}\ndevice.geolocation locationPurpose");
        let errors = device_grants(&both).unwrap_err();
        assert!(errors[0].contains("(grant-twice)"), "{errors:?}");
    }

    /// LLP 1133.004 §9 P2: a push line names no purpose and stands beside the
    /// notifications line, whose authorization it shows under.
    #[test]
    fn push_needs_notifications_and_names_no_purpose() {
        let ok = device_grants("device.notifications purpose.notifications\ndevice.push").unwrap();
        let push = ok.iter().find(|g| g.device.name == "push").unwrap();
        assert!(push.purpose.is_empty());
        assert_eq!(push.device.ios_entitlements, ["aps-environment"]);
        assert_eq!(
            push.device.macos_profile,
            ["com.apple.developer.aps-environment"]
        );
        assert!(
            !push.device.tvos && push.device.hardened.is_none() && push.device.macos.is_empty()
        );
        let alone = device_grants("device.push").unwrap_err();
        assert!(alone[0].contains("(grant-needs)"), "{alone:?}");
        let named = device_grants("device.notifications n\ndevice.push purpose.push").unwrap_err();
        assert!(named[0].contains("names no purpose"), "{named:?}");
    }

    /// LLP 1133.004.000 D5: Android's notifications line derives the runtime
    /// permission and the boot permission a timed post is armed again with.
    #[test]
    fn notifications_derive_the_post_and_boot_permissions_on_android() {
        let grants = device_grants("device.notifications n").unwrap();
        assert_eq!(
            grants[0].device.android,
            [
                "android.permission.POST_NOTIFICATIONS",
                "android.permission.RECEIVE_BOOT_COMPLETED"
            ]
        );
    }

    /// LLP 1133.021 D13: background location adds to precise location, and
    /// is refused without it and beside the approximate line.
    #[test]
    fn background_location_needs_precise_location() {
        let ok = "device.geolocation where\ndevice.geolocation-background arrival";
        assert!(granted(ok, "geolocation-background") && granted(ok, "geolocation"));
        let alone = device_grants("device.geolocation-background arrival").unwrap_err();
        assert!(alone[0].contains("(grant-needs)"), "{alone:?}");
        let approximate =
            "device.geolocation-approximate where\ndevice.geolocation-background arrival";
        let errors = device_grants(approximate).unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("(grant-needs)")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("(grant-conflict)")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_device_line_names_a_row_and_one_key() {
        let spec = "net.fetch https://x/\ndevice.microphone purpose.mic\ndevice.speech-recognition purpose.speech";
        let all = device_grants(spec).unwrap();
        assert_eq!(
            all.iter()
                .map(|g| (g.device.name, g.purpose))
                .collect::<Vec<_>>(),
            [
                ("microphone", "purpose.mic"),
                ("speech-recognition", "purpose.speech")
            ]
        );
        assert!(granted(spec, "microphone") && !granted(spec, "camera"));
        let errors = device_grants(
            "device.lidar purpose.x\ndevice.camera\ndevice.camera \"Takes photos.\"\ndevice.microphone a b\ndevice.geolocation a\ndevice.geolocation b",
        )
        .unwrap_err();
        assert_eq!(errors.len(), 5, "{errors:?}");
        assert!(errors[0].contains("not a device grant"));
        assert!(errors[2].contains("not a literal"));
        assert!(errors[4].contains("two purposes"));
        // A row nothing asks about names no purpose (LLP 1133.017 D14).
        let quiet =
            device_grants("device.background-sync\ndevice.periodic-background-sync").unwrap();
        assert!(quiet.iter().all(|g| g.purpose.is_empty()));
        let errors = device_grants("device.background-fetch purpose.x").unwrap_err();
        assert!(errors[0].contains("names no purpose"), "{errors:?}");
        // LLP 1133.028 D6, D10, D13: attestation and the age range ask
        // nothing; tracking names the alert's purpose.
        let rows =
            device_grants("device.attestation\ndevice.age-range\ndevice.tracking purpose.ads")
                .unwrap();
        assert_eq!(
            rows.iter()
                .map(|g| (g.device.name, g.purpose))
                .collect::<Vec<_>>(),
            [
                ("attestation", ""),
                ("age-range", ""),
                ("tracking", "purpose.ads")
            ]
        );
        assert!(!rows[1].device.tvos && rows[0].device.tvos && rows[2].device.tvos);
    }

    /// LLP 1133.001.000 D3: SharePlay names no purpose and signs Group
    /// Activities' Boolean entitlement, on iOS and under a Mac profile.
    #[test]
    fn shareplay_signs_the_group_session_entitlement() {
        let rows = device_grants("device.shareplay").unwrap();
        let row = rows[0].device;
        assert!(rows[0].purpose.is_empty());
        assert_eq!(row.ios_entitlements, ["com.apple.developer.group-session"]);
        assert!(row.entitlement_values.is_empty(), "a Boolean");
        assert_eq!(row.macos_profile, ["com.apple.developer.group-session"]);
        assert!(!row.tvos && row.ios.is_empty() && row.macos.is_empty() && row.web.is_none());
        let errors = device_grants("device.shareplay purpose.play").unwrap_err();
        assert!(errors[0].contains("names no purpose"), "{errors:?}");
    }

    /// The iCloud key-value store names no purpose and signs one entitlement,
    /// whose value the Apple build derives, on iOS, tvOS and under a Mac profile.
    #[test]
    fn icloud_kv_signs_the_key_value_store_entitlement() {
        let rows = device_grants("device.icloud-kv").unwrap();
        let row = rows[0].device;
        let key = "com.apple.developer.ubiquity-kvstore-identifier";
        assert!(rows[0].purpose.is_empty());
        assert_eq!(row.ios_entitlements, [key]);
        assert!(row.entitlement_values.is_empty(), "derived at signing");
        assert_eq!(row.macos_profile, [key]);
        assert!(row.tvos && row.ios.is_empty() && row.android.is_empty() && row.web.is_none());
        let errors = device_grants("device.icloud-kv purpose.sync").unwrap_err();
        assert!(errors[0].contains("names no purpose"), "{errors:?}");
    }

    #[test]
    fn cloudkit_signs_one_container_without_requesting_push_or_a_purpose() {
        let keys = [
            "com.apple.developer.icloud-services",
            "com.apple.developer.icloud-container-identifiers",
            "com.apple.developer.icloud-container-environment",
        ];
        for line in [
            "device.cloudkit",
            "device.cloudkit iCloud.com.example.boards",
        ] {
            let rows = device_grants(line).unwrap();
            let row = rows[0].device;
            assert!(rows[0].purpose.is_empty());
            assert_eq!(rows[0].container, line.split_whitespace().nth(1));
            assert_eq!(row.ios_entitlements, keys);
            assert_eq!(row.macos_profile, keys);
            assert!(!row.tvos && row.ios.is_empty() && row.macos.is_empty());
            assert!(row.ios_modes.is_empty() && row.needs.is_none());
            assert_eq!(device_grants(&format!("{line}\n{line}")).unwrap().len(), 1);
        }
        let errors = device_grants(
            "device.cloudkit iCloud.com.example.one\ndevice.cloudkit iCloud.com.example.two",
        )
        .unwrap_err();
        assert!(errors[0].contains("iCloud.com.example.one"), "{errors:?}");
        assert!(errors[0].contains("iCloud.com.example.two"), "{errors:?}");
        for invalid in [
            "device.cloudkit purpose.sync",
            "device.cloudkit iCloud.",
            "device.cloudkit iCloud.com..example",
            "device.cloudkit iCloud.com.example other",
        ] {
            assert!(device_grants(invalid).is_err(), "{invalid}");
        }
    }

    /// expo/exact#708: Sensitive Content Analysis names no purpose and signs
    /// its client entitlement, valued `["analysis"]`, on iOS and under a Mac
    /// profile; not on tvOS.
    #[test]
    fn sensitive_content_analysis_signs_the_client_entitlement() {
        let rows = device_grants("device.sensitive-content-analysis").unwrap();
        let row = rows[0].device;
        let key = "com.apple.developer.sensitivecontentanalysis.client";
        assert!(rows[0].purpose.is_empty());
        assert_eq!(row.ios_entitlements, [key]);
        assert_eq!(row.macos_profile, [key]);
        assert_eq!(row.entitlement_values, [(key, &["analysis"][..])]);
        assert!(!row.tvos && row.ios.is_empty() && row.macos.is_empty());
        assert!(row.android.is_empty() && row.web.is_none());
        let errors = device_grants("device.sensitive-content-analysis purpose.nudity").unwrap_err();
        assert!(errors[0].contains("names no purpose"), "{errors:?}");
    }

    #[test]
    fn health_is_two_rows_that_keep_no_answers() {
        // LLP 1069.008.000 D1, D7.
        let spec = "device.health-read purpose.read\ndevice.health-write purpose.write";
        let all = device_grants(spec).unwrap();
        assert_eq!(
            all.iter().map(|g| g.device.name).collect::<Vec<_>>(),
            ["health-read", "health-write"]
        );
        assert!(all.iter().all(|g| !g.device.tvos && g.device.web.is_none()));
        assert!(keeps_no_answers("device.health-read purpose.read"));
        assert!(!keeps_no_answers("device.microphone purpose.mic"));
        assert!(
            device_grants("device.health purpose.read purpose.write").is_err(),
            "the fork's one row with two keys"
        );
    }

    #[test]
    fn local_network_writes_the_apple_key_and_no_web_policy() {
        let all = device_grants("device.local-network purpose.lan").unwrap();
        let row = all[0].device;
        assert_eq!(row.ios, ["NSLocalNetworkUsageDescription"]);
        assert_eq!(row.macos, ["NSLocalNetworkUsageDescription"]);
        assert!(row.web.is_none() && row.android.is_empty() && row.hardened.is_none() && row.tvos);
        assert!(!permissions_policy("device.local-network purpose.lan").contains("local"));
    }

    /// LLP 1178 D4: the display list's line names no purpose and turns on
    /// the web's policy feature; the capture's names one and writes the Mac's key.
    #[test]
    fn window_management_and_screen_recording_are_desktop_lines() {
        let all = device_grants("device.window-management").unwrap();
        assert!(!all[0].device.purpose && !all[0].device.tvos);
        assert!(permissions_policy("device.window-management").contains("window-management=(self)"));
        assert!(device_grants("device.window-management purpose.screens").is_err());
        let all = device_grants("device.screen-recording purpose.shot").unwrap();
        assert_eq!(all[0].device.macos, ["NSScreenCaptureUsageDescription"]);
        assert!(all[0].device.web.is_none() && all[0].device.ios.is_empty());
        assert!(device_grants("device.screen-recording").is_err());
    }

    #[test]
    fn the_policy_allows_what_is_granted_and_denies_the_rest() {
        assert_eq!(
            permissions_policy("device.microphone p\ndevice.speech-recognition q"),
            "bluetooth=(), microphone=(self), camera=(), geolocation=(), midi=(), accelerometer=(), gyroscope=(), magnetometer=(), window-management=(), serial=(), hid=(), usb=()"
        );
        assert_eq!(
            permissions_policy(""),
            "bluetooth=(), microphone=(), camera=(), geolocation=(), midi=(), accelerometer=(), gyroscope=(), magnetometer=(), window-management=(), serial=(), hid=(), usb=()"
        );
    }

    /// LLP 1133.033 D3: the background line stands beside the capture's own.
    #[test]
    fn a_capture_in_the_background_needs_the_microphone_line() {
        let errors = device_grants("device.microphone-background purpose.bg").unwrap_err();
        assert!(
            errors[0].contains("needs `device.microphone <purposeKey>` beside it (grant-needs)"),
            "{errors:?}"
        );
        let both =
            device_grants("device.microphone purpose.mic\ndevice.microphone-background purpose.bg")
                .unwrap();
        let row = both[1].device;
        assert_eq!(row.ios_modes, ["audio"]);
        assert_eq!(row.ios_flags, ["ExactCapturesInBackground"]);
        assert_eq!(row.android_services, ["capture:microphone"]);
        assert!(row.web.is_none() && !row.tvos && row.macos.is_empty());
    }

    /// LLP 1133.013 D11: one line turns on the three sensors' features.
    #[test]
    fn motion_is_one_line_for_three_policy_features() {
        let policy = permissions_policy("device.motion purpose.motion");
        for feature in ["accelerometer", "gyroscope", "magnetometer"] {
            assert!(policy.contains(&format!("{feature}=(self)")), "{policy}");
        }
        assert!(policy.contains("microphone=()"), "{policy}");
    }
}
