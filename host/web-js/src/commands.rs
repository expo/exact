//! What a compiled plan links only because it uses it (LLP 1047.002 D1): the
//! host commands its actions name, imported from commands.js by name, the
//! dialog glue its dialogs, alerts, menus and popovers need on the page, and
//! the pieces its links, iframes, lists and buttons need.
use crate::code::Uses;
use crate::style;
use exact_kernel::{NodeType, PropId};
use exact_plan::{BindingKind, NodesId, Plan};
use exact_web::host::template::Parts;
use std::collections::BTreeSet;

/// Whether a link binds its `href`, `rel` or `target`: its `rel` and `target`
/// then follow them (links.js). A static link's the template wrote whole
/// (element.rs `props_of`).
pub(super) fn links(plan: &Plan, parts: &[Option<Parts>]) -> bool {
    parts.iter().zip(plan.nodes.iter()).any(|(p, n)| {
        p.as_ref().is_some_and(|p| p.tag == "a")
            && n.bindings.iter().map(|b| plan.binding(b)).any(|b| {
                b.kind == BindingKind::Prop
                    && [PropId::Href, PropId::Rel, PropId::Target]
                        .iter()
                        .any(|id| b.id == *id as u16)
                    && style::literal(plan, plan.code(b.expr)).is_none()
            })
    })
}

/// Whether the view has an iframe, whose guest origin, sources and app files
/// iframes.js keeps.
pub(super) fn iframes(parts: &[Option<Parts>]) -> bool {
    parts.iter().flatten().any(|p| p.tag == "iframe")
}

/// Whether the view has a List, whose rows list-items.js keeps as list items
/// (navigation.js `listItem`).
pub(super) fn lists(plan: &Plan) -> bool {
    plan.nodes
        .iter()
        .any(|n| n.node_type == NodeType::List as u8)
}

/// Whether a `button` may be other than a platform control (one without
/// `data-button-style`), whose bound fill is read on the page (button-fill.js).
/// The style is a static attribute (element.rs `props_of`) unless it is bound.
pub(super) fn button_fills(plan: &Plan, parts: &[Option<Parts>]) -> bool {
    parts.iter().zip(plan.nodes.iter()).any(|(p, n)| {
        p.as_ref().is_some_and(|p| {
            p.tag == "button"
                && (!p.props.contains_key("data-button-style")
                    || n.bindings
                        .iter()
                        .map(|b| plan.binding(b))
                        .any(|b| b.kind == BindingKind::Prop && b.id == PropId::ButtonStyle as u16))
        })
    })
}

/// The names host/web-js/commands.js exports: the host commands a compiled
/// plan imports from it by name. Read from the file as the compiler is built,
/// so the list and the module cannot drift; a command the file does not export
/// is rt.js's own or a loaded piece's.
fn commands_js_exports() -> &'static BTreeSet<&'static str> {
    static EXPORTS: std::sync::OnceLock<BTreeSet<&'static str>> = std::sync::OnceLock::new();
    EXPORTS.get_or_init(|| {
        include_str!("../commands.js")
            .lines()
            .filter_map(|line| line.strip_prefix("export const "))
            .map(|rest| {
                rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                    .next()
                    .unwrap_or("")
            })
            .filter(|name| !name.is_empty())
            .collect()
    })
}

/// The commands the plan's actions name that commands.js exports, in name
/// order. A command rt.js carries itself, or a loaded piece adds, is not among
/// them and is dispatched as before.
pub(super) fn linked(uses: &Uses) -> Vec<String> {
    (uses.commands.iter())
        .filter(|name| commands_js_exports().contains(name.as_str()))
        .cloned()
        .collect()
}

/// The import of each, as `$c_<name>`.
pub(super) fn import(commands: &[String]) -> String {
    format!(
        "import{{{}}}from\"./commands.js\";",
        commands
            .iter()
            .map(|c| format!("{c} as $c_{c}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// The call that hands them to the runtime (rt.js `useCommands`); `slot` is
/// that function as the module imported it.
pub(super) fn install(slot: &str, commands: &[String]) -> String {
    let table: Vec<String> = commands.iter().map(|c| format!("{c}:$c_{c}")).collect();
    format!("{slot}({{{}}});", table.join(","))
}

/// Whether the node is a dialog, an alertdialog, a menu or a popover, literal
/// or bound (a bound role or popover counts as any): the page then adds
/// prompt-glue.js, alert-message.js and menu-rows.js (main.rs `dialogs.flag`,
/// build.mjs).
pub(super) fn dialog_glue(
    plan: &Plan,
    tag: &str,
    attrs: &[(String, String)],
    node: NodesId,
) -> bool {
    tag == "dialog"
        || attrs.iter().any(|a| {
            a.0 == "popover" || (a.0 == "role" && ["alertdialog", "menu"].contains(&a.1.as_str()))
        })
        || plan
            .node(node)
            .bindings
            .iter()
            .map(|b| plan.binding(b))
            .any(|b| {
                b.kind == BindingKind::Prop
                    && (b.id == PropId::Popover as u16
                        || b.id == PropId::AccessibilityRole as u16
                            && match style::literal(plan, plan.code(b.expr)) {
                                Some(v) => matches!(v.as_str(), Some("alertdialog" | "menu")),
                                None => true,
                            })
            })
}

/// Hands the locale slot `s_<slot>` to chosen.js, for a plan that calls
/// `setLocale`: its choice is written there before the state initializers run.
pub(super) fn locale_slot(plan: &Plan, body: &mut String, slot: impl std::fmt::Display) {
    use std::fmt::Write as _;
    if exact_runner::uses::chooses_locale(plan) {
        let _ = write!(body, "$localeSlot(s_{slot});");
    }
}

/// The format slot `s_<format>`, for a plan whose tables carry format data
/// (LLP 1054.000.003 D6): format-locale.js takes each table's record and
/// makes the slot from the locale slot `s_<locale>`, before any initializer,
/// which may format.
pub(super) fn format_slot(plan: &Plan, body: &mut String, locale: u32) {
    use std::fmt::Write as _;
    let Some(format) = plan.format else {
        return;
    };
    let records: Vec<String> = plan
        .locales
        .iter()
        .map(|row| serde_json::to_string(&[plan.str(row.name), plan.str(row.format)]).unwrap())
        .collect();
    let _ = write!(
        body,
        "$formats([{}]);const s_{}=$formatSlot(s_{locale});",
        records.join(","),
        format.0
    );
}

/// The surfaces the app's GPU modules draw, as the build read them from
/// their compiled registries (`EXACT_JS_GPU_SURFACES`, comma-separated;
/// host/web-js/build.mjs).
pub(super) fn gpu_surfaces() -> Vec<String> {
    std::env::var("EXACT_JS_GPU_SURFACES")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    fn emitted(view: &str) -> String {
        let source = format!(
            "component App\n  state on = false\n  action go\n    on = not on\n  view\n    column\n{view}"
        );
        crate::emit::emit(&contract::compile(&source).unwrap(), false, false)
            .unwrap()
            .js
    }

    /// @ref LLP 1047.002 D1 — list-items.js (a List's rows kept as list
    /// items) is linked only by a plan with a List.
    #[test]
    fn list_items_link_only_where_a_plan_has_a_list() {
        let js = emitted("      list\n        for x in [1, 2] key=x\n          text `${x}`\n");
        assert!(js.contains("import\"./list-items.js\";"), "{js}");
        let js = emitted("      for x in [1, 2] key=x\n        text `${x}`\n");
        assert!(!js.contains("list-items.js"), "{js}");
    }

    /// @ref LLP 1047.002 D1 — button-fill.js (a bound fill on a button that is
    /// not a platform control) is linked only by a plan with such a button, or
    /// one whose style is bound.
    #[test]
    fn button_fill_links_only_where_a_button_may_not_be_a_platform_control() {
        for view in [
            "      button press=go appearance=\"none\"\n        text \"Go\"\n",
            "      button press=go buttonStyle=(on ? \"filled\" : \"bordered\")\n        text \"Go\"\n",
        ] {
            assert!(emitted(view).contains("import\"./button-fill.js\";"), "{view}");
        }
        let js = emitted("      button press=go\n        text \"Go\"\n");
        assert!(!js.contains("button-fill.js"), "{js}");
    }
}
