use super::literal;
use crate::LowerError;
use contract_syntax::{Expr, Node, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn use_foreign_object(view: &[Node], out: &mut Vec<LowerError>) {
    let mut refs = References::default();
    refs.collect(view, false, false);
    if refs.dynamic_id {
        return;
    }
    let mut seen = BTreeSet::new();
    for (id, span) in refs.uses {
        if matches!(refs.targets.get(id), Some(Some((_, true)))) && seen.insert(span) {
            out.push(LowerError {
                id: "lower-svg-use-foreign-object",
                message: format!(
                    "the `foreignObject` in `#{id}` draws nothing through SVG `use`. Place it directly in the SVG for the web; for content on every host, position an ordinary box over the SVG"
                ),
                span,
            });
        }
    }
}

#[derive(Default)]
struct References<'a> {
    // Copies of one source definition resolve only when all are unconditional
    // SVG targets and agree on whether they contain a foreignObject. Other ids
    // still collide; a dynamic id can collide with any literal target.
    targets: BTreeMap<&'a str, Option<(Span, bool)>>,
    dynamic_id: bool,
    uses: Vec<(&'a str, Span)>,
}

impl<'a> References<'a> {
    /// Record whether each target contains a foreignObject in one tree walk.
    /// References inside a target are not followed.
    fn collect(&mut self, nodes: &'a [Node], svg: bool, conditional: bool) -> bool {
        let mut contains = false;
        for node in nodes {
            contains |= match node {
                Node::Element {
                    tag,
                    attrs,
                    children,
                    span,
                    ..
                } => {
                    let svg = svg || tag == "svg";
                    let foreign = svg && tag == "foreignObject";
                    let child_foreign = self.collect(children, svg && !foreign, conditional);
                    let contains = foreign || child_foreign;
                    if let Some(id) = attrs.iter().rev().find(|a| a.name == "id") {
                        if let Expr::Str(id, _) = &id.value {
                            let candidate = (svg && !conditional).then_some((*span, contains));
                            self.targets
                                .entry(id)
                                .and_modify(|target| {
                                    if *target != candidate {
                                        *target = None;
                                    }
                                })
                                .or_insert(candidate);
                        } else {
                            self.dynamic_id = true;
                        }
                    }
                    if svg && tag == "use" {
                        if let Some(id) = literal(attrs, "href").and_then(|s| s.strip_prefix('#')) {
                            self.uses.push((id, *span));
                        }
                    }
                    contains
                }
                // Every list is collected, so not `any`.
                node => {
                    let mut yes = false;
                    for list in node.child_lists() {
                        yes |= self.collect(list, svg, true);
                    }
                    yes
                }
            };
        }
        contains
    }
}
