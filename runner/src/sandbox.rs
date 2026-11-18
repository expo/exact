//! The App Sandbox column: what each grant line asks of the Mac App Store's
//! sandbox, so a store build signs exactly those entitlements and refuses,
//! by name, a line the sandbox cannot spell.
//!
//! @ref LLP 1030.005 D2 (every grant and host capability names its sandbox
//! entitlement; a row with no value is refused in a store build)
//!
//! Keys are written without their `com.apple.security.` prefix ([`PREFIX`]),
//! as Apple's "App Sandbox" entitlement list names them. A Developer ID build
//! reads none of this and keeps every grant.

use crate::device::{Device, DEVICES};

/// What one grant line or device row asks of the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sandbox {
    /// These entitlements, each without [`PREFIX`]; none when empty.
    Keys(&'static [&'static str]),
    /// Nothing of the sandbox: the provisioning profile's keys (the row's
    /// `macos_profile`, `auth.passkey`'s domains, Sign in with Apple's and
    /// Apple Pay's), which a store build always embeds a profile for.
    Profile,
    /// Refused in a store build, for this reason.
    Refused(&'static str),
}

/// Nothing: the line reaches only the container, the bundle, or a service the
/// sandbox does not gate.
pub const NONE: Sandbox = Sandbox::Keys(&[]);
/// The prefix every sandbox key is written without.
pub const PREFIX: &str = "com.apple.security.";
/// What every sandboxed app carries.
pub const APP: &str = "app-sandbox";
/// Outgoing connections: `URLSession`, a web view's remote page, Bonjour's browser.
pub const CLIENT: &str = "network.client";
/// A file or folder the person chose in an open or save panel, or dropped.
pub const USER_SELECTED: &str = "files.user-selected.read-write";

const CLIENT_ONLY: Sandbox = Sandbox::Keys(&[CLIENT]);
const OUTSIDE: &str = "the Mac App Store sandbox reaches no path outside the app's container, its bundle, supported well-known folders and the documents the person chooses; only a temporary-exception entitlement spells another path";

/// What a device row asks of the sandbox: its column, or a refusal for a row
/// that leaves it unset, as a row a package adds may (D2: the audit stays a
/// closed set).
pub fn device(row: &Device) -> Sandbox {
    row.sandbox.unwrap_or(Sandbox::Refused(
        "the row names no sandbox entitlement: a device row fills the sandbox column before a store build takes it (LLP 1030.005 D2)",
    ))
}

/// What one trimmed grant line asks of the sandbox. A family this does not
/// name is refused, so a family added later is audited before a store build
/// takes it.
pub fn line(line: &str) -> Sandbox {
    let mut words = line.split_whitespace();
    let family = words.next().unwrap_or_default();
    let target = words.next().unwrap_or_default();
    match family {
        "net.fetch" | "net.websocket" | "auth.session" | "auth.google" => CLIENT_ONLY,
        // LLP 1133.005 D9: the loopback receiver listens for the browser's redirect.
        "auth.callback"
            if target.starts_with("http://127.0.0.1/") || target.starts_with("http://[::1]/") =>
        {
            Sandbox::Keys(&[CLIENT, "network.server"])
        }
        "auth.callback" => CLIENT_ONLY,
        "auth.passkey" | "auth.apple" | "payment.apple-pay" => Sandbox::Profile,
        // `@exact/local-network` browses (`NWBrowser`) and advertises nothing.
        "net.dns-sd" => CLIENT_ONLY,
        // A socket path is absolute (LLP 1133.005 D4): `unix:` alone lets the
        // app bind in its container, and a named prefix is outside it.
        "net.listen" if target == "unix:" => NONE,
        "net.listen" => Sandbox::Refused(
            "the Mac App Store sandbox binds no socket outside the app's container: write `net.listen unix:` and bind under the container",
        ),
        "fs.read" | "fs.write" | "sqlite.open" => path(target.trim_matches('"')),
        "doc.keep" => Sandbox::Keys(&[USER_SELECTED, "files.bookmarks.app-scope"]),
        "secret.keep" | "secret.hold" | "storage.kv" | "env.launch" | "surface.read"
        | "surface.write" | "browser.automation" | "payment.google-pay" => NONE,
        "env.read" | "process.spawn" => match exact_grants::process::sandbox_refusal(line) {
            Some(why) => Sandbox::Refused(why),
            None => NONE,
        },
        _ if crate::device::is_device_line(line) => {
            let name = family.trim_start_matches("device.");
            DEVICES
                .iter()
                .find(|row| row.name == name)
                .map_or(Sandbox::Refused("not a device row"), device)
        }
        _ if crate::hardened::is_runtime_line(line) => {
            Sandbox::Refused(crate::hardened::SANDBOX_REFUSAL)
        }
        _ => Sandbox::Refused(
            "the family names no sandbox entitlement: a grant family is audited before a store build takes it (LLP 1030.005 D2)",
        ),
    }
}

/// A filesystem target, in D2's order: the container and the bundle take
/// nothing, a chosen document the user-selected key, and supported known
/// folders their asset keys. A home or absolute path is refused.
fn path(target: &str) -> Sandbox {
    match target.split_once(':') {
        Some(("app" | "bundle", _)) => NONE,
        Some(("downloads", _)) => Sandbox::Keys(&["files.downloads.read-write"]),
        Some(("pictures", _)) => Sandbox::Keys(&["assets.pictures.read-write"]),
        Some(("music", _)) => Sandbox::Keys(&["assets.music.read-write"]),
        Some(("videos", _)) => Sandbox::Keys(&["assets.movies.read-write"]),
        Some(("desktop" | "documents" | "screenshots", _)) => Sandbox::Refused(
            "the Mac App Store sandbox has no entitlement for this well-known folder",
        ),
        Some(("doc", _)) => Sandbox::Keys(&[USER_SELECTED]),
        // A Windows path (LLP 1027.001) names nothing on the Mac.
        Some(("win", _)) => NONE,
        _ => Sandbox::Refused(OUTSIDE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_device_row_fills_the_column() {
        let unset: Vec<_> = DEVICES
            .iter()
            .filter(|row| row.sandbox.is_none())
            .map(|row| row.name)
            .collect();
        assert!(unset.is_empty(), "rows without a sandbox value: {unset:?}");
        // A row whose Mac entitlement a profile authorizes is the profile's.
        for row in DEVICES.iter().filter(|row| !row.macos_profile.is_empty()) {
            assert_eq!(row.sandbox, Some(Sandbox::Profile), "{}", row.name);
        }
        // The sandbox reads the same key as the hardened runtime where both exist.
        for row in DEVICES {
            if let (Some(hardened), Some(Sandbox::Keys(keys))) = (row.hardened, row.sandbox) {
                let key = hardened.strip_prefix(PREFIX).unwrap();
                assert!(keys.contains(&key), "{}: {keys:?}", row.name);
            }
        }
    }

    #[test]
    fn a_row_a_package_adds_without_a_value_is_refused() {
        let row = Device {
            name: "package-row",
            ..crate::device::ROW
        };
        let Sandbox::Refused(why) = device(&row) else {
            panic!("an unset row is refused");
        };
        assert!(why.contains("fills the sandbox column"), "{why}");
    }

    #[test]
    fn each_family_has_its_value() {
        let cases: &[(&str, Sandbox)] = &[
            ("net.fetch https://api.example", CLIENT_ONLY),
            ("net.websocket wss://*.example.com", CLIENT_ONLY),
            ("net.fetch local-network", CLIENT_ONLY),
            ("auth.session https://login.example", CLIENT_ONLY),
            ("auth.callback myapp://done", CLIENT_ONLY),
            (
                "auth.callback http://127.0.0.1/callback",
                Sandbox::Keys(&[CLIENT, "network.server"]),
            ),
            ("auth.google 123.apps.googleusercontent.com", CLIENT_ONLY),
            ("auth.passkey example.com", Sandbox::Profile),
            ("auth.apple com.example.web", Sandbox::Profile),
            ("payment.apple-pay merchant.com.example", Sandbox::Profile),
            ("payment.google-pay", NONE),
            ("net.dns-sd _http._tcp", CLIENT_ONLY),
            ("net.listen unix:", NONE),
            ("fs.read app:/notes", NONE),
            ("fs.write app:/notes", NONE),
            ("sqlite.open app:/db.sqlite", NONE),
            ("fs.read bundle:/Resources/data", NONE),
            (
                "fs.write downloads:/",
                Sandbox::Keys(&["files.downloads.read-write"]),
            ),
            (
                "fs.write pictures:/Exports",
                Sandbox::Keys(&["assets.pictures.read-write"]),
            ),
            (
                "fs.write music:/",
                Sandbox::Keys(&["assets.music.read-write"]),
            ),
            (
                "fs.write videos:/",
                Sandbox::Keys(&["assets.movies.read-write"]),
            ),
            ("fs.write doc:/", Sandbox::Keys(&[USER_SELECTED])),
            (
                "doc.keep",
                Sandbox::Keys(&[USER_SELECTED, "files.bookmarks.app-scope"]),
            ),
            ("secret.keep token", NONE),
            ("secret.keep key biometry-any", NONE),
            ("secret.hold field", NONE),
            ("storage.kv prefs", NONE),
            ("env.read HOME", NONE),
            ("env.launch API_BASE", NONE),
            ("surface.read clipboard", NONE),
            ("browser.automation", NONE),
            ("process.spawn bundle:/Helpers/tool", NONE),
            (
                "device.camera purpose.camera",
                Sandbox::Keys(&["device.camera"]),
            ),
            ("device.push", Sandbox::Profile),
            ("device.login-item", NONE),
            (
                "device.hid",
                Sandbox::Keys(&["device.usb", "device.bluetooth"]),
            ),
        ];
        for (grant, want) in cases {
            assert_eq!(line(grant), *want, "{grant}");
        }
    }

    #[test]
    fn each_refusal_is_named() {
        let refused = |grant: &str| match line(grant) {
            Sandbox::Refused(why) => why,
            other => panic!("{grant} is {other:?}"),
        };
        assert!(refused("fs.read home:/Documents").contains("temporary-exception"));
        for folder in ["desktop", "documents", "screenshots"] {
            assert!(refused(&format!("fs.write {folder}:/")).contains("no entitlement"));
        }
        assert!(refused("fs.write /Users/me/notes").contains("temporary-exception"));
        assert!(refused("sqlite.open home:/db").contains("outside the app's container"));
        assert!(refused("net.listen unix:/tmp/sock").contains("binds no socket"));
        assert_eq!(
            refused("process.spawn path:t3"),
            "the Mac App Store sandbox runs no program outside the app's bundle"
        );
        for spawn in [
            "process.spawn home:/.t3/bin",
            "process.spawn app:/bin",
            "process.spawn /usr/bin/git",
        ] {
            assert!(
                refused(spawn).contains("outside the app's bundle"),
                "{spawn}"
            );
        }
        assert!(refused("env.read login:PATH").contains("login shell"));
        assert!(
            refused("runtime.allow-jit Helpers/node/bin/node").contains("app-sandbox and inherit")
        );
        assert!(refused("device.health-read purpose.health").contains("HealthKit"));
        assert!(refused("device.health-write purpose.health").contains("HealthKit"));
        assert!(refused("telepathy.read minds").contains("audited"));
    }
}
