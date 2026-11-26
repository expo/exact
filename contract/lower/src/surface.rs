//! Checked inputs to a canvas surface (LLP 1009 D2a).
use crate::{err, LowerError};
use contract_syntax::Expr;
use contract_types::Scope;

impl crate::Lowerer<'_> {
    pub(crate) fn lower_surface(
        &mut self,
        name: &str,
        args: &[Expr],
        scope: &Scope,
        locals: u16,
    ) -> Result<exact_plan::SurfacesId, LowerError> {
        let mut codes = Vec::with_capacity(args.len());
        for arg in args {
            let (key, value) = match arg {
                Expr::NamedArg(key, value, _) => (key.as_str(), value.as_ref()),
                _ => ("", arg),
            };
            if let Expr::Call(head, args, span) = value {
                if head == "color" {
                    let [Expr::Str(token, _)] = args.as_slice() else {
                        return err(
                            "type-surface-color",
                            "color() takes one literal design colour token name",
                            *span,
                        );
                    };
                    let plan = self.b.plan();
                    let Some(i) = plan
                        .theme
                        .iter()
                        .position(|row| plan.str(row.name) == token && row.kind == 0)
                    else {
                        let why = self.theme.iter().find(|t| &t.name == token).map_or_else(
                            || format!("no design colour token named `{token}`"),
                            |t| {
                                format!(
                                    "`{token}` is a {} token, expected a colour token",
                                    t.kind.name()
                                )
                            },
                        );
                        return err("forma-surface-color", why, *span);
                    };
                    codes.push((
                        key,
                        self.b.constant(&exact_plan::Value::Unit),
                        None,
                        Some(exact_plan::ThemeId(i as u32)),
                    ));
                    continue;
                }
            }
            let (code, ty) = self.typed_code(value, scope, locals)?;
            codes.push((key, code, self.ty_id(&ty).ok(), None));
        }
        Ok(self.b.surface_with_colors(name, codes))
    }
}
