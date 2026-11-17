use super::{compatibility_with_trust, Manifest};

fn app(name: &str) -> crate::TempDir {
    let dir = std::env::temp_dir().join(format!(
        "exact-compat-notification-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/src")).unwrap();
    std::fs::write(dir.join("data/src/lib.rs"), "pub struct App;\n").unwrap();
    std::fs::write(
        dir.join("data/Cargo.toml"),
        "[package]\nname = \"app-data\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.json"),
        r#"{"name":"App","icons":[{"src":"assets/a.png"}],"app":{"id":"com.example.app","name":"App"},"host":{"ios":{"minimumOS":"17.0","backgroundModes":["fetch"]}},"deploy":{"origin":"continuous","bundles":"continuous"}}"#,
    )
    .unwrap();
    crate::TempDir(dir)
}

#[test]
fn an_ios_notification_service_is_native_cohort_identity() {
    let dir = app("notification-service");
    let service = dir.join("modules/apple/notification-service");
    std::fs::create_dir_all(&service).unwrap();
    std::fs::write(
        service.join("NotificationService.swift"),
        "final class NotificationService {}\n",
    )
    .unwrap();
    let manifest = Manifest::read(&dir).unwrap();
    let build = |platform, target| {
        compatibility_with_trust(&dir, platform, target, &manifest, Some(""), "development")
            .unwrap()
    };
    let first = build("ios", "aarch64-apple-ios");
    assert_eq!(
        first.inputs["notificationService"]["files"],
        serde_json::json!([{
            "path": "NotificationService.swift",
            "sha256": "a21ff9b27f8fabd2d07ee2680a0f4434f79501bb444fea40097b2aa165d0e92d"
        }])
    );
    assert_eq!(
        first.inputs["notificationService"]["bundleSuffix"],
        "notifications"
    );
    assert!(build("ios", "aarch64-apple-tvos").inputs["notificationService"].is_null());
    assert!(build("macos", "aarch64-apple-darwin").inputs["notificationService"].is_null());
    std::fs::write(service.join(".DS_Store"), "Finder metadata").unwrap();
    std::fs::create_dir_all(service.join(".helpers")).unwrap();
    std::fs::write(service.join(".helpers/Hidden.swift"), "hidden\n").unwrap();
    std::fs::create_dir_all(service.join("target")).unwrap();
    std::fs::write(service.join("target/Generated.swift"), "generated\n").unwrap();
    assert_eq!(first.id, build("ios", "aarch64-apple-ios").id);
    std::fs::write(service.join("Helpers.swift"), "func help() {}\n").unwrap();
    std::fs::create_dir(service.join("Helpers")).unwrap();
    std::fs::write(service.join("Helpers/Decode.swift"), "func decode() {}\n").unwrap();
    let helper = build("ios", "aarch64-apple-ios");
    assert_ne!(first.id, helper.id);
    assert_eq!(
        helper.inputs["notificationService"]["files"],
        serde_json::json!([
            {"path": "Helpers.swift", "sha256": "d3a776918b4c577d94357a6c6721eba846820c037956fabb4aa282ae3fc1d7ba"},
            {"path": "Helpers/Decode.swift", "sha256": "33bbb09bfc58262a07dac2cd169be2a80836d4f90f9c159470b3b125de67fd36"},
            {"path": "NotificationService.swift", "sha256": "a21ff9b27f8fabd2d07ee2680a0f4434f79501bb444fea40097b2aa165d0e92d"}
        ])
    );
    std::fs::write(
        service.join("NotificationService.swift"),
        "final class NotificationService { let revision = 2 }\n",
    )
    .unwrap();
    assert_ne!(helper.id, build("ios", "aarch64-apple-ios").id);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn notification_service_identity_refuses_a_linked_root_or_parent() {
    use std::os::unix::fs::symlink;

    for (case, link, target, source) in [
        (
            "notification-service-root-link",
            "modules/apple/notification-service",
            "outside/service",
            "NotificationService.swift",
        ),
        (
            "notification-service-apple-link",
            "modules/apple",
            "outside/apple",
            "notification-service/NotificationService.swift",
        ),
        (
            "notification-service-modules-link",
            "modules",
            "outside/modules",
            "apple/notification-service/NotificationService.swift",
        ),
    ] {
        let dir = app(case);
        let linked = dir.join(link);
        let outside = dir.join(target);
        std::fs::create_dir_all(linked.parent().unwrap()).unwrap();
        std::fs::create_dir_all(outside.join(source).parent().unwrap()).unwrap();
        std::fs::write(outside.join(source), "final class NotificationService {}\n").unwrap();
        symlink(&outside, &linked).unwrap();
        let manifest = Manifest::read(&dir).unwrap();
        let error = compatibility_with_trust(
            &dir,
            "ios",
            "aarch64-apple-ios",
            &manifest,
            Some(""),
            "development",
        )
        .unwrap_err();
        assert!(error.contains(link), "{error}");
        assert!(error.contains("symbolic link"), "{error}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn notification_service_ignores_a_linked_modules_directory_with_no_service() {
    use std::os::unix::fs::symlink;

    let dir = app("notification-service-linked-modules-without-service");
    let outside = dir.join("outside-modules");
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, dir.join("modules")).unwrap();
    let manifest = Manifest::read(&dir).unwrap();
    let compat = compatibility_with_trust(
        &dir,
        "ios",
        "aarch64-apple-ios",
        &manifest,
        Some(""),
        "development",
    )
    .unwrap();
    assert!(compat.inputs["notificationService"].is_null());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn notification_service_refuses_an_unreadable_ancestor() {
    use std::os::unix::fs::PermissionsExt;

    let dir = app("notification-service-unreadable-ancestor");
    let modules = dir.join("modules");
    let service = modules.join("apple/notification-service");
    std::fs::create_dir_all(&service).unwrap();
    std::fs::write(
        service.join("NotificationService.swift"),
        "final class NotificationService {}\n",
    )
    .unwrap();
    let manifest = Manifest::read(&dir).unwrap();
    std::fs::set_permissions(&modules, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&modules).is_ok() {
        std::fs::set_permissions(&modules, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        return;
    }
    let result = compatibility_with_trust(
        &dir,
        "ios",
        "aarch64-apple-ios",
        &manifest,
        Some(""),
        "development",
    );
    std::fs::set_permissions(&modules, std::fs::Permissions::from_mode(0o700)).unwrap();
    let error = result.unwrap_err();
    assert!(error.contains("Permission denied"), "{error}");
    std::fs::remove_dir_all(dir).unwrap();
}
