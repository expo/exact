//! Run by Cargo, with no arguments, it does nothing. Run with arguments, it
//! is `exact-js-bake` (`exact_js_bake::cli`).

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("cargo:rerun-if-changed=build.rs");
        return std::process::ExitCode::SUCCESS;
    }
    exact_js_bake::cli::main(&args)
}
