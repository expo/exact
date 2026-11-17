//! A snapshot entry's slots from the host: typed props replace only the
//! entry's root slot (LLP 1126 D2), and a widget's chosen configuration its
//! configuration slot. Every host that boots an entry fills them so.
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
        let json: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("entry {what}: {e}"))?;
        let value = if what == "configuration" {
            let shape = plan.type_(row.ty);
            if shape.kind != TypeKind::Record
                || shape.fields.iter().any(|id| {
                    let field = plan.type_(plan.field(id).ty);
                    field.kind != TypeKind::String || field.fields.len == 0
                })
            {
                return Err("entry configuration has no fixed choices".into());
            }
            // LLP 1126: ignore removed parameters and default missing or removed
            // values. The compiler puts each parameter's default first in its enum.
            Value::record(
                plan.type_(row.ty)
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
            )
        } else {
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
