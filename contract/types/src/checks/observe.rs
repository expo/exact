use super::{err, infer, Expr, Scope, Shapes, Span, Ty, TypeError};

/// Checks `observe(name[, Shape(key=value)[, severity]])`,
/// `observeAttributes(Shape(key=value))`, `observeError(message[, "TypeName"])` and
/// `observeFeedback(id, message[, rating[, min, max]][, Shape(key=value)])`.
pub(super) fn args(
    name: &str,
    args: &[Expr],
    scope: &Scope,
    shapes: &Shapes,
    span: Span,
) -> Result<(), TypeError> {
    if name == "observeFeedback" {
        return feedback(args, scope, shapes, span);
    }
    let (usage, max) = match name {
        "observe" => ("`observe(name)`, `observe(name, Shape(key=value))` or `observe(name, Shape(key=value), \"warn\")`", 3),
        "observeAttributes" => ("`observeAttributes(Shape(key=value))`", 1),
        _ => ("`observeError(message)` or `observeError(message, \"TypeName\")`", 2),
    };
    if args.is_empty() || args.len() > max {
        return err("type-observe", usage.to_string(), span);
    }
    for (i, arg) in args.iter().enumerate() {
        let ty = infer(arg, scope, shapes)?;
        let record = name == "observeAttributes" || (name == "observe" && i == 1);
        // The attributes must be constructed here: a record value carries no
        // field names for the host.
        if record && !arg.constructs_shape() {
            return err(
                "type-observe",
                format!("{usage}: the attributes are a shape constructed here, every field named"),
                arg.span(),
            );
        }
        let ok = if record {
            !matches!(ty, Ty::String | Ty::Number | Ty::Bool)
        } else {
            matches!(ty, Ty::String)
        };
        if !ok {
            return err(
                "type-observe",
                format!("{usage}: argument {} is `{ty}`", i + 1),
                arg.span(),
            );
        }
    }
    Ok(())
}

const FEEDBACK: &str = "`observeFeedback(id, message)`, `observeFeedback(id, message, rating)` or `observeFeedback(id, message, rating, min, max)`, each with an optional `Shape(key=value)` last";

/// The id and message are strings, then a rating with an optional scale, all
/// numbers. A last argument that constructs a shape is the attributes.
fn feedback(args: &[Expr], scope: &Scope, shapes: &Shapes, span: Span) -> Result<(), TypeError> {
    let shape = args.len() > 2 && args.last().is_some_and(Expr::constructs_shape);
    let numbers = args.len().saturating_sub(2 + usize::from(shape));
    if args.len() < 2 || !matches!(numbers, 0 | 1 | 3) {
        return err("type-observe", FEEDBACK.to_string(), span);
    }
    for (i, arg) in args.iter().enumerate() {
        let ty = infer(arg, scope, shapes)?;
        let ok = match i {
            0 | 1 => matches!(ty, Ty::String),
            _ if shape && i == args.len() - 1 => !matches!(ty, Ty::String | Ty::Number | Ty::Bool),
            _ => matches!(ty, Ty::Number),
        };
        if !ok && matches!(ty, Ty::Record(_)) {
            return err(
                "type-observe",
                format!(
                    "{FEEDBACK}: the attributes are a shape constructed here, every field named"
                ),
                arg.span(),
            );
        }
        if !ok {
            return err(
                "type-observe",
                format!("{FEEDBACK}: argument {} is `{ty}`", i + 1),
                arg.span(),
            );
        }
    }
    Ok(())
}
