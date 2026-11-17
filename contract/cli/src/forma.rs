//! `contract forma check | eval | ir` — an app's design on its own (LLP
//! 1121 D2): the file the compiler reads (`app.json`'s `"forma"`, else
//! `app.forma` beside `app.contract`), checked against the schema's roles
//! by the build's one evaluator (`exact-forma`), its tokens in one app tuple,
//! or its IR (`forma.json`, generated and never committed).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage:
  contract forma check <app dir | app.contract | file.forma>
  contract forma eval <app dir | app.contract | file.forma> [--mode <axis>=<value>[,…]]
  contract forma ir <app dir | app.contract | file.forma> [-o <forma.json>]
  contract forma fmt <app dir | app.contract | file.forma> [--check]
  contract forma serve --stdio <app dir>   (the design's one writer, forma/1 over JSON lines)
  contract forma gen ts|rust|swift|kotlin <app dir | app.contract | file.forma> [-o <dir>]   (bindings, in the app's .exact/forma/)
  contract forma import css <stylesheet> [--app <app dir>]   (its custom properties written into the design, between markers a later import rewrites)
  contract forma import shadcn|tailwind <stylesheet> | dtcg <file or dir> | pen <file.pen> | astryx <resolved json> [--app <app dir>] [--prefix <prefix>]   (a proposal, as JSON)
  contract forma import-plan <design file>   (one import planned alone: {request, design} on stdin, {ok} or {error} out; the wire's)
  contract forma export dtcg <app dir | app.contract | file.forma> [-o <file>]   (DTCG 2025.10; its baseline kept in .exact/)
  (a flag may come before the path or after it)";

pub fn run(args: &[String]) -> ExitCode {
    if args.first().is_some_and(|a| a == "serve") {
        return crate::forma_serve::run(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "import-plan") {
        return crate::forma_import::plan_alone(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "import") && args.get(1).is_some_and(|a| a == "css") {
        return crate::forma_import_css::run(&args[2..]);
    }
    if args.first().is_some_and(|a| a == "import") {
        return crate::forma_import::run(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "gen") {
        return crate::forma_gen::run(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "export") {
        return crate::forma_export::run(&args[1..]);
    }
    if matches!(args, [] | [_]) || args.iter().any(|a| a == "--help" || a == "-h") {
        let help = args.iter().any(|a| a == "--help" || a == "-h");
        println!("{USAGE}");
        return if help {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        };
    }
    // The one path, and a flag with its value before or after it.
    let verb = args[0].as_str();
    let (mut input, mut rest) = (None, Vec::new());
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => {
                rest.push("--check".to_string());
                i += 1;
            }
            flag @ ("--mode" | "-o") if i + 1 < args.len() => {
                rest.push(flag.to_string());
                rest.push(args[i + 1].clone());
                i += 2;
            }
            path if input.is_none() && !path.starts_with('-') => {
                input = Some(path.to_string());
                i += 1;
            }
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
    if refuse_input(Path::new(&input)) {
        return ExitCode::from(2);
    }
    let rest = rest.as_slice();
    // A verb takes only its own flag: refused before the design is read.
    let fits = match (verb, rest) {
        ("check", []) | ("eval", []) | ("ir", []) | ("fmt", []) => true,
        ("fmt", [flag]) => flag == "--check",
        ("eval", [flag, _]) => flag == "--mode",
        ("ir", [flag, _]) => flag == "-o",
        _ => false,
    };
    if !fits {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    // The app's components' params (LLP 1121 D15) fold with its design; an
    // app with params and no design has them alone. A design named apart
    // (a `.forma` the app does not build) is checked alone.
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let built = app_contract(Path::new(&input));
    let apart = Path::new(&input).extension().is_some_and(|e| e == "forma")
        && contract::design_path(&built).map(|d| canonical(&d))
            != Some(canonical(Path::new(&input)));
    let app = match apart {
        true => contract::forma_params::AppParams::default(),
        false => contract::forma_params::params_of(&built),
    };
    let params = app.tokens.as_slice();
    let (path, src) = match design(Path::new(&input)) {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(src) => (path, src),
            Err(e) => {
                eprintln!("contract forma: {}: {e}", path.display());
                return ExitCode::from(2);
            }
        },
        // Params or packages' designs (LLP 1121 D11) and no design of its own: theirs alone.
        None if app.inline && !app.packages.is_empty() && verb != "fmt" => {
            eprintln!("contract forma: {input}'s tokens are an inline `theme`, which `contract forma` doesn't fold with its packages' designs; `contract build` checks them");
            return ExitCode::from(2);
        }
        None if (!params.is_empty() || !app.packages.is_empty()) && verb != "fmt" => {
            let dir = app_contract(Path::new(&input))
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            (dir.join("app.forma"), "forma app\n".to_string())
        }
        None => {
            eprintln!("contract forma: {input} has no design (an app.forma, or the file app.json's \"forma\" names)");
            return ExitCode::from(2);
        }
    };
    if verb == "fmt" {
        return fmt(&path, &src, !rest.is_empty());
    }
    // Its params and its packages' tokens (LLP 1121 D11) laid in, as the build lays them.
    let folded = match (params.is_empty(), app.packages.is_empty()) {
        (true, true) => exact_forma::check(&src, &contract_lower::forma_vocab()),
        // With packages: what the build refuses of them too (`fold_with`).
        (_, false) => {
            contract::forma_params::fold_with(&src, &app, &contract_lower::forma_vocab(), None)
                .map(|(folding, _)| folding)
        }
        (false, true) => exact_forma::parse(&src).and_then(|d| {
            exact_forma::check_design(
                contract::forma_params::laid(&d, &app, None),
                &contract_lower::forma_vocab(),
            )
        }),
    };
    let folding = match folded {
        Ok(folding) => folding,
        Err(all) => {
            for d in &all {
                eprintln!("{}", contract::forma_params::located(d, &app, &path));
            }
            // A profile's colour is the compiler's to resolve, with the
            // declarations in the app's Contract (LLP 1100 D3).
            let lines: Vec<&str> = src.lines().collect();
            let profiled = |d: &exact_forma::Diagnostic| {
                lines
                    .get(d.line as usize - 1)
                    .is_some_and(|l| l.contains("color(--"))
            };
            if all.iter().all(profiled) {
                eprintln!("contract forma: a profile's colour (`color(--…)`) is resolved by the compiler with the app's declarations; `contract build` checks this design");
                return ExitCode::from(2);
            }
            return ExitCode::from(1);
        }
    };
    // What the compiler refuses of a design beyond the crate's check (a
    // CSS word as a name, a role that varies by an axis …), as a build would.
    // An app with params is built itself, as it builds its design; so is
    // one whose Contract did not load (its params are unknown), so the
    // build says why.
    // An app with params or package designs (LLP 1121 D11) is admitted by its own build.
    // The app's `assets/`, where its build reads its icons' SVGs: beside its
    // app.contract, or beside a design checked apart from any app.
    // A design checked by its own path whose app is a directory above it
    // (app.json's "forma" naming it) reads that app's.
    let home = match apart {
        false => built.parent().map(Path::to_path_buf),
        true => {
            let file = canonical(&path);
            file.ancestors()
                .skip(1)
                .find(|d| {
                    let at = d.join("app.contract");
                    at.is_file()
                        && contract::design_path(&at).map(|p| canonical(&p)).as_ref() == Some(&file)
                })
                .map(Path::to_path_buf)
                .or_else(|| path.parent().map(Path::to_path_buf))
        }
    };
    let svgs = svgs(&folding, home.as_deref(), &app);
    let refused = match admission(
        &src,
        &svgs,
        (!params.is_empty() || app.failed || !app.packages.is_empty()).then_some(built),
    ) {
        Ok(refused) => refused,
        Err(why) => {
            eprintln!("contract forma: {why}");
            return ExitCode::from(2);
        }
    };
    if !refused.is_empty() {
        for (file, line, col, id, message) in &refused {
            let file = file.clone().unwrap_or_else(|| path.clone());
            eprintln!("{}:{line}:{col}: {id}: {message}", file.display());
        }
        return ExitCode::from(1);
    }
    match (verb, rest) {
        ("check", []) => {
            // The author's information, never a refusal (LLP 1121 D3).
            for w in &folding.warnings {
                eprintln!("{}", contract::forma_params::located(w, &app, &path));
            }
            let design = &folding.design;
            // A param is its component's, counted apart (D15).
            let tokens =
                folding.tokens.len() - params.iter().filter(|p| p.param.kind != "icon").count();
            let counted = match params.len() {
                0 => String::new(),
                n => format!(", {n} param{}", if n == 1 { "" } else { "s" }),
            };
            println!(
                "{}: {} token{}{counted}, {} ax{} ({} tuple{}): checked",
                path.display(),
                tokens,
                if tokens == 1 { "" } else { "s" },
                design.axes.len(),
                if design.axes.len() == 1 { "is" } else { "es" },
                folding.tuples,
                if folding.tuples == 1 { "" } else { "s" }
            );
            ExitCode::SUCCESS
        }
        ("eval", []) => eval(&folding, ""),
        ("eval", [flag, modes]) if flag == "--mode" => eval(&folding, modes),
        ("ir", []) => {
            println!("{}", ir(&src, &app, &folding));
            ExitCode::SUCCESS
        }
        ("ir", [flag, out]) if flag == "-o" => {
            match std::fs::write(out, ir(&src, &app, &folding) + "\n") {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("contract forma: {out}: {e}");
                    ExitCode::from(2)
                }
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Refuse former names only after the caller has parsed a design or app path.
pub(crate) fn refuse_input(input: &Path) -> bool {
    let message = if input.extension().is_some_and(|e| e == "tamber") {
        Some(format!(
            "rename {} to {}",
            input.display(),
            input.with_extension("forma").display()
        ))
    } else {
        contract::design_name_error(&app_contract(input))
    };
    if let Some(message) = message {
        eprintln!("contract forma: {message}");
        return true;
    }
    false
}

/// Each SVG the build reads for `folding`'s icons, where it reads it: under
/// the assets of the root whose file writes it (a package's under its root,
/// stage 13), by its path there. `home` is the app's directory.
pub(crate) fn svgs<'f>(
    folding: &'f exact_forma::Folding,
    home: Option<&Path>,
    app: &contract::forma_params::AppParams,
) -> Vec<(PathBuf, &'f str, Option<PathBuf>)> {
    let mut svgs: Vec<(PathBuf, &str, Option<PathBuf>)> = folding
        .icons
        .iter()
        .filter_map(|i| {
            let svg = i.svg.as_deref()?;
            let at = contract_lower::icons::Home::of(i, home, &app.packages, &app.tokens);
            // A package's read as the build reads it (round 3, R3-4): one that leaves it is
            // not copied, and is refused as the build refuses it.
            let root = at.package.and(at.root).map(Path::to_path_buf);
            Some((at.svg(svg)?, svg, root))
        })
        .collect();
    svgs.sort_unstable();
    svgs.dedup();
    svgs
}

/// The compiler's refusals of design `src`, read as the design of an app
/// with one empty view and nothing else (no manifest: nothing in one but
/// `"forma"` reaches a design, and its other files would stop the build
/// first): each `(file, line, col, id, message)`, the file `None` for the
/// design's. An app with params (`app`, its `app.contract`) is built
/// itself, since its params read the design (a dial only a param reads is
/// read), and each refusal is its own file's. An `Err` when the build could
/// not be made, or stopped before the design.
#[allow(clippy::type_complexity)]
pub(crate) fn admission(
    src: &str,
    svgs: &[(PathBuf, &str, Option<PathBuf>)],
    app: Option<PathBuf>,
) -> Result<Vec<(Option<PathBuf>, u32, u32, String, String)>, String> {
    if let Some(app) = app {
        return Ok(match contract::compile_path_all(&app, false) {
            Ok(_) => Vec::new(),
            Err(all) => all
                .into_iter()
                .map(|e| {
                    (
                        e.file.map(PathBuf::from),
                        e.span.line,
                        e.span.col,
                        e.id,
                        e.message,
                    )
                })
                .collect(),
        });
    }
    let dir = std::env::temp_dir().join(format!("contract-forma-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let made = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join("app.forma"), src))
        .and_then(|()| {
            std::fs::write(
                dir.join("app.contract"),
                "component App\n  view\n    column\n",
            )
        })
        // The design's icons read their SVGs under the app's `assets/`, as
        // its build reads them: each one it names copied there (one the app
        // lacks is not, and is refused as the build refuses it).
        .and_then(|()| {
            for (from, svg, package) in svgs {
                let bytes = match package {
                    Some(root) => match contract_lower::icons::read_confined(from, root) {
                        Ok(bytes) => bytes,
                        Err(_) => continue,
                    },
                    None if from.is_file() => std::fs::read(from)?,
                    None => continue,
                };
                let to = dir.join("assets").join(svg);
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                // Its bytes, in a file of the copy's own (a read-only
                // source's permissions not carried over).
                std::fs::write(to, bytes)?;
            }
            Ok(())
        });
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("{}: {e}", dir.display()));
    }
    let result = contract::compile_path_all(&dir.join("app.contract"), false);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(_) => Ok(Vec::new()),
        Err(all) => {
            let (design, other): (Vec<_>, Vec<_>) = all
                .into_iter()
                .partition(|e| e.file.as_ref().is_some_and(|f| f.ends_with("app.forma")));
            match (design.is_empty(), other.first()) {
                (true, Some(e)) => Err(format!(
                    "the design's build stopped first: [{}] {}",
                    e.id, e.message
                )),
                _ => Ok(design
                    .into_iter()
                    .map(|e| (None, e.span.line, e.span.col, e.id, e.message))
                    .collect()),
            }
        }
    }
}

/// The design `input` names: a `.forma` file, or an app's (its directory or
/// its `app.contract`), where the compiler finds it.
pub(crate) fn design(input: &Path) -> Option<PathBuf> {
    if input.extension().is_some_and(|e| e == "forma") {
        return Some(input.to_path_buf());
    }
    let root = if input.is_dir() {
        input.join("app.contract")
    } else {
        input.to_path_buf()
    };
    contract::design_path(&root).filter(|p| p.is_file())
}

/// Each token's value in the tuple `modes` names (`axis=value`, by commas or
/// spaces, each axis once; the rest at their defaults), as the plan carries
/// it.
fn eval(folding: &exact_forma::Folding, modes: &str) -> ExitCode {
    let mut pins = Vec::new();
    for pin in modes.split([',', ' ']).filter(|p| !p.is_empty()) {
        match pin.split_once('=') {
            Some((axis, value)) if !pins.iter().any(|(a, _)| *a == axis) => {
                pins.push((axis, value))
            }
            _ => {
                eprintln!("contract forma eval: `{pin}`: a mode is `axis=value`, each axis once");
                return ExitCode::from(2);
            }
        }
    }
    let Some(tuple) = folding.tuple(&pins) else {
        eprintln!("contract forma eval: `{modes}` names an axis or value the design lacks");
        return ExitCode::from(2);
    };
    // A param is read by its full name, in `ir`'s `components` (D15).
    let tokens = folding
        .tokens
        .iter()
        .filter(|t| !t.name.starts_with(contract_lower::params::PREFIX));
    for token in tokens {
        if let Some(value) = folding.value(&token.name, tuple) {
            println!("{} = {}", token.name, value.css());
        }
    }
    ExitCode::SUCCESS
}

/// `forma fmt` (LLP 1121 §3): the design in its canonical layout, written
/// in place; with `check`, nothing written and exit 1 where it would change.
fn fmt(path: &Path, src: &str, check: bool) -> ExitCode {
    let formatted = match exact_forma::format(src) {
        Ok(f) => f,
        Err(all) => {
            for d in &all {
                eprintln!("{}:{d}", path.display());
            }
            return ExitCode::from(1);
        }
    };
    if formatted == src {
        return ExitCode::SUCCESS;
    }
    if check {
        eprintln!("{}: not in forma fmt's layout", path.display());
        return ExitCode::from(1);
    }
    match std::fs::write(path, formatted) {
        Ok(()) => {
            println!("{}: formatted", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("contract forma: {}: {e}", path.display());
            ExitCode::from(2)
        }
    }
}

/// The design's IR, with its app's components' params (LLP 1121 D15) when
/// it declares any, or uses an exported library component (its parts and
/// variants, stage 12 D3): under `components` by full name, never among the
/// tokens by their names below the IR.
fn ir(
    src: &str,
    app: &contract::forma_params::AppParams,
    folding: &exact_forma::Folding,
) -> String {
    if app.tokens.is_empty() && app.library.is_empty() {
        return folding.ir();
    }
    let Ok(mut ir) = serde_json::from_str::<serde_json::Value>(&folding.ir()) else {
        return folding.ir();
    };
    // (as `describe` lists them: a library component with no params too, stage 12 D3)
    if !app.tokens.is_empty() || app.library.iter().any(|(_, _, d)| d.library) {
        contract::forma_params::hide(&mut ir);
        let design = exact_forma::parse(src).ok();
        ir["components"] = contract::forma_params::components(
            app,
            design.as_ref(),
            &contract_lower::forma_vocab(),
        );
    }
    // Parts and variants, in the IR file only (LLP 1139 D5; theme2's Q2).
    if !app.library.is_empty() {
        ir["library"] = contract::forma_params::library(app, exact_forma::parse(src).ok().as_ref());
    }
    serde_json::to_string_pretty(&ir).unwrap_or_else(|_| folding.ir())
}

/// The app's root Contract for an input: the directory's `app.contract`,
/// the file itself, or the `app.contract` beside a design.
fn app_contract(input: &Path) -> std::path::PathBuf {
    if input.is_dir() {
        return input.join("app.contract");
    }
    if input.extension().is_some_and(|e| e == "contract") {
        return input.to_path_buf();
    }
    input
        .parent()
        .unwrap_or(Path::new("."))
        .join("app.contract")
}
