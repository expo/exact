//! Authored tests and property read validation.

use crate::CompileError;
use contract_syntax::{PropertyDecl, Step, TestDecl};
use std::path::Path;

/// Parse properties without compiling an application.
pub fn properties(src: &str) -> Result<Vec<PropertyDecl>, CompileError> {
    Ok(contract_syntax::parse(src)?.properties)
}

/// Validate authored properties against the app before running its scripts.
pub fn check_properties_path(
    path: &Path,
    properties: &[PropertyDecl],
) -> Result<(), Vec<CompileError>> {
    resolve_properties_path(path, &mut properties.to_vec())
}

/// Validate properties and bind their action scopes to the app's expanded names.
pub fn resolve_properties_path(
    path: &Path,
    properties: &mut [PropertyDecl],
) -> Result<(), Vec<CompileError>> {
    let (file, expanded) = property_program(path)?;
    check_properties(&file, &expanded, properties)
}

/// Validate reads and resolve action scopes in a loaded program.
pub fn check_properties(
    file: &contract_syntax::File,
    expanded: &contract_syntax::Expanded,
    properties: &mut [PropertyDecl],
) -> Result<(), Vec<CompileError>> {
    let errors: Vec<_> = contract_analyze::check_properties(&expanded.root, &file.fns, properties)
        .into_iter()
        .chain(contract_analyze::resolve_actions(expanded, properties))
        .map(CompileError::from)
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check package substitutes against the names linked into this app.
pub fn check_test_substitutes(
    path: &Path,
    file: &contract_syntax::File,
    compiled: Option<&exact_plan::Plan>,
) -> Result<(), Vec<CompileError>> {
    let mut hosts = Vec::new();
    if file.tests.iter().all(|t| !t.hosts.is_empty()) {
        for host in file.tests.iter().flat_map(|t| &t.hosts) {
            if !hosts.contains(host) {
                hosts.push(host.clone());
            }
        }
    }
    let steps: Vec<_> = file
        .launch
        .iter()
        .map(|s| (s, &hosts))
        .chain(
            file.tests
                .iter()
                .flat_map(|t| t.steps.iter().map(|s| (s, &t.hosts))),
        )
        .filter_map(|(s, hosts)| match s {
            Step::PreferExtension { key, value, span } => Some((key, value, *span, hosts)),
            _ => None,
        })
        .collect();
    if steps.is_empty() {
        return Ok(());
    }
    let built;
    let plan = match compiled {
        Some(plan) => plan,
        None => {
            built = crate::compile_path(path).map_err(|e| vec![e])?;
            &built
        }
    };
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let mut errors = Vec::new();
    let mut declarations = std::collections::BTreeMap::new();
    for (key, value, span, hosts) in steps {
        if let Err(why) =
            test_substitute(plan, root, key, value.as_deref(), hosts, &mut declarations)
        {
            errors.push(crate::modules::error(
                "test-extension-substitute",
                format!("prefer extension: {why}"),
                span,
            ));
        }
    }
    check_launch_arrivals(plan, file, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// Leading values reach the package together, including a tag list written after its field.
fn check_launch_arrivals(
    plan: &exact_plan::Plan,
    file: &contract_syntax::File,
    errors: &mut Vec<CompileError>,
) {
    use serde_json::Value;
    use std::collections::BTreeMap;
    let mut arrivals = BTreeMap::new();
    for row in &plan.modules {
        let extension = plan.str(row.extension);
        if !arrivals.contains_key(extension) {
            let sub: Value = serde_json::from_str(plan.str(row.substitute)).unwrap_or_default();
            if sub["arrival"].is_object() {
                arrivals.insert(extension, sub["arrival"].clone());
            }
        }
    }
    if arrivals.is_empty() {
        return;
    }
    for test in &file.tests {
        let mut initial = BTreeMap::new();
        for step in file.launch.iter().chain(
            test.steps
                .iter()
                .take_while(|s| contract_syntax::is_launch(s)),
        ) {
            if let Step::PreferExtension { key, value, span } = step {
                match value
                    .as_deref()
                    .and_then(|v| serde_json::from_str::<Value>(v).ok())
                {
                    Some(v) => {
                        initial.insert(key.as_str(), (v, *span));
                    }
                    None => {
                        initial.remove(key.as_str());
                    }
                }
            }
        }
        for (extension, arrival) in &arrivals {
            let Some(field) = arrival["field"].as_str() else {
                continue;
            };
            let Some((v, span)) = initial.get(format!("{extension}.{field}").as_str()) else {
                continue;
            };
            if let Some(why) =
                exact_runner::runner::check_arrival_field(extension, field, arrival, v, |k| {
                    initial.get(k).map(|(v, _)| v)
                })
            {
                errors.push(crate::modules::error(
                    "test-extension-substitute",
                    format!("test `{}`: {why}", test.name),
                    *span,
                ));
            }
        }
    }
}

fn test_substitute(
    plan: &exact_plan::Plan,
    root: &Path,
    key: &str,
    value: Option<&str>,
    hosts: &[String],
    declarations: &mut std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<(), String> {
    let (extension, name) = key.split_once('.').unwrap();
    let value = value
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut package_words = None;
    for row in plan
        .modules
        .iter()
        .filter(|row| plan.str(row.extension) == extension)
    {
        let sub: serde_json::Value =
            serde_json::from_str(plan.str(row.substitute)).unwrap_or_default();
        let (word, owner) = name
            .split_once('#')
            .map_or((name, None), |(w, o)| (w, Some(o)));
        if word == plan.str(row.declared) && owner == sub["owner"].as_str() {
            let mut failures = std::collections::BTreeMap::<String, Vec<&str>>::new();
            for target in contract_syntax::parser::TEST_HOSTS {
                if !hosts.is_empty() && !hosts.iter().any(|h| h == target) {
                    continue;
                }
                if let Err(why) = exact_runner::runner::check_source_substitute(
                    plan,
                    row,
                    key,
                    &sub,
                    value.as_ref(),
                    target,
                    |_| None,
                ) {
                    failures.entry(why).or_default().push(target);
                }
            }
            if !failures.is_empty() {
                return Err(failures
                    .into_iter()
                    .map(|(why, targets)| format!("{why} (on {})", targets.join(", ")))
                    .collect::<Vec<_>>()
                    .join("; "));
            }
            return Ok(());
        }
        package_words.get_or_insert_with(|| (plan.str(row.package), sub["agent"].clone()));
    }
    let package_words = package_words.or_else(|| {
        let view = plan
            .views
            .iter()
            .find(|v| plan.str(v.extension) == extension)?;
        Some((
            plan.str(view.package),
            serde_json::from_str(plan.str(view.agent)).unwrap_or_default(),
        ))
    });
    let (package, words) =
        package_words.ok_or_else(|| format!("unknown package `{extension}` in this app"))?;
    let ty = words[name]
        .as_str()
        .ok_or_else(|| format!("unknown substitute `{name}` in package `{package}`"))?;
    let Some(value) = value else { return Ok(()) };
    let shapes = match declarations.entry(package.to_owned()) {
        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(crate::declaration::agent_shapes(root, package)?)
        }
    };
    if crate::declaration::agent_value_fits(ty, &value, shapes)? {
        Ok(())
    } else {
        Err(format!("`{key}` expects {ty}"))
    }
}

/// Resolve stored fixtures to the app's scopes and check state comparisons against its types.
pub fn resolve_test_states_path(
    path: &Path,
    tests: &mut [TestDecl],
) -> Result<(), Vec<CompileError>> {
    use contract_syntax::{StateComparison as C, StateOperand};
    use contract_types::{Ty, TypeError};
    fn relations<'a>(steps: &'a [Step], out: &mut Vec<&'a Step>) {
        for step in steps {
            match step {
                Step::ExpectState {
                    value: StateOperand::State(_),
                    ..
                } => out.push(step),
                Step::Perf { steps, .. } => relations(steps, out),
                _ => {}
            }
        }
    }
    let mut steps = Vec::new();
    let mut stored = Vec::new();
    for step in tests.iter_mut().flat_map(|test| &mut test.steps) {
        match step {
            Step::Stored { name, window, .. } => stored.push((name, window)),
            step => relations(std::slice::from_ref(step), &mut steps),
        }
    }
    if steps.is_empty() && (stored.is_empty() || !path.is_file()) {
        return Ok(());
    }
    let (file, expanded) = property_program(path)?;
    for (name, window) in stored {
        *window = expanded
            .root
            .persistence(name)
            .is_some_and(|state| state.window);
    }
    if steps.is_empty() {
        return Ok(());
    }
    let checked = contract_types::check_all(
        &file,
        false,
        contract_lower::tags::style,
        crate::strings_tables(path.parent().unwrap_or_else(|| Path::new("."))).map_err(
            |message| {
                vec![CompileError::from(TypeError {
                    id: "type-test-state",
                    message,
                    span: contract_syntax::Span::point(1, 1),
                })]
            },
        )?,
    )
    .map_err(|errors| {
        errors
            .into_iter()
            .map(CompileError::from)
            .collect::<Vec<_>>()
    })?;
    let root = &checked.expanded.root;
    let scope = checked
        .types
        .component_scope(root, &checked.types.components[0]);
    let path_type = |path: &str, span| -> Result<Ty, TypeError> {
        let bad = || TypeError {
            id: "type-test-state",
            message: format!("`state {path}` is not a state path in the app"),
            span,
        };
        let mut parts = path.split('.');
        let name = parts.next().unwrap();
        if !root.states.iter().any(|s| s.name == name)
            && !root.derives.iter().any(|d| d.name == name)
            && !root.resources.iter().any(|r| r.name == name)
            && !root.mutations.iter().any(|m| m.name == name)
        {
            return Err(bad());
        }
        let mut ty = scope.lookup(name).ok_or_else(bad)?.1.clone();
        for part in parts {
            while let Ty::Option(inner) = ty {
                ty = *inner;
            }
            ty = match &ty {
                Ty::List(inner) if part.parse::<u32>().is_ok() => *inner.clone(),
                Ty::List(_) if part == "length" => Ty::Number,
                Ty::Record(name) => checked
                    .types
                    .shapes
                    .field(name, part)
                    .ok_or_else(bad)?
                    .1
                    .clone(),
                _ => return Err(bad()),
            };
        }
        Ok(ty)
    };
    let mut errors = Vec::new();
    for step in steps {
        let Step::ExpectState {
            name,
            comparison,
            value: StateOperand::State(other),
            span,
        } = step
        else {
            unreachable!()
        };
        let check = || {
            let left = path_type(name, *span)?;
            let right = path_type(other, *span)?;
            let valid = match comparison {
                C::Equal | C::NotEqual => left.unify(&right).is_some(),
                C::AtLeast | C::AtMost => left == Ty::Number && right == Ty::Number,
                C::In => matches!(&right, Ty::List(inner) if left.unify(inner).is_some()),
            };
            if valid {
                return Ok(());
            }
            let operator = match comparison {
                C::Equal => "==",
                C::NotEqual => "!=",
                C::AtLeast => ">=",
                C::AtMost => "<=",
                C::In => "in",
            };
            Err(TypeError {
                id: "type-test-state",
                message: format!(
                    "cannot compare `state {name}` ({left}) {operator} `state {other}` ({right})"
                ),
                span: *span,
            })
        };
        if let Err(error) = check() {
            errors.push(error.into());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check observed suggestions against every write and caller before printing them.
pub fn check_suggestions_path(
    path: &Path,
    properties: &[PropertyDecl],
) -> Result<Vec<contract_analyze::SuggestionCheck>, Vec<CompileError>> {
    let (file, expanded) = property_program(path)?;
    let mut resolved = properties.to_vec();
    let errors = contract_analyze::resolve_actions(&expanded, &mut resolved);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(CompileError::from).collect());
    }
    Ok(contract_analyze::check_suggestions(
        &expanded, &file, &resolved,
    ))
}

/// Load the app and expand its components for property checks.
pub fn property_program(
    path: &Path,
) -> Result<(contract_syntax::File, contract_syntax::Expanded), Vec<CompileError>> {
    // A relative path makes the `use` containment check compare a canonical
    // sibling against a relative root and refuse it as leaving the app directory
    // (`docs/agent-pitfalls.md`); the device runner passes the test file as typed.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = canonical.as_path();
    let src = crate::read_source(path)?;
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let (file, sources) = crate::sources::load(path, &src, root)?;
    let expanded = contract_syntax::expand(&file).map_err(|e| sources.resolve(e.into()))?;
    Ok((file, expanded))
}

/// The `test` blocks of a file (LLP 1017 P7) — normally `app.test.contract`
/// beside the app, holding nothing else. Parsed, never compiled: a test is a
/// script for the agent driver (`scripts/agent.mjs --test`), and its steps
/// are the eight operations plus `expect` lines that read their replies.
/// The file's top-level launch lines lead each test's steps, unless the test
/// names the same fact itself (habits F7, calendar F13).
pub fn tests(src: &str) -> Result<Vec<TestDecl>, CompileError> {
    let file = contract_syntax::parse(src)?;
    Ok(tests_with_defaults(file.tests, &file.launch))
}

/// Apply a file's launch defaults after checking the steps as written.
pub fn tests_with_defaults(tests: Vec<TestDecl>, launch: &[Step]) -> Vec<TestDecl> {
    tests
        .into_iter()
        .map(|mut test| {
            // By fact, among the test's own leading launch lines: a `fail
            // fetch` line by its prefix (LLP 1103 D3); a later one is a step.
            let leading = test
                .steps
                .iter()
                .take_while(|s| contract_syntax::is_launch(s));
            let own = |l: &Step| leading.clone().any(|s| contract_syntax::same_launch(s, l));
            let inherited = launch
                .iter()
                .filter(|l| !own(l))
                .cloned()
                .collect::<Vec<_>>();
            test.steps = inherited.into_iter().chain(test.steps).collect();
            test
        })
        .collect()
}
