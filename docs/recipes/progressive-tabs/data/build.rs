//! Write the view's shapes into `OUT_DIR/shapes.rs` as Rust structs, so a field added to `app.contract` is a
//! compile error here rather than a record built one position off.

fn main() {
    contract::build_rust(std::path::Path::new("../app.contract")).expect("contract rust");
}
