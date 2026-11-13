//! Compilation and advice share a loaded program while retaining separate diagnostics.

use crate::{
    app_root, compile_loaded, read_source, sources, surface, warning_rank, CompileError,
    SurfaceFindings, Warned,
};
use contract_syntax::File;
use std::path::Path;

type Found<T> = Result<T, Vec<CompileError>>;

/// A compilation and the CLI's independent surface and clock advice.
pub struct Advised {
    /// The plan or refusals, with lowering's warnings in rank order.
    pub compiled: Warned,
    /// Surface findings, including whether their declaration is stale.
    pub surfaces: Found<SurfaceFindings>,
    /// Non-blocking resource clock warnings, absent when loading was refused.
    pub clocks: Vec<CompileError>,
}

/// Compile and collect CLI advice from one source load. Syntax refusals still
/// allow partial checking, but suppress advice about the incomplete program.
///
/// Under `Profile::Terminal` the plan is lowered for the terminal and says
/// `surface terminal` (LLP 1101.003 D9), and what the terminal profile
/// refuses (LLP 1101 D2) is a warning: this is `contract build --surface
/// terminal`, which measures a GUI source on the terminal host (LLP 1121
/// stage 11 C). A terminal entry compiles through
/// [`crate::compile_path_terminal`], which refuses it.
pub fn compile_path_advised(
    path: &Path,
    mapped: bool,
    profile: contract_lower::Profile,
) -> Advised {
    let mut refused = sources::Refused::default();
    let loaded = read_source(path).map_err(|e| vec![e]).and_then(|src| {
        let root = app_root(path)?;
        let (file, sources) = sources::load_partial(path, &src, &root, &mut refused)?;
        Ok((root, file, sources))
    });
    let (root, file, sources) = match loaded {
        Ok(loaded) => loaded,
        Err(errors) => {
            let errors = refused.join::<()>(Err(errors)).unwrap_err();
            return Advised {
                surfaces: Err(errors.clone()),
                clocks: Vec::new(),
                compiled: (Err(errors), Vec::new()),
            };
        }
    };
    let (surfaces, clocks) = if refused.errors.is_empty() {
        (
            surface_advice(&root, &file, &sources),
            clock_advice(&file, &sources),
        )
    } else {
        (Err(refused.errors.clone()), Vec::new())
    };
    let mut warnings = Vec::new();
    if profile == contract_lower::Profile::Terminal {
        let unadmitted = crate::terminal::check(&file).err().unwrap_or_default();
        warnings.extend(unadmitted.into_iter().map(|e| CompileError {
            pass: "warning",
            ..sources.resolve(e)
        }));
    }
    let compiled = compile_loaded(
        path,
        &root,
        file,
        sources,
        mapped,
        false,
        profile,
        Some(&mut warnings),
    );
    if !refused.errors.is_empty() {
        warnings.clear();
    }
    warnings.sort_by_key(warning_rank);
    Advised {
        compiled: (refused.join(compiled), warnings),
        surfaces,
        clocks,
    }
}

/// Surface findings for a complete program; unreadable sources or declarations refuse.
pub fn surface_findings(path: &Path) -> Found<SurfaceFindings> {
    let src = read_source(path).map_err(|e| vec![e])?;
    let root = app_root(path)?;
    let (file, sources) = sources::load(path, &src, &root)?;
    surface_advice(&root, &file, &sources)
}

fn surface_advice(root: &Path, file: &File, sources: &sources::Sources) -> Found<SurfaceFindings> {
    let findings = match surface::arguments(root).map_err(|e| vec![e])? {
        Some(declared) => contract_analyze::check_surface_arguments(file, &declared)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| sources.resolve(e.into()))
            .collect(),
        None => Vec::new(),
    };
    Ok(SurfaceFindings {
        findings,
        newer: surface::newer_rust(root),
    })
}

/// Non-blocking advice for resource arguments fed by a live frame clock.
/// Includes imported derives and functions, with the authored resource location.
pub fn resource_clock_warnings(path: &Path) -> Found<Vec<CompileError>> {
    let src = read_source(path).map_err(|e| vec![e])?;
    let root = app_root(path)?;
    let (file, sources) = sources::load(path, &src, &root)?;
    Ok(clock_advice(&file, &sources))
}

fn clock_advice(file: &File, sources: &sources::Sources) -> Vec<CompileError> {
    contract_analyze::resource_clock_warnings(file)
        .into_iter()
        .map(|e| sources.resolve(e.into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_loading_keeps_the_independent_passes_results() {
        let dir = std::env::temp_dir().join(format!("contract-advice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.contract");
        let diagnostics =
            |errors: &[CompileError]| errors.iter().map(CompileError::to_json).collect::<Vec<_>>();
        for source in [
            "component App\n  view\n    text \"ok\"\n",
            "shape Answer\n  n: number\ncomponent App\n  derive clock = performanceNow()\n  resource answer = query(clock) as shape Answer\n  view\n    text \"clock\"\n",
            "component App\n  state n = missing\n  view\n    text \"bad type\"\n",
            "component App\n  state n =\n  state m = missing\n  view\n    text \"partial\"\n",
            "use Missing from \"./absent.contract\"\ncomponent App\n  view\n    text \"missing file\"\n",
        ] {
            std::fs::write(&path, source).unwrap();
            sources::LOADS.with(|loads| loads.set(0));
            let advised = compile_path_advised(&path, true, contract_lower::Profile::Web);
            assert_eq!(sources::LOADS.with(|loads| loads.get()), 1);
            let (compiled, warnings) = crate::compile_path_all_warned(&path, true);
            assert_eq!(diagnostics(&warnings), diagnostics(&advised.compiled.1));
            match (compiled, advised.compiled.0) {
                (Ok((plan, map)), Ok((shared, shared_map))) => {
                    let bytes = plan.encode();
                    assert_eq!(bytes, shared.encode());
                    assert_eq!(map.unwrap().json(&bytes), shared_map.unwrap().json(&bytes));
                }
                (Err(a), Err(b)) => assert_eq!(diagnostics(&a), diagnostics(&b)),
                _ => panic!("shared loading changed admission"),
            }
            match (surface_findings(&path), advised.surfaces) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a.newer, b.newer);
                    assert_eq!(diagnostics(&a.findings), diagnostics(&b.findings));
                }
                (Err(a), Err(b)) => assert_eq!(diagnostics(&a), diagnostics(&b)),
                _ => panic!("shared loading changed surface advice"),
            }
            assert_eq!(diagnostics(&resource_clock_warnings(&path).unwrap_or_default()), diagnostics(&advised.clocks));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
