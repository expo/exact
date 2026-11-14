//! Source expressions behind executable rows for translation validation.
use super::*;

/// Embed the checked instance expressions, before lowering replaces their
/// reads by executable row loads. Names join them to the plan's derive rows.
pub fn shared_expressions(
    src: &str,
    path: Option<&Path>,
) -> Result<Vec<(String, String)>, CompileError> {
    let mut file = match path {
        Some(path) => load(path, src)?,
        None => contract_syntax::parse(src)?,
    };
    contract_syntax::resolve_clock_timelines(&mut file)?;
    crate::params(&mut file)?;
    crate::vars(&mut file)?;
    let strings = path.map(strings_beside).transpose()?.flatten();
    let checked = contract_types::check_all(&file, false, contract_lower::tags::style, strings)
        .map_err(|mut all| CompileError::from(all.swap_remove(0)))?;
    let e = Emitter {
        out: String::new(),
        root: checked.expanded.root.clone(),
        checked: &checked,
        format_used: Default::default(),
        records: Default::default(),
    };
    checked
        .expanded
        .thunks
        .iter()
        .map(|t| Ok((t.name.clone(), e.expr(&t.expr)?)))
        .collect()
}
