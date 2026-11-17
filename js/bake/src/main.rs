//! `exact-js-bake`: the bake command (`exact_js_bake::cli`).

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    exact_js_bake::cli::main(&args)
}
