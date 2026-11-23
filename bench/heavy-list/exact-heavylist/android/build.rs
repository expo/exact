//! Compile and bake `app.contract` into `OUT_DIR/app.plan`, with the
//! compatibility id beside it, for `exact_painted::canvas_jni!` to embed.

use contract::DataSource;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../app.contract");
    println!("cargo:rerun-if-changed=../app.json");
    println!("cargo:rerun-if-changed=../data");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }
    let plan = contract::compile_path(std::path::Path::new("../app.contract"))
        .unwrap_or_else(|e| panic!("app.contract:{e}"));
    let baked = contract::bake(plan, exact_heavylist_data::Heavy::default())
        .unwrap_or_else(|e| panic!("bake: {e:?}"));
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("app.plan"), baked.encode()).unwrap();
    // The Canvas host is the Linux host's presenter, so the id is computed as for Linux.
    let app_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let target = std::env::var("TARGET").unwrap_or_default();
    let manifest = contract::Manifest::read(&app_dir).unwrap_or_else(|e| panic!("app.json: {e}"));
    let source = exact_heavylist_data::Heavy::default();
    let grants = source.grants();
    let compat = exact_bake::compatibility_id(
        &app_dir,
        "linux",
        &target,
        &manifest,
        Some(grants),
        exact_bake::hands_out_work!(source),
    )
    .unwrap_or_else(|e| panic!("compatibility id: {e}"));
    std::fs::write(out_dir.join("compat.json"), compat.to_json()).unwrap();
    contract::native::android_entry(&app_dir, &manifest, &baked, &out_dir)
        .unwrap_or_else(|e| panic!("android: {e}"));
}
