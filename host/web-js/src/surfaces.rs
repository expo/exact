//! Surface arguments, evaluated separately and named if they trap.
use super::{commands, type_code, Em};
use crate::code::{self, Scope};
use std::fmt::Write as _;

impl Em<'_> {
    pub(super) fn surface(&mut self, i: u32, e: &str, scope: &Scope) -> Result<(), String> {
        let plan = self.plan;
        // Its surface's inputs, named or positional (LLP 1009 D2).
        if let Some(sf) = plan.node(exact_plan::NodesId(i)).surface {
            let sf = &plan.surfaces[sf.0 as usize];
            // A surface the app's GPU module draws (the build names them)
            // goes to it; any other is a Canvas 2D surface, drawn by the data module and replayed by the web host's own glue.
            let name = plan.str(sf.name);
            let gpu = commands::gpu_surfaces().iter().any(|s| s == name);
            let named = sf.mode == exact_plan::SurfaceArgsMode::Named;
            if !gpu {
                // Positional values, their declared types where known, and
                // the authored names (LLP 1056 D1: a draw's `args`).
                self.canvas2d.insert(name.to_string());
                let values = sf
                    .args
                    .iter()
                    .enumerate()
                    .map(|(k, a)| {
                        let a = &plan.surface_args[a.0 as usize];
                        code::expression(
                            plan,
                            plan.code(a.expr),
                            &code::evaluation(
                                scope,
                                format!("site {i} surface argument {}", k + 1),
                                true,
                            ),
                            &mut self.uses,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let types: Vec<String> = sf
                    .args
                    .iter()
                    .map(|a| {
                        let a = &plan.surface_args[a.0 as usize];
                        crate::faces::arg_type(plan, plan.code(a.expr)).map_or("0".into(), |t| {
                            serde_json::to_string(&type_code(plan, t)).unwrap()
                        })
                    })
                    .collect();
                let names: Vec<String> = if named {
                    sf.args
                        .iter()
                        .map(|a| {
                            serde_json::to_string(plan.str(plan.surface_args[a.0 as usize].name))
                                .unwrap()
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let c2 = self.uses.rt("c2");
                let _ = write!(
                    self.out,
                    "{c2}({e},{},()=>[{}],[{}],[{}]);",
                    serde_json::to_string(name).unwrap(),
                    values.join(","),
                    types.join(","),
                    names.join(",")
                );
            } else {
                let mut values = Vec::new();
                for (k, a) in sf.args.iter().enumerate() {
                    let a = &plan.surface_args[a.0 as usize];
                    let v = code::expression(
                        plan,
                        plan.code(a.expr),
                        &code::evaluation(
                            scope,
                            format!("site {i} surface argument {}", k + 1),
                            true,
                        ),
                        &mut self.uses,
                    )?;
                    values.push(if named {
                        format!("{}:{v}", serde_json::to_string(plan.str(a.name)).unwrap())
                    } else {
                        v
                    });
                }
                let gs = self.uses.rt("gs");
                let body = if named {
                    format!("({{{}}})", values.join(","))
                } else {
                    format!("[{}]", values.join(","))
                };
                let _ = write!(
                    self.out,
                    "{gs}({e},{},()=>{body});",
                    serde_json::to_string(name).unwrap()
                );
            }
        }
        Ok(())
    }
}
