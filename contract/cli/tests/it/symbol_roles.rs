//! Portable symbol roles and aliases (`kernel/tables/schema.json`).

/// expo/exact#2013: globe and SIM card roles lower by role, Apple name, and
/// Material alias, warning `lower-symbol-alias` on non-role spellings.
#[test]
fn globe_and_sim_card_roles_answer_by_role_apple_name_and_material_alias() {
    for (written, role) in [
        ("globe", "globe"),
        ("language", "globe"),
        ("sf/globe", "globe"),
        ("sim-card", "sim-card"),
        ("simcard", "sim-card"),
        ("sim_card", "sim-card"),
        ("sf/simcard", "sim-card"),
        ("sim-card-fill", "sim-card-fill"),
        ("simcard.fill", "sim-card-fill"),
        ("sf/simcard.fill", "sim-card-fill"),
    ] {
        let (plan, warnings) = contract::compile_warned(&format!(
            "component App\n  view\n    image \"symbol:{written}\"\n"
        ));
        let plan = plan.unwrap_or_else(|e| panic!("{written}: {e}"));
        assert!(
            plan.strings.iter().any(|x| x == &format!("symbol:{role}")),
            "{written}: {:?}",
            plan.strings
        );
        assert!(
            written == role
                || !plan
                    .strings
                    .iter()
                    .any(|x| x == &format!("symbol:{written}")),
            "{written}"
        );
        let said: Vec<_> = warnings
            .iter()
            .map(|w| (w.id.as_str(), w.message.as_str()))
            .collect();
        if written == role {
            assert!(said.is_empty(), "{said:?}");
        } else {
            let message = format!("symbol `{written}` is the role `{role}`: write `symbol:{role}`");
            assert_eq!(said, [("lower-symbol-alias", message.as_str())]);
        }
    }
}
