//! Read-only logical row text: no DOM construction, effects or host commands.
use super::{sites::Site, style, Em};
use crate::code::{self, Frame, Scope};
use exact_kernel::{NodeType, PropId, StyleId};
use exact_plan::{BindingKind, EventKind, NodesId, RegionKind};

impl Em<'_> {
    pub(super) fn logical_children(
        &mut self,
        sites: &[Site],
        scope: &Scope,
    ) -> Result<String, String> {
        let mut children = Vec::new();
        for site in sites {
            children.push(match *site {
                Site::Node(i) => {
                    let node = self.plan.node(NodesId(i));
                    // A nested collection owns its selection and copy (LLP 1070 §11).
                    if node.bindings.iter().map(|b| self.plan.binding(b)).any(|b| {
                        b.kind == BindingKind::Prop
                            && b.id == PropId::Virtualized as u16
                            && style::literal(self.plan, self.plan.code(b.expr))
                                == Some(exact_plan::Value::Bool(true))
                    }) {
                        continue;
                    }
                    let kind = if node.node_type == NodeType::Text as u8 {
                        1
                    } else if node.node_type == NodeType::Image as u8 {
                        2
                    } else {
                        0
                    };
                    let mut props = Vec::new();
                    if [NodeType::Pressable as u8, NodeType::Control as u8]
                        .contains(&node.node_type)
                        || node
                            .handlers
                            .iter()
                            .any(|h| self.plan.handler(h).event == EventKind::Press)
                    {
                        props.push("control:true".into());
                    }
                    for b in node.bindings.iter().map(|b| self.plan.binding(b)) {
                        let name = match b.kind {
                            BindingKind::Style if b.id == StyleId::Display as u16 => "display",
                            BindingKind::Style if b.id == StyleId::UserSelect as u16 => "select",
                            BindingKind::Style if b.id == StyleId::Visibility as u16 => {
                                "visibility"
                            }
                            BindingKind::Prop if b.id == PropId::Inert as u16 => "inert",
                            BindingKind::Prop if b.id == PropId::AccessibilityRole as u16 => "role",
                            BindingKind::Prop if b.id == PropId::Text as u16 && kind == 1 => "text",
                            BindingKind::Prop
                                if b.id == PropId::AccessibilityLabel as u16 && kind == 2 =>
                            {
                                "text"
                            }
                            _ => continue,
                        };
                        let value = code::called_expression(self.bound(i, "{}", &b, scope)?);
                        props.push(format!("{name}:{value}"));
                    }
                    let children = self.logical_children(self.sites.of_node(i), scope)?;
                    format!("[{kind},{{{}}},{children}]", props.join(","))
                }
                Site::Region(r) => {
                    let row = &self.plan.regions[r as usize];
                    let subject = code::expression(
                        self.plan,
                        self.plan.code(row.subject),
                        scope,
                        &mut self.uses,
                    )?;
                    let depth = scope.frames.len();
                    let arms: Vec<_> = row.arms.iter().map(|a| a.0).collect();
                    let mut bodies = Vec::new();
                    for (k, arm) in arms.iter().enumerate() {
                        let mut inner = scope.clone();
                        inner.frames.push(match row.kind {
                            RegionKind::Each => Frame {
                                item: Some(format!("i{depth}")),
                                index: Some(format!("x{depth}")),
                                bound: None,
                            },
                            RegionKind::Match if k == 0 => Frame {
                                bound: Some(format!("b{depth}")),
                                ..Frame::default()
                            },
                            _ => Frame::default(),
                        });
                        let decl = self.owned(*arm, &format!("$t{arm}"), &mut inner, None)?;
                        let children = self.logical_children(self.sites.of_arm(*arm), &inner)?;
                        bodies.push(format!("{decl}return {children};"));
                    }
                    let value = match row.kind {
                        RegionKind::Each => format!(
                            "({subject}).flatMap(($i,$x)=>{{const i{depth}=()=>$i,x{depth}=()=>$x;{}}})",
                            bodies[0]
                        ),
                        RegionKind::When => format!(
                            "(()=>{{if({subject}){{{}}}{} }})()",
                            bodies[0],
                            bodies.get(1).cloned().unwrap_or("return [];".into())
                        ),
                        RegionKind::Match => format!(
                            "(($b)=>{{if($b!==null){{const b{depth}=()=>$b;{}}}{} }})({subject})",
                            bodies[0],
                            bodies.get(1).cloned().unwrap_or("return [];".into())
                        ),
                    };
                    format!("...{value}")
                }
            });
        }
        Ok(format!("[{}]", children.join(",")))
    }
}
