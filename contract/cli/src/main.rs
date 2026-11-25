//! `contract build` compiles a source, prints a summary and writes the plan.
//! Baking needs the app's data crate and happens in the app's own build
//! (see `apps/caltrain`).
//! `contract types <file.contract> [-o <app.d.ts>] [--check]` — generate TypeScript's
//! data-source signatures from the same plan tables the executor reads.
//! `contract rust <file.contract> [-o <shapes.rs>]` — the same shapes as Rust
//! structs with their `Value` conversions, for a data crate's `build.rs`.
//! `contract verify` searches the app on the runner and checks it against
//! the Lean semantics when available (`verify.rs`).
//! `contract gallery <app> -o <dir>` — the design tool's variant gallery of the
//! app's library components, written into `<dir>` (`contract::gallery`).

use std::process::ExitCode;

// These macros cover stdout in this binary's modules. Internal pipes keep
// Rust's ignored SIGPIPE so their callers can handle write errors.
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::print_stdout(format_args!($($arg)*))
    };
}

macro_rules! println {
    () => { print!("\n") };
    ($($arg:tt)*) => { print!("{}\n", format_args!($($arg)*)) };
}

fn print_stdout(args: std::fmt::Arguments<'_>) {
    use std::io::Write as _;

    let result = {
        let mut out = std::io::stdout().lock();
        out.write_fmt(args).and_then(|()| out.flush())
    };
    if let Err(error) = result {
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("contract: stdout: {error}");
        std::process::exit(1);
    }
}

mod ambient;
mod build;
mod diff;
mod forma;
mod forma_contract;
mod forma_explain;
mod forma_export;
mod forma_gen;
mod forma_history;
mod forma_import;
mod forma_import_css;
mod forma_ops;
mod forma_packages;
mod forma_param_lines;
mod forma_rows;
mod forma_serve;
mod migrate;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/it/scratch.rs"]
mod scratch;
mod verify;
mod vocab;

fn usage() -> String {
    let build = build::USAGE.trim_start_matches("usage: ");
    let verify = verify::USAGE
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("usage: ");
    format!("usage:
  {build}
  contract migrate <file.contract>
  contract symbols <file.contract> [--stdin] [--editor] [--name <exact-name>]
  contract sources <file.contract>
  contract fmt [--check | --stdout | --stdin] <file.contract>
  contract fmt --uses <file.contract>
  contract types <file.contract> [-o <app.d.ts>] [--check]
  contract rust <file.contract> [-o <shapes.rs>]
  contract module-rust <file.contract> [-o <modules.rs>]
  contract test <file.test.contract>
  contract lean <file.contract> [--components] [--name <ident>] [-o <file.lean>]
  {verify}
  contract vocab [--json] [--all] [<name> …]
  contract gallery <app> -o <dir>  (<app>: a directory or app.contract; <dir>: a build cache, never the app's tree)
  contract forma check <app> | eval <app> [--mode <axis>=<value>] | ir <app> [-o <forma.json>] | fmt <app> [--check]  (<app>: a directory, app.contract or file.forma; a flag before or after it)")
}

#[cfg(windows)]
fn main() -> ExitCode {
    // @ref LLP 1035.005 D2 — the Windows executable's default 1 MiB stack
    // cannot compile the supported 256-site depth in a debug build.
    std::thread::Builder::new()
        .name("contract-cli".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .expect("cannot start compiler worker")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    run()
}

fn run() -> ExitCode {
    if let Err(e) = contract::root::find_for_executable() {
        eprintln!("contract: {e}");
        return ExitCode::from(2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h") if args.len() == 1 => {
            println!("{}", usage());
            ExitCode::SUCCESS
        }
        Some("build") => build::run(&args[1..]),
        Some("migrate") => migrate::run(&args[1..]),
        Some("symbols") => symbols(&args[1..]),
        Some("sources") => sources(&args[1..]),
        Some("fmt") => fmt(&args[1..]),
        Some("test") => tests(&args[1..]),
        Some("lean") => lean(&args[1..]),
        Some("verify") => verify::run(&args[1..]),
        Some("vocab") => vocab::run(&args[1..]),
        Some("gallery") => gallery(&args[1..]),
        Some("forma") => forma::run(&args[1..]),
        Some("tamber") => {
            eprintln!("contract: rename the tamber verb to forma: use `contract forma`");
            ExitCode::from(2)
        }
        // The union of tokens a binding may name follows the design's `exports` (LLP 1121 D7).
        Some("types") => types(&args[1..], "types", "app.d.ts", |path| {
            let (plan, _) = contract::compile_path_all_unchecked(path, false)
                .map_err(|mut all| all.swap_remove(0).to_string())?;
            contract::typescript_exporting(&plan, contract::design_exports(path).as_deref(), &[])
                .map_err(|e| format!("{}: {e}", path.display()))
        }),
        Some("rust") => types(&args[1..], "rust", "shapes.rs", |path| {
            let (plan, call_records) = contract::compile_path_for_rust(path)
                .map_err(|mut all| all.swap_remove(0).to_string())?;
            contract::rust(&plan, &call_records).map_err(|e| format!("{}: {e}", path.display()))
        }),
        Some("module-rust") => types(
            &args[1..],
            "module-rust",
            "modules.rs",
            contract::module_rust_path,
        ),
        _ => {
            eprintln!("{}", usage());
            ExitCode::from(2)
        }
    }
}

/// `contract gallery <app> -o <dir>`: a line per cell refused, and a summary.
fn gallery(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: contract gallery <app> -o <dir>";
    let (input, out) = match args {
        [flag] if matches!(flag.as_str(), "--help" | "-h") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [input, o, out] if o == "-o" => (input, out),
        [o, out, input] if o == "-o" => (input, out),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let input = std::path::Path::new(input);
    let app = if input.is_dir() {
        input.join("app.contract")
    } else {
        input.to_path_buf()
    };
    match contract::gallery::generate(&app, std::path::Path::new(out)) {
        Ok(g) => {
            for c in g.cells.iter().filter(|c| c.refused.is_some()) {
                let what = c
                    .varied
                    .as_ref()
                    .map(|(p, v)| format!("{p}={v}"))
                    .unwrap_or_else(|| "first values".into());
                println!(
                    "gallery: {} · {what}: can't place: {}",
                    c.component,
                    c.refused.as_deref().unwrap_or("")
                );
            }
            let components = g
                .cells
                .iter()
                .map(|c| &c.identity)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            let refused = g.cells.iter().filter(|c| c.refused.is_some()).count();
            println!(
                "gallery: {components} components, {} cells ({refused} refused) -> {}",
                g.cells.len(),
                g.source.display()
            );
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("gallery: {why}");
            ExitCode::FAILURE
        }
    }
}

fn symbols(args: &[String]) -> ExitCode {
    const USAGE: &str =
        "usage: contract symbols <file.contract> [--stdin] [--editor] [--name <exact-name>]
       contract symbols <file.contract> --editor [--stdin] [--component <name>] [--tag <tag>]
              [--has <attr> …] [--where 'attr=expression' …] [--match <text> …] [--or <filters> …]
  --stdin reads the named file's text from stdin.
  --editor adds types, shapes and view elements. Element filters return only matching elements.
  --component and --tag match exactly. All --has and --where filters must match.
  Each --match is a case-sensitive substring of authored text or attributes. Any can match.
  --or starts another filter group. Elements that satisfy any group appear once.
  Example
    contract symbols app.contract --editor --component Editor --tag button --match 'Save draft' --or --component Preview --tag button --match Cancel";
    let Some(command) = symbol_args(args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let SymbolCommand::Query(SymbolArgs {
        input,
        stdin,
        editor,
        name,
        filters,
    }) = command
    else {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    };
    let path = std::path::Path::new(input);
    let query = || match (filters.is_empty(), editor) {
        (false, _) => contract::editor_elements_json(path, &filters),
        (true, true) => contract::editor_symbols_json(path, name),
        (true, false) => contract::symbols_json(path, name),
    };
    let symbols = if stdin {
        let mut text = String::new();
        if let Err(cause) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text) {
            eprintln!("contract symbols: stdin: {cause}");
            return ExitCode::from(1);
        }
        contract::with_contract_texts(vec![(path.into(), text)], query)
    } else {
        query()
    };
    match symbols {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        // What it found still prints, as `sources` prints a graph whose
        // loading stopped (LLP 1091 D10); `errors` and the exit say why.
        Err(partial) => {
            println!("{}", partial.json);
            for error in &partial.errors {
                eprintln!("{error}");
            }
            ExitCode::from(1)
        }
    }
}

struct SymbolArgs<'a> {
    input: &'a str,
    stdin: bool,
    editor: bool,
    name: Option<&'a str>,
    filters: Vec<contract::ElementFilters<'a>>,
}

enum SymbolCommand<'a> {
    Help,
    Query(SymbolArgs<'a>),
}

fn symbol_args(args: &[String]) -> Option<SymbolCommand<'_>> {
    let mut args = args.iter().map(String::as_str);
    let (mut input, mut name, mut group) = (None, None, None);
    let mut filters = Vec::new();
    let (mut stdin, mut editor) = (false, false);
    while let Some(arg) = args.next() {
        match arg {
            "--help" | "-h" => return Some(SymbolCommand::Help),
            "--stdin" if !stdin => stdin = true,
            "--editor" if !editor => editor = true,
            "--name" if input.is_some() && name.is_none() => name = Some(args.next()?),
            "--or" => filters.push(group.take()?),
            "--component" | "--tag" | "--match" | "--has" | "--where" => {
                let value = args.next().filter(|value| !value.is_empty())?;
                let filters = group.get_or_insert_with(contract::ElementFilters::default);
                match arg {
                    "--component" if filters.component.is_none() => filters.component = Some(value),
                    "--tag" if filters.tag.is_none() => filters.tag = Some(value),
                    "--match" => filters.text.push(value),
                    "--has" => filters.has.push(value),
                    "--where" => {
                        let (name, value) = value.split_once('=')?;
                        let (name, value) = (name.trim(), value.trim());
                        if name.is_empty() || value.is_empty() {
                            return None;
                        }
                        filters.values.push((name, value));
                    }
                    _ => return None,
                }
            }
            _ if !arg.starts_with('-') && input.is_none() => input = Some(arg),
            _ => return None,
        }
    }
    if let Some(group) = group {
        filters.push(group);
    } else if !filters.is_empty() {
        return None;
    }
    if !filters.is_empty() && (!editor || name.is_some()) {
        return None;
    }
    Some(SymbolCommand::Query(SymbolArgs {
        input: input?,
        stdin,
        editor,
        name,
        filters,
    }))
}

/// `contract sources`: every file compiling the root reads, as JSON, even
/// when loading stops (LLP 1091 D10) — what a capture, a watch and a deploy
/// take with the app. Exits 1 when the loader refused.
fn sources(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: contract sources <file.contract>";
    let input = match args {
        [flag] if matches!(flag.as_str(), "--help" | "-h") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [input] if !input.starts_with('-') => input,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let graph = contract::source_graph(std::path::Path::new(input));
    let sources: Vec<_> = graph
        .sources
        .iter()
        .map(|source| {
            let path = source.path.display().to_string();
            match &source.origin {
                contract::Origin::App => serde_json::json!({"path": path, "origin": "app"}),
                contract::Origin::Builtin => {
                    serde_json::json!({"path": path, "origin": "builtin"})
                }
                contract::Origin::Package {
                    name,
                    version,
                    root,
                    manifest,
                } => serde_json::json!({
                    "path": path,
                    "origin": "package",
                    "package": name,
                    "version": version,
                    "root": root.display().to_string(),
                    "manifest": manifest.display().to_string(),
                }),
            }
        })
        .collect();
    let errors: Vec<serde_json::Value> = graph
        .errors
        .iter()
        .map(|e| serde_json::from_str(&e.to_json()).unwrap_or_default())
        .collect();
    let packages: Vec<_> = graph
        .packages
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "version": p.version,
                "root": p.root.display().to_string(),
                "manifest": p.manifest.display().to_string(),
            })
        })
        .collect();
    let consulted: Vec<_> = graph
        .consulted
        .iter()
        .map(|m| m.display().to_string())
        .collect();
    println!(
        "{}",
        serde_json::json!({"sources": sources, "packages": packages, "consulted": consulted, "errors": errors})
    );
    if graph.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// TypeScript declarations or Rust bindings derived from the plan.
fn types(
    args: &[String],
    command: &str,
    out: &str,
    generate: fn(&std::path::Path) -> Result<String, String>,
) -> ExitCode {
    let usage = format!(
        "usage: contract {command} <file.contract> [-o <{out}>]{}",
        if command == "types" { " [--check]" } else { "" }
    );
    let checks = args.iter().filter(|a| a.as_str() == "--check").count();
    if checks > 1 || (checks > 0 && command != "types") {
        eprintln!("{usage}");
        return ExitCode::from(2);
    }
    let check = checks == 1;
    let args: Vec<_> = args
        .iter()
        .filter(|a| a.as_str() != "--check")
        .cloned()
        .collect();
    let args = args.as_slice();
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{usage}");
        return ExitCode::SUCCESS;
    }
    let valid = matches!(args, [input] if !input.starts_with('-'))
        || matches!(args, [input, flag, output] if !input.starts_with('-') && flag == "-o" && !output.starts_with('-'));
    if !valid {
        eprintln!("{usage}");
        return ExitCode::from(2);
    }
    // Shapes are all a declaration file needs: a game's surface arguments
    // never stop it, and are named as warnings (the platformer's diary, R4).
    let path = std::path::Path::new(&args[0]);
    if let Ok(surfaces) = contract::surface_findings(path) {
        surface_warnings(&surfaces);
    }
    if let Ok(warnings) = contract::resource_clock_warnings(path) {
        for finding in warnings {
            eprintln!("{}", warning(finding));
        }
    }
    let result = generate(path);
    match result {
        Ok(declarations) => {
            if check {
                let default = format!("{}.d.ts", args[0]);
                let output = args.get(2).unwrap_or(&default);
                if std::fs::read(output).ok().as_deref() != Some(declarations.as_bytes()) {
                    eprintln!(
                        "types: {output} is missing or stale; run contract types {} -o {output}",
                        args[0]
                    );
                    return ExitCode::from(1);
                }
                eprintln!("types: {output} is current");
                return ExitCode::SUCCESS;
            }
            if let Some(output) = args.get(2) {
                if let Err(error) = std::fs::write(output, declarations) {
                    eprintln!("{output}: {error}");
                    return ExitCode::from(1);
                }
                // Silence read as "did it run?" (app farm wave 2: 100); stdout stays empty.
                eprintln!("{command}: wrote {output}");
            } else {
                print!("{declarations}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

/// A game's surface-argument findings on stderr, as warnings: the
/// declaration is the last GPU build's, which a bake rewrites and checks.
fn surface_warnings(surfaces: &contract::SurfaceFindings) {
    for finding in &surfaces.findings {
        eprintln!("{}", warning(finding.clone()));
    }
    if let (false, Some(newer)) = (surfaces.findings.is_empty(), &surfaces.newer) {
        let stale = contract::CompileError {
            pass: "warning",
            id: "surface-declaration-stale".into(),
            message: ".shells/surfaces.json is older than this file: it names the arguments of the game's last GPU build; a web or native build rewrites it".into(),
            span: contract_syntax::Span {
                line: 1,
                col: 1,
                end_col: 1,
                source_id: 0,
            },
            file: Some(newer.as_path().into()),
            related: Box::new([]),
        };
        eprintln!("{stale}");
    }
}

/// `finding` as a warning, which prints as a lowering warning does
/// (`file:line:col warning [id] message`), the shape the editor's problem
/// matcher reads (editors/vscode, `$exact-contract`).
fn warning(finding: contract::CompileError) -> contract::CompileError {
    contract::CompileError {
        pass: "warning",
        ..finding
    }
}

/// `contract test <file.test.contract>`: the tests as JSON for the agent
/// driver (`bun scripts/agent.mjs <host> --test <file>` runs them).
fn tests(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: contract test <file.test.contract>";
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let input = match args {
        [input] if !input.is_empty() && !input.starts_with('-') => input,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let src = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{input}: {e}");
            return ExitCode::from(1);
        }
    };
    match contract_syntax::parse(&src) {
        Ok(file) => {
            let test = std::path::Path::new(input);
            let paired = test.with_file_name(
                test.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .replace(".test.contract", ".contract"),
            );
            let app = test.with_file_name("app.contract");
            let app = if app.is_file() { app } else { paired };
            let substitutes = contract::check_test_substitutes(&app, &file, None);
            let mut t = contract::tests_with_defaults(file.tests, &file.launch);
            let mut p = file.properties;
            let properties = if p.is_empty() {
                Ok(())
            } else {
                contract::resolve_properties_path(&app, &mut p)
            };
            let states = contract::resolve_test_states_path(&app, &mut t);
            let errors: Vec<_> = properties
                .err()
                .into_iter()
                .flatten()
                .chain(states.err().into_iter().flatten())
                .chain(substitutes.err().into_iter().flatten())
                .collect();
            if !errors.is_empty() {
                for e in errors {
                    if e.file.is_some() {
                        eprintln!("{e}");
                    } else {
                        eprintln!("{input}:{e}");
                    }
                }
                return ExitCode::from(1);
            }
            println!("{}", contract::authored_json(&t, &p));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{input}:{e}");
            ExitCode::from(1)
        }
    }
}

/// Explicit formatting, with read-only preview and check modes, and one for
/// an editor's unsaved text; `--uses` writes the `use` lines each file of the
/// program lacks (LLP 1091 D1).
fn fmt(args: &[String]) -> ExitCode {
    const USAGE: &str = "usage: contract fmt [--check | --stdout | --stdin] <file.contract>
       contract fmt --uses <file.contract>
  --stdin: stdin's text, formatted to stdout; <file.contract> names it and is not read";
    const MODES: [&str; 3] = ["--check", "--stdout", "--stdin"];
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match args {
        [flag, input] | [input, flag] if flag == "--uses" && !input.starts_with('-') => {
            return fix_uses(input);
        }
        _ => {}
    }
    let (input, mode) = match args {
        [input] if !input.starts_with('-') => (input, ""),
        [flag, input] | [input, flag]
            if MODES.contains(&flag.as_str()) && !input.starts_with('-') =>
        {
            (input, flag.as_str())
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let before = match mode {
        "--stdin" => std::io::read_to_string(std::io::stdin()),
        _ => std::fs::read_to_string(input),
    };
    let result = before.map_err(|e| e.to_string()).and_then(|before| {
        let after = contract_syntax::fmt::format(&before).map_err(|e| e.to_string())?;
        Ok((before, after))
    });
    match result {
        Ok((before, after)) => match mode {
            "--stdout" | "--stdin" => {
                print!("{after}");
                ExitCode::SUCCESS
            }
            "--check" if before != after => {
                print!("{}", diff::unified(input, &before, &after));
                ExitCode::from(1)
            }
            "--check" => ExitCode::SUCCESS,
            _ if before == after => ExitCode::SUCCESS,
            _ => match std::fs::write(input, after) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{input}: {error}");
                    ExitCode::from(1)
                }
            },
        },
        Err(error) => {
            eprintln!("{input}:{error}");
            ExitCode::from(1)
        }
    }
}

/// `contract fmt --uses <root.contract>`: each file written and its lines,
/// then whatever no line answers, refused as the build refuses it.
fn fix_uses(input: &str) -> ExitCode {
    match contract::fix_uses(std::path::Path::new(input)) {
        Ok((written, refused)) => {
            for (path, lines) in &written {
                println!("{}:", path.display());
                for line in lines {
                    println!("  {line}");
                }
            }
            // A file no `use` reaches is no part of this program, so its
            // names were not checked (euxy FRICTION 1): say which.
            let unreached = contract::unreached(std::path::Path::new(input));
            let names: Vec<String> = unreached
                .iter()
                .filter_map(|p| Some(p.file_name()?.to_string_lossy().into_owned()))
                .collect();
            if written.is_empty() && refused.is_empty() && names.is_empty() {
                println!("{input}: every file names what it uses");
            } else if refused.is_empty() && !names.is_empty() {
                let are = if names.len() == 1 { "is" } else { "are" };
                println!(
                    "{input}: every file it reaches names what it uses; {} beside it {are} not reached from it (another entry, or a file it uses nothing from)",
                    names.join(", ")
                );
            }
            for e in &refused {
                eprintln!("{e}");
            }
            if refused.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(all) => {
            for e in &all {
                eprintln!("{e}");
            }
            ExitCode::from(1)
        }
    }
}

/// `contract lean <file.contract>`: the program as a `Contract.Program`
/// term of the Lean semantics (`semantics/`), after the whole compiler
/// accepts it.
fn lean(args: &[String]) -> ExitCode {
    const USAGE: &str =
        "usage: contract lean <file.contract> [--components] [--name <ident>] [-o <file.lean>]";
    let mut input = None;
    let mut name = "program".to_string();
    let mut output = None;
    let mut components = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--components" => components = true,
            "--name" => match it.next() {
                Some(n) => name = n.clone(),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "-o" => match it.next() {
                Some(o) => output = Some(o.clone()),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other if !other.starts_with('-') && input.is_none() => input = Some(other.to_string()),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(input) = input else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let path = std::path::Path::new(&input);
    let emitted = if components {
        contract::lean::lean_components_path(path, &name)
    } else {
        contract::lean::lean_path(path, &name)
    };
    match emitted {
        Ok(text) => {
            let text = format!("import Contract\n\n{text}");
            match output {
                Some(o) => {
                    if let Err(e) = std::fs::write(&o, text) {
                        eprintln!("{o}: {e}");
                        return ExitCode::from(1);
                    }
                }
                None => print!("{text}"),
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
