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
            // A surface nothing draws (the build names them) is neither: its canvas is only the box Contract paints.
            if commands::unregistered_surfaces().iter().any(|s| s == name) {
                return Ok(());
            }
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
                            &self.site_evaluation(
                                scope,
                                i,
                                format!("surface argument {}", k + 1),
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
                        &self.site_evaluation(
                            scope,
                            i,
                            format!("surface argument {}", k + 1),
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

    /// A computed bitmap size or getContext setting (LLP 1056 D6): written by
    /// pieces.js `cb`, which redraws the canvas at a new size generation.
    pub(super) fn canvas_setting(&mut self, b: &exact_plan::BindingsRow, e: &str, f: &str) -> bool {
        let Some(name) = crate::style::canvas_setting(b) else {
            return false;
        };
        let cb = self.uses.rt("cb");
        let _ = write!(self.out, "{cb}({e},\"{name}\",{f});");
        true
    }
}

#[cfg(test)]
mod tests {
    /// A computed bitmap size or setting is bound (LLP 1056 D6), a literal one written as the element's attribute.
    #[test]
    fn a_computed_bitmap_size_is_bound() {
        let plan = contract::compile(
            "component App\n  state wide = 640\n  state p3 = false\n  view\n    canvas surface=pad() bitmap-width=wide bitmap-height=480 color-space=(p3 ? \"display-p3\" : \"srgb\")\n",
        )
        .unwrap();
        let js = crate::emit::emit(&plan, false, false).unwrap().js;
        for bound in ["\"data-bitmap-width\"", "\"data-color-space\""] {
            assert!(js.contains(&format!("cb(e0,{bound},")), "{js}");
        }
        assert!(
            js.contains("\"data-bitmap-height\":\"480\"")
                && !js.contains("cb(e0,\"data-bitmap-height\""),
            "{js}"
        );
    }
}
