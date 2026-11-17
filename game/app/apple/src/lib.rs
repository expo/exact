//! The host's C exports for every game without a data crate, over an empty data
//! source. The game's own Apple crate defines the plan and compat.json this
//! reads (`exact_game_app::bake_shared_declaration`), so this crate names no game.

use std::sync::OnceLock;

extern "Rust" {
    #[link_name = "exact_game_plan"]
    static PLAN: &'static [u8];
    #[link_name = "exact_game_compat"]
    static COMPAT: &'static str;
}

fn plan() -> &'static [u8] {
    // SAFETY: the game's shell crate defines both statics, immutable, in the same archive.
    unsafe { PLAN }
}

fn compat() -> &'static str {
    // SAFETY: as `plan`.
    unsafe { COMPAT }
}

/// The game's app id, `inputs.app` of its compat.json: the id a game's own
/// entry names as a literal.
fn app_id() -> &'static str {
    static APP: OnceLock<String> = OnceLock::new();
    APP.get_or_init(|| {
        let compat: serde_json::Value =
            serde_json::from_str(compat()).expect("the game's compat.json is JSON");
        compat["inputs"]["app"]
            .as_str()
            .expect("the game's compat.json names its app")
            .to_owned()
    })
}

// A game's own entry, but for its app id; exact-game-app's tests hold the two equal,
// and a game whose entry differs compiles its own host.
include!("entry.rs");
