//! The resident dev driver for the Forma binding host: `forma-host-dev <source> <out>`.
//! Run through `bun host/web/dev.mjs --app forma-host --wasm`, which pushes each plan to the page.

fn main() -> std::process::ExitCode {
    exact_web::dev::main::<forma_host_data::FormaHost>()
}
