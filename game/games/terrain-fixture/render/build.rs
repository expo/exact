//! Reflect the shader inventory the game bake assembles (gpu.shaderRoots, each
//! after its gpu.shaderPreludes) in EXACT_GAME_SHADERS: `shaders::SHADERS`
//! names the interfaces the hosts register.
fn main() {
    println!("cargo:rerun-if-env-changed=EXACT_GAME_SHADERS");
    let dir = std::path::PathBuf::from(
        std::env::var_os("EXACT_GAME_SHADERS").expect("build through the game bake"),
    );
    println!("cargo:rerun-if-changed={}", dir.display());
    let generated = exact_gpu_reflect::generate(&dir).unwrap_or_else(|e| panic!("{e}"));
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("shaders.rs");
    std::fs::write(out, generated.rust).unwrap();
}
