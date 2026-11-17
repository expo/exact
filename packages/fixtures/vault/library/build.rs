fn main() {
    println!("cargo:rerun-if-changed=src/apple.c");
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        // Also available to the Mac build tools in an iOS Cargo invocation:
        // LIBSQLITE3_FLAGS reaches host dependencies as well as the target.
        cc::Build::new()
            .file("src/apple.c")
            .define("fstatfs", "exact_sqlite_fstatfs")
            .compile("exact_sqlite_apple");
    }
}
