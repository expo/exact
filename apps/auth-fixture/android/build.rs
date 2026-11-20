//! Bake the plan, bytecode and Canvas entry for the Android APK.

fn main() {
    if exact_js_bake::take_android_linked().unwrap_or_else(|e| panic!("Android bake: {e}")) {
        return;
    }
    exact_js_bake::build_android(std::path::Path::new(".."))
        .unwrap_or_else(|e| panic!("Android bake: {e}"));
}
