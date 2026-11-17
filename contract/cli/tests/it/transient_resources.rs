//! `resource … transient` (LLP 1156.005 D4).

/// `transient` after a resource's shape reaches its plan row (LLP 1156.005 D4).
#[test]
fn a_transient_resource_is_marked_in_the_plan() {
    let src = include_str!("../../../corpus/surface.contract");
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with("resource "))
        .expect("a resource");
    let plan = contract::compile(src).unwrap();
    assert!(!plan.resources[0].transient);
    let marked = src.replace(line, &format!("{line} transient"));
    assert!(contract::compile(&marked).unwrap().resources[0].transient);
}

#[test]
fn keep_is_explicit_for_surface_readers_only() {
    let src = include_str!("../../../corpus/surface.contract");
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with("resource "))
        .unwrap();
    let marked = src.replace(line, &format!("{line} keep"));
    assert!(contract::compile(&marked).unwrap().resources[0].keep);
    assert!(contract::compile(&marked.replace("exactSurface", "exactCanvasPerformance")).is_err());
    assert!(contract::compile(&src.replace(line, &format!("{line} transient keep"))).is_err());
    assert!(
        contract::compile(&contract_syntax::fmt::format(&marked).unwrap())
            .unwrap()
            .resources[0]
            .keep
    );
}
