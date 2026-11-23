//! Generate the app's native module bindings in `OUT_DIR/modules.rs` (LLP 1133 §5).

fn main() {
    contract::build_module_rust(std::path::Path::new("../app.contract")).expect("module bindings");
}
