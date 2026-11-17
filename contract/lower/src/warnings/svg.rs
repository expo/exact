use super::literal;
use crate::LowerError;
use contract_checking::View;
use contract_syntax::{Expr, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn use_foreign_object(view: &View<'_>, out: &mut Vec<LowerError>) {
    let mut refs = References::default();
    refs.collect(view);
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
    fn collect(&mut self, view: &View<'a>) {
        let mut contains = Vec::new();
        for (node, (svg, conditional), entering) in
            view.events((false, false), |node, &(svg, conditional)| {
                Some(match node.element() {
                    Some((tag, _, _)) => {
                        ((svg || tag == "svg") && tag != "foreignObject", conditional)
                    }
                    None => (svg, true),
                })
            })
        {
            if entering {
                contains.push(false);
                continue;
            }
            let child_foreign = contains.pop().unwrap();
            let mut foreign = child_foreign;
            if let Some((tag, _, attrs)) = node.element() {
                let svg = svg || tag == "svg";
                foreign |= svg && tag == "foreignObject";
                if let Some(id) = attrs.iter().rev().find(|a| a.name == "id") {
                    if let Expr::Str(id, _) = &id.value {
                        let candidate = (svg && !conditional).then_some((node.span(), foreign));
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
                        self.uses.push((id, node.span()));
                    }
                }
            }
            if let Some(parent) = contains.last_mut() {
                *parent |= foreign;
            }
        }
    }
}
