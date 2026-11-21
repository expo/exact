//! Action registrations share compiled bodies and argument checks across component uses.
use super::{code, templates::Templates, type_code, type_code_numeric, Em, Scope};
use std::fmt::Write as _;

impl Em<'_> {
    pub(super) fn declare_actions(&mut self, body: &mut String) -> Result<(), String> {
        let plan = self.plan;
        let capture = !self.site_attrs && !self.dev_reload;
        let mut templates = Templates::default();
        let mut actions = String::new();
        for (i, r) in plan.actions.iter().enumerate() {
            // An action that touches row slots takes the row in force first.
            let rows = code::touches_rows(plan, plan.code(r.body));
            let scope = Scope {
                rows: rows.then(|| "$r".to_string()),
                action: true,
                capture,
                ..Scope::default()
            };
            let mut f = code::function(
                plan,
                plan.code(r.body),
                &code::evaluation(&scope, format!("action {:?}", plan.str(r.name)), false),
                r.params.len as usize,
                &mut self.uses,
            )
            .map_err(|e| format!("action {}: {e}", plan.str(r.name)))?;
            if rows {
                self.row_actions.insert(i);
                f = f.replacen('(', "($r,", 1).replace("($r,)", "($r)");
            }
            // The parameters' types: an argument outside its type is refused,
            // as the runner refuses it (ArgumentType), before the body runs. A
            // hidden parameter (`@capture:…`, a child's captured prop) is the
            // compiler's argument: its numbers need not be finite.
            let types: Vec<String> = r
                .params
                .iter()
                .map(|p| {
                    let param = plan.param(p);
                    let code =
                        type_code_numeric(plan, param.ty, plan.str(param.name).starts_with('@'));
                    scope.operand(serde_json::to_string(&code).unwrap())
                })
                .collect();
            // A string parameter's name, which an argument past MAX_STRING
            // names (the runner's StringTooLong, LLP 1090 D6).
            let names: Vec<String> =
                r.params
                    .iter()
                    .map(|p| match type_code(plan, plan.param(p).ty).as_str() {
                        "s" => scope
                            .operand(serde_json::to_string(plan.str(plan.param(p).name)).unwrap()),
                        _ => "0".into(),
                    })
                    .collect();
            let act = self.uses.rt("act");
            let expr = if types.is_empty() {
                format!("{act}({f})")
            } else {
                let tail = if names.iter().any(|n| n != "0") {
                    format!(",{},[{}]", rows as u8, names.join(","))
                } else if rows {
                    ",1".into()
                } else {
                    String::new()
                };
                format!("{act}({f},[{}]{tail})", types.join(","))
            };
            let expr = if capture {
                templates.expression(expr)
            } else {
                expr
            };
            write!(actions, "const a_{i}={expr};").unwrap();
        }
        let (factories, actions) = templates.render(actions, false, "$action");
        body.push_str(&factories);
        body.push_str(&actions);
        Ok(())
    }
}
