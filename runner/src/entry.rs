//! A snapshot entry's values from the host: typed props replace only the
//! entry's root slot (LLP 1126 D2), and a widget's chosen configuration fills
//! its configuration slot and its refresh's second argument. Every host that
//! boots or refreshes an entry reads them here.
use crate::Carried;
use exact_plan::{Plan, TypeKind, TypesId, Value};

/// The entry's slots the host supplies: its props (or an activity's state)
/// and a configured widget's chosen values. Either left out keeps the value
/// the bake compiled in (the preview, the configuration's defaults).
pub fn slots(
    plan: &Plan,
    props: Option<&[u8]>,
    configuration: Option<&[u8]>,
) -> Result<Carried, String> {
    let props_slot = if plan.activity_slots.is_empty() {
        "snapshot_props"
    } else {
        "snapshot_state"
    };
    let mut slots = Vec::new();
    for (name, bytes, what) in [
        (props_slot, props, "props"),
        ("snapshot_configuration", configuration, "configuration"),
    ] {
        let Some(bytes) = bytes else { continue };
        let row = plan
            .slots
            .iter()
            .find(|s| plan.str(s.name) == name && s.owner.is_none());
        let Some(row) = row else {
            // A widget the person configured before an update that removed
            // its configuration renders with none.
            if what == "configuration" {
                continue;
            }
            return Err("entry has no props slot".into());
        };
        let value = if what == "configuration" {
            chosen(plan, row.ty, Some(bytes))?
        } else {
            let json: serde_json::Value =
                serde_json::from_slice(bytes).map_err(|e| format!("entry {what}: {e}"))?;
            decode(plan, row.ty, &json)
                .ok_or_else(|| format!("entry {what}: not of the declared shape"))?
        };
        slots.push((name.into(), value));
    }
    Ok(Carried {
        slots,
        ..Carried::default()
    })
}

/// A snapshot entry's refresh arguments: the date and, for a configured
/// widget, its placement's chosen values, decoded as [`slots`] decodes them.
/// No configuration is every parameter's default.
pub fn refresh_args(
    plan: &Plan,
    now: f64,
    configuration: Option<&[u8]>,
) -> Result<Vec<Value>, String> {
    let refresh = plan.source(plan.snapshot_refresh.ok_or("not a snapshot entry")?);
    let mut args = vec![Value::Number(now)];
    if let Some(parameter) = refresh.params.iter().nth(1) {
        args.push(chosen(
            plan,
            plan.source_param(parameter).ty,
            configuration,
        )?);
    }
    Ok(args)
}

/// A configuration record of type `ty` from a placement's JSON. LLP 1126:
/// ignore removed parameters and default missing or removed values. The
/// compiler puts each parameter's default first in its enum.
fn chosen(plan: &Plan, ty: TypesId, configuration: Option<&[u8]>) -> Result<Value, String> {
    let json = match configuration {
        Some(bytes) => {
            serde_json::from_slice(bytes).map_err(|e| format!("entry configuration: {e}"))?
        }
        None => serde_json::Value::Null,
    };
    let shape = plan.type_(ty);
    if shape.kind != TypeKind::Record
        || shape.fields.iter().any(|id| {
            let field = plan.type_(plan.field(id).ty);
            field.kind != TypeKind::String || field.fields.len == 0
        })
    {
        return Err("entry configuration has no fixed choices".into());
    }
    Ok(Value::record(
        shape
            .fields
            .iter()
            .map(|id| {
                let field = plan.field(id);
                json.get(plan.str(field.name))
                    .and_then(|v| decode(plan, field.ty, v))
                    .unwrap_or_else(|| {
                        let first = plan
                            .type_(field.ty)
                            .fields
                            .iter()
                            .next()
                            .expect("configuration enum");
                        Value::str(plan.str(plan.field(first).name))
                    })
            })
            .collect(),
    ))
}
fn decode(plan: &Plan, ty: TypesId, json: &serde_json::Value) -> Option<Value> {
    let row = plan.type_(ty);
    let value = match row.kind {
        TypeKind::Number => Value::Number(json.as_f64().filter(|n| n.is_finite())?),
        TypeKind::Bool => Value::Bool(json.as_bool()?),
        TypeKind::String => Value::str(json.as_str()?),
        TypeKind::Unit if json.is_null() => Value::Unit,
        TypeKind::Option => Value::Option(if json.is_null() {
            None
        } else {
            Some(std::rc::Rc::new(decode(plan, row.elem?, json)?))
        }),
        TypeKind::List => Value::list(
            json.as_array()?
                .iter()
                .map(|v| decode(plan, row.elem?, v))
                .collect::<Option<Vec<_>>>()?,
        ),
        TypeKind::Record => {
            let fields = json.as_object()?;
            if fields.len() != row.fields.len as usize {
                return None;
            }
            Value::record(
                row.fields
                    .iter()
                    .map(|id| {
                        let field = plan.field(id);
                        decode(plan, field.ty, fields.get(plan.str(field.name))?)
                    })
                    .collect::<Option<Vec<_>>>()?,
            )
        }
        _ => return None,
    };
    value.conforms(plan, ty).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_plan::builder::PlanBuilder;

    fn entry(configured: bool) -> Plan {
        let mut b = PlanBuilder::new(0, 0);
        let number = b.primitive(TypeKind::Number);
        let mut parameters = vec![number];
        if configured {
            let rows = b.string_enum(&["two".into(), "one".into()]);
            parameters.push(b.record("", &[("rows", rows)]));
        }
        let refresh = b.source("refresh", &parameters, number);
        let mut plan = b.finish().unwrap();
        plan.snapshot_refresh = Some(refresh);
        plan
    }

    #[test]
    fn refresh_args_are_the_date_and_the_chosen_configuration_over_its_defaults() {
        let configured = entry(true);
        for (chosen, rows) in [
            (None, "two"),
            (Some("{}"), "two"),
            (Some(r#"{"rows":"one"}"#), "one"),
            (Some(r#"{"rows":"one","removed":"old"}"#), "one"),
            (Some(r#"{"rows":"seven"}"#), "two"),
            (Some(r#"{"columns":"one"}"#), "two"),
        ] {
            assert_eq!(
                refresh_args(&configured, 7.0, chosen.map(str::as_bytes)).unwrap(),
                [Value::Number(7.0), Value::record(vec![Value::str(rows)])],
                "{chosen:?}"
            );
        }
        assert!(refresh_args(&configured, 7.0, Some(b"rows")).is_err());
        assert_eq!(
            refresh_args(&entry(false), 7.0, Some(br#"{"rows":"one"}"#)).unwrap(),
            [Value::Number(7.0)],
            "an unconfigured widget's refresh takes the date alone"
        );
    }
}
