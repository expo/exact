//! Compile and bake `app.contract` into `OUT_DIR/app.plan` at build time, with
//! the compatibility id beside it, for `exact_painted::canvas_jni!`,
//! and what the plan links (`contract::native::android_entry`: Media3's player,
//! LLP 1042.001 D2).

use contract::DataSource;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The library is empty off Android (src/lib.rs): nothing to bake.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }
    if exact_bake::host::take_android_linked(&[]).unwrap_or_else(|e| panic!("Android bake: {e}")) {
        return;
    }
    contract::rerun_if_changed(std::path::Path::new("../app.contract"));
    println!("cargo:rerun-if-changed=../app.json");
    println!("cargo:rerun-if-changed=../assets");
    println!("cargo:rerun-if-changed=../data");
    println!("cargo:rerun-if-changed=../../../Cargo.lock");
    let plan = match contract::compile_path(std::path::Path::new("../app.contract")) {
        Ok(p) => p,
        Err(e) => panic!("app.contract:{e}"),
    };
    let baked =
        contract::bake(plan, video_player_data::Player).unwrap_or_else(|e| panic!("bake: {e:?}"));
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("app.plan"), baked.encode()).unwrap();
    // The compatibility id as the Linux host computes it for this target: the
    // Canvas host is that host's presenter under Android's own Canvas, with no
    // update store (host/android/README.md).
    let app_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let target = std::env::var("TARGET").unwrap_or_default();
    let manifest = contract::Manifest::read(&app_dir).unwrap_or_else(|e| panic!("app.json: {e}"));
    let grants = video_player_data::Player.grants();
    let compat = exact_bake::compatibility_id_sources(
        &app_dir,
        "linux",
        &target,
        &manifest,
        Some(grants),
        None,
        exact_bake::hands_out_work!(video_player_data::Player),
    )
    .unwrap_or_else(|e| panic!("compatibility id: {e}"));
    std::fs::write(out_dir.join("compat.json"), compat.to_json()).unwrap();
    contract::native::android_entry(&app_dir, &manifest, &baked, &out_dir)
        .unwrap_or_else(|e| panic!("android: {e}"));
}
