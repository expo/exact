//! `exact-bake compat <app-dir> --platform <p> [--target <triple>] [--json]`
//! — the compatibility id (LLP 1030 D3a) the bake writes beside the plan,
//! and with `--json` the inputs it digests, for reading why two differ.
//! `--grants` and `--rust-grants` supply a fresh bake's source declarations.
//! `exact-bake graph <app.plan> <compat.json> <target>` — the plan's
//! requirement graph against those compatibility inputs (`scripts/push.mjs`).

use std::process::ExitCode;

const USAGE: &str = "usage:
  exact-bake compat <app-dir> --platform <ios|macos|linux|web> [--target <triple>] [--grants <text>] [--rust-grants <text>] [--json]
  exact-bake graph <app.plan> <compat.json> <target>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h") if args.len() == 1 => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("compat") => compat(&args[1..]),
        Some("graph") if args.len() == 4 => graph(&args[1], &args[2], &args[3]),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

struct CompatOptions<'a> {
    dir: &'a str,
    platform: &'a str,
    target: Option<&'a str>,
    grants: Option<&'a str>,
    rust_grants: Option<&'a str>,
    json: bool,
}

fn compat_options(args: &[String]) -> Option<CompatOptions<'_>> {
    let (dir, flags) = args.split_first()?;
    if dir.is_empty() || dir.starts_with('-') {
        return None;
    }
    let (mut platform, mut target, mut json) = (None, None, false);
    let (mut grants, mut rust_grants) = (None, None);
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--platform" if platform.is_none() => {
                let value = flags.next()?.as_str();
                if !matches!(value, "ios" | "macos" | "linux" | "web") {
                    return None;
                }
                platform = Some(value);
            }
            "--target" if target.is_none() => {
                let value = flags.next()?.as_str();
                if value.is_empty() || value.starts_with('-') {
                    return None;
                }
                target = Some(value);
            }
            "--json" if !json => json = true,
            "--grants" if grants.is_none() => {
                grants = Some(flags.next()?.as_str()).filter(|v| !v.starts_with("--"));
                grants?;
            }
            "--rust-grants" if rust_grants.is_none() => {
                rust_grants = Some(flags.next()?.as_str()).filter(|v| !v.starts_with("--"));
                rust_grants?;
            }
            _ => return None,
        }
    }
    if rust_grants.is_some() && grants.is_none() {
        return None;
    }
    Some(CompatOptions {
        dir,
        platform: platform?,
        target,
        grants,
        rust_grants,
        json,
    })
}

/// `exact-bake compat <app-dir> --platform <p> [--target <triple>] [--json]`:
/// the compatibility id of the app built for that platform — the id alone,
/// or with `--json` the id and every input it digests. Callers can supply
/// source grants from a completed bake without linking the native host.
fn compat(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: exact-bake compat <app-dir> --platform <ios|macos|linux|web> [--target <triple>] [--grants <text>] [--rust-grants <text>] [--json]";
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let Some(CompatOptions {
        dir,
        platform,
        target,
        grants,
        rust_grants,
        json,
    }) = compat_options(args)
    else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let target = target
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS));
    let app_dir = std::path::Path::new(dir);
    let result = contract::Manifest::read(app_dir).and_then(|m| {
        exact_bake::compatibility_id_sources(app_dir, platform, &target, &m, grants, rust_grants)
    });
    match result {
        Ok(c) => {
            if json {
                print!("{}", c.to_json());
            } else {
                println!("{}", c.id);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("exact-bake compat: {e}");
            ExitCode::from(1)
        }
    }
}

/// `exact-bake graph <app.plan> <compat.json> <target>`: the graph as JSON.
fn graph(plan: &str, compat: &str, target: &str) -> ExitCode {
    let result = std::fs::read(plan)
        .map_err(|e| format!("{plan}: {e}"))
        .and_then(|plan| {
            let compat: serde_json::Value = serde_json::from_slice(
                &std::fs::read(compat).map_err(|e| format!("{compat}: {e}"))?,
            )
            .map_err(|e| format!("{compat}: {e}"))?;
            exact_bake::plan_graph(&plan, &compat["inputs"], target)
        });
    match result {
        Ok(graph) => {
            println!("{graph}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("exact-bake graph: {error}");
            ExitCode::FAILURE
        }
    }
}
