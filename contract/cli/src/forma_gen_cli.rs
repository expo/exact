//! Command-line parsing, validation and file output for Forma bindings.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str =
    "usage: contract forma gen ts|rust|swift|kotlin <app dir | app.contract | file.forma> [-o <dir>]";

pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let (mut target, mut input, mut out) = (None, None, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" if i + 1 < args.len() => {
                out = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            t @ ("ts" | "rust" | "swift" | "kotlin") if target.is_none() => {
                target = Some(t.to_string())
            }
            path if input.is_none() && !path.starts_with('-') => input = Some(path.to_string()),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let (Some(target), Some(input)) = (target, input) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if crate::forma::refuse_input(Path::new(&input)) {
        return ExitCode::from(2);
    }
    // An app whose design is its packages' alone (LLP 1121 D11) has theirs, laid beside an empty one.
    let alone = || {
        let dir = Path::new(&input);
        let contract = dir.join("app.contract");
        (dir.is_dir()
            && !contract::forma_params::params_of(&contract)
                .packages
                .is_empty())
        .then(|| dir.join("app.forma"))
    };
    let Some(path) = crate::forma::design(Path::new(&input)).or_else(alone) else {
        eprintln!("contract forma gen: {input} has no design (an app.forma, or the file app.json's \"forma\" names)");
        return ExitCode::from(2);
    };
    // The app's own `.exact/`: the directory named, or the one its
    // `app.contract` is in; given a design file, the nearest directory
    // above it whose app that design is (else the file's own).
    let app = Path::new(&input);
    let root = match app.is_dir() {
        true => app.to_path_buf(),
        false => {
            let beside = app.parent().unwrap_or(Path::new(".")).to_path_buf();
            let canonical = |p: &Path| p.canonicalize().ok();
            beside
                .ancestors()
                .find(|dir| {
                    dir.join("app.contract").is_file()
                        && crate::forma::design(dir).and_then(|d| canonical(&d)) == canonical(&path)
                })
                .map_or(beside.clone(), Path::to_path_buf)
        }
    };
    // The app's components' params, tokens of its design (D15), as its
    // build checks them: a build that refuses writes no names.
    let contract = root.join("app.contract");
    if contract.is_file() {
        if let Err(e) = contract::compile_path(&contract) {
            eprintln!("{}: {}: {}", contract.display(), e.id, e.message);
            return ExitCode::from(1);
        }
    }
    let params = contract::forma_params::params_of(&contract);
    let files = match std::fs::read_to_string(&path)
        .or_else(|e| match path.exists() || params.packages.is_empty() {
            true => Err(e),
            false => Ok("forma app\n".to_string()),
        })
        .map_err(|e| vec![e.to_string()])
        .and_then(|src| contract::forma_gen::files(&target, &path, &src, &params))
    {
        Ok(files) => files,
        Err(all) => {
            for e in all {
                eprintln!("{e}");
            }
            return ExitCode::from(1);
        }
    };
    let dir = out.unwrap_or_else(|| root.join(".exact").join("forma"));
    for (name, text) in files {
        let file = dir.join(name);
        // Unchanged, not written: a watcher sees nothing move.
        if std::fs::read_to_string(&file).is_ok_and(|old| old == text) {
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, text)) {
            eprintln!("contract forma gen: {}: {e}", file.display());
            return ExitCode::from(2);
        }
        println!("{}", file.display());
    }
    ExitCode::SUCCESS
}
