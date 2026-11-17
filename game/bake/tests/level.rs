use exact_game::{asset::Level, Data, Game, Input, Vec3, World};

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

#[derive(Default, Data)]
struct Island {
    seed: u64,
    lanterns: Vec<Vec3>,
    heights: Vec<f32>,
    sign: String,
}
struct LevelGame;
impl Game for LevelGame {
    const ID: &'static str = "typed-level-bake";
    const LEVEL: Option<Level> = Some(Level::of::<Island>("island.level.json"));
    type Args = ();
    fn setup(_: &mut World, _: &()) {}
    fn tick(_: &mut World, _: &Input, _: &()) {}
}
struct NoLevel;
impl Game for NoLevel {
    const ID: &'static str = "no-level";
    type Args = ();
    fn setup(_: &mut World, _: &()) {}
    fn tick(_: &mut World, _: &Input, _: &()) {}
}
#[test]
fn level_bake_validates_the_authors_derive_before_delivery() {
    let dir = TempDir(std::env::temp_dir().join(format!("exact-level-{}", std::process::id())));
    std::fs::create_dir_all(&dir).unwrap();
    let level = Level::of::<Island>("island.level.json");
    let source = dir.join(level.name);
    let receipt = dir.join(".shells/level.json");
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("config.level.json"), b"{}").unwrap();
    std::fs::write(dir.join("assets/config.level.json"), b"{}").unwrap();
    exact_game_bake::bake_game_level::<NoLevel>(&dir).unwrap();
    assert!(!receipt.exists());
    std::fs::write(
        &source,
        r#"{"seed":7,"lanterns":[[1,"bad",3]],"sign":"hello"}"#,
    )
    .unwrap();
    let error = exact_game_bake::bake_game_level::<LevelGame>(&dir).unwrap_err();
    assert!(error.contains("lanterns.0"), "{error}");
    assert!(!dir.join("assets").join(level.name).exists());
    assert!(!receipt.exists());
    std::fs::write(&source, r#"{"heights":[1,"bad"]}"#).unwrap();
    let error = exact_game_bake::bake_game_level::<LevelGame>(&dir).unwrap_err();
    assert!(error.contains("heights.1"), "{error}");
    assert!(!dir.join("assets").join(level.name).exists());
    assert!(!receipt.exists());
    std::fs::write(
        &source,
        r#"{"seed":7,"lanterns":[[1,2,3]],"heights":[0.25,0.5],"sign":"hello"}"#,
    )
    .unwrap();
    exact_game_bake::bake_game_level::<LevelGame>(&dir).unwrap();
    let delivered = dir.join("assets").join(level.name);
    let before = std::fs::read(&delivered).unwrap();
    assert_eq!(before, std::fs::read(&source).unwrap());
    assert_eq!(
        std::fs::read_to_string(&receipt).unwrap(),
        r#""island.level.json""#
    );
    std::fs::write(
        &source,
        r#"{"seed":8,"lanterns":[[1,2,3]],"heights":[0.25,0.75],"sign":"hello"}"#,
    )
    .unwrap();
    exact_game_bake::bake_game_level::<LevelGame>(&dir).unwrap();
    assert_ne!(before, std::fs::read(&delivered).unwrap());
    exact_game_bake::bake_game_level::<NoLevel>(&dir).unwrap();
    assert!(!receipt.exists());
    assert_eq!(
        std::fs::read(dir.join("assets/config.level.json")).unwrap(),
        b"{}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
