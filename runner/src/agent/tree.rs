//! The runner's tree read, with opt-in plan identity for development layout.
//! @ref LLP 1104 D2; LLP 1035.002 D6 — only the driver joins the source map.

use super::*;

/// Add the session identity after an operation, preserving any clock it already reports.
pub fn tagged<D: DataSource>(runner: &Runner<D>, mut reply: String) -> String {
    if !reply.ends_with('}') {
        return reply;
    }
    let values = [
        ("generation", runner.generation().to_string()),
        ("epoch", runner.kernel().epoch().to_string()),
        ("incarnation", runner.kernel().incarnation().to_string()),
        ("clock", num(runner.now_ms()).to_string()),
    ];
    for (key, value) in values {
        if field_num(&reply, key).is_none() {
            reply.pop();
            if reply.len() > 1 {
                reply.push(',');
            }
            let _ = write!(reply, "\"{key}\":{value}}}");
        }
    }
    reply
}

pub(super) fn request<D: DataSource>(runner: &Runner<D>, request: &str) -> String {
    let shallow = match after_key(request, "shallow") {
        None => false,
        Some(value) => match value.split([',', '}']).next().unwrap().trim() {
            "true" => true,
            "false" => false,
            _ => return error("tree shallow must be a boolean"),
        },
    };
    let Some((root, depth)) = (match target(runner, request) {
        Ok(found) => found,
        Err(e) => return error(&e),
    }) else {
        if shallow {
            return error("shallow tree needs a target");
        }
        return all(runner, field_bool(request, "plan"));
    };
    let kernel = runner.kernel();
    if shallow {
        let mut row = kernel.row(root).expect("located live node");
        row.depth = depth;
        return tree_rows(runner, &[row], &[root], field_bool(request, "plan"));
    }
    let mut subtree = kernel.rows(Some(root)).unwrap_or_default();
    for row in &mut subtree {
        row.depth = row.depth.saturating_add(depth);
    }
    tree_rows(runner, &subtree, &[root], field_bool(request, "plan"))
}

pub(super) fn all<D: DataSource>(runner: &Runner<D>, plan: bool) -> String {
    let rows = runner.kernel().rows(None).unwrap_or_default();
    tree_rows(runner, &rows, &runner.roots(), plan)
}

fn tree_rows<D: DataSource>(
    runner: &Runner<D>,
    rows: &[exact_kernel::export::NodeRow],
    roots: &[u32],
    plan: bool,
) -> String {
    let kernel = runner.kernel();
    let mut s = String::new();
    let _ = write!(
        s,
        "{{\"generation\":{},\"epoch\":{},\"incarnation\":{},\"clock\":{},\"roots\":",
        runner.generation(),
        kernel.epoch(),
        kernel.incarnation(),
        num(runner.now_ms())
    );
    ids(roots, &mut s);
    s.push_str(",\"nodes\":[");
    let handlers = (rows.len() != 1).then(|| runner.handlers());
    let activity = crate::head::activity(kernel, &runner.roots());
    let mut first = true;
    for row in rows {
        let Some(node) = kernel.node(row.id) else {
            continue;
        };
        if !first {
            s.push(',');
        }
        first = false;
        let _ = write!(s, "{{\"id\":{},\"parent\":", node.id);
        match node.parent {
            Some(p) => {
                let _ = write!(s, "{p}");
            }
            None => s.push_str("null"),
        }
        let _ = write!(s, ",\"depth\":{},\"type\":", row.depth);
        quote(node.node_type.name(), &mut s);
        if plan {
            if let Some(site) = runner.plan_site_of(node.id) {
                let _ = write!(s, ",\"site\":{}", site.0);
            }
        }
        s.push_str(",\"props\":{");
        props_json(&node, &mut s);
        s.push_str("},\"handlers\":[");
        let single;
        let events = if let Some(all) = &handlers {
            all.get(&node.id).map_or(&[][..], Vec::as_slice)
        } else {
            single = runner.handlers_of(node.id);
            &single
        };
        for (i, e) in events.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            quote(e.name(), &mut s);
        }
        s.push(']');
        let (inactive, hidden) = activity.get(&node.id).copied().unwrap_or_default();
        if inactive {
            s.push_str(",\"inactive\":true");
            // LLP 1153 D8: `hidden` where no host shows the route (every
            // inactive one but a page a sheet covers), so an agent reading
            // the tree sees which screen shows.
            if hidden {
                s.push_str(",\"hidden\":true");
            }
        }
        if runner.unopened_tabs().contains(&node.id) {
            s.push_str(",\"unopened\":true");
        }
        // A status item is placed in the menu bar or tray, not the window,
        // where the target has one (LLP 1133.025 D4); `tap` reaches it the
        // same. Elsewhere the button is an ordinary button.
        if runner.places_status_items() && crate::desktop::presents(node.props, "status") {
            s.push_str(",\"presented\":\"status\"");
        }
        s.push_str(",\"children\":");
        ids(&node.children(), &mut s);
        s.push('}');
    }
    s.push(']');
    if plan {
        s.push_str(",\"planDigest\":");
        quote(runner.inspection_digest(), &mut s);
    }
    s.push('}');
    s
}

/// Props and SVG geometry by their schema names, with password values masked.
pub(super) fn props_json(node: &NodeRef<'_>, s: &mut String) {
    for (i, (id, value)) in node.props.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        quote(id.name(), s);
        s.push(':');
        match value {
            PropValue::Str(t) if id == exact_kernel::PropId::Value => {
                quote(shown_value(node.props, t), s)
            }
            PropValue::Str(t) => quote(t, s),
            PropValue::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
            PropValue::Int(i) => {
                let _ = write!(s, "{i}");
            }
            PropValue::Float(f) => {
                let _ = write!(s, "{}", num(*f));
            }
        }
    }

    // SVG 2 geometry lives in style rows, unlike the other SVG attributes.
    // Expose it beside those attributes on every host using the runner's tree.
    use exact_kernel::NodeType;
    let geometry: &[StyleId] = match node.node_type {
        NodeType::SvgCircle => &[StyleId::Cx, StyleId::Cy, StyleId::R],
        NodeType::SvgEllipse => &[StyleId::Cx, StyleId::Cy, StyleId::Rx, StyleId::Ry],
        NodeType::SvgRect => &[
            StyleId::X,
            StyleId::Y,
            StyleId::Width,
            StyleId::Height,
            StyleId::Rx,
            StyleId::Ry,
        ],
        NodeType::SvgUse | NodeType::SvgViewport | NodeType::SvgForeignObject => {
            &[StyleId::X, StyleId::Y, StyleId::Width, StyleId::Height]
        }
        _ => &[],
    };
    let mut comma = node.props.iter().next().is_some();
    for &row in geometry.iter().filter(|row| node.style.mask.has(**row)) {
        if comma {
            s.push(',');
        }
        comma = true;
        quote(row.name(), s);
        s.push(':');
        row_json(row, node.computed(row), s);
    }
}
