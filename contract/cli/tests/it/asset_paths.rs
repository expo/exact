//! A literal file outside `assets/` is refused when an app compiles
//! (expo/exact-build#4): the build bundles `assets/` (and `deck/`) only, so
//! `image "public/logo.png"` built green and the phone drew nothing.
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const ID: &str = "lower-asset-path";

struct AppDir(PathBuf);

impl AppDir {
    fn new(source: &str) -> AppDir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "exact-contract-asset-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("app.contract"), source).unwrap();
        // The source graph names canonical paths; macOS's temporary
        // directory is under a symlink (`/var` is `/private/var`).
        AppDir(dir.canonicalize().unwrap())
    }

    /// Each `lower-asset-path` refusal, its line and message.
    fn refused(&self) -> Vec<(u32, String)> {
        match contract::compile_path_all(&self.0.join("app.contract"), false) {
            Ok(_) => Vec::new(),
            Err(all) => all
                .into_iter()
                .filter(|e| e.id == ID)
                .map(|e| (e.span.line, e.message))
                .collect(),
        }
    }
}

impl Drop for AppDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn app(view: &str) -> String {
    format!("component App\n  state on = false\n  view\n    column\n{view}")
}

#[test]
fn a_literal_image_outside_assets_is_refused_with_where_it_ships_from() {
    let dir = AppDir::new(&app("      image \"public/logo.png\" width=96 height=96\n"));
    assert_eq!(
        dir.refused(),
        [(
            5,
            "image source `public/logo.png` is outside assets/, the folder every host ships the app's pictures and media from; move the file under assets/ and write \"assets/logo.png\"".to_string()
        )]
    );
    // Either arm of a choice, a template of literals, a bare file name, a
    // video's source and poster: each named on its own line.
    let dir = AppDir::new(&app(concat!(
        "      image (on ? \"assets/a.png\" : \"public/b.png\")\n",
        "      image `public/c.png`\n",
        "      image \"logo.png\"\n",
        "      video \"media/clip.mp4\" poster=\"public/poster.png\"\n",
    )));
    let refused = dir.refused();
    assert_eq!(
        refused.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
        [5, 6, 7, 8, 8],
        "{refused:?}"
    );
    assert!(
        refused[3]
            .1
            .starts_with("video source `media/clip.mp4` is outside assets/")
            && refused[4]
                .1
                .starts_with("video poster `public/poster.png` is outside assets/"),
        "{refused:?}"
    );
}

#[test]
fn a_bundled_folder_in_another_case_is_refused_as_its_spelling() {
    let dir = AppDir::new(&app("      image \"Assets/logo.png\"\n"));
    assert_eq!(
        dir.refused(),
        [(
            5,
            "image source `Assets/logo.png` spells the assets/ folder in another case, which a case-sensitive file system (Linux, a web server) does not find; write it \"assets/…\" in lowercase".to_string()
        )]
    );
}

#[test]
fn a_component_used_twice_is_refused_once() {
    let dir = AppDir::new("component App\n  view\n    column\n      Logo()\n      Logo()\n\ncomponent Logo\n  view\n    image \"public/logo.png\"\n");
    assert_eq!(
        dir.refused()
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>(),
        [9]
    );
}

#[test]
fn bundled_paths_urls_symbols_and_computed_sources_still_compile() {
    let source = app(concat!(
        "      image \"assets/logo.png\"\n",
        "      image \"./assets/icons/star.svg\"\n",
        "      image \"deck/slide.png\"\n",
        "      image \"https://example.com/logo.png\"\n",
        "      image \"symbol:heart\"\n",
        "      image (on ? \"symbol:heart\" : \"assets/b.png\")\n",
        "      video \"https://example.com/clip.mp4\" poster=\"assets/poster.png\"\n",
    ));
    let dir = AppDir::new(&source);
    contract::compile_path(&dir.0.join("app.contract")).unwrap_or_else(|e| panic!("{e}"));
    // Source text with no app directory has no folders to ship from.
    contract::compile(&app("      image \"public/logo.png\"\n")).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn bundled_svg_refusals_and_drops_are_reported_at_the_image_source() {
    let source = app("      image \"assets/picture.svg\"\n");
    let dir = AppDir::new(&source);
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"host":{"linux":{}}}"#,
    )
    .unwrap();
    let cases: &[(&[u8], &str)] = &[
        (
            b"<!DOCTYPE svg [<!ENTITY a 'x'>]><svg/>",
            "entity declaration",
        ),
        (b"<svg>\n<g>\n</svg>", "line 3: `</svg>` closes `<g>`"),
        (b"<svg>&missing;</svg>", "&missing;"),
        (b"<svg>\xff</svg>", "invalid utf-8"),
    ];
    for &(svg, reason) in cases {
        std::fs::write(dir.0.join("assets/picture.svg"), svg).unwrap();
        let Err(errors) = contract::compile_path_all(&dir.0.join("app.contract"), false) else {
            panic!("a native build must refuse invalid SVG");
        };
        let found = errors
            .iter()
            .find(|e| e.id == "lower-image-svg")
            .unwrap_or_else(|| panic!("{errors:?}"));
        assert_eq!((found.span.line, found.span.col), (5, 13));
        assert!(found.message.contains("assets/picture.svg"), "{found:?}");
        assert!(found.message.contains(reason), "{found:?}");
    }
    std::fs::write(
        dir.0.join("assets/picture.svg"),
        "<svg data-cafe\u{301}='x'>\n<script>alert(1)</script>\n<image href='https://example.com/x.png'/>\n<use href='https://example.com/x.svg'/>\n<a href='https://example.com'/>\n</svg>",
    )
    .unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&dir.0.join("app.contract"), false);
    result.unwrap();
    let warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.id == "lower-image-svg")
        .collect();
    assert_eq!(warnings.len(), 5, "{warnings:?}");
    for (warning, reason) in
        warnings
            .iter()
            .zip(["data-cafe\u{301}", "<script>", "<image>", "<use>", "<a>"])
    {
        assert!(warning.is_warning());
        assert_eq!((warning.span.line, warning.span.col), (5, 13));
        assert!(warning.message.contains(reason), "{warning:?}");
        assert!(warning.message.contains("To fix"), "{warning:?}");
    }
}

#[test]
fn unavailable_svg_images_are_left_to_the_host() {
    let dir = AppDir::new(&app(concat!(
        "      image \"assets/missing.svg\"\n",
        "      image \"assets/directory.svg\"\n",
        "      image \"assets/missing.png\"\n",
    )));
    std::fs::create_dir(dir.0.join("assets/directory.svg")).unwrap();
    for host in ["linux", "web"] {
        std::fs::write(
            dir.0.join("app.json"),
            format!(r#"{{"app":{{"id":"dev.test.svg","name":"SVG"}},"host":{{"{host}":{{}}}}}}"#),
        )
        .unwrap();
        let (result, warnings) =
            contract::compile_path_all_warned(&dir.0.join("app.contract"), false);
        result.unwrap_or_else(|e| panic!("{host}: {e:?}"));
        assert!(
            !warnings.iter().any(|w| w.id == "lower-image-svg"),
            "{warnings:?}"
        );
    }
}

#[test]
fn valid_svg_images_and_web_only_refusals_keep_their_builds() {
    let dir = AppDir::new(&app("      image \"assets/picture.svg\"\n"));
    let entry = dir.0.join("app.contract");
    let file = dir.0.join("assets/picture.svg");
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"host":{"linux":{},"web":{}}}"#,
    )
    .unwrap();
    for svg in [
        "<svg width='24' height='24'><path d='M0 0h1'/></svg>",
        "<!DOCTYPE svg PUBLIC '-//W3C//DTD SVG 1.1//EN' 'x.dtd'><svg><text>A &amp; B &#65;&#x42;</text></svg>",
        "<svg><style><![CDATA[path {fill: red}]]></style><path d='M0 0h1'/></svg>",
        "<svg xmlns:m='urn:m' m:cafe\u{301}='x'><m:cafe\u{301}></m:cafe\u{301}><path d='M0 0h1'/></svg>",
    ] {
        std::fs::write(&file, svg).unwrap();
        let (result, warnings) = contract::compile_path_all_warned(&entry, false);
        result.unwrap_or_else(|e| panic!("{svg}: {e:?}"));
        assert!(!warnings.iter().any(|w| w.id == "lower-image-svg"), "{warnings:?}");
    }
    std::fs::write(&file, "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg/>").unwrap();
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"host":{"web":{}}}"#,
    )
    .unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&entry, false);
    result.unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.id == "lower-image-svg" && w.message.contains("entity declaration")));
    std::fs::write(&file, b"<svg>\xff</svg>").unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&entry, false);
    result.unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.id == "lower-image-svg" && w.message.contains("invalid utf-8")));
    // A target crate also makes this a native app, independently of host keys.
    std::fs::create_dir(dir.0.join("linux")).unwrap();
    std::fs::write(dir.0.join("linux/Cargo.toml"), "").unwrap();
    assert!(contract::compile_path_all(&entry, false).is_err());
}

#[test]
fn literal_svg_choices_are_checked_once_per_source_and_dynamic_sources_are_left_to_the_host() {
    let dir = AppDir::new("component App\n  state source = \"assets/dynamic.svg\"\n  state on = false\n  view\n    column\n      image source\n      image \"https://example.com/remote.svg\"\n      image \"app:/mutable.svg\"\n      image \"assets/picture.png\"\n      image (on ? \"assets/bad.svg\" : `./assets/good.svg`)\n      Logo()\n      Logo()\n\ncomponent Logo\n  view\n    image \"assets/bad.svg\"\n");
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"deploy":{"binaries":{"android":{}}}}"#,
    )
    .unwrap();
    std::fs::write(dir.0.join("assets/bad.svg"), "<svg><script/></svg>").unwrap();
    std::fs::write(dir.0.join("assets/good.svg"), "<svg/>").unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&dir.0.join("app.contract"), false);
    result.unwrap();
    let lines: Vec<_> = warnings
        .iter()
        .filter(|w| w.id == "lower-image-svg")
        .map(|w| w.span.line)
        .collect();
    assert_eq!(lines, [10, 16], "{warnings:?}");
}

#[test]
fn bundled_deck_svg_images_receive_parser_diagnostics() {
    let dir = AppDir::new(&app("      image `./deck/picture.svg`\n"));
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"host":{"linux":{}}}"#,
    )
    .unwrap();
    std::fs::create_dir(dir.0.join("deck")).unwrap();
    let entry = dir.0.join("app.contract");
    let image = dir.0.join("deck/picture.svg");
    std::fs::write(&image, "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg/>").unwrap();
    let errors = contract::compile_path_all(&entry, false)
        .err()
        .expect("SVG refused");
    assert!(
        errors.iter().any(|e| e.id == "lower-image-svg"
            && e.message.contains("entity declaration")
            && e.span.line == 5),
        "{errors:?}"
    );
    std::fs::write(&image, "<svg><script/></svg>").unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&entry, false);
    result.unwrap();
    assert!(
        warnings.iter().any(|e| e.id == "lower-image-svg"
            && e.message.contains("<script>")
            && e.span.line == 5),
        "{warnings:?}"
    );
}

#[test]
fn a_raster_image_with_an_svg_extension_is_left_to_the_host() {
    let dir = AppDir::new(&app("      image \"assets/picture.svg\"\n"));
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"host":{"linux":{}}}"#,
    )
    .unwrap();
    for bytes in [
        include_bytes!("../../../../apps/markdown/assets/monitor.png").as_slice(),
        include_bytes!("../../../../packages/camera/fixture/card.jpg").as_slice(),
    ] {
        std::fs::write(dir.0.join("assets/picture.svg"), bytes).unwrap();
        let (result, warnings) =
            contract::compile_path_all_warned(&dir.0.join("app.contract"), false);
        result.unwrap();
        assert!(
            !warnings.iter().any(|e| e.id == "lower-image-svg"),
            "{warnings:?}"
        );
    }
}

#[test]
fn a_windows_crate_makes_svg_parser_refusals_errors() {
    let dir = AppDir::new(&app("      image \"assets/picture.svg\"\n"));
    std::fs::write(
        dir.0.join("app.json"),
        r#"{"app":{"id":"dev.test.svg","name":"SVG"},"host":{"web":{}}}"#,
    )
    .unwrap();
    std::fs::create_dir(dir.0.join("windows")).unwrap();
    std::fs::write(dir.0.join("windows/Cargo.toml"), "").unwrap();
    std::fs::write(
        dir.0.join("assets/picture.svg"),
        "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg/>",
    )
    .unwrap();
    let errors = contract::compile_path_all(&dir.0.join("app.contract"), false)
        .err()
        .expect("SVG refused");
    assert!(
        errors.iter().any(|e| e.id == "lower-image-svg"),
        "{errors:?}"
    );
}

#[test]
fn literal_svg_images_are_build_inputs_even_before_the_file_exists() {
    let dir = AppDir::new("use Logo from \"./logo.contract\"\ncomponent App\n  state on = false\n  view\n    column\n      image (on ? \"assets/picture.svg\" : `./deck/missing.svg`)\n      image \"https://example.com/remote.svg\"\n      image \"assets/raster.png\"\n      Logo(picture=\"assets/nested.svg\")\n");
    std::fs::write(
        dir.0.join("logo.contract"),
        "component Logo\n  props\n    picture: string\n  view\n    image picture\n",
    )
    .unwrap();
    std::fs::write(dir.0.join("assets/picture.svg"), "<svg/>").unwrap();
    let entry = dir.0.join("app.contract");
    let graph = contract::source_graph(&entry);
    assert!(graph.errors.is_empty(), "{:?}", graph.errors);
    assert!(
        graph
            .sources
            .iter()
            .any(|s| s.path == dir.0.join("assets/picture.svg")),
        "{graph:?}"
    );
    for path in ["deck/missing.svg", "assets/nested.svg"] {
        assert!(graph.consulted.contains(&dir.0.join(path)), "{graph:?}");
    }
    std::fs::create_dir(dir.0.join("deck")).unwrap();
    std::fs::write(dir.0.join("deck/missing.svg"), "<svg/>").unwrap();
    let graph = contract::source_graph(&entry);
    assert!(
        graph
            .sources
            .iter()
            .any(|s| s.path == dir.0.join("deck/missing.svg")),
        "{graph:?}"
    );
    assert!(!graph.consulted.contains(&dir.0.join("deck/missing.svg")));
    assert!(!graph.sources.iter().any(|s| s.path.ends_with("raster.png")));
}

#[test]
fn svg_image_build_inputs_are_reported_to_cargo() {
    const CHILD: &str = "EXACT_TEST_SVG_RERUN";
    if let Some(entry) = std::env::var_os(CHILD) {
        contract::rerun_if_changed(std::path::Path::new(&entry));
        return;
    }
    let dir = AppDir::new(&app(concat!(
        "      image \"assets/picture.svg\"\n",
        "      image \"deck/nested/missing.svg\"\n",
    )));
    std::fs::write(dir.0.join("assets/picture.svg"), "<svg/>").unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "asset_paths::svg_image_build_inputs_are_reported_to_cargo",
            "--nocapture",
        ])
        .env(CHILD, dir.0.join("app.contract"))
        .env_remove("EXACT_BAKE_PACKAGE_DIR")
        .output()
        .unwrap();
    assert!(child.status.success(), "{child:?}");
    let output = String::from_utf8(child.stdout).unwrap();
    for line in [
        format!(
            "cargo:rerun-if-changed={}",
            dir.0.join("assets/picture.svg").display()
        ),
        format!(
            "cargo:metadata=exact-optional-input={}",
            dir.0.join("deck/nested/missing.svg").display()
        ),
        format!("cargo:rerun-if-changed={}", dir.0.display()),
    ] {
        assert!(output.lines().any(|found| found == line), "{output}");
    }
}
