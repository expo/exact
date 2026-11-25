//! The bake for a Contract game UI, optionally with one linked Rust data source.
use exact_bake::host::{apple_host, empty_entry, Shape};
use std::{env, fs, path::PathBuf};

/// Bake a host from the declaration produced by its GPU build, without linking gameplay.
pub fn bake_declaration(platform: &str, app_dir: &str) {
    require_declaration(app_dir);
    bake(platform, app_dir);
}

/// Bake a game without a data crate for the Apple shell such games share
/// (`exact-game-apple`, game/app/apple), which its crate depends on: the entry
/// only defines the plan and compat.json that shell reads, so the game's own
/// Apple crate compiles no host. `game/app/shells.mjs` (`sharesAppleEntry`)
/// generates this call only for a game the shared entry fits.
pub fn bake_shared_declaration(app_dir: &str) {
    require_declaration(app_dir);
    bake_source("apple", app_dir, (), "()", "Default::default()", true);
}

/// Bake a host with an ordinary linked data source wrapped by host storage.
pub fn bake_data_declaration<D: contract::DataSource + Default>(
    platform: &str,
    app_dir: &str,
    data_type: &str,
) {
    require_declaration(app_dir);
    let wrapped = format!("exact_data_host::Storage<{data_type}>");
    let constructor = format!("exact_data_host::Storage::new({data_type}::default())");
    bake_source(
        platform,
        app_dir,
        D::default(),
        &wrapped,
        &constructor,
        false,
    );
}

fn require_declaration(app_dir: &str) {
    let path = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join(app_dir)
        .join(".shells/surfaces.json");
    println!("cargo:rerun-if-changed={}", path.display());
    fs::metadata(&path).expect("build the game's GPU shell before its host");
    // The keys `Actions` bind, which the bake's shortcut warning reads.
    let keys = path.with_file_name("keys.json");
    if keys.exists() {
        println!("cargo:rerun-if-changed={}", keys.display());
    }
}

/// Bake the app named by the generated shell, with no JS or Rust module.
/// `apple` selects macOS or iOS from Cargo's target OS.
pub fn bake(platform: &str, app_dir: &str) {
    bake_source(platform, app_dir, (), "()", "Default::default()", false);
}

/// A game's host crates: the Rust replacement off, the desktop packages composed without hatches, and the plain Apple
/// host.
const GAME: Shape = Shape {
    replacement: false,
    composed: false,
    apple_linked: false,
    shortcuts: true,
};

fn bake_source<D: contract::DataSource>(
    platform: &str,
    app_dir: &str,
    data: D,
    data_type: &str,
    constructor: &str,
    shared: bool,
) {
    exact_bake::host::bake(
        platform,
        app_dir,
        &GAME,
        data,
        // A game is the plain Apple host (`GAME.apple_linked` is false),
        // which links every capability.
        false,
        data_type,
        constructor,
        |entry| {
            // The shared shell holds this game's host: the entry it would compile names nothing
            // of the game but its app id (`SHARED_ENTRY`). shells.mjs decided that from the build's
            // environment before Cargo ran; a build in another environment finds otherwise.
            shared.then(|| {
            let own = empty_entry(entry.constructor, entry.inputs, "app_id()")
                .map(|e| format!("{e}\n{}\n", apple_host("plan()", "compat()")));
            assert!(
                own.as_deref() == Ok(SHARED_ENTRY),
                "this game's Apple entry is not exact-game-apple's (preloads, a link set without SQLite, \
                 EXACT_NATIVE_SOURCE or EXACT_RUST_BUNDLE), but its .shells workspace was generated in \
                 another environment to share it; a build through exact.mjs or host/apple/build.mjs \
                 generates it for its own"
            );
            SHARED_SHELL.to_string()
        })
        },
    );
}

/// exact-game-apple's entry: a game's own, with the app id read from its compat.json.
const SHARED_ENTRY: &str = include_str!("../apple/src/entry.rs");

/// A shared-shell game's own Apple crate: the plan and compat.json that
/// `exact-game-apple` reads, and that crate, which holds the host.
const SHARED_SHELL: &str = r#"extern crate exact_game_apple;
/// The game's baked plan, which `exact-game-apple` reads.
#[export_name = "exact_game_plan"]
pub static EXACT_GAME_PLAN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"));
/// The game's compat.json, which `exact-game-apple` reads.
#[export_name = "exact_game_compat"]
pub static EXACT_GAME_COMPAT: &str = include_str!(concat!(env!("OUT_DIR"), "/compat.json"));
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use exact_bake::host::runtime_entry;
    use exact_runner::Value;

    /// Removes its directory when dropped, so a failing test leaves nothing behind.
    struct TempDir(std::path::PathBuf);
    impl std::ops::Deref for TempDir {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl AsRef<std::path::Path> for TempDir {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_game_without_data_names_its_runtime_storage() {
        for platform in ["ios", "web", "linux"] {
            let inputs = serde_json::json!({
                "app": "dev.exact.kept-game", "platform": platform, "linked": []
            });
            let entry = runtime_entry("()", "Default::default()", "off", &inputs).unwrap();
            assert!(entry.contains("fn app_id(&self) -> &str { \"dev.exact.kept-game\" }"));
            if platform == "web" {
                assert!(
                    !entry.contains("exact_logic::"),
                    "web shells do not link exact-logic"
                );
            }
            assert!(
                entry.contains("AppData, ExactEmptyData,")
                    || entry.contains("type AppData = ExactEmptyData;")
            );
            assert_eq!(
                runtime_entry("Source", "source()", "off", &inputs).unwrap(),
                contract::rust_entry("Source", "source()", "off", &inputs).unwrap()
            );
        }
    }

    /// exact-game-apple's entry is what a development game's own entry would be,
    /// with the app id read from its compat.json: when the generated entry moves,
    /// this fails rather than every game quietly compiling its own host again.
    #[test]
    fn the_shared_shell_is_the_generated_entry() {
        let inputs = serde_json::json!({"app": "dev.exact.kept-game", "platform": "ios"});
        let entry = empty_entry("Default::default()", &inputs, "app_id()").unwrap();
        let host = apple_host("plan()", "compat()");
        assert_eq!(SHARED_ENTRY, format!("{entry}\n{host}\n"));
    }

    #[test]
    fn contract_source_graph_includes_imported_sources() {
        let dir = TempDir(
            std::env::temp_dir().join(format!("game-contract-watch-{}", std::process::id())),
        );
        fs::create_dir_all(&dir).unwrap();
        let root = dir.join("app.contract");
        let imported = dir.join("personal-inventory.contract");
        fs::write(&root, "use Inventory from \"./personal-inventory.contract\"\ncomponent App\n  view\n    Inventory()\n").unwrap();
        fs::write(
            &imported,
            "component Inventory\n  view\n    text \"inventory\"\n",
        )
        .unwrap();
        let graph = contract::source_graph(&root);
        assert!(graph
            .sources
            .iter()
            .any(|source| source.path.ends_with("app.contract")));
        assert!(graph
            .sources
            .iter()
            .any(|source| source.path.ends_with("personal-inventory.contract")));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn empty_named_call_binds_every_surface_default() {
        #[derive(Default)]
        struct Defaults;
        impl exact_gpu::Surface for Defaults {
            fn arguments(&self) -> Vec<(&'static str, Value)> {
                vec![
                    ("seed", Value::Number(7.)),
                    ("paused", Value::Bool(true)),
                    ("label", Value::str("default")),
                ]
            }
            fn bind(
                &mut self,
                values: &[Value],
                _: Option<f64>,
            ) -> Result<(), exact_gpu::SurfaceError> {
                assert_eq!(
                    values,
                    self.arguments()
                        .into_iter()
                        .map(|(_, v)| v)
                        .collect::<Vec<_>>()
                );
                Ok(())
            }
            fn render(
                &mut self,
                _: &exact_gpu::Frame,
                _: &exact_gpu::wgpu::Device,
                _: &exact_gpu::wgpu::Queue,
                _: &mut exact_gpu::wgpu::CommandEncoder,
                _: &exact_gpu::wgpu::TextureView,
                _: exact_gpu::wgpu::TextureFormat,
            ) -> bool {
                false
            }
        }
        let plan =
            contract::compile("component App\n  view\n    canvas surface=arena()\n").unwrap();
        let bytes = plan.encode();
        assert_eq!(&bytes[4..8], &exact_plan::FORMAT_VERSION.to_le_bytes());
        let plan = exact_plan::Plan::decode(&bytes).unwrap();
        assert_eq!(plan.surfaces[0].mode, exact_plan::SurfaceArgsMode::Named);
        let mut runner = exact_runner::Runner::boot(
            plan,
            (),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let update = runner.take_surface_updates().remove(0);
        static REGISTRY: exact_gpu::Registry = exact_gpu::Registry {
            surfaces: &[("arena", 0, || Box::<Defaults>::default())],
            shaders: &[],
        };
        let mut module = exact_gpu::Module::new(&REGISTRY);
        let id = module.create_headless("arena").unwrap();
        assert!(module.bind_json(id, &update.arguments_json(), None));
    }

    #[test]
    fn game_declaration_checks_names_and_arity_in_unmounted_branches() {
        let dir =
            TempDir(std::env::temp_dir().join(format!("game-declaration-{}", std::process::id())));
        fs::create_dir_all(dir.join(".shells")).unwrap();
        fs::write(
            dir.join(".shells/surfaces.json"),
            serde_json::json!({"arena": [
                {"name": "seed", "default": 0.0},
                {"name": "paused", "default": false},
                {"name": "restart", "default": false},
                {"name": "label", "default": ""},
            ]})
            .to_string(),
        )
        .unwrap();
        for (call, valid) in [
            ("arena()", true),
            (
                "arena(seed=7, paused=true, restart=false, label=\"hello\")",
                true,
            ),
            ("arena(label=\"hello\", seed=7)", true),
            ("arena(7, false, false, \"hello\")", true),
            ("arena(7, false, false, \"hello\", 9)", false),
            ("arena(typo=7)", false),
            ("world(seed=7)", false),
            ("simulation(seed=7)", false),
        ] {
            let result = contract::compile_path_source(&dir.join("app.contract"), &format!(
                "component App\n  state visible = false\n  view\n    column\n      if visible\n        canvas surface={call}\n      else\n        text \"Menu\"\n"
            )).map(|plan| contract::bake(plan, ()).unwrap());
            assert_eq!(result.is_ok(), valid, "{call}: {:?}", result.as_ref().err());
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
