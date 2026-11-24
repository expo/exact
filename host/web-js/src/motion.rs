//! The kernel's motion seam, as the compiler writes it (rt.js `mo`): which
//! nodes the motion engine follows, and what it is told about each.

use super::{value_js, Em};
use crate::code::Scope;
use crate::style;
use exact_kernel::{PropId, StyleId};
use exact_plan::{BindingKind, EventKind, NodesId, Value};
use std::fmt::Write as _;

/// The rows presence plays (presence-glue.js, and shared.js for a route's
/// entrance and exit, LLP 1013.001): a node whose inline declarations hold
/// one registers with presence (`presence_registration`).
pub(super) const PRESENCE_ROWS: [&str; 4] = [
    "--exact-layout-transition:",
    "--exact-exit-animation:",
    "--exact-view-transition-new:",
    "--exact-view-transition-old:",
];

impl Em<'_> {
    /// A fitted text (LLP 1116.001 D1.4) and a symbol effect (LLP 1077 D12) among a
    /// node's inline declarations: the runtime piece that scales or plays them after
    /// each tree update.
    pub(super) fn after_updates_in(&mut self, declarations: &str) {
        for (property, piece) in [
            ("--exact-fit:", "fitTexts"),
            ("--exact-symbol-effect:", "symbolEffects"),
        ] {
            if declarations.contains(property) {
                let piece = self.uses.rt(piece);
                let _ = write!(self.out, "{piece}();");
            }
        }
    }

    /// Resolve SVG rotation eligibility with the current animation name and axis.
    pub(super) fn svg_animation(
        &mut self,
        i: u32,
        id: StyleId,
        e: &str,
        f: &str,
        scope: &Scope,
    ) -> Result<Option<String>, String> {
        let plan = self.plan;
        let row = plan.node(NodesId(i));
        if !exact_kernel::NodeType::from_wire(row.node_type)
            .is_some_and(exact_kernel::NodeType::is_svg_element)
            || !matches!(id, StyleId::Animation | StyleId::ExitAnimation)
        {
            return Ok(None);
        }
        let binding = |id: StyleId| {
            row.bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == BindingKind::Style && b.id == id as u16)
        };
        let mut spatial = Vec::new();
        let mut inherited = Vec::new();
        for rule in &plan.keyframes {
            let frames =
                exact_motion::Keyframes::parse(plan.str(rule.css)).map_err(|e| e.to_string())?;
            let name = plan.str(rule.name);
            if frames.spatial_rotation() {
                spatial.push(name);
            }
            if frames
                .properties()
                .contains(&exact_motion::Property::Rotate)
                && frames
                    .track(
                        exact_motion::Property::Rotate,
                        exact_motion::Value::four(0., 1., 0., 0.),
                        false,
                        &exact_motion::Easing::Linear,
                    )
                    .iter()
                    .any(|(_, _, v)| v.y != 0. || v.z != 0.)
            {
                inherited.push(name);
            }
        }
        let rotate = match binding(StyleId::Rotate) {
            Some(b) => self.bound(i, e, &b, scope)?,
            None => "()=>0".into(),
        };
        let say = self.uses.rt("say");
        let spatial = serde_json::to_string(&spatial).unwrap();
        let inherited = serde_json::to_string(&inherited).unwrap();
        Ok(Some(format!(
            r#"()=>{{const v=({f})(),r=String(({rotate})()).trim().split(/\s+/),axis=r.length===4?(Number(r[0])!==0||Number(r[1])!==0):r.some(w=>/^[xy]$/i.test(w)),style=document.createElement('div').style;style.animation=v??'';const names=style.animationName.split(',').map(n=>n.trim().replace(/^['"]|['"]$/g,''));if(names.some(n=>{spatial}.includes(n)||axis&&{inherited}.includes(n))){{{say}('animation refused: spatial rotation on an SVG element; animate a box around the svg');return 'none'}}return v}}"#
        )))
    }

    /// Whether node `i` binds a route's entrance or exit, which loads
    /// shared.js as a written one does (LLP 1013.001 D1).
    pub(super) fn binds_route_part(&self, i: u32) -> bool {
        let parts = [
            StyleId::ViewTransitionNew as u16,
            StyleId::ViewTransitionOld as u16,
        ];
        (self.plan.node(NodesId(i)).bindings.iter())
            .map(|b| self.plan.binding(b))
            .any(|b| b.kind == BindingKind::Style && parts.contains(&b.id))
    }

    /// The runtime function that registers node `i` with presence, if any. A bound
    /// exit takes `px`, which keeps its value from the start of each commit and
    /// loads presence even when the first value is `none`; another node takes `pr`
    /// when `inline` (the declarations `rows::presence_decls` moved inline)
    /// holds one of `PRESENCE_ROWS`. The other inline declarations (a press, a
    /// fit, a timeline) belong to other pieces and load no presence.
    pub(super) fn presence_registration(&self, i: u32, inline: &str) -> Option<&'static str> {
        let declares = inline
            .split_inclusive(';')
            .any(|decl| PRESENCE_ROWS.iter().any(|row| decl.starts_with(row)));
        let bound_exit = self
            .plan
            .node(NodesId(i))
            .bindings
            .iter()
            .map(|b| self.plan.binding(b))
            .any(|b| {
                b.kind == BindingKind::Style
                    && b.id == StyleId::ExitAnimation as u16
                    && style::literal(self.plan, self.plan.code(b.expr)).is_none()
                    && crate::presence::exit_animations(self.plan, self.plan.code(b.expr), false)
                        .held
                        .values()
                        .any(Option::is_some)
            });
        if bound_exit {
            Some("px")
        } else {
            (declares || self.binds_route_part(i)).then_some("pr")
        }
    }

    /// Hands the runtime the probes through which each commit reads bound exits
    /// and layout transitions (presence-bindings.js), before the first commit
    /// builds the view: only a plan that binds one links them.
    pub(super) fn presence_bindings(&mut self, body: &mut String) {
        if ["px", "lt"]
            .iter()
            .any(|name| self.uses.names.contains(*name))
        {
            let install = self.uses.rt("usePresenceBindings");
            let probes = self.uses.rt("presenceUse");
            let _ = write!(body, "{install}({probes}());");
        }
    }

    /// Whether the motion engine follows node `i`: one a swipe holds, a
    /// swipe's indicator, one whose `transition` can be a spring (css.rs
    /// leaves springs to the engine), a transform drag's target, or a drag
    /// timeline's source.
    pub(crate) fn is_motion_node(&self, i: u32) -> bool {
        let plan = self.plan;
        let row = &plan.node(NodesId(i));
        let binding = |id: u16, kind: BindingKind| {
            row.bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == kind && b.id == id)
        };
        let swiped = row.handlers.iter().any(|h| {
            matches!(
                plan.handler(h).event,
                EventKind::Swiperight | EventKind::Swipeleft
            )
        });
        let indicator =
            binding(PropId::SwipeIndicator as u16, BindingKind::Prop).is_some_and(|b| {
                !matches!(
                    style::literal(plan, plan.code(b.expr)),
                    Some(Value::Bool(false))
                )
            });
        let spring = binding(StyleId::Transition as u16, BindingKind::Style).is_some_and(|b| {
            match style::literal(plan, plan.code(b.expr)) {
                Some(v) => v.as_str().is_some_and(|t| t.contains("spring")),
                // Settled derives, state and resource answers are opaque here;
                // any computed transition can select a spring (LLP 1047 D2).
                None => true,
            }
        });
        let target = self.transforms.contains(&i);
        // A drag timeline's source: the motion piece resolves the plan's
        // timeline names (motion-glue.js `followTimelines`, LLP 1057.003 D5).
        let source = binding(StyleId::DragTimeline as u16, BindingKind::Style).is_some();
        swiped || indicator || spring || target || source
    }

    /// A node the motion engine follows (rt.js `mo`, the kernel's motion
    /// seam): one a swipe holds, a swipe's indicator, or one whose
    /// `transition` can be a spring (css.rs leaves springs to the engine).
    pub(crate) fn motion_node(&mut self, i: u32, e: &str, scope: &Scope) -> Result<(), String> {
        if !self.is_motion_node(i) {
            return Ok(());
        }
        let plan = self.plan;
        let row = &plan.node(NodesId(i));
        let binding = |id: u16, kind: BindingKind| {
            row.bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == kind && b.id == id)
        };
        self.motion = true;
        let mut values = Vec::new();
        for (id, initial) in [
            (StyleId::Translate, "0"),
            (StyleId::Scale, "1"),
            (StyleId::Rotate, "0"),
            (StyleId::Opacity, "1"),
            (StyleId::Transition, "\"\""),
            (StyleId::TranslateVelocity, "null"),
        ] {
            values.push(match binding(id as u16, BindingKind::Style) {
                None => initial.to_string(),
                Some(b) => match style::literal(plan, plan.code(b.expr)) {
                    Some(v) => value_js(&v),
                    None => format!("(({})()??{initial})", self.bound(i, e, &b, scope)?),
                },
            });
        }
        let mo = self.uses.rt("mo");
        let _ = write!(self.out, "{mo}({e},()=>[{}]);", values.join(","));
        Ok(())
    }

    /// Possible owners need their authored motion values registered before a
    /// drag starts. The live binding is resolved from the DOM after each commit;
    /// only a literal mismatch can rule an ancestor out here.
    pub(super) fn drag_targets(&self, prop: PropId) -> std::collections::BTreeSet<u32> {
        let plan = self.plan;
        let binding = |i: u32, prop: PropId| {
            plan.node(NodesId(i))
                .bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == BindingKind::Prop && b.id == prop as u16)
        };
        let mut out = std::collections::BTreeSet::new();
        for i in 0..plan.nodes.len() as u32 {
            let Some(b) = binding(i, prop) else {
                continue;
            };
            let name = style::literal(plan, plan.code(b.expr));
            if name
                .as_ref()
                .is_some_and(|v| v.as_str().is_none_or(str::is_empty))
            {
                continue;
            }
            let mut at = self.parent_of(i);
            while let Some(p) = at {
                if let Some(id) = binding(p, PropId::Id) {
                    let id = style::literal(plan, plan.code(id.expr));
                    let possible = match (&name, &id) {
                        (_, Some(id)) if id.as_str().is_none_or(str::is_empty) => false,
                        (Some(name), Some(id)) => name.as_str() == id.as_str(),
                        _ => true,
                    };
                    if possible {
                        out.insert(p);
                    }
                }
                at = self.parent_of(p);
            }
        }
        out
    }

    pub(super) fn reorder_handle(&self, i: u32) -> bool {
        let plan = self.plan;
        plan.node(NodesId(i))
            .bindings
            .iter()
            .map(|b| plan.binding(b))
            .any(|b| b.kind == BindingKind::Prop && b.id == PropId::ReorderFor as u16)
    }

    /// A height drag's owner (rt.js `mh`): its numeric height and its
    /// `transition`, which a released drag springs by.
    pub(crate) fn height_owner(&mut self, i: u32, e: &str, scope: &Scope) -> Result<(), String> {
        if !self.heights.contains(&i) {
            return Ok(());
        }
        self.motion = true;
        let plan = self.plan;
        let mut values = Vec::new();
        for (id, initial) in [(StyleId::Height, "null"), (StyleId::Transition, "\"\"")] {
            let b = plan
                .node(NodesId(i))
                .bindings
                .iter()
                .map(|b| plan.binding(b))
                .find(|b| b.kind == BindingKind::Style && b.id == id as u16);
            values.push(match b {
                None => initial.to_string(),
                Some(b) => match style::literal(plan, plan.code(b.expr)) {
                    Some(v) => value_js(&v),
                    None => format!("(({})()??{initial})", self.bound(i, e, &b, scope)?),
                },
            });
        }
        let mh = self.uses.rt("mh");
        let _ = write!(self.out, "{mh}({e},()=>[{}]);", values.join(","));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn computed_transitions_register_motion_without_guessing_their_values() {
        for (declarations, transition, motion) in [
            ("", "\"translate 400ms ease\"", false),
            ("", "\"none\"", false),
            ("", "\"translate -exact-spring(140, 10, 1)\"", true),
            ("  derive spring = \"translate -exact-spring(140, 10, 1)\"\n", "spring", true),
            ("  state spring = \"none\"\n", "spring", true),
            ("  derive spring = \"translate -exact-spring(140, 10, 1)\"\n  derive curve = spring\n", "curve", true),
        ] {
            let plan = contract::compile(&format!(
                "component App\n{declarations}  view\n    view translate=\"100px 0px\" transition={transition}\n"
            )).unwrap();
            let emitted = crate::emit::emit(&plan, false, false).unwrap();
            assert_eq!(emitted.motion, motion, "{declarations}{transition}");
        }
    }
}
