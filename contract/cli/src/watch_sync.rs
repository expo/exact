//! Bake-time authority for phone-owned watch replicas.
use crate::manifest::WatchSync;
use exact_plan::{Opcode, Operand, Plan, TypesId};

fn shape(plan: &Plan, ty: TypesId) -> String {
    let row = plan.type_(ty);
    format!(
        "{:?}:{:?}:{}",
        row.kind,
        row.fields
            .iter()
            .map(|f| (plan.str(plan.field(f).name), shape(plan, plan.field(f).ty)))
            .collect::<Vec<_>>(),
        row.elem.map(|e| shape(plan, e)).unwrap_or_default()
    )
}

/// What an installed binary fixes about replication: the allowlist in its
/// Info.plist, and the type each selected state has in its embedded watch plan,
/// which refuses a snapshot whose value is of another type. Either changing
/// needs a new binary.
pub fn identity(sync: &WatchSync, phone: Option<&Plan>) -> Result<serde_json::Value, String> {
    let mut identity = sync.json();
    let mut types = serde_json::Map::new();
    for name in &sync.state {
        let slot = phone
            .and_then(|p| {
                p.slots
                    .iter()
                    .find(|s| s.persist == exact_plan::PersistScope::App && p.str(s.name) == name)
            })
            .ok_or_else(|| format!("watch.sync.state `{name}` must be a root persist state"))?;
        types.insert(name.clone(), shape(phone.unwrap(), slot.ty).into());
    }
    // An app with no selected state keeps the identity it had.
    if !types.is_empty() {
        identity["types"] = types.into();
    }
    Ok(identity)
}

/// Both plans must agree on selected root slots; no watch action may assign one.
pub fn plans(sync: &WatchSync, phone: &Plan, watch: &Plan) -> Result<(), String> {
    let mut slots = Vec::new();
    for name in &sync.state {
        let find = |p: &Plan| {
            p.slots
                .iter()
                .position(|s| s.persist == exact_plan::PersistScope::App && p.str(s.name) == name)
        };
        let (Some(p), Some(w)) = (find(phone), find(watch)) else {
            return Err(format!(
                "watch.sync.state `{name}` must be a root persist state in both entries"
            ));
        };
        if shape(phone, phone.slots[p].ty) != shape(watch, watch.slots[w].ty) {
            return Err(format!(
                "watch.sync.state `{name}` must have the same type in both entries"
            ));
        }
        slots.push(w);
    }
    for action in watch.actions.iter() {
        let code = watch.code(action.body);
        let mut r = exact_plan::bytes::Reader::new(watch.read_code(code).0);
        while !r.is_empty() {
            let op = Opcode::from_wire(r.u8().map_err(|_| "invalid watch action")?)
                .ok_or("invalid watch action")?;
            for operand in op.operands() {
                match operand {
                    Operand::U8 | Operand::Enum(_) => {
                        r.u8().map_err(|_| "invalid operand")?;
                    }
                    Operand::U16 => {
                        r.u16().map_err(|_| "invalid operand")?;
                    }
                    Operand::F64 => {
                        r.f64().map_err(|_| "invalid operand")?;
                    }
                    _ => {
                        let v = r.u32().map_err(|_| "invalid operand")? as usize;
                        if op == Opcode::StoreSlot && slots.contains(&v) {
                            return Err(format!(
                                "watch.sync.state `{}` is phone-owned; remove the watch assignment",
                                watch.str(watch.slots[v].name)
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Presence policies, including optional ones, are never export authority.
pub fn grants(sync: &WatchSync, grants: &str) -> Result<(), String> {
    let set = exact_runner::grants::parse(grants).map_err(|e| e.join("; "))?;
    for name in &sync.secrets {
        if !set.secrets().any(|n| n == name) || set.presence().any(|(n, _)| n == name) {
            return Err(format!("watch.sync.secrets `{name}` requires an ordinary secret.keep grant without a presence policy"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(state: &str, action: &str) -> Plan {
        crate::compile(&format!(
            "component A\n  state liked = {state} persist\n{action}  view\n    text \"hello\"\n"
        ))
        .unwrap()
    }
    #[test]
    fn only_matching_phone_owned_persist_slots() {
        let sync = WatchSync {
            state: vec!["liked".into()],
            secrets: vec![],
        };
        let phone = plan("false", "");
        assert!(plans(&sync, &phone, &phone).is_ok());
        assert!(plans(&sync, &phone, &plan("0", "")).is_err());
        assert!(plans(
            &sync,
            &phone,
            &plan("false", "  action toggle\n    liked = !liked\n")
        )
        .is_err());
        let missing = crate::compile("component A\n  view\n    text \"hi\"\n").unwrap();
        assert!(plans(&sync, &phone, &missing).is_err());
    }
    #[test]
    fn ordinary_grants_only() {
        let sync = WatchSync {
            state: vec![],
            secrets: vec!["seed".into()],
        };
        assert!(grants(&sync, "secret.keep seed").is_ok());
        for line in [
            "",
            "secret.keep other",
            "secret.keep seed user-presence",
            "secret.keep seed user-presence optional",
        ] {
            assert!(grants(&sync, line).is_err(), "{line}");
        }
    }
}
