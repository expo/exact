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
        AppDir(dir)
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
