//! Provider checks along component paths, bounded by emitted site depth.

use super::{err, infer, Ref, Scope, Sink, Ty, TypeError, Types};
use crate::checks::can_unify;
use contract_syntax::{Binding, Component, File, Node, Span};
use std::collections::BTreeSet;

#[derive(Clone)]
struct Provider<'a> {
    binding: &'a Binding,
    ty: Result<Ty, TypeError>,
    computed_action: bool,
}

impl<'a> Provider<'a> {
    fn new(binding: &'a Binding, scope: &Scope, types: &Types, file: &File) -> Self {
        Self {
            binding,
            ty: infer(&binding.expr, scope, &types.shapes),
            computed_action: super::uses::computed_action(&binding.expr, scope, types, file),
        }
    }
}

/// A slot's fill, checked where `children` stands with its caller's scope
/// and providers (LLP 1035.005.000 D9: the slot's own section does not
/// reach it).
#[derive(Clone)]
struct Fill<'a> {
    nodes: &'a [Node],
    scope: Scope,
    provides: Vec<Provider<'a>>,
    outer: Option<Box<Fill<'a>>>,
}

/// Check each inject against the nearest provider on its component path.
/// A data inject that would repeat its provider's error can be replaced on recovery.
pub(super) fn check_injects(
    root: &Component,
    scope: &Scope,
    types: &Types,
    file: &File,
    sink: &mut Sink,
    refused: &mut BTreeSet<Span>,
) -> Vec<TypeError> {
    let mut provided = Vec::new();
    for b in &root.provides {
        let provider = Provider::new(b, scope, types, file);
        if let Err(error) = &provider.ty {
            sink.push(error.clone());
        }
        provided.push(provider);
    }
    let mut errors = InjectErrors {
        sink,
        refused,
        invalid: Vec::new(),
    };
    let result = check_inject_nodes(
        &root.view,
        scope,
        types,
        file,
        &mut provided,
        None,
        0,
        0,
        &mut errors,
    );
    errors.sink.keep_unit(result);
    errors.invalid
}

struct InjectErrors<'a> {
    sink: &'a mut Sink,
    refused: &'a mut BTreeSet<Span>,
    invalid: Vec<TypeError>,
}

#[allow(clippy::too_many_arguments)]
fn check_inject_nodes<'a>(
    nodes: &'a [Node],
    scope: &Scope,
    types: &Types,
    file: &'a File,
    provides: &mut Vec<Provider<'a>>,
    fill: Option<&Fill<'a>>,
    depth: u32,
    site_depth: usize,
    errors: &mut InjectErrors<'_>,
) -> Result<(), TypeError> {
    for node in nodes {
        let is_site = !matches!(node, Node::Use { .. } | Node::Children { .. });
        if is_site && site_depth >= 256 {
            return err(
                "syntax-inline-depth",
                "inlined views nest more than 256 sites deep",
                node.span(),
            );
        }
        let next_site_depth = site_depth + usize::from(is_site);
        match node {
            Node::Element { children, .. } => {
                check_inject_nodes(
                    children,
                    scope,
                    types,
                    file,
                    provides,
                    fill,
                    depth,
                    next_site_depth,
                    errors,
                )?;
            }
            Node::Use {
                name,
                children,
                span,
                ..
            } => {
                // A use of an unknown component was refused and never expanded.
                let Some(target) = file
                    .components
                    .iter()
                    .position(|component| &component.name == name)
                else {
                    continue;
                };
                let target_c = &file.components[target];
                let target_t = &types.components[target];
                for (j, inject) in target_c.injects.iter().enumerate() {
                    let Some(provider) = provides
                        .iter()
                        .rev()
                        .find(|p| p.binding.name == inject.name)
                    else {
                        continue;
                    };
                    let Provider {
                        binding,
                        ty: got,
                        computed_action,
                    } = provider;
                    let want = &target_t.props[target_c.props.len() + j];
                    let got = match got {
                        Ok(ty) => ty,
                        Err(error) => {
                            // Actions are checked where invoked so their diagnostics can
                            // name the handler and the binding that supplied it.
                            if !matches!(want, Ty::Action(_)) && errors.refused.insert(binding.span)
                            {
                                errors.invalid.push(error.clone());
                            }
                            continue;
                        }
                    };
                    if matches!(want, Ty::Action(_)) {
                        errors.sink.keep_unit(super::uses::check_action_binding(
                            &binding.name,
                            &binding.expr,
                            binding.span,
                            *computed_action,
                            true,
                        ));
                    }
                    if !can_unify(want, got) {
                        errors.sink.push(TypeError {
                            id: "type-provide",
                            message: format!(
                                "the provided `{}` is `{got}`, but `{name}` injects `{want}`",
                                inject.name
                            ),
                            span: binding.span,
                        });
                    }
                }
                if depth >= 32 {
                    return err(
                        "syntax-inline-depth",
                        format!("component `{name}` nests too deeply (a cycle?)"),
                        *span,
                    );
                }
                let target_scope = types.component_scope(target_c, target_t);
                let child_fill = target_c.slot.then(|| Fill {
                    nodes: children,
                    scope: scope.clone(),
                    provides: provides.clone(),
                    outer: fill.cloned().map(Box::new),
                });
                let outer = provides.len();
                for b in &target_c.provides {
                    provides.push(Provider::new(b, &target_scope, types, file));
                }
                let checked = check_inject_nodes(
                    &target_c.view,
                    &target_scope,
                    types,
                    file,
                    provides,
                    child_fill.as_ref(),
                    depth + 1,
                    next_site_depth,
                    errors,
                );
                provides.truncate(outer);
                checked?;
            }
            Node::Children { .. } => {
                if let Some(fill) = fill {
                    check_inject_nodes(
                        fill.nodes,
                        &fill.scope,
                        types,
                        file,
                        &mut fill.provides.clone(),
                        fill.outer.as_deref(),
                        depth,
                        next_site_depth,
                        errors,
                    )?;
                }
            }
            Node::When {
                then, otherwise, ..
            } => {
                check_inject_nodes(
                    then,
                    scope,
                    types,
                    file,
                    provides,
                    fill,
                    depth,
                    next_site_depth,
                    errors,
                )?;
                check_inject_nodes(
                    otherwise,
                    scope,
                    types,
                    file,
                    provides,
                    fill,
                    depth,
                    next_site_depth,
                    errors,
                )?;
            }
            Node::Each {
                var,
                index,
                list,
                body,
                ..
            } => {
                let ty = infer(list, scope, &types.shapes)?;
                let Ty::List(item) = ty else {
                    return err(
                        "type-each-list",
                        format!("`for` needs a list, given `{ty}`"),
                        list.span(),
                    );
                };
                let mut inner = scope.clone();
                inner.push_each(var, index.as_deref(), *item);
                check_inject_nodes(
                    body,
                    &inner,
                    types,
                    file,
                    provides,
                    fill,
                    depth,
                    next_site_depth,
                    errors,
                )?;
            }
            Node::Match {
                subject,
                some,
                none,
                ..
            } => {
                let ty = infer(subject, scope, &types.shapes)?;
                let Ty::Option(item) = ty else {
                    return err(
                        "type-match-subject",
                        format!("`match` needs an option, given `{ty}`"),
                        subject.span(),
                    );
                };
                let mut inner = scope.clone();
                inner.push_region(Some((some.0.clone(), Ref::Bound(0), *item)));
                check_inject_nodes(
                    &some.1,
                    &inner,
                    types,
                    file,
                    provides,
                    fill,
                    depth,
                    next_site_depth,
                    errors,
                )?;
                let mut none_scope = scope.clone();
                none_scope.push_region(None);
                check_inject_nodes(
                    none,
                    &none_scope,
                    types,
                    file,
                    provides,
                    fill,
                    depth,
                    next_site_depth,
                    errors,
                )?;
            }
        }
    }
    Ok(())
}
