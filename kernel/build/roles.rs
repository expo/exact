#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoleRow {
    role: String,
    member_roles: Vec<String>,
    member_tags: Vec<String>,
    scope: String,
    axis: String,
    orientation: bool,
    wraps: bool,
    press: bool,
    ends: bool,
    tab_entry: Option<String>,
}

fn validate_roles(schema: &Schema) {
    let mut seen = BTreeSet::new();
    for row in &schema.roles {
        assert!(
            seen.insert(&row.role),
            "schema: duplicate role {}",
            row.role
        );
        assert!(
            !row.role.is_empty() && row.role.bytes().all(|c| c.is_ascii_lowercase()),
            "schema: invalid role {}",
            row.role
        );
        assert!(
            ["children", "descendants"].contains(&row.scope.as_str())
                && ["both", "horizontal", "vertical"].contains(&row.axis.as_str()),
            "schema: role {}: invalid scope or axis",
            row.role
        );
        assert!(
            !row.member_roles.is_empty() || !row.member_tags.is_empty(),
            "schema: role {}: no members",
            row.role
        );
        let mut members = BTreeSet::new();
        for role in &row.member_roles {
            let name = role.strip_suffix('*').unwrap_or(role);
            assert!(
                !name.is_empty()
                    && name.bytes().all(|c| c.is_ascii_lowercase())
                    && members.insert(role),
                "schema: role {}: invalid or duplicate member role {role}",
                row.role
            );
        }
        let mut tags = BTreeSet::new();
        for tag in &row.member_tags {
            assert!(
                ["button", "a[href]"].contains(&tag.as_str()) && tags.insert(tag),
                "schema: role {}: invalid or duplicate member tag {tag}",
                row.role
            );
        }
        assert!(
            row.tab_entry
                .as_deref()
                .is_none_or(|p| matches!(p, "aria-checked" | "aria-selected")),
            "schema: role {}: invalid Tab entry",
            row.role
        );
    }
}

fn write_roles(w: &mut String, rows: &[RoleRow]) {
    w.push_str(concat!(
        "/// The arrows a painted group takes (LLP 1166.000 D1).\n",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
        "pub enum RoleAxis { Both, Horizontal, Vertical }\n",
        "/// A composite role's membership and keys, from the schema.\n",
        "#[derive(Debug)]\n",
        "pub struct RoleKeys {\n",
        "pub role: &'static str, pub member_roles: &'static [&'static str],\n",
        "pub member_tags: &'static [&'static str], pub descendants: bool,\n",
        "pub axis: RoleAxis, pub orientation: bool, pub wraps: bool,\n",
        "pub press: bool, pub ends: bool, pub tab_entry: Option<PropId>,\n",
        "}\n",
        "/// Keys Exact supplies for painted composite roles (LLP 1166.000 D1).\n",
        "pub const ROLE_KEYS: &[RoleKeys] = &[\n",
    ));
    for row in rows {
        let entry = match row.tab_entry.as_deref() {
            Some("aria-checked") => "Some(PropId::AccessibilityChecked)",
            Some("aria-selected") => "Some(PropId::AccessibilitySelected)",
            None => "None",
            _ => unreachable!(),
        };
        writeln!(w,
            "RoleKeys {{ role: {:?}, member_roles: &{:?}, member_tags: &{:?}, descendants: {}, axis: RoleAxis::{}, orientation: {}, wraps: {}, press: {}, ends: {}, tab_entry: {entry} }},",
            row.role, row.member_roles, row.member_tags, row.scope == "descendants",
            pascal(&row.axis), row.orientation, row.wraps, row.press, row.ends
        ).unwrap();
    }
    w.push_str("];\n");
}
